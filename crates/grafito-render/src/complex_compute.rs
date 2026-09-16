//! GPU compute pipeline for complex expressions on vertices.

use bytemuck::{Pod, Zeroable};
use std::collections::BTreeMap;
use std::sync::{atomic::AtomicBool, Arc};

use crate::gpu_readback::{PendingGpuReadback, ReadbackPoll};
use grafito_complex::math::complex_expr::ComplexExpr;
use grafito_complex::math::complex_opcode::{compile_complex_expr, ComplexBytecodeProgram};

const MAX_VERTICES: usize = 65536;
const MAX_COMPLEX_CONSTANTS: usize = 256;
const MAX_COMPLEX_CODE: usize = 4096;
const GPU_COMPLEX_STACK_SIZE: usize = 32;

pub(crate) fn complex_constant_pair_index(operand: u32) -> usize {
    (operand / 2) as usize
}

pub(crate) fn pack_complex_constants(constants: &[f64]) -> Option<Vec<[f32; 2]>> {
    let pair_count = complex_constant_pair_index(u32::try_from(constants.len()).ok()?);
    if !constants.len().is_multiple_of(2) || pair_count > MAX_COMPLEX_CONSTANTS {
        return None;
    }

    constants
        .as_chunks::<2>()
        .0
        .iter()
        .map(|pair| {
            let re = pair[0] as f32;
            let im = pair[1] as f32;
            (re.is_finite() && im.is_finite()).then_some([re, im])
        })
        .collect()
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
            3..=7 | 16 | 17 => {
                if stack_depth < 2 {
                    return false;
                }
                stack_depth -= 1;
            }
            8..=15 | 18 | 19 | 22..=33 | 102..=105 => {
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
    fn constants_are_addressed_as_complex_pairs() {
        assert_eq!(super::complex_constant_pair_index(0), 0);
        assert_eq!(super::complex_constant_pair_index(1), 0);
        assert_eq!(super::complex_constant_pair_index(2), 1);
    }

    #[test]
    fn cpu_only_complex_opcodes_are_rejected_before_gpu_dispatch() {
        assert!(super::gpu_program_is_supported(&[1, 2, 3, 105]));
        assert!(!super::gpu_program_is_supported(&[100]));
        assert!(!super::gpu_program_is_supported(&[109]));
    }

    #[test]
    fn programs_deeper_than_the_wgsl_stack_are_rejected_before_dispatch() {
        let mut code = vec![2; 33];
        code.extend(vec![3; 32]);

        assert!(!super::gpu_program_is_supported(&code));
    }
}

pub struct ComplexComputePipeline {
    pipeline: wgpu::ComputePipeline,
    bind_group_layout: wgpu::BindGroupLayout,
    params_buffer: wgpu::Buffer,
    bytecode_buffer: wgpu::Buffer,
    constants_buffer: wgpu::Buffer,
    in_buffer: wgpu::Buffer,
    out_buffer: wgpu::Buffer,
    out_readback: wgpu::Buffer,
    max_vertices: usize,
    /// GPU timestamp queries (feature `profiling`); no-op sin la feature.
    timing: crate::gpu_timing::GpuTimingHandle,
}

#[repr(C)]
#[derive(Copy, Clone, Pod, Zeroable)]
struct TransformParamsUniform {
    vertex_count: u32,
    code_len: u32,
    _pad0: u32,
    _pad1: u32,
}

/// Submit compilado y validado, listo para `submit_buffers` (origen único
/// sync/async). Todo lo que toca la GPU sale de acá; el plan es puro CPU.
struct ComplexSubmit {
    params: TransformParamsUniform,
    code: Vec<u32>,
    constants: Vec<[f32; 2]>,
    in_data: Vec<[f32; 2]>,
}

/// Dispatch en vuelo: el submit ya está en la GPU y la espera se distribuye
/// en frames vía [`PendingGpuReadback`]. Se resuelve con `resolve_eval`.
/// El buffer readback pertenece al pipeline (persistente), así que el `wait`
/// puede cruzar frames sin mover memoria GPU entre threads.
#[derive(Debug)]
pub struct PendingComplexEval {
    vertex_count: usize,
    wait: PendingGpuReadback,
}

impl PendingComplexEval {
    /// Poll non-blocking delegado al waiter (para futuro slot en `canvas.rs`).
    pub fn poll(&mut self) -> ReadbackPoll {
        self.wait.poll()
    }
}

impl ComplexComputePipeline {
    pub fn new(device: &wgpu::Device, queue: &wgpu::Queue) -> Self {
        let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("Complex Compute Shader"),
            source: wgpu::ShaderSource::Wgsl(include_str!("complex_compute.wgsl").into()),
        });

        let bind_group_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("Complex Compute Bind Group Layout"),
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
                        ty: wgpu::BufferBindingType::Storage { read_only: true },
                        has_dynamic_offset: false,
                        min_binding_size: None,
                    },
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 4,
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
            label: Some("Complex Compute Pipeline Layout"),
            bind_group_layouts: &[&bind_group_layout],
            push_constant_ranges: &[],
        });

        let pipeline = device.create_compute_pipeline(&wgpu::ComputePipelineDescriptor {
            label: Some("Complex Compute Pipeline"),
            layout: Some(&pipeline_layout),
            module: &shader,
            entry_point: "cs_main",
            compilation_options: wgpu::PipelineCompilationOptions::default(),
            cache: None,
        });

        let params_buffer = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("Complex Compute Params"),
            size: std::mem::size_of::<TransformParamsUniform>() as u64,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });

        let bytecode_buffer = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("Complex Compute Bytecode"),
            size: 4096 * std::mem::size_of::<u32>() as u64,
            usage: wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        // Zero-init the bytecode buffer so residual opcodes from previous
        // evaluations cannot corrupt the interpreter's stack. The WGSL shader
        // iterates over `code_len` instructions, but wgpu does not guarantee
        // zeroed storage buffers on creation.
        queue.write_buffer(
            &bytecode_buffer,
            0,
            &[0u8; 4096 * std::mem::size_of::<u32>()],
        );

        let constants_buffer = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("Complex Compute Constants"),
            size: 512 * std::mem::size_of::<f32>() as u64, // 256 constants * 2 f32s
            usage: wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        queue.write_buffer(
            &constants_buffer,
            0,
            &[0u8; 512 * std::mem::size_of::<f32>()],
        );

        let vertex_bytes = MAX_VERTICES * std::mem::size_of::<[f32; 2]>();

        let in_buffer = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("Complex Compute In Vertices"),
            size: vertex_bytes as u64,
            usage: wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });

        let out_buffer = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("Complex Compute Out Vertices"),
            size: vertex_bytes as u64,
            usage: wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_SRC,
            mapped_at_creation: false,
        });

        let out_readback = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("Complex Compute Out Readback"),
            size: vertex_bytes as u64,
            usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ,
            mapped_at_creation: false,
        });

        Self {
            pipeline,
            bind_group_layout,
            params_buffer,
            bytecode_buffer,
            constants_buffer,
            in_buffer,
            out_buffer,
            out_readback,
            max_vertices: MAX_VERTICES,
            timing: crate::gpu_timing::create(device, queue, "Complex Compute", 1),
        }
    }

    /// Compila y valida un submit sin tocar la GPU: origen único para el path
    /// síncrono (`evaluate`) y el asíncrono (`dispatch`).
    fn plan_submit(
        &self,
        expr: &ComplexExpr,
        in_points: &[grafito_geometry::Point2],
        variables: &BTreeMap<String, f64>,
    ) -> Option<ComplexSubmit> {
        if in_points.is_empty() {
            return Some(ComplexSubmit {
                params: TransformParamsUniform {
                    vertex_count: 0,
                    code_len: 0,
                    _pad0: 0,
                    _pad1: 0,
                },
                code: Vec::new(),
                constants: Vec::new(),
                in_data: Vec::new(),
            });
        }
        let mut prog = ComplexBytecodeProgram::default();
        if compile_complex_expr(expr, variables, &[("z", 0), ("x", 1), ("y", 2)], &mut prog)
            .is_err()
        {
            return None; // Fallback to CPU if compilation fails
        }

        // The compiler also accepts CPU-only special functions. Reject their
        // bytecodes so callers take the established CPU fallback instead of
        // silently receiving NaN vertices from the shader.
        if prog.code.len() > MAX_COMPLEX_CODE || !gpu_program_is_supported(&prog.code) {
            return None;
        }
        let f32_constants = pack_complex_constants(&prog.constants)?;

        if in_points.len() > self.max_vertices {
            return None;
        }
        let vertex_count = u32::try_from(in_points.len()).ok()?;

        let mut in_data = Vec::with_capacity(in_points.len());
        for p in in_points {
            let x = p.x as f32;
            let y = p.y as f32;
            if !x.is_finite() || !y.is_finite() {
                return None;
            }
            in_data.push([x, y]);
        }

        let params = TransformParamsUniform {
            vertex_count,
            code_len: u32::try_from(prog.code.len()).ok()?,
            _pad0: 0,
            _pad1: 0,
        };
        Some(ComplexSubmit {
            params,
            code: prog.code,
            constants: f32_constants,
            in_data,
        })
    }

    /// Escribe uniformes, hace submit del dispatch y arma el `map_async`.
    /// Barato y non-blocking: no espera a la GPU. Retorna el flag que el
    /// waiter background (o el poll síncrono legacy) observará.
    fn submit_buffers(
        &self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        submit: &ComplexSubmit,
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
        queue.write_buffer(&self.in_buffer, 0, bytemuck::cast_slice(&submit.in_data));

        let bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("Complex Compute Bind Group"),
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
                    resource: self.in_buffer.as_entire_binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 4,
                    resource: self.out_buffer.as_entire_binding(),
                },
            ],
        });

        let mut encoder = device.create_command_encoder(&wgpu::CommandEncoderDescriptor {
            label: Some("Complex Compute Encoder"),
        });
        {
            let mut cpass = encoder.begin_compute_pass(&wgpu::ComputePassDescriptor {
                label: Some("Complex Compute Pass"),
                // GPU timing opt-in tras la feature `profiling` (ver gpu_timing);
                // sin la feature esto es `None` — cero costo en release.
                timestamp_writes: crate::gpu_timing::timestamp_writes(&self.timing, 0),
            });
            cpass.set_pipeline(&self.pipeline);
            cpass.set_bind_group(0, &bind_group, &[]);
            let wg = (submit.params.vertex_count).div_ceil(64).max(1);
            cpass.dispatch_workgroups(wg, 1, 1);
        }

        let vertex_bytes = (submit.in_data.len() * std::mem::size_of::<[f32; 2]>()) as u64;
        encoder.copy_buffer_to_buffer(&self.out_buffer, 0, &self.out_readback, 0, vertex_bytes);
        crate::gpu_timing::resolve(&self.timing, &mut encoder);
        queue.submit(std::iter::once(encoder.finish()));

        let slice = self.out_readback.slice(..vertex_bytes);
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
    fn copy_mapped_points(&self, vertex_count: usize) -> Option<Vec<grafito_geometry::Point2>> {
        let vertex_bytes = (vertex_count * std::mem::size_of::<[f32; 2]>()) as u64;
        let slice = self.out_readback.slice(..vertex_bytes);
        let data = slice.get_mapped_range();
        let values_f32: &[[f32; 2]] = bytemuck::cast_slice(&data);
        if values_f32.len() < vertex_count
            || values_f32
                .iter()
                .any(|value| !value[0].is_finite() || !value[1].is_finite())
        {
            drop(data);
            self.out_readback.unmap();
            return None;
        }
        let mut result = Vec::with_capacity(vertex_count);
        for v in &values_f32[..vertex_count] {
            result.push(grafito_geometry::Point2::new(v[0] as f64, v[1] as f64));
        }
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
    /// inmediatamente con un [`PendingComplexEval`]. El hilo del frame nunca
    /// bloquea; el resolve llega en un frame posterior vía
    /// [`Self::resolve_eval`]. Retorna `None` en los mismos casos que
    /// [`Self::evaluate`].
    pub fn dispatch(
        &self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        expr: &ComplexExpr,
        in_points: &[grafito_geometry::Point2],
        variables: &BTreeMap<String, f64>,
    ) -> Option<PendingComplexEval> {
        let submit = self.plan_submit(expr, in_points, variables)?;
        if submit.in_data.is_empty() {
            return None;
        }
        let vertex_count = submit.in_data.len();
        let map_ok = self.submit_buffers(device, queue, &submit);
        log::trace!("Complex compute async dispatch (wait distributed over frames)");
        Some(PendingComplexEval {
            vertex_count,
            wait: PendingGpuReadback::submit(&map_ok),
        })
    }

    /// Resolve non-blocking de un dispatch previo. Solo copia si el poll ya
    /// reportó `Mapped`; en cualquier otro caso hace `unmap` y retorna `None`
    /// (el llamante usa el fallback CPU honesto). Nunca espera: el llamante
    /// debe haber hecho el `device.poll(Maintain::Poll)` no-bloqueante del
    /// frame antes de llamar.
    pub fn resolve_eval(
        &self,
        pending: PendingComplexEval,
    ) -> Option<Vec<grafito_geometry::Point2>> {
        let PendingComplexEval {
            vertex_count,
            mut wait,
        } = pending;
        if wait.poll() != ReadbackPoll::Mapped {
            self.abort_eval();
            return None;
        }
        self.copy_mapped_points(vertex_count)
    }

    /// Evaluates the complex expression on a set of vertices
    /// Returns the transformed vec2 points, or None if AST unsupported
    ///
    /// Path síncrono legacy (bloquea hasta 250 ms): solo para callers sin slot
    /// background. El prepare usa `dispatch` + `resolve_eval`.
    pub fn evaluate(
        &self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        expr: &ComplexExpr,
        in_points: &[grafito_geometry::Point2],
        variables: &BTreeMap<String, f64>,
    ) -> Option<Vec<grafito_geometry::Point2>> {
        if in_points.is_empty() {
            return Some(Vec::new());
        }
        let submit = self.plan_submit(expr, in_points, variables)?;
        let vertex_count = submit.in_data.len();
        let map_ok = self.submit_buffers(device, queue, &submit);
        log::trace!("Complex compute sync readback (bounded poll) — 1 intento por frame");
        let mapped = crate::sync_readback_with_timeout(device, &map_ok);
        crate::gpu_timing::read_and_log(&self.timing, device, "Complex Compute");

        if !mapped {
            // `unmap` is idempotent: when `map_async` reported an
            // error the buffer was never mapped, so this is a no-op.
            self.out_readback.unmap();
            return None;
        }

        self.copy_mapped_points(vertex_count)
    }
}
