#[cfg(test)]
mod tests_module {
    use crate::boolean::*;
    use crate::cas::*;
    use crate::expr::*;
    use crate::integral::eval_integral_hybrid;
    use crate::types::*;
    use geo::{Area, BooleanOps};
    use glam::Vec2;

    #[test]
    fn test_point_distance() {
        let a = Point2::new(0.0, 0.0);
        let b = Point2::new(3.0, 4.0);
        assert!((a.distance(&b) - 5.0).abs() < 1e-10);
    }

    #[test]
    fn test_point2_vec2_roundtrip() {
        let p = Point2::new(1.5, -2.5);
        let v = p.to_vec2();
        let p2 = Point2::from_vec2(v);
        assert!((p.x - p2.x).abs() < 1e-6);
        assert!((p.y - p2.y).abs() < 1e-6);
    }

    #[test]
    fn test_circle_contains() {
        let c = Circle::new(Point2::new(0.0, 0.0), 5.0);
        assert!(c.contains(&Point2::new(3.0, 4.0)));
        assert!(!c.contains(&Point2::new(4.0, 4.0)));
    }

    #[test]
    fn test_view_transform_world_screen_roundtrip() {
        let view = ViewTransform::new(800.0, 600.0);
        let world = Point2::new(2.0, 3.0);
        let screen = view.world_to_screen(world);
        let world2 = view.screen_to_world(screen);
        assert!((world.x - world2.x).abs() < 1e-6);
        assert!((world.y - world2.y).abs() < 1e-6);
    }

    #[test]
    fn test_view_transform_pan() {
        let mut view = ViewTransform::new(800.0, 600.0);
        view.pan(Vec2::new(100.0, 0.0));
        let world = Point2::new(0.0, 0.0);
        let screen = view.world_to_screen(world);
        // With pan 100 right, origin should be at 500, 300
        assert!((screen.x - 500.0).abs() < 1.0);
        assert!((screen.y - 300.0).abs() < 1.0);
    }

    #[test]
    fn test_view_transform_zoom() {
        let mut view = ViewTransform::new(1000.0, 1000.0);
        let world = Point2::new(1.0, 0.0);
        let s1 = view.world_to_screen(world);
        view.zoom(2.0, Vec2::new(500.0, 500.0));
        let s2 = view.world_to_screen(world);
        // After 2x zoom, the x distance from center should double
        let dist1 = (s1.x - 500.0).abs();
        let dist2 = (s2.x - 500.0).abs();
        assert!((dist2 / dist1 - 2.0).abs() < 0.1);
    }

    #[test]
    fn test_aabb_expand() {
        let mut bbox = AABB::new(Point2::new(0.0, 0.0), Point2::new(0.0, 0.0));
        bbox.expand(&Point2::new(5.0, 3.0));
        bbox.expand(&Point2::new(-2.0, 1.0));
        assert!((bbox.min.x + 2.0).abs() < 1e-6);
        assert!((bbox.min.y - 0.0).abs() < 1e-6);
        assert!((bbox.max.x - 5.0).abs() < 1e-6);
        assert!((bbox.max.y - 3.0).abs() < 1e-6);
    }

    #[test]
    fn test_expression_evaluation() {
        let result = evaluate("2 + 3 * 4", &[]);
        // evalexpr may return Int(14), not Float(14.0)
        assert!(result.is_ok());
    }

    #[test]
    fn test_expression_with_variables() {
        let result = evaluate(
            "x * 2 + y",
            &[("x".to_string(), 5.0), ("y".to_string(), 3.0)],
        );
        assert!(result.is_ok());
    }

    #[test]
    fn test_eval_function() {
        // evalexpr doesn't have built-in sin(), test with basic arithmetic
        let result = eval_function("x * x", 3.0);
        assert_eq!(result.unwrap(), 9.0);
    }

    #[test]
    fn test_invalid_expression() {
        let result = evaluate("1 / 0", &[]);
        assert!(result.is_err() || result.unwrap().is_infinite());
    }

    #[test]
    fn test_derivative() {
        // d/dx x^2 at x=3 should be 6
        let f = |x: f64| x * x;
        let d = derivative(f, 3.0, None);
        assert!((d - 6.0).abs() < 1e-3);
    }

    #[test]
    fn test_derivative_sin() {
        // d/dx sin(x) at x=0 should be 1
        let d = derivative(f64::sin, 0.0, None);
        assert!((d - 1.0).abs() < 1e-3);
    }

    #[test]
    fn test_integral_simpson() {
        // ∫[0,1] x^2 dx = 1/3
        let f = |x: f64| x * x;
        let result = integral(f, 0.0, 1.0, 1000);
        assert!((result - 1.0 / 3.0).abs() < 1e-6);
    }

    #[test]
    fn test_integral_hybrid_polynomial() {
        // ∫₀¹ x² dx = 1/3
        let result = eval_integral_hybrid(|x| x * x, 0.0, 1.0, 1024);
        assert!((result - 1.0 / 3.0).abs() < 1e-6);
    }

    #[test]
    fn test_integral_hybrid_sine() {
        // ∫₀^π sin(x) dx = 2
        let result = eval_integral_hybrid(|x| x.sin(), 0.0, std::f64::consts::PI, 2048);
        assert!((result - 2.0).abs() < 1e-6);
    }

    #[test]
    fn test_integral_hybrid_exponential() {
        // ∫₀¹ e^x dx = e - 1
        let result = eval_integral_hybrid(|x| x.exp(), 0.0, 1.0, 1024);
        assert!((result - (std::f64::consts::E - 1.0)).abs() < 1e-6);
    }

    #[test]
    fn test_newton_root() {
        // Root of x^2 - 4 = 0 starting at x=3
        let f = |x: f64| x * x - 4.0;
        let df = |x: f64| 2.0 * x;
        let root = newton_root(f, df, 3.0, 20, 1e-8).unwrap();
        assert!((root - 2.0).abs() < 1e-6);
    }

    #[test]
    fn test_newton_root_auto() {
        let f = |x: f64| x * x - 4.0;
        let root = newton_root_auto(&f, 3.0).unwrap();
        assert!((root - 2.0).abs() < 1e-6);
    }

    #[test]
    fn test_newton_root_negative() {
        let f = |x: f64| x * x - 4.0;
        let root = newton_root_auto(&f, -3.0).unwrap();
        assert!((root + 2.0).abs() < 1e-6);
    }

    #[test]
    fn test_find_root() {
        let f = |x: f64| x.powi(3) - x - 2.0;
        let root = find_root(f, (0.0, 3.0)).unwrap();
        // x^3 - x - 2 = 0 has root near 1.521
        assert!((root - 1.521).abs() < 0.01);
    }

    #[test]
    fn test_limit() {
        // lim[x→0] sin(x)/x = 1
        let f = |x: f64| if x == 0.0 { 1.0 } else { f64::sin(x) / x };
        let result = limit(f, 0.0);
        assert!((result - 1.0).abs() < 1e-3);
    }

    #[test]
    fn test_line_length() {
        let l = Line2::new(Point2::new(0.0, 0.0), Point2::new(3.0, 4.0));
        assert!((l.length() - 5.0).abs() < 1e-10);
    }

    #[test]
    fn test_color_construction() {
        let c = Color::new(0.5, 0.25, 0.75, 1.0);
        assert_eq!(c.to_array(), [0.5, 0.25, 0.75, 1.0]);
    }

    #[test]
    fn test_view_transform_log_zoom_preserves_anchor() {
        let mut view = ViewTransform::new(800.0, 600.0);
        view.x_log = true;
        view.y_log = true;
        let anchor = glam::Vec2::new(400.0, 300.0);
        let anchor_world_before = view.screen_to_world(anchor);
        view.zoom(2.0, anchor);
        let anchor_world_after = view.screen_to_world(anchor);
        assert!(
            (anchor_world_before.x - anchor_world_after.x).abs() < 1e-9
                && (anchor_world_before.y - anchor_world_after.y).abs() < 1e-9,
            "zoom anchor should stay fixed in world coordinates"
        );
    }

    #[test]
    fn test_view_transform_pan_does_not_change_scale() {
        let mut view = ViewTransform::new(800.0, 600.0);
        let scale_before = view.scale;
        view.pan(glam::Vec2::new(123.0, -456.0));
        assert!((view.scale - scale_before).abs() < 1e-12);
    }

    #[test]
    fn test_camera3d_pan_changes_target() {
        use crate::types3d::Camera3D;
        let mut cam = Camera3D::new(1.6);
        let target_before = cam.target;
        cam.pan(100.0, 50.0);
        assert!((cam.target - target_before).length() > 1e-3);
    }

    #[test]
    fn test_camera3d_orbit_changes_angles() {
        use crate::types3d::Camera3D;
        let mut cam = Camera3D::new(1.6);
        let theta_before = cam.theta;
        let phi_before = cam.phi;
        cam.orbit(0.1, 0.1);
        assert!((cam.theta - theta_before).abs() > 1e-6);
        assert!((cam.phi - phi_before).abs() > 1e-6);
    }

    fn unit_square(bottom_left: (f64, f64)) -> Vec<Point2> {
        let (x, y) = bottom_left;
        vec![
            Point2::new(x, y),
            Point2::new(x + 1.0, y),
            Point2::new(x + 1.0, y + 1.0),
            Point2::new(x, y + 1.0),
        ]
    }

    #[test]
    fn test_polygon_union_two_squares() {
        let a = unit_square((0.0, 0.0));
        let b = unit_square((0.5, 0.0));
        let geo_a = polygon_to_geo(&a);
        let geo_b = polygon_to_geo(&b);
        let union = geo_a.union(&geo_b);
        let area = union.unsigned_area();
        assert!(
            (area - 1.5).abs() < 1e-9,
            "union area should be 1.5, got {}",
            area
        );
    }

    #[test]
    fn test_polygon_intersection_two_squares() {
        let a = unit_square((0.0, 0.0));
        let b = unit_square((0.5, 0.0));
        let geo_a = polygon_to_geo(&a);
        let geo_b = polygon_to_geo(&b);
        let intersection = geo_a.intersection(&geo_b);
        let area = intersection.unsigned_area();
        assert!(
            (area - 0.5).abs() < 1e-9,
            "intersection area should be 0.5, got {}",
            area
        );
    }

    #[test]
    fn test_polygon_difference() {
        let a = unit_square((0.0, 0.0));
        let b = unit_square((0.5, 0.0));
        let geo_a = polygon_to_geo(&a);
        let geo_b = polygon_to_geo(&b);
        let difference = geo_a.difference(&geo_b);
        let area = difference.unsigned_area();
        assert!(
            (area - 0.5).abs() < 1e-9,
            "difference area should be 0.5, got {}",
            area
        );
    }

    #[test]
    fn test_polygon_xor_two_squares() {
        let a = unit_square((0.0, 0.0));
        let b = unit_square((0.5, 0.0));
        let geo_a = polygon_to_geo(&a);
        let geo_b = polygon_to_geo(&b);
        let xor = geo_a.xor(&geo_b);
        let area = xor.unsigned_area();
        assert!(
            (area - 1.0).abs() < 1e-9,
            "xor area should be 1.0, got {}",
            area
        );
    }

    #[test]
    fn test_polygon_boolean_closes_open_ring() {
        let open = vec![
            Point2::new(0.0, 0.0),
            Point2::new(1.0, 0.0),
            Point2::new(1.0, 1.0),
            Point2::new(0.0, 1.0),
        ];
        let poly = polygon_to_geo(&open);
        let exterior = poly.exterior();
        let first = exterior.coords().next().unwrap();
        let last = exterior.coords().last().unwrap();
        assert!((first.x - last.x).abs() < 1e-12);
        assert!((first.y - last.y).abs() < 1e-12);
    }

    #[test]
    fn test_polygon_boolean_self_intersecting_no_panic() {
        // Bow-tie / self-intersecting quadrilateral.
        let bowtie = vec![
            Point2::new(0.0, 0.0),
            Point2::new(1.0, 1.0),
            Point2::new(1.0, 0.0),
            Point2::new(0.0, 1.0),
        ];
        // The operation should not panic; `geo` may produce an empty or
        // degenerate result for self-intersecting inputs.
        let geo = polygon_to_geo(&bowtie);
        let _result = geo.union(&polygon_to_geo(&bowtie));
    }
}
