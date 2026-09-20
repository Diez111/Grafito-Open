//! Sonda GPU + micro-cómputo real (WGSL) con fallback honesto.
//!
//! Enumera adaptadores del `wgpu` lockeado del workspace (22.x, el mismo que
//! renderiza Grafito) y corre un conteo de pares unitarios del cuadrado
//! unidad en un compute shader, comparando contra CPU (= 4). Si no hay
//! adaptador, driver o el device tarda más de 25 s, devuelve error honesto
//! en español en vez de panickear: el laboratorio sigue en CPU.
//!
//! Todo el trabajo vive en un hilo hijo con `recv_timeout`: un driver colgado
//! jamás cuelga el loop stdio.

use serde_json::{json, Value};
use std::sync::mpsc;
use std::time::{Duration, Instant};

/// Timeout total de la sonda (adquisición + cómputo + readback).
const PROBE_TIMEOUT: Duration = Duration::from_secs(25);
/// Timeout del readback tras el submit.
const READBACK_TIMEOUT: Duration = Duration::from_secs(10);

/// Cuadrado unidad como 4×vec2f (el CPU cuenta 4 pares unitarios).
const SQUARE_PTS: [f32; 8] = [0.0, 0.0, 1.0, 0.0, 1.0, 1.0, 0.0, 1.0];

const SHADER: &str = r#"
struct Pts { p: array<vec2f, 4> };
@group(0) @binding(0) var<storage, read> pts: Pts;
@group(0) @binding(1) var<storage, read_write> cnt: atomic<u32>;
@compute @workgroup_size(1)
fn main() {
    var c = 0u;
    for (var i = 0u; i < 4u; i++) {
        for (var j = i + 1u; j < 4u; j++) {
            let d = distance(pts.p[i], pts.p[j]);
            if (abs(d - 1.0) < 0.001) { c += 1u; }
        }
    }
    atomicStore(&cnt, c);
}
"#;

pub fn gpu_tool_defs() -> Vec<Value> {
    vec![json!({
        "name": "gpu_probe",
        "description": "Detecta GPUs vía wgpu y corre un micro-cómputo real (conteo unitario del cuadrado en WGSL) comparado contra CPU. Si no hay adaptador/driver, error honesto: el lab sigue en CPU.",
        "inputSchema": {"type": "object", "properties": {}},
        "annotations": {"readOnlyHint": true, "destructiveHint": false}
    })]
}

/// `gpu_probe()`: adaptadores + micro-cómputo o error honesto.
pub fn gpu_probe() -> Result<Value, String> {
    let (tx, rx) = mpsc::channel();
    std::thread::spawn(move || {
        let _ = tx.send(probe_inner());
    });
    rx.recv_timeout(PROBE_TIMEOUT).map_err(|_| {
        "gpu_probe: timeout (>25 s) adquiriendo la GPU; el laboratorio sigue en CPU (rayon)"
            .to_string()
    })?
}

fn probe_inner() -> Result<Value, String> {
    let instance = wgpu::Instance::new(wgpu::InstanceDescriptor {
        backends: wgpu::Backends::all(),
        ..Default::default()
    });
    let mut adapters_out = Vec::new();
    for adapter in instance.enumerate_adapters(wgpu::Backends::all()) {
        let info = adapter.get_info();
        adapters_out.push(json!({
            "name": info.name,
            "backend": format!("{:?}", info.backend),
            "device": info.device,
            "driver": info.driver,
        }));
    }
    if adapters_out.is_empty() {
        return Err(
            "gpu_probe: sin adaptadores GPU visibles para wgpu (sin driver Vulkan/Metal/DX12 o headless sin ICD); el laboratorio sigue en CPU (rayon)".to_string(),
        );
    }
    // Primer adaptador que acepte un device; si ninguno, error honesto.
    let mut last_err = String::from("ningún adaptador aceptó device");
    for adapter in instance.enumerate_adapters(wgpu::Backends::all()) {
        match pollster::block_on(adapter.request_device(
            &wgpu::DeviceDescriptor {
                label: Some("grafito-mcp-probe"),
                required_features: wgpu::Features::empty(),
                required_limits: wgpu::Limits::default(),
                memory_hints: wgpu::MemoryHints::default(),
            },
            None,
        )) {
            Ok((device, queue)) => return run_micro(&device, &queue, &adapters_out),
            Err(e) => last_err = format!("device rechazado: {e}"),
        }
    }
    Err(format!(
        "gpu_probe: {last_err}; el laboratorio sigue en CPU"
    ))
}

fn run_micro(
    device: &wgpu::Device,
    queue: &wgpu::Queue,
    adapters: &[Value],
) -> Result<Value, String> {
    use wgpu::util::DeviceExt;
    let mut bytes = Vec::with_capacity(32);
    for v in SQUARE_PTS {
        bytes.extend_from_slice(&v.to_le_bytes());
    }
    let pts = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
        label: Some("pts"),
        contents: &bytes,
        usage: wgpu::BufferUsages::STORAGE,
    });
    let out = device.create_buffer(&wgpu::BufferDescriptor {
        label: Some("cnt"),
        size: 4,
        usage: wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_SRC,
        mapped_at_creation: false,
    });
    let staging = device.create_buffer(&wgpu::BufferDescriptor {
        label: Some("staging"),
        size: 4,
        usage: wgpu::BufferUsages::MAP_READ | wgpu::BufferUsages::COPY_DST,
        mapped_at_creation: false,
    });
    let module = device.create_shader_module(wgpu::ShaderModuleDescriptor {
        label: Some("unit_count"),
        source: wgpu::ShaderSource::Wgsl(SHADER.into()),
    });
    let layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
        label: Some("bgl"),
        entries: &[
            wgpu::BindGroupLayoutEntry {
                binding: 0,
                visibility: wgpu::ShaderStages::COMPUTE,
                ty: wgpu::BindingType::Buffer {
                    ty: wgpu::BufferBindingType::Storage { read_only: true },
                    has_dynamic_offset: false,
                    min_binding_size: None,
                },
                count: None,
            },
            wgpu::BindGroupLayoutEntry {
                binding: 1,
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
        label: Some("pl"),
        bind_group_layouts: &[&layout],
        push_constant_ranges: &[],
    });
    let pipeline = device.create_compute_pipeline(&wgpu::ComputePipelineDescriptor {
        label: Some("unit_count"),
        layout: Some(&pipeline_layout),
        module: &module,
        entry_point: "main",
        compilation_options: wgpu::PipelineCompilationOptions::default(),
        cache: None,
    });
    let bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
        label: Some("bg"),
        layout: &layout,
        entries: &[
            wgpu::BindGroupEntry {
                binding: 0,
                resource: pts.as_entire_binding(),
            },
            wgpu::BindGroupEntry {
                binding: 1,
                resource: out.as_entire_binding(),
            },
        ],
    });
    let mut encoder =
        device.create_command_encoder(&wgpu::CommandEncoderDescriptor { label: Some("enc") });
    {
        let mut pass = encoder.begin_compute_pass(&wgpu::ComputePassDescriptor {
            label: Some("pass"),
            timestamp_writes: None,
        });
        pass.set_pipeline(&pipeline);
        pass.set_bind_group(0, &bind_group, &[]);
        pass.dispatch_workgroups(1, 1, 1);
    }
    encoder.copy_buffer_to_buffer(&out, 0, &staging, 0, 4);
    queue.submit(Some(encoder.finish()));

    let done = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false));
    let done2 = done.clone();
    staging.slice(..).map_async(wgpu::MapMode::Read, move |_| {
        done2.store(true, std::sync::atomic::Ordering::SeqCst);
    });
    let deadline = Instant::now() + READBACK_TIMEOUT;
    while !done.load(std::sync::atomic::Ordering::SeqCst) {
        if Instant::now() >= deadline {
            return Err(
                "gpu_probe: readback agotado (>10 s); el laboratorio sigue en CPU".to_string(),
            );
        }
        device.poll(wgpu::Maintain::Poll);
        std::thread::yield_now();
    }
    let view = staging.slice(..).get_mapped_range();
    let raw: [u8; 4] = [view[0], view[1], view[2], view[3]];
    drop(view);
    staging.unmap();
    let gpu_count = u32::from_le_bytes(raw);
    let cpu_count = crate::unit_pairs_spatial(
        &[
            grafito_geometry::Point2::new(0.0, 0.0),
            grafito_geometry::Point2::new(1.0, 0.0),
            grafito_geometry::Point2::new(1.0, 1.0),
            grafito_geometry::Point2::new(0.0, 1.0),
        ],
        1e-9,
    )
    .map_err(|e| format!("gpu_probe: CPU falló ({e}); GPU={gpu_count}"))?;
    Ok(json!({
        "tool": "gpu_probe",
        "adapters": adapters,
        "micro_compute": {"gpu": gpu_count, "cpu": cpu_count, "match": gpu_count as usize == cpu_count},
        "note": if gpu_count as usize == cpu_count {
            "GPU y CPU coinciden en el cuadrado unidad; el camino compute está vivo"
        } else {
            "DIFIEREN: no usar la GPU para búsqueda hasta auditar el shader"
        },
    }))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn defs_tienen_una_tool() {
        assert_eq!(gpu_tool_defs().len(), 1);
    }

    #[test]
    fn probe_nunca_panickea() {
        // Con GPU: match:true. Sin GPU/driver: error honesto en español.
        // El test jamás falla por ausencia de hardware.
        match gpu_probe() {
            Ok(v) => {
                assert_eq!(v["tool"], json!("gpu_probe"));
                assert!(
                    v["micro_compute"]["match"] == json!(true),
                    "GPU difiere del CPU: {v}"
                );
            }
            Err(e) => {
                assert!(!e.is_empty());
                let low = e.to_lowercase();
                assert!(
                    low.contains("gpu")
                        || low.contains("adaptador")
                        || low.contains("cpu")
                        || low.contains("timeout"),
                    "error no honesto: {e}"
                );
            }
        }
    }
}
