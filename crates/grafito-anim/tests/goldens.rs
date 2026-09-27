//! Goldens de regresión visual de los 69 templates.
//!
//! Cobertura: `CANONICAL_TEMPLATES` (13 canónicas + 52 de los 9 `tpl_*` + 3 de `tpl_extra`).
//! Un test recorre los 69 y pinea por template `(kind, w, h, fnv1a)`.
//!
//! Kinds:
//!
//! - `rgba` (16: linalg 6 + 4d 5 + graphs 5): 1 frame real de 96×72
//!   vía `render_*_rango(id, 96, 72, 8, 4, 1)`; hash FNV-1a del RGBA.
//! - `geom` (17: 3d 7 vía `muestra_frame` + chaos 5 y edo 5 vía
//!   `escena_para` a alpha 0.5): sin renderer RGBA en este crate
//!   (devuelven geometría que el frente proyecta); se pinea el hash
//!   del `Debug`. Dims `0×0` = sin frame.
//! - `spec` (20: am1 6 + am2 6 + stats 8): sin renderer directo;
//!   se pinea el hash del `Debug` de su salida determinista
//!   (`describe`/`ficha`/`titulo_para` + claves vivas). Sin frame.
//! - `proto` (13 canónicas): su RGBA vive en `grafito-app`
//!   (`anim_native`, fuera de este crate); se pinea el `Debug` de
//!   `request_for_concept(id, id)`. Sin frame.
//!
//! Sin dependencias nuevas (crate + std), sin red. Todo error o panic
//! de un template se REPORTA como fallo del test con su id (no hay skips).

use std::panic::AssertUnwindSafe;

const W: u32 = 96;
const H: u32 = 72;
const TOTAL: usize = 8;
const MID: usize = 4;

/// FNV-1a 64 (offset 14695981039346656037, prime 1099511628211). Puro.
fn fnv1a64(bytes: &[u8]) -> u64 {
    let mut h: u64 = 14_695_981_039_346_656_037;
    for b in bytes {
        h ^= u64::from(*b);
        h = h.wrapping_mul(1_099_511_628_211);
    }
    h
}

fn hash_debug(s: &str) -> u64 {
    fnv1a64(s.as_bytes())
}

/// Calcula el golden de un template: `(kind, w, h, hash)`.
/// `Err` honesto con el motivo (el test lo convierte en fallo con el id).
fn golden_de(id: &str) -> Result<(&'static str, u32, u32, u64), String> {
    match id {
        // ── RGBA: linalgebra (6) ──
        "vectores-combinacion-lineal"
        | "matriz-transformacion"
        | "determinante-area"
        | "eigenvectores"
        | "cambio-de-base"
        | "producto-cruz" => {
            let frames = grafito_anim::tpl_linalg::render_linalg_rango(id, W, H, TOTAL, MID, 1)
                .map_err(|e| format!("render_linalg_rango: {e:?}"))?;
            let f = frames
                .into_iter()
                .next()
                .ok_or_else(|| "render_linalg_rango devolvió 0 frames".to_string())?;
            if f.width != W || f.height != H {
                return Err(format!("dims {}x{} != {W}x{H}", f.width, f.height));
            }
            Ok(("rgba", f.width, f.height, fnv1a64(&f.pixels)))
        }
        // ── RGBA: politopos 4D (5) ──
        "tesseract-xw" | "celda-24" | "hipercubo-corte" | "estereografica" | "simplex-nd" => {
            let frames = grafito_anim::tpl_4d::render_4d_rango(id, W, H, TOTAL, MID, 1)
                .map_err(|e| format!("render_4d_rango: {e:?}"))?;
            let f = frames
                .into_iter()
                .next()
                .ok_or_else(|| "render_4d_rango devolvió 0 frames".to_string())?;
            if f.width != W || f.height != H {
                return Err(format!("dims {}x{} != {W}x{H}", f.width, f.height));
            }
            Ok(("rgba", f.width, f.height, fnv1a64(&f.pixels)))
        }
        // ── RGBA: grafos (5) ──
        "moser-spindle-coloreo"
        | "bfs-animado"
        | "force-directed"
        | "unit-distance"
        | "camino-minimo" => {
            let frames = grafito_anim::tpl_graphs::render_graphs_rango(id, W, H, TOTAL, MID, 1)
                .map_err(|e| format!("render_graphs_rango: {e:?}"))?;
            let f = frames
                .into_iter()
                .next()
                .ok_or_else(|| "render_graphs_rango devolvió 0 frames".to_string())?;
            if f.width != W || f.height != H {
                return Err(format!("dims {}x{} != {W}x{H}", f.width, f.height));
            }
            Ok(("rgba", f.width, f.height, fnv1a64(&f.pixels)))
        }
        // ── RGBA: álgebra extra EoLA 7-9 (3) ──
        "matriz-inversa-nucleo" | "matriz-no-cuadrada" | "producto-punto-dualidad" => {
            let frames = grafito_anim::tpl_extra::render_extra_rango(id, W, H, TOTAL, MID, 1)
                .map_err(|e| format!("render_extra_rango: {e:?}"))?;
            let f = frames
                .into_iter()
                .next()
                .ok_or_else(|| "render_extra_rango devolvió 0 frames".to_string())?;
            if f.width != W || f.height != H {
                return Err(format!("dims {}x{} != {W}x{H}", f.width, f.height));
            }
            Ok(("rgba", f.width, f.height, fnv1a64(&f.pixels)))
        }
        // ── Geometría 3D (7): sin RGBA en este crate ──
        "sup-paraboloide-tangente"
        | "sup-toro-rotante"
        | "sup-campo-vectorial"
        | "sup-interseccion"
        | "sup-onda-3d"
        | "sup-silla-descenso"
        | "sup-laplace-3d" => {
            let params = grafito_anim::tpl_3d::Params3D::por_defecto(id);
            let m = grafito_anim::tpl_3d::muestra_frame(id, &params, MID, TOTAL, 2000)
                .map_err(|e| format!("muestra_frame: {e:?}"))?;
            Ok(("geom", 0, 0, hash_debug(&format!("{m:?}"))))
        }
        // ── Geometría caos (5) ──
        "chaos-mandelbrot-zoom"
        | "chaos-lorenz"
        | "chaos-bifurcacion-barrido"
        | "chaos-julia-morph"
        | "chaos-pendulo-doble" => {
            let params = grafito_anim::tpl_chaos::ChaosParams::por_defecto(id);
            let mut scratch = grafito_anim::anims::Scratch::nuevo();
            let escena = grafito_anim::tpl_chaos::escena_para(id, &params, 0.5, &mut scratch)
                .map_err(|e| format!("escena_para chaos: {e:?}"))?;
            Ok(("geom", 0, 0, hash_debug(&format!("{escena:?}"))))
        }
        // ── Geometría EDO (5) ──
        "edo-campo-direcciones"
        | "edo-convolucion"
        | "edo-laplace"
        | "edo-fourier-epiciclos"
        | "edo-calor-onda" => {
            let params = grafito_anim::tpl_edo::EdoParams::por_defecto(id);
            let mut scratch = grafito_anim::anims::Scratch::nuevo();
            let escena = grafito_anim::tpl_edo::escena_para(id, &params, 0.5, &mut scratch)
                .map_err(|e| format!("escena_para edo: {e:?}"))?;
            Ok(("geom", 0, 0, hash_debug(&format!("{escena:?}"))))
        }
        // ── Specs AM1 (6) ──
        "riemann-sums" | "epsilon-delta" | "chain-rule" | "taylor-remainder"
        | "improper-integral" | "ode-slope-field" => {
            let meta = grafito_anim::tpl_am1::describe(id);
            if meta.is_none() {
                return Err("describe devolvió None".to_string());
            }
            let claves = grafito_anim::tpl_am1::params_clave(id);
            if claves.is_empty() {
                return Err("params_clave vacío".to_string());
            }
            Ok(("spec", 0, 0, hash_debug(&format!("{meta:?}|{claves:?}"))))
        }
        // ── Specs AM2 (6) ──
        "partial-derivatives"
        | "gradient-descent"
        | "lagrange-multipliers"
        | "double-integral"
        | "green-stokes"
        | "jacobian" => {
            let ficha = grafito_anim::tpl_am2::ficha(id);
            if ficha.is_none() {
                return Err("ficha devolvió None".to_string());
            }
            let es = grafito_anim::tpl_am2::es_plantilla_am2(id);
            if !es {
                return Err("es_plantilla_am2 devolvió false".to_string());
            }
            Ok(("spec", 0, 0, hash_debug(&format!("{ficha:?}|{es}"))))
        }
        // ── Specs stats (8) ──
        "distribuciones"
        | "limite-central"
        | "teorema-bayes"
        | "regresion-lineal"
        | "pca-rotacion"
        | "perceptron-mlp"
        | "backprop-flujo"
        | "descenso-gradiente-3d" => {
            let titulo = grafito_anim::tpl_stats::titulo_para(id, W, H)
                .map_err(|e| format!("titulo_para: {e:?}"))?;
            let vivos = grafito_anim::tpl_stats::params_vivos(id)
                .map_err(|e| format!("params_vivos: {e:?}"))?;
            Ok(("spec", 0, 0, hash_debug(&format!("{titulo:?}|{vivos:?}"))))
        }
        // ── Canónicas (13): dispatcher de protocolo ──
        "derivative-slope"
        | "integral-area"
        | "taylor-series"
        | "conformal-map"
        | "pitagoras"
        | "euler"
        | "fourier"
        | "logistic-bifurcation"
        | "gradient-field"
        | "mobius-transform"
        | "universal"
        | "subspace"
        | "fractal" => {
            let req = grafito_anim::protocol::request_for_concept(id, id)
                .map_err(|e| format!("request_for_concept: {e:?}"))?;
            Ok(("proto", 0, 0, hash_debug(&format!("{req:?}"))))
        }
        otro => Err(format!("template fuera de los 65: {otro:?}")),
    }
}

/// Tabla pineada `(id, kind, w, h, fnv1a)` — medida en este box 2026-09-27.
const ESPERADOS: &[(&str, &str, u32, u32, u64)] = &[
    ("derivative-slope", "proto", 0, 0, 0xa1e9_73d8_a9c4_d43e),
    ("integral-area", "proto", 0, 0, 0x1a74_9b06_b6de_e198),
    ("taylor-series", "proto", 0, 0, 0x96b3_7c11_28ce_70a0),
    ("conformal-map", "proto", 0, 0, 0x188b_e1c6_13ea_d7d8),
    ("pitagoras", "proto", 0, 0, 0xaf5f_4927_7c87_7c88),
    ("euler", "proto", 0, 0, 0x715f_7b9b_d041_6c3c),
    ("fourier", "proto", 0, 0, 0xee07_280c_3f1b_e7c0),
    ("logistic-bifurcation", "proto", 0, 0, 0xba40_5355_71cf_405e),
    ("gradient-field", "proto", 0, 0, 0x07d8_5673_60f6_6356),
    ("mobius-transform", "proto", 0, 0, 0xb094_28c9_c450_adc8),
    ("universal", "proto", 0, 0, 0x52fc_7ab6_9930_8c34),
    ("subspace", "proto", 0, 0, 0x852d_6cec_a949_aa3c),
    ("fractal", "proto", 0, 0, 0xa170_8d02_0940_a92c),
    ("cambio-de-base", "rgba", 96, 72, 0xf84a_6113_aef8_cad4),
    ("determinante-area", "rgba", 96, 72, 0x2c3a_1838_82cc_9996),
    ("eigenvectores", "rgba", 96, 72, 0x0cf4_1b1f_6d53_15f3),
    (
        "matriz-transformacion",
        "rgba",
        96,
        72,
        0xd08d_e192_8464_6a89,
    ),
    ("producto-cruz", "rgba", 96, 72, 0x6ed7_8c7c_cf53_d531),
    (
        "vectores-combinacion-lineal",
        "rgba",
        96,
        72,
        0xceff_221d_e7ae_b407,
    ),
    ("chain-rule", "spec", 0, 0, 0xda23_2496_e286_f6e3),
    ("epsilon-delta", "spec", 0, 0, 0x8d47_c6c4_c6aa_4fb6),
    ("improper-integral", "spec", 0, 0, 0x240c_dcd6_a04d_9819),
    ("ode-slope-field", "spec", 0, 0, 0xd4f3_45fb_2d09_cebd),
    ("riemann-sums", "spec", 0, 0, 0xcb84_37b9_a7b1_ee8d),
    ("taylor-remainder", "spec", 0, 0, 0xb667_37a2_8f87_dd57),
    ("double-integral", "spec", 0, 0, 0x9784_5602_a411_f672),
    ("gradient-descent", "spec", 0, 0, 0x761d_51b0_8556_8f18),
    ("green-stokes", "spec", 0, 0, 0x2a87_0333_07b1_6bc0),
    ("jacobian", "spec", 0, 0, 0x947d_7309_5c61_8d44),
    ("lagrange-multipliers", "spec", 0, 0, 0x903f_835f_48b5_dc9f),
    ("partial-derivatives", "spec", 0, 0, 0x4cd8_a3f8_9425_0550),
    (
        "chaos-bifurcacion-barrido",
        "geom",
        0,
        0,
        0x5df3_5508_c386_e511,
    ),
    ("chaos-julia-morph", "geom", 0, 0, 0x1230_7e97_2264_b49c),
    ("chaos-lorenz", "geom", 0, 0, 0x2219_3d9f_af86_8985),
    ("chaos-mandelbrot-zoom", "geom", 0, 0, 0xdc52_acb7_f9a0_b9ae),
    ("chaos-pendulo-doble", "geom", 0, 0, 0xab27_2f99_361f_a58e),
    ("edo-calor-onda", "geom", 0, 0, 0x1876_8f9a_2d6c_1a01),
    ("edo-campo-direcciones", "geom", 0, 0, 0x7240_cef9_bf00_75f5),
    ("edo-convolucion", "geom", 0, 0, 0xddf7_11c1_6591_5ac1),
    ("edo-fourier-epiciclos", "geom", 0, 0, 0x3c03_bec4_846e_628f),
    ("edo-laplace", "geom", 0, 0, 0xa98a_2b5f_8d59_6448),
    ("backprop-flujo", "spec", 0, 0, 0xbcde_4041_57dc_d970),
    ("descenso-gradiente-3d", "spec", 0, 0, 0xf2a0_4b71_3655_3897),
    ("distribuciones", "spec", 0, 0, 0x460e_6ec9_a9ea_01a1),
    ("limite-central", "spec", 0, 0, 0xe1a8_378e_a4e2_89b7),
    ("pca-rotacion", "spec", 0, 0, 0x697f_925e_2fb9_6559),
    ("perceptron-mlp", "spec", 0, 0, 0xc199_4b2a_7413_ee0f),
    ("regresion-lineal", "spec", 0, 0, 0x65d9_8ce7_73be_6e2c),
    ("teorema-bayes", "spec", 0, 0, 0x2c06_205b_def3_e406),
    ("sup-campo-vectorial", "geom", 0, 0, 0x1387_15bc_5868_f793),
    ("sup-interseccion", "geom", 0, 0, 0x9879_ef2d_ae68_b610),
    ("sup-onda-3d", "geom", 0, 0, 0xf127_9bda_42cd_ac70),
    (
        "sup-paraboloide-tangente",
        "geom",
        0,
        0,
        0x49e0_bac6_c0cd_b1cc,
    ),
    ("sup-silla-descenso", "geom", 0, 0, 0xc59d_57ae_72a5_c5f8),
    ("sup-laplace-3d", "geom", 0, 0, 0x0efe_1605_39e7_9cc6),
    ("sup-toro-rotante", "geom", 0, 0, 0x31e4_8d5a_e758_18ff),
    ("celda-24", "rgba", 96, 72, 0x5cc0_9976_eb1b_c1d9),
    ("estereografica", "rgba", 96, 72, 0x105c_f261_b718_ed77),
    ("hipercubo-corte", "rgba", 96, 72, 0xbf17_39e5_831d_639e),
    ("simplex-nd", "rgba", 96, 72, 0x5ebc_45b7_678d_0b79),
    ("tesseract-xw", "rgba", 96, 72, 0x7147_50ce_643e_9248),
    ("bfs-animado", "rgba", 96, 72, 0xc886_1d2f_ae4c_36f0),
    ("camino-minimo", "rgba", 96, 72, 0x3166_edb9_d615_9842),
    ("force-directed", "rgba", 96, 72, 0x767a_44c4_2f4c_9f8f),
    (
        "moser-spindle-coloreo",
        "rgba",
        96,
        72,
        0x34f3_364a_f817_b712,
    ),
    ("unit-distance", "rgba", 96, 72, 0x26e4_ed9e_589f_c2ca),
    (
        "matriz-inversa-nucleo",
        "rgba",
        96,
        72,
        0x7614_01fb_fef6_74f1,
    ),
    ("matriz-no-cuadrada", "rgba", 96, 72, 0xb899_a9ba_a242_c19d),
    (
        "producto-punto-dualidad",
        "rgba",
        96,
        72,
        0x5992_a004_624f_d6c6,
    ),
];

#[test]
fn goldens_cubren_69_sin_duplicados() {
    assert_eq!(ESPERADOS.len(), 69, "la tabla pinea los 69 templates");
    let mut ids: Vec<&str> = ESPERADOS.iter().map(|e| e.0).collect();
    ids.sort_unstable();
    ids.dedup();
    assert_eq!(ids.len(), 69, "sin duplicados en la tabla");
    let canon = grafito_anim::protocol::CANONICAL_TEMPLATES;
    assert_eq!(canon.len(), 69, "el protocolo expone 69 canónicas");
    for (id, _, _, _, _) in ESPERADOS {
        assert!(
            canon.contains(id),
            "{id} de la tabla debe estar en CANONICAL_TEMPLATES"
        );
    }
    for id in canon {
        assert!(
            ESPERADOS.iter().any(|e| e.0 == *id),
            "{id} canónica debe estar en la tabla"
        );
    }
}

#[test]
fn goldens_69_templates() {
    let mut fallos: Vec<String> = Vec::new();
    let mut ok = 0_usize;
    for (id, kind_esp, w_esp, h_esp, hash_esp) in ESPERADOS {
        let res = std::panic::catch_unwind(AssertUnwindSafe(|| golden_de(id)));
        match res {
            Err(_) => fallos.push(format!("{id}: PANIC al renderizar el frame chico")),
            Ok(Err(e)) => fallos.push(format!("{id}: ERROR al renderizar: {e}")),
            Ok(Ok((kind, w, h, hash))) => {
                println!("GOLDEN {id} {kind} {w}x{h} {hash:016x}");
                if kind != *kind_esp || w != *w_esp || h != *h_esp || hash != *hash_esp {
                    fallos.push(format!(
                        "{id}: esperado {kind_esp}/{w_esp}x{h_esp}#{hash_esp:016x} \
                         pero dio {kind}/{w}x{h}#{hash:016x}"
                    ));
                } else {
                    ok += 1;
                }
            }
        }
    }
    assert!(
        fallos.is_empty(),
        "goldens rotos ({ok} ok, {} fallidos):\n{}",
        fallos.len(),
        fallos.join("\n")
    );
    assert_eq!(ok, 69, "cubiertos los 69");
}
