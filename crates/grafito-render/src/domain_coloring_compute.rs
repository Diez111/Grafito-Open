//! GPU compute pipeline for domain coloring grid evaluation.
//!
//! Evaluates a complex expression f(z) on a 2D grid of points and returns
//! RGBA colors using HSL domain coloring (hue=arg, lightness=atan(ln(|f|))).
//! This offloads the O(N²) per-cell evaluation from CPU to GPU.

use bytemuck::{Pod, Zeroable};
use std::collections::HashMap;

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
    in_buffer: wgpu::Buffer,
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
}

/// Result of a GPU domain coloring evaluation: one RGBA color per grid cell.
pub type GridColors = Vec<[f32; 4]>;

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

        let cell_bytes = MAX_CELLS * std::mem::size_of::<[f32; 2]>();
        let in_buffer = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("Domain Coloring In Points"),
            size: cell_bytes as u64,
            usage: wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });

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
            in_buffer,
            out_buffer,
            out_readback,
            timing: crate::gpu_timing::create(device, queue, "Domain Coloring", 1),
        }
    }

    /// Evaluates the complex expression on a grid of (x, y) points and returns
    /// RGBA colors. Returns None if the expression cannot be compiled for GPU
    /// or if the grid is too large.
    pub fn evaluate(
        &self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        expr: &ComplexExpr,
        points: &[(f64, f64)],
        variables: &HashMap<String, f64>,
        dc_mode: u32,
    ) -> Option<GridColors> {
        if points.is_empty() {
            return Some(Vec::new());
        }

        if !domain_cells_within_budget(points.len()) {
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

        let grid_size = u32::try_from(points.len()).ok()?;

        let mut in_data = Vec::with_capacity(points.len());
        for &(x, y) in points {
            let x = x as f32;
            let y = y as f32;
            if !x.is_finite() || !y.is_finite() {
                return None;
            }
            in_data.push([x, y]);
        }

        let point_bytes = (points.len() * std::mem::size_of::<[f32; 2]>()) as u64;
        let color_bytes = (points.len() * std::mem::size_of::<[f32; 4]>()) as u64;

        let params = GridParamsUniform {
            grid_size,
            code_len: u32::try_from(prog.code.len()).ok()?,
            dc_mode,
            _pad1: 0,
        };

        queue.write_buffer(&self.params_buffer, 0, bytemuck::cast_slice(&[params]));
        queue.write_buffer(&self.bytecode_buffer, 0, bytemuck::cast_slice(&prog.code));
        queue.write_buffer(
            &self.constants_buffer,
            0,
            bytemuck::cast_slice(&f32_constants),
        );
        queue.write_buffer(&self.in_buffer, 0, bytemuck::cast_slice(&in_data));

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
                    resource: self.in_buffer.as_entire_binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 4,
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
            let wg = (grid_size).div_ceil(64).max(1);
            cpass.dispatch_workgroups(wg, 1, 1);
        }

        encoder.copy_buffer_to_buffer(&self.out_buffer, 0, &self.out_readback, 0, color_bytes);
        crate::gpu_timing::resolve(&self.timing, &mut encoder);
        queue.submit(std::iter::once(encoder.finish()));

        let slice = self.out_readback.slice(..color_bytes);
        let map_ok = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false));
        let map_ok_clone = map_ok.clone();
        slice.map_async(wgpu::MapMode::Read, move |result| {
            if result.is_ok() {
                map_ok_clone.store(true, std::sync::atomic::Ordering::SeqCst);
            }
        });
        // TODO P1 (B6-Next): migrar al patrón `dispatch_*` + slot background
        // (ver `function_compute`/`implicit_compute` + `GpuComputeSlot` en
        // canvas.rs). Sin waiter thread: en wgpu 22 `Device` no es `Clone`;
        // la espera se distribuye en frames (poll no-bloqueante por frame).
        // Mitigación actual: poll acotado con timeout en vez de Wait infinito.
        log::trace!("Domain coloring sync readback (bounded poll) — 1 intento por frame");
        let mapped = crate::sync_readback_with_timeout(device, &map_ok);
        crate::gpu_timing::read_and_log(&self.timing, device, "Domain Coloring");

        if !mapped {
            self.out_readback.unmap();
            return None;
        }

        let data = slice.get_mapped_range();
        let colors_f32: &[[f32; 4]] = bytemuck::cast_slice(&data);
        if colors_f32
            .iter()
            .any(|color| color.iter().any(|component| !component.is_finite()))
        {
            drop(data);
            self.out_readback.unmap();
            return None;
        }
        let result = colors_f32.to_vec();
        drop(data);
        self.out_readback.unmap();

        let _ = point_bytes;
        Some(result)
    }
}
