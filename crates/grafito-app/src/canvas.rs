//! GPU-backed canvas callbacks.
//!
//! Sets up the shared `GpuCanvasResources`, builds 2D/3D geometry through the
//! `grafito_render` pipeline, and issues the `egui_wgpu` paint callbacks used
//! by the central canvas.

use egui::epaint::PaintCallbackInfo;
use egui_wgpu::CallbackTrait;
use grafito_core::Document;
use grafito_geometry::Camera3D;
use grafito_render::Renderer;
use std::sync::Arc;
use std::sync::RwLock;

pub struct GpuCanvasResources {
    pub renderer: Arc<RwLock<Renderer>>,
    pub buffers_2d: Option<PersistentBuffers>,
    pub buffers_3d: Option<PersistentBuffers>,
}

pub struct PersistentBuffers {
    pub vertex_buffer: wgpu::Buffer,
    pub index_buffer: wgpu::Buffer,
    pub vertex_capacity: usize,
    pub index_capacity: usize,
    pub index_count: u32,
}

pub struct CanvasCallback {
    pub document: Arc<Document>,
    pub dark_mode: bool,
}

impl CallbackTrait for CanvasCallback {
    fn prepare(
        &self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        _screen_descriptor: &egui_wgpu::ScreenDescriptor,
        _encoder: &mut wgpu::CommandEncoder,
        callback_resources: &mut egui_wgpu::CallbackResources,
    ) -> Vec<wgpu::CommandBuffer> {
        #[cfg(feature = "profile")]
        puffin::profile_scope!("canvas_prepare");

        let (vertices, indices) = {
            let Some(resources) = callback_resources.get::<GpuCanvasResources>() else {
                log::warn!("GpuCanvasResources not registered in prepare (2D)");
                return vec![];
            };
            let Ok(renderer) = resources.renderer.read() else {
                log::warn!("Renderer lock poisoned in prepare (2D)");
                return vec![];
            };

            let sw = self.document.view().screen_size.x;
            let sh = self.document.view().screen_size.y;
            let mvp = glam::Mat4::orthographic_rh(0.0, sw, sh, 0.0, -1.0, 1.0);
            renderer.update_mvp(queue, mvp);

            log::debug!(
                "CanvasCallback prepare: screen={}x{} objects={}",
                sw,
                sh,
                self.document.object_count()
            );

            // Try to evaluate implicit curves on the GPU before building geometry.
            // If a curve cannot be compiled to GPU bytecode, the geometry builder
            // will fall back to the CPU evaluator through the per-object cache.
            {
                #[cfg(feature = "profile")]
                puffin::profile_scope!("gpu_compute_implicit");
                if let Some(compute) = renderer.implicit_compute.as_ref() {
                    for (_, obj) in self.document.objects_iter() {
                        if let grafito_core::GeoObject::ImplicitCurve(ic) = obj {
                            let _ = grafito_render::implicit_compute::maybe_compute_on_gpu(
                                compute,
                                device,
                                queue,
                                ic,
                                self.document.view(),
                                &self.document.variables,
                                self.document.render_quality,
                            );
                        }
                    }
                }
            }

            // Try to evaluate 1D functions on the GPU as well.
            {
                #[cfg(feature = "profile")]
                puffin::profile_scope!("gpu_compute_function");
                if let Some(compute) = renderer.function_compute.as_ref() {
                    let view = *self.document.view();
                    let world_tl = view.screen_to_world(glam::Vec2::new(0.0, 0.0));
                    let world_br = view.screen_to_world(view.screen_size);
                    let grid_size =
                        grafito_core::function_sampling::recommended_grid_size_for_quality(
                            view.screen_size.x,
                            self.document.render_quality,
                        );
                    for (_, obj) in self.document.objects_iter() {
                        if let grafito_core::GeoObject::Function(fun) = obj {
                            let domain = (
                                fun.domain_min.unwrap_or(world_tl.x),
                                fun.domain_max.unwrap_or(world_br.x),
                            );
                            let _ = grafito_render::function_compute::maybe_compute_function_on_gpu(
                                compute,
                                device,
                                queue,
                                fun,
                                domain,
                                grid_size,
                                &self.document.variables,
                            );
                        }
                    }
                }
            }

            // Try to evaluate parametric curves and surfaces on the GPU.
            {
                #[cfg(feature = "profile")]
                puffin::profile_scope!("gpu_compute_parametric");
                if let Some(compute) = renderer.parametric_compute.as_ref() {
                    for (_, obj) in self.document.objects_iter() {
                        match obj {
                            grafito_core::GeoObject::ParametricCurve2D(pc) => {
                                let _ =
                                grafito_render::parametric_compute::maybe_compute_curve_2d_on_gpu(
                                    compute,
                                    device,
                                    queue,
                                    pc,
                                    4000,
                                    &self.document.variables,
                                );
                            }
                            grafito_core::GeoObject::ParametricCurve3D(pc) => {
                                let _ =
                                grafito_render::parametric_compute::maybe_compute_curve_3d_on_gpu(
                                    compute,
                                    device,
                                    queue,
                                    pc,
                                    4000,
                                    &self.document.variables,
                                );
                            }
                            grafito_core::GeoObject::PolarCurve(pol) => {
                                let _ =
                                    grafito_render::parametric_compute::maybe_compute_polar_on_gpu(
                                        compute,
                                        device,
                                        queue,
                                        pol,
                                        4000,
                                        &self.document.variables,
                                    );
                            }
                            grafito_core::GeoObject::Surface3D(su) => {
                                let res = su.mesh_res.min(128);
                                let _ =
                                grafito_render::parametric_compute::maybe_compute_surface_on_gpu(
                                    compute,
                                    device,
                                    queue,
                                    su,
                                    res,
                                    &self.document.variables,
                                );
                            }
                            _ => {}
                        }
                    }
                }
            }

            // Try to evaluate 2D vector fields on the GPU.
            {
                #[cfg(feature = "profile")]
                puffin::profile_scope!("gpu_compute_vector_field");
                if let Some(compute) = renderer.vector_compute.as_ref() {
                    for (_, obj) in self.document.objects_iter() {
                        if let grafito_core::GeoObject::VectorField2D(vf) = obj {
                            let _ =
                                grafito_render::vector_compute::maybe_compute_vector_field_on_gpu(
                                    compute,
                                    device,
                                    queue,
                                    vf,
                                    self.document.view(),
                                    &self.document.variables,
                                );
                        }
                    }
                }
            }

            #[cfg(feature = "profile")]
            puffin::profile_scope!("geometry_build");
            renderer.build_geometry(&self.document, self.dark_mode)
        };

        log::debug!(
            "CanvasCallback geometry: vertices={} indices={}",
            vertices.len(),
            indices.len()
        );

        if vertices.is_empty() {
            return vec![];
        }

        let vertex_data = bytemuck::cast_slice(&vertices);
        let index_data = bytemuck::cast_slice(&indices);
        let vertex_size = vertex_data.len();
        let index_size = index_data.len();

        let Some(resources) = callback_resources.get_mut::<GpuCanvasResources>() else {
            log::warn!("GpuCanvasResources not registered in prepare (2D, buffers)");
            return vec![];
        };

        let buffers = resources.buffers_2d.get_or_insert_with(|| {
            let vb = device.create_buffer(&wgpu::BufferDescriptor {
                label: Some("Canvas 2D Vertex Buffer"),
                size: (vertex_size * 2) as u64,
                usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
                mapped_at_creation: false,
            });
            let ib = device.create_buffer(&wgpu::BufferDescriptor {
                label: Some("Canvas 2D Index Buffer"),
                size: (index_size * 2) as u64,
                usage: wgpu::BufferUsages::INDEX | wgpu::BufferUsages::COPY_DST,
                mapped_at_creation: false,
            });
            PersistentBuffers {
                vertex_buffer: vb,
                index_buffer: ib,
                vertex_capacity: vertex_size * 2,
                index_capacity: index_size * 2,
                index_count: 0,
            }
        });

        if vertex_size > buffers.vertex_capacity {
            let new_capacity = vertex_size * 2;
            buffers.vertex_buffer = device.create_buffer(&wgpu::BufferDescriptor {
                label: Some("Canvas 2D Vertex Buffer"),
                size: new_capacity as u64,
                usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
                mapped_at_creation: false,
            });
            buffers.vertex_capacity = new_capacity;
        }

        if index_size > buffers.index_capacity {
            let new_capacity = index_size * 2;
            buffers.index_buffer = device.create_buffer(&wgpu::BufferDescriptor {
                label: Some("Canvas 2D Index Buffer"),
                size: new_capacity as u64,
                usage: wgpu::BufferUsages::INDEX | wgpu::BufferUsages::COPY_DST,
                mapped_at_creation: false,
            });
            buffers.index_capacity = new_capacity;
        }

        queue.write_buffer(&buffers.vertex_buffer, 0, vertex_data);
        queue.write_buffer(&buffers.index_buffer, 0, index_data);
        buffers.index_count = indices.len() as u32;

        vec![]
    }

    fn paint(
        &self,
        _info: PaintCallbackInfo,
        render_pass: &mut wgpu::RenderPass<'static>,
        callback_resources: &egui_wgpu::CallbackResources,
    ) {
        #[cfg(feature = "profile")]
        puffin::profile_scope!("canvas_paint");

        let Some(resources) = callback_resources.get::<GpuCanvasResources>() else {
            return;
        };
        let Some(buffers) = &resources.buffers_2d else {
            return;
        };

        if buffers.index_count == 0 {
            return;
        }

        let Ok(renderer) = resources.renderer.read() else {
            return;
        };

        // egui-wgpu already sets the viewport/scissor to the PaintCallback rect
        // before invoking this callback, so we render directly into that region.
        log::debug!("CanvasCallback paint: index_count={}", buffers.index_count);
        render_pass.set_pipeline(&renderer.pipeline);
        render_pass.set_bind_group(0, &renderer.mvp_bind_group, &[]);
        render_pass.set_vertex_buffer(0, buffers.vertex_buffer.slice(..));
        render_pass.set_index_buffer(buffers.index_buffer.slice(..), wgpu::IndexFormat::Uint32);
        render_pass.draw_indexed(0..buffers.index_count, 0, 0..1);
    }
}

pub struct Canvas3DCallback {
    pub document: Arc<Document>,
    pub camera: Camera3D,
    pub dark_mode: bool,
    pub screen_w: f32,
    pub screen_h: f32,
}

impl CallbackTrait for Canvas3DCallback {
    fn prepare(
        &self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        _screen_descriptor: &egui_wgpu::ScreenDescriptor,
        _encoder: &mut wgpu::CommandEncoder,
        callback_resources: &mut egui_wgpu::CallbackResources,
    ) -> Vec<wgpu::CommandBuffer> {
        #[cfg(feature = "profile")]
        puffin::profile_scope!("canvas_prepare_3d");

        let (vertices, indices) = {
            let Some(resources) = callback_resources.get::<GpuCanvasResources>() else {
                log::warn!("GpuCanvasResources not registered in prepare (3D)");
                return vec![];
            };
            let Ok(renderer) = resources.renderer.read() else {
                log::warn!("Renderer lock poisoned in prepare (3D)");
                return vec![];
            };

            let mvp =
                glam::Mat4::orthographic_rh(0.0, self.screen_w, self.screen_h, 0.0, -1.0, 1.0);
            renderer.update_mvp(queue, mvp);

            log::debug!(
                "Canvas3DCallback prepare: screen={}x{} objects={}",
                self.screen_w,
                self.screen_h,
                self.document.object_count()
            );

            #[cfg(feature = "profile")]
            puffin::profile_scope!("geometry_build_3d");
            renderer.build_3d_geometry(
                &self.document,
                &self.camera,
                self.dark_mode,
                self.screen_w,
                self.screen_h,
            )
        };

        log::debug!(
            "Canvas3DCallback geometry: vertices={} indices={}",
            vertices.len(),
            indices.len()
        );

        if vertices.is_empty() {
            return vec![];
        }

        let vertex_data = bytemuck::cast_slice(&vertices);
        let index_data = bytemuck::cast_slice(&indices);
        let vertex_size = vertex_data.len();
        let index_size = index_data.len();

        let Some(resources) = callback_resources.get_mut::<GpuCanvasResources>() else {
            log::warn!("GpuCanvasResources not registered in prepare (3D, buffers)");
            return vec![];
        };

        let buffers = resources.buffers_3d.get_or_insert_with(|| {
            let vb = device.create_buffer(&wgpu::BufferDescriptor {
                label: Some("Canvas 3D Vertex Buffer"),
                size: (vertex_size * 2) as u64,
                usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
                mapped_at_creation: false,
            });
            let ib = device.create_buffer(&wgpu::BufferDescriptor {
                label: Some("Canvas 3D Index Buffer"),
                size: (index_size * 2) as u64,
                usage: wgpu::BufferUsages::INDEX | wgpu::BufferUsages::COPY_DST,
                mapped_at_creation: false,
            });
            PersistentBuffers {
                vertex_buffer: vb,
                index_buffer: ib,
                vertex_capacity: vertex_size * 2,
                index_capacity: index_size * 2,
                index_count: 0,
            }
        });

        if vertex_size > buffers.vertex_capacity {
            let new_capacity = vertex_size * 2;
            buffers.vertex_buffer = device.create_buffer(&wgpu::BufferDescriptor {
                label: Some("Canvas 3D Vertex Buffer"),
                size: new_capacity as u64,
                usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
                mapped_at_creation: false,
            });
            buffers.vertex_capacity = new_capacity;
        }

        if index_size > buffers.index_capacity {
            let new_capacity = index_size * 2;
            buffers.index_buffer = device.create_buffer(&wgpu::BufferDescriptor {
                label: Some("Canvas 3D Index Buffer"),
                size: new_capacity as u64,
                usage: wgpu::BufferUsages::INDEX | wgpu::BufferUsages::COPY_DST,
                mapped_at_creation: false,
            });
            buffers.index_capacity = new_capacity;
        }

        queue.write_buffer(&buffers.vertex_buffer, 0, vertex_data);
        queue.write_buffer(&buffers.index_buffer, 0, index_data);
        buffers.index_count = indices.len() as u32;

        vec![]
    }

    fn paint(
        &self,
        _info: PaintCallbackInfo,
        render_pass: &mut wgpu::RenderPass<'static>,
        callback_resources: &egui_wgpu::CallbackResources,
    ) {
        #[cfg(feature = "profile")]
        puffin::profile_scope!("canvas_paint_3d");

        let Some(resources) = callback_resources.get::<GpuCanvasResources>() else {
            return;
        };
        let Some(buffers) = &resources.buffers_3d else {
            return;
        };

        if buffers.index_count == 0 {
            return;
        }

        let Ok(renderer) = resources.renderer.read() else {
            return;
        };

        // egui-wgpu already sets the viewport/scissor to the PaintCallback rect
        // before invoking this callback, so we render directly into that region.
        log::debug!(
            "Canvas3DCallback paint: index_count={}",
            buffers.index_count
        );
        render_pass.set_pipeline(&renderer.pipeline_3d);
        render_pass.set_bind_group(0, &renderer.mvp_bind_group, &[]);
        render_pass.set_vertex_buffer(0, buffers.vertex_buffer.slice(..));
        render_pass.set_index_buffer(buffers.index_buffer.slice(..), wgpu::IndexFormat::Uint32);
        render_pass.draw_indexed(0..buffers.index_count, 0, 0..1);
    }
}
