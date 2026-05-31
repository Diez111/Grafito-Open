use glam::{Vec3, Mat4};

#[test]
fn test_camera_project() {
    let aspect = 1.6;
    let mut camera = grafito_geometry::types3d::Camera3D::new(aspect);
    camera.distance = 60.0;
    camera.target = Vec3::new(0.0, 0.0, 20.0);
    
    let p = grafito_geometry::types3d::Point3D::new(10.0, 20.0, 25.0);
    let proj = camera.project(&p, 1000.0, 800.0);
    println!("Projection of (10, 20, 25): {:?}", proj);
}
