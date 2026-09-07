//! GPU-backed canvas callbacks.
//!
//! Sets up the shared `GpuCanvasResources`, builds 2D/3D geometry through the
//! `grafito_render` pipeline, and issues the `egui_wgpu` paint callbacks used
//! by the central canvas.

use egui::epaint::PaintCallbackInfo;
use egui_wgpu::CallbackTrait;
use grafito_core::{Document, GeoObject, ObjectId, RenderQuality};
use grafito_geometry::Camera3D;
use grafito_render::function_compute::{
    resolve_function_job, FunctionDispatchOutcome, PendingFunctionJob,
};
use grafito_render::gpu_readback::MAX_GPU_READBACK_JOBS_IN_FLIGHT;
use grafito_render::implicit_compute::{
    advance_implicit_job, ImplicitDispatchOutcome, ImplicitResolveStep, PendingImplicitJob,
};
use grafito_render::{DepthRenderTarget, Renderer, Vertex3D};
use std::collections::{BTreeMap, BTreeSet};
use std::sync::Arc;
use std::sync::RwLock;

#[derive(Debug, Clone, PartialEq)]
pub struct Cache2DKey {
    pub version: u64,
    pub view: grafito_geometry::ViewTransform,
    pub render_quality: RenderQuality,
    pub dark_mode: bool,
    pub transient_revision: u64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum BaseRenderer2D {
    Cpu,
    Gpu,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub(crate) enum Scene2DReadiness {
    #[default]
    Pending,
    GpuReady,
    CpuOnly,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub(crate) enum Scene3DReadiness {
    #[default]
    Pending,
    GpuReady,
    CpuOnly,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct Scene2DPlan {
    pub base_renderer: BaseRenderer2D,
    pub schedule_gpu_prepare: bool,
    pub callback_paints_base: bool,
}

pub(crate) fn plan_2d_scene(
    use_gpu: bool,
    renderer_ready: bool,
    scene_readiness: Scene2DReadiness,
    transient_animation: bool,
    view_is_changing: bool,
    canvas_size: egui::Vec2,
) -> Scene2DPlan {
    let gpu_available = use_gpu
        && renderer_ready
        && !transient_animation
        && !view_is_changing
        && canvas_size.x > 0.0
        && canvas_size.y > 0.0;
    if !gpu_available {
        return Scene2DPlan {
            base_renderer: BaseRenderer2D::Cpu,
            schedule_gpu_prepare: false,
            callback_paints_base: false,
        };
    }
    match scene_readiness {
        Scene2DReadiness::GpuReady => Scene2DPlan {
            base_renderer: BaseRenderer2D::Gpu,
            schedule_gpu_prepare: true,
            callback_paints_base: true,
        },
        Scene2DReadiness::Pending => Scene2DPlan {
            base_renderer: BaseRenderer2D::Cpu,
            schedule_gpu_prepare: true,
            callback_paints_base: false,
        },
        Scene2DReadiness::CpuOnly => Scene2DPlan {
            base_renderer: BaseRenderer2D::Cpu,
            schedule_gpu_prepare: false,
            callback_paints_base: false,
        },
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct Cache3DKey {
    pub version: u64,
    pub camera: Camera3D,
    pub render_quality: RenderQuality,
    pub dark_mode: bool,
    pub screen_w: f32,
    pub screen_h: f32,
}

pub struct GpuCanvasResources {
    pub renderer: Arc<RwLock<Option<Renderer>>>,
    pub buffers_2d: Option<PersistentBuffers>,
    pub buffers_3d: Option<Persistent3DBuffers>,
    pub cache_2d: Option<Cache2DKey>,
    pub cache_3d: Option<Cache3DKey>,
    pub scene_readiness: GpuSceneReadiness,
    /// Slot background de compute 2D (frente B6, cap 1): el dispatch retorna
    /// sin esperar y el resolve llega en frames posteriores. Mientras hay job
    /// en vuelo el frame pinta el último-frame-válido (fallback CPU) y el
    /// readiness queda `Pending` ("calculando…" observable).
    pub gpu_compute_slot: GpuComputeSlot,
}

/// Job GPU en vuelo: un dispatch de función o de implícita. El slot admite
/// como máximo [`MAX_GPU_READBACK_JOBS_IN_FLIGHT`] (1): si llega otro, el
/// viejo se aborta (`unmap` idempotente) y se descarta por generación.
/// Nunca hay cola infinita.
#[derive(Debug)]
pub(crate) enum PendingGpuComputeJob {
    Function {
        object_id: ObjectId,
        job: PendingFunctionJob,
    },
    Implicit {
        object_id: ObjectId,
        job: PendingImplicitJob,
    },
}

/// Slot cap-1 para jobs GPU en vuelo con descarte del viejo por generación.
/// Genérico sobre el job para testear la máquina sin GPU (`J = mock` en
/// tests); en producción `J = PendingGpuComputeJob`.
#[derive(Debug)]
pub struct GpuComputeSlot<J = PendingGpuComputeJob> {
    pending: Option<(u64, J)>,
    next_generation: u64,
}

/// `Default` manual sin bound `J: Default`: el slot inicia libre sea cual
/// sea el job (el `derive` exigiría `PendingGpuComputeJob: Default`).
impl<J> Default for GpuComputeSlot<J> {
    fn default() -> Self {
        Self {
            pending: None,
            next_generation: 0,
        }
    }
}

impl<J> GpuComputeSlot<J> {
    /// Ocupa el slot. Si había otro job en vuelo, lo retorna para que el
    /// llamante lo aborte (nunca se encola: cap 1 por presupuesto).
    pub(crate) fn submit(&mut self, job: J) -> Option<J> {
        let generation = self.next_generation;
        self.next_generation = self.next_generation.wrapping_add(1);
        self.pending
            .replace((generation, job))
            .map(|(_, evicted)| evicted)
    }

    /// Extrae el job en vuelo (el avance lo consume y devuelve `NotReady` o
    /// `Done`; en el primer caso se re-encola con [`Self::requeue`]).
    pub(crate) fn take(&mut self) -> Option<(u64, J)> {
        self.pending.take()
    }

    /// Re-encola un job aún en vuelo. El slot debe estar libre (el take y el
    /// avance son secuenciales en el mismo prepare, sin reentrancia); si se
    /// ocupó entretanto se reporta y el llamante debe abortar el job.
    pub(crate) fn requeue(&mut self, generation: u64, job: J) -> Option<J> {
        if self.pending.is_none() {
            self.pending = Some((generation, job));
            None
        } else {
            log::error!("GpuComputeSlot ocupado en requeue; se descarta el job re-encolado");
            Some(job)
        }
    }

    pub(crate) fn is_pending(&self) -> bool {
        self.pending.is_some()
    }

    /// Estado "calculando…" para futuro wiring UI (panels fuera de este
    /// frente): `true` mientras hay un readback GPU en vuelo para la escena.
    /// Nunca es un spinner infinito: el timeout del waiter (250 ms) lo
    /// convierte en fallback CPU, garantizado.
    #[allow(dead_code)] // B6-Next: badge "calculando…" en panels (prohibido en este frente)
    pub fn is_computing(&self) -> bool {
        self.is_pending()
    }
}

/// El slot es cap-1 por presupuesto; si el presupuesto cambia, este frente
/// (take/requeue/drain) debe revisarse. Fijado en compilación.
const _: () = assert!(MAX_GPU_READBACK_JOBS_IN_FLIGHT == 1);

/// Aborta un job desalojado u obsoleto: `unmap` idempotente del buffer
/// implicado para no dejar el pipeline inutilizado. Requiere el compute del
/// mismo pipeline que hizo el dispatch.
fn abort_pending_job(renderer: &Renderer, evicted: PendingGpuComputeJob) {
    match evicted {
        PendingGpuComputeJob::Function { .. } => {
            if let Some(compute) = renderer.function_compute.as_ref() {
                compute.abort_eval();
            } else {
                log::warn!("GpuComputeSlot: sin function pipeline para abortar job desalojado");
            }
        }
        PendingGpuComputeJob::Implicit { .. } => {
            if let Some(compute) = renderer.implicit_compute.as_ref() {
                compute.abort_eval();
            } else {
                log::warn!("GpuComputeSlot: sin implicit pipeline para abortar job desalojado");
            }
        }
    }
}

/// Drena el slot background al inicio del prepare (frente B6, non-blocking).
///
/// - Sin job en vuelo: no hace nada.
/// - Job vigente y listo: popula el cache del objeto (el build posterior hará
///   hit) y libera el slot.
/// - Job vigente y aún en vuelo: lo re-encola; el llamante debe saltar el
///   dispatch nuevo y pintar el último-frame-válido (fallback CPU).
/// - Job obsoleto (objeto borrado, tipo cambiado o key distinta): `unmap` +
///   descarte honesto sin escribir.
///
/// Nunca espera a la GPU: el llamante debe haber hecho el
/// `device.poll(Maintain::Poll)` no-bloqueante del frame antes de llamar.
fn drain_gpu_compute_slot(resources: &mut GpuCanvasResources, document: &Document) {
    let Some((generation, pending)) = resources.gpu_compute_slot.take() else {
        return;
    };
    let Ok(renderer_lock) = resources.renderer.read() else {
        log::warn!("Renderer lock poisoned drenando GpuComputeSlot; se descarta el job");
        return;
    };
    let Some(renderer) = renderer_lock.as_ref() else {
        return;
    };
    match pending {
        PendingGpuComputeJob::Function { object_id, job } => {
            let (Some(compute), Some(obj)) = (
                renderer.function_compute.as_ref(),
                document.get_object(object_id),
            ) else {
                if let Some(compute) = renderer.function_compute.as_ref() {
                    compute.abort_eval();
                }
                return;
            };
            let GeoObject::Function(fun) = obj else {
                compute.abort_eval();
                return;
            };
            // `resolve_function_job` re-chequea la key: si el objeto cambió,
            // no escribe y retorna false (descarte honesto). Terminal: no se
            // re-encola (el resolve de función es de una sola fase).
            let _ = resolve_function_job(compute, fun, &document.variables, job);
        }
        PendingGpuComputeJob::Implicit { object_id, job } => {
            let (Some(compute), Some(obj)) = (
                renderer.implicit_compute.as_ref(),
                document.get_object(object_id),
            ) else {
                if let Some(compute) = renderer.implicit_compute.as_ref() {
                    compute.abort_eval();
                }
                return;
            };
            let GeoObject::ImplicitCurve(ic) = obj else {
                compute.abort_eval();
                return;
            };
            match advance_implicit_job(compute, ic, &document.variables, job) {
                ImplicitResolveStep::Done(_) => {}
                ImplicitResolveStep::NotReady(boxed) => {
                    let requeue = PendingGpuComputeJob::Implicit {
                        object_id,
                        job: *boxed,
                    };
                    if let Some(orphan) = resources.gpu_compute_slot.requeue(generation, requeue) {
                        abort_pending_job(renderer, orphan);
                    }
                }
            }
        }
    }
}

pub struct PersistentBuffers {
    pub vertex_buffer: wgpu::Buffer,
    pub index_buffer: wgpu::Buffer,
    pub vertex_capacity: usize,
    pub index_capacity: usize,
    pub index_count: u32,
    pub object_ranges: BTreeMap<ObjectId, std::ops::Range<u32>>,
    /// Scene that last populated these buffers. Never paint stale geometry.
    pub completed_key: Option<Cache2DKey>,
}

fn completed_2d_buffer_matches_scene(
    completed_key: Option<&Cache2DKey>,
    current_key: &Cache2DKey,
) -> bool {
    completed_key == Some(current_key)
}

fn callback_can_paint_2d(
    frame_authorized_gpu_base: bool,
    completed_key: Option<&Cache2DKey>,
    current_key: &Cache2DKey,
) -> bool {
    frame_authorized_gpu_base && completed_2d_buffer_matches_scene(completed_key, current_key)
}

fn callback_can_paint_3d(
    frame_authorized_gpu_scene: bool,
    completed_key: Option<&Cache3DKey>,
    current_key: &Cache3DKey,
) -> bool {
    frame_authorized_gpu_scene && completed_key == Some(current_key)
}

/// Buffers de la escena 3D. Los streams opaco y wire son independientes:
/// sólidos sin relleno, atractores y líneas no deben depender de un triángulo
/// opaco para poder crear el target con profundidad.
#[derive(Default)]
pub struct Persistent3DBuffers {
    pub vertex_buffer: Option<wgpu::Buffer>,
    pub index_buffer: Option<wgpu::Buffer>,
    pub vertex_capacity: usize,
    pub index_capacity: usize,
    pub index_count: u32,
    pub wire_vertex_buffer: Option<wgpu::Buffer>,
    pub wire_index_buffer: Option<wgpu::Buffer>,
    pub wire_vertex_capacity: usize,
    pub wire_index_capacity: usize,
    pub wire_index_count: u32,
    pub depth_target_3d: Option<DepthRenderTarget>,
    /// Scene that last wrote the offscreen target. Never composite stale frames.
    pub depth_target_key: Option<Cache3DKey>,
}

impl Persistent3DBuffers {
    fn invalidate_depth_target_key(&mut self) {
        self.depth_target_key = None;
    }
}

#[derive(Clone, Default)]
pub struct GpuSceneReadiness {
    state: Arc<RwLock<GpuSceneReadinessState>>,
}

#[derive(Default)]
struct GpuSceneReadinessState {
    two_d: Option<(Cache2DKey, Scene2DReadiness)>,
    three_d: Option<(Cache3DKey, Scene3DReadiness)>,
}

impl GpuSceneReadiness {
    pub fn clear(&self) {
        if let Ok(mut state) = self.state.write() {
            state.two_d = None;
            state.three_d = None;
        }
    }

    pub fn has_2d(&self, key: &Cache2DKey) -> bool {
        self.status_2d(key) == Scene2DReadiness::GpuReady
    }

    pub(crate) fn status_2d(&self, key: &Cache2DKey) -> Scene2DReadiness {
        self.state
            .read()
            .map_or(Scene2DReadiness::Pending, |state| {
                state
                    .two_d
                    .as_ref()
                    .map_or(Scene2DReadiness::Pending, |(stored, status)| {
                        if stored == key {
                            *status
                        } else {
                            Scene2DReadiness::Pending
                        }
                    })
            })
    }

    pub fn has_3d(&self, key: &Cache3DKey) -> bool {
        self.status_3d(key) == Scene3DReadiness::GpuReady
    }

    pub(crate) fn status_3d(&self, key: &Cache3DKey) -> Scene3DReadiness {
        self.state
            .read()
            .map_or(Scene3DReadiness::Pending, |state| {
                state
                    .three_d
                    .as_ref()
                    .map_or(Scene3DReadiness::Pending, |(stored, status)| {
                        if stored == key {
                            *status
                        } else {
                            Scene3DReadiness::Pending
                        }
                    })
            })
    }

    fn clear_2d(&self) {
        if let Ok(mut state) = self.state.write() {
            state.two_d = None;
        }
    }

    fn clear_3d(&self) {
        if let Ok(mut state) = self.state.write() {
            state.three_d = None;
        }
    }

    fn mark_2d(&self, key: Cache2DKey) {
        if let Ok(mut state) = self.state.write() {
            state.two_d = Some((key, Scene2DReadiness::GpuReady));
        }
    }

    fn mark_2d_cpu_only(&self, key: Cache2DKey) {
        if let Ok(mut state) = self.state.write() {
            state.two_d = Some((key, Scene2DReadiness::CpuOnly));
        }
    }

    fn mark_3d(&self, key: Cache3DKey) {
        if let Ok(mut state) = self.state.write() {
            state.three_d = Some((key, Scene3DReadiness::GpuReady));
        }
    }

    fn mark_3d_cpu_only(&self, key: Cache3DKey) {
        if let Ok(mut state) = self.state.write() {
            state.three_d = Some((key, Scene3DReadiness::CpuOnly));
        }
    }
}

fn doubled_buffer_capacity(payload_size: usize) -> Option<(usize, wgpu::BufferAddress)> {
    let capacity = payload_size.checked_mul(2)?;
    let address = u64::try_from(capacity).ok()?;
    (capacity > 0).then_some((capacity, address))
}

fn upload_3d_geometry(
    device: &wgpu::Device,
    queue: &wgpu::Queue,
    buffers: &mut Persistent3DBuffers,
    vertices: &[Vertex3D],
    indices: &[u32],
    vertex_label: &'static str,
    index_label: &'static str,
) -> bool {
    if vertices.is_empty() || indices.is_empty() {
        buffers.index_count = 0;
        return true;
    }
    let vertex_data = bytemuck::cast_slice(vertices);
    let index_data = bytemuck::cast_slice(indices);
    let Ok(index_count) = u32::try_from(indices.len()) else {
        return false;
    };
    if vertex_data.len() > buffers.vertex_capacity || buffers.vertex_buffer.is_none() {
        let Some((capacity, size)) = doubled_buffer_capacity(vertex_data.len()) else {
            return false;
        };
        buffers.vertex_buffer = Some(device.create_buffer(&wgpu::BufferDescriptor {
            label: Some(vertex_label),
            size,
            usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        }));
        buffers.vertex_capacity = capacity;
    }
    if index_data.len() > buffers.index_capacity || buffers.index_buffer.is_none() {
        let Some((capacity, size)) = doubled_buffer_capacity(index_data.len()) else {
            return false;
        };
        buffers.index_buffer = Some(device.create_buffer(&wgpu::BufferDescriptor {
            label: Some(index_label),
            size,
            usage: wgpu::BufferUsages::INDEX | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        }));
        buffers.index_capacity = capacity;
    }
    let (Some(vertex_buffer), Some(index_buffer)) = (&buffers.vertex_buffer, &buffers.index_buffer)
    else {
        return false;
    };
    queue.write_buffer(vertex_buffer, 0, vertex_data);
    queue.write_buffer(index_buffer, 0, index_data);
    buffers.index_count = index_count;
    true
}

const GPU_3D_CURVE_STEPS: usize = 4_000;
const GPU_3D_MAX_ATTRACTORS: usize = 8;
const GPU_3D_MAX_ATTRACTOR_STEPS: usize = 16_000;
const GPU_2D_CURVE_STEPS: usize = 4_000;
// Frente B6: `implicit_compute` y `function_compute` ya NO bloquean el hilo
// de prepare: usan dispatch sin espera + slot background (`GpuComputeSlot`,
// cap 1) con resolve en frames posteriores (ver `gpu_readback.rs`). El resto
// (`parametric_compute`, `vector_compute`, ...) mantiene el readback síncrono
// acotado vía `sync_readback_with_timeout` en `grafito-render/src/lib.rs`.
//
// Mitigación Wait→Poll: el readback NO usa `device.poll(Maintain::Wait)`
// bloqueante; usa `Maintain::Poll` (no bloqueante) en un bucle con timeout de
// 250 ms (`SYNC_GPU_READBACK_TIMEOUT` = `GPU_READBACK_TIMEOUT`, origen único),
// de modo que una GPU colgada no puede congelar el hilo de prepare de egui
// indefinidamente. Aun así, el readback síncrono bloquea el hilo hasta el
// deadline, por lo que se acota a UN intento por frame.
//
// Verificado: `take(MAX_SYNC_GPU_COMPUTE_ATTEMPTS_PER_PREPARE)` en prepare() 2D
// y 3D garantiza 1 readback síncrono como máximo por frame: los callbacks 2D
// (`CanvasCallback`) y 3D (`Canvas3DCallback`) son mutuamente excluyentes
// (match `ViewMode::D2`/`ViewMode::D3` en app.rs), por lo que el `.take(1)` de
// cada rama acota el readback global a 1 por frame.
// TODO P1 (B6-Next): migrar `parametric`/`vector`/`complex`/`domain`/`fill`
// al patrón `dispatch_*` + slot (ver `function_compute`/`implicit_compute`).
// Sin waiter thread a propósito: en wgpu 22 `Device` no es `Clone`, así que
// la espera se distribuye en frames (poll no-bloqueante por frame).
const MAX_SYNC_GPU_COMPUTE_ATTEMPTS_PER_PREPARE: usize = 1;

/// Helper de readback asíncrono sin `device.poll(Wait)` bloqueante.
/// Mitigación Wait→Poll ya activa: `grafito-render::sync_readback_with_timeout`
/// usa `device.poll(Maintain::Poll)` (no bloqueante) en bucle con timeout de
/// 250 ms, por lo que una GPU colgada no congela el hilo de prepare.
/// Frente B6: Implicit/Function ya resuelven vía slot background
/// (`GpuComputeSlot` + `drain_gpu_compute_slot`); el resto sigue limitado a
/// `MAX_SYNC_GPU_COMPUTE_ATTEMPTS_PER_PREPARE` intentos sincrónicos
/// (ver `CanvasCallback::prepare` y `Canvas3DCallback::prepare`).
#[allow(dead_code)]
fn gpu_async_readback_todo_note() {
    // Placeholder para el batch asíncrono futuro; no hace `device.poll(Wait)`.
}

/// Decide si un callback debe intentar GPU en este frame.
/// Evita `Wait` bloqueante durante interacciones continuas (arrastre/zoom/pan).
#[allow(dead_code)]
fn should_attempt_gpu_compute(view_is_changing: bool, transient_animation: bool) -> bool {
    !view_is_changing && !transient_animation
}

/// Acota presupuesto de trabajo VRAM/dispatch por frame sin `poll(Wait)` extra.
/// Usado por `gpu_2d_pre_dispatch_plan` y `gpu_3d_pre_dispatch_plan` para no
/// exceder `MAX_WORLD_MESH_WORK_UNITS` ni memoria de buffers.
#[allow(dead_code)]
fn gpu_budget_allows(remaining: usize, required: usize) -> bool {
    remaining.checked_sub(required).is_some()
}

/// Itera a lo sumo `MAX_SYNC_GPU_COMPUTE_ATTEMPTS_PER_PREPARE` jobs por frame
/// para garantizar un único readback síncrono como máximo. El readback usa
/// `Maintain::Poll` (no bloqueante) con timeout (mitigación Wait→Poll en
/// `grafito-render::sync_readback_with_timeout`); este `take(1)` acota el
/// bloqueo del hilo de prepare a un intento por frame.
/// TODO P1 async batch: cuando el batch asíncrono esté listo, este helper
/// dejará de hacer `take(1)` y pasará a encolar todos los jobs en el batch.
fn limited_gpu_jobs<I>(iter: I) -> impl Iterator<Item = ObjectId>
where
    I: IntoIterator<Item = ObjectId>,
{
    iter.into_iter()
        .take(MAX_SYNC_GPU_COMPUTE_ATTEMPTS_PER_PREPARE)
}

/// Selects only visible 2D GPU cache evaluations that fit the same per-frame
/// 65,536-unit quota used to prepare 3D world meshes. The 2D callback has no
/// independent WorldMesh stream, so sampled work is also its cache capacity.
fn gpu_2d_pre_dispatch_plan(
    document: &Document,
    function_grid_size: usize,
    quality: RenderQuality,
) -> Vec<ObjectId> {
    let objects = grafito_render::ordered_visible_2d_objects(document);

    let mut remaining_work = grafito_render::depth_3d::MAX_WORLD_MESH_WORK_UNITS;
    let mut dispatch_ids = Vec::new();
    for (id, object) in objects {
        let work_units = match object {
            GeoObject::ImplicitCurve(_) => {
                let grid_size = match quality {
                    RenderQuality::Preview => grafito_core::implicit_curve::recommended_grid_size(
                        document.view().screen_size.x,
                        document.view().screen_size.y,
                    )
                    .min(128),
                    RenderQuality::Normal => grafito_core::implicit_curve::recommended_grid_size(
                        document.view().screen_size.x,
                        document.view().screen_size.y,
                    )
                    .min(512),
                    RenderQuality::High => grafito_core::implicit_curve::recommended_grid_size(
                        document.view().screen_size.x,
                        document.view().screen_size.y,
                    )
                    .min(grafito_core::implicit_curve::MAX_IMPLICIT_GRID_SIZE),
                };
                grid_size
                    .checked_add(1)
                    .and_then(|size| size.checked_pow(2))
            }
            GeoObject::Function(_) => function_grid_size.checked_add(1),
            GeoObject::ParametricCurve2D(_) | GeoObject::PolarCurve(_) => {
                GPU_2D_CURVE_STEPS.checked_add(1)
            }
            GeoObject::VectorField2D(field) => field
                .density
                .clamp(5, 128)
                .checked_add(1)
                .and_then(|size| size.checked_pow(2)),
            _ => None,
        };

        let Some(work_units) = work_units else {
            continue;
        };
        if !gpu_budget_allows(remaining_work, work_units) {
            continue;
        }
        remaining_work -= work_units;
        dispatch_ids.push(id);
    }
    dispatch_ids
}

/// Selects only the GPU evaluations that the world-mesh build can still use.
///
/// The renderer sorts object IDs and uses the same work charges below before
/// sampling. Keeping this plan deterministic prevents GPU cache work for later
/// objects that `WorldMesh` will necessarily skip.
fn gpu_3d_pre_dispatch_plan(document: &Document) -> BTreeSet<ObjectId> {
    let mut objects: Vec<_> = document.objects_iter().collect();
    objects.sort_unstable_by(|left, right| left.0.cmp(right.0));

    let mut remaining_work = grafito_render::depth_3d::MAX_WORLD_MESH_WORK_UNITS;
    let mut output_budget = grafito_render::depth_3d::WorldMeshOutputBudget::default();
    let mut attractors_rendered = 0usize;
    let mut dispatch_ids = BTreeSet::new();
    for (id, object) in objects {
        if !object.is_visible() {
            continue;
        }
        let Some(output) = grafito_render::depth_3d::world_mesh_output_usage_for_quality(
            object,
            document.render_quality,
        ) else {
            continue;
        };

        match object {
            GeoObject::Surface3D(surface) => {
                let resolution = surface.mesh_res.clamp(2, 128);
                let work_units = (resolution + 1) * (resolution + 1);
                if !reserve_gpu_3d_resources(
                    &mut remaining_work,
                    &mut output_budget,
                    work_units,
                    output,
                ) {
                    break;
                }
                dispatch_ids.insert(*id);
            }
            GeoObject::ParametricCurve3D(_) => {
                if !reserve_gpu_3d_resources(
                    &mut remaining_work,
                    &mut output_budget,
                    GPU_3D_CURVE_STEPS + 1,
                    output,
                ) {
                    break;
                }
                dispatch_ids.insert(*id);
            }
            GeoObject::Attractor3D(attractor) => {
                if attractors_rendered >= GPU_3D_MAX_ATTRACTORS {
                    break;
                }
                if !reserve_gpu_3d_resources(
                    &mut remaining_work,
                    &mut output_budget,
                    attractor.steps.min(GPU_3D_MAX_ATTRACTOR_STEPS),
                    output,
                ) {
                    break;
                }
                attractors_rendered += 1;
            }
            GeoObject::VectorField3D(field) => {
                let samples_per_axis = field.density.clamp(3, 15) + 1;
                let work_units = samples_per_axis * samples_per_axis * samples_per_axis;
                if !reserve_gpu_3d_resources(
                    &mut remaining_work,
                    &mut output_budget,
                    work_units,
                    output,
                ) {
                    break;
                }
            }
            _ => {
                if !output_budget.fits(output) {
                    break;
                }
                output_budget.consume(output);
            }
        }
    }
    dispatch_ids
}

fn reserve_gpu_3d_resources(
    remaining_work: &mut usize,
    output_budget: &mut grafito_render::depth_3d::WorldMeshOutputBudget,
    work_units: usize,
    output: grafito_render::depth_3d::WorldMeshStreamUsage,
) -> bool {
    let Some(remaining) = remaining_work.checked_sub(work_units) else {
        return false;
    };
    if !output_budget.fits(output) {
        return false;
    }
    *remaining_work = remaining;
    output_budget.consume(output);
    true
}

/// Lado máximo (en píxeles físicos) del target offscreen 3D.
///
/// Un canvas 8K (7680×4320) a DPI 2.0 pediría un target de 15360×8640
/// (~530 MB solo color+depth), que excede el límite de textura 2D de la
/// mayoría de GPUs y puede agotar la memoria de una iGPU. Se escala el lado
/// mayor a este tope preservando el aspect ratio; el composite egui estira el
/// resultado al canvas real (trade-off de calidad aceptable, el fallback CPU
/// sigue renderizando a resolución completa).
const MAX_3D_OFFSCREEN_TARGET_DIMENSION: u32 = 4096;

/// Acota el tamaño del target offscreen 3D para resize 0/1px/8K sin panic ni
/// OOM. Devuelve al menos 1×1 y nunca excede `MAX_3D_OFFSCREEN_TARGET_DIMENSION`
/// en ningún eje, preservando el aspect ratio.
fn cap_offscreen_target_size(width: u32, height: u32) -> (u32, u32) {
    let width = width.max(1);
    let height = height.max(1);
    let max_dim = MAX_3D_OFFSCREEN_TARGET_DIMENSION;
    let scale = (width.max(height) as f64 / max_dim as f64).max(1.0);
    (
        ((width as f64 / scale).ceil() as u32).clamp(1, max_dim),
        ((height as f64 / scale).ceil() as u32).clamp(1, max_dim),
    )
}

pub struct CanvasCallback {
    pub document: Arc<Document>,
    pub dark_mode: bool,
    pub transient_revision: u64,
    pub homotopy_time: f64,
    pub paint_base: bool,
    pub paint_object: Option<ObjectId>,
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
            render_quality: self.document.render_quality,
            dark_mode: self.dark_mode,
            transient_revision: self.transient_revision,
        };

        // Resize defensivo: un canvas colapsado a 0×0 (o 1px) no debe construir
        // geometría degenerada ni una MVP con división por cero. `plan_2d_scene`
        // ya evita programar el callback con tamaño 0, pero el resize puede
        // ocurrir entre el plan y este prepare.
        let sw = self.document.view().screen_size.x;
        let sh = self.document.view().screen_size.y;
        if !sw.is_finite() || !sh.is_finite() || sw <= 0.0 || sh <= 0.0 {
            resources.scene_readiness.mark_2d_cpu_only(current_key);
            return vec![];
        }

        // Frente B6: un poll no-bloqueante por frame dispara los callbacks
        // `map_async` pendientes sin esperar a la GPU (µs), y el drain avanza
        // el slot background (resolve non-blocking o re-encola). El hilo UI
        // nunca bloquea: mientras el job sigue en vuelo se pinta el
        // último-frame-válido por la ruta CPU.
        device.poll(wgpu::Maintain::Poll);
        drain_gpu_compute_slot(resources, &self.document);

        if resources.buffers_2d.as_ref().is_some_and(|buffers| {
            completed_2d_buffer_matches_scene(buffers.completed_key.as_ref(), &current_key)
        }) && resources.cache_2d.as_ref() == Some(&current_key)
        {
            log::debug!("CanvasCallback prepare (2D): cache hit!");
            resources.scene_readiness.mark_2d(current_key);
            return vec![];
        }
        resources.scene_readiness.clear_2d();

        // Frente B6: con un job background en vuelo no se encola más trabajo
        // GPU (cap 1): este frame construye el fallback CPU
        // (último-frame-válido, sin flicker negro) y el readiness queda
        // Pending ("calculando…" observable vía `is_computing`).
        let gpu_busy = resources.gpu_compute_slot.is_pending();

        let (vertices, indices, object_ranges) = {
            let Ok(renderer_lock) = resources.renderer.write() else {
                log::warn!("Renderer lock poisoned in prepare (2D)");
                return vec![];
            };
            let Some(renderer) = renderer_lock.as_ref() else {
                return vec![]; // Still compiling in background
            };

            let mvp = glam::Mat4::orthographic_rh(0.0, sw, sh, 0.0, -1.0, 1.0);
            renderer.update_mvp(queue, mvp);

            log::debug!(
                "CanvasCallback prepare: screen={}x{} objects={}",
                sw,
                sh,
                self.document.object_count()
            );

            // GPU computing for objects using a single-pass objects_iter.
            // Con job en vuelo se salta el dispatch (cap 1); Implicit/Function
            // usan dispatch sin espera (slot background) y el resto mantiene
            // el path síncrono acotado a 1 intento por frame.
            if !gpu_busy {
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

                let dispatch_ids = gpu_2d_pre_dispatch_plan(
                    &self.document,
                    function_grid_size,
                    self.document.render_quality,
                );
                // Limita GPU a un único `Wait` por frame vía `limited_gpu_jobs`
                // (ver TODO async batch arriba). Evita saturar VRAM/tiempo de frame.
                for id in limited_gpu_jobs(dispatch_ids) {
                    let Some(obj) = self.document.get_object(id) else {
                        continue;
                    };
                    match obj {
                        grafito_core::GeoObject::ImplicitCurve(ic) => {
                            if let Some(compute) = implicit_comp {
                                // Frente B6: dispatch sin espera; el resolve
                                // llega en un frame posterior vía el slot.
                                match grafito_render::implicit_compute::dispatch_implicit_on_gpu(
                                    compute,
                                    device,
                                    queue,
                                    ic,
                                    &view,
                                    &self.document.variables,
                                    self.document.render_quality,
                                ) {
                                    ImplicitDispatchOutcome::Dispatched(boxed) => {
                                        if let Some(evicted) = resources.gpu_compute_slot.submit(
                                            PendingGpuComputeJob::Implicit {
                                                object_id: id,
                                                job: *boxed,
                                            },
                                        ) {
                                            abort_pending_job(renderer, evicted);
                                        }
                                    }
                                    ImplicitDispatchOutcome::Cached
                                    | ImplicitDispatchOutcome::Unsupported => {}
                                }
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
                                // Frente B6: dispatch sin espera; el resolve
                                // llega en un frame posterior vía el slot.
                                match grafito_render::function_compute::dispatch_function_on_gpu(
                                    compute,
                                    device,
                                    queue,
                                    fun,
                                    domain,
                                    function_grid_size,
                                    &self.document.variables,
                                ) {
                                    FunctionDispatchOutcome::Dispatched(job) => {
                                        if let Some(evicted) = resources.gpu_compute_slot.submit(
                                            PendingGpuComputeJob::Function { object_id: id, job },
                                        ) {
                                            abort_pending_job(renderer, evicted);
                                        }
                                    }
                                    FunctionDispatchOutcome::Cached
                                    | FunctionDispatchOutcome::Unsupported => {}
                                }
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
            // Domain/complex compute also performs synchronous readback. Build
            // those objects through the deterministic CPU fallback so this
            // callback cannot add more waits after the bounded cache attempt.
            // A future async batch can re-enable these device/queue arguments.
            let (vertices, indices, object_ranges, scene_complete) = renderer
                .build_geometry_with_object_ranges_at(
                    &self.document,
                    self.dark_mode,
                    false,
                    None,
                    None,
                    self.homotopy_time,
                );
            if !scene_complete {
                log::warn!(
                    "CanvasCallback prepare (2D): scene geometry is incomplete; selecting terminal CPU fallback"
                );
                resources.cache_2d = None;
                resources
                    .scene_readiness
                    .mark_2d_cpu_only(current_key.clone());
                return vec![];
            }
            (vertices, indices, object_ranges)
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
            // Frente B6: con job en vuelo no se autoriza la escena (ver abajo).
            if !resources.gpu_compute_slot.is_pending() {
                resources.cache_2d = Some(current_key.clone());
                if let Some(buffers) = &mut resources.buffers_2d {
                    buffers.index_count = 0;
                    buffers.object_ranges.clear();
                    buffers.completed_key = Some(current_key.clone());
                }
                resources.scene_readiness.mark_2d(current_key);
            }
            return vec![];
        }

        let vertex_data = bytemuck::cast_slice(&vertices);
        let index_data = bytemuck::cast_slice(&indices);
        let vertex_size = vertex_data.len();
        let index_size = index_data.len();
        let Some((initial_vertex_capacity, initial_vertex_buffer_size)) =
            doubled_buffer_capacity(vertex_size)
        else {
            log::error!("2D vertex geometry exceeds the GPU buffer size limit");
            return vec![];
        };
        let Some((initial_index_capacity, initial_index_buffer_size)) =
            doubled_buffer_capacity(index_size)
        else {
            log::error!("2D index geometry exceeds the GPU buffer size limit");
            return vec![];
        };
        let Ok(index_count) = u32::try_from(indices.len()) else {
            log::error!("2D index geometry exceeds the u32 draw-index limit");
            return vec![];
        };

        let encoder = device.create_command_encoder(&wgpu::CommandEncoderDescriptor {
            label: Some("Canvas Callback Encoder"),
        });

        let buffers = resources.buffers_2d.get_or_insert_with(|| {
            let vb = device.create_buffer(&wgpu::BufferDescriptor {
                label: Some("Canvas 2D Vertex Buffer"),
                size: initial_vertex_buffer_size,
                usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
                mapped_at_creation: false,
            });
            let ib = device.create_buffer(&wgpu::BufferDescriptor {
                label: Some("Canvas 2D Index Buffer"),
                size: initial_index_buffer_size,
                usage: wgpu::BufferUsages::INDEX | wgpu::BufferUsages::COPY_DST,
                mapped_at_creation: false,
            });
            PersistentBuffers {
                vertex_buffer: vb,
                index_buffer: ib,
                vertex_capacity: initial_vertex_capacity,
                index_capacity: initial_index_capacity,
                index_count: 0,
                object_ranges: BTreeMap::new(),
                completed_key: None,
            }
        });

        if vertex_size > buffers.vertex_capacity {
            let Some((new_capacity, new_buffer_size)) = doubled_buffer_capacity(vertex_size) else {
                log::error!("2D vertex geometry exceeds the GPU buffer size limit");
                return vec![];
            };
            buffers.vertex_buffer = device.create_buffer(&wgpu::BufferDescriptor {
                label: Some("Canvas 2D Vertex Buffer"),
                size: new_buffer_size,
                usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
                mapped_at_creation: false,
            });
            buffers.vertex_capacity = new_capacity;
        }

        if index_size > buffers.index_capacity {
            let Some((new_capacity, new_buffer_size)) = doubled_buffer_capacity(index_size) else {
                log::error!("2D index geometry exceeds the GPU buffer size limit");
                return vec![];
            };
            buffers.index_buffer = device.create_buffer(&wgpu::BufferDescriptor {
                label: Some("Canvas 2D Index Buffer"),
                size: new_buffer_size,
                usage: wgpu::BufferUsages::INDEX | wgpu::BufferUsages::COPY_DST,
                mapped_at_creation: false,
            });
            buffers.index_capacity = new_capacity;
        }

        queue.write_buffer(&buffers.vertex_buffer, 0, vertex_data);
        queue.write_buffer(&buffers.index_buffer, 0, index_data);
        buffers.index_count = index_count;
        buffers.object_ranges = object_ranges;
        // Frente B6: mientras hay un job background en vuelo, la escena queda
        // Pending ("calculando…"): no se setea `completed_key`/`cache_2d` ni
        // se marca `GpuReady`, así el frame pinta el último-frame-válido por
        // la ruta CPU (sin flicker negro) y el cache-hit con pendiente es
        // inalcanzable por construcción. El timeout (250 ms) libera el slot.
        if resources.gpu_compute_slot.is_pending() {
            return vec![encoder.finish()];
        }
        buffers.completed_key = Some(current_key.clone());

        resources.cache_2d = Some(current_key.clone());
        resources.scene_readiness.mark_2d(current_key);
        vec![encoder.finish()]
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
        let current_key = Cache2DKey {
            version: self.document.version,
            view: *self.document.view(),
            render_quality: self.document.render_quality,
            dark_mode: self.dark_mode,
            transient_revision: self.transient_revision,
        };
        if !callback_can_paint_2d(
            self.paint_base,
            buffers.completed_key.as_ref(),
            &current_key,
        ) {
            return;
        }

        if buffers.index_count == 0 {
            return;
        }
        let Some(object_id) = self.paint_object else {
            return;
        };
        let Some(index_range) = buffers.object_ranges.get(&object_id) else {
            return;
        };

        if let Ok(renderer_lock) = resources.renderer.read() {
            if let Some(renderer) = renderer_lock.as_ref() {
                log::debug!("CanvasCallback paint: index_count={}", buffers.index_count);
                render_pass.set_pipeline(&renderer.pipeline);
                render_pass.set_bind_group(0, &renderer.mvp_bind_group, &[]);
                render_pass.set_vertex_buffer(0, buffers.vertex_buffer.slice(..));
                render_pass
                    .set_index_buffer(buffers.index_buffer.slice(..), wgpu::IndexFormat::Uint32);
                render_pass.draw_indexed(index_range.clone(), 0, 0..1);
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
    pub paint_scene: bool,
}

impl CallbackTrait for Canvas3DCallback {
    fn prepare(
        &self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        screen_descriptor: &egui_wgpu::ScreenDescriptor,
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
            render_quality: self.document.render_quality,
            dark_mode: self.dark_mode,
            screen_w: self.screen_w,
            screen_h: self.screen_h,
        };

        // Resize defensivo: canvas 0×0 no debe crear target ni geometría.
        if !self.screen_w.is_finite()
            || !self.screen_h.is_finite()
            || self.screen_w <= 0.0
            || self.screen_h <= 0.0
        {
            resources.scene_readiness.mark_3d_cpu_only(current_key);
            return vec![];
        }
        // 8K/DPI alto: acota el target offscreen para no alocar cientos de MB
        // en iGPU (ver `cap_offscreen_target_size`).
        let (target_width, target_height) = cap_offscreen_target_size(
            (self.screen_w * screen_descriptor.pixels_per_point).ceil() as u32,
            (self.screen_h * screen_descriptor.pixels_per_point).ceil() as u32,
        );
        let target_ready = resources
            .buffers_3d
            .as_ref()
            .and_then(|buffers| buffers.depth_target_3d.as_ref())
            .is_some_and(|target| target.matches_size(target_width, target_height));
        if resources.buffers_3d.is_some()
            && target_ready
            && resources.cache_3d.as_ref() == Some(&current_key)
        {
            log::debug!("Canvas3DCallback prepare: cache hit!");
            resources.scene_readiness.mark_3d(current_key);
            return vec![];
        }
        resources.scene_readiness.clear_3d();
        // `Cache3DKey` uses logical points, so a DPI-only target mismatch can
        // otherwise leave an old physical target authorized after an early return.
        if let Some(buffers) = resources.buffers_3d.as_mut() {
            buffers.invalidate_depth_target_key();
        }

        let mut camera = self.camera;
        camera.aspect = self.screen_w / self.screen_h.max(1.0);
        let mesh = {
            let Ok(renderer_lock) = resources.renderer.read() else {
                log::warn!("Renderer lock poisoned in prepare (3D)");
                return vec![];
            };
            let Some(renderer) = renderer_lock.as_ref() else {
                return vec![]; // Still compiling in background
            };

            renderer.update_mvp(queue, camera.mvp());

            log::debug!(
                "Canvas3DCallback prepare: screen={}x{} objects={}",
                self.screen_w,
                self.screen_h,
                self.document.object_count()
            );

            #[cfg(feature = "profile")]
            puffin::profile_scope!("gpu_compute_3d");
            if let Some(compute) = renderer.parametric_compute.as_ref() {
                let dispatch_ids = gpu_3d_pre_dispatch_plan(&self.document);
                // Limita GPU a un único `Wait` por frame vía `limited_gpu_jobs`
                // (ver TODO async batch). Misma cota que en 2D para no bloquear UI.
                for id in limited_gpu_jobs(dispatch_ids) {
                    let Some(obj) = self.document.get_object(id) else {
                        continue;
                    };
                    match obj {
                        grafito_core::GeoObject::ParametricCurve3D(pc) => {
                            let _ =
                                grafito_render::parametric_compute::maybe_compute_curve_3d_on_gpu(
                                    compute,
                                    device,
                                    queue,
                                    pc,
                                    GPU_3D_CURVE_STEPS,
                                    &self.document.variables,
                                );
                        }
                        grafito_core::GeoObject::Surface3D(su) => {
                            let res = su.mesh_res.clamp(2, 128);
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

            #[cfg(feature = "profile")]
            puffin::profile_scope!("geometry_build_3d");
            Renderer::build_3d_world_mesh(&self.document, &camera, self.screen_w, self.screen_h)
        };

        if !mesh.is_complete() {
            log::warn!(
                "Canvas3DCallback kept CPU ownership because the visible scene exceeds GPU mesh limits"
            );
            resources.scene_readiness.mark_3d_cpu_only(current_key);
            return vec![];
        }
        if let Err(error) = mesh.validate() {
            log::error!("Canvas3DCallback rejected invalid world mesh: {error}");
            resources.scene_readiness.mark_3d_cpu_only(current_key);
            return vec![];
        }
        log::debug!(
            "Canvas3DCallback world mesh: opaque={} wire={}",
            mesh.opaque_indices.len(),
            mesh.wire_indices.len()
        );

        let needs_target = {
            let buffers = resources.buffers_3d.get_or_insert_with(Default::default);
            if !upload_3d_geometry(
                device,
                queue,
                buffers,
                &mesh.opaque_vertices,
                &mesh.opaque_indices,
                "Canvas 3D Opaque Vertex Buffer",
                "Canvas 3D Opaque Index Buffer",
            ) {
                log::error!("3D opaque geometry exceeds the GPU buffer size limit");
                resources.scene_readiness.mark_3d_cpu_only(current_key);
                return vec![];
            }
            buffers
                .depth_target_3d
                .as_ref()
                .is_none_or(|target| !target.matches_size(target_width, target_height))
        };
        if needs_target {
            let Ok(renderer_lock) = resources.renderer.read() else {
                return vec![];
            };
            let Some(renderer) = renderer_lock.as_ref() else {
                return vec![];
            };
            let target = renderer.create_depth_render_target(device, target_width, target_height);
            let buffers = resources.buffers_3d.get_or_insert_with(Default::default);
            buffers.depth_target_3d = Some(target);
        }
        let buffers = resources.buffers_3d.get_or_insert_with(Default::default);
        let wire_vertices = bytemuck::cast_slice(&mesh.wire_vertices);
        let wire_indices = bytemuck::cast_slice(&mesh.wire_indices);
        if mesh.wire_vertices.is_empty() || mesh.wire_indices.is_empty() {
            buffers.wire_index_count = 0;
        } else {
            let Some((vertex_capacity, vertex_size)) = doubled_buffer_capacity(wire_vertices.len())
            else {
                resources.scene_readiness.mark_3d_cpu_only(current_key);
                return vec![];
            };
            let Some((index_capacity, index_size)) = doubled_buffer_capacity(wire_indices.len())
            else {
                resources.scene_readiness.mark_3d_cpu_only(current_key);
                return vec![];
            };
            let Ok(index_count) = u32::try_from(mesh.wire_indices.len()) else {
                resources.scene_readiness.mark_3d_cpu_only(current_key);
                return vec![];
            };
            if wire_vertices.len() > buffers.wire_vertex_capacity
                || buffers.wire_vertex_buffer.is_none()
            {
                buffers.wire_vertex_buffer = Some(device.create_buffer(&wgpu::BufferDescriptor {
                    label: Some("Canvas 3D Wire Vertex Buffer"),
                    size: vertex_size,
                    usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
                    mapped_at_creation: false,
                }));
                buffers.wire_vertex_capacity = vertex_capacity;
            }
            if wire_indices.len() > buffers.wire_index_capacity
                || buffers.wire_index_buffer.is_none()
            {
                buffers.wire_index_buffer = Some(device.create_buffer(&wgpu::BufferDescriptor {
                    label: Some("Canvas 3D Wire Index Buffer"),
                    size: index_size,
                    usage: wgpu::BufferUsages::INDEX | wgpu::BufferUsages::COPY_DST,
                    mapped_at_creation: false,
                }));
                buffers.wire_index_capacity = index_capacity;
            }
            if let (Some(vertex_buffer), Some(index_buffer)) =
                (&buffers.wire_vertex_buffer, &buffers.wire_index_buffer)
            {
                queue.write_buffer(vertex_buffer, 0, wire_vertices);
                queue.write_buffer(index_buffer, 0, wire_indices);
                buffers.wire_index_count = index_count;
            }
        }
        let Some(target) = buffers.depth_target_3d.as_ref() else {
            resources.scene_readiness.mark_3d_cpu_only(current_key);
            return vec![];
        };
        let Ok(renderer_lock) = resources.renderer.read() else {
            return vec![];
        };
        let Some(renderer) = renderer_lock.as_ref() else {
            return vec![];
        };
        let mut encoder = device.create_command_encoder(&wgpu::CommandEncoderDescriptor {
            label: Some("Canvas 3D Depth Encoder"),
        });
        {
            let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("Canvas 3D Depth Pass"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: &target.render_color_view,
                    resolve_target: target.resolve_target(),
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Clear(wgpu::Color::TRANSPARENT),
                        store: wgpu::StoreOp::Store,
                    },
                })],
                depth_stencil_attachment: Some(wgpu::RenderPassDepthStencilAttachment {
                    view: &target.depth_view,
                    depth_ops: Some(wgpu::Operations {
                        load: wgpu::LoadOp::Clear(1.0),
                        store: wgpu::StoreOp::Store,
                    }),
                    stencil_ops: None,
                }),
                timestamp_writes: None,
                occlusion_query_set: None,
            });
            if buffers.index_count > 0 {
                if let (Some(vertex_buffer), Some(index_buffer)) =
                    (&buffers.vertex_buffer, &buffers.index_buffer)
                {
                    pass.set_pipeline(&renderer.pipeline_3d);
                    pass.set_bind_group(0, &renderer.mvp_bind_group, &[]);
                    pass.set_vertex_buffer(0, vertex_buffer.slice(..));
                    pass.set_index_buffer(index_buffer.slice(..), wgpu::IndexFormat::Uint32);
                    pass.draw_indexed(0..buffers.index_count, 0, 0..1);
                }
            }
            if buffers.wire_index_count > 0 {
                if let (Some(vertex_buffer), Some(index_buffer)) =
                    (&buffers.wire_vertex_buffer, &buffers.wire_index_buffer)
                {
                    pass.set_pipeline(&renderer.pipeline_3d_wire);
                    pass.set_bind_group(0, &renderer.mvp_bind_group, &[]);
                    pass.set_vertex_buffer(0, vertex_buffer.slice(..));
                    pass.set_index_buffer(index_buffer.slice(..), wgpu::IndexFormat::Uint32);
                    pass.draw_indexed(0..buffers.wire_index_count, 0, 0..1);
                }
            }
        }
        buffers.depth_target_key = Some(current_key.clone());
        resources.cache_3d = Some(current_key.clone());
        resources.scene_readiness.mark_3d(current_key);
        vec![encoder.finish()]
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
        let Some(target) = &buffers.depth_target_3d else {
            return;
        };
        let current_key = Cache3DKey {
            version: self.document.version,
            camera: self.camera,
            render_quality: self.document.render_quality,
            dark_mode: self.dark_mode,
            screen_w: self.screen_w,
            screen_h: self.screen_h,
        };
        if !callback_can_paint_3d(
            self.paint_scene,
            buffers.depth_target_key.as_ref(),
            &current_key,
        ) {
            return;
        }

        let Ok(renderer_lock) = resources.renderer.read() else {
            return;
        };
        let Some(renderer) = renderer_lock.as_ref() else {
            return;
        };

        // egui's render pass owns only color. The depth-tested scene was
        // rendered offscreen in prepare and is composited inside this clipped
        // callback region.
        renderer.composite_depth_render_target(render_pass, target);
    }
}

#[cfg(test)]
mod tests {
    use super::{
        callback_can_paint_2d, callback_can_paint_3d, plan_2d_scene, BaseRenderer2D, Cache2DKey,
        Cache3DKey, GpuComputeSlot, GpuSceneReadiness, Persistent3DBuffers, Scene2DReadiness,
        Scene3DReadiness,
    };
    use grafito_core::{
        GeoObject, ObjectId, ParametricCurve2DObj, ParametricCurve3DObj, RegularPolychoron4DObj,
        RenderQuality, Sphere3DObj, Surface3DObj, Torus3DObj,
    };
    use grafito_geometry::{Camera3D, Color, Point3D, RegularPolychoron, ViewTransform};

    fn fixed_object_id(value: u128) -> ObjectId {
        let hex = format!("{value:032x}");
        let uuid = format!(
            "{}-{}-{}-{}-{}",
            &hex[..8],
            &hex[8..12],
            &hex[12..16],
            &hex[16..20],
            &hex[20..]
        );
        serde_json::from_str(&format!("\"{uuid}\"")).unwrap()
    }

    #[test]
    fn doubled_buffer_capacity_rejects_zero_and_overflow() {
        assert_eq!(super::doubled_buffer_capacity(32), Some((64, 64)));
        assert_eq!(super::doubled_buffer_capacity(0), None);
        assert_eq!(super::doubled_buffer_capacity(usize::MAX), None);
    }

    #[test]
    fn scene_readiness_requires_an_exact_successful_scene_key() {
        let readiness = GpuSceneReadiness::default();
        let two_d = Cache2DKey {
            version: 1,
            view: ViewTransform::new(800.0, 600.0),
            render_quality: RenderQuality::Normal,
            dark_mode: false,
            transient_revision: 0,
        };
        let three_d = Cache3DKey {
            version: 1,
            camera: Camera3D::new(4.0 / 3.0),
            render_quality: RenderQuality::Normal,
            dark_mode: false,
            screen_w: 800.0,
            screen_h: 600.0,
        };

        assert!(!readiness.has_2d(&two_d));
        assert!(!readiness.has_3d(&three_d));
        readiness.mark_2d(two_d.clone());
        readiness.mark_3d(three_d.clone());
        assert!(readiness.has_2d(&two_d));
        assert!(readiness.has_3d(&three_d));

        let changed_view = Cache2DKey {
            view: ViewTransform::new(801.0, 600.0),
            ..two_d.clone()
        };
        assert!(!readiness.has_2d(&changed_view));
        let changed_transient = Cache2DKey {
            transient_revision: 1,
            ..two_d
        };
        assert!(!readiness.has_2d(&changed_transient));
        readiness.clear();
        assert!(!readiness.has_2d(&changed_view));
        assert!(!readiness.has_3d(&three_d));
    }

    #[test]
    fn stale_2d_buffer_cannot_paint_after_new_prepare_fails() {
        let successful_key = Cache2DKey {
            version: 1,
            view: ViewTransform::new(800.0, 600.0),
            render_quality: RenderQuality::Normal,
            dark_mode: false,
            transient_revision: 0,
        };
        let failed_prepare_key = Cache2DKey {
            version: 2,
            ..successful_key.clone()
        };
        let completed_buffer_key = Some(successful_key.clone());

        assert!(super::completed_2d_buffer_matches_scene(
            completed_buffer_key.as_ref(),
            &successful_key
        ));
        assert!(
            !super::completed_2d_buffer_matches_scene(
                completed_buffer_key.as_ref(),
                &failed_prepare_key
            ),
            "a failed prepare must not let the previous scene overlay the CPU fallback"
        );
    }

    #[test]
    fn cold_gpu_scene_keeps_cpu_ownership_while_scheduling_warmup() {
        let plan = plan_2d_scene(
            true,
            true,
            Scene2DReadiness::Pending,
            false,
            false,
            egui::vec2(800.0, 600.0),
        );

        assert_eq!(plan.base_renderer, BaseRenderer2D::Cpu);
        assert!(plan.schedule_gpu_prepare);
        assert!(!plan.callback_paints_base);
    }

    #[test]
    fn changing_view_defers_gpu_warmup_until_the_interaction_settles() {
        let changing = plan_2d_scene(
            true,
            true,
            Scene2DReadiness::Pending,
            false,
            true,
            egui::vec2(800.0, 600.0),
        );
        assert_eq!(changing.base_renderer, BaseRenderer2D::Cpu);
        assert!(!changing.schedule_gpu_prepare);
        assert!(!changing.callback_paints_base);

        let settled = plan_2d_scene(
            true,
            true,
            Scene2DReadiness::Pending,
            false,
            false,
            egui::vec2(800.0, 600.0),
        );
        assert_eq!(settled.base_renderer, BaseRenderer2D::Cpu);
        assert!(settled.schedule_gpu_prepare);
        assert!(!settled.callback_paints_base);
    }

    #[test]
    fn locked_renderer_keeps_cpu_ownership_without_callback() {
        let plan = plan_2d_scene(
            true,
            false,
            Scene2DReadiness::GpuReady,
            false,
            false,
            egui::vec2(800.0, 600.0),
        );

        assert_eq!(plan.base_renderer, BaseRenderer2D::Cpu);
        assert!(!plan.schedule_gpu_prepare);
        assert!(!plan.callback_paints_base);
    }

    #[test]
    fn warmup_callback_cannot_claim_the_cpu_owned_frame_after_prepare() {
        let key = Cache2DKey {
            version: 7,
            view: ViewTransform::new(800.0, 600.0),
            render_quality: RenderQuality::Normal,
            dark_mode: false,
            transient_revision: 3,
        };

        assert!(!callback_can_paint_2d(false, Some(&key), &key));
        assert!(callback_can_paint_2d(true, Some(&key), &key));
    }

    #[test]
    fn terminal_cpu_only_scene_stops_scheduling_gpu_warmup() {
        let readiness = GpuSceneReadiness::default();
        let key = Cache2DKey {
            version: 9,
            view: ViewTransform::new(800.0, 600.0),
            render_quality: RenderQuality::Normal,
            dark_mode: false,
            transient_revision: 0,
        };
        readiness.mark_2d_cpu_only(key.clone());

        assert_eq!(readiness.status_2d(&key), Scene2DReadiness::CpuOnly);
        assert_eq!(
            readiness.status_2d(&Cache2DKey {
                version: key.version + 1,
                ..key.clone()
            }),
            Scene2DReadiness::Pending,
            "a changed exact key must be eligible for a new GPU prepare"
        );
        let plan = plan_2d_scene(
            true,
            true,
            readiness.status_2d(&key),
            false,
            false,
            egui::vec2(800.0, 600.0),
        );
        assert_eq!(plan.base_renderer, BaseRenderer2D::Cpu);
        assert!(!plan.schedule_gpu_prepare);
        assert!(!plan.callback_paints_base);
    }

    #[test]
    fn terminal_cpu_only_3d_scene_is_tracked_per_exact_key() {
        let readiness = GpuSceneReadiness::default();
        let key = Cache3DKey {
            version: 9,
            camera: Camera3D::new(4.0 / 3.0),
            render_quality: RenderQuality::Normal,
            dark_mode: false,
            screen_w: 800.0,
            screen_h: 600.0,
        };
        readiness.mark_3d_cpu_only(key.clone());

        assert_eq!(readiness.status_3d(&key), Scene3DReadiness::CpuOnly);
        assert_eq!(
            readiness.status_3d(&Cache3DKey {
                camera: Camera3D::new(16.0 / 9.0),
                ..key
            }),
            Scene3DReadiness::Pending,
        );
    }

    #[test]
    fn cold_3d_callback_cannot_claim_the_cpu_owned_frame_after_prepare() {
        let key = Cache3DKey {
            version: 3,
            camera: Camera3D::new(4.0 / 3.0),
            render_quality: RenderQuality::Normal,
            dark_mode: false,
            screen_w: 800.0,
            screen_h: 600.0,
        };

        assert!(!callback_can_paint_3d(false, Some(&key), &key));
        assert!(callback_can_paint_3d(true, Some(&key), &key));
    }

    #[test]
    fn failed_depth_target_rebuild_cannot_authorize_stale_paint() {
        let key = Cache3DKey {
            version: 3,
            camera: Camera3D::new(4.0 / 3.0),
            render_quality: RenderQuality::Normal,
            dark_mode: false,
            screen_w: 800.0,
            screen_h: 600.0,
        };
        let mut buffers = Persistent3DBuffers {
            depth_target_key: Some(key.clone()),
            ..Default::default()
        };

        buffers.invalidate_depth_target_key();

        assert!(
            !callback_can_paint_3d(true, buffers.depth_target_key.as_ref(), &key),
            "a failed DPI target rebuild must not composite an old offscreen target"
        );
    }

    #[test]
    fn advancing_transient_animation_forces_cpu_ownership() {
        let plan = plan_2d_scene(
            true,
            true,
            Scene2DReadiness::GpuReady,
            true,
            false,
            egui::vec2(800.0, 600.0),
        );

        assert_eq!(plan.base_renderer, BaseRenderer2D::Cpu);
        assert!(!plan.schedule_gpu_prepare);
    }

    #[test]
    fn gpu_3d_dispatch_plan_keeps_surfaces_at_the_world_mesh_work_limit() {
        let mut document = grafito_core::Document::new();
        let mut ids = Vec::new();
        for _ in 0..4 {
            let mut surface = Surface3DObj::new("0", (-1.0, 1.0), (-1.0, 1.0));
            surface.mesh_res = 127;
            ids.push(document.add_object(GeoObject::Surface3D(surface)));
        }

        let planned = super::gpu_3d_pre_dispatch_plan(&document);

        assert_eq!(planned.len(), 4);
        assert!(ids.into_iter().all(|id| planned.contains(&id)));
    }

    #[test]
    fn gpu_3d_dispatch_plan_skips_over_budget_surfaces_without_populating_caches() {
        let mut document = grafito_core::Document::new();
        let mut ids = Vec::new();
        for _ in 0..5 {
            let mut surface = Surface3DObj::new("0", (-1.0, 1.0), (-1.0, 1.0));
            surface.mesh_res = 128;
            ids.push(document.add_object(GeoObject::Surface3D(surface)));
        }
        ids.sort_unstable();

        let planned = super::gpu_3d_pre_dispatch_plan(&document);

        assert_eq!(planned.len(), 3);
        assert!(ids[..3].iter().all(|id| planned.contains(id)));
        assert!(ids[3..].iter().all(|id| !planned.contains(id)));
        assert!(document.objects_iter().all(|(_, object)| {
            matches!(object, GeoObject::Surface3D(surface) if surface.cached_grid.read().unwrap_or_else(|p| p.into_inner()).is_empty())
        }));
    }

    #[test]
    fn gpu_3d_dispatch_plan_skips_a_curve_after_transparent_toruses_exhaust_wire_streams() {
        let mut output_torus = Torus3DObj::new(Point3D::new(0.0, 0.0, 0.0), 2.0, 0.5);
        output_torus.color = Color::new(0.2, 0.4, 0.8, 0.5);
        let output = grafito_render::depth_3d::world_mesh_output_usage_for_quality(
            &GeoObject::Torus3D(output_torus),
            RenderQuality::Normal,
        )
        .expect("a transparent torus has bounded WorldMesh output");
        let mut output_budget = grafito_render::depth_3d::WorldMeshOutputBudget::default();
        for _ in 0..122 {
            assert!(output_budget.fits(output));
            output_budget.consume(output);
        }
        assert!(
            !output_budget.fits(output),
            "the 123rd transparent torus must exceed the wire vertex capacity"
        );

        let mut document = grafito_core::Document::new();
        for value in 1..=123 {
            let mut torus = Torus3DObj::new(Point3D::new(0.0, 0.0, 0.0), 2.0, 0.5);
            torus.id = fixed_object_id(value);
            torus.color = Color::new(0.2, 0.4, 0.8, 0.5);
            document.add_object(GeoObject::Torus3D(torus));
        }
        let mut curve = ParametricCurve3DObj::new("t", "0", "0", 0.0, 1.0);
        curve.id = fixed_object_id(u128::MAX);
        let curve_id = document.add_object(GeoObject::ParametricCurve3D(curve));

        let planned = super::gpu_3d_pre_dispatch_plan(&document);

        assert!(
            !planned.contains(&curve_id),
            "a curve after exhausted WorldMesh wire streams must not be computed or cached"
        );
    }

    #[test]
    fn gpu_3d_dispatch_plan_stops_after_opaque_120_cells_exhaust_the_output_budget() {
        let mut document = grafito_core::Document::new();
        for value in 1..=81 {
            let mut polychoron = RegularPolychoron4DObj::new(RegularPolychoron::OneTwentyCell);
            polychoron.id = fixed_object_id(value);
            document.add_object(GeoObject::RegularPolychoron4D(polychoron));
        }
        let mut curve = ParametricCurve3DObj::new("t", "0", "0", 0.0, 1.0);
        curve.id = fixed_object_id(u128::MAX);
        let curve_id = document.add_object(GeoObject::ParametricCurve3D(curve));

        let planned = super::gpu_3d_pre_dispatch_plan(&document);

        assert!(
            !planned.contains(&curve_id),
            "a curve after opaque 120-cells overflow WorldMesh output must not be computed or cached"
        );
    }

    #[test]
    fn gpu_3d_dispatch_plan_accounts_transparent_solid_output_in_the_wire_stream() {
        let mut document = grafito_core::Document::new();
        for value in 1..=102 {
            let mut sphere = Sphere3DObj::new(Point3D::new(0.0, 0.0, 0.0), 1.0);
            sphere.id = fixed_object_id(value);
            sphere.fill_color = Some(Color::new(0.2, 0.4, 0.8, 0.5));
            document.add_object(GeoObject::Sphere3D(sphere));
        }
        let mut curve = ParametricCurve3DObj::new("t", "0", "0", 0.0, 1.0);
        curve.id = fixed_object_id(u128::MAX);
        let curve_id = document.add_object(GeoObject::ParametricCurve3D(curve));

        let planned = super::gpu_3d_pre_dispatch_plan(&document);

        assert!(
            !planned.contains(&curve_id),
            "a curve after transparent solids exhaust WorldMesh wire capacity must not be computed"
        );
    }

    #[test]
    fn gpu_2d_dispatch_plan_excludes_hidden_and_all_3d_objects() {
        let mut document = grafito_core::Document::new();
        let visible_2d = document.add_object(GeoObject::ParametricCurve2D(
            ParametricCurve2DObj::new("t", "t", 0.0, 1.0),
        ));
        let hidden_2d = document.add_object(GeoObject::ParametricCurve2D(
            ParametricCurve2DObj::new("t", "t^2", 0.0, 1.0),
        ));
        let visible_3d = document.add_object(GeoObject::ParametricCurve3D(
            ParametricCurve3DObj::new("t", "0", "0", 0.0, 1.0),
        ));
        let hidden_surface = document.add_object(GeoObject::Surface3D(Surface3DObj::new(
            "x + y",
            (-1.0, 1.0),
            (-1.0, 1.0),
        )));
        document
            .get_object_mut(hidden_2d)
            .expect("hidden 2D curve exists")
            .set_visible(false);
        document
            .get_object_mut(hidden_surface)
            .expect("hidden surface exists")
            .set_visible(false);

        let planned = super::gpu_2d_pre_dispatch_plan(&document, 1_000, RenderQuality::Normal);

        assert!(planned.contains(&visible_2d));
        assert!(!planned.contains(&hidden_2d));
        assert!(!planned.contains(&visible_3d));
        assert!(!planned.contains(&hidden_surface));
    }

    #[test]
    fn gpu_2d_dispatch_plan_applies_the_shared_work_budget_deterministically() {
        let mut document = grafito_core::Document::new();
        let mut ids = Vec::new();
        for value in 1..=17 {
            let mut curve = ParametricCurve2DObj::new("t", "t", 0.0, 1.0);
            curve.id = fixed_object_id(value);
            ids.push(document.add_object(GeoObject::ParametricCurve2D(curve)));
        }
        ids.sort_unstable();

        let planned = super::gpu_2d_pre_dispatch_plan(&document, 1_000, RenderQuality::Normal);

        assert_eq!(planned.len(), 16);
        assert!(ids[..16].iter().all(|id| planned.contains(id)));
        assert!(ids[16..].iter().all(|id| !planned.contains(id)));
    }

    #[test]
    fn offscreen_target_cap_handles_zero_one_pixel_and_8k_without_oom() {
        // 0×0 → nunca un target de 0 (wgpu rechaza texturas vacías).
        assert_eq!(super::cap_offscreen_target_size(0, 0), (1, 1));
        // 1px se mantiene.
        assert_eq!(super::cap_offscreen_target_size(1, 1), (1, 1));
        // Resolución normal no se toca.
        assert_eq!(super::cap_offscreen_target_size(1920, 1080), (1920, 1080));
        // 8K a DPI 2.0 (15360×8640) se escala al tope preservando aspect.
        let (w, h) = super::cap_offscreen_target_size(15360, 8640);
        assert!(w <= super::MAX_3D_OFFSCREEN_TARGET_DIMENSION);
        assert!(h <= super::MAX_3D_OFFSCREEN_TARGET_DIMENSION);
        assert_eq!(w, super::MAX_3D_OFFSCREEN_TARGET_DIMENSION);
        assert!((h as f64 / w as f64 - 8640.0 / 15360.0).abs() < 0.01);
        // 8K a DPI 1.0 (7680×4320) también se acota.
        let (w, h) = super::cap_offscreen_target_size(7680, 4320);
        assert!(w <= super::MAX_3D_OFFSCREEN_TARGET_DIMENSION);
        assert!(h <= super::MAX_3D_OFFSCREEN_TARGET_DIMENSION);
        // Valores extremos (u32::MAX) no desbordan ni devuelven 0.
        let (w, h) = super::cap_offscreen_target_size(u32::MAX, u32::MAX);
        assert_eq!(w, super::MAX_3D_OFFSCREEN_TARGET_DIMENSION);
        assert_eq!(h, super::MAX_3D_OFFSCREEN_TARGET_DIMENSION);
    }

    #[test]
    fn offscreen_target_cap_preserves_aspect_ratio_for_ultrawide() {
        let (w, h) = super::cap_offscreen_target_size(15360, 2160);
        assert_eq!(w, super::MAX_3D_OFFSCREEN_TARGET_DIMENSION);
        assert!((h as f64 / w as f64 - 2160.0 / 15360.0).abs() < 0.01);
        assert!(h >= 1);
    }

    /// El slot inicia libre: sin job en vuelo no hay estado "calculando…".
    #[test]
    fn gpu_compute_slot_starts_free_without_computing() {
        let slot = GpuComputeSlot::<u32>::default();
        assert!(!slot.is_pending());
        assert!(!slot.is_computing());
    }

    /// Cap 1: el segundo submit desaloja al viejo (nunca cola infinita) y el
    /// desalojado se retorna para abortar (`unmap` idempotente).
    #[test]
    fn gpu_compute_slot_caps_one_job_and_evicts_oldest() {
        let mut slot = GpuComputeSlot::default();
        assert!(slot.submit(10_u32).is_none());
        assert!(slot.is_pending());
        assert!(slot.is_computing());
        assert_eq!(slot.submit(20_u32), Some(10_u32));
        assert_eq!(slot.take(), Some((1, 20_u32)));
        assert!(!slot.is_pending());
        assert!(!slot.is_computing());
    }

    /// Ciclo take → avance → requeue conserva la generación; el take libera
    /// el slot y el requeue lo vuelve a ocupar (sin reentrancia en prepare).
    #[test]
    fn gpu_compute_slot_take_requeue_cycle_keeps_generation() {
        let mut slot = GpuComputeSlot::default();
        assert!(slot.submit(7_u32).is_none());
        let Some((generation, job)) = slot.take() else {
            panic!("slot must hold the submitted job");
        };
        assert_eq!(generation, 0);
        assert!(!slot.is_pending());
        assert!(slot.requeue(generation, job).is_none());
        assert!(slot.is_pending());
        assert_eq!(slot.take(), Some((0, 7_u32)));
    }

    /// Re-encolar con el slot ocupado devuelve el job (no se pierde en
    /// silencio): el llamante debe abortarlo.
    #[test]
    fn gpu_compute_slot_requeue_into_occupied_slot_returns_job() {
        let mut slot = GpuComputeSlot::default();
        slot.submit(1_u32);
        assert_eq!(slot.requeue(99, 2_u32), Some(2_u32));
        assert_eq!(slot.take(), Some((0, 1_u32)));
    }
}
