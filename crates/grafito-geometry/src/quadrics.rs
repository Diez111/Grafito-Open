//! Clasificación real de cuádricas `a*x² + b*y² + c*z² + d*xy + e*yz + f*zx +
//! g*x + h*y + i*z + j = 0` y malla wireframe exacta por tipo.
//!
//! Cerebro puro: sin `egui`, sin `wgpu`. Todo es `f64` finito y acotado:
//! - La matriz simétrica `Q(a..f)` se diagonaliza con autovalores cerrados del
//!   polinomio característico cúbico (Cardano trigonométrico, tres raíces
//!   reales para `Q` simétrica) y centro por Cramer.
//! - Los invariantes (rango de `Q`, signatura, residuo `k` tras trasladar)
//!   distinguen 17 tipos afines reales.
//! - Cada tipo con lugar real no vacío emite polilíneas wireframe exactas
//!   (esfera escalada/rotada, `cosh`/`sinh` en hiperboloides, silla
//!   doblemente reglada, abanico desde el ápice en el cono, generatrices en
//!   cilindros, rejillas en planos).
//! - Lo degenerado sin superficie (imaginarios, punto, recta, vacío,
//!   coeficientes nulos) devuelve `Err` honesto: jamás se dibuja un
//!   elipsoide falso.
//!
//! Presupuesto: ninguna malla supera [`QUADRIC_MAX_WIRE_SEGMENTS`] segmentos
//! (96, igual que el wireframe legacy de 3 círculos × 32), así el render
//! (`QUADRIC_WIRE_SEGMENTS` en `depth_3d`) no necesita recalibración.

use crate::Point3D;
use core::fmt;

/// Tolerancia relativa para rangos y residuos de la clasificación.
pub const QUADRIC_CLASSIFY_EPS: f64 = 1e-9;
/// Semiextensión local (unidades del mundo) para recortar superficies abiertas.
pub const QUADRIC_OPEN_HALF_EXTENT: f64 = 8.0;
/// Pasos alrededor del eje en anillos de la malla.
pub const QUADRIC_U_STEPS: usize = 24;
/// Tope de segmentos wireframe por cuádrica (presupuesto de render).
pub const QUADRIC_MAX_WIRE_SEGMENTS: usize = 96;

/// Los 17 tipos afines reales de cuádricas.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum QuadricKind {
    /// Esfera (autovalores iguales, centro y radio reales).
    Sphere,
    /// Elipsoide real (incluye esferas escaladas y rotadas).
    Ellipsoid,
    /// `x²/a² + y²/b² - z²/c² = 1`: una hoja conexa.
    HyperboloidOneSheet,
    /// `x²/a² + y²/b² - z²/c² = -1`: dos hojas.
    HyperboloidTwoSheets,
    /// `w = dir·r²` sobre el plano propio.
    EllipticParaboloid,
    /// Silla doblemente reglada.
    HyperbolicParaboloid,
    /// Cono real con ápice (reglado por el ápice).
    Cone,
    /// Elipse extruida sobre el eje nulo.
    EllipticCylinder,
    /// Hipérbola extruida sobre el eje nulo.
    HyperbolicCylinder,
    /// Parábola extruida sobre el eje nulo.
    ParabolicCylinder,
    /// Par de planos reales que se cortan en el eje nulo.
    IntersectingPlanes,
    /// Par de planos reales paralelos distintos.
    ParallelPlanes,
    /// Plano (doble o lineal `l·p + j = 0`).
    CoincidentPlane,
    /// Lugar vacío tipo elipsoide.
    ImaginaryEllipsoid,
    /// Un punto, sin superficie.
    ImaginaryCone,
    /// Vacío o recta, sin superficie.
    ImaginaryCylinder,
    /// Lugar vacío tipo planos.
    ImaginaryPlanes,
}

impl QuadricKind {
    /// Nombre estable en español para UI y badge honesto.
    #[must_use]
    pub const fn name(self) -> &'static str {
        match self {
            Self::Sphere => "esfera",
            Self::Ellipsoid => "elipsoide",
            Self::HyperboloidOneSheet => "hiperboloide de una hoja",
            Self::HyperboloidTwoSheets => "hiperboloide de dos hojas",
            Self::EllipticParaboloid => "paraboloide elíptico",
            Self::HyperbolicParaboloid => "paraboloide hiperbólico",
            Self::Cone => "cono",
            Self::EllipticCylinder => "cilindro elíptico",
            Self::HyperbolicCylinder => "cilindro hiperbólico",
            Self::ParabolicCylinder => "cilindro parabólico",
            Self::IntersectingPlanes => "planos que se cortan",
            Self::ParallelPlanes => "planos paralelos",
            Self::CoincidentPlane => "plano",
            Self::ImaginaryEllipsoid => "elipsoide imaginario (vacío)",
            Self::ImaginaryCone => "cono imaginario (punto)",
            Self::ImaginaryCylinder => "cilindro imaginario (vacío)",
            Self::ImaginaryPlanes => "planos imaginarios (vacío)",
        }
    }

    /// ¿Tiene superficie real para mallar?
    #[must_use]
    pub const fn has_real_locus(self) -> bool {
        !matches!(
            self,
            Self::ImaginaryEllipsoid
                | Self::ImaginaryCone
                | Self::ImaginaryCylinder
                | Self::ImaginaryPlanes
        )
    }
}

/// Error honesto de clasificación o mallado de cuádricas.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum QuadricError {
    /// Algún coeficiente no es finito.
    NonFinite,
    /// Sin parte cuadrática ni lineal (constante sola o todo cero).
    Empty,
    /// Tipo clasificado pero sin superficie real (vacío, punto o recta).
    NoRealLocus { kind: QuadricKind },
    /// Parámetros fuera de rango mallable (no finitos o gigantes).
    Unbounded,
}

impl fmt::Display for QuadricError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::NonFinite => write!(f, "cuádrica con coeficientes no finitos"),
            Self::Empty => write!(f, "cuádrica degenerada sin superficie (vacía o total)"),
            Self::NoRealLocus { kind } => {
                write!(f, "cuádrica degenerada sin superficie: {}", kind.name())
            }
            Self::Unbounded => write!(f, "cuádrica con parámetros fuera de rango mallable"),
        }
    }
}

impl std::error::Error for QuadricError {}

/// Cuádrica clasificada: tipo + marco ortonormal + parámetros.
///
/// `center` es el centro (o ápice/vértice/punto del eje) en coordenadas del
/// mundo; `axes` son las columnas del marco propio (`axes[2]` es el eje
/// distinguido: eje del cono, transversal de hiperboloides, nulo de
/// cilindros/planos o normal del plano). `params` depende del tipo:
/// - Esfera/elipsoide: semiejes `(rx, ry, rz)`.
/// - Hiperboloides: semiejes `(a, b, c)` con `x²/a² + y²/b² - z²/c² = ±1`.
/// - Paraboloide elíptico: `(a, b, dir)` con `w = dir·r²`, `dir = ±1`.
/// - Paraboloide hiperbólico: `(a, b, 0)` con `w = (x/a)² - (y/b)²`.
/// - Cono: pendientes `(p, q, h)` con `w = ±h`, `u = ±p·h`, `v = ±q·h`.
/// - Cilindros elíptico/hiperbólico: `(a, b, h)` (semiejes y semialtura).
/// - Cilindro parabólico: `(lambda, lineal, k)` crudos de
///   `lambda·q² + lineal·s + k = 0`.
/// - Planos que se cortan: `(m, 0, 0)` con direcciones `e0 ± m·e1`.
/// - Planos paralelos: `(offset, 0, 0)` a `±offset` sobre `e0`.
/// - Plano coincidente: `(0, 0, 0)`.
#[derive(Debug, Clone, Copy)]
pub struct QuadricShape {
    pub kind: QuadricKind,
    pub center: [f64; 3],
    pub axes: [[f64; 3]; 3],
    pub params: [f64; 3],
}

/// Matriz simétrica `Q` desde `[a, b, c, d, e, f, g, h, i, j]`.
#[must_use]
pub const fn quadric_matrix(coeffs: [f64; 10]) -> [[f64; 3]; 3] {
    [
        [coeffs[0], coeffs[3] / 2.0, coeffs[5] / 2.0],
        [coeffs[3] / 2.0, coeffs[1], coeffs[4] / 2.0],
        [coeffs[5] / 2.0, coeffs[4] / 2.0, coeffs[2]],
    ]
}

fn dot(a: [f64; 3], b: [f64; 3]) -> f64 {
    a[0] * b[0] + a[1] * b[1] + a[2] * b[2]
}

fn cross(a: [f64; 3], b: [f64; 3]) -> [f64; 3] {
    [
        a[1] * b[2] - a[2] * b[1],
        a[2] * b[0] - a[0] * b[2],
        a[0] * b[1] - a[1] * b[0],
    ]
}

fn norm(v: [f64; 3]) -> f64 {
    dot(v, v).sqrt()
}

fn mat_vec(m: [[f64; 3]; 3], v: [f64; 3]) -> [f64; 3] {
    [dot(m[0], v), dot(m[1], v), dot(m[2], v)]
}

fn det3(m: [[f64; 3]; 3]) -> f64 {
    m[0][0] * (m[1][1] * m[2][2] - m[1][2] * m[2][1])
        - m[0][1] * (m[1][0] * m[2][2] - m[1][2] * m[2][0])
        + m[0][2] * (m[1][0] * m[2][1] - m[1][1] * m[2][0])
}

/// Autovalores de la matriz simétrica `Q` por su polinomio característico
/// cúbico `λ³ - c1·λ² + c2·λ - c3 = 0`, resuelto en forma cerrada (Cardano
/// trigonométrico: `Q` simétrica siempre tiene tres raíces reales).
/// Devuelve `None` si la aritmética no es finita. Salida descendente.
fn symmetric_eigenvalues(q: [[f64; 3]; 3]) -> Option<[f64; 3]> {
    let c1 = q[0][0] + q[1][1] + q[2][2];
    let c2 = q[0][0] * q[1][1] - q[0][1] * q[0][1] + q[0][0] * q[2][2] - q[0][2] * q[0][2]
        + q[1][1] * q[2][2]
        - q[1][2] * q[1][2];
    let c3 = det3(q);
    if !(c1.is_finite() && c2.is_finite() && c3.is_finite()) {
        return None;
    }
    // Deprimida `y³ + p·y + qq = 0` con `λ = y + c1/3`.
    let p = c2 - c1 * c1 / 3.0;
    let qq = -2.0 * c1 * c1 * c1 / 27.0 + c1 * c2 / 3.0 - c3;
    if !(p.is_finite() && qq.is_finite()) {
        return None;
    }
    let offset = c1 / 3.0;
    let half = qq / 2.0;
    let third = p / 3.0;
    let disc = half * half + third * third * third;
    if !disc.is_finite() {
        return None;
    }
    if disc <= 0.0 {
        // Tres raíces reales: forma trigonométrica de Cardano.
        let r = (-third).max(0.0).sqrt();
        let denom = r * r * r;
        if !(r.is_finite() && r > 0.0 && denom.is_finite() && denom > 0.0) {
            // Raíz triple aproximada (esfera): traza repartida.
            return offset.is_finite().then_some([offset, offset, offset]);
        }
        let arg = (-half / denom).clamp(-1.0, 1.0);
        let phi = arg.acos();
        if !phi.is_finite() {
            return None;
        }
        let two_r = 2.0 * r;
        let mut out = [
            two_r * (phi / 3.0).cos() + offset,
            two_r * ((phi - 2.0 * core::f64::consts::PI) / 3.0).cos() + offset,
            two_r * ((phi + 2.0 * core::f64::consts::PI) / 3.0).cos() + offset,
        ];
        out.sort_by(|x, y| y.total_cmp(x));
        out.iter().all(|v| v.is_finite()).then_some(out)
    } else {
        // Solo por redondeo en raíz múltiple: una real + par que conserva traza.
        let root = disc.sqrt();
        let y0 = (-half + root).cbrt() + (-half - root).cbrt();
        if !y0.is_finite() {
            return None;
        }
        let mut out = [y0 + offset, -y0 / 2.0 + offset, -y0 / 2.0 + offset];
        out.sort_by(|x, y| y.total_cmp(x));
        out.iter().all(|v| v.is_finite()).then_some(out)
    }
}

/// Autovector unitario para un autovalor aislado (nulidad 1D): cruza los
/// pares de filas de `Q - λI` y se queda con el mayor. Solo se usa cuando el
/// autovalor no se repite, así el resultado es el eje propio verdadero.
fn eigenvector_for(q: [[f64; 3]; 3], lambda: f64, scale: f64) -> [f64; 3] {
    let m = [
        [q[0][0] - lambda, q[0][1], q[0][2]],
        [q[1][0], q[1][1] - lambda, q[1][2]],
        [q[2][0], q[2][1], q[2][2] - lambda],
    ];
    let candidates = [cross(m[0], m[1]), cross(m[0], m[2]), cross(m[1], m[2])];
    let mut best = [1.0, 0.0, 0.0];
    let mut best_len = 0.0;
    for cand in candidates {
        let len = norm(cand);
        if len.is_finite() && len > best_len {
            best_len = len;
            best = cand;
        }
    }
    if best_len > 1e-12 * scale.max(1.0) {
        return [best[0] / best_len, best[1] / best_len, best[2] / best_len];
    }
    // Inalcanzable para autovalor aislado de matriz simétrica; finito igual.
    best
}

/// Eje canónico más ortogonal a `against` (para completar multiplicidades).
fn canonical_orthogonal(against: [f64; 3]) -> [f64; 3] {
    let mut best = [1.0, 0.0, 0.0];
    let mut score = f64::INFINITY;
    for cand in [[1.0, 0.0, 0.0], [0.0, 1.0, 0.0], [0.0, 0.0, 1.0]] {
        let align = dot(cand, against).abs();
        if align.is_finite() && align < score {
            score = align;
            best = cand;
        }
    }
    let d = dot(best, against);
    let v = [
        best[0] - d * against[0],
        best[1] - d * against[1],
        best[2] - d * against[2],
    ];
    let len = norm(v);
    if len.is_finite() && len > 1e-12 {
        [v[0] / len, v[1] / len, v[2] / len]
    } else {
        best
    }
}

/// Marco ortonormal en el orden de `lambdas` (descendente), diestro.
///
/// Los autovalores repetidos generan un plano/espacio propio completo: el
/// eje aislado se resuelve por cruces y el resto se completa con
/// ortogonales exactos, así ningún eje cae fuera de su espacio propio.
fn eigenframe(q: [[f64; 3]; 3], lambdas: [f64; 3]) -> [[f64; 3]; 3] {
    let scale = lambdas[0].abs().max(1.0);
    let gtol = 1e-9 * scale.max(1e-300);
    let eq01 = (lambdas[0] - lambdas[1]).abs() <= gtol;
    let eq12 = (lambdas[1] - lambdas[2]).abs() <= gtol;
    if eq01 && eq12 {
        return [[1.0, 0.0, 0.0], [0.0, 1.0, 0.0], [0.0, 0.0, 1.0]];
    }
    if eq01 {
        // Plano propio λ0≈λ1; eje aislado λ2 por cruces.
        let w = eigenvector_for(q, lambdas[2], scale);
        let u = canonical_orthogonal(w);
        let v = cross(w, u);
        return [u, v, w];
    }
    if eq12 {
        let u = eigenvector_for(q, lambdas[0], scale);
        let v = canonical_orthogonal(u);
        let w = cross(u, v);
        return [u, v, w];
    }
    let mut vecs = [
        eigenvector_for(q, lambdas[0], scale),
        eigenvector_for(q, lambdas[1], scale),
        eigenvector_for(q, lambdas[2], scale),
    ];
    if dot(vecs[2], cross(vecs[0], vecs[1])) < 0.0 {
        vecs[2] = [-vecs[2][0], -vecs[2][1], -vecs[2][2]];
    }
    vecs
}

fn finite_shape(
    kind: QuadricKind,
    center: [f64; 3],
    axes: [[f64; 3]; 3],
    params: [f64; 3],
) -> Option<QuadricShape> {
    if center.iter().all(|v| v.is_finite())
        && axes.iter().flatten().all(|v| v.is_finite())
        && params.iter().all(|v| v.is_finite())
    {
        Some(QuadricShape {
            kind,
            center,
            axes,
            params,
        })
    } else {
        None
    }
}

/// Clasifica la cuádrica de coeficientes `[a, b, c, d, e, f, g, h, i, j]`.
///
/// Devuelve `Err` honesto cuando no hay superficie real (vacío, punto,
/// recta), los coeficientes no son finitos o la forma está fuera de rango.
pub fn classify_quadric(coeffs: [f64; 10]) -> Result<QuadricShape, QuadricError> {
    if coeffs.iter().any(|v| !v.is_finite()) {
        return Err(QuadricError::NonFinite);
    }
    let q = quadric_matrix(coeffs);
    let lin = [coeffs[6], coeffs[7], coeffs[8]];
    let j = coeffs[9];
    let qnorm = q.iter().flatten().fold(0.0_f64, |m, v| m.max(v.abs()));
    if !qnorm.is_finite() {
        return Err(QuadricError::Unbounded);
    }
    let tol = QUADRIC_CLASSIFY_EPS * qnorm.max(1.0);
    // Sin parte cuadrática: plano lineal o constante.
    if qnorm <= QUADRIC_CLASSIFY_EPS {
        let lin_norm = norm(lin);
        if lin_norm > QUADRIC_CLASSIFY_EPS {
            let n = [lin[0] / lin_norm, lin[1] / lin_norm, lin[2] / lin_norm];
            let center = [
                -j * n[0] / lin_norm,
                -j * n[1] / lin_norm,
                -j * n[2] / lin_norm,
            ];
            let helper = if n[0].abs() < 0.9 {
                [1.0, 0.0, 0.0]
            } else {
                [0.0, 1.0, 0.0]
            };
            let e0 = cross(n, helper);
            let e0_len = norm(e0);
            if !(e0_len.is_finite() && e0_len > 1e-12) {
                return Err(QuadricError::Unbounded);
            }
            let e0 = [e0[0] / e0_len, e0[1] / e0_len, e0[2] / e0_len];
            let e1 = cross(n, e0);
            return finite_shape(
                QuadricKind::CoincidentPlane,
                center,
                [e0, e1, n],
                [0.0, 0.0, 0.0],
            )
            .ok_or(QuadricError::Unbounded);
        }
        return Err(QuadricError::Empty);
    }
    let lambdas = symmetric_eigenvalues(q).ok_or(QuadricError::Unbounded)?;
    let vecs = eigenframe(q, lambdas);
    let rank = lambdas.iter().filter(|v| v.abs() > tol).count();

    if rank == 3 {
        return classify_central(q, lin, j, lambdas, vecs, tol);
    }
    if rank == 2 {
        return classify_rank2(lin, j, lambdas, vecs, tol);
    }
    if rank == 1 {
        return classify_rank1(lin, j, lambdas, vecs, tol);
    }
    Err(QuadricError::Empty)
}

/// Centro por Cramer sobre `Q·t = -l/2` + invariantes de signatura.
fn classify_central(
    q: [[f64; 3]; 3],
    lin: [f64; 3],
    j: f64,
    lambdas: [f64; 3],
    vecs: [[f64; 3]; 3],
    tol: f64,
) -> Result<QuadricShape, QuadricError> {
    let det = det3(q);
    if !(det.is_finite() && det.abs() > tol * tol * tol) {
        return Err(QuadricError::Unbounded);
    }
    let rhs = [-lin[0] / 2.0, -lin[1] / 2.0, -lin[2] / 2.0];
    let c0 = cross(q[1], q[2]);
    let c1 = cross(q[2], q[0]);
    let c2 = cross(q[0], q[1]);
    let center = [
        (c0[0] * rhs[0] + c1[0] * rhs[1] + c2[0] * rhs[2]) / det,
        (c0[1] * rhs[0] + c1[1] * rhs[1] + c2[1] * rhs[2]) / det,
        (c0[2] * rhs[0] + c1[2] * rhs[1] + c2[2] * rhs[2]) / det,
    ];
    if !center.iter().all(|v| v.is_finite()) {
        return Err(QuadricError::Unbounded);
    }
    let kval = j + 0.5 * dot(center, lin);
    if !kval.is_finite() {
        return Err(QuadricError::Unbounded);
    }
    let ktol = QUADRIC_CLASSIFY_EPS * (1.0 + j.abs() + dot(center, mat_vec(q, center)).abs());
    let npos = lambdas.iter().filter(|v| **v > 0.0).count();
    if npos == 3 || npos == 0 {
        let s = if npos == 3 { 1.0 } else { -1.0 };
        let rhs_k = -kval * s;
        if rhs_k > ktol {
            let radii = [
                (rhs_k / lambdas[0].abs()).sqrt(),
                (rhs_k / lambdas[1].abs()).sqrt(),
                (rhs_k / lambdas[2].abs()).sqrt(),
            ];
            if !(radii.iter().all(|v| v.is_finite() && *v > 0.0) && radii.iter().all(|v| *v <= 1e6))
            {
                return Err(QuadricError::Unbounded);
            }
            let spread = (lambdas[0] - lambdas[2]).abs() / lambdas[0].abs().max(1e-300);
            let kind = if spread <= 1e-6 {
                QuadricKind::Sphere
            } else {
                QuadricKind::Ellipsoid
            };
            return finite_shape(kind, center, vecs, radii).ok_or(QuadricError::Unbounded);
        }
        if rhs_k < -ktol {
            return Err(QuadricError::NoRealLocus {
                kind: QuadricKind::ImaginaryEllipsoid,
            });
        }
        return Err(QuadricError::NoRealLocus {
            kind: QuadricKind::ImaginaryCone,
        });
    }
    // Signatura mixta.
    if kval.abs() <= ktol {
        let (p, qq) = cone_slopes(lambdas)?;
        let h = QUADRIC_OPEN_HALF_EXTENT
            .min(2.0 * p.max(qq).max(1.0))
            .max(1.0);
        return finite_shape(QuadricKind::Cone, center, vecs, [p, qq, h])
            .ok_or(QuadricError::Unbounded);
    }
    let rhs_k = -kval;
    let radii = [
        (rhs_k.abs() / lambdas[0].abs()).sqrt(),
        (rhs_k.abs() / lambdas[1].abs()).sqrt(),
        (rhs_k.abs() / lambdas[2].abs()).sqrt(),
    ];
    if !(radii.iter().all(|v| v.is_finite() && *v > 0.0) && radii.iter().all(|v| *v <= 1e6)) {
        return Err(QuadricError::Unbounded);
    }
    // Una hoja ⟺ `(npos == 2) == (rhs_k > 0)`.
    let one_sheet = (npos == 2) == (rhs_k > 0.0);
    let kind = if one_sheet {
        QuadricKind::HyperboloidOneSheet
    } else {
        QuadricKind::HyperboloidTwoSheets
    };
    finite_shape(kind, center, reorder_mixed_axes(vecs, lambdas, npos), radii)
        .ok_or(QuadricError::Unbounded)
}

/// Rango 2: paraboloides (término axial) o cilindros/planos en el plano.
#[allow(clippy::too_many_arguments)]
fn classify_rank2(
    lin: [f64; 3],
    j: f64,
    lambdas: [f64; 3],
    vecs: [[f64; 3]; 3],
    tol: f64,
) -> Result<QuadricShape, QuadricError> {
    let null_idx = lambdas.iter().position(|v| v.abs() <= tol).unwrap_or(2);
    let mut planar = [0, 0];
    let mut found = 0;
    for (i, v) in lambdas.iter().enumerate() {
        if i != null_idx && v.abs() > 0.0 && found < 2 {
            planar[found] = i;
            found += 1;
        }
    }
    if found != 2 {
        return Err(QuadricError::Unbounded);
    }
    let (i1, i2) = (planar[0], planar[1]);
    let (l1, l2) = (lambdas[i1], lambdas[i2]);
    let (e1, e2, w) = (vecs[i1], vecs[i2], vecs[null_idx]);
    let lp1 = dot(lin, e1);
    let lp2 = dot(lin, e2);
    let la = dot(lin, w);
    let latol = QUADRIC_CLASSIFY_EPS * (1.0 + norm(lin));
    let kp = j - (lp1 * lp1 / (4.0 * l1) + lp2 * lp2 / (4.0 * l2));
    if !kp.is_finite() {
        return Err(QuadricError::Unbounded);
    }
    let base = [
        -(lp1 / (2.0 * l1)) * e1[0] - (lp2 / (2.0 * l2)) * e2[0],
        -(lp1 / (2.0 * l1)) * e1[1] - (lp2 / (2.0 * l2)) * e2[1],
        -(lp1 / (2.0 * l1)) * e1[2] - (lp2 / (2.0 * l2)) * e2[2],
    ];
    if !base.iter().all(|v| v.is_finite()) {
        return Err(QuadricError::Unbounded);
    }
    if la.abs() > latol {
        let s0 = -kp / la;
        if !s0.is_finite() {
            return Err(QuadricError::Unbounded);
        }
        let center = [
            base[0] + s0 * w[0],
            base[1] + s0 * w[1],
            base[2] + s0 * w[2],
        ];
        let a = (la.abs() / l1.abs()).sqrt();
        let b = (la.abs() / l2.abs()).sqrt();
        if !(a.is_finite() && b.is_finite() && a > 0.0 && b > 0.0 && a <= 1e6 && b <= 1e6) {
            return Err(QuadricError::Unbounded);
        }
        if (l1 > 0.0) == (l2 > 0.0) {
            let dir = -l1.signum() * la.signum();
            return finite_shape(
                QuadricKind::EllipticParaboloid,
                center,
                [e1, e2, w],
                [a, b, dir],
            )
            .ok_or(QuadricError::Unbounded);
        }
        return finite_shape(
            QuadricKind::HyperbolicParaboloid,
            center,
            [e1, e2, w],
            [a, b, 0.0],
        )
        .ok_or(QuadricError::Unbounded);
    }
    let kptol = QUADRIC_CLASSIFY_EPS * (1.0 + kp.abs() + j.abs());
    let same_sign = (l1 > 0.0) == (l2 > 0.0);
    if same_sign {
        if kp < -kptol {
            let a = (-kp / l1.abs()).sqrt();
            let b = (-kp / l2.abs()).sqrt();
            if !(a.is_finite() && b.is_finite() && a > 0.0 && b > 0.0 && a <= 1e6 && b <= 1e6) {
                return Err(QuadricError::Unbounded);
            }
            return finite_shape(
                QuadricKind::EllipticCylinder,
                base,
                [e1, e2, w],
                [a, b, QUADRIC_OPEN_HALF_EXTENT],
            )
            .ok_or(QuadricError::Unbounded);
        }
        return Err(QuadricError::NoRealLocus {
            kind: QuadricKind::ImaginaryCylinder,
        });
    }
    if kp.abs() <= kptol {
        let m = (l1.abs() / l2.abs().max(1e-300)).sqrt();
        if !(m.is_finite() && m > 0.0 && m <= 1e6) {
            return Err(QuadricError::Unbounded);
        }
        return finite_shape(
            QuadricKind::IntersectingPlanes,
            base,
            [e1, e2, w],
            [m, 0.0, 0.0],
        )
        .ok_or(QuadricError::Unbounded);
    }
    let a = (kp.abs() / l1.abs()).sqrt();
    let b = (kp.abs() / l2.abs()).sqrt();
    if !(a.is_finite() && b.is_finite() && a > 0.0 && b > 0.0 && a <= 1e6 && b <= 1e6) {
        return Err(QuadricError::Unbounded);
    }
    finite_shape(
        QuadricKind::HyperbolicCylinder,
        base,
        [e1, e2, w],
        [a, b, QUADRIC_OPEN_HALF_EXTENT],
    )
    .ok_or(QuadricError::Unbounded)
}

/// Rango 1: cilindro parabólico o planos (paralelos, doble o imaginarios).
#[allow(clippy::too_many_arguments)]
fn classify_rank1(
    lin: [f64; 3],
    j: f64,
    lambdas: [f64; 3],
    vecs: [[f64; 3]; 3],
    tol: f64,
) -> Result<QuadricShape, QuadricError> {
    let q_idx = lambdas.iter().position(|v| v.abs() > tol).unwrap_or(0);
    let lambda = lambdas[q_idx];
    let mut rest = [0, 0];
    let mut found = 0;
    for (i, _) in lambdas.iter().enumerate() {
        if i != q_idx && found < 2 {
            rest[found] = i;
            found += 1;
        }
    }
    if found != 2 {
        return Err(QuadricError::Unbounded);
    }
    let (u, vdir, wdir) = (vecs[q_idx], vecs[rest[0]], vecs[rest[1]]);
    let lq = dot(lin, u);
    let lb = dot(lin, vdir);
    let lc = dot(lin, wdir);
    let lin_tol = QUADRIC_CLASSIFY_EPS * (1.0 + norm(lin));
    let kp = j - lq * lq / (4.0 * lambda);
    if !kp.is_finite() {
        return Err(QuadricError::Unbounded);
    }
    let shift = -lq / (2.0 * lambda);
    if !shift.is_finite() {
        return Err(QuadricError::Unbounded);
    }
    let base = [shift * u[0], shift * u[1], shift * u[2]];
    let transverse = (lb * lb + lc * lc).sqrt();
    if transverse > lin_tol {
        return finite_shape(
            QuadricKind::ParabolicCylinder,
            base,
            [u, vdir, wdir],
            [lambda, transverse, kp],
        )
        .ok_or(QuadricError::Unbounded);
    }
    let kptol = QUADRIC_CLASSIFY_EPS * (1.0 + kp.abs() + j.abs());
    if kp.abs() <= kptol {
        let helper = if u[0].abs() < 0.9 {
            [1.0, 0.0, 0.0]
        } else {
            [0.0, 1.0, 0.0]
        };
        let t1 = cross(u, helper);
        let t1_len = norm(t1);
        if !(t1_len.is_finite() && t1_len > 1e-12) {
            return Err(QuadricError::Unbounded);
        }
        let t1 = [t1[0] / t1_len, t1[1] / t1_len, t1[2] / t1_len];
        let t2 = cross(u, t1);
        return finite_shape(
            QuadricKind::CoincidentPlane,
            base,
            [t1, t2, u],
            [0.0, 0.0, 0.0],
        )
        .ok_or(QuadricError::Unbounded);
    }
    if (lambda > 0.0) == (kp < 0.0) {
        let offset = (-kp / lambda.abs().max(1e-300)).sqrt();
        if !(offset.is_finite() && offset <= 1e6) {
            return Err(QuadricError::Unbounded);
        }
        return finite_shape(
            QuadricKind::ParallelPlanes,
            base,
            [u, vdir, wdir],
            [offset, 0.0, 0.0],
        )
        .ok_or(QuadricError::Unbounded);
    }
    Err(QuadricError::NoRealLocus {
        kind: QuadricKind::ImaginaryPlanes,
    })
}

/// Pendientes del cono de signatura mixta con `k ≈ 0`.
fn cone_slopes(lambdas: [f64; 3]) -> Result<(f64, f64), QuadricError> {
    let mut pos: Vec<f64> = lambdas.iter().copied().filter(|v| *v > 0.0).collect();
    let mut neg: Vec<f64> = lambdas.iter().copied().filter(|v| *v < 0.0).collect();
    pos.sort_by(|x, y| x.total_cmp(y));
    neg.sort_by(|x, y| y.total_cmp(x));
    let (p, q) = if pos.len() == 2 && neg.len() == 1 {
        (
            (pos[0] / neg[0].abs()).sqrt(),
            (pos[1] / neg[0].abs()).sqrt(),
        )
    } else if pos.len() == 1 && neg.len() == 2 {
        (
            (neg[0].abs() / pos[0]).sqrt(),
            (neg[1].abs() / pos[0]).sqrt(),
        )
    } else {
        return Err(QuadricError::Unbounded);
    };
    if p.is_finite() && q.is_finite() && p > 0.0 && q > 0.0 && p <= 1e6 && q <= 1e6 {
        Ok((p, q))
    } else {
        Err(QuadricError::Unbounded)
    }
}

/// Eje distinguido al final para signatura mixta (negativo si `npos == 2`).
fn reorder_mixed_axes(vecs: [[f64; 3]; 3], lambdas: [f64; 3], npos: usize) -> [[f64; 3]; 3] {
    let mut out = vecs;
    if npos == 2 {
        if lambdas[2] > 0.0 {
            if lambdas[0] < 0.0 {
                out = [vecs[1], vecs[2], vecs[0]];
            } else {
                out = [vecs[0], vecs[2], vecs[1]];
            }
        }
    } else if npos == 1 {
        if lambdas[1] > 0.0 {
            out = [vecs[1], vecs[0], vecs[2]];
        } else if lambdas[2] > 0.0 {
            out = [vecs[2], vecs[1], vecs[0]];
        }
    }
    if dot(out[2], cross(out[0], out[1])) < 0.0 {
        out[2] = [-out[2][0], -out[2][1], -out[2][2]];
    }
    out
}

// ── Malla wireframe exacta ────────────────────────────────────────────────

fn push_point(
    out: &mut Vec<Point3D>,
    center: [f64; 3],
    axes: [[f64; 3]; 3],
    local: [f64; 3],
) -> bool {
    let p = [
        center[0] + axes[0][0] * local[0] + axes[1][0] * local[1] + axes[2][0] * local[2],
        center[1] + axes[0][1] * local[0] + axes[1][1] * local[1] + axes[2][1] * local[2],
        center[2] + axes[0][2] * local[0] + axes[1][2] * local[1] + axes[2][2] * local[2],
    ];
    if p.iter().all(|v| v.is_finite()) && p.iter().all(|v| v.abs() <= 1e12) {
        out.push(Point3D::new(p[0], p[1], p[2]));
        true
    } else {
        false
    }
}

/// Anillo en el plano local `plane` (0: YZ, 1: XZ, 2: XY) con `fixed` en el
/// eje restante; `radius_fn(theta)` da las dos coordenadas del plano.
fn ring(
    shape: &QuadricShape,
    radius_fn: impl Fn(f64) -> Option<[f64; 2]>,
    steps: usize,
    fixed: [f64; 3],
    plane: usize,
) -> Vec<Point3D> {
    let mut pts = Vec::new();
    for i in 0..=steps {
        let theta = 2.0 * core::f64::consts::PI * i as f64 / steps as f64;
        let Some(uv) = radius_fn(theta) else { break };
        let mut local = fixed;
        if plane == 0 {
            local[1] = uv[0];
            local[2] = uv[1];
        } else if plane == 1 {
            local[0] = uv[0];
            local[2] = uv[1];
        } else {
            local[0] = uv[0];
            local[1] = uv[1];
        }
        if !push_point(&mut pts, shape.center, shape.axes, local) {
            break;
        }
    }
    pts
}

fn wire_segments(polylines: &[Vec<Point3D>]) -> usize {
    polylines.iter().map(|p| p.len().saturating_sub(1)).sum()
}

/// Malla wireframe exacta de una forma clasificada.
///
/// Cada polilínea son puntos consecutivos a unir con segmentos. Respeta el
/// presupuesto ([`QUADRIC_MAX_WIRE_SEGMENTS`]) y falla honesto si el tipo no
/// tiene superficie real.
pub fn quadric_wire_points(shape: &QuadricShape) -> Result<Vec<Vec<Point3D>>, QuadricError> {
    if !shape.kind.has_real_locus() {
        let kind = shape.kind;
        return Err(QuadricError::NoRealLocus { kind });
    }
    let mut lines: Vec<Vec<Point3D>> = Vec::new();
    match shape.kind {
        QuadricKind::Sphere | QuadricKind::Ellipsoid => {
            let [rx, ry, rz] = shape.params;
            for (plane, ru, rv) in [(2, rx, ry), (1, rx, rz), (0, ry, rz)] {
                let mut pts = Vec::new();
                for i in 0..=32 {
                    let t = 2.0 * core::f64::consts::PI * f64::from(i) / 32.0;
                    let (cu, cv) = (ru * t.cos(), rv * t.sin());
                    let local = if plane == 2 {
                        [cu, cv, 0.0]
                    } else if plane == 1 {
                        [cu, 0.0, cv]
                    } else {
                        [0.0, cu, cv]
                    };
                    if !push_point(&mut pts, shape.center, shape.axes, local) {
                        break;
                    }
                }
                if pts.len() >= 2 {
                    lines.push(pts);
                }
            }
        }
        QuadricKind::HyperboloidOneSheet => {
            let [a, b, c] = shape.params;
            let h = (2.0 * c).min(QUADRIC_OPEN_HALF_EXTENT).max(c * 0.5);
            for z in [-h, 0.0, h] {
                let r = (1.0 + (z / c) * (z / c)).sqrt();
                let pts = ring(
                    shape,
                    |t| Some([a * r * t.cos(), b * r * t.sin()]),
                    QUADRIC_U_STEPS,
                    [0.0, 0.0, z],
                    2,
                );
                if pts.len() >= 2 {
                    lines.push(pts);
                }
            }
            for k in 0..4 {
                let t0 = core::f64::consts::PI * f64::from(k) / 4.0;
                let (ct, st) = (t0.cos(), t0.sin());
                let mut pts = Vec::new();
                for i in 0..=6 {
                    let z = -h + 2.0 * h * f64::from(i) / 6.0;
                    let r = (1.0 + (z / c) * (z / c)).sqrt();
                    if !push_point(
                        &mut pts,
                        shape.center,
                        shape.axes,
                        [a * r * ct, b * r * st, z],
                    ) {
                        break;
                    }
                }
                if pts.len() >= 2 {
                    lines.push(pts);
                }
            }
        }
        QuadricKind::HyperboloidTwoSheets => {
            let [a, b, c] = shape.params;
            for s in [-1.0, 1.0] {
                for zk in [1.0, 2.0] {
                    let z = s * c * zk;
                    if z.abs() > QUADRIC_OPEN_HALF_EXTENT {
                        continue;
                    }
                    let r = ((z / c) * (z / c) - 1.0).max(0.0).sqrt();
                    let pts = ring(
                        shape,
                        |t| Some([a * r * t.cos(), b * r * t.sin()]),
                        16,
                        [0.0, 0.0, z],
                        2,
                    );
                    if pts.len() >= 2 {
                        lines.push(pts);
                    }
                }
                for k in 0..4 {
                    let t0 = core::f64::consts::PI * f64::from(k) / 4.0;
                    let (ct, st) = (t0.cos(), t0.sin());
                    let mut pts = Vec::new();
                    for i in 0..=4 {
                        let w = 1.0 + f64::from(i) * 0.375;
                        let z = s * c * w;
                        if z.abs() > QUADRIC_OPEN_HALF_EXTENT {
                            break;
                        }
                        let r = (w * w - 1.0).max(0.0).sqrt();
                        if !push_point(
                            &mut pts,
                            shape.center,
                            shape.axes,
                            [a * r * ct, b * r * st, z],
                        ) {
                            break;
                        }
                    }
                    if pts.len() >= 2 {
                        lines.push(pts);
                    }
                }
            }
        }
        QuadricKind::EllipticParaboloid => {
            let [a, b, dir] = shape.params;
            for r in [0.5, 1.0, 1.5] {
                let pts = ring(
                    shape,
                    |t| Some([a * r * t.cos(), b * r * t.sin()]),
                    QUADRIC_U_STEPS,
                    [0.0, 0.0, dir * r * r],
                    2,
                );
                if pts.len() >= 2 {
                    lines.push(pts);
                }
            }
            for k in 0..4 {
                let t0 = core::f64::consts::PI * f64::from(k) / 4.0;
                let (ct, st) = (t0.cos(), t0.sin());
                let mut pts = Vec::new();
                for i in 0..=6 {
                    let r = f64::from(i) / 3.0;
                    if !push_point(
                        &mut pts,
                        shape.center,
                        shape.axes,
                        [a * r * ct, b * r * st, dir * r * r],
                    ) {
                        break;
                    }
                }
                if pts.len() >= 2 {
                    lines.push(pts);
                }
            }
        }
        QuadricKind::HyperbolicParaboloid => {
            // Doblemente reglada: `x = a(u+v)/2, y = b(v-u)/2, z = u·v`.
            let [a, b, _] = shape.params;
            for k in 0..6 {
                let fixed = -1.25 + 0.5 * f64::from(k);
                for swap in [false, true] {
                    let mut pts = Vec::new();
                    for i in 0..=8 {
                        let v = -1.5 + 3.0 * f64::from(i) / 8.0;
                        let (u, vv) = if swap { (v, fixed) } else { (fixed, v) };
                        let local = [a * (u + vv) / 2.0, b * (vv - u) / 2.0, u * vv];
                        if local.iter().any(|x| x.abs() > QUADRIC_OPEN_HALF_EXTENT) {
                            continue;
                        }
                        if !push_point(&mut pts, shape.center, shape.axes, local) {
                            break;
                        }
                    }
                    if pts.len() >= 2 {
                        lines.push(pts);
                    }
                }
            }
        }
        QuadricKind::Cone => {
            let [p, q, h] = shape.params;
            for z in [-h, h] {
                let pts = ring(
                    shape,
                    |t| Some([p * h * t.cos(), q * h * t.sin()]),
                    QUADRIC_U_STEPS,
                    [0.0, 0.0, z],
                    2,
                );
                if pts.len() >= 2 {
                    lines.push(pts);
                }
            }
            for k in 0..4 {
                let t0 = core::f64::consts::PI * f64::from(k) / 4.0;
                let (ct, st) = (t0.cos(), t0.sin());
                for s in [-1.0, 1.0] {
                    let mut pts = Vec::new();
                    for i in 0..=6 {
                        let w = s * h * f64::from(i) / 6.0;
                        if !push_point(
                            &mut pts,
                            shape.center,
                            shape.axes,
                            [p * w * ct, q * w * st, w],
                        ) {
                            break;
                        }
                    }
                    if pts.len() >= 2 && lines.len() < 12 {
                        lines.push(pts);
                    }
                }
            }
        }
        QuadricKind::EllipticCylinder => {
            let [a, b, h] = shape.params;
            for z in [-h, 0.0, h] {
                let pts = ring(
                    shape,
                    |t| Some([a * t.cos(), b * t.sin()]),
                    QUADRIC_U_STEPS,
                    [0.0, 0.0, z],
                    2,
                );
                if pts.len() >= 2 {
                    lines.push(pts);
                }
            }
            for k in 0..4 {
                let t0 = core::f64::consts::PI * f64::from(k) / 4.0;
                let (ct, st) = (t0.cos(), t0.sin());
                let mut pts = Vec::new();
                for i in 0..=6 {
                    let z = -h + 2.0 * h * f64::from(i) / 6.0;
                    if !push_point(&mut pts, shape.center, shape.axes, [a * ct, b * st, z]) {
                        break;
                    }
                }
                if pts.len() >= 2 {
                    lines.push(pts);
                }
            }
        }
        QuadricKind::HyperbolicCylinder => {
            let [a, b, h] = shape.params;
            for branch in [-1.0, 1.0] {
                for zk in [-h, 0.0, h] {
                    let mut pts = Vec::new();
                    for i in 0..=12 {
                        let t = -2.0 + 4.0 * f64::from(i) / 12.0;
                        let local = [branch * a * t.cosh(), b * t.sinh(), zk];
                        if local.iter().any(|x| x.abs() > QUADRIC_OPEN_HALF_EXTENT) {
                            continue;
                        }
                        if !push_point(&mut pts, shape.center, shape.axes, local) {
                            break;
                        }
                    }
                    if pts.len() >= 2 {
                        lines.push(pts);
                    }
                }
                for k in 0..2 {
                    let t = -1.0 + 2.0 * f64::from(k);
                    let mut pts = Vec::new();
                    for i in 0..=6 {
                        let z = -h + 2.0 * h * f64::from(i) / 6.0;
                        if !push_point(
                            &mut pts,
                            shape.center,
                            shape.axes,
                            [branch * a * t.cosh(), b * t.sinh(), z],
                        ) {
                            break;
                        }
                    }
                    if pts.len() >= 2 {
                        lines.push(pts);
                    }
                }
            }
        }
        QuadricKind::ParabolicCylinder => {
            let [lambda, lineal, k] = shape.params;
            let h = QUADRIC_OPEN_HALF_EXTENT;
            let qmax = ((lineal * h + k.abs()) / lambda.abs().max(1e-300))
                .sqrt()
                .min(h);
            if !(qmax.is_finite() && qmax > 0.0) {
                return Err(QuadricError::Unbounded);
            }
            for z in [-h, 0.0, h] {
                let mut pts = Vec::new();
                for i in 0..=20 {
                    let qq = -qmax + 2.0 * qmax * f64::from(i) / 20.0;
                    let s = -(lambda * qq * qq + k) / lineal;
                    if !s.is_finite() || s.abs() > h {
                        continue;
                    }
                    if !push_point(&mut pts, shape.center, shape.axes, [qq, s, z]) {
                        break;
                    }
                }
                if pts.len() >= 2 {
                    lines.push(pts);
                }
            }
            for kk in 0..6 {
                let qq = -qmax + 2.0 * qmax * f64::from(kk) / 5.0;
                let s = -(lambda * qq * qq + k) / lineal;
                if !s.is_finite() || s.abs() > h {
                    continue;
                }
                let mut pts = Vec::new();
                for i in 0..=6 {
                    let z = -h + 2.0 * h * f64::from(i) / 6.0;
                    if !push_point(&mut pts, shape.center, shape.axes, [qq, s, z]) {
                        break;
                    }
                }
                if pts.len() >= 2 {
                    lines.push(pts);
                }
            }
        }
        QuadricKind::IntersectingPlanes => {
            let [m, _, _] = shape.params;
            lines = plane_pair_grids(shape, m, QUADRIC_OPEN_HALF_EXTENT / 2.0)?;
        }
        QuadricKind::ParallelPlanes | QuadricKind::CoincidentPlane => {
            let offsets: Vec<f64> = if shape.kind == QuadricKind::ParallelPlanes {
                vec![-shape.params[0], shape.params[0]]
            } else {
                vec![0.0]
            };
            for off in offsets {
                grids_on_plane(&mut lines, shape, off, QUADRIC_OPEN_HALF_EXTENT / 2.0);
            }
        }
        QuadricKind::ImaginaryEllipsoid
        | QuadricKind::ImaginaryCone
        | QuadricKind::ImaginaryCylinder
        | QuadricKind::ImaginaryPlanes => {
            let kind = shape.kind;
            return Err(QuadricError::NoRealLocus { kind });
        }
    }
    // Presupuesto: recorta polilíneas enteras si hace falta (nunca parcial).
    while wire_segments(&lines) > QUADRIC_MAX_WIRE_SEGMENTS && lines.len() > 1 {
        lines.pop();
    }
    Ok(lines)
}

/// Rejillas de dos planos que se cortan en `e2`: direcciones `e0 ± m·e1`.
fn plane_pair_grids(
    shape: &QuadricShape,
    m: f64,
    h: f64,
) -> Result<Vec<Vec<Point3D>>, QuadricError> {
    let len = (1.0 + m * m).sqrt();
    if !(len.is_finite() && len > 0.0) {
        return Err(QuadricError::Unbounded);
    }
    let mut lines = Vec::new();
    for s in [1.0, -1.0] {
        let d = [1.0 / len, s * m / len];
        for kk in 0..3 {
            let off = -h + 2.0 * h * f64::from(kk) / 2.0;
            let mut along = Vec::new();
            for i in 0..=8 {
                let t = -h + 2.0 * h * f64::from(i) / 8.0;
                if !push_point(
                    &mut along,
                    shape.center,
                    shape.axes,
                    [d[0] * t, d[1] * t, off],
                ) {
                    break;
                }
            }
            if along.len() >= 2 {
                lines.push(along);
            }
            let mut axial = Vec::new();
            for i in 0..=8 {
                let t = -h + 2.0 * h * f64::from(i) / 8.0;
                if !push_point(
                    &mut axial,
                    shape.center,
                    shape.axes,
                    [d[0] * off, d[1] * off, t],
                ) {
                    break;
                }
            }
            if axial.len() >= 2 {
                lines.push(axial);
            }
        }
    }
    Ok(lines)
}

/// Rejilla 5×5 sobre el plano local `z = off`.
fn grids_on_plane(lines: &mut Vec<Vec<Point3D>>, shape: &QuadricShape, off: f64, h: f64) {
    for kk in 0..5 {
        let c = -h + 2.0 * h * f64::from(kk) / 4.0;
        for swap in [false, true] {
            let mut pts = Vec::new();
            for i in 0..=8 {
                let t = -h + 2.0 * h * f64::from(i) / 8.0;
                let local = if swap { [t, c, off] } else { [c, t, off] };
                if !push_point(&mut pts, shape.center, shape.axes, local) {
                    break;
                }
            }
            if pts.len() >= 2 {
                lines.push(pts);
            }
        }
    }
}

/// Permutación del marco propio a ejes del mundo, si es alineado a ejes.
///
/// Devuelve `perm` con `mundo[perm[i]] = params[i]`: cada eje propio debe
/// tener una componente dominante `|v| > 1 - 1e-6`. La esfera siempre
/// devuelve identidad (sus radios son iguales). `None` indica marco rotado:
/// el llamador debe usar la malla exacta rotada, jamás radios sin rotar.
pub fn quadric_axis_permutation(shape: &QuadricShape) -> Option<[usize; 3]> {
    if shape.kind == QuadricKind::Sphere {
        return Some([0, 1, 2]);
    }
    let mut perm = [0; 3];
    let mut used = [false; 3];
    for (i, axis) in shape.axes.iter().enumerate() {
        let mut best = 0;
        let mut best_abs = 0.0;
        for (k, v) in axis.iter().enumerate() {
            if v.abs() > best_abs {
                best_abs = v.abs();
                best = k;
            }
        }
        if !(best_abs.is_finite() && best_abs > 1.0 - 1e-6) {
            return None;
        }
        if used[best] {
            return None;
        }
        used[best] = true;
        perm[i] = best;
    }
    Some(perm)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn coeffs(a: f64, b: f64, c: f64, j: f64) -> [f64; 10] {
        [a, b, c, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, j]
    }

    #[test]
    fn sphere_unit_classifies() {
        let shape = classify_quadric(coeffs(1.0, 1.0, 1.0, -1.0)).expect("esfera");
        assert_eq!(shape.kind, QuadricKind::Sphere);
        assert!(shape.kind.has_real_locus());
        for r in shape.params {
            assert!((r - 1.0).abs() < 1e-9, "radio {r}");
        }
    }

    #[test]
    fn diagonal_ellipsoid_radii() {
        // `0.25x² + y² + z² - 0.5x - 0.75 = 0` → centro (1,0,0), radios {2,1,1}.
        let c = [0.25, 1.0, 1.0, 0.0, 0.0, 0.0, -0.5, 0.0, 0.0, -0.75];
        let shape = classify_quadric(c).expect("elipsoide");
        assert_eq!(shape.kind, QuadricKind::Ellipsoid);
        assert!((shape.center[0] - 1.0).abs() < 1e-9);
        let mut radii = shape.params;
        radii.sort_by(|x, y| x.total_cmp(y));
        assert!((radii[0] - 1.0).abs() < 1e-9);
        assert!((radii[1] - 1.0).abs() < 1e-9);
        assert!((radii[2] - 2.0).abs() < 1e-9);
        // Marco alineado a ejes: la permutación existe y cada vértice
        // `centro + eje·radio` cae en la superficie (residuo ~0).
        let perm = quadric_axis_permutation(&shape).expect("alineado");
        for (axis, radius) in shape.axes.iter().zip(shape.params.iter()) {
            let mut p = shape.center;
            for (k, coord) in p.iter_mut().enumerate() {
                *coord += axis[k] * radius;
            }
            let residual = c[0] * p[0] * p[0]
                + c[1] * p[1] * p[1]
                + c[2] * p[2] * p[2]
                + c[6] * p[0]
                + c[7] * p[1]
                + c[8] * p[2]
                + c[9];
            assert!(residual.abs() < 1e-9, "vértice: residuo {residual}");
        }
        // El radio mayor (2) mapea al eje X del mundo.
        let mut world = [0.0; 3];
        for (i, p) in perm.iter().enumerate() {
            world[*p] = shape.params[i];
        }
        assert!((world[0] - 2.0).abs() < 1e-9);
    }

    #[test]
    fn one_sheet_acceptance_vector() {
        // `[1,1,-1,0.., -1]`: hiperboloide de una hoja.
        let shape = classify_quadric(coeffs(1.0, 1.0, -1.0, -1.0)).expect("una hoja");
        assert_eq!(shape.kind, QuadricKind::HyperboloidOneSheet);
        let mesh = quadric_wire_points(&shape).expect("malla una hoja");
        assert!(!mesh.is_empty());
        assert!(wire_segments(&mesh) <= QUADRIC_MAX_WIRE_SEGMENTS);
        assert!(mesh.len() > 3, "una hoja tiene anillos + perfiles");
        let all: Vec<Point3D> = mesh.iter().flatten().copied().collect();
        assert!(all.iter().any(|p| (p.z - shape.center[2]).abs() < 1e-9));
    }

    #[test]
    fn two_sheets_has_two_clusters() {
        let shape = classify_quadric(coeffs(1.0, 1.0, -1.0, 1.0)).expect("dos hojas");
        assert_eq!(shape.kind, QuadricKind::HyperboloidTwoSheets);
        let mesh = quadric_wire_points(&shape).expect("malla dos hojas");
        let all: Vec<Point3D> = mesh.iter().flatten().copied().collect();
        assert!(all.iter().any(|p| p.z > 0.5));
        assert!(all.iter().any(|p| p.z < -0.5));
        assert!(wire_segments(&mesh) <= QUADRIC_MAX_WIRE_SEGMENTS);
    }

    #[test]
    fn elliptic_paraboloid_opens() {
        // `x² + y² - z = 0`.
        let shape = classify_quadric([1.0, 1.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, -1.0, 0.0])
            .expect("paraboloide");
        assert_eq!(shape.kind, QuadricKind::EllipticParaboloid);
        let mesh = quadric_wire_points(&shape).expect("malla paraboloide");
        assert!(wire_segments(&mesh) <= QUADRIC_MAX_WIRE_SEGMENTS);
        let all: Vec<Point3D> = mesh.iter().flatten().copied().collect();
        assert!(all.iter().any(|p| p.z > 0.25));
    }

    #[test]
    fn hyperbolic_paraboloid_is_ruled() {
        // `x² - y² - z = 0`: silla.
        let shape =
            classify_quadric([1.0, -1.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, -1.0, 0.0]).expect("silla");
        assert_eq!(shape.kind, QuadricKind::HyperbolicParaboloid);
        let mesh = quadric_wire_points(&shape).expect("malla silla");
        assert!(mesh.len() >= 8, "doblemente reglada");
    }

    #[test]
    fn cone_has_apex_fan() {
        // `x² + y² - z² = 0`.
        let shape = classify_quadric(coeffs(1.0, 1.0, -1.0, 0.0)).expect("cono");
        assert_eq!(shape.kind, QuadricKind::Cone);
        let mesh = quadric_wire_points(&shape).expect("malla cono");
        let all: Vec<Point3D> = mesh.iter().flatten().copied().collect();
        assert!(
            all.iter().any(|p| {
                (p.x - shape.center[0]).abs() < 1e-9
                    && (p.y - shape.center[1]).abs() < 1e-9
                    && (p.z - shape.center[2]).abs() < 1e-9
            }),
            "abanico desde el ápice"
        );
    }

    #[test]
    fn elliptic_cylinder_extrudes() {
        // `x² + y² = 1`.
        let shape = classify_quadric(coeffs(1.0, 1.0, 0.0, -1.0)).expect("cilindro");
        assert_eq!(shape.kind, QuadricKind::EllipticCylinder);
        let mesh = quadric_wire_points(&shape).expect("malla cilindro");
        let all: Vec<Point3D> = mesh.iter().flatten().copied().collect();
        assert!(all.iter().any(|p| p.z > 1.0));
        assert!(all.iter().any(|p| p.z < -1.0));
    }

    #[test]
    fn hyperbolic_cylinder_two_branches() {
        // `x² - y² = 1`.
        let shape = classify_quadric([1.0, -1.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, -1.0])
            .expect("cilindro hip");
        assert_eq!(shape.kind, QuadricKind::HyperbolicCylinder);
        let mesh = quadric_wire_points(&shape).expect("malla");
        let all: Vec<Point3D> = mesh.iter().flatten().copied().collect();
        assert!(all.iter().any(|p| p.x > 1.0));
        assert!(all.iter().any(|p| p.x < -1.0));
    }

    #[test]
    fn parabolic_cylinder_opens() {
        // `x² - y = 0`.
        let shape = classify_quadric([1.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, -1.0, 0.0, 0.0])
            .expect("cilindro parab");
        assert_eq!(shape.kind, QuadricKind::ParabolicCylinder);
        assert!(quadric_wire_points(&shape).is_ok());
    }

    #[test]
    fn intersecting_planes_cross() {
        // `x² - y² = 0`: planos `x = ±y`.
        let shape =
            classify_quadric([1.0, -1.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0]).expect("planos");
        assert_eq!(shape.kind, QuadricKind::IntersectingPlanes);
        let mesh = quadric_wire_points(&shape).expect("malla planos");
        assert!(!mesh.is_empty());
    }

    #[test]
    fn parallel_planes_pair() {
        // `x² = 1`.
        let shape = classify_quadric([1.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, -1.0])
            .expect("paralelos");
        assert_eq!(shape.kind, QuadricKind::ParallelPlanes);
        let mesh = quadric_wire_points(&shape).expect("malla paralelos");
        let all: Vec<Point3D> = mesh.iter().flatten().copied().collect();
        assert!(all.iter().any(|p| (p.x - 1.0).abs() < 1e-6));
        assert!(all.iter().any(|p| (p.x + 1.0).abs() < 1e-6));
    }

    #[test]
    fn coincident_plane_single() {
        // `x² = 0`.
        let shape =
            classify_quadric([1.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0]).expect("plano");
        assert_eq!(shape.kind, QuadricKind::CoincidentPlane);
        assert!(quadric_wire_points(&shape).is_ok());
    }

    #[test]
    fn imaginary_is_honest_err() {
        // `x² + y² + z² = -1`: vacío.
        let err = classify_quadric(coeffs(1.0, 1.0, 1.0, 1.0)).expect_err("vacío");
        assert_eq!(
            err,
            QuadricError::NoRealLocus {
                kind: QuadricKind::ImaginaryEllipsoid
            }
        );
        // Punto `x² + y² + z² = 0`.
        let err = classify_quadric(coeffs(1.0, 1.0, 1.0, 0.0)).expect_err("punto");
        assert_eq!(
            err,
            QuadricError::NoRealLocus {
                kind: QuadricKind::ImaginaryCone
            }
        );
        // `x² = -1`: planos imaginarios.
        let err = classify_quadric([1.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 1.0])
            .expect_err("imaginario");
        assert_eq!(
            err,
            QuadricError::NoRealLocus {
                kind: QuadricKind::ImaginaryPlanes
            }
        );
        // `x² + y² = -1`: cilindro imaginario.
        let err = classify_quadric([1.0, 1.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 1.0])
            .expect_err("cilindro imag");
        assert_eq!(
            err,
            QuadricError::NoRealLocus {
                kind: QuadricKind::ImaginaryCylinder
            }
        );
    }

    #[test]
    fn non_finite_and_empty_err() {
        assert_eq!(
            classify_quadric([f64::NAN, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0])
                .expect_err("nan"),
            QuadricError::NonFinite
        );
        assert_eq!(
            classify_quadric([0.0; 10]).expect_err("cero"),
            QuadricError::Empty
        );
    }

    #[test]
    fn rotated_ellipsoid_keeps_radii() {
        // Término cruzado `d = -1`: rotación de 45° en el plano XY.
        let shape =
            classify_quadric([1.5, 1.5, 1.0, -1.0, 0.0, 0.0, 0.0, 0.0, 0.0, -1.0]).expect("rotado");
        assert_eq!(shape.kind, QuadricKind::Ellipsoid);
        // Autovalores 2, 1, 1 → radios `1/√2`, 1, 1 (descendente).
        assert!((shape.params[0] - 1.0 / 2.0_f64.sqrt()).abs() < 1e-6);
        // Marco rotado 45°: sin permutación a ejes (jamás radios sin rotar).
        assert_eq!(quadric_axis_permutation(&shape), None);
    }

    #[test]
    fn all_meshes_fit_budget() {
        let cases = [
            coeffs(1.0, 1.0, 1.0, -1.0),
            [0.25, 1.0, 1.0, 0.0, 0.0, 0.0, -0.5, 0.0, 0.0, -0.75],
            coeffs(1.0, 1.0, -1.0, -1.0),
            coeffs(1.0, 1.0, -1.0, 1.0),
            [1.0, 1.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, -1.0, 0.0],
            [1.0, -1.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, -1.0, 0.0],
            coeffs(1.0, 1.0, -1.0, 0.0),
            coeffs(1.0, 1.0, 0.0, -1.0),
            [1.0, -1.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, -1.0],
            [1.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, -1.0, 0.0, 0.0],
            [1.0, -1.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0],
            [1.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, -1.0],
            [1.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0],
        ];
        for c in cases {
            let shape = classify_quadric(c).expect("debe clasificar");
            let mesh = quadric_wire_points(&shape).expect("debe mallar");
            assert!(
                wire_segments(&mesh) <= QUADRIC_MAX_WIRE_SEGMENTS,
                "{:?} excede presupuesto",
                shape.kind
            );
        }
    }
}
