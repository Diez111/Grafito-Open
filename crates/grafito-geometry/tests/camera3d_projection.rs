#![allow(clippy::unwrap_used, clippy::expect_used)]
use glam::Vec3;
use grafito_geometry::{curve_3d_segment_is_continuous, Aabb3D, Camera3D, Point3D};

fn assert_point_close(actual: Point3D, expected: Point3D, tolerance: f64) {
    assert!(
        actual.distance(&expected) <= tolerance,
        "expected {expected:?}, got {actual:?}"
    );
}

#[test]
fn cpu_projection_rejects_a_finite_point_that_overflows_f32() {
    let camera = Camera3D::new(4.0 / 3.0);
    let point = Point3D::new(f64::MAX, 0.0, 0.0);

    assert_eq!(camera.project(&point, 800.0, 600.0), None);
}

#[test]
fn cpu_projection_rejects_a_point_beyond_the_far_plane() {
    let mut camera = Camera3D::new(4.0 / 3.0);
    camera.theta = 0.0;
    camera.phi = 0.0;
    camera.distance = 10.0;
    camera.near = 0.1;
    camera.far = 5.0;

    assert_eq!(
        camera.project(&Point3D::new(0.0, 0.0, 0.0), 800.0, 600.0),
        None
    );
    assert!(camera
        .project(&Point3D::new(7.0, 0.0, 0.0), 800.0, 600.0)
        .is_some());
}

#[test]
fn curve_discontinuity_policy_breaks_finite_pole_jumps() {
    let camera = Camera3D::new(4.0 / 3.0);

    assert!(curve_3d_segment_is_continuous(
        Point3D::new(0.0, 0.0, 0.0),
        Point3D::new(1.0, 0.0, 0.0),
        &camera,
    ));
    assert!(!curve_3d_segment_is_continuous(
        Point3D::new(-10_000.0, 0.0, 0.0),
        Point3D::new(10_000.0, 0.0, 0.0),
        &camera,
    ));
}

#[test]
fn center_screen_ray_points_from_camera_through_target() {
    let mut camera = Camera3D::new(4.0 / 3.0);
    camera.target = Vec3::new(2.0, -1.0, 3.0);
    camera.distance = 12.0;

    let ray = camera
        .screen_ray(400.0, 300.0, 800.0, 600.0)
        .expect("valid center ray");
    let expected_direction = (camera.target - camera.position()).normalize();
    let direction = ray.direction.to_vec3();

    // El origen se unproyecta en la placa near (matemática f32); con el
    // rango de zoom ampliado el condicionamiento numérico relaja la precisión
    // del origen a 1e-3 — la dirección y el plano siguen verificados rigurosamente.
    assert_point_close(ray.origin, Point3D::from_vec3(camera.position()), 1.0e-3);
    assert!(direction.dot(expected_direction) > 0.999_999);
    assert!(ray.min_distance >= camera.near as f64);
    // El margen del rayo escala con far (desproyección en el borde); el rango
    // ampliado de zoom lo vuelve ~0.8%, se verifica con 1% de holgura.
    assert!(
        ray.max_distance <= camera.far as f64 * 1.01 + 1.0,
        "ray.max_distance={} far={}",
        ray.max_distance,
        camera.far
    );

    let plane = camera.construction_plane().expect("valid target plane");
    assert_point_close(
        plane
            .intersect_ray(&ray)
            .expect("center ray hits target plane"),
        Point3D::from_vec3(camera.target),
        1.0e-4,
    );
}

#[test]
fn off_center_screen_ray_round_trips_canvas_local_pointer() {
    let camera = Camera3D::new(16.0 / 9.0);
    let pointer = (123.0, 211.0);
    let ray = camera
        .screen_ray(pointer.0, pointer.1, 1280.0, 720.0)
        .expect("valid off-center ray");
    let plane = camera.construction_plane().expect("valid target plane");
    let hit = plane.intersect_ray(&ray).expect("ray hits target plane");
    let projected = camera
        .project(&hit, 1280.0, 720.0)
        .expect("construction point projects back to canvas");

    assert!((projected.0 - pointer.0).abs() < 1.0e-2);
    assert!((projected.1 - pointer.1).abs() < 1.0e-2);
    assert!(hit.distance(&Point3D::from_vec3(camera.target)) > 0.1);
}

#[test]
fn screen_ray_rejects_nonfinite_degenerate_or_outside_inputs() {
    let camera = Camera3D::new(4.0 / 3.0);
    assert!(camera.screen_ray(0.0, 0.0, 0.0, 600.0).is_none());
    assert!(camera.screen_ray(-1.0, 0.0, 800.0, 600.0).is_none());
    assert!(camera.screen_ray(801.0, 0.0, 800.0, 600.0).is_none());

    let mut invalid = camera;
    invalid.far = invalid.near;
    assert!(invalid.screen_ray(400.0, 300.0, 800.0, 600.0).is_none());

    invalid = camera;
    invalid.target.x = f32::NAN;
    assert!(invalid.screen_ray(400.0, 300.0, 800.0, 600.0).is_none());

    invalid = camera;
    invalid.distance = 0.0;
    assert!(invalid.construction_plane().is_none());
}

#[test]
fn ray_primitive_hits_obey_near_and_far_clipping() {
    let mut camera = Camera3D::new(4.0 / 3.0);
    camera.theta = 0.0;
    camera.phi = 0.0;
    let ray = camera
        .screen_ray(400.0, 300.0, 800.0, 600.0)
        .expect("valid center ray");
    let forward = ray.direction.to_vec3();
    let center = Point3D::from_vec3(camera.position() + forward * 5.0);

    let sphere_hit = ray
        .intersect_sphere(center, 1.0)
        .expect("sphere in the frustum must be hit");
    assert!((sphere_hit - 4.0).abs() < 1.0e-3);

    let half = Vec3::splat(0.5);
    let bounds = Aabb3D::new(
        Point3D::from_vec3(center.to_vec3() - half),
        Point3D::from_vec3(center.to_vec3() + half),
    )
    .expect("valid bounds");
    assert!((ray.intersect_aabb(bounds).expect("box hit") - 4.5).abs() < 1.0e-3);

    let behind_camera = Point3D::from_vec3(camera.position() - forward * 5.0);
    assert!(ray.intersect_sphere(behind_camera, 1.0).is_none());

    // Bien más allá del plano lejano (y del margen del rayo) para que el
    // recorte por far sea determinista aunque gane profundidad de campo.
    let beyond_far = Point3D::from_vec3(camera.position() + forward * (camera.far * 1.5));
    assert!(ray.intersect_sphere(beyond_far, 1.0).is_none());
}

#[test]
fn ray_segment_proximity_handles_crossing_and_degenerate_segments() {
    let mut camera = Camera3D::new(4.0 / 3.0);
    camera.theta = 0.0;
    camera.phi = 0.0;
    let ray = camera
        .screen_ray(400.0, 300.0, 800.0, 600.0)
        .expect("valid center ray");

    let crossing = ray
        .closest_to_segment(Point3D::new(0.0, -2.0, 0.0), Point3D::new(0.0, 2.0, 0.0))
        .expect("crossing segment proximity");
    assert!(
        crossing.separation < 1.0e-6,
        "unexpected crossing proximity: {crossing:?}"
    );
    assert!((crossing.distance_along_ray - 10.0).abs() < 1.0e-3);

    let point = Point3D::new(0.0, 0.0, 0.0);
    let degenerate = ray
        .closest_to_segment(point, point)
        .expect("point proximity");
    assert!(
        degenerate.separation < 1.0e-6,
        "unexpected point proximity: {degenerate:?}"
    );
    assert!((degenerate.distance_along_ray - 10.0).abs() < 1.0e-3);
}
