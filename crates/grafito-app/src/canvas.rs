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

#[derive(Debug, Clone, PartialEq)]
pub struct Cache2DKey {
    pub version: u64,
    pub view: grafito_geometry::ViewTransform,
    pub dark_mode: bool,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Cache3DKey {
    pub version: u64,
    pub camera: Camera3D,
    pub dark_mode: bool,
    pub screen_w: f32,
    pub screen_h: f32,
}

pub struct GpuCanvasResources {
    pub renderer: Arc<RwLock<Option<Renderer>>>,
    pub buffers_2d: Option<PersistentBuffers>,
    pub buffers_3d: Option<PersistentBuffers>,
    pub cache_2d: Option<Cache2DKey>,
    pub cache_3d: Option<Cache3DKey>,
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

        let Some(resources) = callback_resources.get_mut::<GpuCanvasResources>() else {
            log::warn!("GpuCanvasResources not registered in prepare (2D)");
            return vec![];
        };

        let current_key = Cache2DKey {
            version: self.document.version,
            view: *self.document.view(),
            dark_mode: self.dark_mode,
        };

        if resources.buffers_2d.is_some() && resources.cache_2d.as_ref() == Some(&current_key) {
            log::debug!("CanvasCallback prepare (2D): cache hit!");
            return vec![];
        }

        let (vertices, indices) = {
            let Ok(renderer_lock) = resources.renderer.read() else {
                log::warn!("Renderer lock poisoned in prepare (2D)");
                return vec![];
            };
            let Some(renderer) = renderer_lock.as_ref() else {
                return vec![]; // Still compiling in background
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

            // GPU computing for objects using a single-pass objects_iter
            {
                #[cfg(feature = "profile")]
                puffin::profile_scope!("gpu_compute_single_pass");
                let implicit_comp = renderer.implicit_compute.as_ref();
                let function_comp = renderer.function_compute.as_ref();
                let parametric_comp = renderer.parametric_compute.as_ref();
                let vector_comp = renderer.vector_compute.as_ref();

                let view = *self.document.view();
                let world_tl = view.screen_to_world(glam::Vec2::new(0.0, 0.0));
                let world_br = view.screen_to_world(view.screen_size);
                let function_grid_size =
                    grafito_core::function_sampling::recommended_grid_size_for_quality(
                        view.screen_size.x,
                        self.document.render_quality,
                    );

                for (_, obj) in self.document.objects_iter() {
                    match obj {
                        grafito_core::GeoObject::ImplicitCurve(ic) => {
                            if let Some(compute) = implicit_comp {
                                let _ = grafito_render::implicit_compute::maybe_compute_on_gpu(
                                    compute,
                                    device,
                                    queue,
                                    ic,
                                    &view,
                                    &self.document.variables,
                                    self.document.render_quality,
                                );
                            }
                        }
                        grafito_core::GeoObject::Function(fun) => {
                            if let Some(compute) = function_comp {
                                let min_x = self.document.resolve_expr(
                                    &fun.domain_min_expr,
                                    fun.domain_min.unwrap_or(world_tl.x),
                                );
                                let max_x = self.document.resolve_expr(
                                    &fun.domain_max_expr,
                                    fun.domain_max.unwrap_or(world_br.x),
                                );
                                let domain = (min_x, max_x);
                                let _ =
                                    grafito_render::function_compute::maybe_compute_function_on_gpu(
                                        compute,
                                        device,
                                        queue,
                                        fun,
                                        domain,
                                        function_grid_size,
                                        &self.document.variables,
                                    );
                            }
                        }
                        grafito_core::GeoObject::ParametricCurve2D(pc) => {
                            if let Some(compute) = parametric_comp {
                                let _ = grafito_render::parametric_compute::maybe_compute_curve_2d_on_gpu(
                                    compute,
                                    device,
                                    queue,
                                    pc,
                                    4000,
                                    &self.document.variables,
                                );
                            }
                        }
                        grafito_core::GeoObject::ParametricCurve3D(pc) => {
                            if let Some(compute) = parametric_comp {
                                let _ = grafito_render::parametric_compute::maybe_compute_curve_3d_on_gpu(
                                    compute,
                                    device,
                                    queue,
                                    pc,
                                    4000,
                                    &self.document.variables,
                                );
                            }
                        }
                        grafito_core::GeoObject::PolarCurve(pol) => {
                            if let Some(compute) = parametric_comp {
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
                        }
                        grafito_core::GeoObject::Surface3D(su) => {
                            if let Some(compute) = parametric_comp {
                                let res = su.mesh_res.min(128);
                                let _ = grafito_render::parametric_compute::maybe_compute_surface_on_gpu(
                                    compute,
                                    device,
                                    queue,
                                    su,
                                    res,
                                    &self.document.variables,
                                );
                            }
                        }
                        grafito_core::GeoObject::VectorField2D(vf) => {
                            if let Some(compute) = vector_comp {
                                let _ = grafito_render::vector_compute::maybe_compute_vector_field_on_gpu(
                                    compute,
                                    device,
                                    queue,
                                    vf,
                                    &view,
                                    &self.document.variables,
                                );
                            }
                        }
                        _ => {}
                    }
                }
            }

            #[cfg(feature = "profile")]
            puffin::profile_scope!("geometry_build");
            renderer.build_geometry(&self.document, self.dark_mode, false)
        };

        log::debug!(
            "CanvasCallback geometry: vertices={} indices={}",
            vertices.len(),
            indices.len()
        );

        let Some(resources) = callback_resources.get_mut::<GpuCanvasResources>() else {
            log::warn!("GpuCanvasResources not registered in prepare (2D, buffers)");
            return vec![];
        };

        if vertices.is_empty() {
            resources.cache_2d = Some(current_key);
            if let Some(buffers) = &mut resources.buffers_2d {
                buffers.index_count = 0;
            }
            return vec![];
        }

        let vertex_data = bytemuck::cast_slice(&vertices);
        let index_data = bytemuck::cast_slice(&indices);
        let vertex_size = vertex_data.len();
        let index_size = index_data.len();

        let encoder = device.create_command_encoder(&wgpu::CommandEncoderDescriptor {
            label: Some("Canvas Callback Encoder"),
        });

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

        resources.cache_2d = Some(current_key);
        vec![encoder.finish()]
    }

    fn paint(
        &self,
        info: PaintCallbackInfo,
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

        if let Ok(renderer_lock) = resources.renderer.read() {
            if let Some(renderer) = renderer_lock.as_ref() {
                log::debug!("CanvasCallback paint: index_count={}", buffers.index_count);
                render_pass.set_viewport(
                    info.clip_rect.min.x,
                    info.clip_rect.min.y,
                    info.clip_rect.width(),
                    info.clip_rect.height(),
                    0.0,
                    1.0,
                );
                render_pass.set_pipeline(&renderer.pipeline);
                render_pass.set_bind_group(0, &renderer.mvp_bind_group, &[]);
                render_pass.set_vertex_buffer(0, buffers.vertex_buffer.slice(..));
                render_pass
                    .set_index_buffer(buffers.index_buffer.slice(..), wgpu::IndexFormat::Uint32);
                render_pass.draw_indexed(0..buffers.index_count, 0, 0..1);
            }
        }
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

        let Some(resources) = callback_resources.get_mut::<GpuCanvasResources>() else {
            log::warn!("GpuCanvasResources not registered in prepare (3D)");
            return vec![];
        };

        let current_key = Cache3DKey {
            version: self.document.version,
            camera: self.camera,
            dark_mode: self.dark_mode,
            screen_w: self.screen_w,
            screen_h: self.screen_h,
        };

        if resources.buffers_3d.is_some() && resources.cache_3d.as_ref() == Some(&current_key) {
            log::debug!("Canvas3DCallback prepare: cache hit!");
            return vec![];
        }

        let (vertices, indices) = {
            let Ok(renderer_lock) = resources.renderer.read() else {
                log::warn!("Renderer lock poisoned in prepare (3D)");
                return vec![];
            };
            let Some(renderer) = renderer_lock.as_ref() else {
                return vec![]; // Still compiling in background
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
            puffin::profile_scope!("gpu_compute_3d");
            let parametric_comp = renderer.parametric_compute.as_ref();
            for (_, obj) in self.document.objects_iter() {
                match obj {
                    grafito_core::GeoObject::ParametricCurve3D(pc) => {
                        if let Some(compute) = parametric_comp {
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
                    }
                    grafito_core::GeoObject::Surface3D(su) => {
                        if let Some(compute) = parametric_comp {
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
                    }
                    _ => {}
                }
            }

            #[cfg(feature = "profile")]
            puffin::profile_scope!("geometry_build_3d");
            renderer.build_3d_geometry(
                &self.document,
                &self.camera,
                self.dark_mode,
                self.screen_w,
                self.screen_h,
                false,
            )
        };

        log::debug!(
            "Canvas3DCallback geometry: vertices={} indices={}",
            vertices.len(),
            indices.len()
        );

        let Some(resources) = callback_resources.get_mut::<GpuCanvasResources>() else {
            log::warn!("GpuCanvasResources not registered in prepare (3D, buffers)");
            return vec![];
        };

        if vertices.is_empty() {
            resources.cache_3d = Some(current_key);
            if let Some(buffers) = &mut resources.buffers_3d {
                buffers.index_count = 0;
            }
            return vec![];
        }

        let vertex_data = bytemuck::cast_slice(&vertices);
        let index_data = bytemuck::cast_slice(&indices);
        let vertex_size = vertex_data.len();
        let index_size = index_data.len();

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

        resources.cache_3d = Some(current_key);
        vec![]
    }

    fn paint(
        &self,
        info: PaintCallbackInfo,
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

        let Ok(renderer_lock) = resources.renderer.read() else {
            return;
        };
        let Some(renderer) = renderer_lock.as_ref() else {
            return;
        };

        // egui-wgpu already sets the viewport/scissor to the PaintCallback rect
        // before invoking this callback, so we render directly into that region.
        // Wait, egui-wgpu sets the viewport to the FULL SCREEN, and scissor to the rect!
        // We must set the viewport to the rect so clip space maps correctly!
        render_pass.set_viewport(
            info.clip_rect.min.x,
            info.clip_rect.min.y,
            info.clip_rect.width(),
            info.clip_rect.height(),
            0.0,
            1.0,
        );
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
