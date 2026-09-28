//! Superficies 3D CPU/offline (nivel 3Blue1Brown, espejo de `ThreeDScene`).
//!
//! - [`surfaces3d::Surface3D`]: mallas paramétricas `(u, v) -> [x, y, z]` con normales
//!   por diferencias centrales, constructores (`esfera`, `plano`, `toro`) y
//!   salida sólida ([`surfaces3d::Surface3D::solido`], sombreado Lambert a doble cara
//!   con luz direccional) o wireframe ([`surfaces3d::Surface3D::alambre`]).
//! - [`surfaces3d::Curva3D`]: `traza_en` 3D (espejo de Manim `TracedPath`) con
//!   [`surfaces3d::Curva3D::proyecta`] que corta el trazo donde cae detrás de la cámara.
//! - [`surfaces3d::Ejes3D`]: ejes con ticks y [`surfaces3d::Ejes3D::etiquetas`] posicionadas por
//!   [`Camera::project_3d`].
//! - Orden por profundidad = painter's algorithm `O(n log n)` (`sort` por
//!   [`Camera::depth_of`], lejos→cerca): espejo CONCEPTUAL del
//!   `depth_3d`/`composite_3d` de `grafito-render`, SIN depender de wgpu
//!   (este crate es CPU/offline: se proyecta a 2D y se emiten polígonos
//!   ordenados).
//!
//! Presupuestos (acotados, sin `O(n²)` patológico): res 2..=128 por lado
//! (default 32; ≤16641 vértices, ≤32768 tris; el sort domina con
//! `O(n log n)`), curvas 2..=512 puntos, ticks 0..=64 por eje. Todo lo que
//! excede es `Err` honesto. Las caras detrás de la cámara se descartan (es
//! oclusión, no error); los puntos de muestreo no finitos son `Err` (acá no
//! se inventa geometría).

use super::{Camera, SceneError, SceneResult};
use serde::{Deserialize, Serialize};
use std::cmp::Ordering;

/// Res mínima por lado de una malla (una celda ya tesela).
pub const SURF3D_MIN_RES: usize = 2;
/// Res máxima por lado (128² celdas = 32768 tris; el sort sigue `O(n log n)`).
pub const SURF3D_MAX_RES: usize = 128;
/// Res default por lado (32² = 2048 tris: lindo sin pesar).
pub const SURF3D_DEFAULT_RES: usize = 32;
/// Vértices máximos (`129 × 129`; paridad con res máxima).
pub const SURF3D_MAX_VERTICES: usize = 16_641;
/// Triángulos máximos (`2 × 128 × 128`).
pub const SURF3D_MAX_TRIS: usize = 32_768;
/// Coordenada máxima (`|x| ≤ 1e6`, paridad con figuras).
pub const SURF3D_MAX_COORD: f64 = 1_000_000.0;
/// Puntos mínimos de una curva 3D.
pub const CURVA3D_MIN_PUNTOS: usize = 2;
/// Puntos máximos de una curva 3D (paridad con `SCENE_MORPH_MAX_SAMPLES`).
pub const CURVA3D_MAX_PUNTOS: usize = 512;
/// Semilongitud mínima de un eje (paridad con figuras: 1e-9).
pub const EJES3D_MIN_SEMI: f64 = 1e-9;
/// Semilongitud máxima de un eje (paridad con figuras: 1e6).
pub const EJES3D_MAX_SEMI: f64 = 1_000_000.0;
/// Ticks máximos por eje (paridad con `MAX_FIELD_DIVISIONS`).
pub const EJES3D_MAX_TICKS: usize = 64;

// ── Aritmética [f64; 3] (sin glam: este crate no depende de GPU/math) ───────

/// ¿Los 3 componentes finitos?
fn fin3(p: &[f64; 3]) -> bool {
    p.iter().all(|v| v.is_finite())
}

/// Resta componente a componente. Pura.
fn vsub(a: [f64; 3], b: [f64; 3]) -> [f64; 3] {
    [a[0] - b[0], a[1] - b[1], a[2] - b[2]]
}

/// Producto cruz. Puro.
fn vcross(a: [f64; 3], b: [f64; 3]) -> [f64; 3] {
    [
        a[1] * b[2] - a[2] * b[1],
        a[2] * b[0] - a[0] * b[2],
        a[0] * b[1] - a[1] * b[0],
    ]
}

/// Producto punto. Puro.
fn vdot(a: [f64; 3], b: [f64; 3]) -> f64 {
    a[0] * b[0] + a[1] * b[1] + a[2] * b[2]
}

/// Norma euclídea. Pura.
fn vlen(a: [f64; 3]) -> f64 {
    vdot(a, a).sqrt()
}

/// Normalizado, o `fallback` si degenera/no finito. Puro, sin pánicos.
fn vnorm_o(a: [f64; 3], fallback: [f64; 3]) -> [f64; 3] {
    let l = vlen(a);
    if !l.is_finite() || l < 1e-12 {
        return fallback;
    }
    let inv = 1.0 / l;
    let n = [a[0] * inv, a[1] * inv, a[2] * inv];
    if fin3(&n) {
        n
    } else {
        fallback
    }
}

// ── Malla paramétrica ──────────────────────────────────────────────────────

/// Malla paramétrica muestreada en grilla `(nu+1) × (nv+1)`.
///
/// `puntos`/`normales` van en fila-mayor (`idx = j·(nu+1) + i`, `i` sobre
/// `u`, `j` sobre `v`); las normales salen por diferencias centrales de la
/// parametrización (unilaterales en bordes; `[0,0,1]` si el punto es
/// singular —polo de esfera— en vez de inventar).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Surface3D {
    pub nu: usize,
    pub nv: usize,
    pub u_range: [f64; 2],
    pub v_range: [f64; 2],
    pub puntos: Vec<[f64; 3]>,
    pub normales: Vec<[f64; 3]>,
}

impl Surface3D {
    /// Muestrea `f(u, v)` en la grilla (`nu`/`nv` 2..=128, rangos finitos
    /// con `min < max`). `Err` honesto si un punto no es finito o excede
    /// ±1e6 (con índices `i, j` para ubicarlo).
    pub fn try_new(
        nu: usize,
        nv: usize,
        u_range: [f64; 2],
        v_range: [f64; 2],
        mut f: impl FnMut(f64, f64) -> [f64; 3],
    ) -> SceneResult<Self> {
        for (nombre, n) in [("nu", nu), ("nv", nv)] {
            if !(SURF3D_MIN_RES..=SURF3D_MAX_RES).contains(&n) {
                return Err(SceneError::MobjectInvalido {
                    donde: "Surface3D",
                    detalle: format!("{nombre} {n} fuera de {SURF3D_MIN_RES}..={SURF3D_MAX_RES}"),
                });
            }
        }
        for (nombre, r) in [("u_range", &u_range), ("v_range", &v_range)] {
            if !r.iter().all(|v| v.is_finite()) {
                return Err(SceneError::MobjectInvalido {
                    donde: "Surface3D",
                    detalle: format!("{nombre} no finito"),
                });
            }
            if !matches!(r[0].partial_cmp(&r[1]), Some(Ordering::Less)) {
                return Err(SceneError::MobjectInvalido {
                    donde: "Surface3D",
                    detalle: format!("{nombre} necesita min < max"),
                });
            }
        }
        let cols = nu.checked_add(1).ok_or(SceneError::PresupuestoExcedido {
            detalle: "grilla nu+1 desborda usize".to_string(),
        })?;
        let filas = nv.checked_add(1).ok_or(SceneError::PresupuestoExcedido {
            detalle: "grilla nv+1 desborda usize".to_string(),
        })?;
        let n_pts = cols
            .checked_mul(filas)
            .ok_or(SceneError::PresupuestoExcedido {
                detalle: "la malla desborda usize: bajá la res".to_string(),
            })?;
        if n_pts > SURF3D_MAX_VERTICES {
            return Err(SceneError::PresupuestoExcedido {
                detalle: format!("{n_pts} vértices (máximo {SURF3D_MAX_VERTICES}): bajá la res"),
            });
        }
        let du = (u_range[1] - u_range[0]) / (nu as f64);
        let dv = (v_range[1] - v_range[0]) / (nv as f64);
        if !du.is_finite() || !dv.is_finite() || du <= 0.0 || dv <= 0.0 {
            return Err(SceneError::MobjectInvalido {
                donde: "Surface3D",
                detalle: "paso de grilla degenerado: revisá los rangos".to_string(),
            });
        }
        let mut puntos = Vec::with_capacity(n_pts);
        for j in 0..=nv {
            for i in 0..=nu {
                let u = u_range[0] + (i as f64) * du;
                let v = v_range[0] + (j as f64) * dv;
                let p = f(u, v);
                if !fin3(&p) {
                    return Err(SceneError::MobjectInvalido {
                        donde: "Surface3D",
                        detalle: format!("f({u}, {v}) no finita en la celda i={i} j={j}"),
                    });
                }
                if p.iter().any(|v| v.abs() > SURF3D_MAX_COORD) {
                    return Err(SceneError::MobjectInvalido {
                        donde: "Surface3D",
                        detalle: format!("f({u}, {v}) excede ±{SURF3D_MAX_COORD} en i={i} j={j}"),
                    });
                }
                puntos.push(p);
            }
        }
        let mut normales = Vec::with_capacity(n_pts);
        for j in 0..=nv {
            for i in 0..=nu {
                let i0 = i.saturating_sub(1);
                let i1 = (i + 1).min(nu);
                let j0 = j.saturating_sub(1);
                let j1 = (j + 1).min(nv);
                let pa = puntos
                    .get(j.saturating_mul(cols).saturating_add(i1))
                    .copied()
                    .unwrap_or([0.0, 0.0, 0.0]);
                let pb = puntos
                    .get(j.saturating_mul(cols).saturating_add(i0))
                    .copied()
                    .unwrap_or([0.0, 0.0, 0.0]);
                let pc = puntos
                    .get(j1.saturating_mul(cols).saturating_add(i))
                    .copied()
                    .unwrap_or([0.0, 0.0, 0.0]);
                let pd = puntos
                    .get(j0.saturating_mul(cols).saturating_add(i))
                    .copied()
                    .unwrap_or([0.0, 0.0, 0.0]);
                normales.push(vnorm_o(vcross(vsub(pa, pb), vsub(pc, pd)), [0.0, 0.0, 1.0]));
            }
        }
        Ok(Self {
            nu,
            nv,
            u_range,
            v_range,
            puntos,
            normales,
        })
    }

    /// Esfera de `radio` (`u ∈ [0, 2π)`, `v ∈ [0, π]`; `radio` 1e-9..=1e6).
    /// Los polos son singulares por diseño (normales `[0,0,±1]` honestas).
    pub fn esfera(radio: f64, nu: usize, nv: usize) -> SceneResult<Self> {
        if !radio.is_finite() || !(1e-9..=1e6).contains(&radio) {
            return Err(SceneError::MobjectInvalido {
                donde: "Surface3D::esfera",
                detalle: format!("radio {radio} fuera de 1e-9..=1e6"),
            });
        }
        let tau = std::f64::consts::TAU;
        let pi = std::f64::consts::PI;
        Self::try_new(nu, nv, [0.0, tau], [0.0, pi], |u, v| {
            let (su, cu) = u.sin_cos();
            let (sv, cv) = v.sin_cos();
            [radio * sv * cu, radio * sv * su, radio * cv]
        })
    }

    /// Plano `ancho × alto` centrado en el origen (`z = 0`, normal `+z`).
    /// Dims 1e-9..=1e6.
    pub fn plano(ancho: f64, alto: f64, nu: usize, nv: usize) -> SceneResult<Self> {
        for (nombre, d) in [("ancho", ancho), ("alto", alto)] {
            if !d.is_finite() || !(1e-9..=1e6).contains(&d) {
                return Err(SceneError::MobjectInvalido {
                    donde: "Surface3D::plano",
                    detalle: format!("{nombre} {d} fuera de 1e-9..=1e6"),
                });
            }
        }
        Self::try_new(
            nu,
            nv,
            [-ancho / 2.0, ancho / 2.0],
            [-alto / 2.0, alto / 2.0],
            |u, v| [u, v, 0.0],
        )
    }

    /// Toro (`mayor > menor > 0`, ambos ≤ 1e6; `mayor > menor` lo deja sin
    /// autointersección, como el `Torus` Manim).
    pub fn toro(mayor: f64, menor: f64, nu: usize, nv: usize) -> SceneResult<Self> {
        for (nombre, d) in [("mayor", mayor), ("menor", menor)] {
            if !d.is_finite() || !(1e-9..=1e6).contains(&d) {
                return Err(SceneError::MobjectInvalido {
                    donde: "Surface3D::toro",
                    detalle: format!("{nombre} {d} fuera de 1e-9..=1e6"),
                });
            }
        }
        if !matches!(menor.partial_cmp(&mayor), Some(Ordering::Less)) {
            return Err(SceneError::MobjectInvalido {
                donde: "Surface3D::toro",
                detalle: "necesito mayor > menor (si no se autointersecta)".to_string(),
            });
        }
        let tau = std::f64::consts::TAU;
        Self::try_new(nu, nv, [0.0, tau], [0.0, tau], |u, v| {
            let (su, cu) = u.sin_cos();
            let (sv, cv) = v.sin_cos();
            let r = mayor + menor * cv;
            [r * cu, r * su, menor * sv]
        })
    }

    /// Índices de triángulos (`2·nu·nv`; orden fijo, a doble cara el winding
    /// no importa porque el Lambert usa `|n·l|`). Puro, sin pánicos.
    pub fn triangulos(&self) -> Vec<[usize; 3]> {
        let cols = self.nu.saturating_add(1);
        let mut out = Vec::with_capacity(self.nu.saturating_mul(self.nv).saturating_mul(2));
        for j in 0..self.nv {
            for i in 0..self.nu {
                let a = j.saturating_mul(cols).saturating_add(i);
                let b = a.saturating_add(1);
                let c = a.saturating_add(cols);
                let d = c.saturating_add(1);
                out.push([a, d, b]);
                out.push([a, c, d]);
            }
        }
        out
    }

    /// Cantidad de triángulos (≤ `SURF3D_MAX_TRIS` por construcción).
    pub fn n_tris(&self) -> usize {
        self.nu.saturating_mul(self.nv).saturating_mul(2)
    }

    /// Malla sólida: tris proyectados con Lambert a doble cara, ordenados
    /// lejos→cerca (painter `O(n log n)`).
    ///
    /// `luz` es la dirección HACIA la luz (finita, no nula); el sombreado es
    /// `|n·l|` 0..1 —a doble cara, sin culling, como las `Surface` 3b1b que
    /// se miran por ambos lados—. Las caras con algún vértice detrás de la
    /// cámara se descartan (oclusión, no error); las degeneradas (área ~0,
    /// polos) también. `Err` honesto si la luz degenera o los tris exceden
    /// el presupuesto.
    pub fn solido(&self, cam: Camera, luz: [f64; 3]) -> SceneResult<Vec<CaraPintada>> {
        let l = Self::luz_normalizada(luz)?;
        if self.n_tris() > SURF3D_MAX_TRIS {
            return Err(SceneError::PresupuestoExcedido {
                detalle: format!(
                    "{} tris (máximo {SURF3D_MAX_TRIS}): bajá la res",
                    self.n_tris()
                ),
            });
        }
        // Proyecta cada vértice UNA vez (O(n)); las caras solo indexan.
        let mut prov: Vec<Option<([f64; 2], f64)>> = Vec::with_capacity(self.puntos.len());
        for p in &self.puntos {
            match (cam.project_3d(*p), cam.depth_of(*p)) {
                (Some(q), Some(d)) => prov.push(Some((q, d))),
                _ => prov.push(None),
            }
        }
        let mut caras = Vec::with_capacity(self.n_tris());
        for t in self.triangulos() {
            let (a, b, c) = match (prov.get(t[0]), prov.get(t[1]), prov.get(t[2])) {
                (Some(Some(a)), Some(Some(b)), Some(Some(c))) => (*a, *b, *c),
                _ => continue,
            };
            let (p0, p1, p2) = match (
                self.puntos.get(t[0]),
                self.puntos.get(t[1]),
                self.puntos.get(t[2]),
            ) {
                (Some(p0), Some(p1), Some(p2)) => (*p0, *p1, *p2),
                _ => continue,
            };
            let n = vcross(vsub(p1, p0), vsub(p2, p0));
            let area2 = vlen(n);
            if !area2.is_finite() || area2 < 1e-18 {
                continue;
            }
            let n = [n[0] / area2, n[1] / area2, n[2] / area2];
            let s = vdot(n, l).abs().clamp(0.0, 1.0);
            caras.push(CaraPintada {
                poly: [a.0, b.0, c.0],
                depth: (a.1 + b.1 + c.1) / 3.0,
                shade: s,
                normal: n,
            });
        }
        // Painter: lejos-primero. `sort` estable O(n log n), sin O(n²).
        caras.sort_by(|a, b| b.depth.partial_cmp(&a.depth).unwrap_or(Ordering::Equal));
        Ok(caras)
    }

    /// Wireframe: aristas de la grilla proyectadas, ordenadas lejos→cerca
    /// (painter `O(n log n)`). Aristas con algún extremo detrás de la cámara
    /// se descartan. `Err` honesto solo por presupuesto (la luz no aplica).
    pub fn alambre(&self, cam: Camera) -> SceneResult<Vec<TrazoProfundo>> {
        let n_seg = self
            .nu
            .checked_add(1)
            .and_then(|c| c.checked_mul(self.nv))
            .and_then(|h| {
                self.nv
                    .checked_add(1)
                    .and_then(|f| f.checked_mul(self.nu))
                    .and_then(|v| h.checked_add(v))
            })
            .ok_or(SceneError::PresupuestoExcedido {
                detalle: "el wireframe desborda usize: bajá la res".to_string(),
            })?;
        if n_seg > SURF3D_MAX_TRIS.saturating_mul(2) {
            return Err(SceneError::PresupuestoExcedido {
                detalle: format!("{n_seg} trazos de wireframe: bajá la res"),
            });
        }
        let cols = self.nu.saturating_add(1);
        let mut prov: Vec<Option<([f64; 2], f64)>> = Vec::with_capacity(self.puntos.len());
        for p in &self.puntos {
            match (cam.project_3d(*p), cam.depth_of(*p)) {
                (Some(q), Some(d)) => prov.push(Some((q, d))),
                _ => prov.push(None),
            }
        }
        let mut trazos = Vec::with_capacity(n_seg);
        let mut arista = |a: usize, b: usize| {
            if let (Some(Some(p)), Some(Some(q))) = (prov.get(a), prov.get(b)) {
                trazos.push(TrazoProfundo {
                    a: p.0,
                    b: q.0,
                    depth: (p.1 + q.1) / 2.0,
                });
            }
        };
        for j in 0..=self.nv {
            for i in 0..self.nu {
                arista(
                    j.saturating_mul(cols).saturating_add(i),
                    j.saturating_mul(cols).saturating_add(i + 1),
                );
            }
        }
        for j in 0..self.nv {
            for i in 0..=self.nu {
                arista(
                    j.saturating_mul(cols).saturating_add(i),
                    (j + 1).saturating_mul(cols).saturating_add(i),
                );
            }
        }
        trazos.sort_by(|a, b| b.depth.partial_cmp(&a.depth).unwrap_or(Ordering::Equal));
        Ok(trazos)
    }

    /// Dirección de luz validada y normalizada. `Err` honesto si degenera.
    fn luz_normalizada(luz: [f64; 3]) -> SceneResult<[f64; 3]> {
        if !fin3(&luz) {
            return Err(SceneError::MobjectInvalido {
                donde: "luz",
                detalle: "dirección no finita".to_string(),
            });
        }
        let l = vlen(luz);
        if !l.is_finite() || l < 1e-9 {
            return Err(SceneError::MobjectInvalido {
                donde: "luz",
                detalle: "dirección nula: apuntá la luz a algún lado".to_string(),
            });
        }
        Ok([luz[0] / l, luz[1] / l, luz[2] / l])
    }
}

/// Triángulo sólido proyectado y sombreado (sale de [`Surface3D::solido`],
/// ya ordenado lejos→cerca: pintar en orden tapa bien sin z-buffer).
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct CaraPintada {
    /// Vértices 2D de mundo.
    pub poly: [[f64; 2]; 3],
    /// Profundidad media ([`Camera::depth_of`]; mayor = más lejos).
    pub depth: f64,
    /// Lambert a doble cara `|n·l|` 0..1.
    pub shade: f64,
    /// Normal geométrica 3D unitaria.
    pub normal: [f64; 3],
}

/// Trazo de wireframe proyectado con profundidad (sale de
/// [`Surface3D::alambre`], ya ordenado lejos→cerca).
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct TrazoProfundo {
    pub a: [f64; 2],
    pub b: [f64; 2],
    /// Profundidad media (mayor = más lejos).
    pub depth: f64,
}

// ── Curva 3D ───────────────────────────────────────────────────────────────

/// Curva 3D muestreada (espejo de Manim `TracedPath`, pero offline: la
/// `traza_en` muestrea `f(t)` en `n` puntos y `proyecta` la baja a 2D).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Curva3D {
    pub puntos: Vec<[f64; 3]>,
    pub t0: f64,
    pub t1: f64,
}

impl Curva3D {
    /// Muestrea `f(t)` en `n` puntos equiespaciados sobre `[t0, t1]`
    /// (`n` 2..=512, `t0 < t1` finitos, puntos finitos y acotados ±1e6).
    /// Todo `Err` honesto.
    pub fn traza_en(
        n: usize,
        t0: f64,
        t1: f64,
        mut f: impl FnMut(f64) -> [f64; 3],
    ) -> SceneResult<Self> {
        if !(CURVA3D_MIN_PUNTOS..=CURVA3D_MAX_PUNTOS).contains(&n) {
            return Err(SceneError::MobjectInvalido {
                donde: "Curva3D",
                detalle: format!("{n} puntos (válido {CURVA3D_MIN_PUNTOS}..={CURVA3D_MAX_PUNTOS})"),
            });
        }
        if !t0.is_finite() || !t1.is_finite() {
            return Err(SceneError::MobjectInvalido {
                donde: "Curva3D",
                detalle: "t0/t1 no finitos".to_string(),
            });
        }
        if !matches!(t0.partial_cmp(&t1), Some(Ordering::Less)) {
            return Err(SceneError::MobjectInvalido {
                donde: "Curva3D",
                detalle: "necesito t0 < t1".to_string(),
            });
        }
        let mut puntos = Vec::with_capacity(n);
        for k in 0..n {
            let t = if n <= 1 {
                t1
            } else {
                t0 + (t1 - t0) * (k as f64) / ((n - 1) as f64)
            };
            if !t.is_finite() {
                return Err(SceneError::MobjectInvalido {
                    donde: "Curva3D",
                    detalle: format!("t no finito en la muestra {k}"),
                });
            }
            let p = f(t);
            if !fin3(&p) {
                return Err(SceneError::MobjectInvalido {
                    donde: "Curva3D",
                    detalle: format!("f({t}) no finita en la muestra {k}"),
                });
            }
            if p.iter().any(|v| v.abs() > SURF3D_MAX_COORD) {
                return Err(SceneError::MobjectInvalido {
                    donde: "Curva3D",
                    detalle: format!("f({t}) excede ±{SURF3D_MAX_COORD} en la muestra {k}"),
                });
            }
            puntos.push(p);
        }
        Ok(Self { puntos, t0, t1 })
    }

    /// Proyecta a 2D de mundo, cortando el trazo donde cae detrás de la
    /// cámara (`None` de [`Camera::project_3d`]): devuelve los trazos
    /// visibles (puede ser vacío si todo quedó atrás; eso es oclusión, no
    /// error). Puro, sin pánicos.
    pub fn proyecta(&self, cam: Camera) -> Vec<Vec<[f64; 2]>> {
        let mut trazos: Vec<Vec<[f64; 2]>> = Vec::new();
        let mut actual: Vec<[f64; 2]> = Vec::new();
        for p in &self.puntos {
            match cam.project_3d(*p) {
                Some(q) => actual.push(q),
                None => {
                    if actual.len() >= 2 {
                        trazos.push(std::mem::take(&mut actual));
                    } else {
                        actual.clear();
                    }
                }
            }
        }
        if actual.len() >= 2 {
            trazos.push(actual);
        }
        trazos
    }
}

// ── Ejes 3D ────────────────────────────────────────────────────────────────

/// Ejes 3D centrados en `centro` con semilongitud `semi` por eje
/// (espejo de Manim `ThreeDAxes`, offline: el renderer dibuja).
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct Ejes3D {
    pub centro: [f64; 3],
    pub semi: [f64; 3],
    /// Ticks por eje (0 = sin ticks, máximo 64).
    pub ticks: [usize; 3],
}

/// Tick con etiqueta posicionada (sale de [`Ejes3D::etiquetas`]).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct TickEtiqueta {
    /// Eje 0/1/2 (x/y/z).
    pub eje: usize,
    /// Valor sobre el eje (coordenada mundo).
    pub valor: f64,
    /// Posición 3D del tick.
    pub pos3d: [f64; 3],
    /// Posición 2D (`None` si el tick cayó detrás de la cámara).
    pub pos: Option<[f64; 2]>,
    /// Texto (`{valor:.2}`).
    pub texto: String,
}

impl Ejes3D {
    /// Constructor validado (`centro` finito, `semi` 1e-9..=1e6 por eje,
    /// `ticks` 0..=64 por eje). Todo `Err` honesto.
    pub fn try_new(centro: [f64; 3], semi: [f64; 3], ticks: [usize; 3]) -> SceneResult<Self> {
        if !fin3(&centro) {
            return Err(SceneError::MobjectInvalido {
                donde: "Ejes3D",
                detalle: "centro no finito".to_string(),
            });
        }
        for (k, s) in semi.iter().enumerate() {
            if !s.is_finite() || !(EJES3D_MIN_SEMI..=EJES3D_MAX_SEMI).contains(s) {
                return Err(SceneError::MobjectInvalido {
                    donde: "Ejes3D",
                    detalle: format!(
                        "semi[{k}] {s} fuera de {EJES3D_MIN_SEMI}..={EJES3D_MAX_SEMI}"
                    ),
                });
            }
        }
        for (k, n) in ticks.iter().enumerate() {
            if *n > EJES3D_MAX_TICKS {
                return Err(SceneError::MobjectInvalido {
                    donde: "Ejes3D",
                    detalle: format!("ticks[{k}] {n} (máximo {EJES3D_MAX_TICKS})"),
                });
            }
        }
        Ok(Self {
            centro,
            semi,
            ticks,
        })
    }

    /// Los 3 segmentos (`x`, `y`, `z`) de `centro - semi` a `centro + semi`.
    /// Puro, sin pánicos.
    pub fn ejes_segmentos(self) -> [[[f64; 3]; 2]; 3] {
        let mut out = [[[0.0; 3]; 2]; 3];
        for eje in 0..3 {
            let mut a = self.centro;
            let mut b = self.centro;
            a[eje] -= self.semi[eje];
            b[eje] += self.semi[eje];
            out[eje] = [a, b];
        }
        out
    }

    /// Posiciones 3D de los ticks del `eje` (0/1/2; `Err` honesto si el eje
    /// no existe). `n` ticks equiespaciados con extremos incluidos (`n == 1`
    /// → solo el centro); `n == 0` → vacío. Puro, sin pánicos.
    pub fn ticks_en(self, eje: usize) -> SceneResult<Vec<[f64; 3]>> {
        if eje > 2 {
            return Err(SceneError::MobjectInvalido {
                donde: "Ejes3D",
                detalle: format!("eje {eje} (válido 0/1/2 = x/y/z)"),
            });
        }
        let n = self.ticks[eje];
        let mut out = Vec::with_capacity(n);
        for k in 0..n {
            let s = if n <= 1 {
                0.0
            } else {
                -self.semi[eje] + 2.0 * self.semi[eje] * (k as f64) / ((n - 1) as f64)
            };
            if !s.is_finite() {
                continue;
            }
            let mut p = self.centro;
            p[eje] += s;
            if fin3(&p) {
                out.push(p);
            }
        }
        Ok(out)
    }

    /// Ticks de los 3 ejes con etiquetas posicionadas por `cam`
    /// (`pos = None` si el tick cayó detrás). Puro, sin pánicos.
    pub fn etiquetas(self, cam: Camera) -> Vec<TickEtiqueta> {
        let total = self.ticks.iter().sum::<usize>();
        let mut out = Vec::with_capacity(total);
        for eje in 0..3 {
            let ticks = match self.ticks_en(eje) {
                Ok(t) => t,
                Err(_) => continue,
            };
            for p in ticks {
                let valor = p[eje];
                out.push(TickEtiqueta {
                    eje,
                    valor,
                    pos3d: p,
                    pos: cam.project_3d(p),
                    texto: format!("{valor:.2}"),
                });
            }
        }
        out
    }
}

#[cfg(test)]
mod surfaces3d_tests {
    use super::super::{Ortho, RateFunc};
    use super::*;

    fn cam_orto() -> Camera {
        Camera::Ortho(Ortho::default_16_9())
    }

    fn cam_persp() -> Camera {
        Camera::perspective(60.0, [0.0, 0.0, 5.0], [0.0, 0.0, 0.0]).unwrap()
    }

    #[test]
    fn esfera_normales_unitarias_y_radio_exactos() {
        let m = Surface3D::esfera(2.0, 16, 16).unwrap();
        assert_eq!(m.puntos.len(), 17 * 17);
        assert_eq!(m.normales.len(), m.puntos.len());
        for (p, n) in m.puntos.iter().zip(m.normales.iter()) {
            let r = vlen(*p);
            // Polos (v = 0/π): el radio cierra igual aunque la normal caiga al fallback.
            assert!((r - 2.0).abs() < 1e-9, "radio {r}");
            let l = vlen(*n);
            assert!((l - 1.0).abs() < 1e-9, "normal {n:?}");
        }
        assert_eq!(m.n_tris(), 2 * 16 * 16);
        assert_eq!(m.triangulos().len(), m.n_tris());
    }

    #[test]
    fn lambert_doble_cara_frente_dorso_y_cero_de_canto() {
        let plano = Surface3D::plano(4.0, 2.0, 4, 4).unwrap();
        // De frente: sombra total.
        let de_frente = plano.solido(cam_orto(), [0.0, 0.0, 1.0]).unwrap();
        assert!(!de_frente.is_empty());
        for c in &de_frente {
            assert!((c.shade - 1.0).abs() < 1e-9, "shade {}", c.shade);
        }
        // Del dorso: doble cara, también ilumina (|n·l|).
        let de_dorso = plano.solido(cam_orto(), [0.0, 0.0, -1.0]).unwrap();
        assert_eq!(de_dorso.len(), de_frente.len());
        for c in &de_dorso {
            assert!((c.shade - 1.0).abs() < 1e-9, "shade {}", c.shade);
        }
        // De canto: cero.
        let de_canto = plano.solido(cam_orto(), [1.0, 0.0, 0.0]).unwrap();
        for c in &de_canto {
            assert!(c.shade.abs() < 1e-9, "shade {}", c.shade);
        }
    }

    #[test]
    fn painter_ordena_lejos_primero_en_escena_de_prueba() {
        let m = Surface3D::esfera(1.0, 12, 12).unwrap();
        let caras = m.solido(cam_persp(), [0.5, 1.0, 0.3]).unwrap();
        assert!(!caras.is_empty());
        // Lejos→cerca: profundidades no-crecientes, con rango real.
        for par in caras.windows(2) {
            assert!(
                par[0].depth + 1e-12 >= par[1].depth,
                "painter roto: {} < {}",
                par[0].depth,
                par[1].depth
            );
        }
        let primera = caras.first().unwrap().depth;
        let ultima = caras.last().unwrap().depth;
        assert!(primera > ultima + 0.5, "sin rango de profundidad");
        // Todo proyectado finito y sombreado 0..1.
        for c in &caras {
            assert!(c.depth.is_finite());
            assert!((0.0..=1.0).contains(&c.shade));
            for v in &c.poly {
                assert!(v.iter().all(|x| x.is_finite()));
            }
        }
    }

    #[test]
    fn presupuestos_malla_curva_ejes_y_luz() {
        // Res fuera de 2..=128.
        assert!(Surface3D::esfera(1.0, 1, 8).is_err());
        assert!(Surface3D::esfera(1.0, 8, 129).is_err());
        assert_eq!(SURF3D_MAX_VERTICES, 129 * 129);
        assert_eq!(SURF3D_MAX_TRIS, 2 * 128 * 128);
        // Rangos y geometría.
        assert!(Surface3D::try_new(4, 4, [1.0, 0.0], [0.0, 1.0], |u, v| [u, v, 0.0]).is_err());
        assert!(
            Surface3D::try_new(4, 4, [0.0, 1.0], [0.0, 1.0], |_, _| [f64::NAN, 0.0, 0.0]).is_err()
        );
        assert!(Surface3D::try_new(4, 4, [0.0, 1.0], [0.0, 1.0], |_, _| [2e6, 0.0, 0.0]).is_err());
        assert!(Surface3D::esfera(0.0, 8, 8).is_err());
        assert!(Surface3D::esfera(f64::INFINITY, 8, 8).is_err());
        assert!(Surface3D::toro(1.0, 1.0, 8, 8).is_err());
        assert!(Surface3D::toro(1.0, 2.0, 8, 8).is_err());
        assert!(Surface3D::toro(1.0, 0.5, 8, 8).is_ok());
        assert!(Surface3D::plano(0.0, 1.0, 4, 4).is_err());
        // Curva: n, ventana y puntos.
        assert!(Curva3D::traza_en(1, 0.0, 1.0, |t| [t, 0.0, 0.0]).is_err());
        assert!(Curva3D::traza_en(513, 0.0, 1.0, |t| [t, 0.0, 0.0]).is_err());
        assert!(Curva3D::traza_en(8, 1.0, 1.0, |t| [t, 0.0, 0.0]).is_err());
        assert!(Curva3D::traza_en(8, 1.0, 0.0, |t| [t, 0.0, 0.0]).is_err());
        assert!(Curva3D::traza_en(8, 0.0, 1.0, |_| [f64::INFINITY, 0.0, 0.0]).is_err());
        // Ejes y luz.
        assert!(Ejes3D::try_new([0.0, 0.0, 0.0], [1.0, 1.0, 1.0], [65, 0, 0]).is_err());
        assert!(Ejes3D::try_new([0.0, 0.0, 0.0], [0.0, 1.0, 1.0], [4, 4, 4]).is_err());
        assert!(Ejes3D::try_new([0.0, 0.0, 0.0], [1.0, 1.0, 1.0], [4, 4, 4])
            .unwrap()
            .ticks_en(3)
            .is_err());
        let m = Surface3D::esfera(1.0, 8, 8).unwrap();
        assert!(m.solido(cam_persp(), [0.0, 0.0, 0.0]).is_err());
        assert!(m.solido(cam_persp(), [f64::NAN, 0.0, 0.0]).is_err());
    }

    #[test]
    fn res_maxima_no_excede_tris_y_sort_no_es_cuadratico() {
        // 128² celdas: el df ordena 32768−256 tris (polos degenerados fuera)
        // en O(n log n) —este test lo acota por construcción + tiempo sano.
        let m = Surface3D::esfera(1.0, SURF3D_MAX_RES, SURF3D_MAX_RES).unwrap();
        assert_eq!(m.puntos.len(), SURF3D_MAX_VERTICES);
        let caras = m.solido(cam_persp(), [0.5, 1.0, 0.3]).unwrap();
        assert!(caras.len() <= SURF3D_MAX_TRIS);
        assert!(caras.len() > SURF3D_MAX_TRIS - 1000);
        for par in caras.windows(2) {
            assert!(par[0].depth + 1e-9 >= par[1].depth);
        }
    }

    #[test]
    fn curva_corta_el_trazo_detras_de_la_camara() {
        // z(t) = -10(t-1)²+10: visible, oculta (z>5), visible → 2 trazos.
        let c = Curva3D::traza_en(41, 0.0, 2.0, |t| {
            let x = t - 1.0;
            [x, 0.0, -10.0 * x * x + 10.0]
        })
        .unwrap();
        assert_eq!(c.puntos.len(), 41);
        let trazos = c.proyecta(cam_persp());
        assert_eq!(trazos.len(), 2, "trazos {trazos:?}");
        for tr in &trazos {
            assert!(tr.len() >= 2);
            for p in tr {
                assert!(p.iter().all(|x| x.is_finite()));
            }
        }
        // Todo adelante → un solo trazo.
        let recta = Curva3D::traza_en(16, 0.0, 1.0, |t| [-2.0 + 4.0 * t, 0.0, 0.0]).unwrap();
        assert_eq!(recta.proyecta(cam_persp()).len(), 1);
        // Todo atrás → vacío honesto (oclusión, no error).
        let atras = Curva3D::traza_en(16, 0.0, 1.0, |t| [t, 0.0, 10.0]).unwrap();
        assert!(atras.proyecta(cam_persp()).is_empty());
    }

    #[test]
    fn ejes_ticks_y_labels_coinciden_con_project_3d() {
        let ejes = Ejes3D::try_new([0.0, 0.0, 0.0], [2.0, 1.0, 1.0], [5, 0, 3]).unwrap();
        let seg = ejes.ejes_segmentos();
        assert_eq!(seg[0], [[-2.0, 0.0, 0.0], [2.0, 0.0, 0.0]]);
        let tx = ejes.ticks_en(0).unwrap();
        assert_eq!(tx.len(), 5);
        assert!((tx[0][0] + 2.0).abs() < 1e-12);
        assert!((tx[4][0] - 2.0).abs() < 1e-12);
        assert!(ejes.ticks_en(1).unwrap().is_empty());
        let labels = ejes.etiquetas(cam_orto());
        assert_eq!(labels.len(), 8);
        for l in &labels {
            let esperado = cam_orto().project_3d(l.pos3d).unwrap();
            assert_eq!(l.pos, Some(esperado));
            assert_eq!(l.texto, format!("{:.2}", l.valor));
        }
        // El primero del eje x vale -2.00.
        let primero = labels.iter().find(|l| l.eje == 0).unwrap();
        assert_eq!(primero.texto, "-2.00");
        // Detrás de la cámara el label queda en None honesto.
        let atras = Ejes3D::try_new([0.0, 0.0, 10.0], [1.0, 1.0, 1.0], [2, 0, 0]).unwrap();
        for l in atras.etiquetas(cam_persp()) {
            assert_eq!(l.pos, None);
        }
    }

    #[test]
    fn alambre_ordena_lejos_primero_y_acota() {
        let plano = Surface3D::plano(2.0, 2.0, 2, 2).unwrap();
        let trazos = plano.alambre(cam_orto()).unwrap();
        // (nu+1)·nv + (nv+1)·nu = 12 aristas, todas a depth 0 (z = 0).
        assert_eq!(trazos.len(), 12);
        for t in &trazos {
            assert_eq!(t.depth, 0.0);
        }
        let m = Surface3D::esfera(1.0, 12, 12).unwrap();
        let al = m.alambre(cam_persp()).unwrap();
        assert_eq!(al.len(), (13 * 12) + (13 * 12));
        for par in al.windows(2) {
            assert!(par[0].depth + 1e-12 >= par[1].depth);
        }
        // La res default existe y es sensata (32 → 2048 tris).
        assert_eq!(SURF3D_DEFAULT_RES, 32);
        let _ = RateFunc::Linear;
    }
}
