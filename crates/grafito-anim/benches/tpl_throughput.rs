#![allow(clippy::unwrap_used, clippy::expect_used)]
#![allow(deprecated)]
//! Throughput de los samplers representativos de los 9 `tpl_*.rs`.
//!
//! Qué mide (1 frame representativo por bench, frame medio de un set de 48):
//!
//! | Bench | Módulo | Sampler (1 frame) |
//! |---|---|---|
//! | `tpl3d_toro_1f` | `tpl_3d` | `muestra_frame("sup-toro-rotante", …, 24, 48, 2000)` (res N/A: matemática pura) |
//! | `tpl4d_tesseract_{640x480,1280x720}_1f` | `tpl_4d` | `render_4d_rango("tesseract-xw", w, h, 48, 24, 1)` (1 frame RGBA) |
//! | `tplam1_riemann_suma_1f` | `tpl_am1` | `RiemannSums(0,2, 8..=256).suma_en(24)` (res N/A) |
//! | `tplam2_cuadros_1f` | `tpl_am2` | `Descenso::pos_en + Parciales::cuadro + DobleIntegral::cuadro` (res N/A) |
//! | `tplchaos_mandel_64x48_1f` | `tpl_chaos` | `grilla_mandelbrot(-0.5, 0, 3.0, 64, 48, 64)` = tope de celdas (res N/A directa) |
//! | `tpledo_fourier_traza_16x512_1f` | `tpl_edo` | `traza_fourier(16, 512)` = topes de términos y muestras (res N/A) |
//! | `tplgraphs_force_{640x480,1280x720}_1f` | `tpl_graphs` | `render_graphs_rango("force-directed", w, h, 48, 24, 1)` |
//! | `tpllinalg_matriz_{640x480,1280x720}_1f` | `tpl_linalg` | `render_linalg_rango("matriz-transformacion", w, h, 48, 24, 1)` |
//! | `tplpipeline_fxaa_{640x480,1280x720}_1f` | `tpl_pipeline` | `fxaa_lite` sobre frame sólido (peor caso: todo borde) |
//! | `tplpipeline_downsample_{640x480,1280x720}_1f` | `tpl_pipeline` | `downsample_box(factor 2)` (SSAA resolve) |
//! | `tplstats_curva_1f` | `tpl_stats` | `NormalAnim::curva_en(129 pts) + BinomialAnim::barras_en(32 barras)` (res N/A) |
//!
//! Dónde aplica la resolución: solo los samplers RGBA (`tpl_4d`, `tpl_graphs`,
//! `tpl_linalg`, `tpl_pipeline`) escalan con `w×h`; el resto es matemática
//! pura con costo independiente del canvas (se mide una vez y se declara N/A).
//! `tpl_chaos` escala con la grilla (`nx×ny`, tope 64×48 = 3072 celdas): no se
//! muestrea a resolución de canvas porque el tope del módulo lo prohíbe; se
//! reporta costo por celda para extrapolar.
//!
//! Picos de memoria (método): estima analítica, sin allocator real (sin dhat):
//! `estima_bytes(w,h,frames) = w*h*4*frames` con checked (`None` = desborde)
//! para los sets RGBA + tamaño de la salida acotada en los samplers puros
//! (grilla 64×48×u32, traza 512×`[f64; 2]`, curva 129×`[f64; 2]`, barras
//! 32×f32). Allocs por frame RGBA: 1 `Vec<u8>` píxel-buffer + 1 slot del
//! `Vec` de frames; los samplers puros no alocan fuera de su salida acotada.
//! Los `println!("perf-base …")` del setup imprimen los picos para el log CI.
//!
//! Config corta para CI: 15 muestras, 1 s de medición (paridad
//! `grafito-render/benches/cache_keys.rs`).
//!
//! Re-correr: `cargo bench -p grafito-anim --bench tpl_throughput --locked -- --quick`
//!
//! ## Resultados (medidos 2026-09-27, `--quick`, release, este box)
//!
//! | Bench | Mediana/frame | Pico mem frame/set |
//! |---|---|---|
//! | tpl4d_tesseract 640×480 / 1280×720 | 0.41 ms / 1.14 ms | 1.2 MiB / 3.7 MiB |
//! | tplgraphs_force 640×480 / 1280×720 | 0.32 ms / 1.01 ms | idem |
//! | tpllinalg_matriz 640×480 / 1280×720 | 0.45 ms / 1.22 ms | idem |
//! | tlpipeline_fxaa 640×480 / 1280×720 | 7.21 ms / 21.38 ms | 1 alloc frame (in+out) |
//! | tlpipeline_downsample×2 640×480 / 1280×720 | 1.36 ms / 4.30 ms | idem |
//! | tpl3d_toro_1f | 53 µs | salida acotada (malla res=32) |
//! | tplam1_riemann_suma_1f | 2.1 µs | sin alloc (1 f64) |
//! | tplam2_cuadros_1f | 32 ns | sin alloc (3 structs Copy) |
//! | tplchaos_mandel_64×48_1f | 120 µs (39 ns/celda) | 12 KiB grilla |
//! | tpledo_fourier_traza_16×512_1f | 121 µs | 8 KiB traza |
//! | tplstats_curva_1f | 1.8 µs | 2.1 KiB curva + 128 B barras |
//!
//! Más caro: `tpl_pipeline::fxaa_lite` a 1280×720 (~21.4 ms, ~65 % del
//! presupuesto; a 640×480 ~7.2 ms). Segundo: `downsample_box` a 720p
//! (~4.3 ms). Los renderers RGBA (4d/graphs/linalg) entran holgados
//! (≤1.3 ms); los samplers puros son despreciables (ns–µs).
//!
//! Veredicto 33 ms/frame (30 fps): cada sampler individual ENTRA a ambas
//! resoluciones. Pero la pila completa por frame a 720p
//! (render ~1.2 + fxaa ~21.4 + downsample ~4.3 ≈ 27 ms) queda al ~82 % del
//! presupuesto: fxaa+downsample juntos a 720p solo si no hay nada más;
//! para componer con margen, bajar a 640×480 (pila ≈ 9 ms) o aplicar fxaa
//! una vez (no por frame) o por rangos. Memoria: set de 48 a 640×480 =
//! 56.25 MiB (entra en el chunk de 64 MiB, 54 frames/chunk); a 1280×720 =
//! 168.75 MiB (NO entra: componer por rangos de 18 frames, drenando).
//!
//! Gate: informativo, sin gate en CI (como `native_frames`).

use criterion::{black_box, criterion_group, criterion_main, Criterion, Throughput};
use std::time::Duration;

const TOTAL_48: usize = 48;
const MID_24: usize = 24;

const W640: u32 = 640;
const H480: u32 = 480;
const W720P: u32 = 1280;
const H720P: u32 = 720;

/// Bytes de 1 frame RGBA (w·h·4, sin desborde a estas resoluciones).
const fn bytes_frame(w: u32, h: u32) -> u64 {
    w as u64 * h as u64 * 4
}

fn picos_memoria() {
    // Picos analíticos del setup (visibles en el log; paridad F5-base).
    let set_640 = grafito_anim::tpl_4d::estima_bytes(W640, H480, TOTAL_48);
    let set_720 = grafito_anim::tpl_4d::estima_bytes(W720P, H720P, TOTAL_48);
    println!(
        "perf-base tpl_throughput: set48_640x480={:?} set48_1280x720={:?} chunk640={} chunk720={}",
        set_640,
        set_720,
        grafito_anim::tpl_4d::frames_por_chunk(W640, H480),
        grafito_anim::tpl_4d::frames_por_chunk(W720P, H720P),
    );
    println!(
        "perf-base tpl_throughput puros: chaos_celdas={} ({} B) edo_traza_pts=512 ({} B) stats_curva_pts={} stats_barras=32",
        64_usize * 48_usize,
        64_usize * 48_usize * size_of::<u32>(),
        512_usize * size_of::<[f64; 2]>(),
        grafito_anim::tpl_stats::STATS_CURVA_PTS,
    );
    // Chequeo de presupuestos del setup (falla honesto si cambian los topes).
    assert_eq!(set_640, Some(58_982_400));
    assert_eq!(set_720, Some(176_947_200));
    assert!(set_640.is_some_and(|n| n <= 64 * 1024 * 1024));
    assert!(set_720.is_some_and(|n| n > 64 * 1024 * 1024));
}

fn bench_rgba_640(c: &mut Criterion) {
    let mut g = c.benchmark_group("rgba_640x480");
    g.throughput(Throughput::Bytes(bytes_frame(W640, H480)));
    g.bench_function("tpl4d_tesseract_640x480_1f", |b| {
        b.iter(|| {
            grafito_anim::tpl_4d::render_4d_rango(
                black_box("tesseract-xw"),
                black_box(W640),
                black_box(H480),
                black_box(TOTAL_48),
                black_box(MID_24),
                black_box(1),
            )
            .expect("frame 4d valido")
        })
    });
    g.bench_function("tplgraphs_force_640x480_1f", |b| {
        b.iter(|| {
            grafito_anim::tpl_graphs::render_graphs_rango(
                black_box("force-directed"),
                black_box(W640),
                black_box(H480),
                black_box(TOTAL_48),
                black_box(MID_24),
                black_box(1),
            )
            .expect("frame graphs valido")
        })
    });
    g.bench_function("tpllinalg_matriz_640x480_1f", |b| {
        b.iter(|| {
            grafito_anim::tpl_linalg::render_linalg_rango(
                black_box("matriz-transformacion"),
                black_box(W640),
                black_box(H480),
                black_box(TOTAL_48),
                black_box(MID_24),
                black_box(1),
            )
            .expect("frame linalg valido")
        })
    });
    let fixture = grafito_anim::tpl_pipeline::RgbaFrame::solido(W640, H480, [200, 210, 255, 255])
        .expect("fixture 640 valido");
    g.bench_function("tplpipeline_fxaa_640x480_1f", |b| {
        b.iter(|| grafito_anim::tpl_pipeline::fxaa_lite(black_box(&fixture)).expect("fxaa valido"))
    });
    g.bench_function("tplpipeline_downsample_640x480_1f", |b| {
        b.iter(|| {
            grafito_anim::tpl_pipeline::downsample_box(black_box(&fixture), black_box(2))
                .expect("downsample valido")
        })
    });
    g.finish();
}

fn bench_rgba_720(c: &mut Criterion) {
    let mut g = c.benchmark_group("rgba_1280x720");
    g.throughput(Throughput::Bytes(bytes_frame(W720P, H720P)));
    g.bench_function("tpl4d_tesseract_1280x720_1f", |b| {
        b.iter(|| {
            grafito_anim::tpl_4d::render_4d_rango(
                black_box("tesseract-xw"),
                black_box(W720P),
                black_box(H720P),
                black_box(TOTAL_48),
                black_box(MID_24),
                black_box(1),
            )
            .expect("frame 4d valido")
        })
    });
    g.bench_function("tplgraphs_force_1280x720_1f", |b| {
        b.iter(|| {
            grafito_anim::tpl_graphs::render_graphs_rango(
                black_box("force-directed"),
                black_box(W720P),
                black_box(H720P),
                black_box(TOTAL_48),
                black_box(MID_24),
                black_box(1),
            )
            .expect("frame graphs valido")
        })
    });
    g.bench_function("tpllinalg_matriz_1280x720_1f", |b| {
        b.iter(|| {
            grafito_anim::tpl_linalg::render_linalg_rango(
                black_box("matriz-transformacion"),
                black_box(W720P),
                black_box(H720P),
                black_box(TOTAL_48),
                black_box(MID_24),
                black_box(1),
            )
            .expect("frame linalg valido")
        })
    });
    let fixture = grafito_anim::tpl_pipeline::RgbaFrame::solido(W720P, H720P, [200, 210, 255, 255])
        .expect("fixture 720 valido");
    g.bench_function("tplpipeline_fxaa_1280x720_1f", |b| {
        b.iter(|| grafito_anim::tpl_pipeline::fxaa_lite(black_box(&fixture)).expect("fxaa valido"))
    });
    g.bench_function("tplpipeline_downsample_1280x720_1f", |b| {
        b.iter(|| {
            grafito_anim::tpl_pipeline::downsample_box(black_box(&fixture), black_box(2))
                .expect("downsample valido")
        })
    });
    g.finish();
}

fn bench_puros(c: &mut Criterion) {
    picos_memoria();

    c.bench_function("tpl3d_toro_1f", |b| {
        let params = grafito_anim::tpl_3d::Params3D::por_defecto("sup-toro-rotante");
        b.iter(|| {
            grafito_anim::tpl_3d::muestra_frame(
                black_box("sup-toro-rotante"),
                black_box(&params),
                black_box(MID_24),
                black_box(TOTAL_48),
                black_box(2000),
            )
            .expect("muestra 3d valida")
        })
    });

    c.bench_function("tplam1_riemann_suma_1f", |b| {
        let riemann = grafito_anim::tpl_am1::RiemannSums::try_new(
            0.0,
            2.0,
            8,
            256,
            TOTAL_48,
            grafito_anim::tpl_am1::RiemannRegla::PuntoMedio,
            grafito_anim::tpl_am1::RiemannFuncion::Cuadratica,
        )
        .expect("riemann valido");
        b.iter(|| black_box(riemann.suma_en(black_box(MID_24))))
    });

    c.bench_function("tplam2_cuadros_1f", |b| {
        let descenso = grafito_anim::tpl_am2::Descenso::try_new(2.0, 2.0, 0.2, TOTAL_48)
            .expect("descenso valido");
        let parciales = grafito_anim::tpl_am2::Parciales::try_new(1.0, 0.5, -2.0, 2.0)
            .expect("parciales validos");
        let doble = grafito_anim::tpl_am2::DobleIntegral::try_new(-2.0, 2.0).expect("doble valida");
        b.iter(|| {
            let a = descenso.pos_en(black_box(0.5));
            let c = parciales.cuadro(black_box(MID_24));
            let d = doble.cuadro(black_box(MID_24));
            black_box((a, c, d))
        })
    });

    c.bench_function("tplchaos_mandel_64x48_1f", |b| {
        b.iter(|| {
            grafito_anim::tpl_chaos::grilla_mandelbrot(
                black_box(-0.5),
                black_box(0.0),
                black_box(3.0),
                black_box(64),
                black_box(48),
                black_box(64),
            )
            .expect("grilla valida")
        })
    });

    c.bench_function("tpledo_fourier_traza_16x512_1f", |b| {
        b.iter(|| {
            grafito_anim::tpl_edo::traza_fourier(black_box(16), black_box(512))
                .expect("traza valida")
        })
    });

    c.bench_function("tplstats_curva_1f", |b| {
        let normal = grafito_anim::tpl_stats::NormalAnim::try_new(-1.0, 1.0, 1.0, 0.5)
            .expect("normal valida");
        let binomial =
            grafito_anim::tpl_stats::BinomialAnim::try_new(31, 0.2, 0.8).expect("binomial valida");
        let mut curva = vec![[0.0_f64, 0.0_f64]; grafito_anim::tpl_stats::STATS_CURVA_PTS];
        let mut barras = vec![0.0_f32; 32];
        b.iter(|| {
            let n = normal
                .curva_en(black_box(0.5), black_box(curva.as_mut_slice()))
                .expect("curva valida");
            binomial
                .barras_en(black_box(0.5), black_box(barras.as_mut_slice()))
                .expect("barras validas");
            black_box(n)
        })
    });
}

criterion_group!(
    name = benches;
    config = Criterion::default()
        .sample_size(15)
        .measurement_time(Duration::from_secs(1))
        .warm_up_time(Duration::from_millis(500));
    targets = bench_rgba_640, bench_rgba_720, bench_puros
);
criterion_main!(benches);
