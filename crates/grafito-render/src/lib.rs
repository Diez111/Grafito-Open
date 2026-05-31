//! Grafito Render — GPU-accelerated 2D/3D renderer using wgpu.

use grafito_geometry::{Color, Point2, Point3D, ViewTransform, Camera3D};
use grafito_geometry::expr::eval_function_with_vars;
use grafito_core::{Document, GeoObject};
use wgpu::util::DeviceExt;
use rayon::prelude::*;

#[cfg(test)]
mod tests;

/// Simple lighting calculation for 3D objects
pub fn calculate_lighting(base_color: Color, normal: glam::Vec3, light_dir: glam::Vec3) -> Color {
    let ambient = 0.3;
    let diffuse = 0.7;
    
    let normal = normal.normalize();
    let light_dir = light_dir.normalize();
    
    let dot = normal.dot(light_dir).max(0.0);
    let intensity = ambient + diffuse * dot;
    
    Color::new(
        (base_color.r * intensity).min(1.0),
        (base_color.g * intensity).min(1.0),
        (base_color.b * intensity).min(1.0),
        base_color.a,
    )
}

/// A simple vertex with position and color.
#[repr(C)]
#[derive(Copy, Clone, Debug, bytemuck::Pod, bytemuck::Zeroable)]
pub struct Vertex {
    pub position: [f32; 3],
    pub color: [f32; 4],
}

impl Vertex {
    pub fn new(x: f32, y: f32, color: Color) -> Self {
        Self {
            position: [x, y, 0.0],
            color: color.to_array(),
        }
    }

    pub fn desc() -> wgpu::VertexBufferLayout<'static> {
        wgpu::VertexBufferLayout {
            array_stride: std::mem::size_of::<Vertex>() as wgpu::BufferAddress,
            step_mode: wgpu::VertexStepMode::Vertex,
            attributes: &[
                wgpu::VertexAttribute {
                    offset: 0,
                    shader_location: 0,
                    format: wgpu::VertexFormat::Float32x3,
                },
                wgpu::VertexAttribute {
                    offset: std::mem::size_of::<[f32; 3]>() as wgpu::BufferAddress,
                    shader_location: 1,
                    format: wgpu::VertexFormat::Float32x4,
                },
            ],
        }
    }
}

pub struct Renderer {
    pub pipeline: wgpu::RenderPipeline,
    pub pipeline_3d: wgpu::RenderPipeline,
    pub mvp_bind_group_layout: wgpu::BindGroupLayout,
    pub mvp_buffer: wgpu::Buffer,
    pub mvp_bind_group: wgpu::BindGroup,
}

impl Renderer {
    pub fn new(device: &wgpu::Device, target_format: wgpu::TextureFormat, enable_msaa: bool) -> Self {
        let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("Grafito Shader"),
            source: wgpu::ShaderSource::Wgsl(include_str!("shader.wgsl").into()),
        });

        let mvp_bind_group_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("MVP Bind Group Layout"),
            entries: &[wgpu::BindGroupLayoutEntry {
                binding: 0,
                visibility: wgpu::ShaderStages::VERTEX,
                ty: wgpu::BindingType::Buffer {
                    ty: wgpu::BufferBindingType::Uniform,
                    has_dynamic_offset: false,
                    min_binding_size: None,
                },
                count: None,
            }],
        });

        let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("Render Pipeline Layout"),
            bind_group_layouts: &[&mvp_bind_group_layout],
            push_constant_ranges: &[],
        });

        let multisample = if enable_msaa {
            wgpu::MultisampleState {
                count: 4,
                mask: !0,
                alpha_to_coverage_enabled: false,
            }
        } else {
            wgpu::MultisampleState::default()
        };

        let pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("2D Render Pipeline"),
            layout: Some(&pipeline_layout),
            vertex: wgpu::VertexState {
                module: &shader,
                entry_point: "vs_main",
                buffers: &[Vertex::desc()],
                compilation_options: wgpu::PipelineCompilationOptions::default(),
            },
            fragment: Some(wgpu::FragmentState {
                module: &shader,
                entry_point: "fs_main",
                compilation_options: wgpu::PipelineCompilationOptions::default(),
                targets: &[Some(wgpu::ColorTargetState {
                    format: target_format,
                    blend: Some(wgpu::BlendState::ALPHA_BLENDING),
                    write_mask: wgpu::ColorWrites::ALL,
                })],
            }),
            primitive: wgpu::PrimitiveState {
                topology: wgpu::PrimitiveTopology::TriangleList,
                strip_index_format: None,
                front_face: wgpu::FrontFace::Ccw,
                cull_mode: None,
                polygon_mode: wgpu::PolygonMode::Fill,
                unclipped_depth: false,
                conservative: false,
            },
            depth_stencil: None,
            multisample,
            multiview: None,
            cache: None,
        });

        let pipeline_3d = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("3D Render Pipeline"),
            layout: Some(&pipeline_layout),
            vertex: wgpu::VertexState {
                module: &shader,
                entry_point: "vs_main",
                buffers: &[Vertex::desc()],
                compilation_options: wgpu::PipelineCompilationOptions::default(),
            },
            fragment: Some(wgpu::FragmentState {
                module: &shader,
                entry_point: "fs_main",
                compilation_options: wgpu::PipelineCompilationOptions::default(),
                targets: &[Some(wgpu::ColorTargetState {
                    format: target_format,
                    blend: Some(wgpu::BlendState::ALPHA_BLENDING),
                    write_mask: wgpu::ColorWrites::ALL,
                })],
            }),
            primitive: wgpu::PrimitiveState {
                topology: wgpu::PrimitiveTopology::TriangleList,
                strip_index_format: None,
                front_face: wgpu::FrontFace::Ccw,
                cull_mode: Some(wgpu::Face::Back),
                polygon_mode: wgpu::PolygonMode::Fill,
                unclipped_depth: false,
                conservative: false,
            },
            depth_stencil: None,
            multisample,
            multiview: None,
            cache: None,
        });

        let mvp_buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("MVP Buffer"),
            contents: bytemuck::cast_slice(&[glam::Mat4::IDENTITY.to_cols_array()]),
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
        });

        let mvp_bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("MVP Bind Group"),
            layout: &mvp_bind_group_layout,
            entries: &[wgpu::BindGroupEntry {
                binding: 0,
                resource: mvp_buffer.as_entire_binding(),
            }],
        });

        Self {
            pipeline,
            pipeline_3d,
            mvp_bind_group_layout,
            mvp_buffer,
            mvp_bind_group,
        }
    }

    pub fn update_mvp(&self, queue: &wgpu::Queue, mvp: glam::Mat4) {
        queue.write_buffer(
            &self.mvp_buffer,
            0,
            bytemuck::cast_slice(&mvp.to_cols_array()),
        );
    }

    #[cfg(test)]
    pub fn build_geometry_static(
        document: &Document,
        view: &ViewTransform,
        dark_mode: bool,
    ) -> (Vec<Vertex>, Vec<u32>) {
        let mut vertices = Vec::new();
        let mut indices = Vec::new();

        Self::build_grid_static(&mut vertices, &mut indices, view, dark_mode);
        Self::build_axes_static(&mut vertices, &mut indices, view, dark_mode);

        for (_, obj) in document.objects_iter() {
            if !obj.is_visible() {
                continue;
            }
            match obj {
                GeoObject::Point(p) => {
                    let screen = view.world_to_screen(p.position);
                    let size = p.size.max(1.0);
                    Self::add_rect(&mut vertices, &mut indices, screen, size, size, p.color);
                }
                GeoObject::Line(l) => {
                    let a = view.world_to_screen(l.start);
                    let b = view.world_to_screen(l.end);
                    Self::add_line_segment(&mut vertices, &mut indices, a, b, l.width, l.color);
                }
                GeoObject::Circle(c) => {
                    let center = view.world_to_screen(c.center);
                    let radius = (c.radius as f32) * view.scale as f32;
                    Self::add_circle_stroke(&mut vertices, &mut indices, center, radius, c.width, c.color);
                    if let Some(fill) = c.fill_color {
                        Self::add_circle_fill(&mut vertices, &mut indices, center, radius, fill);
                    }
                }
                GeoObject::Polygon(poly) => {
                    if poly.vertices.len() >= 3 {
                        let screen_verts: Vec<_> = poly.vertices.iter()
                            .map(|v| view.world_to_screen(*v))
                            .collect();
                        if let Some(fill) = poly.fill_color {
                            Self::add_polygon_fill(&mut vertices, &mut indices, &screen_verts, fill);
                        }
                        Self::add_polygon_stroke(&mut vertices, &mut indices, &screen_verts, poly.width, poly.color);
                    }
                }
                _ => {}
            }
        }

        (vertices, indices)
    }

    #[cfg(test)]
    fn build_grid_static(
        vertices: &mut Vec<Vertex>,
        indices: &mut Vec<u32>,
        view: &ViewTransform,
        dark_mode: bool,
    ) {
        let world_tl = view.screen_to_world(glam::Vec2::new(0.0, 0.0));
        let world_br = view.screen_to_world(view.screen_size);

        let min_x = world_tl.x.floor() as i32 - 1;
        let max_x = world_br.x.ceil() as i32 + 1;
        let min_y = world_br.y.floor() as i32 - 1;
        let max_y = world_tl.y.ceil() as i32 + 1;

        let color = if dark_mode {
            Color::new(0.25, 0.25, 0.25, 1.0)
        } else {
            Color::LIGHT_GRAY
        };

        for x in min_x..=max_x {
            let a = view.world_to_screen(Point2::new(x as f64, min_y as f64));
            let b = view.world_to_screen(Point2::new(x as f64, max_y as f64));
            Self::add_line_segment(vertices, indices, a, b, 1.0, color);
        }

        for y in min_y..=max_y {
            let a = view.world_to_screen(Point2::new(min_x as f64, y as f64));
            let b = view.world_to_screen(Point2::new(max_x as f64, y as f64));
            Self::add_line_segment(vertices, indices, a, b, 1.0, color);
        }
    }

    #[cfg(test)]
    fn build_axes_static(
        vertices: &mut Vec<Vertex>,
        indices: &mut Vec<u32>,
        view: &ViewTransform,
        dark_mode: bool,
    ) {
        let world_tl = view.screen_to_world(glam::Vec2::new(0.0, 0.0));
        let world_br = view.screen_to_world(view.screen_size);

        let x_axis_y = 0.0f64.clamp(world_br.y, world_tl.y);
        let y_axis_x = 0.0f64.clamp(world_tl.x, world_br.x);

        let axis_color = if dark_mode {
            Color::new(0.7, 0.7, 0.7, 1.0)
        } else {
            Color::BLACK
        };

        let x_axis_a = view.world_to_screen(Point2::new(world_tl.x, x_axis_y));
        let x_axis_b = view.world_to_screen(Point2::new(world_br.x, x_axis_y));
        Self::add_line_segment(vertices, indices, x_axis_a, x_axis_b, 2.0, axis_color);

        let y_axis_a = view.world_to_screen(Point2::new(y_axis_x, world_br.y));
        let y_axis_b = view.world_to_screen(Point2::new(y_axis_x, world_tl.y));
        Self::add_line_segment(vertices, indices, y_axis_a, y_axis_b, 2.0, axis_color);
    }

    #[cfg(test)]
    pub fn build_3d_geometry_static(
        document: &Document,
        camera: &Camera3D,
        dark_mode: bool,
        screen_w: f32,
        screen_h: f32,
    ) -> (Vec<Vertex>, Vec<u32>) {
        let mut vertices = Vec::new();
        let mut indices = Vec::new();

        Self::build_3d_grid_static(&mut vertices, &mut indices, camera, dark_mode, screen_w, screen_h);
        Self::build_3d_axes_static(&mut vertices, &mut indices, camera, screen_w, screen_h);

        for (_, obj) in document.objects_iter() {
            if !obj.is_visible() {
                continue;
            }
            match obj {
                GeoObject::Point3D(p) => {
                    if let Some(screen_pos) = camera.project(&p.position, screen_w, screen_h) {
                        let size = p.size.max(1.0);
                        Self::add_rect(&mut vertices, &mut indices, 
                            glam::Vec2::new(screen_pos.0, screen_pos.1), 
                            size, size, p.color);
                    }
                }
                _ => {}
            }
        }

        (vertices, indices)
    }

    #[cfg(test)]
    fn build_3d_grid_static(
        vertices: &mut Vec<Vertex>,
        indices: &mut Vec<u32>,
        camera: &Camera3D,
        dark_mode: bool,
        screen_w: f32,
        screen_h: f32,
    ) {
        let fov_rad = camera.fov.to_radians();
        let frustum_height = 2.0 * camera.distance * (fov_rad * 0.5).tan();
        let pixels_per_unit = (screen_h / frustum_height) as f64;
        let target_world_step = 80.0 / pixels_per_unit.max(1e-6);
        let magnitude = target_world_step.log10().floor();
        let base = 10f64.powf(magnitude);
        let factor = target_world_step / base;
        
        let major_step = if factor < 2.0 { 1.0 * base }
            else if factor < 5.0 { 2.0 * base }
            else { 5.0 * base };
        let minor_step = major_step / 5.0;

        if minor_step <= 1e-9 { return; }

        let color = if dark_mode {
            Color::new(0.25, 0.25, 0.25, 1.0)
        } else {
            Color::LIGHT_GRAY
        };

        let center_x = camera.target.x as f64;
        let center_z = camera.target.z as f64;
        let aspect = screen_w / screen_h.max(1.0);
        let view_range = (frustum_height * aspect.max(1.0) * 1.8) as f64;

        let start_x = ((center_x - view_range) / minor_step).floor() * minor_step;
        let end_x = ((center_x + view_range) / minor_step).ceil() * minor_step;
        let start_z = ((center_z - view_range) / minor_step).floor() * minor_step;
        let end_z = ((center_z + view_range) / minor_step).ceil() * minor_step;

        let line_count_x = ((end_x - start_x) / minor_step).round() as i64;
        let line_count_z = ((end_z - start_z) / minor_step).round() as i64;

        if line_count_x <= 500 && line_count_z <= 500 {
            for xi in 0..=line_count_x {
                let x = start_x + xi as f64 * minor_step;
                let p1 = Point3D::new(x, 0.0, start_z);
                let p2 = Point3D::new(x, 0.0, end_z);
                Self::add_line_3d_static(vertices, indices, camera, &p1, &p2, 1.0, color, screen_w, screen_h);
            }

            for zi in 0..=line_count_z {
                let z = start_z + zi as f64 * minor_step;
                let p1 = Point3D::new(start_x, 0.0, z);
                let p2 = Point3D::new(end_x, 0.0, z);
                Self::add_line_3d_static(vertices, indices, camera, &p1, &p2, 1.0, color, screen_w, screen_h);
            }
        }
    }

    #[cfg(test)]
    fn build_3d_axes_static(
        vertices: &mut Vec<Vertex>,
        indices: &mut Vec<u32>,
        camera: &Camera3D,
        screen_w: f32,
        screen_h: f32,
    ) {
        let fov_rad = camera.fov.to_radians();
        let frustum_height = 2.0 * camera.distance * (fov_rad * 0.5).tan();
        let aspect = screen_w / screen_h.max(1.0);
        let axis_len = (frustum_height * aspect.max(1.0) * 1.8) as f64;

        let red = Color::new(0.86, 0.2, 0.2, 1.0);
        let green = Color::new(0.2, 0.7, 0.2, 1.0);
        let blue = Color::new(0.2, 0.2, 0.86, 1.0);

        Self::add_line_3d_static(vertices, indices, camera,
            &Point3D::new(-axis_len, 0.0, 0.0),
            &Point3D::new(axis_len, 0.0, 0.0),
            2.0, red, screen_w, screen_h);

        Self::add_line_3d_static(vertices, indices, camera,
            &Point3D::new(0.0, -axis_len, 0.0),
            &Point3D::new(0.0, axis_len, 0.0),
            2.0, green, screen_w, screen_h);

        Self::add_line_3d_static(vertices, indices, camera,
            &Point3D::new(0.0, 0.0, -axis_len),
            &Point3D::new(0.0, 0.0, axis_len),
            2.0, blue, screen_w, screen_h);
    }

    #[cfg(test)]
    fn add_line_3d_static(
        vertices: &mut Vec<Vertex>,
        indices: &mut Vec<u32>,
        camera: &Camera3D,
        a: &Point3D,
        b: &Point3D,
        width: f32,
        color: Color,
        screen_w: f32,
        screen_h: f32,
    ) {
        if let (Some(sa), Some(sb)) = (camera.project(a, screen_w, screen_h), 
                                       camera.project(b, screen_w, screen_h)) {
            Self::add_line_segment(vertices, indices,
                glam::Vec2::new(sa.0, sa.1),
                glam::Vec2::new(sb.0, sb.1),
                width, color);
        }
    }

    pub fn build_geometry(
        &self,
        document: &Document,
        dark_mode: bool,
    ) -> (Vec<Vertex>, Vec<u32>) {
        let mut vertices = Vec::new();
        let mut indices = Vec::new();

        let view_transform = document.view().clone();

        self.build_grid(&mut vertices, &mut indices, &view_transform, dark_mode);
        self.build_axes(&mut vertices, &mut indices, &view_transform, dark_mode);

        for (_, obj) in document.objects_iter() {
            if !obj.is_visible() {
                continue;
            }
            match obj {
                GeoObject::Point(p) => {
                    let screen = view_transform.world_to_screen(p.position);
                    let size = p.size.max(1.0);
                    Self::add_rect(&mut vertices, &mut indices, screen, size, size, p.color);
                }
                GeoObject::Line(l) => {
                    let a = view_transform.world_to_screen(l.start);
                    let b = view_transform.world_to_screen(l.end);
                    Self::add_line_segment(&mut vertices, &mut indices, a, b, l.width, l.color);
                }
                GeoObject::Circle(c) => {
                    let screen_center = view_transform.world_to_screen(c.center);
                    let radius = (c.radius as f32) * (view_transform.scale as f32);
                    Self::add_circle_stroke(&mut vertices, &mut indices, screen_center, radius, c.width, c.color);
                    if let Some(fill) = c.fill_color {
                        Self::add_circle_fill(&mut vertices, &mut indices, screen_center, radius, fill);
                    }
                }
                GeoObject::Polygon(poly) => {
                    if poly.vertices.len() >= 3 {
                        let screen_verts: Vec<_> = poly.vertices.iter()
                            .map(|v| view_transform.world_to_screen(*v))
                            .collect();
                        if let Some(fill) = poly.fill_color {
                            Self::add_polygon_fill(&mut vertices, &mut indices, &screen_verts, fill);
                        }
                        Self::add_polygon_stroke(&mut vertices, &mut indices, &screen_verts, poly.width, poly.color);
                    }
                }
                GeoObject::Function(fun) => {
                    let world_tl = view_transform.screen_to_world(glam::Vec2::new(0.0, 0.0));
                    let world_br = view_transform.screen_to_world(view_transform.screen_size);
                    let min_x = fun.domain_min.unwrap_or(world_tl.x);
                    let max_x = fun.domain_max.unwrap_or(world_br.x);
                    let screen_width = view_transform.screen_size.x as f64;
                    let steps = ((screen_width * 0.5).max(200.0).min(3000.0)) as usize;
                    let step = (max_x - min_x) / steps as f64;
                    let variables = &document.variables;

                    let samples: Vec<(f64, Option<f64>)> = (0..=steps).into_par_iter().map(|i| {
                        let x = min_x + i as f64 * step;
                        let y = eval_function_with_vars(&fun.expr, x, variables).ok()
                            .filter(|v| v.is_finite() && v.abs() < 1e6);
                        (x, y)
                    }).collect();

                    let mut prev_screen: Option<glam::Vec2> = None;
                    for (x, y_opt) in &samples {
                        if let Some(y) = y_opt {
                            let s = view_transform.world_to_screen(Point2::new(*x, *y));
                            if let Some(prev) = prev_screen {
                                let gap = (s.x - prev.x).abs();
                                if gap < 300.0 {
                                    Self::add_line_segment(&mut vertices, &mut indices, prev, s, fun.width, fun.color);
                                }
                            }
                            prev_screen = Some(s);
                        } else {
                            prev_screen = None;
                        }
                    }
                }
                GeoObject::Ellipse(el) => {
                    let n = 64;
                    let mut pts = Vec::with_capacity(n);
                    for i in 0..n {
                        let t = i as f64 / n as f64 * std::f64::consts::TAU;
                        let x = el.center.x + el.rx * t.cos() * el.angle.cos() - el.ry * t.sin() * el.angle.sin();
                        let y = el.center.y + el.rx * t.cos() * el.angle.sin() + el.ry * t.sin() * el.angle.cos();
                        let s = view_transform.world_to_screen(Point2::new(x, y));
                        pts.push(s);
                    }
                    if let Some(fill) = el.fill_color {
                        Self::add_polygon_fill(&mut vertices, &mut indices, &pts, fill);
                    }
                    Self::add_polygon_stroke(&mut vertices, &mut indices, &pts, el.width, el.color);
                }
                GeoObject::Parabola(pb) => {
                    let steps = 64;
                    let x_range = 10.0 / view_transform.scale as f64;
                    let mut prev: Option<glam::Vec2> = None;
                    for i in 0..=steps {
                        let t = -x_range + 2.0 * x_range * i as f64 / steps as f64;
                        let (sx, sy) = if pb.vertical {
                            (pb.vertex.x + t, pb.vertex.y + t*t / (4.0 * pb.p.max(0.001)))
                        } else {
                            (pb.vertex.x + t*t / (4.0 * pb.p.max(0.001)), pb.vertex.y + t)
                        };
                        let s = view_transform.world_to_screen(Point2::new(sx, sy));
                        if let Some(prev_p) = prev {
                            if (s.x - prev_p.x).abs() < 300.0 {
                                Self::add_line_segment(&mut vertices, &mut indices, prev_p, s, pb.width, pb.color);
                            }
                        }
                        prev = Some(s);
                    }
                }
                GeoObject::Hyperbola(hb) => {
                    let range = 8.0 / view_transform.scale as f64;
                    let n = 64;
                    let mut prev: Option<glam::Vec2> = None;
                    for i in 0..=n {
                        let x = hb.center.x + hb.a + range * i as f64 / n as f64;
                        let dx = x - hb.center.x;
                        if dx > hb.a {
                            let y_off = hb.b * ((dx/hb.a).powi(2) - 1.0).sqrt();
                            for &sign in &[1.0f64, -1.0] {
                                let y = hb.center.y + sign * y_off;
                                let s = view_transform.world_to_screen(Point2::new(x, y));
                                if let Some(prev_p) = prev {
                                    if (s.x - prev_p.x).abs() < 300.0 {
                                        Self::add_line_segment(&mut vertices, &mut indices, prev_p, s, hb.width, hb.color);
                                    }
                                }
                                prev = Some(s);
                            }
                        }
                    }
                    prev = None;
                    for i in 0..=n {
                        let x = hb.center.x - hb.a - range * i as f64 / n as f64;
                        let dx = (x - hb.center.x).abs();
                        if dx > hb.a {
                            let y_off = hb.b * ((dx/hb.a).powi(2) - 1.0).sqrt();
                            for &sign in &[1.0f64, -1.0] {
                                let y = hb.center.y + sign * y_off;
                                let s = view_transform.world_to_screen(Point2::new(x, y));
                                if let Some(prev_p) = prev {
                                    if (s.x - prev_p.x).abs() < 300.0 {
                                        Self::add_line_segment(&mut vertices, &mut indices, prev_p, s, hb.width, hb.color);
                                    }
                                }
                                prev = Some(s);
                            }
                        }
                    }
                }
                _ => {}
            }
        }

        (vertices, indices)
    }

    fn build_grid(
        &self,
        vertices: &mut Vec<Vertex>,
        indices: &mut Vec<u32>,
        view: &ViewTransform,
        dark_mode: bool,
    ) {
        let world_tl = view.screen_to_world(glam::Vec2::new(0.0, 0.0));
        let world_br = view.screen_to_world(view.screen_size);

        let pixels_per_unit = view.scale as f64;
        let target_world_step = 80.0 / pixels_per_unit.max(1e-50);
        let magnitude = target_world_step.log10().floor();
        let base = 10f64.powf(magnitude);
        let factor = target_world_step / base;
        
        let major_step = if factor < 2.0 { 1.0 * base }
            else if factor < 5.0 { 2.0 * base }
            else { 5.0 * base };

        let min_x = (world_tl.x / major_step).floor() as i32 - 1;
        let max_x = (world_br.x / major_step).ceil() as i32 + 1;
        let min_y = (world_br.y / major_step).floor() as i32 - 1;
        let max_y = (world_tl.y / major_step).ceil() as i32 + 1;

        let color = if dark_mode {
            Color::new(0.25, 0.25, 0.25, 1.0)
        } else {
            Color::LIGHT_GRAY
        };

        for xi in min_x..=max_x {
            let x = xi as f64 * major_step;
            let a = view.world_to_screen(Point2::new(x, min_y as f64 * major_step));
            let b = view.world_to_screen(Point2::new(x, max_y as f64 * major_step));
            Self::add_line_segment(vertices, indices, a, b, 1.0, color);
        }

        for yi in min_y..=max_y {
            let y = yi as f64 * major_step;
            let a = view.world_to_screen(Point2::new(min_x as f64 * major_step, y));
            let b = view.world_to_screen(Point2::new(max_x as f64 * major_step, y));
            Self::add_line_segment(vertices, indices, a, b, 1.0, color);
        }
    }

    fn build_axes(
        &self,
        vertices: &mut Vec<Vertex>,
        indices: &mut Vec<u32>,
        view: &ViewTransform,
        dark_mode: bool,
    ) {
        let world_tl = view.screen_to_world(glam::Vec2::new(0.0, 0.0));
        let world_br = view.screen_to_world(view.screen_size);

        let x_axis_y = 0.0f64.clamp(world_br.y, world_tl.y);
        let y_axis_x = 0.0f64.clamp(world_tl.x, world_br.x);

        let axis_color = if dark_mode {
            Color::new(0.7, 0.7, 0.7, 1.0)
        } else {
            Color::BLACK
        };

        let x_axis_a = view.world_to_screen(Point2::new(world_tl.x, x_axis_y));
        let x_axis_b = view.world_to_screen(Point2::new(world_br.x, x_axis_y));
        Self::add_line_segment(vertices, indices, x_axis_a, x_axis_b, 2.0, axis_color);

        let y_axis_a = view.world_to_screen(Point2::new(y_axis_x, world_br.y));
        let y_axis_b = view.world_to_screen(Point2::new(y_axis_x, world_tl.y));
        Self::add_line_segment(vertices, indices, y_axis_a, y_axis_b, 2.0, axis_color);
    }

    fn add_rect(vertices: &mut Vec<Vertex>, indices: &mut Vec<u32>, center: glam::Vec2, w: f32, h: f32, color: Color) {
        let hw = w * 0.5;
        let hh = h * 0.5;
        let base = vertices.len() as u32;
        vertices.push(Vertex::new(center.x - hw, center.y - hh, color));
        vertices.push(Vertex::new(center.x + hw, center.y - hh, color));
        vertices.push(Vertex::new(center.x + hw, center.y + hh, color));
        vertices.push(Vertex::new(center.x - hw, center.y + hh, color));
        indices.extend_from_slice(&[
            base, base + 1, base + 2,
            base, base + 2, base + 3,
        ]);
    }

    fn add_line_segment(vertices: &mut Vec<Vertex>, indices: &mut Vec<u32>, a: glam::Vec2, b: glam::Vec2, width: f32, color: Color) {
        let dir = b - a;
        if dir.length_squared() < 0.0001 {
            return;
        }
        let dir = dir.normalize();
        let perp = glam::Vec2::new(-dir.y, dir.x) * (width * 0.5).max(0.5);

        let base = vertices.len() as u32;
        vertices.push(Vertex::new(a.x + perp.x, a.y + perp.y, color));
        vertices.push(Vertex::new(b.x + perp.x, b.y + perp.y, color));
        vertices.push(Vertex::new(b.x - perp.x, b.y - perp.y, color));
        vertices.push(Vertex::new(a.x - perp.x, a.y - perp.y, color));
        indices.extend_from_slice(&[
            base, base + 1, base + 2,
            base, base + 2, base + 3,
        ]);
    }

    fn add_circle_stroke(vertices: &mut Vec<Vertex>, indices: &mut Vec<u32>, center: glam::Vec2, radius: f32, width: f32, color: Color) {
        let segments = ((radius * 0.5).max(16.0).min(128.0)) as usize;
        let inner_r = (radius - width * 0.5).max(0.0);
        let outer_r = radius + width * 0.5;
        let base = vertices.len() as u32;

        for i in 0..=segments {
            let theta = (i as f32 / segments as f32) * std::f32::consts::TAU;
            let c = theta.cos();
            let s = theta.sin();
            vertices.push(Vertex::new(center.x + inner_r * c, center.y + inner_r * s, color));
            vertices.push(Vertex::new(center.x + outer_r * c, center.y + outer_r * s, color));
        }

        for i in 0..segments {
            let i0 = base + (i * 2) as u32;
            let i1 = i0 + 1;
            let i2 = i0 + 2;
            let i3 = i0 + 3;
            indices.extend_from_slice(&[i0, i2, i1, i1, i2, i3]);
        }
    }

    fn add_circle_fill(vertices: &mut Vec<Vertex>, indices: &mut Vec<u32>, center: glam::Vec2, radius: f32, color: Color) {
        let segments = ((radius * 0.5).max(16.0).min(128.0)) as usize;
        let center_idx = vertices.len() as u32;
        vertices.push(Vertex::new(center.x, center.y, color));

        for i in 0..=segments {
            let theta = (i as f32 / segments as f32) * std::f32::consts::TAU;
            vertices.push(Vertex::new(
                center.x + radius * theta.cos(),
                center.y + radius * theta.sin(),
                color,
            ));
        }

        for i in 0..segments {
            indices.extend_from_slice(&[
                center_idx,
                center_idx + 1 + i as u32,
                center_idx + 2 + i as u32,
            ]);
        }
    }

    fn add_polygon_fill(vertices: &mut Vec<Vertex>, indices: &mut Vec<u32>, pts: &[glam::Vec2], color: Color) {
        if pts.len() < 3 {
            return;
        }
        // Simple fan triangulation (assumes convex or near-convex)
        let base = vertices.len() as u32;
        for p in pts {
            vertices.push(Vertex::new(p.x, p.y, color));
        }
        for i in 1..(pts.len() - 1) {
            indices.extend_from_slice(&[base, base + i as u32, base + (i + 1) as u32]);
        }
    }

    fn add_polygon_stroke(vertices: &mut Vec<Vertex>, indices: &mut Vec<u32>, pts: &[glam::Vec2], width: f32, color: Color) {
        if pts.len() < 2 {
            return;
        }
        for i in 0..pts.len() {
            let a = pts[i];
            let b = pts[(i + 1) % pts.len()];
            Self::add_line_segment(vertices, indices, a, b, width, color);
        }
    }

    pub fn build_3d_geometry(
        &self,
        document: &Document,
        camera: &Camera3D,
        dark_mode: bool,
        screen_w: f32,
        screen_h: f32,
    ) -> (Vec<Vertex>, Vec<u32>) {
        let mut vertices = Vec::new();
        let mut indices = Vec::new();

        self.build_3d_grid(&mut vertices, &mut indices, camera, dark_mode, screen_w, screen_h);
        self.build_3d_axes(&mut vertices, &mut indices, camera, dark_mode, screen_w, screen_h);

        for (_, obj) in document.objects_iter() {
            if !obj.is_visible() {
                continue;
            }
            match obj {
                GeoObject::Point3D(p) => {
                    if let Some(screen_pos) = camera.project(&p.position, screen_w, screen_h) {
                        let size = p.size.max(1.0);
                        Self::add_rect(&mut vertices, &mut indices, 
                            glam::Vec2::new(screen_pos.0, screen_pos.1), 
                            size, size, p.color);
                    }
                }
                GeoObject::Segment3D(l) => {
                    Self::add_line_3d(&mut vertices, &mut indices, camera, 
                        &l.a, &l.b, l.width, l.color, screen_w, screen_h);
                }
                GeoObject::Sphere3D(s) => {
                    Self::add_wireframe_sphere(&mut vertices, &mut indices, camera,
                        &s.center, s.radius, s.width, s.color, screen_w, screen_h);
                }
                GeoObject::Cube3D(c) => {
                    Self::add_wireframe_cube(&mut vertices, &mut indices, camera,
                        &c.center, c.size, c.width, c.color, screen_w, screen_h);
                }
                GeoObject::Pyramid3D(p) => {
                    Self::add_wireframe_pyramid(&mut vertices, &mut indices, camera,
                        &p.base_center, &p.apex, p.base_size, p.width, p.color, screen_w, screen_h);
                }
                GeoObject::Cone3D(co) => {
                    Self::add_wireframe_cone(&mut vertices, &mut indices, camera,
                        &co.base_center, &co.apex, co.radius, co.width, co.color, screen_w, screen_h);
                }
                GeoObject::Cylinder3D(cy) => {
                    Self::add_wireframe_cylinder(&mut vertices, &mut indices, camera,
                        &cy.base_center, &cy.top_center, cy.radius, cy.width, cy.color, screen_w, screen_h);
                }
                GeoObject::Surface3D(su) => {
                    Self::add_surface_mesh(&mut vertices, &mut indices, camera,
                        su, screen_w, screen_h);
                }
                _ => {}
            }
        }

        (vertices, indices)
    }

    fn build_3d_grid(
        &self,
        vertices: &mut Vec<Vertex>,
        indices: &mut Vec<u32>,
        camera: &Camera3D,
        dark_mode: bool,
        screen_w: f32,
        screen_h: f32,
    ) {
        // Calcular densidad de cuadrícula basada en distancia de cámara
        let fov_rad = camera.fov.to_radians();
        let frustum_height = 2.0 * camera.distance * (fov_rad * 0.5).tan();
        let pixels_per_unit = (screen_h / frustum_height) as f64;
        let target_world_step = 80.0 / pixels_per_unit.max(1e-6);
        let magnitude = target_world_step.log10().floor();
        let base = 10f64.powf(magnitude);
        let factor = target_world_step / base;
        
        let major_step = if factor < 2.0 { 1.0 * base }
            else if factor < 5.0 { 2.0 * base }
            else { 5.0 * base };
        let minor_step = major_step / 5.0;

        if minor_step <= 1e-9 { return; }

        // Color adaptativo para dark/light mode
        let minor_color = if dark_mode {
            Color::new(0.20, 0.20, 0.20, 1.0)
        } else {
            Color::new(0.85, 0.85, 0.85, 1.0)
        };
        
        let major_color = if dark_mode {
            Color::new(0.35, 0.35, 0.35, 1.0)
        } else {
            Color::new(0.70, 0.70, 0.70, 1.0)
        };

        // Calcular rango de vista basado en frustum
        let center_x = camera.target.x as f64;
        let center_z = camera.target.z as f64;
        let aspect = screen_w / screen_h.max(1.0);
        let view_range = (frustum_height * aspect.max(1.0) * 1.5) as f64;

        let start_x = ((center_x - view_range) / minor_step).floor() * minor_step;
        let end_x = ((center_x + view_range) / minor_step).ceil() * minor_step;
        let start_z = ((center_z - view_range) / minor_step).floor() * minor_step;
        let end_z = ((center_z + view_range) / minor_step).ceil() * minor_step;

        let line_count_x = ((end_x - start_x) / minor_step).round() as i64;
        let line_count_z = ((end_z - start_z) / minor_step).round() as i64;

        // Limitar número de líneas para performance (pero permitir más que antes)
        let max_lines = 500;
        if line_count_x > max_lines || line_count_z > max_lines {
            // Si hay demasiadas líneas, usar solo major grid
            let major_line_count_x = ((end_x - start_x) / major_step).round() as i64;
            let major_line_count_z = ((end_z - start_z) / major_step).round() as i64;
            
            if major_line_count_x <= max_lines && major_line_count_z <= max_lines {
                // Dibujar solo major grid
                for xi in 0..=major_line_count_x {
                    let x = start_x + xi as f64 * major_step;
                    let p1 = Point3D::new(x, 0.0, start_z);
                    let p2 = Point3D::new(x, 0.0, end_z);
                    Self::add_line_3d(vertices, indices, camera, &p1, &p2, 1.5, major_color, screen_w, screen_h);
                }

                for zi in 0..=major_line_count_z {
                    let z = start_z + zi as f64 * major_step;
                    let p1 = Point3D::new(start_x, 0.0, z);
                    let p2 = Point3D::new(end_x, 0.0, z);
                    Self::add_line_3d(vertices, indices, camera, &p1, &p2, 1.5, major_color, screen_w, screen_h);
                }
            }
            // Si incluso major grid es demasiado, no dibujar nada
        } else {
            // Dibujar grid completo (minor + major)
            for xi in 0..=line_count_x {
                let x = start_x + xi as f64 * minor_step;
                let is_major = ((x / major_step).round() * major_step - x).abs() < minor_step * 0.1;
                let (color, width) = if is_major {
                    (major_color, 1.5)
                } else {
                    (minor_color, 0.8)
                };
                
                let p1 = Point3D::new(x, 0.0, start_z);
                let p2 = Point3D::new(x, 0.0, end_z);
                Self::add_line_3d(vertices, indices, camera, &p1, &p2, width, color, screen_w, screen_h);
            }

            for zi in 0..=line_count_z {
                let z = start_z + zi as f64 * minor_step;
                let is_major = ((z / major_step).round() * major_step - z).abs() < minor_step * 0.1;
                let (color, width) = if is_major {
                    (major_color, 1.5)
                } else {
                    (minor_color, 0.8)
                };
                
                let p1 = Point3D::new(start_x, 0.0, z);
                let p2 = Point3D::new(end_x, 0.0, z);
                Self::add_line_3d(vertices, indices, camera, &p1, &p2, width, color, screen_w, screen_h);
            }
        }
    }

    fn build_3d_axes(
        &self,
        vertices: &mut Vec<Vertex>,
        indices: &mut Vec<u32>,
        camera: &Camera3D,
        _dark_mode: bool,
        screen_w: f32,
        screen_h: f32,
    ) {
        let fov_rad = camera.fov.to_radians();
        let frustum_height = 2.0 * camera.distance * (fov_rad * 0.5).tan();
        let aspect = screen_w / screen_h.max(1.0);
        let axis_len = (frustum_height * aspect.max(1.0) * 1.8) as f64;

        let red = Color::new(0.86, 0.2, 0.2, 1.0);
        let green = Color::new(0.2, 0.7, 0.2, 1.0);
        let blue = Color::new(0.2, 0.2, 0.86, 1.0);

        Self::add_line_3d(vertices, indices, camera,
            &Point3D::new(-axis_len, 0.0, 0.0),
            &Point3D::new(axis_len, 0.0, 0.0),
            2.0, red, screen_w, screen_h);

        Self::add_line_3d(vertices, indices, camera,
            &Point3D::new(0.0, -axis_len, 0.0),
            &Point3D::new(0.0, axis_len, 0.0),
            2.0, green, screen_w, screen_h);

        Self::add_line_3d(vertices, indices, camera,
            &Point3D::new(0.0, 0.0, -axis_len),
            &Point3D::new(0.0, 0.0, axis_len),
            2.0, blue, screen_w, screen_h);
    }

    fn add_line_3d(
        vertices: &mut Vec<Vertex>,
        indices: &mut Vec<u32>,
        camera: &Camera3D,
        a: &Point3D,
        b: &Point3D,
        width: f32,
        color: Color,
        screen_w: f32,
        screen_h: f32,
    ) {
        // Proyección con clipping en near plane (similar a project_segment en render_3d.rs)
        let mvp = camera.mvp();
        let mut clip_a = mvp * a.to_vec3().extend(1.0);
        let mut clip_b = mvp * b.to_vec3().extend(1.0);

        let near = camera.near;

        // Si ambos puntos están detrás del near plane, no dibujar
        if clip_a.w < near && clip_b.w < near {
            return;
        }

        // Clipping: si un punto está detrás del near plane, interpolar
        if clip_a.w < near {
            let t = (near - clip_a.w) / (clip_b.w - clip_a.w);
            clip_a = clip_a + t * (clip_b - clip_a);
        } else if clip_b.w < near {
            let t = (near - clip_b.w) / (clip_a.w - clip_b.w);
            clip_b = clip_b + t * (clip_a - clip_b);
        }

        // Convertir a NDC (Normalized Device Coordinates)
        let ndc_ax = clip_a.x / clip_a.w;
        let ndc_ay = clip_a.y / clip_a.w;
        let ndc_bx = clip_b.x / clip_b.w;
        let ndc_by = clip_b.y / clip_b.w;

        // Frustum culling: si ambos puntos están fuera del mismo lado, no dibujar
        if ndc_ax.abs() > 2.0 && ndc_bx.abs() > 2.0 && ndc_ax.signum() == ndc_bx.signum() {
            return;
        }
        if ndc_ay.abs() > 2.0 && ndc_by.abs() > 2.0 && ndc_ay.signum() == ndc_by.signum() {
            return;
        }

        // Convertir NDC a screen coordinates
        let sa = glam::Vec2::new(
            (ndc_ax + 1.0) * 0.5 * screen_w,
            (1.0 - ndc_ay) * 0.5 * screen_h,
        );
        let sb = glam::Vec2::new(
            (ndc_bx + 1.0) * 0.5 * screen_w,
            (1.0 - ndc_by) * 0.5 * screen_h,
        );

        Self::add_line_segment(vertices, indices, sa, sb, width, color);
    }

    fn add_wireframe_sphere(
        vertices: &mut Vec<Vertex>,
        indices: &mut Vec<u32>,
        camera: &Camera3D,
        center: &Point3D,
        radius: f64,
        width: f32,
        color: Color,
        screen_w: f32,
        screen_h: f32,
    ) {
        let segments = 32;
        let center_vec = center.to_vec3();
        let r = radius as f32;

        for &(u, v) in &[(glam::Vec3::X, glam::Vec3::Y), 
                         (glam::Vec3::X, glam::Vec3::Z), 
                         (glam::Vec3::Y, glam::Vec3::Z)] {
            let pts = Camera3D::circle_points(center_vec, u, v, r, segments);
            for i in 0..pts.len() {
                let j = (i + 1) % pts.len();
                let p1 = Point3D::from_vec3(pts[i]);
                let p2 = Point3D::from_vec3(pts[j]);
                Self::add_line_3d(vertices, indices, camera, &p1, &p2, width, color, screen_w, screen_h);
            }
        }
    }

    fn add_wireframe_cube(
        vertices: &mut Vec<Vertex>,
        indices: &mut Vec<u32>,
        camera: &Camera3D,
        center: &Point3D,
        size: f64,
        width: f32,
        color: Color,
        screen_w: f32,
        screen_h: f32,
    ) {
        let h = size * 0.5;
        let x = center.x;
        let y = center.y;
        let z = center.z;

        let corners = [
            Point3D::new(x-h, y-h, z-h), Point3D::new(x+h, y-h, z-h),
            Point3D::new(x+h, y+h, z-h), Point3D::new(x-h, y+h, z-h),
            Point3D::new(x-h, y-h, z+h), Point3D::new(x+h, y-h, z+h),
            Point3D::new(x+h, y+h, z+h), Point3D::new(x-h, y+h, z+h),
        ];

        let edges = [
            (0,1), (1,2), (2,3), (3,0),
            (4,5), (5,6), (6,7), (7,4),
            (0,4), (1,5), (2,6), (3,7),
        ];

        for &(i, j) in &edges {
            Self::add_line_3d(vertices, indices, camera, &corners[i], &corners[j], width, color, screen_w, screen_h);
        }
    }

    fn add_wireframe_pyramid(
        vertices: &mut Vec<Vertex>,
        indices: &mut Vec<u32>,
        camera: &Camera3D,
        base_center: &Point3D,
        apex: &Point3D,
        base_size: f64,
        width: f32,
        color: Color,
        screen_w: f32,
        screen_h: f32,
    ) {
        let h = base_size * 0.5;
        let (cx, cy, cz) = (base_center.x, base_center.y, base_center.z);

        let base_corners = [
            Point3D::new(cx-h, cy, cz-h),
            Point3D::new(cx+h, cy, cz-h),
            Point3D::new(cx+h, cy, cz+h),
            Point3D::new(cx-h, cy, cz+h),
        ];

        for i in 0..4 {
            let j = (i + 1) % 4;
            Self::add_line_3d(vertices, indices, camera, &base_corners[i], &base_corners[j], width, color, screen_w, screen_h);
            Self::add_line_3d(vertices, indices, camera, &base_corners[i], apex, width, color, screen_w, screen_h);
        }
    }

    fn add_wireframe_cone(
        vertices: &mut Vec<Vertex>,
        indices: &mut Vec<u32>,
        camera: &Camera3D,
        base_center: &Point3D,
        apex: &Point3D,
        radius: f64,
        width: f32,
        color: Color,
        screen_w: f32,
        screen_h: f32,
    ) {
        let segments = 32;
        let base_vec = base_center.to_vec3();
        let r = radius as f32;

        let pts = Camera3D::circle_points(base_vec, glam::Vec3::X, glam::Vec3::Z, r, segments);
        
        for i in 0..pts.len() {
            let j = (i + 1) % pts.len();
            let p1 = Point3D::from_vec3(pts[i]);
            let p2 = Point3D::from_vec3(pts[j]);
            Self::add_line_3d(vertices, indices, camera, &p1, &p2, width, color, screen_w, screen_h);
            
            if i % 4 == 0 {
                Self::add_line_3d(vertices, indices, camera, &p1, apex, width, color, screen_w, screen_h);
            }
        }
    }

    fn add_wireframe_cylinder(
        vertices: &mut Vec<Vertex>,
        indices: &mut Vec<u32>,
        camera: &Camera3D,
        base_center: &Point3D,
        top_center: &Point3D,
        radius: f64,
        width: f32,
        color: Color,
        screen_w: f32,
        screen_h: f32,
    ) {
        let segments = 32;
        let base_vec = base_center.to_vec3();
        let top_vec = top_center.to_vec3();
        let r = radius as f32;

        let base_pts = Camera3D::circle_points(base_vec, glam::Vec3::X, glam::Vec3::Z, r, segments);
        let top_pts = Camera3D::circle_points(top_vec, glam::Vec3::X, glam::Vec3::Z, r, segments);
        
        for i in 0..base_pts.len() {
            let j = (i + 1) % base_pts.len();
            let bp1 = Point3D::from_vec3(base_pts[i]);
            let bp2 = Point3D::from_vec3(base_pts[j]);
            let tp1 = Point3D::from_vec3(top_pts[i]);
            let tp2 = Point3D::from_vec3(top_pts[j]);
            
            Self::add_line_3d(vertices, indices, camera, &bp1, &bp2, width, color, screen_w, screen_h);
            Self::add_line_3d(vertices, indices, camera, &tp1, &tp2, width, color, screen_w, screen_h);
            
            if i % 8 == 0 {
                Self::add_line_3d(vertices, indices, camera, &bp1, &tp1, width, color, screen_w, screen_h);
            }
        }
    }

    fn add_surface_mesh(
        vertices: &mut Vec<Vertex>,
        indices: &mut Vec<u32>,
        camera: &Camera3D,
        surface: &grafito_core::Surface3DObj,
        screen_w: f32,
        screen_h: f32,
    ) {
        let steps = 20;
        let x_min = surface.x_min;
        let x_max = surface.x_max;
        let y_min = surface.y_min;
        let y_max = surface.y_max;
        let x_step = (x_max - x_min) / steps as f64;
        let y_step = (y_max - y_min) / steps as f64;

        let mut grid = vec![vec![None; steps + 1]; steps + 1];

        for i in 0..=steps {
            for j in 0..=steps {
                let x = x_min + i as f64 * x_step;
                let y = y_min + j as f64 * y_step;
                let vars = vec![("x".to_string(), x), ("y".to_string(), y)];
                if let Ok(z) = grafito_geometry::expr::evaluate(&surface.expr, &vars) {
                    if z.is_finite() && z.abs() < 100.0 {
                        grid[i][j] = Some(Point3D::new(x, z, y));
                    }
                }
            }
        }

        for i in 0..=steps {
            for j in 0..=steps {
                if let Some(p) = grid[i][j] {
                    if i < steps {
                        if let Some(p_right) = grid[i+1][j] {
                            Self::add_line_3d(vertices, indices, camera, &p, &p_right, 
                                surface.width, surface.color, screen_w, screen_h);
                        }
                    }
                    if j < steps {
                        if let Some(p_down) = grid[i][j+1] {
                            Self::add_line_3d(vertices, indices, camera, &p, &p_down, 
                                surface.width, surface.color, screen_w, screen_h);
                        }
                    }
                }
            }
        }
    }
}
