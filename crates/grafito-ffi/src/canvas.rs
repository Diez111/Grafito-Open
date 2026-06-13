use grafito_render::Renderer;
use parking_lot::Mutex;
use std::ptr::NonNull;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

#[cfg(target_os = "android")]
#[link(name = "android")]
extern "C" {
    fn ANativeWindow_release(window: *mut std::ffi::c_void);
}

/// Raw ANativeWindow pointer wrapped so it can be stored in an object sent over FFI.
#[derive(Clone, Copy)]
#[allow(dead_code)]
struct NativeWindowPtr(*mut std::ffi::c_void);
unsafe impl Send for NativeWindowPtr {}
unsafe impl Sync for NativeWindowPtr {}

#[derive(Debug, thiserror::Error, uniffi::Error)]
pub enum CanvasError {
    #[error("Renderer not initialized")]
    NotInitialized,
    #[error("Surface error: {0}")]
    SurfaceError(String),
    #[error("Render error: {0}")]
    RenderError(String),
}

#[derive(uniffi::Object)]
pub struct CanvasRenderer {
    engine: Arc<crate::bridge::GrafitoEngine>,
    instance: Arc<Mutex<Option<wgpu::Instance>>>,
    surface: Arc<Mutex<Option<wgpu::Surface<'static>>>>,
    device: Arc<Mutex<Option<wgpu::Device>>>,
    queue: Arc<Mutex<Option<wgpu::Queue>>>,
    renderer: Arc<Mutex<Option<Renderer>>>,
    surface_format: Arc<Mutex<wgpu::TextureFormat>>,
    width: Arc<Mutex<u32>>,
    height: Arc<Mutex<u32>>,
    running: Arc<AtomicBool>,
    render_thread: Arc<Mutex<Option<std::thread::JoinHandle<()>>>>,
    native_window: Arc<Mutex<Option<NativeWindowPtr>>>,
}

#[uniffi::export]
impl CanvasRenderer {
    #[uniffi::constructor]
    pub fn new(engine: Arc<crate::bridge::GrafitoEngine>, width: u32, height: u32) -> Arc<Self> {
        #[cfg(target_os = "android")]
        android_logger::init_once(
            android_logger::Config::default()
                .with_max_level(log::LevelFilter::Info)
                .with_tag("GrafitoCanvas"),
        );
        Arc::new(Self {
            engine,
            instance: Arc::new(Mutex::new(None)),
            surface: Arc::new(Mutex::new(None)),
            device: Arc::new(Mutex::new(None)),
            queue: Arc::new(Mutex::new(None)),
            renderer: Arc::new(Mutex::new(None)),
            surface_format: Arc::new(Mutex::new(wgpu::TextureFormat::Bgra8Unorm)),
            width: Arc::new(Mutex::new(width)),
            height: Arc::new(Mutex::new(height)),
            running: Arc::new(AtomicBool::new(false)),
            render_thread: Arc::new(Mutex::new(None)),
            native_window: Arc::new(Mutex::new(None)),
        })
    }

    pub fn init_with_surface(self: &Arc<Self>, surface_ptr: u64) -> Result<(), CanvasError> {
        let instance = wgpu::Instance::new(wgpu::InstanceDescriptor {
            backends: wgpu::Backends::VULKAN | wgpu::Backends::GL,
            ..Default::default()
        });
        let a_native_window = NonNull::new(surface_ptr as *mut std::ffi::c_void)
            .ok_or_else(|| CanvasError::SurfaceError("Invalid surface pointer".into()))?;
        *self.native_window.lock() = Some(NativeWindowPtr(a_native_window.as_ptr()));
        let raw_handle = raw_window_handle::RawWindowHandle::AndroidNdk(
            raw_window_handle::AndroidNdkWindowHandle::new(a_native_window.cast()),
        );
        let raw_display = raw_window_handle::RawDisplayHandle::Android(
            raw_window_handle::AndroidDisplayHandle::new(),
        );
        let surface = unsafe {
            instance
                .create_surface_unsafe(wgpu::SurfaceTargetUnsafe::RawHandle {
                    raw_display_handle: raw_display,
                    raw_window_handle: raw_handle,
                })
                .map_err(|e| CanvasError::SurfaceError(format!("create_surface: {:?}", e)))?
        };
        let adapter = pollster::block_on(instance.request_adapter(&wgpu::RequestAdapterOptions {
            power_preference: wgpu::PowerPreference::LowPower,
            compatible_surface: Some(&surface),
            force_fallback_adapter: false,
        }))
        .ok_or_else(|| CanvasError::SurfaceError("No suitable adapter found".into()))?;
        let (device, queue) = pollster::block_on(adapter.request_device(
            &wgpu::DeviceDescriptor {
                label: Some("Grafito Android"),
                required_features: wgpu::Features::empty(),
                required_limits: wgpu::Limits::downlevel_defaults(),
                memory_hints: wgpu::MemoryHints::default(),
            },
            None,
        ))
        .map_err(|e| CanvasError::SurfaceError(format!("request_device: {:?}", e)))?;
        let caps = surface.get_capabilities(&adapter);
        let format = caps
            .formats
            .first()
            .copied()
            .ok_or_else(|| CanvasError::SurfaceError("No surface formats available".into()))?;
        let alpha_mode = caps
            .alpha_modes
            .first()
            .copied()
            .unwrap_or(wgpu::CompositeAlphaMode::Auto);
        let w = *self.width.lock();
        let h = *self.height.lock();
        surface.configure(
            &device,
            &wgpu::SurfaceConfiguration {
                usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
                format,
                width: w.max(1),
                height: h.max(1),
                present_mode: wgpu::PresentMode::Fifo,
                alpha_mode,
                view_formats: vec![],
                desired_maximum_frame_latency: 2,
            },
        );
        let renderer = Renderer::new(&device, format, false);
        *self.instance.lock() = Some(instance);
        *self.surface.lock() = Some(surface);
        *self.device.lock() = Some(device);
        *self.queue.lock() = Some(queue);
        *self.renderer.lock() = Some(renderer);
        *self.surface_format.lock() = format;
        log::info!("CanvasRenderer initialized ({}x{}, {:?})", w, h, format);
        Ok(())
    }

    pub fn render_frame(self: &Arc<Self>) -> Result<(), CanvasError> {
        log::info!("=== render_frame CALLED ===");

        // ---- Block 1: build geometry (only doc + camera locks) ----
        let doc_arc = self.engine.get_document();
        let cam_arc = self.engine.get_camera();
        let dark_mode = self.engine.get_dark_mode();
        let view_mode = self.engine.get_view_mode();
        let w = *self.width.lock() as f32;
        let h = *self.height.lock() as f32;
        let is_3d = view_mode == "3D";

        log::info!(
            "render_frame: view_mode={}, is_3d={}, screen={}x{}, dark_mode={}",
            view_mode,
            is_3d,
            w,
            h,
            dark_mode
        );

        // Get renderer ref briefly for build_geometry
        let renderer_guard = self.renderer.lock();
        let renderer = renderer_guard.as_ref().ok_or(CanvasError::NotInitialized)?;

        let (vertices, indices, mvp) = {
            let mut doc_guard = doc_arc.lock();
            let mut cam_guard = cam_arc.lock();

            // Fix aspect ratio and screen size on every frame
            cam_guard.aspect = (w / h).max(0.001);
            doc_guard.view_mut().screen_size = glam::Vec2::new(w, h);

            log::info!("Document screen_size: {:?}", doc_guard.view().screen_size);
            log::info!("Object count: {}", doc_guard.objects_iter().count());

            let (v, i) = if is_3d {
                renderer.build_3d_geometry(&doc_guard, &cam_guard, dark_mode, w, h)
            } else {
                renderer.build_geometry(&doc_guard, dark_mode)
            };

            log::info!("Generated {} vertices, {} indices", v.len(), i.len());

            let m = glam::Mat4::orthographic_rh(0.0, w, h, 0.0, -1.0, 1.0);
            (v, i, m)
        };
        drop(renderer_guard);

        // ---- Block 2: GPU commands (device / queue / surface) ----
        let dev_guard = self.device.lock();
        let q_guard = self.queue.lock();
        let s_guard = self.surface.lock();
        let r_guard = self.renderer.lock();
        let device = dev_guard.as_ref().ok_or(CanvasError::NotInitialized)?;
        let queue = q_guard.as_ref().ok_or(CanvasError::NotInitialized)?;
        let surface = s_guard.as_ref().ok_or(CanvasError::NotInitialized)?;
        let renderer = r_guard.as_ref().ok_or(CanvasError::NotInitialized)?;

        renderer.update_mvp(queue, mvp);

        let output = surface
            .get_current_texture()
            .map_err(|e| CanvasError::SurfaceError(format!("get_current_texture: {:?}", e)))?;
        let view = output
            .texture
            .create_view(&wgpu::TextureViewDescriptor::default());

        let bg = if dark_mode {
            wgpu::Color {
                r: 0.12,
                g: 0.12,
                b: 0.12,
                a: 1.0,
            }
        } else {
            wgpu::Color {
                r: 0.96,
                g: 0.96,
                b: 0.96,
                a: 1.0,
            }
        };

        let mut encoder = device.create_command_encoder(&wgpu::CommandEncoderDescriptor {
            label: Some("Render Encoder"),
        });

        if !indices.is_empty() {
            let vb = device.create_buffer(&wgpu::BufferDescriptor {
                label: Some("Vertex"),
                size: (vertices.len() * std::mem::size_of::<grafito_render::Vertex>()) as u64,
                usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
                mapped_at_creation: false,
            });
            let ib = device.create_buffer(&wgpu::BufferDescriptor {
                label: Some("Index"),
                size: (indices.len() * std::mem::size_of::<u32>()) as u64,
                usage: wgpu::BufferUsages::INDEX | wgpu::BufferUsages::COPY_DST,
                mapped_at_creation: false,
            });
            queue.write_buffer(&vb, 0, bytemuck::cast_slice(&vertices));
            queue.write_buffer(&ib, 0, bytemuck::cast_slice(&indices));

            {
                let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                    label: Some("Render Pass"),
                    color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                        view: &view,
                        resolve_target: None,
                        ops: wgpu::Operations {
                            load: wgpu::LoadOp::Clear(bg),
                            store: wgpu::StoreOp::Store,
                        },
                    })],
                    depth_stencil_attachment: None,
                    timestamp_writes: None,
                    occlusion_query_set: None,
                });
                let p = if is_3d {
                    &renderer.pipeline_3d
                } else {
                    &renderer.pipeline
                };
                pass.set_pipeline(p);
                pass.set_bind_group(0, &renderer.mvp_bind_group, &[]);
                pass.set_vertex_buffer(0, vb.slice(..));
                pass.set_index_buffer(ib.slice(..), wgpu::IndexFormat::Uint32);
                pass.draw_indexed(0..indices.len() as u32, 0, 0..1);
            }
        } else {
            {
                let _pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                    label: Some("Clear"),
                    color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                        view: &view,
                        resolve_target: None,
                        ops: wgpu::Operations {
                            load: wgpu::LoadOp::Clear(bg),
                            store: wgpu::StoreOp::Store,
                        },
                    })],
                    depth_stencil_attachment: None,
                    timestamp_writes: None,
                    occlusion_query_set: None,
                });
            }
        }
        queue.submit(std::iter::once(encoder.finish()));
        output.present();
        Ok(())
    }

    pub fn start_render_loop(self: &Arc<Self>) {
        self.running.store(true, Ordering::SeqCst);
        let s = self.clone();
        let h = std::thread::spawn(move || {
            let d = std::time::Duration::from_micros(16667);
            while s.running.load(Ordering::SeqCst) {
                let t0 = std::time::Instant::now();
                let result =
                    std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| s.render_frame()));
                match result {
                    Ok(Ok(())) => {}
                    Ok(Err(e)) => log::error!("Render error: {}", e),
                    Err(_) => {
                        log::error!("Render thread panicked; stopping render loop");
                        s.running.store(false, Ordering::SeqCst);
                    }
                }
                let dt = t0.elapsed();
                if dt < d {
                    std::thread::sleep(d - dt);
                }
            }
        });
        *self.render_thread.lock() = Some(h);
    }

    pub fn stop_render_loop(self: &Arc<Self>) {
        self.running.store(false, Ordering::SeqCst);
        if let Some(h) = self.render_thread.lock().take() {
            let _ = h.join();
        }
    }

    pub fn resize(self: &Arc<Self>, width: u32, height: u32) {
        *self.width.lock() = width;
        *self.height.lock() = height;
        let device = self.device.lock();
        let surface = self.surface.lock();
        if let (Some(device), Some(surface)) = (device.as_ref(), surface.as_ref()) {
            let fmt = *self.surface_format.lock();
            surface.configure(
                device,
                &wgpu::SurfaceConfiguration {
                    usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
                    format: fmt,
                    width: width.max(1),
                    height: height.max(1),
                    present_mode: wgpu::PresentMode::Fifo,
                    alpha_mode: wgpu::CompositeAlphaMode::Auto,
                    view_formats: vec![],
                    desired_maximum_frame_latency: 2,
                },
            );
        }
    }

    pub fn cleanup(self: &Arc<Self>) {
        self.stop_render_loop();
        *self.renderer.lock() = None;
        *self.surface.lock() = None;
        *self.queue.lock() = None;
        *self.device.lock() = None;
        *self.instance.lock() = None;
        #[cfg(target_os = "android")]
        if let Some(window) = self.native_window.lock().take() {
            unsafe { ANativeWindow_release(window.0) };
        }
    }

    pub fn get_width(self: &Arc<Self>) -> u32 {
        *self.width.lock()
    }
    pub fn get_height(self: &Arc<Self>) -> u32 {
        *self.height.lock()
    }
}
