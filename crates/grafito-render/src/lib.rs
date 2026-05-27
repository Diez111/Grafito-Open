//! Grafito Render — GPU-accelerated 2D renderer using wgpu.

use grafito_geometry::{Color, Point2, ViewTransform};
use grafito_core::{Document, GeoObject};
use wgpu::util::DeviceExt;

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
    pub device: wgpu::Device,
    pub queue: wgpu::Queue,
    pub pipeline: wgpu::RenderPipeline,
    pub mvp_bind_group_layout: wgpu::BindGroupLayout,
    pub mvp_buffer: wgpu::Buffer,
    pub mvp_bind_group: wgpu::BindGroup,
}

impl Renderer {
    pub async fn new(
        adapter: &wgpu::Adapter,
        surface: &wgpu::Surface<'static>,
        _width: u32,
        _height: u32,
    ) -> Self {
        let (device, queue) = adapter
            .request_device(
                &wgpu::DeviceDescriptor {
                    required_features: wgpu::Features::empty(),
                    required_limits: wgpu::Limits::default(),
                    label: Some("Grafito Device"),
                    memory_hints: wgpu::MemoryHints::Performance,
                },
                None,
            )
            .await
            .unwrap();

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

        let pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("Render Pipeline"),
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
                    format: surface.get_capabilities(adapter).formats[0],
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
            multisample: wgpu::MultisampleState::default(),
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
            device,
            queue,
            pipeline,
            mvp_bind_group_layout,
            mvp_buffer,
            mvp_bind_group,
        }
    }

    pub fn resize(&self, _width: u32, _height: u32) {
        // Surface reconfiguration handled by app
    }

    pub fn update_mvp(&self, mvp: glam::Mat4) {
        self.queue.write_buffer(
            &self.mvp_buffer,
            0,
            bytemuck::cast_slice(&mvp.to_cols_array()),
        );
    }

    pub fn render(
        &self,
        view: &wgpu::TextureView,
        document: &Document,
        screen_width: f32,
        screen_height: f32,
    ) {
        let mut vertices = Vec::new();
        let mut indices = Vec::new();

        let view_transform = document.view().clone();
        let mvp = view_transform.mvp_matrix();
        self.update_mvp(mvp);

        // Grid
        self.build_grid(&mut vertices, &mut indices, &view_transform, screen_width, screen_height);

        // Axes
        self.build_axes(&mut vertices, &mut indices, &view_transform, screen_width, screen_height);

        // Objects
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
                    let center = view_transform.world_to_screen(c.center);
                    let radius = (c.radius as f32) * view_transform.scale;
                    Self::add_circle_stroke(&mut vertices, &mut indices, center, radius, c.width, c.color);
                    if let Some(fill) = c.fill_color {
                        Self::add_circle_fill(&mut vertices, &mut indices, center, radius, fill);
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
                _ => {}
            }
        }

        if vertices.is_empty() {
            // Clear to white even if empty
            let mut encoder = self.device.create_command_encoder(&wgpu::CommandEncoderDescriptor {
                label: Some("Render Encoder"),
            });
            {
                let _render_pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                    label: Some("Clear Pass"),
                    color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                        view,
                        resolve_target: None,
                        ops: wgpu::Operations {
                            load: wgpu::LoadOp::Clear(wgpu::Color {
                                r: 1.0,
                                g: 1.0,
                                b: 1.0,
                                a: 1.0,
                            }),
                            store: wgpu::StoreOp::Store,
                        },
                    })],
                    depth_stencil_attachment: None,
                    timestamp_writes: None,
                    occlusion_query_set: None,
                });
            }
            self.queue.submit(std::iter::once(encoder.finish()));
            return;
        }

        let vertex_buffer = self.device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("Vertex Buffer"),
            contents: bytemuck::cast_slice(&vertices),
            usage: wgpu::BufferUsages::VERTEX,
        });

        let index_buffer = self.device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("Index Buffer"),
            contents: bytemuck::cast_slice(&indices),
            usage: wgpu::BufferUsages::INDEX,
        });

        let mut encoder = self.device.create_command_encoder(&wgpu::CommandEncoderDescriptor {
            label: Some("Render Encoder"),
        });

        {
            let mut render_pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("Render Pass"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view,
                    resolve_target: None,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Clear(wgpu::Color {
                            r: 1.0,
                            g: 1.0,
                            b: 1.0,
                            a: 1.0,
                        }),
                        store: wgpu::StoreOp::Store,
                    },
                })],
                depth_stencil_attachment: None,
                timestamp_writes: None,
                occlusion_query_set: None,
            });

            render_pass.set_pipeline(&self.pipeline);
            render_pass.set_bind_group(0, &self.mvp_bind_group, &[]);
            render_pass.set_vertex_buffer(0, vertex_buffer.slice(..));
            render_pass.set_index_buffer(index_buffer.slice(..), wgpu::IndexFormat::Uint32);
            render_pass.draw_indexed(0..(indices.len() as u32), 0, 0..1);
        }

        self.queue.submit(std::iter::once(encoder.finish()));
    }

    fn build_grid(
        &self,
        vertices: &mut Vec<Vertex>,
        indices: &mut Vec<u32>,
        view: &ViewTransform,
        _screen_width: f32,
        _screen_height: f32,
    ) {
        let world_tl = view.screen_to_world(glam::Vec2::new(0.0, 0.0));
        let world_br = view.screen_to_world(view.screen_size);

        let min_x = world_tl.x.floor() as i32 - 1;
        let max_x = world_br.x.ceil() as i32 + 1;
        let min_y = world_br.y.floor() as i32 - 1;
        let max_y = world_tl.y.ceil() as i32 + 1;

        let color = Color::LIGHT_GRAY;

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

    fn build_axes(
        &self,
        vertices: &mut Vec<Vertex>,
        indices: &mut Vec<u32>,
        view: &ViewTransform,
        _screen_width: f32,
        _screen_height: f32,
    ) {
        let world_tl = view.screen_to_world(glam::Vec2::new(0.0, 0.0));
        let world_br = view.screen_to_world(view.screen_size);

        let x_axis_y = 0.0f64.clamp(world_br.y, world_tl.y);
        let y_axis_x = 0.0f64.clamp(world_tl.x, world_br.x);

        let x_axis_a = view.world_to_screen(Point2::new(world_tl.x, x_axis_y));
        let x_axis_b = view.world_to_screen(Point2::new(world_br.x, x_axis_y));
        Self::add_line_segment(vertices, indices, x_axis_a, x_axis_b, 2.0, Color::BLACK);

        let y_axis_a = view.world_to_screen(Point2::new(y_axis_x, world_br.y));
        let y_axis_b = view.world_to_screen(Point2::new(y_axis_x, world_tl.y));
        Self::add_line_segment(vertices, indices, y_axis_a, y_axis_b, 2.0, Color::BLACK);
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
}
