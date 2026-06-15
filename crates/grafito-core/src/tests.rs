#[cfg(test)]
#[allow(clippy::module_inception)]
mod tests {
    use crate::*;
    use grafito_geometry::*;
    use std::collections::HashMap;

    #[test]
    fn test_object_id_creation() {
        let id1 = ObjectId::new();
        let id2 = ObjectId::new();
        assert_ne!(id1, id2);
    }

    #[test]
    fn test_document_add_point() {
        let mut doc = Document::new();
        let obj = GeoObject::Point(PointObj::new(Point2::new(1.0, 2.0)));
        let id = doc.add_object(obj);
        assert_eq!(doc.object_count(), 1);
        assert!(doc.get_object(id).is_some());
    }

    #[test]
    fn test_document_auto_label() {
        let mut doc = Document::new();
        let obj = GeoObject::Point(PointObj::new(Point2::new(0.0, 0.0)));
        let id = doc.add_object(obj);
        let stored = doc.get_object(id).unwrap();
        assert_eq!(stored.label(), "P"); // First point gets label "P" (not "P1")
    }

    #[test]
    fn test_document_auto_label_multiple() {
        let mut doc = Document::new();
        doc.add_object(GeoObject::Point(PointObj::new(Point2::new(0.0, 0.0))));
        doc.add_object(GeoObject::Point(PointObj::new(Point2::new(1.0, 1.0))));
        doc.add_object(GeoObject::Line(LineObj::new(
            Point2::new(0.0, 0.0),
            Point2::new(1.0, 1.0),
        )));
        assert_eq!(doc.object_count(), 3);
    }

    #[test]
    fn test_document_remove_object() {
        let mut doc = Document::new();
        let obj = GeoObject::Point(PointObj::new(Point2::new(0.0, 0.0)));
        let id = doc.add_object(obj);
        assert!(doc.remove_object(id).is_some());
        assert_eq!(doc.object_count(), 0);
    }

    #[test]
    fn test_document_selection() {
        let mut doc = Document::new();
        let id = doc.add_object(GeoObject::Point(PointObj::new(Point2::new(0.0, 0.0))));
        doc.select(id);
        assert!(doc.is_selected(id));
        doc.deselect(id);
        assert!(!doc.is_selected(id));
    }

    #[test]
    fn test_document_clear() {
        let mut doc = Document::new();
        doc.add_object(GeoObject::Point(PointObj::new(Point2::new(0.0, 0.0))));
        doc.add_object(GeoObject::Point(PointObj::new(Point2::new(1.0, 1.0))));
        doc.clear();
        assert_eq!(doc.object_count(), 0);
    }

    #[test]
    fn test_document_variables() {
        let mut doc = Document::new();
        doc.set_variable("a".into(), 42.0);
        assert_eq!(doc.get_variable("a"), Some(42.0));
        assert_eq!(doc.get_variable("b"), None);
    }

    #[test]
    fn test_document_visible_filtering() {
        let mut doc = Document::new();
        doc.add_object(GeoObject::Point(PointObj::new(Point2::new(0.0, 0.0))));
        let all_visible = doc.objects_iter().all(|(_, obj)| obj.is_visible());
        assert!(all_visible);
    }

    #[test]
    fn test_pick_object_point() {
        let mut doc = Document::new();
        doc.set_view(ViewTransform::new(800.0, 600.0));
        doc.add_object(GeoObject::Point(PointObj::new(Point2::new(2.0, 3.0))));
        // Pick near the point
        let result = doc.pick_object(Point2::new(2.0, 3.0), 1.0);
        assert!(result.is_some());
        // Pick far away
        let result = doc.pick_object(Point2::new(100.0, 100.0), 1.0);
        assert!(result.is_none());
    }

    #[test]
    fn test_spatial_index_insert_and_query() {
        let mut idx = SpatialIndex::new();
        let id1 = ObjectId::new();
        let id2 = ObjectId::new();
        let id3 = ObjectId::new();

        // Insert three objects with bounding boxes
        idx.insert(id1, 0.0, 0.0, 2.0, 2.0); // Box around (1,1)
        idx.insert(id2, 2.0, 2.0, 4.0, 4.0); // Box around (3,3)
        idx.insert(id3, 9.0, 9.0, 11.0, 11.0); // Box around (10,10)

        assert_eq!(idx.len(), 3);

        // Query near (1,1) should find id1
        let candidates = idx.candidates(1.0, 1.0, 0.5);
        assert!(candidates.contains(&id1));
        assert!(!candidates.contains(&id3));

        // Query near (10,10) should find id3
        let candidates = idx.candidates(10.0, 10.0, 0.5);
        assert!(candidates.contains(&id3));
        assert!(!candidates.contains(&id1));
    }

    #[test]
    fn test_constraint_graph_free_object() {
        let mut cg = ConstraintGraph::new();
        let id = ObjectId::new();
        cg.add_free_object(id);
        assert!(cg.is_free(&id));
        assert_eq!(cg.free_count(), 1);
    }

    #[test]
    fn test_constraint_graph_add_constraint() {
        let mut cg = ConstraintGraph::new();
        let a = ObjectId::new();
        let b = ObjectId::new();
        let m = ObjectId::new();
        cg.add_free_object(a);
        cg.add_free_object(b);
        let cons_id = cg.add_constraint("Midpoint", vec![a, b], vec![m], HashMap::new());
        assert_eq!(cg.constraint_count(), 1);
        assert!(!cg.is_free(&m));
        assert!(cg.is_free(&a));
        assert_eq!(cg.creator_of(&m).unwrap().id, cons_id);
    }

    #[test]
    fn test_constraint_graph_update_order() {
        let mut cg = ConstraintGraph::new();
        let a = ObjectId::new();
        let b = ObjectId::new();
        let m = ObjectId::new();
        let m2 = ObjectId::new();
        cg.add_free_object(a);
        cg.add_free_object(b);
        // M = Midpoint[A, B]
        cg.add_constraint("Midpoint", vec![a, b], vec![m], HashMap::new());
        // M2 = Midpoint[M, B]
        cg.add_constraint("Midpoint", vec![m, b], vec![m2], HashMap::new());

        // When A changes, both constraints should be in update order
        let order = cg.get_update_order(&[a]);
        assert_eq!(order.len(), 2); // Both constraints need re-evaluation
    }

    #[test]
    fn test_constraint_graph_cycle_detection() {
        let mut cg = ConstraintGraph::new();
        let a = ObjectId::new();
        let b = ObjectId::new();
        let c = ObjectId::new();
        cg.add_free_object(a);

        // C1: A -> B
        cg.add_constraint("C1", vec![a], vec![b], HashMap::new());
        // C2: B -> C
        cg.add_constraint("C2", vec![b], vec![c], HashMap::new());
        // C3: C -> B (creates a cycle B -> C -> B)
        cg.add_constraint("C3", vec![c], vec![b], HashMap::new());

        let order = cg.get_update_order(&[a]);
        assert_eq!(
            order.len(),
            3,
            "all reachable constraints should be returned"
        );
        assert!(order.contains(&0));
        assert!(order.contains(&1));
        assert!(order.contains(&2));
        // No duplicates
        let unique: std::collections::HashSet<_> = order.iter().collect();
        assert_eq!(unique.len(), order.len());
    }

    #[test]
    fn test_document_add_constructed_object() {
        let mut doc = Document::new();
        let a = doc.add_object(GeoObject::Point(PointObj::new(Point2::new(0.0, 0.0))));
        let b = doc.add_object(GeoObject::Point(PointObj::new(Point2::new(4.0, 0.0))));
        let (m, _) = doc.add_constructed_object(
            GeoObject::Point(PointObj::new(Point2::new(2.0, 0.0)).with_label("M")),
            "Midpoint",
            &[a, b],
        );
        assert!(!doc.is_free_object(&m));
        assert!(doc.is_free_object(&a));
        assert_eq!(doc.constraints.constraint_count(), 1);
    }

    #[test]
    fn test_document_move_point_propagation() {
        let mut doc = Document::new();
        let a = doc.add_object(GeoObject::Point(PointObj::new(Point2::new(0.0, 0.0))));
        let b = doc.add_object(GeoObject::Point(PointObj::new(Point2::new(4.0, 0.0))));
        doc.add_constructed_object(
            GeoObject::Point(PointObj::new(Point2::new(2.0, 0.0)).with_label("M")),
            "Midpoint",
            &[a, b],
        );
        // Move A to (2, 2) - should mark M as affected
        let affected = doc.move_point(a, Point2::new(2.0, 2.0));
        assert!(affected.contains(&a)); // A itself is affected
                                        // The midpoint object ID should also be in the affected list
        assert!(affected.len() >= 2);
    }

    #[test]
    fn test_document_propagation_order() {
        let mut doc = Document::new();
        let a = doc.add_object(GeoObject::Point(PointObj::new(Point2::new(0.0, 0.0))));
        let b = doc.add_object(GeoObject::Point(PointObj::new(Point2::new(4.0, 0.0))));
        doc.add_constructed_object(
            GeoObject::Point(PointObj::new(Point2::new(2.0, 0.0)).with_label("M")),
            "Midpoint",
            &[a, b],
        );
        let order = doc.propagation_order(&[a]);
        assert_eq!(order.len(), 1); // One constraint to re-evaluate
    }

    #[test]
    fn test_document_remove_cleans_constraints() {
        let mut doc = Document::new();
        let a = doc.add_object(GeoObject::Point(PointObj::new(Point2::new(0.0, 0.0))));
        let b = doc.add_object(GeoObject::Point(PointObj::new(Point2::new(4.0, 0.0))));
        let (m, _) = doc.add_constructed_object(
            GeoObject::Point(PointObj::new(Point2::new(2.0, 0.0)).with_label("M")),
            "Midpoint",
            &[a, b],
        );
        assert_eq!(doc.constraints.constraint_count(), 1);
        doc.remove_object(m);
        assert_eq!(doc.constraints.constraint_count(), 0);
    }

    #[test]
    fn test_document_clear_resets_constraints() {
        let mut doc = Document::new();
        let a = doc.add_object(GeoObject::Point(PointObj::new(Point2::new(0.0, 0.0))));
        let b = doc.add_object(GeoObject::Point(PointObj::new(Point2::new(4.0, 0.0))));
        doc.add_constructed_object(
            GeoObject::Point(PointObj::new(Point2::new(2.0, 0.0)).with_label("M")),
            "Midpoint",
            &[a, b],
        );
        assert_eq!(doc.constraints.free_count(), 2);
        doc.clear();
        assert_eq!(doc.constraints.constraint_count(), 0);
        assert_eq!(doc.constraints.free_count(), 0);
    }

    #[test]
    fn test_implicit_curve_evaluation_and_caching() {
        let ic = ImplicitCurveObj::new("x^3 + y^3", "3*x*y", RelationOperator::Eq);
        let vars = std::collections::HashMap::new();
        let view_bounds = (-3.0, 3.0, -3.0, 3.0);
        let grid_size = 40;

        // First call computes and caches.
        let segs1 = {
            let guard = implicit_curve::segments_or_compute(
                &ic,
                view_bounds,
                grid_size,
                &vars,
                RenderQuality::Normal,
            );
            guard.clone()
        };
        assert!(
            !segs1.is_empty(),
            "folium of descartes should produce segments"
        );

        // The cached region is a padded/snapped superset of the requested bounds.
        let region = ic
            .cached_region
            .read()
            .unwrap()
            .expect("cached region should be set");
        assert!(
            view_bounds.0 >= region.0
                && view_bounds.1 <= region.1
                && view_bounds.2 >= region.2
                && view_bounds.3 <= region.3,
            "cached region should contain the requested view bounds"
        );

        // Second call with identical parameters returns the cached value.
        let segs2 = {
            let guard = implicit_curve::segments_or_compute(
                &ic,
                view_bounds,
                grid_size,
                &vars,
                RenderQuality::Normal,
            );
            guard.clone()
        };
        assert_eq!(segs1.len(), segs2.len());

        // A different view invalidates and recomputes.
        let segs3 = {
            let guard = implicit_curve::segments_or_compute(
                &ic,
                (-1.0, 1.0, -1.0, 1.0),
                grid_size,
                &vars,
                RenderQuality::Normal,
            );
            guard.clone()
        };
        // May be empty if zoomed into a region without the curve, but cache key changed.
        assert!(ic
            .cached_key
            .read()
            .unwrap()
            .as_ref()
            .map(|k| k.view_bounds != view_bounds)
            .unwrap_or(false));
        let _ = segs3;
    }

    #[test]
    fn test_implicit_curve_cache_reuses_for_small_pan() {
        let ic = ImplicitCurveObj::new("x^3 + y^3", "3*x*y", RelationOperator::Eq);
        let vars = std::collections::HashMap::new();
        let view_bounds = (-3.0, 3.0, -3.0, 3.0);
        let grid_size = 40;

        let segs1 = {
            let guard = implicit_curve::segments_or_compute(
                &ic,
                view_bounds,
                grid_size,
                &vars,
                RenderQuality::Normal,
            );
            guard.clone()
        };

        // A small pan stays inside the padded/snapped cached region, so the
        // previously computed segments are reused.
        let panned = (
            view_bounds.0 + 0.1,
            view_bounds.1 + 0.1,
            view_bounds.2,
            view_bounds.3,
        );
        let segs2 = {
            let guard = implicit_curve::segments_or_compute(
                &ic,
                panned,
                grid_size,
                &vars,
                RenderQuality::Normal,
            );
            guard.clone()
        };
        assert_eq!(segs1.len(), segs2.len(), "small pan should reuse cache");
    }

    #[test]
    fn test_implicit_curve_cache_recomputes_for_far_pan() {
        let ic = ImplicitCurveObj::new("x^3 + y^3", "3*x*y", RelationOperator::Eq);
        let vars = std::collections::HashMap::new();
        let view_bounds = (-3.0, 3.0, -3.0, 3.0);
        let grid_size = 40;

        drop(implicit_curve::segments_or_compute(
            &ic,
            view_bounds,
            grid_size,
            &vars,
            RenderQuality::Normal,
        ));
        let first_region = *ic.cached_region.read().unwrap().as_ref().unwrap();

        // A far-away view is outside the padded cached region, so it recomputes
        // and updates the cached region.
        let far = (10.0, 16.0, 10.0, 16.0);
        drop(implicit_curve::segments_or_compute(
            &ic,
            far,
            grid_size,
            &vars,
            RenderQuality::Normal,
        ));
        let second_region = *ic.cached_region.read().unwrap().as_ref().unwrap();

        assert_ne!(
            first_region, second_region,
            "far pan should update cached region"
        );
        assert!(
            far.0 >= second_region.0
                && far.1 <= second_region.1
                && far.2 >= second_region.2
                && far.3 <= second_region.3,
            "new cached region should contain the far view bounds"
        );
    }

    #[test]
    fn test_constraint_params_serialize_roundtrip() {
        let mut doc = Document::new();
        let a = doc.add_object(GeoObject::Point(PointObj::new(Point2::new(0.0, 0.0))));
        let mut params = HashMap::new();
        params.insert("dx".to_string(), 3.0);
        params.insert("dy".to_string(), 4.0);
        let (_p, cons_id) = doc.add_constructed_object_with_params(
            GeoObject::Point(PointObj::new(Point2::new(3.0, 4.0)).with_label("P'")),
            "Translate",
            &[a],
            params,
        );

        let json = serde_json::to_string(&doc).expect("serialize document");
        let loaded: Document = serde_json::from_str(&json).expect("deserialize document");

        let cons = loaded
            .constraints
            .get_constraint(cons_id)
            .expect("constraint should survive roundtrip");
        assert_eq!(cons.params.get("dx"), Some(&3.0));
        assert_eq!(cons.params.get("dy"), Some(&4.0));
    }

    #[test]
    fn test_perpendicular_constraint() {
        let mut doc = Document::new();
        let a = doc.add_object(GeoObject::Point(PointObj::new(Point2::new(0.0, 0.0))));
        let b = doc.add_object(GeoObject::Point(PointObj::new(Point2::new(4.0, 0.0))));
        let line = doc.add_object(GeoObject::Line(LineObj::new(
            Point2::new(0.0, 0.0),
            Point2::new(4.0, 0.0),
        )));
        let p = doc.add_object(GeoObject::Point(PointObj::new(Point2::new(2.0, 3.0))));
        let (perp, _) = doc.add_constructed_object(
            GeoObject::Line(
                LineObj::new_with_kind(
                    Point2::new(0.0, 0.0),
                    Point2::new(1.0, 1.0),
                    LineKind::Line,
                )
                .with_label("P"),
            ),
            "Perpendicular",
            &[line, p],
        );
        let order = doc.propagation_order(&[a, b, p]);
        doc.re_evaluate_constraints(&order);
        let perp_obj = doc.get_object(perp).unwrap();
        if let GeoObject::Line(l) = perp_obj {
            let dx = l.end.x - l.start.x;
            let dy = l.end.y - l.start.y;
            // Direction should be vertical (perpendicular to horizontal reference)
            assert!((dx).abs() < 1e-9, "perpendicular should be vertical");
            assert!(dy.abs() > 1e-9);
            // Should pass through point (2, 3)
            assert!((l.start.x - 2.0).abs() < 1e-9);
            let min_y = l.start.y.min(l.end.y);
            let max_y = l.start.y.max(l.end.y);
            assert!(min_y <= 3.0 + 1e-9 && max_y >= 3.0 - 1e-9);
        } else {
            panic!("expected line");
        }
    }

    #[test]
    fn test_parallel_constraint() {
        let mut doc = Document::new();
        let a = doc.add_object(GeoObject::Point(PointObj::new(Point2::new(0.0, 0.0))));
        let b = doc.add_object(GeoObject::Point(PointObj::new(Point2::new(4.0, 2.0))));
        let line = doc.add_object(GeoObject::Line(LineObj::new(
            Point2::new(0.0, 0.0),
            Point2::new(4.0, 2.0),
        )));
        let p = doc.add_object(GeoObject::Point(PointObj::new(Point2::new(1.0, 5.0))));
        let (parallel, _) = doc.add_constructed_object(
            GeoObject::Line(
                LineObj::new_with_kind(
                    Point2::new(0.0, 0.0),
                    Point2::new(1.0, 1.0),
                    LineKind::Line,
                )
                .with_label("L"),
            ),
            "Parallel",
            &[line, p],
        );
        let order = doc.propagation_order(&[a, b, p]);
        doc.re_evaluate_constraints(&order);
        let parallel_obj = doc.get_object(parallel).unwrap();
        if let GeoObject::Line(l) = parallel_obj {
            let dx = l.end.x - l.start.x;
            let dy = l.end.y - l.start.y;
            // Slope should be 0.5 (same as reference line)
            assert!((dy / dx - 0.5).abs() < 1e-9);
            // Should pass through point (1, 5)
            let t = (1.0 - l.start.x) / dx;
            let y_at_x1 = l.start.y + t * dy;
            assert!((y_at_x1 - 5.0).abs() < 1e-9);
        } else {
            panic!("expected line");
        }
    }

    #[test]
    fn test_circle_by_center_radius_constraint() {
        let mut doc = Document::new();
        let center = doc.add_object(GeoObject::Point(PointObj::new(Point2::new(2.0, 3.0))));
        let mut params = HashMap::new();
        params.insert("radius".to_string(), 5.0);
        let (circle, _) = doc.add_constructed_object_with_params(
            GeoObject::Circle(CircleObj::new(Point2::new(0.0, 0.0), 1.0).with_label("C")),
            "CircleByCenterRadius",
            &[center],
            params,
        );
        let order = doc.propagation_order(&[center]);
        doc.re_evaluate_constraints(&order);
        let circle_obj = doc.get_object(circle).unwrap();
        if let GeoObject::Circle(c) = circle_obj {
            assert!((c.center.x - 2.0).abs() < 1e-9);
            assert!((c.center.y - 3.0).abs() < 1e-9);
            assert!((c.radius - 5.0).abs() < 1e-9);
        } else {
            panic!("expected circle");
        }
    }

    #[test]
    fn test_circle_by_three_points_constraint() {
        let mut doc = Document::new();
        let a = doc.add_object(GeoObject::Point(PointObj::new(Point2::new(1.0, 0.0))));
        let b = doc.add_object(GeoObject::Point(PointObj::new(Point2::new(0.0, 1.0))));
        let c = doc.add_object(GeoObject::Point(PointObj::new(Point2::new(-1.0, 0.0))));
        let (circle, _) = doc.add_constructed_object(
            GeoObject::Circle(CircleObj::new(Point2::new(0.0, 0.0), 1.0).with_label("C")),
            "CircleByThreePoints",
            &[a, b, c],
        );
        let order = doc.propagation_order(&[a, b, c]);
        doc.re_evaluate_constraints(&order);
        let circle_obj = doc.get_object(circle).unwrap();
        if let GeoObject::Circle(circ) = circle_obj {
            assert!((circ.center.x - 0.0).abs() < 1e-9);
            assert!((circ.center.y - 0.0).abs() < 1e-9);
            assert!((circ.radius - 1.0).abs() < 1e-9);
        } else {
            panic!("expected circle");
        }
    }

    #[test]
    fn test_ellipse_by_foci() {
        let mut doc = Document::new();
        let f1 = doc.add_point(Point2::new(-1.0, 0.0));
        let f2 = doc.add_point(Point2::new(1.0, 0.0));
        let p = doc.add_point(Point2::new(0.0, 2.0));
        let cons = doc.add_ellipse_by_foci_constraint(f1, f2, p);
        let order = doc.constraints.get_update_order(&[f1, f2, p]);
        doc.re_evaluate_constraints(&order);
        let out_id = doc.constraints.get_constraint(cons).unwrap().outputs[0];
        let ell = doc.get_object(out_id).unwrap();
        if let GeoObject::Ellipse(e) = ell {
            assert!((e.center.x).abs() < 1e-9);
            assert!((e.center.y).abs() < 1e-9);
            assert!((e.rx - 5f64.sqrt()).abs() < 1e-9);
            assert!((e.ry - 2.0).abs() < 1e-9);
            assert!((e.angle).abs() < 1e-9);
        } else {
            panic!("expected ellipse");
        }
    }

    #[test]
    fn test_parabola_by_focus_directrix() {
        let mut doc = Document::new();
        let focus = doc.add_point(Point2::new(0.0, 1.0));
        let directrix = doc.add_object(GeoObject::Line(
            LineObj::new_with_kind(
                Point2::new(-1.0, -1.0),
                Point2::new(1.0, -1.0),
                LineKind::Line,
            )
            .with_label("d"),
        ));
        let cons = doc.add_parabola_by_focus_directrix_constraint(focus, directrix);
        let order = doc.constraints.get_update_order(&[focus, directrix]);
        doc.re_evaluate_constraints(&order);
        let out_id = doc.constraints.get_constraint(cons).unwrap().outputs[0];
        let par = doc.get_object(out_id).unwrap();
        if let GeoObject::Parabola(pb) = par {
            assert!((pb.vertex.x).abs() < 1e-9);
            assert!((pb.vertex.y).abs() < 1e-9);
            assert!((pb.p - 1.0).abs() < 1e-9);
            assert!(pb.vertical);
            assert!((pb.angle).abs() < 1e-9);
        } else {
            panic!("expected parabola");
        }
    }

    #[test]
    fn test_hyperbola_by_foci() {
        let mut doc = Document::new();
        let f1 = doc.add_point(Point2::new(-1.0, 0.0));
        let f2 = doc.add_point(Point2::new(1.0, 0.0));
        let p = doc.add_point(Point2::new(2.0, 1.0));
        let cons = doc.add_hyperbola_by_foci_constraint(f1, f2, p);
        let order = doc.constraints.get_update_order(&[f1, f2, p]);
        doc.re_evaluate_constraints(&order);
        let out_id = doc.constraints.get_constraint(cons).unwrap().outputs[0];
        let hyp = doc.get_object(out_id).unwrap();
        if let GeoObject::Hyperbola(h) = hyp {
            assert!((h.center.x).abs() < 1e-9);
            assert!((h.center.y).abs() < 1e-9);
            let d1 = Point2::new(2.0, 1.0).distance(&Point2::new(-1.0, 0.0));
            let d2 = Point2::new(2.0, 1.0).distance(&Point2::new(1.0, 0.0));
            let a_expected = (d1 - d2).abs() * 0.5;
            let c = 1.0;
            let b_expected = (c * c - a_expected * a_expected).max(0.0).sqrt();
            assert!((h.a - a_expected).abs() < 1e-9);
            assert!((h.b - b_expected).abs() < 1e-9);
            assert!(h.horizontal);
        } else {
            panic!("expected hyperbola");
        }
    }

    #[test]
    fn test_conic_by_five_points_ellipse() {
        let mut doc = Document::new();
        let pts: Vec<ObjectId> = [
            Point2::new(1.0, 0.0),
            Point2::new(0.0, 1.0),
            Point2::new(-1.0, 0.0),
            Point2::new(0.0, -1.0),
            Point2::new(2f64.sqrt() / 2.0, 2f64.sqrt() / 2.0),
        ]
        .iter()
        .map(|p| doc.add_point(*p))
        .collect();
        let cons = doc.add_conic_by_five_points_constraint(&pts);
        let order = doc.constraints.get_update_order(&pts);
        doc.re_evaluate_constraints(&order);
        let out_id = doc.constraints.get_constraint(cons).unwrap().outputs[0];
        let obj = doc.get_object(out_id).unwrap();
        if let GeoObject::Ellipse(e) = obj {
            assert!((e.center.x).abs() < 1e-6);
            assert!((e.center.y).abs() < 1e-6);
            assert!((e.rx - 1.0).abs() < 1e-6);
            assert!((e.ry - 1.0).abs() < 1e-6);
        } else {
            panic!("expected ellipse from five points");
        }
    }

    #[test]
    fn test_conic_by_five_points_hyperbola() {
        let mut doc = Document::new();
        let x_at_y1 = 2.0 * 2f64.sqrt();
        let pts: Vec<ObjectId> = [
            Point2::new(2.0, 0.0),
            Point2::new(-2.0, 0.0),
            Point2::new(x_at_y1, 1.0),
            Point2::new(x_at_y1, -1.0),
            Point2::new(-x_at_y1, 1.0),
        ]
        .iter()
        .map(|p| doc.add_point(*p))
        .collect();
        let cons = doc.add_conic_by_five_points_constraint(&pts);
        let order = doc.constraints.get_update_order(&pts);
        doc.re_evaluate_constraints(&order);
        let out_id = doc.constraints.get_constraint(cons).unwrap().outputs[0];
        let obj = doc.get_object(out_id).unwrap();
        if let GeoObject::Hyperbola(h) = obj {
            assert!((h.center.x).abs() < 1e-6);
            assert!((h.center.y).abs() < 1e-6);
            assert!((h.a - 2.0).abs() < 1e-6);
            assert!((h.b - 1.0).abs() < 1e-6);
            assert!(h.horizontal);
        } else {
            panic!("expected hyperbola from five points");
        }
    }

    #[test]
    fn test_constraint_params_backward_compatible() {
        // Serialize a document with a constraint, then strip the params field to
        // simulate an old JSON document that has no params field on Constraint.
        let mut doc = Document::new();
        let a = doc.add_object(GeoObject::Point(PointObj::new(Point2::new(0.0, 0.0))));
        let (_m, _) = doc.add_constructed_object(
            GeoObject::Point(PointObj::new(Point2::new(1.0, 1.0)).with_label("M")),
            "Midpoint",
            &[a, a],
        );

        let mut json = serde_json::to_string(&doc).expect("serialize document");
        // Remove the params field from the serialized constraint to mimic old saves.
        json = json.replace(",\"params\":{}", "");

        let loaded: Document = serde_json::from_str(&json).expect("deserialize old document");
        let cons = loaded
            .constraints
            .get_constraint(0)
            .expect("constraint 0 should exist");
        assert!(cons.params.is_empty());
    }

    #[test]
    fn test_circle_radius_expr_recomputes() {
        let mut doc = Document::new();
        doc.set_variable("r".to_string(), 3.0);
        let mut circle = CircleObj::new(Point2::new(0.0, 0.0), 1.0);
        circle.radius_expr = Some("r".to_string());
        let id = doc.add_object(GeoObject::Circle(circle));
        doc.recompute_bound_parameters();
        if let GeoObject::Circle(c) = doc.get_object(id).unwrap() {
            assert!((c.radius - 3.0).abs() < 1e-9);
        } else {
            panic!("expected circle");
        }

        doc.set_variable("r".to_string(), 5.0);
        if let GeoObject::Circle(c) = doc.get_object(id).unwrap() {
            assert!((c.radius - 5.0).abs() < 1e-9);
        } else {
            panic!("expected circle");
        }
    }

    #[test]
    fn test_point_expr_recomputes() {
        let mut doc = Document::new();
        doc.set_variable("a".to_string(), 2.0);
        doc.set_variable("b".to_string(), 3.0);
        let mut point = PointObj::new(Point2::new(0.0, 0.0));
        point.x_expr = Some("a+1".to_string());
        point.y_expr = Some("2*b".to_string());
        let id = doc.add_object(GeoObject::Point(point));
        doc.recompute_bound_parameters();
        if let GeoObject::Point(p) = doc.get_object(id).unwrap() {
            assert!((p.position.x - 3.0).abs() < 1e-9);
            assert!((p.position.y - 6.0).abs() < 1e-9);
        } else {
            panic!("expected point");
        }
    }

    #[test]
    fn test_function_samples_caching() {
        let fun = FunctionObj::new("sin(x)");
        let vars = std::collections::HashMap::new();
        let domain = (0.0, std::f64::consts::TAU);
        let grid_size = 100;

        // First call computes and caches.
        let samples1 = {
            let guard = function_sampling::samples_or_compute(&fun, domain, grid_size, &vars);
            guard.clone()
        };
        assert!(!samples1.is_empty());

        // Second call with identical parameters returns the cached value.
        let samples2 = {
            let guard = function_sampling::samples_or_compute(&fun, domain, grid_size, &vars);
            guard.clone()
        };
        assert_eq!(samples1, samples2);
    }

    #[test]
    fn test_function_samples_pan_reuse() {
        let fun = FunctionObj::new("sin(x)");
        let vars = std::collections::HashMap::new();
        // Use non-symmetric bounds so the padded/snapped region does not sit
        // exactly on snap-cell boundaries; a tiny pan then reuses the cache.
        let domain = (0.1, 6.1);
        let grid_size = 100;

        let samples1 = {
            let guard = function_sampling::samples_or_compute(&fun, domain, grid_size, &vars);
            guard.clone()
        };
        let first_key = fun.cached_key.read().unwrap().clone();
        assert!(!samples1.is_empty());

        // A small pan stays inside the same snapped padded region.
        let panned = (domain.0 + 0.05, domain.1 + 0.05);
        let samples2 = {
            let guard = function_sampling::samples_or_compute(&fun, panned, grid_size, &vars);
            guard.clone()
        };
        let second_key = fun.cached_key.read().unwrap().clone();
        assert!(!samples2.is_empty());
        assert_eq!(first_key, second_key, "small pan should reuse cache");
    }

    #[test]
    fn test_function_samples_far_domain_recompute() {
        let fun = FunctionObj::new("sin(x)");
        let vars = std::collections::HashMap::new();
        let domain = (0.0, std::f64::consts::TAU);
        let grid_size = 100;

        drop(function_sampling::samples_or_compute(
            &fun, domain, grid_size, &vars,
        ));
        let first_key = fun.cached_key.read().unwrap().clone();

        // A far-away domain is outside the padded cached region, so it
        // recomputes and updates the cached key.
        let far = (10.0, 20.0);
        drop(function_sampling::samples_or_compute(
            &fun, far, grid_size, &vars,
        ));
        let second_key = fun.cached_key.read().unwrap().clone();

        assert_ne!(first_key, second_key, "far domain should update cache key");
    }

    #[test]
    fn test_expression_fields_backward_compatible() {
        // Old JSON without x_expr/y_expr/radius_expr should deserialize to None.
        let mut doc = Document::new();
        let id = doc.add_object(GeoObject::Point(PointObj::new(Point2::new(1.0, 2.0))));
        let mut json = serde_json::to_string(&doc).expect("serialize document");
        // Strip the new optional fields if present to mimic old saves.
        json = json.replace(",\"x_expr\":null", "");
        json = json.replace(",\"y_expr\":null", "");

        let loaded: Document = serde_json::from_str(&json).expect("deserialize old document");
        if let GeoObject::Point(p) = loaded.get_object(id).unwrap() {
            assert!(p.x_expr.is_none());
            assert!(p.y_expr.is_none());
            assert_eq!(p.position, Point2::new(1.0, 2.0));
        } else {
            panic!("expected point");
        }
    }

    #[test]
    fn test_parametric_curve_2d_caching() {
        let pc = ParametricCurve2DObj::new("cos(t)", "sin(t)", 0.0, std::f64::consts::TAU);
        let vars = std::collections::HashMap::new();
        let steps = 200;

        let samples1 = {
            let guard = parametric_sampling::samples_or_compute_curve_2d(&pc, steps, &vars);
            guard.clone()
        };
        assert!(!samples1.is_empty());
        let key1 = pc.cached_key.read().unwrap().clone();

        let samples2 = {
            let guard = parametric_sampling::samples_or_compute_curve_2d(&pc, steps, &vars);
            guard.clone()
        };
        assert_eq!(samples1, samples2);
        let key2 = pc.cached_key.read().unwrap().clone();
        assert_eq!(key1, key2, "cache key should be reused");
    }

    #[test]
    fn test_polar_curve_caching() {
        let pol = PolarCurveObj::new("1", 0.0, std::f64::consts::TAU);
        let vars = std::collections::HashMap::new();
        let steps = 200;

        let samples1 = {
            let guard = parametric_sampling::samples_or_compute_polar(&pol, steps, &vars);
            guard.clone()
        };
        assert!(!samples1.is_empty());
        let key1 = pol.cached_key.read().unwrap().clone();

        let samples2 = {
            let guard = parametric_sampling::samples_or_compute_polar(&pol, steps, &vars);
            guard.clone()
        };
        assert_eq!(samples1, samples2);
        let key2 = pol.cached_key.read().unwrap().clone();
        assert_eq!(key1, key2, "cache key should be reused");
    }

    #[test]
    fn test_surface_3d_caching() {
        let surf = Surface3DObj::new("x^2 + y^2", (-1.0, 1.0), (-1.0, 1.0));
        let vars = std::collections::HashMap::new();
        let res = 40;

        let grid1 = {
            let guard = parametric_sampling::samples_or_compute_surface(&surf, res, &vars);
            guard.clone()
        };
        assert!(!grid1.is_empty());
        let key1 = surf.cached_key.read().unwrap().clone();

        let grid2 = {
            let guard = parametric_sampling::samples_or_compute_surface(&surf, res, &vars);
            guard.clone()
        };
        assert_eq!(grid1, grid2);
        let key2 = surf.cached_key.read().unwrap().clone();
        assert_eq!(key1, key2, "cache key should be reused");
    }

    #[test]
    fn test_segment_expression_binding() {
        let mut doc = Document::new();
        let mut line = LineObj::new(Point2::new(0.0, 0.0), Point2::new(1.0, 1.0));
        line.start_x_expr = Some("a".to_string());
        line.end_y_expr = Some("b".to_string());
        doc.set_variable("a".to_string(), 5.0);
        doc.set_variable("b".to_string(), 10.0);
        assert_eq!(doc.resolve_expr(&line.start_x_expr, line.start.x), 5.0);
        assert_eq!(doc.resolve_expr(&line.end_y_expr, line.end.y), 10.0);
    }

    #[test]
    fn test_line_expression_binding() {
        let mut doc = Document::new();
        let line =
            LineObj::new_with_kind(Point2::new(0.0, 0.0), Point2::new(1.0, 1.0), LineKind::Line)
                .with_start_expr("cx - 1", "cy - 1")
                .with_end_expr("cx + 1", "cy + 1");
        doc.set_variable("cx".to_string(), 3.0);
        doc.set_variable("cy".to_string(), 4.0);
        assert_eq!(doc.resolve_expr(&line.start_x_expr, line.start.x), 2.0);
        assert_eq!(doc.resolve_expr(&line.start_y_expr, line.start.y), 3.0);
        assert_eq!(doc.resolve_expr(&line.end_x_expr, line.end.x), 4.0);
        assert_eq!(doc.resolve_expr(&line.end_y_expr, line.end.y), 5.0);
    }

    #[test]
    fn test_polygon_expression_binding() {
        let mut doc = Document::new();
        let mut poly = PolygonObj::new(vec![
            Point2::new(0.0, 0.0),
            Point2::new(1.0, 0.0),
            Point2::new(0.5, 1.0),
        ]);
        poly.set_vertex_expr(0, Some("px".to_string()), Some("py".to_string()));
        poly.set_vertex_expr(2, Some("qx".to_string()), Some("qy".to_string()));
        doc.set_variable("px".to_string(), -1.0);
        doc.set_variable("py".to_string(), -1.0);
        doc.set_variable("qx".to_string(), 2.0);
        doc.set_variable("qy".to_string(), 3.0);
        assert_eq!(doc.resolve_expr(poly.x_exprs.first().unwrap(), 0.0), -1.0);
        assert_eq!(doc.resolve_expr(poly.y_exprs.first().unwrap(), 0.0), -1.0);
        assert_eq!(doc.resolve_expr(poly.x_exprs.get(2).unwrap(), 0.5), 2.0);
        assert_eq!(doc.resolve_expr(poly.y_exprs.get(2).unwrap(), 1.0), 3.0);
    }

    #[test]
    fn test_function_domain_expression_binding() {
        let mut doc = Document::new();
        let mut fun = FunctionObj::new("sin(x)");
        fun.domain_min = Some(0.0);
        fun.domain_max = Some(1.0);
        fun.domain_min_expr = Some("a".to_string());
        fun.domain_max_expr = Some("b".to_string());
        doc.set_variable("a".to_string(), -2.0);
        doc.set_variable("b".to_string(), 2.0);
        assert_eq!(
            doc.resolve_expr(&fun.domain_min_expr, fun.domain_min.unwrap()),
            -2.0
        );
        assert_eq!(
            doc.resolve_expr(&fun.domain_max_expr, fun.domain_max.unwrap()),
            2.0
        );
    }

    #[test]
    fn test_parametric_curve_t_range_expression_binding() {
        let mut doc = Document::new();
        let mut pc = ParametricCurve2DObj::new("cos(t)", "sin(t)", 0.0, 1.0);
        pc.t_min_expr = Some("a".to_string());
        pc.t_max_expr = Some("b".to_string());
        doc.set_variable("a".to_string(), 0.0);
        doc.set_variable("b".to_string(), std::f64::consts::TAU);
        assert_eq!(doc.resolve_expr(&pc.t_min_expr, pc.t_min), 0.0);
        assert_eq!(
            doc.resolve_expr(&pc.t_max_expr, pc.t_max),
            std::f64::consts::TAU
        );
    }

    #[test]
    fn test_distance_constraint() {
        let mut doc = Document::new();
        let a = doc.add_point(Point2::new(0.0, 0.0));
        let b = doc.add_point(Point2::new(5.0, 0.0));
        doc.add_distance_constraint(a, b, 10.0);
        doc.re_evaluate_constraints(&[]);

        let pa = doc.point_position(a).unwrap();
        let pb = doc.point_position(b).unwrap();
        let d = pa.distance(&pb);
        assert!((d - 10.0).abs() < 1e-6, "distance should be 10, got {}", d);
    }

    #[test]
    fn test_angle_constraint() {
        let mut doc = Document::new();
        let l1 = doc.add_object(GeoObject::Line(LineObj::new(
            Point2::new(0.0, 0.0),
            Point2::new(1.0, 0.0),
        )));
        let l2 = doc.add_object(GeoObject::Line(LineObj::new(
            Point2::new(0.0, 0.0),
            Point2::new(1.0, 1.0),
        )));
        doc.add_angle_constraint(l1, l2, 90.0);
        doc.re_evaluate_constraints(&[]);

        let GeoObject::Line(line1) = doc.get_object(l1).unwrap() else {
            panic!("expected line");
        };
        let GeoObject::Line(line2) = doc.get_object(l2).unwrap() else {
            panic!("expected line");
        };
        let d1 = Point2::new(line1.end.x - line1.start.x, line1.end.y - line1.start.y);
        let d2 = Point2::new(line2.end.x - line2.start.x, line2.end.y - line2.start.y);
        let len1 = (d1.x * d1.x + d1.y * d1.y).sqrt();
        let len2 = (d2.x * d2.x + d2.y * d2.y).sqrt();
        assert!(len1 > 1e-6 && len2 > 1e-6);
        let dot = d1.x * d2.x + d1.y * d2.y;
        let cos_angle = dot / (len1 * len2);
        let angle = cos_angle.clamp(-1.0, 1.0).acos().to_degrees();
        assert!(
            (angle - 90.0).abs() < 1e-4,
            "angle should be 90°, got {}",
            angle
        );
    }

    #[test]
    fn test_tangent_constraint() {
        let mut doc = Document::new();
        let circle = doc.add_object(GeoObject::Circle(CircleObj::new(
            Point2::new(0.0, 0.0),
            5.0,
        )));
        let line = doc.add_object(GeoObject::Line(LineObj::new(
            Point2::new(1.0, 0.0),
            Point2::new(0.0, 1.0),
        )));
        doc.add_tangent_constraint(circle, line);
        doc.re_evaluate_constraints(&[]);

        let GeoObject::Circle(c) = doc.get_object(circle).unwrap() else {
            panic!("expected circle");
        };
        let GeoObject::Line(l) = doc.get_object(line).unwrap() else {
            panic!("expected line");
        };
        let dx = l.end.x - l.start.x;
        let dy = l.end.y - l.start.y;
        let len2 = dx * dx + dy * dy;
        let dist = if len2 < 1e-24 {
            c.center.distance(&l.start)
        } else {
            ((l.end.x - l.start.x) * (l.start.y - c.center.y)
                - (l.start.x - c.center.x) * (l.end.y - l.start.y))
                .abs()
                / len2.sqrt()
        };
        assert!(
            (dist - c.radius).abs() < 1e-6,
            "distance to line ({}) should equal radius ({})",
            dist,
            c.radius
        );
    }

    #[test]
    fn test_coincident_constraint() {
        let mut doc = Document::new();
        let a = doc.add_point(Point2::new(0.0, 0.0));
        let b = doc.add_point(Point2::new(3.0, 4.0));
        doc.add_coincident_constraint(a, b);
        let order = doc.constraints.get_update_order(&[a, b]);
        doc.re_evaluate_constraints(&order);

        let pa = doc.point_position(a).unwrap();
        let pb = doc.point_position(b).unwrap();
        assert!((pa.x - pb.x).abs() < 1e-6);
        assert!((pa.y - pb.y).abs() < 1e-6);
    }

    #[test]
    fn test_horizontal_constraint() {
        let mut doc = Document::new();
        let line = doc.add_object(GeoObject::Line(LineObj::new(
            Point2::new(0.0, 0.0),
            Point2::new(1.0, 1.0),
        )));
        doc.add_horizontal_constraint(line);
        doc.re_evaluate_constraints(&[]);

        let GeoObject::Line(l) = doc.get_object(line).unwrap() else {
            panic!("expected line");
        };
        assert!((l.start.y - l.end.y).abs() < 1e-6);
    }

    #[test]
    fn test_vertical_constraint() {
        let mut doc = Document::new();
        let line = doc.add_object(GeoObject::Line(LineObj::new(
            Point2::new(0.0, 0.0),
            Point2::new(1.0, 1.0),
        )));
        doc.add_vertical_constraint(line);
        doc.re_evaluate_constraints(&[]);

        let GeoObject::Line(l) = doc.get_object(line).unwrap() else {
            panic!("expected line");
        };
        assert!((l.start.x - l.end.x).abs() < 1e-6);
    }

    #[test]
    fn test_equal_length_constraint() {
        let mut doc = Document::new();
        let l1 = doc.add_object(GeoObject::Line(LineObj::new(
            Point2::new(0.0, 0.0),
            Point2::new(3.0, 0.0),
        )));
        let l2 = doc.add_object(GeoObject::Line(LineObj::new(
            Point2::new(0.0, 0.0),
            Point2::new(1.0, 1.0),
        )));
        doc.add_equal_length_constraint(l1, l2);
        doc.re_evaluate_constraints(&[]);

        let GeoObject::Line(line1) = doc.get_object(l1).unwrap() else {
            panic!("expected line");
        };
        let GeoObject::Line(line2) = doc.get_object(l2).unwrap() else {
            panic!("expected line");
        };
        let len1 = line1.start.distance(&line1.end);
        let len2 = line2.start.distance(&line2.end);
        assert!(
            (len1 - len2).abs() < 1e-6,
            "lengths should be equal: {} vs {}",
            len1,
            len2
        );
    }

    #[test]
    fn test_symmetry_constraint() {
        let mut doc = Document::new();
        let p = doc.add_point(Point2::new(1.0, 2.0));
        let q = doc.add_point(Point2::new(3.0, 4.0));
        let mirror = doc.add_object(GeoObject::Line(LineObj::new(
            Point2::new(0.0, 0.0),
            Point2::new(1.0, 0.0),
        )));
        doc.add_symmetry_constraint(p, q, mirror);
        doc.re_evaluate_constraints(&[]);

        let pa = doc.point_position(p).unwrap();
        let pb = doc.point_position(q).unwrap();
        let GeoObject::Line(m) = doc.get_object(mirror).unwrap() else {
            panic!("expected line");
        };

        let mid_x = (pa.x + pb.x) * 0.5;
        let mid_y = (pa.y + pb.y) * 0.5;
        let dir_x = m.end.x - m.start.x;
        let dir_y = m.end.y - m.start.y;

        let cross = dir_x * (mid_y - m.start.y) - dir_y * (mid_x - m.start.x);
        assert!(
            cross.abs() < 1e-5,
            "midpoint should lie on mirror line, cross={}",
            cross
        );

        let dot = (pb.x - pa.x) * dir_x + (pb.y - pa.y) * dir_y;
        assert!(
            dot.abs() < 1e-5,
            "q-p should be perpendicular to mirror line, dot={}",
            dot
        );
    }

    #[test]
    fn test_rotated_parabola_sampling() {
        let pb = ParabolaObj {
            id: ObjectId::new(),
            label: String::new(),
            vertex: Point2::new(1.0, 2.0),
            p: 1.0,
            vertical: true,
            angle: std::f64::consts::FRAC_PI_4,
            color: Color::BLACK,
            visible: true,
            width: 2.0,
        };
        let cos_a = pb.angle.cos();
        let sin_a = pb.angle.sin();

        // Vertex maps to itself.
        let lx0 = 0.0;
        let ly0 = 0.0;
        let wx0 = pb.vertex.x + lx0 * cos_a - ly0 * sin_a;
        let wy0 = pb.vertex.y + lx0 * sin_a + ly0 * cos_a;
        assert!((wx0 - 1.0).abs() < 1e-12);
        assert!((wy0 - 2.0).abs() < 1e-12);

        // t = 2 => local (2, 1), rotated 45 deg around vertex.
        let t = 2.0;
        let lx = t;
        let ly = t * t / (4.0 * pb.p);
        let wx = pb.vertex.x + lx * cos_a - ly * sin_a;
        let wy = pb.vertex.y + lx * sin_a + ly * cos_a;
        let expected = Point2::new(1.0 + 1.0 / 2f64.sqrt(), 2.0 + 3.0 / 2f64.sqrt());
        assert!((wx - expected.x).abs() < 1e-12, "wx={}", wx);
        assert!((wy - expected.y).abs() < 1e-12, "wy={}", wy);
    }

    #[test]
    fn test_rotated_hyperbola_sampling() {
        let hb = HyperbolaObj {
            id: ObjectId::new(),
            label: String::new(),
            center: Point2::new(1.0, 2.0),
            a: 2.0,
            b: 1.0,
            horizontal: true,
            angle: std::f64::consts::FRAC_PI_4,
            color: Color::BLACK,
            visible: true,
            width: 2.0,
        };
        let cos_a = hb.angle.cos();
        let sin_a = hb.angle.sin();

        // t = 0 => local (a, 0) = (2, 0), rotated 45 deg around center.
        let lx = hb.a;
        let ly = 0.0;
        let wx = hb.center.x + lx * cos_a - ly * sin_a;
        let wy = hb.center.y + lx * sin_a + ly * cos_a;
        assert!((wx - (1.0 + 2.0 / 2f64.sqrt())).abs() < 1e-12, "wx={}", wx);
        assert!((wy - (2.0 + 2.0 / 2f64.sqrt())).abs() < 1e-12, "wy={}", wy);
    }

    #[test]
    fn test_polygon_boolean_union_document() {
        use geo::BooleanOps;
        let mut doc = Document::new();
        let square = PolygonObj::new(vec![
            Point2::new(0.0, 0.0),
            Point2::new(2.0, 0.0),
            Point2::new(2.0, 2.0),
            Point2::new(0.0, 2.0),
        ]);
        let offset = PolygonObj::new(vec![
            Point2::new(1.0, 1.0),
            Point2::new(3.0, 1.0),
            Point2::new(3.0, 3.0),
            Point2::new(1.0, 3.0),
        ]);
        let geo_a = grafito_geometry::boolean::polygon_to_geo(&square.vertices);
        let geo_b = grafito_geometry::boolean::polygon_to_geo(&offset.vertices);
        doc.add_object(GeoObject::Polygon(square));
        doc.add_object(GeoObject::Polygon(offset));
        let union = geo_a.union(&geo_b);
        let polys = grafito_geometry::boolean::multipolygon_to_polygons(&union);
        assert_eq!(
            polys.len(),
            1,
            "union of two overlapping squares is one polygon"
        );
        // Area of union = 4 + 4 - 1 = 7
        let area: f64 = polys[0]
            .windows(2)
            .map(|w| w[0].x * w[1].y - w[1].x * w[0].y)
            .sum::<f64>()
            .abs()
            * 0.5;
        assert!((area - 7.0).abs() < 1e-9, "union area should be 7");
    }

    #[test]
    fn test_vector_field_cache_reuse() {
        let vf = VectorField2DObj::new("x", "y");
        let vars = std::collections::HashMap::new();
        let view = ViewTransform::new(800.0, 600.0);
        let view_bounds = (-3.0, 3.0, -3.0, 3.0);
        let grid_size = 20;

        let samples1 = {
            let guard =
                vector_field_sampling::samples_or_compute(&vf, view_bounds, grid_size, &vars);
            guard.clone()
        };
        let key1 = vf.cached_key.read().unwrap().clone();
        assert!(!samples1.is_empty());

        let samples2 = {
            let guard =
                vector_field_sampling::samples_or_compute(&vf, view_bounds, grid_size, &vars);
            guard.clone()
        };
        let key2 = vf.cached_key.read().unwrap().clone();
        assert_eq!(samples1, samples2);
        assert_eq!(key1, key2, "cache key should be reused");

        // The cached key stores the padded/snapped bounds, which contain the
        // requested view bounds.
        let cached_bounds = vf.cached_key.read().unwrap().as_ref().unwrap().view_bounds;
        assert!(
            view_bounds.0 >= cached_bounds.0
                && view_bounds.1 <= cached_bounds.1
                && view_bounds.2 >= cached_bounds.2
                && view_bounds.3 <= cached_bounds.3,
            "cached bounds should contain requested view bounds"
        );

        // The direct evaluator returns the same shape of samples.
        let direct =
            vector_field_sampling::evaluate_vector_field_2d(&vf, view_bounds, &view, &vars);
        assert_eq!(direct.len(), samples1.len());
    }

    #[test]
    fn test_vector_field_padded_domain() {
        let vf = VectorField2DObj::new("x", "y");
        let vars = std::collections::HashMap::new();
        let view_bounds = (-3.0, 3.0, -3.0, 3.0);
        let grid_size = 20;

        let samples1 = {
            let guard =
                vector_field_sampling::samples_or_compute(&vf, view_bounds, grid_size, &vars);
            guard.clone()
        };

        // A small pan stays inside the padded/snapped region, so the cache is
        // reused even though the requested view bounds changed slightly.
        let panned = (
            view_bounds.0 + 0.1,
            view_bounds.1 + 0.1,
            view_bounds.2,
            view_bounds.3,
        );
        let samples2 = {
            let guard = vector_field_sampling::samples_or_compute(&vf, panned, grid_size, &vars);
            guard.clone()
        };
        assert_eq!(
            samples1.len(),
            samples2.len(),
            "small pan should reuse cache"
        );

        // A far-away view leaves the cached region and recomputes.
        drop(vector_field_sampling::samples_or_compute(
            &vf,
            view_bounds,
            grid_size,
            &vars,
        ));
        let first_key = vf.cached_key.read().unwrap().clone();
        let far = (10.0, 16.0, 10.0, 16.0);
        drop(vector_field_sampling::samples_or_compute(
            &vf, far, grid_size, &vars,
        ));
        let second_key = vf.cached_key.read().unwrap().clone();
        assert_ne!(first_key, second_key, "far pan should update cache key");
    }

    #[test]
    fn test_conic_by_five_points_rotated() {
        let mut doc = Document::new();
        // Five points on an ellipse with rx=2, ry=1 rotated by 45 deg around the origin.
        let angle = std::f64::consts::FRAC_PI_4;
        let cos_a = angle.cos();
        let sin_a = angle.sin();
        let rx = 2.0;
        let ry = 1.0;
        let ts = [0.0, 0.6, 1.2, 2.5, 4.0];
        let pts: Vec<ObjectId> = ts
            .iter()
            .map(|&t: &f64| {
                let cx = rx * t.cos();
                let sy = ry * t.sin();
                let x = cx * cos_a - sy * sin_a;
                let y = cx * sin_a + sy * cos_a;
                doc.add_point(Point2::new(x, y))
            })
            .collect();
        let cons = doc.add_conic_by_five_points_constraint(&pts);
        let order = doc.constraints.get_update_order(&pts);
        doc.re_evaluate_constraints(&order);
        let out_id = doc.constraints.get_constraint(cons).unwrap().outputs[0];
        let obj = doc.get_object(out_id).unwrap();
        if let GeoObject::Ellipse(e) = obj {
            assert!((e.center.x).abs() < 1e-6);
            assert!((e.center.y).abs() < 1e-6);
            // The eigendecomposition may swap rx/ry and add a 90 deg phase to the angle.
            let swapped = (e.rx - ry).abs() < 1e-6
                && (e.ry - rx).abs() < 1e-6
                && ((e.angle - angle).abs() - std::f64::consts::FRAC_PI_2).abs() < 1e-6;
            let direct = (e.rx - rx).abs() < 1e-6
                && (e.ry - ry).abs() < 1e-6
                && (e.angle - angle).abs() < 1e-6;
            assert!(
                swapped || direct,
                "unexpected ellipse parameters: rx={}, ry={}, angle={}",
                e.rx,
                e.ry,
                e.angle
            );
        } else {
            panic!("expected ellipse from five points");
        }
    }

    #[test]
    fn test_cpu_function_evaluation_no_cross_contamination() {
        let vars = std::collections::HashMap::new();
        let domain = (-std::f64::consts::PI, std::f64::consts::PI);
        let grid_size = 100;

        // Evaluate x^2 first.
        let fun_x2 = FunctionObj::new("x^2");
        drop(function_sampling::samples_or_compute(
            &fun_x2, domain, grid_size, &vars,
        ));

        // Immediately evaluate sin(x) on the CPU fallback path and verify the
        // samples stay inside the expected range.
        let fun_sin = FunctionObj::new("sin(x)");
        let samples = function_sampling::samples_or_compute(&fun_sin, domain, grid_size, &vars);
        assert!(!samples.is_empty(), "sin(x) should produce samples");
        for (x, y_opt) in samples.iter() {
            if let Some(y) = y_opt {
                assert!(
                    y.abs() <= 1.0 + 1e-6,
                    "sin({}) = {} is outside [-1, 1]",
                    x,
                    y
                );
            }
        }
    }

    #[test]
    fn test_integral_function_sampling() {
        // ∫₀ˣ t² dt = x³/3
        let fun = FunctionObj::new("x^2").as_integral("x", 0.0);
        let domain = (0.0, 3.0);
        let grid_size = 100;
        let vars = HashMap::new();
        let samples = function_sampling::samples_or_compute(&fun, domain, grid_size, &vars);
        assert!(!samples.is_empty(), "integral samples should not be empty");

        let expected_at_3 = 9.0; // 3³/3
        let (_, y_at_3) = samples
            .iter()
            .min_by(|(x, _), (x2, _)| (x - 3.0).abs().partial_cmp(&(x2 - 3.0).abs()).unwrap())
            .expect("sample near x=3");
        let y_at_3 = y_at_3.expect("integral should be finite near x=3");
        assert!(
            (y_at_3 - expected_at_3).abs() < 0.05,
            "integral of x^2 from 0 to 3 should be ~9, got {}",
            y_at_3
        );

        // Check intermediate point x=1 → 1/3
        let (_, y_at_1) = samples
            .iter()
            .min_by(|(x, _), (x2, _)| (x - 1.0).abs().partial_cmp(&(x2 - 1.0).abs()).unwrap())
            .expect("sample near x=1");
        let y_at_1 = y_at_1.expect("integral should be finite near x=1");
        assert!(
            (y_at_1 - 1.0 / 3.0).abs() < 0.05,
            "integral at x=1 should be ~1/3, got {}",
            y_at_1
        );
    }

    #[test]
    fn test_piecewise_function_sampling() {
        // piecewise(x<0, x^2, x>=0, sqrt(x))
        let fun = FunctionObj::new("piecewise(x<0, x^2, x>=0, sqrt(x))");
        let domain = (-2.0, 2.0);
        let grid_size = 200;
        let vars = HashMap::new();
        let samples = function_sampling::samples_or_compute(&fun, domain, grid_size, &vars);
        assert!(!samples.is_empty(), "piecewise samples should not be empty");

        // At x=-1 (x<0 true): should return (-1)^2 = 1
        let (x_at, y_at) = samples
            .iter()
            .min_by(|(x, _), (x2, _)| (x + 1.0).abs().partial_cmp(&(x2 + 1.0).abs()).unwrap())
            .expect("sample near x=-1");
        let y_neg1 = y_at
            .unwrap_or_else(|| panic!("piecewise should be finite at x={}, sample={}", x_at, x_at));
        assert!(
            (y_neg1 - 1.0).abs() < 0.15,
            "piecewise at x={} should be ~1, got {}",
            x_at,
            y_neg1
        );

        // At x=1 (x<0 false, x>=0 true): should return sqrt(1) = 1
        let (x_at2, y_at2) = samples
            .iter()
            .min_by(|(x, _), (x2, _)| (x - 1.0).abs().partial_cmp(&(x2 - 1.0).abs()).unwrap())
            .expect("sample near x=1");
        let y_pos1 = y_at2.unwrap_or_else(|| {
            panic!(
                "piecewise should be finite at x={}, sample={}",
                x_at2, x_at2
            )
        });
        assert!(
            (y_pos1 - 1.0).abs() < 0.15,
            "piecewise at x={} should be ~1, got {}",
            x_at2,
            y_pos1
        );
    }
}
