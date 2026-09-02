//! GPU compute pipeline for complex expressions on vertices.

use bytemuck::{Pod, Zeroable};
use std::collections::HashMap;

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
}

#[repr(C)]
#[derive(Copy, Clone, Pod, Zeroable)]
struct TransformParamsUniform {
    vertex_count: u32,
    code_len: u32,
    _pad0: u32,
    _pad1: u32,
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
        }
    }

    /// Evaluates the complex expression on a set of vertices
    /// Returns the transformed vec2 points, or None if AST unsupported
    pub fn evaluate(
        &self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        expr: &ComplexExpr,
        in_points: &[grafito_geometry::Point2],
        variables: &HashMap<String, f64>,
    ) -> Option<Vec<grafito_geometry::Point2>> {
        if in_points.is_empty() {
            return Some(Vec::new());
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

        let vertex_bytes = (in_points.len() * std::mem::size_of::<[f32; 2]>()) as u64;

        let params = TransformParamsUniform {
            vertex_count,
            code_len: u32::try_from(prog.code.len()).ok()?,
            _pad0: 0,
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
                timestamp_writes: None,
            });
            cpass.set_pipeline(&self.pipeline);
            cpass.set_bind_group(0, &bind_group, &[]);
            let wg = (vertex_count).div_ceil(64).max(1);
            cpass.dispatch_workgroups(wg, 1, 1);
        }

        encoder.copy_buffer_to_buffer(&self.out_buffer, 0, &self.out_readback, 0, vertex_bytes);
        queue.submit(std::iter::once(encoder.finish()));

        let slice = self.out_readback.slice(..vertex_bytes);
        let map_ok = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false));
        let map_ok_clone = map_ok.clone();
        slice.map_async(wgpu::MapMode::Read, move |result| {
            if result.is_ok() {
                map_ok_clone.store(true, std::sync::atomic::Ordering::SeqCst);
            }
        });
        // TODO P1: mover a spawn_blocking — Wait bloquea el hilo de prepare (acotado a 1 intento por frame)
        log::trace!("Complex compute sync readback (Wait) — bloqueante, 1 intento por frame");
        device.poll(wgpu::Maintain::Wait);

        if !map_ok.load(std::sync::atomic::Ordering::SeqCst) {
            // `unmap` is idempotent: when `map_async` reported an
            // error the buffer was never mapped, so this is a no-op.
            self.out_readback.unmap();
            return None;
        }

        let data = slice.get_mapped_range();
        let values_f32: &[[f32; 2]] = bytemuck::cast_slice(&data);
        if values_f32
            .iter()
            .any(|value| !value[0].is_finite() || !value[1].is_finite())
        {
            drop(data);
            self.out_readback.unmap();
            return None;
        }
        let mut result = Vec::with_capacity(values_f32.len());
        for v in values_f32 {
            result.push(grafito_geometry::Point2::new(v[0] as f64, v[1] as f64));
        }
        drop(data);
        self.out_readback.unmap();

        Some(result)
    }
}
