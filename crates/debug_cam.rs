use glam::{Vec3, Mat4};

fn main() {
    let aspect = 1.6;
    let distance = 60.0_f32;
    let phi = 0.6_f32;
    let theta = 0.8_f32;
    let target = Vec3::new(0.0, 0.0, 20.0);
    
    let pos = Vec3::new(
        distance * phi.cos() * theta.cos(),
        distance * phi.sin(),
        distance * phi.cos() * theta.sin(),
    ) + target;

    println!("Camera pos: {:?}", pos);
    
    let view = Mat4::look_at_rh(pos, target, Vec3::Y);
    let proj = Mat4::perspective_rh(60.0_f32.to_radians(), aspect, 0.1, 1000.0);
    let mvp = proj * view;
    
    // A point on the attractor (Lorenz typical point)
    let p = Vec3::new(10.0, 20.0, 25.0);
    
    let clip = mvp * p.extend(1.0);
    println!("Clip coords: {:?}", clip);
    
    if clip.w > 0.0 {
        let ndc_x = clip.x / clip.w;
        let ndc_y = clip.y / clip.w;
        let sx = (ndc_x + 1.0) * 0.5 * 1000.0;
        let sy = (1.0 - ndc_y) * 0.5 * 800.0;
        println!("Screen coords: {}, {}", sx, sy);
    } else {
        println!("Point is behind camera! clip.w = {}", clip.w);
    }
}
