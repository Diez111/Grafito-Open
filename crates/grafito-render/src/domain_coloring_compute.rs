//! GPU compute pipeline for domain coloring grid evaluation.
//!
//! Evaluates a complex expression f(z) on a 2D grid of points and returns
//! RGBA colors using HSL domain coloring (hue=arg, lightness=atan(ln(|f|))).
//! This offloads the O(N²) per-cell evaluation from CPU to GPU.

use bytemuck::{Pod, Zeroable};
use std::collections::BTreeMap;
use std::sync::{atomic::AtomicBool, Arc};

use crate::gpu_readback::{PendingGpuReadback, ReadbackPoll};
use grafito_complex::math::complex_expr::ComplexExpr;
use grafito_complex::math::complex_opcode::{compile_complex_expr, ComplexBytecodeProgram};

/// Presupuesto de celdas por dispatch de domain coloring (500×500 = 250k).
/// Ver `docs/architecture.md:8` — GPU domain_coloring_compute 250k cells/dispatch.
const MAX_CELLS: usize = 250_000;
const MAX_COMPLEX_CODE: usize = 4096;
const GPU_COMPLEX_STACK_SIZE: usize = 32;

/// Rechaza grids por encima del presupuesto [`MAX_CELLS`] antes de tocar la GPU.
pub(crate) fn domain_cells_within_budget(cells: usize) -> bool {
    cells <= MAX_CELLS
}

pub(crate) fn gpu_program_is_supported(code: &[u32]) -> bool {
    let mut stack_depth = 0usize;
    for instruction in code {
        match instruction & 0xFF {
            0 => {}
            1 | 2 => {
                if stack_depth == GPU_COMPLEX_STACK_SIZE {
                    return false;
                }
                stack_depth += 1;
            }
            3..=7 => {
                if stack_depth < 2 {
                    return false;
                }
                stack_depth -= 1;
            }
            8..=15 | 22..=27 | 31..=33 | 102..=105 => {
                if stack_depth == 0 {
                    return false;
                }
            }
            _ => return false,
        }
    }
    stack_depth == 1
}

#[cfg(test)]
mod tests {
    #[test]
    fn cpu_only_complex_opcodes_are_rejected_before_domain_dispatch() {
        assert!(super::gpu_program_is_supported(&[1, 2, 3, 105]));
        assert!(!super::gpu_program_is_supported(&[28]));
        assert!(!super::gpu_program_is_supported(&[100]));
    }

    #[test]
    fn domain_programs_deeper_than_the_wgsl_stack_are_rejected_before_dispatch() {
        let mut code = vec![2; 33];
        code.extend(vec![3; 32]);

        assert!(!super::gpu_program_is_supported(&code));
    }

    #[test]
    fn domain_cell_budget_rejects_over_250k_without_clamping() {
        // MAX_CELLS 250k (500×500) es un presupuesto duro: por encima se
        // rechaza (None) en vez de recortar silenciosamente.
        assert!(super::domain_cells_within_budget(250_000));
        assert!(super::domain_cells_within_budget(0));
        assert!(!super::domain_cells_within_budget(250_001));
    }
}

pub struct DomainColoringComputePipeline {
    pipeline: wgpu::ComputePipeline,
    bind_group_layout: wgpu::BindGroupLayout,
    params_buffer: wgpu::Buffer,
    bytecode_buffer: wgpu::Buffer,
    constants_buffer: wgpu::Buffer,
    out_buffer: wgpu::Buffer,
    out_readback: wgpu::Buffer,
    /// GPU timestamp queries (feature `profiling`); no-op sin la feature.
    timing: crate::gpu_timing::GpuTimingHandle,
}

#[repr(C)]
#[derive(Copy, Clone, Pod, Zeroable)]
struct GridParamsUniform {
    grid_size: u32,
    code_len: u32,
    dc_mode: u32,
    _pad1: u32,
    /// Origen de la malla regular (x_min, y_min).
    grid_origin: [f32; 2],
    /// Paso de celda (dx, dy).
    grid_step: [f32; 2],
    /// Lado en celdas (`grid_size == grid_res * grid_res`).
    grid_res: u32,
    _pad2: u32,
}

/// Malla regular de domain coloring: los centros de celda se derivan en el
/// shader (`origin + (idx/res + 0.5, idx%res + 0.5) * step`, fila-mayor igual
/// que el CPU). Sin subir `in_points`: -2 MiB por dispatch de 250k celdas
/// (más el `Vec` huésped de 4 MB).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct DomainGrid {
    /// Esquina inferior-izquierda del dominio.
    pub x_min: f64,
    /// Esquina inferior-izquierda del dominio.
    pub y_min: f64,
    /// Ancho de celda en x.
    pub dx: f64,
    /// Ancho de celda en y.
    pub dy: f64,
    /// Lado en celdas (celdas totales = `res * res`).
    pub res: usize,
}

impl DomainGrid {
    /// Celdas totales (`None` si desborda `usize`).
    pub fn cell_count(&self) -> Option<usize> {
        self.res.checked_mul(self.res)
    }
}

/// Result of a GPU domain coloring evaluation: one RGBA color per grid cell.
pub type GridColors = Vec<[f32; 4]>;

/// Submit compilado y validado, listo para `submit_buffers` (origen único
/// sync/async). Todo lo que toca la GPU sale de acá; el plan es puro CPU.
struct DomainColoringSubmit {
    params: GridParamsUniform,
    code: Vec<u32>,
    constants: Vec<[f32; 2]>,
    cells: usize,
}

/// Dispatch en vuelo: el submit ya está en la GPU y la espera se distribuye
/// en frames vía [`PendingGpuReadback`]. Se resuelve con `resolve_eval`.
/// El buffer readback pertenece al pipeline (persistente), así que el `wait`
/// puede cruzar frames sin mover memoria GPU entre threads.
#[derive(Debug)]
pub struct PendingDomainColoringEval {
    cell_count: usize,
    wait: PendingGpuReadback,
}

impl PendingDomainColoringEval {
    /// Poll non-blocking delegado al waiter (para futuro slot en `canvas.rs`).
    pub fn poll(&mut self) -> ReadbackPoll {
        self.wait.poll()
    }
}

impl DomainColoringComputePipeline {
    pub fn new(device: &wgpu::Device, queue: &wgpu::Queue) -> Self {
        let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("Domain Coloring Compute Shader"),
            source: wgpu::ShaderSource::Wgsl(include_str!("domain_coloring_compute.wgsl").into()),
        });

        let bind_group_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("Domain Coloring Bind Group Layout"),
            entries: &[
                wgpu::BindGroupLayoutEntry {
                    binding: 0,
                    visibility: wgpu::ShaderStages::COMPUTE,
                    ty: wgpu::BindingType::Buffer {
                        ty: wgpu::BufferBindingType::Uniform,
                        has_dynamic_offset: false,
                        min_binding_size: None,
                    },
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 1,
                    visibility: wgpu::ShaderStages::COMPUTE,
                    ty: wgpu::BindingType::Buffer {
                        ty: wgpu::BufferBindingType::Storage { read_only: true },
                        has_dynamic_offset: false,
                        min_binding_size: None,
                    },
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 2,
                    visibility: wgpu::ShaderStages::COMPUTE,
                    ty: wgpu::BindingType::Buffer {
                        ty: wgpu::BufferBindingType::Storage { read_only: true },
                        has_dynamic_offset: false,
                        min_binding_size: None,
                    },
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 3,
                    visibility: wgpu::ShaderStages::COMPUTE,
                    ty: wgpu::BindingType::Buffer {
                        ty: wgpu::BufferBindingType::Storage { read_only: false },
                        has_dynamic_offset: false,
                        min_binding_size: None,
                    },
                    count: None,
                },
            ],
        });

        let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("Domain Coloring Pipeline Layout"),
            bind_group_layouts: &[&bind_group_layout],
            push_constant_ranges: &[],
        });

        let pipeline = device.create_compute_pipeline(&wgpu::ComputePipelineDescriptor {
            label: Some("Domain Coloring Compute Pipeline"),
            layout: Some(&pipeline_layout),
            module: &shader,
            entry_point: "cs_main",
            compilation_options: wgpu::PipelineCompilationOptions::default(),
            cache: None,
        });

        let params_buffer = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("Domain Coloring Params"),
            size: std::mem::size_of::<GridParamsUniform>() as u64,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });

        let bytecode_buffer = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("Domain Coloring Bytecode"),
            size: 4096 * std::mem::size_of::<u32>() as u64,
            usage: wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        queue.write_buffer(
            &bytecode_buffer,
            0,
            &[0u8; 4096 * std::mem::size_of::<u32>()],
        );

        let constants_buffer = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("Domain Coloring Constants"),
            size: 512 * std::mem::size_of::<f32>() as u64,
            usage: wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        queue.write_buffer(
            &constants_buffer,
            0,
            &[0u8; 512 * std::mem::size_of::<f32>()],
        );

        let color_bytes = MAX_CELLS * std::mem::size_of::<[f32; 4]>();
        let out_buffer = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("Domain Coloring Out Colors"),
            size: color_bytes as u64,
            usage: wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_SRC,
            mapped_at_creation: false,
        });

        let out_readback = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("Domain Coloring Out Readback"),
            size: color_bytes as u64,
            usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ,
            mapped_at_creation: false,
        });

        Self {
            pipeline,
            bind_group_layout,
            params_buffer,
            bytecode_buffer,
            constants_buffer,
            out_buffer,
            out_readback,
            timing: crate::gpu_timing::create(device, queue, "Domain Coloring", 1),
        }
    }

    /// Compila y valida un submit sin tocar la GPU: origen único para el path
    /// síncrono (`evaluate`) y el asíncrono (`dispatch`).
    fn plan_submit(
        expr: &ComplexExpr,
        variables: &BTreeMap<String, f64>,
        dc_mode: u32,
        grid: &DomainGrid,
    ) -> Option<DomainColoringSubmit> {
        let cells = grid.cell_count()?;
        if cells == 0 {
            return Some(DomainColoringSubmit {
                params: GridParamsUniform {
                    grid_size: 0,
                    code_len: 0,
                    dc_mode,
                    _pad1: 0,
                    grid_origin: [0.0, 0.0],
                    grid_step: [0.0, 0.0],
                    grid_res: 0,
                    _pad2: 0,
                },
                code: Vec::new(),
                constants: Vec::new(),
                cells: 0,
            });
        }
        if !domain_cells_within_budget(cells) {
            return None;
        }

        let mut prog = ComplexBytecodeProgram::default();
        if compile_complex_expr(expr, variables, &[("z", 0), ("x", 1), ("y", 2)], &mut prog)
            .is_err()
        {
            return None;
        }

        if prog.code.len() > MAX_COMPLEX_CODE || !gpu_program_is_supported(&prog.code) {
            return None;
        }
        let f32_constants = crate::complex_compute::pack_complex_constants(&prog.constants)?;

        let grid_size = u32::try_from(cells).ok()?;
        let grid_res = u32::try_from(grid.res).ok()?;
        // Origen y paso en f32 (igual que antes por punto): no finitos
        // rechazan el submit en vez de subir basura a la GPU.
        let origin = [grid.x_min as f32, grid.y_min as f32];
        let step = [grid.dx as f32, grid.dy as f32];
        if !origin.iter().chain(step.iter()).all(|v| v.is_finite()) {
            return None;
        }

        let params = GridParamsUniform {
            grid_size,
            code_len: u32::try_from(prog.code.len()).ok()?,
            dc_mode,
            _pad1: 0,
            grid_origin: origin,
            grid_step: step,
            grid_res,
            _pad2: 0,
        };
        Some(DomainColoringSubmit {
            params,
            code: prog.code,
            constants: f32_constants,
            cells,
        })
    }

    /// Escribe uniformes, hace submit del dispatch y arma el `map_async`.
    /// Barato y non-blocking: no espera a la GPU. Retorna el flag que el
    /// waiter background (o el poll síncrono legacy) observará.
    fn submit_buffers(
        &self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        submit: &DomainColoringSubmit,
    ) -> Arc<AtomicBool> {
        queue.write_buffer(
            &self.params_buffer,
            0,
            bytemuck::cast_slice(&[submit.params]),
        );
        queue.write_buffer(&self.bytecode_buffer, 0, bytemuck::cast_slice(&submit.code));
        queue.write_buffer(
            &self.constants_buffer,
            0,
            bytemuck::cast_slice(&submit.constants),
        );

        let bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("Domain Coloring Bind Group"),
            layout: &self.bind_group_layout,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: self.params_buffer.as_entire_binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: self.bytecode_buffer.as_entire_binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 2,
                    resource: self.constants_buffer.as_entire_binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 3,
                    resource: self.out_buffer.as_entire_binding(),
                },
            ],
        });

        let mut encoder = device.create_command_encoder(&wgpu::CommandEncoderDescriptor {
            label: Some("Domain Coloring Encoder"),
        });
        {
            let mut cpass = encoder.begin_compute_pass(&wgpu::ComputePassDescriptor {
                label: Some("Domain Coloring Pass"),
                // GPU timing opt-in tras la feature `profiling` (ver gpu_timing);
                // sin la feature esto es `None` — cero costo en release.
                timestamp_writes: crate::gpu_timing::timestamp_writes(&self.timing, 0),
            });
            cpass.set_pipeline(&self.pipeline);
            cpass.set_bind_group(0, &bind_group, &[]);
            let wg = (submit.params.grid_size).div_ceil(64).max(1);
            cpass.dispatch_workgroups(wg, 1, 1);
        }

        let color_bytes = (submit.cells * std::mem::size_of::<[f32; 4]>()) as u64;
        encoder.copy_buffer_to_buffer(&self.out_buffer, 0, &self.out_readback, 0, color_bytes);
        crate::gpu_timing::resolve(&self.timing, &mut encoder);
        queue.submit(std::iter::once(encoder.finish()));

        let slice = self.out_readback.slice(..color_bytes);
        let map_ok = Arc::new(AtomicBool::new(false));
        let map_ok_clone = Arc::clone(&map_ok);
        slice.map_async(wgpu::MapMode::Read, move |result| {
            if result.is_ok() {
                map_ok_clone.store(true, std::sync::atomic::Ordering::SeqCst);
            }
        });
        map_ok
    }

    /// Copia inmediata del readback ya mapeado (sin espera GPU). El llamante
    /// debe garantizar que el buffer está mapeado (`ReadbackPoll::Mapped`).
    fn copy_mapped_colors(&self, cell_count: usize) -> Option<GridColors> {
        let color_bytes = (cell_count * std::mem::size_of::<[f32; 4]>()) as u64;
        let slice = self.out_readback.slice(..color_bytes);
        let data = slice.get_mapped_range();
        let colors_f32: &[[f32; 4]] = bytemuck::cast_slice(&data);
        if colors_f32.len() < cell_count
            || colors_f32
                .iter()
                .any(|color| color.iter().any(|component| !component.is_finite()))
        {
            drop(data);
            self.out_readback.unmap();
            return None;
        }
        let result = colors_f32[..cell_count].to_vec();
        drop(data);
        self.out_readback.unmap();
        Some(result)
    }

    /// Libera el buffer readback sin bloquear. Idempotente: si el `map_async`
    /// falló o sigue pendiente, es no-op seguro; se llama en todo camino de
    /// descarte (job obsoleto, timeout, objeto borrado) para no dejar el
    /// pipeline inutilizado.
    pub fn abort_eval(&self) {
        self.out_readback.unmap();
    }

    /// Dispatch sin espera: hace submit + `map_async` y retorna
    /// inmediatamente con un [`PendingDomainColoringEval`]. El hilo del frame
    /// nunca bloquea; el resolve llega en un frame posterior vía
    /// [`Self::resolve_eval`]. Retorna `None` en los mismos casos que
    /// [`Self::evaluate`].
    pub fn dispatch(
        &self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        expr: &ComplexExpr,
        grid: &DomainGrid,
        variables: &BTreeMap<String, f64>,
        dc_mode: u32,
    ) -> Option<PendingDomainColoringEval> {
        let submit = Self::plan_submit(expr, variables, dc_mode, grid)?;
        if submit.cells == 0 {
            return None;
        }
        let cell_count = submit.cells;
        let map_ok = self.submit_buffers(device, queue, &submit);
        log::trace!("Domain coloring async dispatch (wait distributed over frames)");
        Some(PendingDomainColoringEval {
            cell_count,
            wait: PendingGpuReadback::submit(&map_ok),
        })
    }

    /// Resolve non-blocking de un dispatch previo. Solo copia si el poll ya
    /// reportó `Mapped`; en cualquier otro caso hace `unmap` y retorna `None`
    /// (el llamante usa el fallback CPU honesto). Nunca espera: el llamante
    /// debe haber hecho el `device.poll(Maintain::Poll)` no-bloqueante del
    /// frame antes de llamar.
    pub fn resolve_eval(&self, pending: PendingDomainColoringEval) -> Option<GridColors> {
        let PendingDomainColoringEval {
            cell_count,
            mut wait,
        } = pending;
        if wait.poll() != ReadbackPoll::Mapped {
            self.abort_eval();
            return None;
        }
        self.copy_mapped_colors(cell_count)
    }

    /// Evaluates the complex expression on a grid of (x, y) points and returns
    /// RGBA colors. Returns None if the expression cannot be compiled for GPU
    /// or if the grid is too large.
    ///
    /// Path síncrono legacy (bloquea hasta 250 ms): solo para callers sin slot
    /// background. El prepare usa `dispatch` + `resolve_eval`.
    pub fn evaluate(
        &self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        expr: &ComplexExpr,
        grid: &DomainGrid,
        variables: &BTreeMap<String, f64>,
        dc_mode: u32,
    ) -> Option<GridColors> {
        let submit = Self::plan_submit(expr, variables, dc_mode, grid)?;
        if submit.cells == 0 {
            return Some(Vec::new());
        }
        let cell_count = submit.cells;
        let map_ok = self.submit_buffers(device, queue, &submit);
        log::trace!("Domain coloring sync readback (bounded poll) — 1 intento por frame");
        let mapped = crate::sync_readback_with_timeout(device, &map_ok);
        crate::gpu_timing::read_and_log(&self.timing, device, "Domain Coloring");

        if !mapped {
            self.out_readback.unmap();
            return None;
        }

        self.copy_mapped_colors(cell_count)
    }
}
