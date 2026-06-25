//! Grafito Render — Renderizador 2D/3D acelerado por GPU con wgpu.
//!
//! Este crate convierte un [`Document`] en geometría
//! lista para GPU, gestiona los pipelines de cómputo (`function_compute`,
//! `implicit_compute`, `parametric_compute`) y proporciona utilidades de
//! iluminación y vértices.
//!
//! # Ejemplo mínimo
//!
//! ```
//! use grafito_render::Vertex;
//! use grafito_geometry::Color;
//!
//! let v = Vertex::new(0.0, 0.0, Color::BLACK);
//! assert_eq!(v.position, [0.0, 0.0, 0.0]);
//! ```
#![allow(
    clippy::too_many_arguments,
    clippy::needless_range_loop,
    dead_code,
    clippy::manual_clamp
)]

use grafito_core::{Document, GeoObject};
use grafito_geometry::conformal::algebraic_mappings::ConformalMap;
use grafito_geometry::{Camera3D, Color, Point2, Point3D, ViewTransform};
use wgpu::util::DeviceExt;

pub mod fill_compute;
pub mod function_compute;
pub mod implicit_compute;
pub mod parametric_compute;
pub mod vector_compute;

#[cfg(test)]
mod tests;

/// Transforma segmentos independientes por un mapa conforme sin crear líneas
/// puente entre segmentos consecutivos del marching squares.
pub fn transform_complex_mapping_segments(
    map: ConformalMap,
    segments: &[(Point2, Point2)],
    subdivisions: usize,
) -> Vec<(Point2, Point2)> {
    let subdivisions = subdivisions.max(1);
    let mut strokes = Vec::new();
    for (a, b) in segments {
        let mut prev: Option<Point2> = None;
        for i in 0..=subdivisions {
            let t = i as f64 / subdivisions as f64;
            let z = num_complex::Complex64::new(a.x + t * (b.x - a.x), a.y + t * (b.y - a.y));
            let current = map.apply(z).and_then(|w| {
                if w.re.is_finite() && w.im.is_finite() {
                    Some(Point2::new(w.re, w.im))
                } else {
                    None
                }
            });
            if let (Some(prev), Some(current)) = (prev, current) {
                strokes.push((prev, current));
            }
            prev = current;
        }
    }
    strokes
}

/// Cálculo simple de iluminación para objetos 3D
pub fn calculate_lighting(base_color: Color, normal: glam::Vec3, light_dir: glam::Vec3) -> Color {
    let ambient = 0.45;
    let diffuse = 0.65;

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

/// Un vértice simple con posición y color.
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
    pub implicit_compute: Option<crate::implicit_compute::ImplicitComputePipeline>,
    pub function_compute: Option<crate::function_compute::FunctionComputePipeline>,
    pub parametric_compute: Option<crate::parametric_compute::ParametricComputePipeline>,
    pub vector_compute: Option<crate::vector_compute::VectorComputePipeline>,
    pub fill_compute: Option<crate::fill_compute::FillComputePipeline>,
}

fn hsv_to_rgb(h: f32, s: f32, v: f32) -> Color {
    let c = v * s;
    let x = c * (1.0 - ((h * 6.0) % 2.0 - 1.0).abs());
    let m = v - c;
    let (r, g, b) = match (h * 6.0) as i32 % 6 {
        0 => (c, x, 0.0),
        1 => (x, c, 0.0),
        2 => (0.0, c, x),
        3 => (0.0, x, c),
        4 => (x, 0.0, c),
        _ => (c, 0.0, x),
    };
    Color::new(r + m, g + m, b + m, 1.0)
}

impl Renderer {
    pub fn new(
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        target_format: wgpu::TextureFormat,
        sample_count: u32,
    ) -> Self {
        let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("Grafito Shader"),
            source: wgpu::ShaderSource::Wgsl(include_str!("shader.wgsl").into()),
        });

        let mvp_bind_group_layout =
            device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
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

        let multisample = if sample_count > 1 {
            wgpu::MultisampleState {
                count: sample_count,
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

        let implicit_compute = Some(crate::implicit_compute::ImplicitComputePipeline::new(
            device, queue, 1024,
        ));
        let function_compute = Some(crate::function_compute::FunctionComputePipeline::new(
            device, queue, 10000,
        ));
        let parametric_compute = Some(crate::parametric_compute::ParametricComputePipeline::new(
            device, queue, 4000, 128,
        ));
        let vector_compute = Some(crate::vector_compute::VectorComputePipeline::new(
            device, queue, 128,
        ));
        let fill_compute = Some(crate::fill_compute::FillComputePipeline::new(device, queue));

        Self {
            pipeline,
            pipeline_3d,
            mvp_bind_group_layout,
            mvp_buffer,
            mvp_bind_group,
            implicit_compute,
            function_compute,
            parametric_compute,
            vector_compute,
            fill_compute,
        }
    }

    pub fn update_mvp(&self, queue: &wgpu::Queue, mvp: glam::Mat4) {
        queue.write_buffer(
            &self.mvp_buffer,
            0,
            bytemuck::cast_slice(&mvp.to_cols_array()),
        );
    }

    pub fn build_geometry_static(
        document: &Document,
        view: &ViewTransform,
        dark_mode: bool,
        include_overlays: bool,
    ) -> (Vec<Vertex>, Vec<u32>) {
        let obj_count = document.object_count().max(1);
        let mut vertices = Vec::with_capacity(obj_count * 256);
        let mut indices = Vec::with_capacity(obj_count * 384);

        if include_overlays {
            Self::build_grid_static(&mut vertices, &mut indices, view, dark_mode);
            Self::build_axes_static(&mut vertices, &mut indices, view, dark_mode);
        }

        for (_, obj) in document.objects_iter() {
            if !obj.is_visible() {
                continue;
            }
            match obj {
                GeoObject::Point(p) if include_overlays => {
                    let screen = view.world_to_screen(p.position);
                    let size = p.size.max(1.0);
                    Self::add_rect(&mut vertices, &mut indices, screen, size, size, p.color);
                }
                GeoObject::Line(l) => {
                    let start = Point2::new(
                        document.resolve_expr(&l.start_x_expr, l.start.x),
                        document.resolve_expr(&l.start_y_expr, l.start.y),
                    );
                    let end = Point2::new(
                        document.resolve_expr(&l.end_x_expr, l.end.x),
                        document.resolve_expr(&l.end_y_expr, l.end.y),
                    );
                    // Defensa nuclear: no dibujar líneas verticales/
                    // horizontales puras (cruzan el canvas de borde a
                    // borde y son visualmente molestas).
                    let world_tl = view.screen_to_world(glam::Vec2::new(0.0, 0.0));
                    let world_br = view.screen_to_world(view.screen_size);
                    let view_bounds = grafito_geometry::AABB::new(
                        Point2::new(world_tl.x.min(world_br.x), world_tl.y.min(world_br.y)),
                        Point2::new(world_tl.x.max(world_br.x), world_tl.y.max(world_br.y)),
                    );
                    let clipped = match l.kind {
                        grafito_core::LineKind::Segment => {
                            grafito_geometry::clip_segment_to_rect(start, end, view_bounds)
                        }
                        grafito_core::LineKind::Ray => {
                            grafito_geometry::clip_ray_to_rect(start, end, view_bounds)
                        }
                        grafito_core::LineKind::Line => {
                            grafito_geometry::clip_line_to_rect(start, end, view_bounds)
                        }
                    };
                    if let Some((clip_start, clip_end)) = clipped {
                        let a = view.world_to_screen(clip_start);
                        let b = view.world_to_screen(clip_end);
                        Self::add_line_segment(&mut vertices, &mut indices, a, b, l.width, l.color);
                    }
                }
                GeoObject::Circle(c) => {
                    let center = view.world_to_screen(c.center);
                    let radius = (c.radius as f32) * view.scale as f32;
                    Self::add_circle_stroke(
                        &mut vertices,
                        &mut indices,
                        center,
                        radius,
                        c.width,
                        c.color,
                    );
                    if let Some(fill) = c.fill_color {
                        Self::add_circle_fill(&mut vertices, &mut indices, center, radius, fill);
                    }
                }
                GeoObject::Polygon(poly) if poly.vertices.len() >= 3 => {
                    let mut screen_verts = Vec::with_capacity(poly.vertices.len());
                    for (i, v) in poly.vertices.iter().enumerate() {
                        let x = document.resolve_expr(poly.x_exprs.get(i).unwrap_or(&None), v.x);
                        let y = document.resolve_expr(poly.y_exprs.get(i).unwrap_or(&None), v.y);
                        screen_verts.push(view.world_to_screen(Point2::new(x, y)));
                    }
                    if let Some(fill) = poly.fill_color {
                        Self::add_polygon_fill(&mut vertices, &mut indices, &screen_verts, fill);
                    }
                    Self::add_polygon_stroke(
                        &mut vertices,
                        &mut indices,
                        &screen_verts,
                        poly.width,
                        poly.color,
                    );
                }
                GeoObject::Pencil(pencil) if pencil.points.len() >= 2 => {
                    Self::draw_pencil_in_view_static(&mut vertices, &mut indices, view, pencil);
                }
                GeoObject::ComplexGrid(cg) => {
                    Self::add_complex_grid_geometry(
                        &mut vertices,
                        &mut indices,
                        document,
                        view,
                        cg,
                    );
                }
                _ => {}
            }
        }

        (vertices, indices)
    }

    fn add_complex_grid_geometry(
        vertices: &mut Vec<Vertex>,
        indices: &mut Vec<u32>,
        document: &Document,
        view_transform: &ViewTransform,
        cg: &grafito_core::ComplexGridObj,
    ) {
        let base_symbol = document.complex_base_symbol.as_str();
        match cg.render_mode {
            1 => {
                let res = cg.density.clamp(50, 500);
                let dx = (cg.x_max - cg.x_min) / res as f64;
                let dy = (cg.y_max - cg.y_min) / res as f64;
                if let Ok(parsed) = grafito_geometry::complex_expr::parse(&cg.expr) {
                    let mut vars = std::collections::HashMap::new();
                    for (k, v) in &document.variables {
                        vars.insert(k.clone(), num_complex::Complex64::new(*v, 0.0));
                    }
                    for i in 0..res {
                        let x = cg.x_min + i as f64 * dx;
                        for j in 0..res {
                            let y = cg.y_min + j as f64 * dy;
                            vars.insert(base_symbol.to_string(), num_complex::Complex64::new(x, y));
                            if let Ok(fz) = parsed.eval(&vars) {
                                if fz.re.is_finite() && fz.im.is_finite() {
                                    let hue = (fz.arg() / std::f64::consts::TAU + 0.5) % 1.0;
                                    let val = (fz.norm() / (fz.norm() + 1.0)).max(0.2);
                                    let color = hsv_to_rgb(hue as f32, 0.8, val as f32);
                                    let sx = view_transform.world_to_screen(Point2::new(x, y));
                                    Self::add_rect(
                                        vertices,
                                        indices,
                                        sx,
                                        (dx * view_transform.scale).max(1.0) as f32,
                                        (dy * view_transform.scale).max(1.0) as f32,
                                        color,
                                    );
                                }
                            }
                        }
                    }
                }
            }
            2 => {
                let res = cg.density.clamp(50, 500);
                let dx = (cg.x_max - cg.x_min) / res as f64;
                let dy = (cg.y_max - cg.y_min) / res as f64;
                if let Ok(ast) = grafito_geometry::expr::prepare_function_ast(
                    &cg.expr,
                    &document.variables,
                    &["x", "y"],
                ) {
                    for i in 0..res {
                        let x = cg.x_min + i as f64 * dx;
                        for j in 0..res {
                            let y = cg.y_min + j as f64 * dy;
                            let val = ast.eval_2d("x", x, "y", y);
                            if val.is_finite() {
                                let t = (val.atan() / std::f64::consts::FRAC_PI_2).clamp(-1.0, 1.0);
                                let hue = 0.66 * (1.0 - (t + 1.0) * 0.5);
                                let color = hsv_to_rgb(hue as f32, 0.85, 0.95);
                                let sx = view_transform.world_to_screen(Point2::new(x, y));
                                Self::add_rect(
                                    vertices,
                                    indices,
                                    sx,
                                    (dx * view_transform.scale).max(1.0) as f32,
                                    (dy * view_transform.scale).max(1.0) as f32,
                                    color,
                                );
                            }
                        }
                    }
                }
            }
            _ => {
                let grid_lines = cg.density.max(1);
                let dx = (cg.x_max - cg.x_min) / grid_lines as f64;
                let dy = (cg.y_max - cg.y_min) / grid_lines as f64;
                if let Ok(parsed) = grafito_geometry::complex_expr::parse(&cg.expr) {
                    let mut vars = std::collections::HashMap::new();
                    for (k, v) in &document.variables {
                        vars.insert(k.clone(), num_complex::Complex64::new(*v, 0.0));
                    }
                    for j in 0..=grid_lines {
                        let y = cg.y_min + j as f64 * dy;
                        let mut prev: Option<glam::Vec2> = None;
                        for i in 0..=grid_lines * 4 {
                            let x = cg.x_min + i as f64 * dx / 4.0;
                            vars.insert(base_symbol.to_string(), num_complex::Complex64::new(x, y));
                            prev = Self::add_complex_grid_sample(
                                vertices,
                                indices,
                                view_transform,
                                cg,
                                &parsed,
                                &vars,
                                prev,
                            );
                        }
                    }
                    for i in 0..=grid_lines {
                        let x = cg.x_min + i as f64 * dx;
                        let mut prev: Option<glam::Vec2> = None;
                        for j in 0..=grid_lines * 4 {
                            let y = cg.y_min + j as f64 * dy / 4.0;
                            vars.insert(base_symbol.to_string(), num_complex::Complex64::new(x, y));
                            prev = Self::add_complex_grid_sample(
                                vertices,
                                indices,
                                view_transform,
                                cg,
                                &parsed,
                                &vars,
                                prev,
                            );
                        }
                    }
                }
            }
        }
    }

    fn add_complex_grid_sample(
        vertices: &mut Vec<Vertex>,
        indices: &mut Vec<u32>,
        view_transform: &ViewTransform,
        cg: &grafito_core::ComplexGridObj,
        parsed: &grafito_geometry::complex_expr::ComplexExpr,
        vars: &std::collections::HashMap<String, num_complex::Complex64>,
        prev: Option<glam::Vec2>,
    ) -> Option<glam::Vec2> {
        let Ok(result) = parsed.eval(vars) else {
            return None;
        };
        if !result.re.is_finite()
            || !result.im.is_finite()
            || result.re.abs() >= 1e6
            || result.im.abs() >= 1e6
        {
            return None;
        }

        let screen = view_transform.world_to_screen(Point2::new(result.re, result.im));
        if let Some(prev_screen) = prev {
            Self::add_line_segment(vertices, indices, prev_screen, screen, 1.0, cg.color);
        }
        Some(screen)
    }

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

    pub fn build_3d_geometry_static(
        document: &Document,
        camera: &Camera3D,
        dark_mode: bool,
        screen_w: f32,
        screen_h: f32,
    ) -> (Vec<Vertex>, Vec<u32>) {
        let obj_count = document.object_count().max(1);
        let mut vertices = Vec::with_capacity(obj_count * 256);
        let mut indices = Vec::with_capacity(obj_count * 384);

        Self::build_3d_grid_static(
            &mut vertices,
            &mut indices,
            camera,
            dark_mode,
            screen_w,
            screen_h,
        );
        Self::build_3d_axes_static(&mut vertices, &mut indices, camera, screen_w, screen_h);

        for (_, obj) in document.objects_iter() {
            if !obj.is_visible() {
                continue;
            }
            match obj {
                GeoObject::Point3D(p) => {
                    if let Some(screen_pos) = camera.project(&p.position, screen_w, screen_h) {
                        let size = p.size.max(1.0);
                        Self::add_rect(
                            &mut vertices,
                            &mut indices,
                            glam::Vec2::new(screen_pos.0, screen_pos.1),
                            size,
                            size,
                            p.color,
                        );
                    }
                }
                GeoObject::Segment3D(s) => {
                    if let (Some(a), Some(b)) = (
                        camera.project(&s.a, screen_w, screen_h),
                        camera.project(&s.b, screen_w, screen_h),
                    ) {
                        Self::add_line_segment(
                            &mut vertices,
                            &mut indices,
                            glam::Vec2::new(a.0, a.1),
                            glam::Vec2::new(b.0, b.1),
                            s.width,
                            s.color,
                        );
                    }
                }
                GeoObject::Sphere3D(s) => {
                    Self::add_wireframe_sphere(
                        &mut vertices,
                        &mut indices,
                        camera,
                        &s.center,
                        s.radius,
                        s.width,
                        s.color,
                        screen_w,
                        screen_h,
                    );
                }
                GeoObject::Cube3D(c) => {
                    Self::add_wireframe_cube(
                        &mut vertices,
                        &mut indices,
                        camera,
                        &c.center,
                        c.size,
                        c.width,
                        c.color,
                        screen_w,
                        screen_h,
                    );
                }
                _ => {}
            }
        }

        (vertices, indices)
    }

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

        let major_step = if factor < 2.0 {
            1.0 * base
        } else if factor < 5.0 {
            2.0 * base
        } else {
            5.0 * base
        };
        let minor_step = major_step / 5.0;

        if minor_step <= 1e-9 {
            return;
        }

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
                Self::add_line_3d_static(
                    vertices, indices, camera, &p1, &p2, 1.0, color, screen_w, screen_h,
                );
            }

            for zi in 0..=line_count_z {
                let z = start_z + zi as f64 * minor_step;
                let p1 = Point3D::new(start_x, 0.0, z);
                let p2 = Point3D::new(end_x, 0.0, z);
                Self::add_line_3d_static(
                    vertices, indices, camera, &p1, &p2, 1.0, color, screen_w, screen_h,
                );
            }
        }
    }

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

        Self::add_line_3d_static(
            vertices,
            indices,
            camera,
            &Point3D::new(-axis_len, 0.0, 0.0),
            &Point3D::new(axis_len, 0.0, 0.0),
            2.0,
            red,
            screen_w,
            screen_h,
        );

        Self::add_line_3d_static(
            vertices,
            indices,
            camera,
            &Point3D::new(0.0, -axis_len, 0.0),
            &Point3D::new(0.0, axis_len, 0.0),
            2.0,
            green,
            screen_w,
            screen_h,
        );

        Self::add_line_3d_static(
            vertices,
            indices,
            camera,
            &Point3D::new(0.0, 0.0, -axis_len),
            &Point3D::new(0.0, 0.0, axis_len),
            2.0,
            blue,
            screen_w,
            screen_h,
        );
    }

    #[allow(clippy::too_many_arguments)]
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
        if let (Some(sa), Some(sb)) = (
            camera.project(a, screen_w, screen_h),
            camera.project(b, screen_w, screen_h),
        ) {
            Self::add_line_segment(
                vertices,
                indices,
                glam::Vec2::new(sa.0, sa.1),
                glam::Vec2::new(sb.0, sb.1),
                width,
                color,
            );
        }
    }

    pub fn build_geometry(
        &self,
        document: &Document,
        dark_mode: bool,
        include_overlays: bool,
    ) -> (Vec<Vertex>, Vec<u32>) {
        let obj_count = document.object_count().max(1);
        let mut vertices = Vec::with_capacity(obj_count * 256);
        let mut indices = Vec::with_capacity(obj_count * 384);

        let view_transform = *document.view();

        if include_overlays {
            self.build_grid(&mut vertices, &mut indices, &view_transform, dark_mode);
            self.build_axes(&mut vertices, &mut indices, &view_transform, dark_mode);
        }

        for (_, obj) in document.objects_iter() {
            if !obj.is_visible() {
                continue;
            }
            match obj {
                GeoObject::Point(p) if include_overlays => {
                    let screen = view_transform.world_to_screen(p.position);
                    let size = p.size.max(1.0);
                    Self::add_rect(&mut vertices, &mut indices, screen, size, size, p.color);
                }
                GeoObject::Line(l) => {
                    let start = Point2::new(
                        document.resolve_expr(&l.start_x_expr, l.start.x),
                        document.resolve_expr(&l.start_y_expr, l.start.y),
                    );
                    let end = Point2::new(
                        document.resolve_expr(&l.end_x_expr, l.end.x),
                        document.resolve_expr(&l.end_y_expr, l.end.y),
                    );
                    let world_tl = view_transform.screen_to_world(glam::Vec2::new(0.0, 0.0));
                    let world_br = view_transform.screen_to_world(view_transform.screen_size);
                    let view_bounds = grafito_geometry::AABB::new(
                        Point2::new(world_tl.x.min(world_br.x), world_tl.y.min(world_br.y)),
                        Point2::new(world_tl.x.max(world_br.x), world_tl.y.max(world_br.y)),
                    );
                    let clipped = match l.kind {
                        grafito_core::LineKind::Segment => {
                            grafito_geometry::clip_segment_to_rect(start, end, view_bounds)
                        }
                        grafito_core::LineKind::Ray => {
                            grafito_geometry::clip_ray_to_rect(start, end, view_bounds)
                        }
                        grafito_core::LineKind::Line => {
                            grafito_geometry::clip_line_to_rect(start, end, view_bounds)
                        }
                    };
                    if let Some((clip_start, clip_end)) = clipped {
                        let a = view_transform.world_to_screen(clip_start);
                        let b = view_transform.world_to_screen(clip_end);
                        Self::add_line_segment(&mut vertices, &mut indices, a, b, l.width, l.color);
                    }
                }
                GeoObject::Circle(c) => {
                    let screen_center = view_transform.world_to_screen(c.center);
                    let radius = (c.radius as f32) * (view_transform.scale as f32);
                    Self::add_circle_stroke(
                        &mut vertices,
                        &mut indices,
                        screen_center,
                        radius,
                        c.width,
                        c.color,
                    );
                    if let Some(fill) = c.fill_color {
                        Self::add_circle_fill(
                            &mut vertices,
                            &mut indices,
                            screen_center,
                            radius,
                            fill,
                        );
                    }
                }
                GeoObject::Polygon(poly) if poly.vertices.len() >= 3 => {
                    let mut screen_verts = Vec::with_capacity(poly.vertices.len());
                    for (i, v) in poly.vertices.iter().enumerate() {
                        let x = document.resolve_expr(poly.x_exprs.get(i).unwrap_or(&None), v.x);
                        let y = document.resolve_expr(poly.y_exprs.get(i).unwrap_or(&None), v.y);
                        screen_verts.push(view_transform.world_to_screen(Point2::new(x, y)));
                    }
                    if let Some(fill) = poly.fill_color {
                        Self::add_polygon_fill(&mut vertices, &mut indices, &screen_verts, fill);
                    }
                    Self::add_polygon_stroke(
                        &mut vertices,
                        &mut indices,
                        &screen_verts,
                        poly.width,
                        poly.color,
                    );
                }
                GeoObject::Pencil(pencil) if pencil.points.len() >= 2 => {
                    Self::draw_pencil_in_view_transform(
                        &mut vertices,
                        &mut indices,
                        &view_transform,
                        pencil,
                    );
                }
                GeoObject::Function(fun) => {
                    let world_tl = view_transform.screen_to_world(glam::Vec2::new(0.0, 0.0));
                    let world_br = view_transform.screen_to_world(view_transform.screen_size);
                    let min_x = document
                        .resolve_expr(&fun.domain_min_expr, fun.domain_min.unwrap_or(world_tl.x));
                    let max_x = document
                        .resolve_expr(&fun.domain_max_expr, fun.domain_max.unwrap_or(world_br.x));
                    let domain = (min_x, max_x);
                    let grid_size =
                        grafito_core::function_sampling::recommended_grid_size_for_quality(
                            view_transform.screen_size.x,
                            document.render_quality,
                        );
                    let samples = grafito_core::function_sampling::samples_or_compute(
                        fun,
                        domain,
                        grid_size,
                        &document.variables,
                    );

                    let mut prev_screen: Option<glam::Vec2> = None;
                    for (x, y_opt) in samples.iter() {
                        if let Some(y) = y_opt {
                            let s = view_transform.world_to_screen(Point2::new(*x, *y));
                            if let Some(prev) = prev_screen {
                                let gap = (s.x - prev.x).abs();
                                if gap < 300.0 {
                                    Self::add_line_segment(
                                        &mut vertices,
                                        &mut indices,
                                        prev,
                                        s,
                                        fun.width,
                                        fun.color,
                                    );
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
                        let x = el.center.x + el.rx * t.cos() * el.angle.cos()
                            - el.ry * t.sin() * el.angle.sin();
                        let y = el.center.y
                            + el.rx * t.cos() * el.angle.sin()
                            + el.ry * t.sin() * el.angle.cos();
                        let s = view_transform.world_to_screen(Point2::new(x, y));
                        pts.push(s);
                    }
                    if let Some(fill) = el.fill_color {
                        Self::add_polygon_fill(&mut vertices, &mut indices, &pts, fill);
                    }
                    Self::add_polygon_stroke(&mut vertices, &mut indices, &pts, el.width, el.color);
                }
                GeoObject::Parabola(pb) => {
                    let steps = 128;
                    let range = (20.0 / view_transform.scale).clamp(0.1, 500.0);
                    let p_safe = pb.p.max(0.001);
                    let cos_a = pb.angle.cos();
                    let sin_a = pb.angle.sin();
                    let mut prev: Option<glam::Vec2> = None;
                    for i in 0..=steps {
                        let t = -range + 2.0 * range * i as f64 / steps as f64;
                        let lx = t;
                        let ly = t * t / (4.0 * p_safe);
                        let wx = pb.vertex.x + lx * cos_a - ly * sin_a;
                        let wy = pb.vertex.y + lx * sin_a + ly * cos_a;
                        if wx.is_finite() && wy.is_finite() {
                            let s = view_transform.world_to_screen(Point2::new(wx, wy));
                            if let Some(prev_p) = prev {
                                if (s.x - prev_p.x).abs() < 300.0 {
                                    Self::add_line_segment(
                                        &mut vertices,
                                        &mut indices,
                                        prev_p,
                                        s,
                                        pb.width,
                                        pb.color,
                                    );
                                }
                            }
                            prev = Some(s);
                        }
                    }
                }
                GeoObject::Hyperbola(hb) => {
                    let n = 64;
                    let epsilon = 0.05;
                    let cos_a = hb.angle.cos();
                    let sin_a = hb.angle.sin();
                    for branch in 0..2 {
                        let t_start = -std::f64::consts::FRAC_PI_2
                            + epsilon
                            + branch as f64 * std::f64::consts::PI;
                        let t_end = std::f64::consts::FRAC_PI_2 - epsilon
                            + branch as f64 * std::f64::consts::PI;
                        let mut prev: Option<glam::Vec2> = None;
                        for i in 0..=n {
                            let t = t_start + (t_end - t_start) * i as f64 / n as f64;
                            let sec = 1.0 / t.cos();
                            let tan = t.tan();
                            let (lx, ly) = if hb.horizontal {
                                (hb.a * sec, hb.b * tan)
                            } else {
                                (hb.b * tan, hb.a * sec)
                            };
                            let wx = hb.center.x + lx * cos_a - ly * sin_a;
                            let wy = hb.center.y + lx * sin_a + ly * cos_a;
                            if wx.is_finite() && wy.is_finite() {
                                let s = view_transform.world_to_screen(Point2::new(wx, wy));
                                if let Some(prev_p) = prev {
                                    if (s.x - prev_p.x).abs() < 300.0 {
                                        Self::add_line_segment(
                                            &mut vertices,
                                            &mut indices,
                                            prev_p,
                                            s,
                                            hb.width,
                                            hb.color,
                                        );
                                    }
                                }
                                prev = Some(s);
                            }
                        }
                    }
                }
                GeoObject::Text(txt) => {
                    let screen = view_transform.world_to_screen(txt.position);
                    Self::add_text_screen(
                        &mut vertices,
                        &mut indices,
                        &txt.content,
                        glam::Vec2::new(screen.x, screen.y),
                        txt.font_size,
                        txt.color,
                    );
                }
                GeoObject::ParametricCurve2D(pc) => {
                    let steps = 4000;
                    let samples = grafito_core::parametric_sampling::samples_or_compute_curve_2d(
                        pc,
                        steps,
                        &document.variables,
                    );
                    let mut prev: Option<[f32; 2]> = None;
                    for &(x, y) in samples.iter() {
                        if x.is_finite() && y.is_finite() {
                            let s = view_transform.world_to_screen(Point2::new(x, y));
                            if let Some(p) = prev {
                                if (s.x - p[0]).abs() < 300.0 && (s.y - p[1]).abs() < 300.0 {
                                    Self::add_line_segment(
                                        &mut vertices,
                                        &mut indices,
                                        glam::Vec2::new(p[0], p[1]),
                                        s,
                                        pc.width,
                                        pc.color,
                                    );
                                }
                            }
                            prev = Some([s.x, s.y]);
                        } else {
                            prev = None;
                        }
                    }
                }
                GeoObject::PolarCurve(pol) => {
                    let steps = 4000;
                    let samples = grafito_core::parametric_sampling::samples_or_compute_polar(
                        pol,
                        steps,
                        &document.variables,
                    );
                    let mut prev: Option<[f32; 2]> = None;
                    for &(x, y) in samples.iter() {
                        if x.is_finite() && y.is_finite() {
                            let s = view_transform.world_to_screen(Point2::new(x, y));
                            if let Some(p) = prev {
                                if (s.x - p[0]).abs() < 300.0 && (s.y - p[1]).abs() < 300.0 {
                                    Self::add_line_segment(
                                        &mut vertices,
                                        &mut indices,
                                        glam::Vec2::new(p[0], p[1]),
                                        s,
                                        pol.width,
                                        pol.color,
                                    );
                                }
                            }
                            prev = Some([s.x, s.y]);
                        } else {
                            prev = None;
                        }
                    }
                }
                GeoObject::ImplicitCurve(_) => {
                    // El render del ImplicitCurve se hace por CPU (ver
                    // `render_2d.rs::draw_object_styled` → brazo
                    // `GeoObject::ImplicitCurve`). Esto evita el doble render
                    // y los problemas de offset que ocurrían cuando GPU y CPU
                    // dibujaban el mismo objeto en sistemas de coordenadas
                    // distintos. La GPU sigue acelerando el cómputo de
                    // marching squares vía `implicit_compute`.
                }
                GeoObject::Histogram(h) => {
                    let bins = grafito_geometry::statistics::histogram(&h.data, h.bins);
                    let max_count = bins.iter().map(|(_, _, c)| *c).fold(0.0f64, f64::max);
                    if max_count > 0.0 && !bins.is_empty() {
                        let y_scale = (h.y_max - h.y_min) / max_count;
                        for (left, right, count) in &bins {
                            let bar_h = h.y_min + count * y_scale;
                            let bl = view_transform.world_to_screen(Point2::new(*left, h.y_min));
                            let tr = view_transform.world_to_screen(Point2::new(*right, bar_h));
                            let w = tr.x - bl.x;
                            let h_bar = tr.y - bl.y;
                            Self::add_rect(&mut vertices, &mut indices, bl, w, h_bar, h.color);
                        }
                    }
                }
                GeoObject::ScatterPlot(sp) => {
                    for (x, y) in sp.xs.iter().zip(sp.ys.iter()) {
                        let s = view_transform.world_to_screen(Point2::new(*x, *y));
                        Self::add_rect(
                            &mut vertices,
                            &mut indices,
                            s,
                            sp.point_size,
                            sp.point_size,
                            sp.color,
                        );
                    }
                }
                GeoObject::BoxPlot(bp) => {
                    if let Some((min, q1, _med, q3, max, outliers)) =
                        grafito_geometry::statistics::boxplot_stats(&bp.data)
                    {
                        let half_w = bp.width_box * 0.5;
                        let x = view_transform
                            .world_to_screen(Point2::new(bp.position, 0.0))
                            .x;
                        let y_min = view_transform.world_to_screen(Point2::new(0.0, min)).y;
                        let y_q1 = view_transform.world_to_screen(Point2::new(0.0, q1)).y;
                        let y_q3 = view_transform.world_to_screen(Point2::new(0.0, q3)).y;
                        let y_max = view_transform.world_to_screen(Point2::new(0.0, max)).y;
                        let hw = (half_w * view_transform.scale) as f32;
                        let bx = x - hw;
                        let bw = hw * 2.0;
                        Self::add_rect(
                            &mut vertices,
                            &mut indices,
                            glam::Vec2::new(bx, y_q3),
                            bw,
                            (y_q1 - y_q3).abs(),
                            bp.color,
                        );
                        Self::add_line_segment(
                            &mut vertices,
                            &mut indices,
                            glam::Vec2::new(x, y_q3),
                            glam::Vec2::new(x, y_max),
                            bp.width,
                            bp.color,
                        );
                        Self::add_line_segment(
                            &mut vertices,
                            &mut indices,
                            glam::Vec2::new(x, y_q1),
                            glam::Vec2::new(x, y_min),
                            bp.width,
                            bp.color,
                        );
                        for o in &outliers {
                            let oy = view_transform.world_to_screen(Point2::new(0.0, *o)).y;
                            Self::add_rect(
                                &mut vertices,
                                &mut indices,
                                glam::Vec2::new(x - 2.0, oy - 2.0),
                                4.0,
                                4.0,
                                bp.color,
                            );
                        }
                    }
                }
                GeoObject::RegressionLine(rl) => {
                    let x_min = rl.xs.iter().cloned().fold(f64::INFINITY, f64::min);
                    let x_max = rl.xs.iter().cloned().fold(f64::NEG_INFINITY, f64::max);
                    let y1 = rl.slope * x_min + rl.intercept;
                    let y2 = rl.slope * x_max + rl.intercept;
                    let a = view_transform.world_to_screen(Point2::new(x_min, y1));
                    let b = view_transform.world_to_screen(Point2::new(x_max, y2));
                    Self::add_line_segment(&mut vertices, &mut indices, a, b, rl.width, rl.color);
                    for (x, y) in rl.xs.iter().zip(rl.ys.iter()) {
                        let s = view_transform.world_to_screen(Point2::new(*x, *y));
                        Self::add_rect(
                            &mut vertices,
                            &mut indices,
                            s,
                            4.0,
                            4.0,
                            Color::new(0.0, 0.0, 1.0, 1.0),
                        );
                    }
                }
                GeoObject::Fractal2D(fr) => {
                    let width = 200u32;
                    let height =
                        (width as f64 * (fr.y_max - fr.y_min) / (fr.x_max - fr.x_min)) as u32;
                    let max_dim = width.max(height);
                    if max_dim <= 2048 {
                        let fractal = match fr.fractal_type.as_str() {
                            "julia" => grafito_geometry::fractals::FractalType::julia_dendrite(),
                            "burning_ship" => {
                                grafito_geometry::fractals::FractalType::burning_ship()
                            }
                            _ => grafito_geometry::fractals::FractalType::mandelbrot(),
                        };
                        let pixels = grafito_geometry::fractals::compute_fractal(
                            &fractal,
                            fr.x_min,
                            fr.x_max,
                            fr.y_min,
                            fr.y_max,
                            width as usize,
                            height as usize,
                        );
                        let dx = (fr.x_max - fr.x_min) / width as f64;
                        let dy = (fr.y_max - fr.y_min) / height as f64;
                        for p in &pixels {
                            let (r, g, b, a) = grafito_geometry::fractals::fractal_color_hsv(
                                p.iter,
                                p.max_iter,
                                p.smooth_value,
                            );
                            let color = Color::new(r, g, b, a);
                            let sx = view_transform.world_to_screen(Point2::new(p.x, p.y));
                            let px_w = (dx * view_transform.scale) as f32;
                            let px_h = (dy * view_transform.scale) as f32;
                            Self::add_rect(
                                &mut vertices,
                                &mut indices,
                                sx,
                                px_w.max(1.0),
                                px_h.max(1.0),
                                color,
                            );
                        }
                    }
                }
                GeoObject::VectorField2D(vf) => {
                    let world_tl = view_transform.screen_to_world(glam::Vec2::new(0.0, 0.0));
                    let world_br = view_transform.screen_to_world(view_transform.screen_size);
                    let view_bounds = (
                        world_tl.x.min(world_br.x),
                        world_tl.x.max(world_br.x),
                        world_br.y.min(world_tl.y),
                        world_br.y.max(world_tl.y),
                    );
                    let grid_size = vf.density.max(5).min(128);
                    let samples = grafito_core::vector_field_sampling::samples_or_compute(
                        vf,
                        view_bounds,
                        grid_size,
                        &document.variables,
                    );
                    for (x, y, u, v) in samples.iter() {
                        if u.is_finite() && v.is_finite() {
                            let mag = (u * u + v * v).sqrt();
                            if mag > 0.001 {
                                let nx = u / mag;
                                let ny = v / mag;
                                let len = mag.min(2.0) * 0.5;
                                let start = Point2::new(*x, *y);
                                let end = Point2::new(x + nx * len, y + ny * len);
                                let s = view_transform.world_to_screen(start);
                                let e = view_transform.world_to_screen(end);
                                Self::add_line_segment(
                                    &mut vertices,
                                    &mut indices,
                                    s,
                                    e,
                                    1.5,
                                    vf.color,
                                );
                            }
                        }
                    }
                }
                GeoObject::ComplexGrid(cg) => {
                    let base_symbol = document.complex_base_symbol.as_str();
                    match cg.render_mode {
                        1 => {
                            let res = cg.density.clamp(50, 500);
                            let dx = (cg.x_max - cg.x_min) / res as f64;
                            let dy = (cg.y_max - cg.y_min) / res as f64;
                            if let Ok(parsed) = grafito_geometry::complex_expr::parse(&cg.expr) {
                                let mut vars = std::collections::HashMap::new();
                                for (k, v) in &document.variables {
                                    vars.insert(k.clone(), num_complex::Complex64::new(*v, 0.0));
                                }
                                for i in 0..res {
                                    let x = cg.x_min + i as f64 * dx;
                                    for j in 0..res {
                                        let y = cg.y_min + j as f64 * dy;
                                        vars.insert(
                                            base_symbol.to_string(),
                                            num_complex::Complex64::new(x, y),
                                        );
                                        if let Ok(fz) = parsed.eval(&vars) {
                                            if fz.re.is_finite() && fz.im.is_finite() {
                                                let mag = fz.norm();
                                                let ang = fz.arg();
                                                let hue = (ang / std::f64::consts::TAU + 0.5) % 1.0;
                                                let sat = 0.8;
                                                let val = (mag / (mag + 1.0)).max(0.2);
                                                let color = hsv_to_rgb(hue as f32, sat, val as f32);
                                                let sx = view_transform
                                                    .world_to_screen(Point2::new(x, y));
                                                Self::add_rect(
                                                    &mut vertices,
                                                    &mut indices,
                                                    sx,
                                                    (dx * view_transform.scale).max(1.0) as f32,
                                                    (dy * view_transform.scale).max(1.0) as f32,
                                                    color,
                                                );
                                            }
                                        }
                                    }
                                }
                            }
                        }
                        2 => {
                            let res = cg.density.clamp(50, 500);
                            let dx = (cg.x_max - cg.x_min) / res as f64;
                            let dy = (cg.y_max - cg.y_min) / res as f64;
                            if let Ok(ast) = grafito_geometry::expr::prepare_function_ast(
                                &cg.expr,
                                &document.variables,
                                &["x", "y"],
                            ) {
                                for i in 0..res {
                                    let x = cg.x_min + i as f64 * dx;
                                    for j in 0..res {
                                        let y = cg.y_min + j as f64 * dy;
                                        let val = ast.eval_2d("x", x, "y", y);
                                        if val.is_finite() {
                                            let t = (val.atan() / std::f64::consts::FRAC_PI_2)
                                                .clamp(-1.0, 1.0);
                                            let hue = 0.66 * (1.0 - (t + 1.0) * 0.5);
                                            let color = hsv_to_rgb(hue as f32, 0.85, 0.95);
                                            let sx =
                                                view_transform.world_to_screen(Point2::new(x, y));
                                            Self::add_rect(
                                                &mut vertices,
                                                &mut indices,
                                                sx,
                                                (dx * view_transform.scale).max(1.0) as f32,
                                                (dy * view_transform.scale).max(1.0) as f32,
                                                color,
                                            );
                                        }
                                    }
                                }
                            }
                        }
                        _ => {
                            let grid_lines = cg.density.max(1);
                            let dx = (cg.x_max - cg.x_min) / grid_lines as f64;
                            let dy = (cg.y_max - cg.y_min) / grid_lines as f64;
                            if let Ok(parsed) = grafito_geometry::complex_expr::parse(&cg.expr) {
                                let mut vars = std::collections::HashMap::new();
                                for (k, v) in &document.variables {
                                    vars.insert(k.clone(), num_complex::Complex64::new(*v, 0.0));
                                }
                                for j in 0..=grid_lines {
                                    let y = cg.y_min + j as f64 * dy;
                                    let mut prev: Option<glam::Vec2> = None;
                                    for i in 0..=grid_lines * 4 {
                                        let x = cg.x_min + i as f64 * dx / 4.0;
                                        vars.insert(
                                            base_symbol.to_string(),
                                            num_complex::Complex64::new(x, y),
                                        );
                                        if let Ok(result) = parsed.eval(&vars) {
                                            if result.re.is_finite()
                                                && result.im.is_finite()
                                                && result.re.abs() < 1e6
                                                && result.im.abs() < 1e6
                                            {
                                                let screen = view_transform.world_to_screen(
                                                    Point2::new(result.re, result.im),
                                                );
                                                if let Some(prev_screen) = prev {
                                                    Self::add_line_segment(
                                                        &mut vertices,
                                                        &mut indices,
                                                        prev_screen,
                                                        screen,
                                                        1.0,
                                                        cg.color,
                                                    );
                                                }
                                                prev = Some(screen);
                                            } else {
                                                prev = None;
                                            }
                                        } else {
                                            prev = None;
                                        }
                                    }
                                }
                                for i in 0..=grid_lines {
                                    let x = cg.x_min + i as f64 * dx;
                                    let mut prev: Option<glam::Vec2> = None;
                                    for j in 0..=grid_lines * 4 {
                                        let y = cg.y_min + j as f64 * dy / 4.0;
                                        vars.insert(
                                            base_symbol.to_string(),
                                            num_complex::Complex64::new(x, y),
                                        );
                                        if let Ok(result) = parsed.eval(&vars) {
                                            if result.re.is_finite()
                                                && result.im.is_finite()
                                                && result.re.abs() < 1e6
                                                && result.im.abs() < 1e6
                                            {
                                                let screen = view_transform.world_to_screen(
                                                    Point2::new(result.re, result.im),
                                                );
                                                if let Some(prev_screen) = prev {
                                                    Self::add_line_segment(
                                                        &mut vertices,
                                                        &mut indices,
                                                        prev_screen,
                                                        screen,
                                                        1.0,
                                                        cg.color,
                                                    );
                                                }
                                                prev = Some(screen);
                                            } else {
                                                prev = None;
                                            }
                                        } else {
                                            prev = None;
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
                GeoObject::ComplexMapping(cm) => {
                    if let Some(target_obj) = document.get_object(cm.target) {
                        let conformal_map = cm.conformal_cache.or_else(|| {
                            grafito_geometry::conformal::algebraic_mappings::ConformalMap::from_expr_string_with_symbol(
                                &cm.expr,
                                document.complex_base_symbol.as_str(),
                            )
                        });
                        if let (GeoObject::ImplicitCurve(ic), Some(map)) =
                            (target_obj, conformal_map)
                        {
                            let cached =
                                ic.cached_segments.read().unwrap_or_else(|p| p.into_inner());
                            let mut source_segments = Vec::new();
                            for (_level, segments) in cached.iter() {
                                for (a, b) in segments {
                                    let len = (a.x - b.x).hypot(a.y - b.y);
                                    if len >= 1e-3 {
                                        source_segments.push((*a, *b));
                                    }
                                }
                            }
                            drop(cached);

                            for (a, b) in
                                transform_complex_mapping_segments(map, &source_segments, 16)
                            {
                                let p1 = view_transform.world_to_screen(a);
                                let p2 = view_transform.world_to_screen(b);
                                if (p2.x - p1.x).abs() < 300.0 && (p2.y - p1.y).abs() < 300.0 {
                                    Self::add_line_segment(
                                        &mut vertices,
                                        &mut indices,
                                        p1,
                                        p2,
                                        1.5,
                                        cm.color,
                                    );
                                }
                            }
                            continue;
                        }

                        let capacity = match target_obj {
                            GeoObject::Polygon(poly) => poly.vertices.len(),
                            GeoObject::Point(_) => 1,
                            GeoObject::Line(_) => 21,
                            GeoObject::Circle(_) => 32,
                            GeoObject::Function(_) => 51,
                            _ => 0,
                        };
                        let mut sample_pts: Vec<Point2> = Vec::with_capacity(capacity);
                        match target_obj {
                            GeoObject::Polygon(poly) => {
                                for (i, v) in poly.vertices.iter().enumerate() {
                                    let x = document
                                        .resolve_expr(poly.x_exprs.get(i).unwrap_or(&None), v.x);
                                    let y = document
                                        .resolve_expr(poly.y_exprs.get(i).unwrap_or(&None), v.y);
                                    sample_pts.push(Point2::new(x, y));
                                }
                            }
                            GeoObject::Point(p) => sample_pts.push(p.position),
                            GeoObject::Line(l) => {
                                let start = Point2::new(
                                    document.resolve_expr(&l.start_x_expr, l.start.x),
                                    document.resolve_expr(&l.start_y_expr, l.start.y),
                                );
                                let end = Point2::new(
                                    document.resolve_expr(&l.end_x_expr, l.end.x),
                                    document.resolve_expr(&l.end_y_expr, l.end.y),
                                );
                                for i in 0..=20 {
                                    let t = i as f64 / 20.0;
                                    sample_pts.push(Point2::new(
                                        start.x + t * (end.x - start.x),
                                        start.y + t * (end.y - start.y),
                                    ));
                                }
                            }
                            GeoObject::Circle(c) => {
                                for i in 0..32 {
                                    let a = i as f64 / 32.0 * std::f64::consts::TAU;
                                    sample_pts.push(Point2::new(
                                        c.center.x + c.radius * a.cos(),
                                        c.center.y + c.radius * a.sin(),
                                    ));
                                }
                            }
                            GeoObject::Function(f) => {
                                let x_min = document.resolve_expr(
                                    &f.domain_min_expr,
                                    f.domain_min.unwrap_or(-10.0),
                                );
                                let x_max = document
                                    .resolve_expr(&f.domain_max_expr, f.domain_max.unwrap_or(10.0));
                                for i in 0..=50 {
                                    let x = x_min + i as f64 / 50.0 * (x_max - x_min);
                                    if let Ok(y) = grafito_geometry::expr::evaluate(
                                        &f.expr,
                                        &[("x".to_string(), x)],
                                    ) {
                                        if y.is_finite() {
                                            sample_pts.push(Point2::new(x, y));
                                        }
                                    }
                                }
                            }
                            _ => {}
                        }
                        if sample_pts.len() >= 2 {
                            if let Some(map) = conformal_map {
                                let mut prev: Option<glam::Vec2> = None;
                                for pt in &sample_pts {
                                    let z = num_complex::Complex64::new(pt.x, pt.y);
                                    if let Some(fz) = map.apply(z) {
                                        if fz.re.is_finite() && fz.im.is_finite() {
                                            let tw = Point2::new(fz.re, fz.im);
                                            let ts = view_transform.world_to_screen(tw);
                                            if let Some(ps) = prev {
                                                if (ts.x - ps.x).abs() < 300.0
                                                    && (ts.y - ps.y).abs() < 300.0
                                                {
                                                    Self::add_line_segment(
                                                        &mut vertices,
                                                        &mut indices,
                                                        ps,
                                                        ts,
                                                        1.5,
                                                        cm.color,
                                                    );
                                                }
                                            }
                                            prev = Some(ts);
                                        }
                                    }
                                }
                            } else if let Ok(parsed) =
                                grafito_geometry::complex_expr::parse(&cm.expr)
                            {
                                let mut prev: Option<glam::Vec2> = None;
                                let mut vars = std::collections::HashMap::new();
                                for pt in &sample_pts {
                                    let z = num_complex::Complex64::new(pt.x, pt.y);
                                    vars.insert(document.complex_base_symbol.clone(), z);
                                    if let Ok(fz) = parsed.eval(&vars) {
                                        if fz.re.is_finite() && fz.im.is_finite() {
                                            let tw = Point2::new(fz.re, fz.im);
                                            let ts = view_transform.world_to_screen(tw);
                                            if let Some(ps) = prev {
                                                if (ts.x - ps.x).abs() < 300.0
                                                    && (ts.y - ps.y).abs() < 300.0
                                                {
                                                    Self::add_line_segment(
                                                        &mut vertices,
                                                        &mut indices,
                                                        ps,
                                                        ts,
                                                        1.5,
                                                        cm.color,
                                                    );
                                                }
                                            }
                                            prev = Some(ts);
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
                GeoObject::PhasePortrait(pp) => {
                    let d = pp.density.max(5).min(40);
                    let dx = (pp.x_max - pp.x_min) / d as f64;
                    let dy = (pp.y_max - pp.y_min) / d as f64;
                    for i in 0..=d {
                        let x = pp.x_min + i as f64 * dx;
                        for j in 0..=d {
                            let y = pp.y_min + j as f64 * dy;
                            if let (Ok(u), Ok(v)) = (
                                grafito_geometry::expr::evaluate(
                                    &pp.expr_dx,
                                    &[("x".to_string(), x), ("y".to_string(), y)],
                                ),
                                grafito_geometry::expr::evaluate(
                                    &pp.expr_dy,
                                    &[("x".to_string(), x), ("y".to_string(), y)],
                                ),
                            ) {
                                if u.is_finite() && v.is_finite() {
                                    let mag = (u * u + v * v).sqrt();
                                    if mag > 0.001 {
                                        let start = Point2::new(x, y);
                                        let end = Point2::new(x + u / mag * 0.5, y + v / mag * 0.5);
                                        let s = view_transform.world_to_screen(start);
                                        let e = view_transform.world_to_screen(end);
                                        Self::add_line_segment(
                                            &mut vertices,
                                            &mut indices,
                                            s,
                                            e,
                                            1.5,
                                            pp.color,
                                        );
                                    }
                                }
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

        let pixels_per_unit = view.scale;
        let target_world_step = 120.0 / pixels_per_unit.max(1e-50);
        let magnitude = target_world_step.log10().floor();
        let base = 10f64.powf(magnitude);
        let factor = target_world_step / base;

        let major_step = if factor < 2.0 {
            1.0 * base
        } else if factor < 5.0 {
            2.0 * base
        } else {
            5.0 * base
        };

        let mut min_x = (world_tl.x / major_step).floor() as i32 - 1;
        let mut max_x = (world_br.x / major_step).ceil() as i32 + 1;
        let mut min_y = (world_br.y / major_step).floor() as i32 - 1;
        let mut max_y = (world_tl.y / major_step).ceil() as i32 + 1;

        // Safety limit to prevent freezing due to massive zoom / floating point precision loss
        if max_x.saturating_sub(min_x) > 500 {
            let center = (min_x + max_x) / 2;
            min_x = center - 250;
            max_x = center + 250;
        }
        if max_y.saturating_sub(min_y) > 500 {
            let center = (min_y + max_y) / 2;
            min_y = center - 250;
            max_y = center + 250;
        }

        let color = if dark_mode {
            Color::new(0.25, 0.25, 0.25, 1.0)
        } else {
            Color::LIGHT_GRAY
        };

        let precision = if major_step >= 1.0 {
            0
        } else if major_step >= 0.1 {
            1
        } else if major_step >= 0.01 {
            2
        } else {
            4
        };

        let format_num = |v: f64| -> String {
            if v.abs() < 1e-9 {
                return "0".to_string();
            }
            let s = format!("{:.*}", precision, v);
            let s = if s.contains('.') {
                s.trim_end_matches('0').trim_end_matches('.').to_string()
            } else {
                s
            };
            if s.is_empty() || s == "-" {
                "0".to_string()
            } else {
                s
            }
        };

        for xi in min_x..=max_x {
            if xi == 0 {
                continue;
            }
            let x = xi as f64 * major_step;
            let a = view.world_to_screen(Point2::new(x, min_y as f64 * major_step));
            let b = view.world_to_screen(Point2::new(x, max_y as f64 * major_step));
            Self::add_line_segment(vertices, indices, a, b, 1.0, color);

            // Draw text label on X axis
            let pos = view.world_to_screen(Point2::new(x, 0.0));
            Self::add_text_screen(
                vertices,
                indices,
                &format_num(x),
                pos + glam::Vec2::new(-8.0, 5.0),
                12.0,
                color,
            );
        }

        for yi in min_y..=max_y {
            if yi == 0 {
                continue;
            }
            let y = yi as f64 * major_step;
            let a = view.world_to_screen(Point2::new(min_x as f64 * major_step, y));
            let b = view.world_to_screen(Point2::new(max_x as f64 * major_step, y));
            Self::add_line_segment(vertices, indices, a, b, 1.0, color);

            // Draw text label on Y axis
            let pos = view.world_to_screen(Point2::new(0.0, y));
            Self::add_text_screen(
                vertices,
                indices,
                &format_num(y),
                pos + glam::Vec2::new(5.0, -8.0),
                12.0,
                color,
            );
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

    fn add_rect(
        vertices: &mut Vec<Vertex>,
        indices: &mut Vec<u32>,
        center: glam::Vec2,
        w: f32,
        h: f32,
        color: Color,
    ) {
        let hw = w * 0.5;
        let hh = h * 0.5;
        let base = vertices.len() as u32;
        vertices.reserve(4);
        indices.reserve(6);
        vertices.push(Vertex::new(center.x - hw, center.y - hh, color));
        vertices.push(Vertex::new(center.x + hw, center.y - hh, color));
        vertices.push(Vertex::new(center.x + hw, center.y + hh, color));
        vertices.push(Vertex::new(center.x - hw, center.y + hh, color));
        indices.extend_from_slice(&[base, base + 1, base + 2, base, base + 2, base + 3]);
    }

    /// Dibuja un PencilObj en el view estático, aplicando clipping
    /// 2D por segmento. Helper compartido por build_geometry y
    /// build_geometry_static para evitar duplicación.
    fn draw_pencil_in_view_static(
        vertices: &mut Vec<Vertex>,
        indices: &mut Vec<u32>,
        view: &grafito_geometry::ViewTransform,
        pencil: &grafito_core::PencilObj,
    ) {
        let world_tl = view.screen_to_world(glam::Vec2::new(0.0, 0.0));
        let world_br = view.screen_to_world(view.screen_size);
        let view_bounds = grafito_geometry::AABB::new(
            Point2::new(world_tl.x.min(world_br.x), world_tl.y.min(world_br.y)),
            Point2::new(world_tl.x.max(world_br.x), world_tl.y.max(world_br.y)),
        );
        for w in pencil.points.windows(2) {
            let a = w[0];
            let b = w[1];
            if let Some((clip_a, clip_b)) =
                grafito_geometry::clip_segment_to_rect(a, b, view_bounds)
            {
                let sa = view.world_to_screen(clip_a);
                let sb = view.world_to_screen(clip_b);
                Self::add_line_segment(vertices, indices, sa, sb, pencil.width, pencil.color);
            }
        }
    }

    /// Dibuja un PencilObj usando view_transform (alias de
    /// draw_pencil_in_view_static para mantener compatibilidad con
    /// el path que ya recibía view_transform por nombre).
    fn draw_pencil_in_view_transform(
        vertices: &mut Vec<Vertex>,
        indices: &mut Vec<u32>,
        view: &grafito_geometry::ViewTransform,
        pencil: &grafito_core::PencilObj,
    ) {
        Self::draw_pencil_in_view_static(vertices, indices, view, pencil);
    }

    fn add_line_segment(
        vertices: &mut Vec<Vertex>,
        indices: &mut Vec<u32>,
        a: glam::Vec2,
        b: glam::Vec2,
        width: f32,
        color: Color,
    ) {
        let dir = b - a;
        if dir.length_squared() < 0.0001 {
            return;
        }
        let width = if width.is_finite() {
            width.clamp(0.5, 8.0)
        } else {
            1.5
        };
        let dir = dir.normalize();
        let perp = glam::Vec2::new(-dir.y, dir.x) * (width * 0.5).max(0.5);

        let base = vertices.len() as u32;
        vertices.reserve(4);
        indices.reserve(6);
        vertices.push(Vertex::new(a.x + perp.x, a.y + perp.y, color));
        vertices.push(Vertex::new(b.x + perp.x, b.y + perp.y, color));
        vertices.push(Vertex::new(b.x - perp.x, b.y - perp.y, color));
        vertices.push(Vertex::new(a.x - perp.x, a.y - perp.y, color));
        indices.extend_from_slice(&[base, base + 1, base + 2, base, base + 2, base + 3]);
    }

    fn add_text_screen(
        vertices: &mut Vec<Vertex>,
        indices: &mut Vec<u32>,
        text: &str,
        mut pos: glam::Vec2,
        size: f32,
        color: Color,
    ) {
        let width = size * 0.15;
        let char_w = size * 0.6;
        let char_spacing = size * 0.2;

        for c in text.chars() {
            let segments: &[(f32, f32, f32, f32)] = match c {
                '0' => &[
                    (0., 0., 1., 0.),
                    (1., 0., 1., 2.),
                    (1., 2., 0., 2.),
                    (0., 2., 0., 0.),
                ],
                '1' => &[(0.5, 0., 0.5, 2.), (0.15, 0.5, 0.5, 0.), (0.2, 2., 0.8, 2.)],
                '2' => &[
                    (0., 0., 1., 0.),
                    (1., 0., 1., 1.),
                    (1., 1., 0., 1.),
                    (0., 1., 0., 2.),
                    (0., 2., 1., 2.),
                ],
                '3' => &[
                    (0., 0., 1., 0.),
                    (1., 0., 1., 2.),
                    (1., 2., 0., 2.),
                    (0., 1., 1., 1.),
                ],
                '4' => &[(0., 0., 0., 1.), (0., 1., 1., 1.), (1., 0., 1., 2.)],
                '5' => &[
                    (1., 0., 0., 0.),
                    (0., 0., 0., 1.),
                    (0., 1., 1., 1.),
                    (1., 1., 1., 2.),
                    (1., 2., 0., 2.),
                ],
                '6' => &[
                    (1., 0., 0., 0.),
                    (0., 0., 0., 2.),
                    (0., 2., 1., 2.),
                    (1., 2., 1., 1.),
                    (1., 1., 0., 1.),
                ],
                '7' => &[(0., 0., 1., 0.), (1., 0., 0.2, 2.)],
                '8' => &[
                    (0., 0., 1., 0.),
                    (1., 0., 1., 2.),
                    (1., 2., 0., 2.),
                    (0., 2., 0., 0.),
                    (0., 1., 1., 1.),
                ],
                '9' => &[
                    (1., 2., 1., 0.),
                    (1., 0., 0., 0.),
                    (0., 0., 0., 1.),
                    (0., 1., 1., 1.),
                ],
                '-' => &[(0.2, 1., 0.8, 1.)],
                '.' => &[
                    (0.45, 1.8, 0.55, 1.8),
                    (0.55, 1.8, 0.55, 2.0),
                    (0.55, 2.0, 0.45, 2.0),
                    (0.45, 2.0, 0.45, 1.8),
                ],
                // Uppercase letters
                'A' => &[(0., 2., 0.5, 0.), (0.5, 0., 1., 2.), (0.2, 1., 0.8, 1.)],
                'B' => &[
                    (0., 0., 0., 2.),
                    (0., 2., 0.8, 2.),
                    (0.8, 2., 1., 1.5),
                    (1., 1.5, 0.8, 1.),
                    (0.8, 1., 0., 1.),
                    (0., 1., 0.8, 1.),
                    (0.8, 1., 1., 0.5),
                    (1., 0.5, 0.8, 0.),
                    (0.8, 0., 0., 0.),
                ],
                'C' => &[(1., 2., 0., 2.), (0., 2., 0., 0.), (0., 0., 1., 0.)],
                'D' => &[
                    (0., 0., 0., 2.),
                    (0., 2., 0.6, 2.),
                    (0.6, 2., 1., 1.),
                    (1., 1., 0.6, 0.),
                    (0.6, 0., 0., 0.),
                ],
                'E' => &[
                    (0., 0., 0., 2.),
                    (0., 2., 1., 2.),
                    (0., 1., 0.7, 1.),
                    (0., 0., 1., 0.),
                ],
                'F' => &[(0., 0., 0., 2.), (0., 2., 1., 2.), (0., 1., 0.7, 1.)],
                'G' => &[
                    (1., 2., 0., 2.),
                    (0., 2., 0., 0.),
                    (0., 0., 1., 0.),
                    (1., 0., 1., 1.),
                    (1., 1., 0.5, 1.),
                ],
                'H' => &[(0., 0., 0., 2.), (0., 1., 1., 1.), (1., 0., 1., 2.)],
                'I' => &[(0.5, 0., 0.5, 2.), (0., 0., 1., 0.), (0., 2., 1., 2.)],
                'J' => &[(0.5, 0., 0.5, 1.6), (0.5, 1.6, 0.2, 2.), (0., 2., 0.5, 2.)],
                'K' => &[(0., 0., 0., 2.), (0., 1., 1., 2.), (0., 1., 1., 0.)],
                'L' => &[(0., 0., 0., 2.), (0., 0., 1., 0.)],
                'M' => &[
                    (0., 2., 0., 0.),
                    (0., 0., 0.5, 1.),
                    (0.5, 1., 1., 0.),
                    (1., 0., 1., 2.),
                ],
                'N' => &[(0., 2., 0., 0.), (0., 0., 1., 2.), (1., 2., 1., 0.)],
                'O' => &[
                    (0., 0., 0., 2.),
                    (0., 2., 1., 2.),
                    (1., 2., 1., 0.),
                    (1., 0., 0., 0.),
                ],
                'P' => &[
                    (0., 0., 0., 2.),
                    (0., 2., 1., 2.),
                    (1., 2., 1., 1.),
                    (1., 1., 0., 1.),
                ],
                'Q' => &[
                    (0., 0., 0., 2.),
                    (0., 2., 1., 2.),
                    (1., 2., 1., 0.),
                    (1., 0., 0., 0.),
                    (0.5, 0.5, 1., 0.),
                ],
                'R' => &[
                    (0., 0., 0., 2.),
                    (0., 2., 1., 2.),
                    (1., 2., 1., 1.),
                    (1., 1., 0., 1.),
                    (0., 1., 1., 0.),
                ],
                'S' => &[
                    (1., 2., 0., 2.),
                    (0., 2., 0., 1.),
                    (0., 1., 1., 1.),
                    (1., 1., 1., 0.),
                    (1., 0., 0., 0.),
                ],
                'T' => &[(0., 2., 1., 2.), (0.5, 2., 0.5, 0.)],
                'U' => &[(0., 2., 0., 0.), (0., 0., 1., 0.), (1., 0., 1., 2.)],
                'V' => &[(0., 2., 0.5, 0.), (0.5, 0., 1., 2.)],
                'W' => &[
                    (0., 2., 0., 0.),
                    (0., 0., 0.5, 1.),
                    (0.5, 1., 1., 0.),
                    (1., 0., 1., 2.),
                ],
                'X' => &[(0., 0., 1., 2.), (0., 2., 1., 0.)],
                'Y' => &[(0., 2., 0.5, 1.), (1., 2., 0.5, 1.), (0.5, 1., 0.5, 0.)],
                'Z' => &[(0., 2., 1., 2.), (1., 2., 0., 0.), (0., 0., 1., 0.)],
                // Lowercase letters
                'a' => &[
                    (0., 0.5, 0., 1.5),
                    (0., 1.5, 1., 1.5),
                    (1., 1.5, 1., 0.),
                    (0., 0., 1., 0.),
                ],
                'b' => &[
                    (0., 0., 0., 2.),
                    (0., 1.5, 0.8, 1.5),
                    (0.8, 1.5, 1., 0.8),
                    (1., 0.8, 0.8, 0.),
                    (0.8, 0., 0., 0.),
                ],
                'c' => &[(1., 1.5, 0., 1.5), (0., 1.5, 0., 0.), (0., 0., 1., 0.)],
                'd' => &[
                    (1., 0., 1., 2.),
                    (1., 1.5, 0.2, 1.5),
                    (0.2, 1.5, 0., 0.8),
                    (0., 0.8, 0.2, 0.),
                    (0.2, 0., 1., 0.),
                ],
                'e' => &[
                    (1., 1.5, 0., 1.5),
                    (0., 1.5, 0., 0.5),
                    (0., 0.5, 0.5, 0.5),
                    (0., 0., 1., 0.),
                ],
                'f' => &[(0.3, 2., 0.7, 2.), (0.5, 2., 0.5, 0.), (0., 1., 0.5, 1.)],
                'g' => &[
                    (0., 0.5, 1., 0.5),
                    (1., 0.5, 1., -0.5),
                    (1., -0.5, 0., -0.5),
                    (0., -0.5, 0., 1.5),
                    (0., 1.5, 1., 1.5),
                ],
                'h' => &[
                    (0., 0., 0., 2.),
                    (0., 1., 0.8, 1.),
                    (0.8, 1., 1., 0.),
                    (1., 0., 1., 0.8),
                ],
                'i' => &[(0.5, 0., 0.5, 1.5), (0.3, 2., 0.7, 2.)],
                'j' => &[(0.5, 0., 0.5, 1.5), (0.5, 1.5, 0.2, 2.), (0.3, 2., 0.7, 2.)],
                'k' => &[(0., 0., 0., 2.), (0., 0.8, 0.8, 1.5), (0., 0.8, 1., 0.)],
                'l' => &[(0.5, 0., 0.5, 2.)],
                'm' => &[
                    (0., 0., 0., 1.5),
                    (0., 1.5, 0.3, 0.),
                    (0.3, 0., 0.6, 1.5),
                    (0.6, 1.5, 0.8, 0.),
                    (0.8, 0., 1., 1.5),
                    (1., 1.5, 1., 0.),
                ],
                'n' => &[
                    (0., 0., 0., 1.5),
                    (0., 1.5, 0.8, 0.),
                    (0.8, 0., 1., 1.5),
                    (1., 1.5, 1., 0.),
                ],
                'o' => &[
                    (0., 0., 0., 1.5),
                    (0., 1.5, 1., 1.5),
                    (1., 1.5, 1., 0.),
                    (1., 0., 0., 0.),
                ],
                'p' => &[
                    (0., -0.5, 0., 1.5),
                    (0., 1.5, 0.8, 1.5),
                    (0.8, 1.5, 1., 0.8),
                    (1., 0.8, 0.8, 0.),
                    (0.8, 0., 0., 0.),
                ],
                'q' => &[
                    (1., -0.5, 1., 1.5),
                    (0.2, 1.5, 1., 1.5),
                    (1., 1.5, 1., 0.8),
                    (1., 0.8, 0.8, 0.),
                    (0.8, 0., 0.2, 0.),
                    (0.2, 0., 0., 0.8),
                ],
                'r' => &[(0., 0., 0., 1.5), (0., 1.5, 0.8, 1.5), (0.8, 1.5, 0.8, 0.8)],
                's' => &[
                    (1., 1.5, 0., 1.5),
                    (0., 1.5, 0., 0.8),
                    (0., 0.8, 1., 0.8),
                    (1., 0.8, 1., 0.),
                    (1., 0., 0., 0.),
                ],
                't' => &[(0.5, 0., 0.5, 2.), (0., 1., 0.8, 1.)],
                'u' => &[(0., 1.5, 0., 0.), (0., 0., 1., 0.), (1., 0., 1., 1.5)],
                'v' => &[(0., 1.5, 0.5, 0.), (0.5, 0., 1., 1.5)],
                'w' => &[
                    (0., 1.5, 0.25, 0.),
                    (0.25, 0., 0.5, 0.8),
                    (0.5, 0.8, 0.75, 0.),
                    (0.75, 0., 1., 1.5),
                ],
                'x' => &[(0., 0., 1., 1.5), (0., 1.5, 1., 0.)],
                'y' => &[
                    (0., 1.5, 0.5, 0.),
                    (0.5, 0., 1., 1.5),
                    (0., -0.5, 0.5, -0.5),
                ],
                'z' => &[(0., 1.5, 1., 1.5), (1., 1.5, 0., 0.), (0., 0., 1., 0.)],
                // Symbols
                '+' => &[(0.5, 0.3, 0.5, 1.7), (0., 1., 1., 1.)],
                '=' => &[(0., 0.5, 1., 0.5), (0., 1.5, 1., 1.5)],
                '*' => &[
                    (0.2, 0.3, 0.8, 1.7),
                    (0.5, 0., 0.5, 2.),
                    (0., 0.7, 1., 1.4),
                    (0., 1.4, 1., 0.7),
                ],
                '/' => &[(1., 0.3, 0., 1.7)],
                '(' => &[(0.7, 2., 0.2, 1.), (0.2, 1., 0.7, 0.)],
                ')' => &[(0.3, 2., 0.8, 1.), (0.8, 1., 0.3, 0.)],
                '[' => &[(0.8, 2., 0.2, 2.), (0.2, 2., 0.2, 0.), (0.2, 0., 0.8, 0.)],
                ']' => &[(0.2, 2., 0.8, 2.), (0.8, 2., 0.8, 0.), (0.8, 0., 0.2, 0.)],
                ' ' => &[],
                _ => &[],
            };

            for &(x1, y1, x2, y2) in segments {
                let a = pos + glam::Vec2::new(x1 * char_w, y1 * char_w);
                let b = pos + glam::Vec2::new(x2 * char_w, y2 * char_w);
                Self::add_line_segment(vertices, indices, a, b, width, color);
            }
            pos.x += char_w + char_spacing;
        }
    }

    fn add_circle_stroke(
        vertices: &mut Vec<Vertex>,
        indices: &mut Vec<u32>,
        center: glam::Vec2,
        radius: f32,
        width: f32,
        color: Color,
    ) {
        let segments = ((radius * 0.5).clamp(16.0, 128.0)) as usize;
        let inner_r = (radius - width * 0.5).max(0.0);
        let outer_r = radius + width * 0.5;
        let base = vertices.len() as u32;
        vertices.reserve((segments + 1) * 2);
        indices.reserve(segments * 6);

        for i in 0..=segments {
            let theta = (i as f32 / segments as f32) * std::f32::consts::TAU;
            let c = theta.cos();
            let s = theta.sin();
            vertices.push(Vertex::new(
                center.x + inner_r * c,
                center.y + inner_r * s,
                color,
            ));
            vertices.push(Vertex::new(
                center.x + outer_r * c,
                center.y + outer_r * s,
                color,
            ));
        }

        for i in 0..segments {
            let i0 = base + (i * 2) as u32;
            let i1 = i0 + 1;
            let i2 = i0 + 2;
            let i3 = i0 + 3;
            indices.extend_from_slice(&[i0, i2, i1, i1, i2, i3]);
        }
    }

    fn add_circle_fill(
        vertices: &mut Vec<Vertex>,
        indices: &mut Vec<u32>,
        center: glam::Vec2,
        radius: f32,
        color: Color,
    ) {
        let segments = ((radius * 0.5).max(16.0).min(128.0)) as usize;
        let center_idx = vertices.len() as u32;
        vertices.reserve(segments + 2);
        indices.reserve(segments * 3);
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

    fn add_polygon_fill(
        vertices: &mut Vec<Vertex>,
        indices: &mut Vec<u32>,
        pts: &[glam::Vec2],
        color: Color,
    ) {
        if pts.len() < 3 {
            return;
        }
        // Simple fan triangulation (assumes convex or near-convex)
        let base = vertices.len() as u32;
        vertices.reserve(pts.len());
        indices.reserve((pts.len() - 1) * 3);
        for p in pts {
            vertices.push(Vertex::new(p.x, p.y, color));
        }
        for i in 1..(pts.len() - 1) {
            indices.extend_from_slice(&[base, base + i as u32, base + (i + 1) as u32]);
        }
    }

    fn add_polygon_stroke(
        vertices: &mut Vec<Vertex>,
        indices: &mut Vec<u32>,
        pts: &[glam::Vec2],
        width: f32,
        color: Color,
    ) {
        if pts.len() < 2 {
            return;
        }
        vertices.reserve(pts.len() * 4);
        indices.reserve(pts.len() * 6);
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
        include_overlays: bool,
    ) -> (Vec<Vertex>, Vec<u32>) {
        let obj_count = document.object_count().max(1);
        let mut vertices = Vec::with_capacity(obj_count * 256);
        let mut indices = Vec::with_capacity(obj_count * 384);

        if include_overlays {
            self.build_3d_grid(
                &mut vertices,
                &mut indices,
                camera,
                dark_mode,
                screen_w,
                screen_h,
            );
            self.build_3d_axes(
                &mut vertices,
                &mut indices,
                camera,
                dark_mode,
                screen_w,
                screen_h,
            );
        }

        for (_, obj) in document.objects_iter() {
            if !obj.is_visible() {
                continue;
            }
            match obj {
                GeoObject::Point3D(_p) => {
                    // Handled by CPU overlay
                }
                GeoObject::Segment3D(l) => {
                    Self::add_line_3d(
                        &mut vertices,
                        &mut indices,
                        camera,
                        &l.a,
                        &l.b,
                        l.width,
                        l.color,
                        screen_w,
                        screen_h,
                    );
                }
                GeoObject::Sphere3D(s) => {
                    if let Some(fc) = s.fill_color {
                        Self::add_solid_sphere(
                            &mut vertices,
                            &mut indices,
                            camera,
                            &s.center,
                            s.radius,
                            fc,
                            screen_w,
                            screen_h,
                        );
                    }
                    Self::add_wireframe_sphere(
                        &mut vertices,
                        &mut indices,
                        camera,
                        &s.center,
                        s.radius,
                        s.width,
                        s.color,
                        screen_w,
                        screen_h,
                    );
                }
                GeoObject::Cube3D(c) => {
                    if let Some(fc) = c.fill_color {
                        Self::add_solid_cube(
                            &mut vertices,
                            &mut indices,
                            camera,
                            &c.center,
                            c.size,
                            fc,
                            screen_w,
                            screen_h,
                        );
                    }
                    Self::add_wireframe_cube(
                        &mut vertices,
                        &mut indices,
                        camera,
                        &c.center,
                        c.size,
                        c.width,
                        c.color,
                        screen_w,
                        screen_h,
                    );
                }
                GeoObject::Pyramid3D(p) => {
                    if let Some(fc) = p.fill_color {
                        Self::add_solid_pyramid(
                            &mut vertices,
                            &mut indices,
                            camera,
                            &p.base_center,
                            &p.apex,
                            p.base_size,
                            fc,
                            screen_w,
                            screen_h,
                        );
                    }
                    Self::add_wireframe_pyramid(
                        &mut vertices,
                        &mut indices,
                        camera,
                        &p.base_center,
                        &p.apex,
                        p.base_size,
                        p.width,
                        p.color,
                        screen_w,
                        screen_h,
                    );
                }
                GeoObject::Cone3D(co) => {
                    if let Some(fc) = co.fill_color {
                        Self::add_solid_cone(
                            &mut vertices,
                            &mut indices,
                            camera,
                            &co.base_center,
                            &co.apex,
                            co.radius,
                            fc,
                            screen_w,
                            screen_h,
                        );
                    }
                    Self::add_wireframe_cone(
                        &mut vertices,
                        &mut indices,
                        camera,
                        &co.base_center,
                        &co.apex,
                        co.radius,
                        co.width,
                        co.color,
                        screen_w,
                        screen_h,
                    );
                }
                GeoObject::Cylinder3D(cy) => {
                    if let Some(fc) = cy.fill_color {
                        Self::add_solid_cylinder(
                            &mut vertices,
                            &mut indices,
                            camera,
                            &cy.base_center,
                            &cy.top_center,
                            cy.radius,
                            fc,
                            screen_w,
                            screen_h,
                        );
                    }
                    Self::add_wireframe_cylinder(
                        &mut vertices,
                        &mut indices,
                        camera,
                        &cy.base_center,
                        &cy.top_center,
                        cy.radius,
                        cy.width,
                        cy.color,
                        screen_w,
                        screen_h,
                    );
                }
                GeoObject::Surface3D(su) => {
                    Self::add_surface_mesh(
                        &mut vertices,
                        &mut indices,
                        camera,
                        su,
                        &document.variables,
                        screen_w,
                        screen_h,
                    );
                }
                GeoObject::Torus3D(to) => {
                    Self::add_solid_torus(
                        &mut vertices,
                        &mut indices,
                        camera,
                        &to.center,
                        to.r_major,
                        to.r_minor,
                        to.color,
                        screen_w,
                        screen_h,
                    );
                    Self::add_wireframe_torus(
                        &mut vertices,
                        &mut indices,
                        camera,
                        &to.center,
                        to.r_major,
                        to.r_minor,
                        to.width,
                        to.color,
                        screen_w,
                        screen_h,
                    );
                }
                GeoObject::MoebiusStrip(mb) => {
                    Self::add_solid_moebius(
                        &mut vertices,
                        &mut indices,
                        camera,
                        &mb.center,
                        mb.radius,
                        mb.width_r,
                        mb.color,
                        screen_w,
                        screen_h,
                    );
                    Self::add_wireframe_moebius(
                        &mut vertices,
                        &mut indices,
                        camera,
                        &mb.center,
                        mb.radius,
                        mb.width_r,
                        mb.width,
                        mb.color,
                        screen_w,
                        screen_h,
                    );
                }
                GeoObject::ParametricCurve3D(pc) => {
                    let steps = 4000;
                    let samples = grafito_core::parametric_sampling::samples_or_compute_curve_3d(
                        pc,
                        steps,
                        &document.variables,
                    );
                    let mut prev: Option<Point3D> = None;
                    for &(x, y, z) in samples.iter() {
                        if x.is_finite() && y.is_finite() && z.is_finite() {
                            let p = Point3D::new(x, y, z);
                            if let Some(prev_p) = prev {
                                Self::add_line_3d(
                                    &mut vertices,
                                    &mut indices,
                                    camera,
                                    &prev_p,
                                    &p,
                                    pc.width,
                                    pc.color,
                                    screen_w,
                                    screen_h,
                                );
                            }
                            prev = Some(p);
                        } else {
                            prev = None;
                        }
                    }
                }
                GeoObject::Attractor3D(at) => {
                    use grafito_geometry::attractors::*;
                    let att_type = match at.attractor_type.as_str() {
                        "lorenz" => AttractorType::lorenz(),
                        "rossler" => AttractorType::rossler(),
                        "thomas" => AttractorType::thomas(),
                        "aizawa" => AttractorType::aizawa(),
                        "chen" => AttractorType::chen(),
                        "halvorsen" => AttractorType::halvorsen(),
                        "dadras" => AttractorType::dadras(),
                        "chua" => AttractorType::chua(),
                        _ => AttractorType::lorenz(),
                    };
                    let points = integrate_attractor(
                        &att_type, at.x0, at.y0, at.z0, at.dt, at.steps, at.skip,
                    );
                    for w in points.windows(2) {
                        Self::add_line_3d(
                            &mut vertices,
                            &mut indices,
                            camera,
                            &w[0],
                            &w[1],
                            at.width,
                            at.color,
                            screen_w,
                            screen_h,
                        );
                    }
                }
                GeoObject::VectorField3D(vf) => {
                    let d = vf.density.max(3).min(15);
                    let dx = (vf.x_max - vf.x_min) / d as f64;
                    let dy = (vf.y_max - vf.y_min) / d as f64;
                    let dz = (vf.z_max - vf.z_min) / d as f64;
                    for i in 0..=d {
                        let x = vf.x_min + i as f64 * dx;
                        for j in 0..=d {
                            let y = vf.y_min + j as f64 * dy;
                            for k in 0..=d {
                                let z = vf.z_min + k as f64 * dz;
                                if let (Ok(u), Ok(v), Ok(w)) = (
                                    grafito_geometry::expr::evaluate(
                                        &vf.expr_u,
                                        &[
                                            ("x".to_string(), x),
                                            ("y".to_string(), y),
                                            ("z".to_string(), z),
                                        ],
                                    ),
                                    grafito_geometry::expr::evaluate(
                                        &vf.expr_v,
                                        &[
                                            ("x".to_string(), x),
                                            ("y".to_string(), y),
                                            ("z".to_string(), z),
                                        ],
                                    ),
                                    grafito_geometry::expr::evaluate(
                                        &vf.expr_w,
                                        &[
                                            ("x".to_string(), x),
                                            ("y".to_string(), y),
                                            ("z".to_string(), z),
                                        ],
                                    ),
                                ) {
                                    if u.is_finite() && v.is_finite() && w.is_finite() {
                                        let mag = (u * u + v * v + w * w).sqrt();
                                        if mag > 0.001 {
                                            let start = Point3D::new(x, y, z);
                                            let end =
                                                Point3D::new(x + u / mag, y + v / mag, z + w / mag);
                                            Self::add_line_3d(
                                                &mut vertices,
                                                &mut indices,
                                                camera,
                                                &start,
                                                &end,
                                                1.5,
                                                vf.color,
                                                screen_w,
                                                screen_h,
                                            );
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
                GeoObject::HyperSurface4D(hs) => {
                    let res = hs.resolution.max(4).min(40);
                    let vertices_count = res * res;
                    if vertices_count < 10000 {
                        for i in 0..res {
                            for j in 0..res {
                                let u = -1.0 + 2.0 * i as f64 / res as f64;
                                let v = -1.0 + 2.0 * j as f64 / res as f64;
                                let x = u * (1.0 - v * v / 2.0).cos();
                                let y = v * (1.0 - u * u / 2.0).cos();
                                let z = u
                                    * v
                                    * hs.rotation_angles.first().copied().unwrap_or(0.0).sin();
                                let p = Point3D::new(x * 3.0, y * 3.0, z * 3.0);
                                let sp = camera.project(&p, screen_w, screen_h);
                                if let Some(screen_p) = sp {
                                    Self::add_rect(
                                        &mut vertices,
                                        &mut indices,
                                        glam::Vec2::new(screen_p.0, screen_p.1),
                                        2.0,
                                        2.0,
                                        hs.color,
                                    );
                                }
                            }
                        }
                    }
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

        let major_step = if factor < 2.0 {
            1.0 * base
        } else if factor < 5.0 {
            2.0 * base
        } else {
            5.0 * base
        };
        let minor_step = major_step / 5.0;

        if minor_step <= 1e-9 {
            return;
        }

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

                let cam_pos = camera.position();
                let mut prev_screen_pos_x: Option<glam::Vec2> = None;
                for xi in 0..=major_line_count_x {
                    let x = start_x + xi as f64 * major_step;
                    let p1 = Point3D::new(x, 0.0, start_z);
                    let p2 = Point3D::new(x, 0.0, end_z);
                    Self::add_line_3d(
                        vertices,
                        indices,
                        camera,
                        &p1,
                        &p2,
                        1.5,
                        major_color,
                        screen_w,
                        screen_h,
                    );

                    if x.abs() > 1e-5 {
                        let dx = x - cam_pos.x as f64;
                        let dy = 0.0 - cam_pos.y as f64;
                        let dz = 0.0 - cam_pos.z as f64;
                        let dist = (dx * dx + dy * dy + dz * dz).sqrt();
                        if dist <= camera.distance as f64 * 1.8 {
                            if let Some((sx, sy)) =
                                camera.project(&Point3D::new(x, 0.0, 0.0), screen_w, screen_h)
                            {
                                let current_sp = glam::Vec2::new(sx, sy);
                                let overlap = if let Some(prev) = prev_screen_pos_x {
                                    current_sp.distance(prev) < 45.0
                                } else {
                                    false
                                };
                                if !overlap {
                                    prev_screen_pos_x = Some(current_sp);
                                    // CPU overlay handles text
                                }
                            }
                        }
                    }
                }

                let mut prev_screen_pos_z: Option<glam::Vec2> = None;
                for zi in 0..=major_line_count_z {
                    let z = start_z + zi as f64 * major_step;
                    let p1 = Point3D::new(start_x, 0.0, z);
                    let p2 = Point3D::new(end_x, 0.0, z);
                    Self::add_line_3d(
                        vertices,
                        indices,
                        camera,
                        &p1,
                        &p2,
                        1.5,
                        major_color,
                        screen_w,
                        screen_h,
                    );

                    if z.abs() > 1e-5 {
                        let dx = 0.0 - cam_pos.x as f64;
                        let dy = 0.0 - cam_pos.y as f64;
                        let dz = z - cam_pos.z as f64;
                        let dist = (dx * dx + dy * dy + dz * dz).sqrt();
                        if dist <= camera.distance as f64 * 1.8 {
                            if let Some((sx, sy)) =
                                camera.project(&Point3D::new(0.0, 0.0, z), screen_w, screen_h)
                            {
                                let current_sp = glam::Vec2::new(sx, sy);
                                let overlap = if let Some(prev) = prev_screen_pos_z {
                                    current_sp.distance(prev) < 45.0
                                } else {
                                    false
                                };
                                if !overlap {
                                    prev_screen_pos_z = Some(current_sp);
                                    // CPU overlay handles text
                                }
                            }
                        }
                    }
                }
            }
            // Si incluso major grid es demasiado, no dibujar nada
        } else {
            // Dibujar grid completo (minor + major)

            let cam_pos = camera.position();
            let mut prev_screen_pos_x: Option<glam::Vec2> = None;
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
                Self::add_line_3d(
                    vertices, indices, camera, &p1, &p2, width, color, screen_w, screen_h,
                );

                if is_major && x.abs() > 1e-5 {
                    let dx = x - cam_pos.x as f64;
                    let dy = 0.0 - cam_pos.y as f64;
                    let dz = 0.0 - cam_pos.z as f64;
                    let dist = (dx * dx + dy * dy + dz * dz).sqrt();
                    if dist <= camera.distance as f64 * 1.8 {
                        if let Some((sx, sy)) =
                            camera.project(&Point3D::new(x, 0.0, 0.0), screen_w, screen_h)
                        {
                            let current_sp = glam::Vec2::new(sx, sy);
                            let overlap = if let Some(prev) = prev_screen_pos_x {
                                current_sp.distance(prev) < 45.0
                            } else {
                                false
                            };
                            if !overlap {
                                prev_screen_pos_x = Some(current_sp);
                                // CPU overlay handles text
                            }
                        }
                    }
                }
            }

            let mut prev_screen_pos_z: Option<glam::Vec2> = None;
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
                Self::add_line_3d(
                    vertices, indices, camera, &p1, &p2, width, color, screen_w, screen_h,
                );

                if is_major && z.abs() > 1e-5 {
                    let dx = 0.0 - cam_pos.x as f64;
                    let dy = 0.0 - cam_pos.y as f64;
                    let dz = z - cam_pos.z as f64;
                    let dist = (dx * dx + dy * dy + dz * dz).sqrt();
                    if dist <= camera.distance as f64 * 1.8 {
                        if let Some((sx, sy)) =
                            camera.project(&Point3D::new(0.0, 0.0, z), screen_w, screen_h)
                        {
                            let current_sp = glam::Vec2::new(sx, sy);
                            let overlap = if let Some(prev) = prev_screen_pos_z {
                                current_sp.distance(prev) < 45.0
                            } else {
                                false
                            };
                            if !overlap {
                                prev_screen_pos_z = Some(current_sp);
                                // CPU overlay handles text
                            }
                        }
                    }
                }
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

        Self::add_line_3d(
            vertices,
            indices,
            camera,
            &Point3D::new(-axis_len, 0.0, 0.0),
            &Point3D::new(axis_len, 0.0, 0.0),
            2.0,
            red,
            screen_w,
            screen_h,
        );

        Self::add_line_3d(
            vertices,
            indices,
            camera,
            &Point3D::new(0.0, -axis_len, 0.0),
            &Point3D::new(0.0, axis_len, 0.0),
            2.0,
            green,
            screen_w,
            screen_h,
        );

        Self::add_line_3d(
            vertices,
            indices,
            camera,
            &Point3D::new(0.0, 0.0, -axis_len),
            &Point3D::new(0.0, 0.0, axis_len),
            2.0,
            blue,
            screen_w,
            screen_h,
        );
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

        for &(u, v) in &[
            (glam::Vec3::X, glam::Vec3::Y),
            (glam::Vec3::X, glam::Vec3::Z),
            (glam::Vec3::Y, glam::Vec3::Z),
        ] {
            let pts = Camera3D::circle_points(center_vec, u, v, r, segments);
            for i in 0..pts.len() {
                let j = (i + 1) % pts.len();
                let p1 = Point3D::from_vec3(pts[i]);
                let p2 = Point3D::from_vec3(pts[j]);
                Self::add_line_3d(
                    vertices, indices, camera, &p1, &p2, width, color, screen_w, screen_h,
                );
            }
        }
    }

    #[allow(clippy::too_many_arguments)]
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
            Point3D::new(x - h, y - h, z - h),
            Point3D::new(x + h, y - h, z - h),
            Point3D::new(x + h, y + h, z - h),
            Point3D::new(x - h, y + h, z - h),
            Point3D::new(x - h, y - h, z + h),
            Point3D::new(x + h, y - h, z + h),
            Point3D::new(x + h, y + h, z + h),
            Point3D::new(x - h, y + h, z + h),
        ];

        let edges = [
            (0, 1),
            (1, 2),
            (2, 3),
            (3, 0),
            (4, 5),
            (5, 6),
            (6, 7),
            (7, 4),
            (0, 4),
            (1, 5),
            (2, 6),
            (3, 7),
        ];

        for &(i, j) in &edges {
            Self::add_line_3d(
                vertices,
                indices,
                camera,
                &corners[i],
                &corners[j],
                width,
                color,
                screen_w,
                screen_h,
            );
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
            Point3D::new(cx - h, cy, cz - h),
            Point3D::new(cx + h, cy, cz - h),
            Point3D::new(cx + h, cy, cz + h),
            Point3D::new(cx - h, cy, cz + h),
        ];

        for i in 0..4 {
            let j = (i + 1) % 4;
            Self::add_line_3d(
                vertices,
                indices,
                camera,
                &base_corners[i],
                &base_corners[j],
                width,
                color,
                screen_w,
                screen_h,
            );
            Self::add_line_3d(
                vertices,
                indices,
                camera,
                &base_corners[i],
                apex,
                width,
                color,
                screen_w,
                screen_h,
            );
        }
    }

    #[allow(clippy::too_many_arguments)]
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
            Self::add_line_3d(
                vertices, indices, camera, &p1, &p2, width, color, screen_w, screen_h,
            );

            if i % 4 == 0 {
                Self::add_line_3d(
                    vertices, indices, camera, &p1, apex, width, color, screen_w, screen_h,
                );
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

            Self::add_line_3d(
                vertices, indices, camera, &bp1, &bp2, width, color, screen_w, screen_h,
            );
            Self::add_line_3d(
                vertices, indices, camera, &tp1, &tp2, width, color, screen_w, screen_h,
            );

            if i % 8 == 0 {
                Self::add_line_3d(
                    vertices, indices, camera, &bp1, &tp1, width, color, screen_w, screen_h,
                );
            }
        }
    }

    fn add_wireframe_torus(
        vertices: &mut Vec<Vertex>,
        indices: &mut Vec<u32>,
        camera: &Camera3D,
        center: &Point3D,
        r_major: f64,
        r_minor: f64,
        width: f32,
        color: Color,
        screen_w: f32,
        screen_h: f32,
    ) {
        let u_steps = 32usize;
        let v_steps = 16usize;
        for i in 0..u_steps {
            let u = i as f64 / u_steps as f64 * std::f64::consts::TAU;
            let u_next = (i + 1) as f64 / u_steps as f64 * std::f64::consts::TAU;
            for j in 0..v_steps {
                let v = j as f64 / v_steps as f64 * std::f64::consts::TAU;
                let x = (r_major + r_minor * v.cos()) * u.cos() + center.x;
                let y = (r_major + r_minor * v.cos()) * u.sin() + center.y;
                let z = r_minor * v.sin() + center.z;
                Self::add_line_3d_circle_segment(
                    vertices,
                    indices,
                    camera,
                    Point3D::new(x, y, z),
                    center,
                    r_major,
                    u,
                    u_next,
                    width,
                    color,
                    screen_w,
                    screen_h,
                );
            }
        }
    }

    fn add_line_3d_circle_segment(
        vertices: &mut Vec<Vertex>,
        indices: &mut Vec<u32>,
        camera: &Camera3D,
        p: Point3D,
        _center: &Point3D,
        _r: f64,
        _u1: f64,
        _u2: f64,
        width: f32,
        color: Color,
        screen_w: f32,
        screen_h: f32,
    ) {
        if let Some(ps) = camera.project(&p, screen_w, screen_h) {
            let s = glam::Vec2::new(ps.0, ps.1);
            Self::add_rect(
                vertices,
                indices,
                s,
                width.max(0.5) * 2.0,
                width.max(0.5) * 2.0,
                color,
            );
        }
    }

    fn add_wireframe_moebius(
        vertices: &mut Vec<Vertex>,
        indices: &mut Vec<u32>,
        camera: &Camera3D,
        center: &Point3D,
        radius: f64,
        width_r: f64,
        line_width: f32,
        color: Color,
        screen_w: f32,
        screen_h: f32,
    ) {
        let steps = 64usize;
        let mut prev: Option<Point3D> = None;
        for i in 0..=steps {
            let t = i as f64 / steps as f64 * std::f64::consts::TAU;
            let s = t * 0.5;
            let x = (radius + width_r * s.cos()) * t.cos() + center.x;
            let y = (radius + width_r * s.cos()) * t.sin() + center.y;
            let z = width_r * s.sin() + center.z;
            let p = Point3D::new(x, y, z);
            if let Some(prev_p) = prev {
                Self::add_line_3d(
                    vertices, indices, camera, &prev_p, &p, line_width, color, screen_w, screen_h,
                );
            }
            prev = Some(p);
        }
    }

    fn add_surface_mesh(
        vertices: &mut Vec<Vertex>,
        indices: &mut Vec<u32>,
        camera: &Camera3D,
        surface: &grafito_core::Surface3DObj,
        variables: &std::collections::HashMap<String, f64>,
        screen_w: f32,
        screen_h: f32,
    ) {
        let resolve = |expr: &Option<String>, fallback: f64| -> f64 {
            match expr {
                Some(e) => {
                    let vars: Vec<(String, f64)> =
                        variables.iter().map(|(k, v)| (k.clone(), *v)).collect();
                    grafito_geometry::expr::evaluate(e, &vars)
                        .ok()
                        .filter(|v| v.is_finite())
                        .unwrap_or(fallback)
                }
                None => fallback,
            }
        };

        let res = surface.mesh_res.min(128);
        let grid =
            grafito_core::parametric_sampling::samples_or_compute_surface(surface, res, variables);
        if grid.is_empty() || grid[0].is_empty() {
            return;
        }
        let n = grid.len().saturating_sub(1);
        let m = grid[0].len().saturating_sub(1);
        let x_min = resolve(&surface.x_min_expr, surface.x_min);
        let x_max = resolve(&surface.x_max_expr, surface.x_max);
        let y_min = resolve(&surface.y_min_expr, surface.y_min);
        let y_max = resolve(&surface.y_max_expr, surface.y_max);
        let x_step = (x_max - x_min) / n as f64;
        let y_step = (y_max - y_min) / m as f64;

        for i in 0..=n {
            let x = x_min + i as f64 * x_step;
            for j in 0..=m {
                let z = grid[i][j];
                if !z.is_finite() || z.abs() >= 100.0 {
                    continue;
                }
                let y = y_min + j as f64 * y_step;
                let p = Point3D::new(x, z, y);
                if i < n {
                    let z_right = grid[i + 1][j];
                    if z_right.is_finite() && z_right.abs() < 100.0 {
                        let x_right = x_min + (i + 1) as f64 * x_step;
                        let p_right = Point3D::new(x_right, z_right, y);
                        Self::add_line_3d(
                            vertices,
                            indices,
                            camera,
                            &p,
                            &p_right,
                            surface.width,
                            surface.color,
                            screen_w,
                            screen_h,
                        );
                    }
                }
                if j < m {
                    let z_down = grid[i][j + 1];
                    if z_down.is_finite() && z_down.abs() < 100.0 {
                        let y_down = y_min + (j + 1) as f64 * y_step;
                        let p_down = Point3D::new(x, z_down, y_down);
                        Self::add_line_3d(
                            vertices,
                            indices,
                            camera,
                            &p,
                            &p_down,
                            surface.width,
                            surface.color,
                            screen_w,
                            screen_h,
                        );
                    }
                }
            }
        }
    }

    fn face_normal(a: &Point3D, b: &Point3D, c: &Point3D) -> glam::Vec3 {
        let u = glam::Vec3::new((b.x - a.x) as f32, (b.y - a.y) as f32, (b.z - a.z) as f32);
        let v = glam::Vec3::new((c.x - a.x) as f32, (c.y - a.y) as f32, (c.z - a.z) as f32);
        let n = u.cross(v);
        let len = n.length();
        if len < 1e-10 {
            glam::Vec3::new(0.0, 1.0, 0.0)
        } else {
            n / len
        }
    }

    fn icosphere(subdivisions: usize) -> (Vec<(f64, f64, f64)>, Vec<u32>) {
        let t = (1.0 + 5.0f64.sqrt()) / 2.0;
        let mut verts = vec![
            (-1.0, t, 0.0),
            (1.0, t, 0.0),
            (-1.0, -t, 0.0),
            (1.0, -t, 0.0),
            (0.0, -1.0, t),
            (0.0, 1.0, t),
            (0.0, -1.0, -t),
            (0.0, 1.0, -t),
            (t, 0.0, -1.0),
            (t, 0.0, 1.0),
            (-t, 0.0, -1.0),
            (-t, 0.0, 1.0),
        ];
        for v in &mut verts {
            let mag = (v.0 * v.0 + v.1 * v.1 + v.2 * v.2).sqrt();
            v.0 /= mag;
            v.1 /= mag;
            v.2 /= mag;
        }
        let mut indices: Vec<u32> = vec![
            0, 11, 5, 0, 5, 1, 0, 1, 7, 0, 7, 10, 0, 10, 11, 1, 5, 9, 5, 11, 4, 11, 10, 2, 10, 7,
            6, 7, 1, 8, 3, 9, 4, 3, 4, 2, 3, 2, 6, 3, 6, 8, 3, 8, 9, 4, 9, 5, 2, 4, 11, 6, 2, 10,
            8, 6, 7, 9, 8, 1,
        ];
        for _ in 0..subdivisions {
            let mut new_indices = Vec::new();
            let mut midpoints = std::collections::HashMap::new();
            let mut get_mid = |v1: u32, v2: u32| -> u32 {
                let key = if v1 < v2 { (v1, v2) } else { (v2, v1) };
                *midpoints.entry(key).or_insert_with(|| {
                    let a = verts[v1 as usize];
                    let b = verts[v2 as usize];
                    let x = (a.0 + b.0) / 2.0;
                    let y = (a.1 + b.1) / 2.0;
                    let z = (a.2 + b.2) / 2.0;
                    let mag = (x * x + y * y + z * z).sqrt();
                    verts.push((x / mag, y / mag, z / mag));
                    (verts.len() - 1) as u32
                })
            };
            for tri in indices.chunks(3) {
                let (a, b, c) = (tri[0], tri[1], tri[2]);
                let m1 = get_mid(a, b);
                let m2 = get_mid(b, c);
                let m3 = get_mid(c, a);
                new_indices.extend_from_slice(&[a, m1, m3, m1, b, m2, m3, m2, c, m1, m2, m3]);
            }
            indices = new_indices;
        }
        (verts, indices)
    }

    fn add_solid_triangle_3d(
        vertices: &mut Vec<Vertex>,
        indices: &mut Vec<u32>,
        camera: &Camera3D,
        p0: &Point3D,
        n0: glam::Vec3,
        p1: &Point3D,
        n1: glam::Vec3,
        p2: &Point3D,
        n2: glam::Vec3,
        fill_color: Color,
        screen_w: f32,
        screen_h: f32,
    ) {
        let light_dir = glam::Vec3::new(0.5, 1.0, 0.3).normalize();
        let c0 = calculate_lighting(fill_color, n0, light_dir);
        let c1 = calculate_lighting(fill_color, n1, light_dir);
        let c2 = calculate_lighting(fill_color, n2, light_dir);
        if let (Some(s0), Some(s1), Some(s2)) = (
            camera.project(p0, screen_w, screen_h),
            camera.project(p1, screen_w, screen_h),
            camera.project(p2, screen_w, screen_h),
        ) {
            let base = vertices.len() as u32;
            vertices.push(Vertex::new(s0.0, s0.1, c0));
            vertices.push(Vertex::new(s1.0, s1.1, c1));
            vertices.push(Vertex::new(s2.0, s2.1, c2));
            indices.extend_from_slice(&[base, base + 1, base + 2]);
        }
    }

    fn add_solid_sphere(
        vertices: &mut Vec<Vertex>,
        indices: &mut Vec<u32>,
        camera: &Camera3D,
        center: &Point3D,
        radius: f64,
        fill_color: Color,
        screen_w: f32,
        screen_h: f32,
    ) {
        let level = 2;
        let (mesh_positions, mesh_indices) = Self::icosphere(level);
        let _light_dir = glam::Vec3::new(0.5, 1.0, 0.3).normalize();
        for tri in mesh_indices.chunks(3) {
            let i0 = tri[0] as usize;
            let i1 = tri[1] as usize;
            let i2 = tri[2] as usize;
            let v0 = mesh_positions[i0];
            let v1 = mesh_positions[i1];
            let v2 = mesh_positions[i2];
            let p0 = Point3D::new(
                center.x + v0.0 * radius,
                center.y + v0.1 * radius,
                center.z + v0.2 * radius,
            );
            let p1 = Point3D::new(
                center.x + v1.0 * radius,
                center.y + v1.1 * radius,
                center.z + v1.2 * radius,
            );
            let p2 = Point3D::new(
                center.x + v2.0 * radius,
                center.y + v2.1 * radius,
                center.z + v2.2 * radius,
            );
            let n = glam::Vec3::new(v0.0 as f32, v0.1 as f32, v0.2 as f32);
            Self::add_solid_triangle_3d(
                vertices, indices, camera, &p0, n, &p1, n, &p2, n, fill_color, screen_w, screen_h,
            );
        }
    }

    fn add_solid_cube(
        vertices: &mut Vec<Vertex>,
        indices: &mut Vec<u32>,
        camera: &Camera3D,
        center: &Point3D,
        size: f64,
        fill_color: Color,
        screen_w: f32,
        screen_h: f32,
    ) {
        let h = size * 0.5;
        let corners = [
            Point3D::new(center.x - h, center.y - h, center.z - h),
            Point3D::new(center.x + h, center.y - h, center.z - h),
            Point3D::new(center.x + h, center.y + h, center.z - h),
            Point3D::new(center.x - h, center.y + h, center.z - h),
            Point3D::new(center.x - h, center.y - h, center.z + h),
            Point3D::new(center.x + h, center.y - h, center.z + h),
            Point3D::new(center.x + h, center.y + h, center.z + h),
            Point3D::new(center.x - h, center.y + h, center.z + h),
        ];
        let faces: [(usize, usize, usize, usize, glam::Vec3); 6] = [
            (0, 1, 2, 3, glam::Vec3::new(0.0, 0.0, -1.0)),
            (4, 5, 6, 7, glam::Vec3::new(0.0, 0.0, 1.0)),
            (0, 1, 5, 4, glam::Vec3::new(0.0, -1.0, 0.0)),
            (2, 3, 7, 6, glam::Vec3::new(0.0, 1.0, 0.0)),
            (0, 3, 7, 4, glam::Vec3::new(-1.0, 0.0, 0.0)),
            (1, 2, 6, 5, glam::Vec3::new(1.0, 0.0, 0.0)),
        ];
        for (a, b, c, d, n) in &faces {
            Self::add_solid_triangle_3d(
                vertices,
                indices,
                camera,
                &corners[*a],
                *n,
                &corners[*b],
                *n,
                &corners[*c],
                *n,
                fill_color,
                screen_w,
                screen_h,
            );
            Self::add_solid_triangle_3d(
                vertices,
                indices,
                camera,
                &corners[*a],
                *n,
                &corners[*c],
                *n,
                &corners[*d],
                *n,
                fill_color,
                screen_w,
                screen_h,
            );
        }
    }

    fn add_solid_pyramid(
        vertices: &mut Vec<Vertex>,
        indices: &mut Vec<u32>,
        camera: &Camera3D,
        base_center: &Point3D,
        apex: &Point3D,
        base_size: f64,
        fill_color: Color,
        screen_w: f32,
        screen_h: f32,
    ) {
        let h = base_size * 0.5;
        let base = [
            Point3D::new(base_center.x - h, base_center.y, base_center.z - h),
            Point3D::new(base_center.x + h, base_center.y, base_center.z - h),
            Point3D::new(base_center.x + h, base_center.y, base_center.z + h),
            Point3D::new(base_center.x - h, base_center.y, base_center.z + h),
        ];
        let apex = *apex;
        for i in 0..4 {
            let j = (i + 1) % 4;
            let n = Self::face_normal(&base[i], &base[j], &apex);
            Self::add_solid_triangle_3d(
                vertices, indices, camera, &base[i], n, &base[j], n, &apex, n, fill_color,
                screen_w, screen_h,
            );
        }
        let n_base = glam::Vec3::new(0.0, -1.0, 0.0);
        Self::add_solid_triangle_3d(
            vertices, indices, camera, &base[0], n_base, &base[1], n_base, &base[2], n_base,
            fill_color, screen_w, screen_h,
        );
        Self::add_solid_triangle_3d(
            vertices, indices, camera, &base[0], n_base, &base[2], n_base, &base[3], n_base,
            fill_color, screen_w, screen_h,
        );
    }

    fn add_solid_cone(
        vertices: &mut Vec<Vertex>,
        indices: &mut Vec<u32>,
        camera: &Camera3D,
        base_center: &Point3D,
        apex: &Point3D,
        radius: f64,
        fill_color: Color,
        screen_w: f32,
        screen_h: f32,
    ) {
        let segs = 32;
        let mut circle = Vec::new();
        for i in 0..segs {
            let a = i as f64 / segs as f64 * std::f64::consts::TAU;
            circle.push(Point3D::new(
                base_center.x + radius * a.cos(),
                base_center.y,
                base_center.z + radius * a.sin(),
            ));
        }
        for i in 0..segs {
            let j = (i + 1) % segs;
            let n = Self::face_normal(&circle[i], &circle[j], apex);
            Self::add_solid_triangle_3d(
                vertices, indices, camera, &circle[i], n, &circle[j], n, apex, n, fill_color,
                screen_w, screen_h,
            );
        }
        let n_base = glam::Vec3::new(0.0, -1.0, 0.0);
        for i in 0..segs {
            let j = (i + 1) % segs;
            Self::add_solid_triangle_3d(
                vertices,
                indices,
                camera,
                &circle[i],
                n_base,
                &circle[j],
                n_base,
                base_center,
                n_base,
                fill_color,
                screen_w,
                screen_h,
            );
        }
    }

    fn add_solid_cylinder(
        vertices: &mut Vec<Vertex>,
        indices: &mut Vec<u32>,
        camera: &Camera3D,
        base_center: &Point3D,
        top_center: &Point3D,
        radius: f64,
        fill_color: Color,
        screen_w: f32,
        screen_h: f32,
    ) {
        let segs = 32;
        let mut b_circle = Vec::new();
        let mut t_circle = Vec::new();
        for i in 0..segs {
            let a = i as f64 / segs as f64 * std::f64::consts::TAU;
            b_circle.push(Point3D::new(
                base_center.x + radius * a.cos(),
                base_center.y,
                base_center.z + radius * a.sin(),
            ));
            t_circle.push(Point3D::new(
                top_center.x + radius * a.cos(),
                top_center.y,
                top_center.z + radius * a.sin(),
            ));
        }
        for i in 0..segs {
            let j = (i + 1) % segs;
            let n = Self::face_normal(&b_circle[i], &t_circle[i], &b_circle[j]);
            Self::add_solid_triangle_3d(
                vertices,
                indices,
                camera,
                &b_circle[i],
                n,
                &t_circle[i],
                n,
                &b_circle[j],
                n,
                fill_color,
                screen_w,
                screen_h,
            );
            Self::add_solid_triangle_3d(
                vertices,
                indices,
                camera,
                &t_circle[i],
                n,
                &t_circle[j],
                n,
                &b_circle[j],
                n,
                fill_color,
                screen_w,
                screen_h,
            );
        }
        let n_bot = glam::Vec3::new(0.0, -1.0, 0.0);
        let n_top = glam::Vec3::new(0.0, 1.0, 0.0);
        for i in 0..segs {
            let j = (i + 1) % segs;
            Self::add_solid_triangle_3d(
                vertices,
                indices,
                camera,
                &b_circle[i],
                n_bot,
                &b_circle[j],
                n_bot,
                base_center,
                n_bot,
                fill_color,
                screen_w,
                screen_h,
            );
            Self::add_solid_triangle_3d(
                vertices,
                indices,
                camera,
                &t_circle[i],
                n_top,
                &t_circle[j],
                n_top,
                top_center,
                n_top,
                fill_color,
                screen_w,
                screen_h,
            );
        }
    }

    fn add_solid_torus(
        vertices: &mut Vec<Vertex>,
        indices: &mut Vec<u32>,
        camera: &Camera3D,
        center: &Point3D,
        r_major: f64,
        r_minor: f64,
        fill_color: Color,
        screen_w: f32,
        screen_h: f32,
    ) {
        let u_steps = 32usize;
        let v_steps = 16usize;
        let mut grid: Vec<Vec<(Point3D, glam::Vec3)>> = Vec::new();
        for i in 0..=u_steps {
            let u = i as f64 / u_steps as f64 * std::f64::consts::TAU;
            let mut row = Vec::new();
            for j in 0..=v_steps {
                let v = j as f64 / v_steps as f64 * std::f64::consts::TAU;
                let x = (r_major + r_minor * v.cos()) * u.cos() + center.x;
                let y = r_minor * v.sin() + center.y;
                let z = (r_major + r_minor * v.cos()) * u.sin() + center.z;
                let nx = (v.cos() * u.cos()) as f32;
                let ny = v.sin() as f32;
                let nz = (v.cos() * u.sin()) as f32;
                row.push((Point3D::new(x, y, z), glam::Vec3::new(nx, ny, nz)));
            }
            grid.push(row);
        }
        let _light_dir = glam::Vec3::new(0.5, 1.0, 0.3).normalize();
        for i in 0..u_steps {
            for j in 0..v_steps {
                let (p00, n00) = grid[i][j];
                let (p10, n10) = grid[i + 1][j];
                let (p01, n01) = grid[i][j + 1];
                let (p11, n11) = grid[i + 1][j + 1];
                Self::add_solid_triangle_3d(
                    vertices, indices, camera, &p00, n00, &p10, n10, &p11, n11, fill_color,
                    screen_w, screen_h,
                );
                Self::add_solid_triangle_3d(
                    vertices, indices, camera, &p00, n00, &p11, n11, &p01, n01, fill_color,
                    screen_w, screen_h,
                );
            }
        }
    }

    fn add_solid_moebius(
        vertices: &mut Vec<Vertex>,
        indices: &mut Vec<u32>,
        camera: &Camera3D,
        center: &Point3D,
        radius: f64,
        width_r: f64,
        fill_color: Color,
        screen_w: f32,
        screen_h: f32,
    ) {
        let u_steps = 64usize;
        let v_steps = 8usize;
        let mut grid: Vec<Vec<(Point3D, glam::Vec3)>> = Vec::new();
        for i in 0..=u_steps {
            let u = i as f64 / u_steps as f64 * std::f64::consts::TAU;
            let mut row = Vec::new();
            for j in 0..=v_steps {
                let v = (j as f64 / v_steps as f64 - 0.5) * 2.0 * width_r;
                let cu = u.cos();
                let su = u.sin();
                let cu2 = (u * 0.5).cos();
                let su2 = (u * 0.5).sin();
                let x = (radius + v * cu2) * cu + center.x;
                let y = v * su2 + center.y;
                let z = (radius + v * cu2) * su + center.z;
                let nx = cu * cu2;
                let ny = su2;
                let nz = su * cu2;
                let mag = (nx * nx + ny * ny + nz * nz).sqrt().max(0.001);
                row.push((
                    Point3D::new(x, y, z),
                    glam::Vec3::new((nx / mag) as f32, (ny / mag) as f32, (nz / mag) as f32),
                ));
            }
            grid.push(row);
        }
        for i in 0..u_steps {
            for j in 0..v_steps {
                let (p00, n00) = grid[i][j];
                let (p10, n10) = grid[i + 1][j];
                let (p01, n01) = grid[i][j + 1];
                let (p11, n11) = grid[i + 1][j + 1];
                Self::add_solid_triangle_3d(
                    vertices, indices, camera, &p00, n00, &p10, n10, &p11, n11, fill_color,
                    screen_w, screen_h,
                );
                Self::add_solid_triangle_3d(
                    vertices, indices, camera, &p00, n00, &p11, n11, &p01, n01, fill_color,
                    screen_w, screen_h,
                );
            }
        }
    }
}
