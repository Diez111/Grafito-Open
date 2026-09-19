use num_complex::Complex64;
use std::collections::HashMap;

use crate::math::complex_expr::ComplexExpr;
use crate::math::complex_opcode::{compile_complex_expr, exec_cpu, ComplexBytecodeProgram};

/// Tope duro de puntos del contorno (anti-DoS).
pub const MAX_CONTOUR_POINTS: usize = 10_000;

/// Tope de segmentos para la cuadratura compuesta: caminos más densos se
/// remuestrean en longitud de arco antes de integrar (16 evaluaciones por
/// segmento ⇒ 32k evaluaciones por recomputo con el tope).
pub const MAX_QUADRATURE_SEGMENTS: usize = 2_048;

/// Arcos por defecto de la cuadratura circular analítica (32 × GL16).
pub const CIRCLE_QUADRATURE_ARCS: usize = 32;

/// Gauss–Legendre de 16 puntos en `[-1, 1]`: `(peso, abscisa)`.
///
/// Integra polinomios de grado ≤ 31 de forma exacta. Es el mismo esquema que
/// usan los integradores de contorno interactivos clásicos; en tramos rectos
/// de una polilínea el integrando es `f(z(t))·z'` con `z'` constante, así que
/// la exactitud por segmento es alta incluso con trazos a mano alzada
/// gruesos (donde la regla del trapecio necesitaba cientos de puntos).
const GAUSS_LEGENDRE_16: [(f64, f64); 16] = [
    (0.0271524594117541, -0.9894009349916499),
    (0.0622535239386479, -0.9445750230732326),
    (0.0951585116824928, -0.8656312023878318),
    (0.1246289712555339, -0.755404408355003),
    (0.1495959888165767, -0.6178762444026438),
    (0.1691565193950025, -0.4580167776572274),
    (0.1826034150449236, -0.2816035507792589),
    (0.1894506104550685, -0.0950125098376374),
    (0.1894506104550685, 0.0950125098376374),
    (0.1826034150449236, 0.2816035507792589),
    (0.1691565193950025, 0.4580167776572274),
    (0.1495959888165767, 0.6178762444026438),
    (0.1246289712555339, 0.755404408355003),
    (0.0951585116824928, 0.8656312023878318),
    (0.0622535239386479, 0.9445750230732326),
    (0.0271524594117541, 0.9894009349916499),
];

#[inline]
fn is_finite(z: Complex64) -> bool {
    z.re.is_finite() && z.im.is_finite()
}

/// Suma compensada de Kahan para `Complex64` (componente a componente):
/// frena la pérdida de dígitos al acumular miles de contribuciones.
#[inline]
fn kahan_add(sum: &mut Complex64, compensation: &mut Complex64, term: Complex64) {
    let y = term - *compensation;
    let t = *sum + y;
    *compensation = (t - *sum) - y;
    *sum = t;
}

/// Evaluador de `f(z)` con fast path de bytecode.
///
/// Compila la expresión UNA vez (`compile_complex_expr` + `exec_cpu`, sin
/// `HashMap` ni `String` por punto) cuando todas las variables del documento
/// son reales — el caso normal. Con variables complejas, o si la expresión
/// usa nodos que el compilador no soporta (`deriv_z`, …), cae al walk
/// `ComplexExpr::eval` con semántica idéntica.
struct ComplexEvaluator<'a> {
    expr: &'a ComplexExpr,
    symbol: &'a str,
    program: Option<ComplexBytecodeProgram>,
    scratch: HashMap<String, Complex64>,
}

impl<'a> ComplexEvaluator<'a> {
    fn new(expr: &'a ComplexExpr, symbol: &'a str, vars: &HashMap<String, Complex64>) -> Self {
        let all_real = vars.values().all(|v| v.im == 0.0);
        let program = if all_real {
            let real_vars: std::collections::BTreeMap<String, f64> =
                vars.iter().map(|(k, v)| (k.clone(), v.re)).collect();
            let mut prog = ComplexBytecodeProgram::default();
            compile_complex_expr(expr, &real_vars, &[(symbol, 0)], &mut prog)
                .ok()
                .map(|()| prog)
        } else {
            None
        };
        Self {
            expr,
            symbol,
            program,
            scratch: vars.clone(),
        }
    }

    fn eval(&mut self, z: Complex64) -> Result<Complex64, String> {
        if let Some(program) = &self.program {
            if let Some(value) = exec_cpu(program, &[z]) {
                return Ok(value);
            }
        }
        self.scratch.insert(self.symbol.to_string(), z);
        self.expr.eval(&self.scratch)
    }
}

/// Integra `f(z) dz` sobre el segmento recto `[z0, z1]` con Gauss–Legendre
/// de 16 puntos (reparametrizado a `[-1, 1]`), con suma compensada.
/// Público: lo usan el acumulador incremental de la app y los tests.
pub fn integrate_segment(
    expr: &ComplexExpr,
    z0: Complex64,
    z1: Complex64,
    vars: &HashMap<String, Complex64>,
    symbol: &str,
) -> Result<Complex64, String> {
    let mut evaluator = ComplexEvaluator::new(expr, symbol, vars);
    integrate_segment_with(&mut evaluator, z0, z1)
}

fn integrate_segment_with(
    evaluator: &mut ComplexEvaluator<'_>,
    z0: Complex64,
    z1: Complex64,
) -> Result<Complex64, String> {
    if !is_finite(z0) || !is_finite(z1) {
        return Err(format!("non-finite segment endpoints: {z0} → {z1}"));
    }
    // Los nodos de Gauss viven en el interior: evaluamos también los extremos
    // para conservar la detección de polos en vértices del contorno (un polo
    // en el interior del segmento sigue siendo indetectable sin localizar
    // polos — el domain coloring los muestra).
    for endpoint in [z0, z1] {
        let value = evaluator.eval(endpoint)?;
        if !is_finite(value) {
            return Err(format!("non-finite value at contour point: {value}"));
        }
    }
    let mid = (z0 + z1) * 0.5;
    let half = (z1 - z0) * 0.5;
    let mut sum = Complex64::new(0.0, 0.0);
    let mut compensation = sum;
    for (weight, abscissa) in GAUSS_LEGENDRE_16 {
        let z = mid + half * abscissa;
        let value = evaluator.eval(z)?;
        if !is_finite(value) {
            return Err(format!("non-finite value at quadrature node: {value}"));
        }
        let term = value * (half * weight);
        if !is_finite(term) {
            return Err("non-finite quadrature contribution".to_string());
        }
        kahan_add(&mut sum, &mut compensation, term);
    }
    if !is_finite(sum) {
        return Err("non-finite segment integral".to_string());
    }
    Ok(sum)
}

/// Remuestrea una polilínea a lo sumo `max_segments` segmentos, equiespaciados
/// en longitud de arco. Conserva primero y último exactos. Con `max_segments
/// == 0` o caminos ya cortos devuelve una copia sin tocar.
pub fn resample_path(path: &[Complex64], max_segments: usize) -> Vec<Complex64> {
    let segments = path.len().saturating_sub(1);
    if max_segments == 0 || segments <= max_segments {
        return path.to_vec();
    }
    let mut cumulative = Vec::with_capacity(path.len());
    cumulative.push(0.0);
    let mut total = 0.0;
    for window in path.windows(2) {
        total += (window[1] - window[0]).norm();
        cumulative.push(total);
    }
    let Some(first) = path.first() else {
        return Vec::new();
    };
    let Some(last) = path.last() else {
        return Vec::new();
    };
    if !total.is_finite() || total <= 0.0 {
        return vec![*first, *last];
    }
    let mut out = Vec::with_capacity(max_segments + 1);
    out.push(*first);
    let mut index = 1usize;
    for k in 1..max_segments {
        let target = total * (k as f64 / max_segments as f64);
        while index < segments && cumulative[index] < target {
            index += 1;
        }
        let s0 = cumulative[index - 1];
        let s1 = cumulative[index];
        let t = if (s1 - s0).abs() > f64::EPSILON {
            ((target - s0) / (s1 - s0)).clamp(0.0, 1.0)
        } else {
            0.0
        };
        out.push(path[index - 1] + (path[index] - path[index - 1]) * t);
    }
    out.push(*last);
    out
}

/// Integra `f(z) dz` sobre una polilínea (contorno) con cuadratura compuesta
/// de Gauss–Legendre por segmento.
///
/// Retorna error si algún `f(z)` no es finito (polo/NaN sobre el contorno),
/// si el contorno excede `MAX_CONTOUR_POINTS` (presupuesto anti-DoS) o si la
/// suma desborda. Los caminos con más de `MAX_QUADRATURE_SEGMENTS` segmentos
/// se remuestrean en longitud de arco antes de integrar.
pub fn contour_integral(
    expr: &ComplexExpr,
    path: &[Complex64],
    vars: &HashMap<String, Complex64>,
    symbol: &str,
) -> Result<Complex64, String> {
    if path.len() < 2 {
        return Ok(Complex64::new(0.0, 0.0));
    }
    if path.len() > MAX_CONTOUR_POINTS {
        return Err(format!(
            "contour too large: {} > {MAX_CONTOUR_POINTS}",
            path.len()
        ));
    }
    let resampled;
    let path: &[Complex64] = if path.len() - 1 > MAX_QUADRATURE_SEGMENTS {
        resampled = resample_path(path, MAX_QUADRATURE_SEGMENTS);
        &resampled
    } else {
        path
    };

    let mut evaluator = ComplexEvaluator::new(expr, symbol, vars);
    let mut sum = Complex64::new(0.0, 0.0);
    let mut compensation = sum;
    for index in 0..path.len() - 1 {
        let segment = integrate_segment_with(&mut evaluator, path[index], path[index + 1])
            .map_err(|error| format!("segment {index}: {error}"))?;
        kahan_add(&mut sum, &mut compensation, segment);
        if !is_finite(sum) {
            return Err(format!("integral overflow at segment {index}"));
        }
    }
    Ok(sum)
}

/// Acumulador incremental de `∮ f(z) dz` para el contorno dibujado a mano:
/// compila la expresión una vez y por punto nuevo integra SOLO el segmento
/// agregado (16 evaluaciones), sin re-evaluar el camino completo.
pub struct ContourAccumulator {
    expression: ComplexExpr,
    symbol: String,
    vars: HashMap<String, Complex64>,
    program: Option<ComplexBytecodeProgram>,
    sum: Complex64,
    compensation: Complex64,
    last: Option<Complex64>,
    segments: usize,
}

impl ContourAccumulator {
    /// Crea el acumulador con la expresión ya parseada y el entorno actual.
    /// `symbol` es el símbolo base (típicamente `z`).
    pub fn new(expression: ComplexExpr, symbol: &str, vars: HashMap<String, Complex64>) -> Self {
        let all_real = vars.values().all(|v| v.im == 0.0);
        let program = if all_real {
            let real_vars: std::collections::BTreeMap<String, f64> =
                vars.iter().map(|(k, v)| (k.clone(), v.re)).collect();
            let mut prog = ComplexBytecodeProgram::default();
            compile_complex_expr(&expression, &real_vars, &[(symbol, 0)], &mut prog)
                .ok()
                .map(|()| prog)
        } else {
            None
        };
        Self {
            expression,
            symbol: symbol.to_string(),
            vars,
            program,
            sum: Complex64::new(0.0, 0.0),
            compensation: Complex64::new(0.0, 0.0),
            last: None,
            segments: 0,
        }
    }

    /// Agrega un punto al contorno. El primer punto solo fija el inicio; los
    /// siguientes integran el segmento nuevo.
    pub fn push(&mut self, z: Complex64) -> Result<(), String> {
        let Some(previous) = self.last else {
            self.last = Some(z);
            return Ok(());
        };
        if previous == z {
            return Ok(());
        }
        if self.segments >= MAX_CONTOUR_POINTS {
            return Err(format!(
                "contour too large: {} segments > {MAX_CONTOUR_POINTS}",
                self.segments
            ));
        }
        let segment = match &self.program {
            Some(program) => Self::integrate_with_program(program, previous, z)?,
            None => integrate_segment(&self.expression, previous, z, &self.vars, &self.symbol)?,
        };
        kahan_add(&mut self.sum, &mut self.compensation, segment);
        self.last = Some(z);
        self.segments += 1;
        Ok(())
    }

    fn integrate_with_program(
        program: &ComplexBytecodeProgram,
        z0: Complex64,
        z1: Complex64,
    ) -> Result<Complex64, String> {
        // Extremos primero: mantiene la detección de polos en vértices
        // (los nodos de Gauss son interiores).
        for endpoint in [z0, z1] {
            let value = exec_cpu(program, &[endpoint])
                .ok_or_else(|| "quadrature evaluation failed".to_string())?;
            if !is_finite(value) {
                return Err(format!("non-finite value at contour point: {value}"));
            }
        }
        let mid = (z0 + z1) * 0.5;
        let half = (z1 - z0) * 0.5;
        let mut sum = Complex64::new(0.0, 0.0);
        let mut compensation = sum;
        for (weight, abscissa) in GAUSS_LEGENDRE_16 {
            let z = mid + half * abscissa;
            let value = exec_cpu(program, &[z])
                .ok_or_else(|| "quadrature evaluation failed".to_string())?;
            if !is_finite(value) {
                return Err(format!("non-finite value at quadrature node: {value}"));
            }
            let term = value * (half * weight);
            if !is_finite(term) {
                return Err("non-finite quadrature contribution".to_string());
            }
            kahan_add(&mut sum, &mut compensation, term);
        }
        if !is_finite(sum) {
            return Err("non-finite segment integral".to_string());
        }
        Ok(sum)
    }

    /// Integral acumulada hasta el momento.
    pub fn value(&self) -> Complex64 {
        self.sum
    }

    /// Cantidad de segmentos integrados.
    pub fn segments(&self) -> usize {
        self.segments
    }

    /// Último punto incorporado (si hay alguno).
    pub fn last(&self) -> Option<Complex64> {
        self.last
    }

    /// Reinicia el acumulador conservando expresión y programa compilado.
    pub fn reset(&mut self) {
        self.sum = Complex64::new(0.0, 0.0);
        self.compensation = Complex64::new(0.0, 0.0);
        self.last = None;
        self.segments = 0;
    }
}

/// Factor de Cauchy: `ΣRes = 1/(2πi)·∮ f dz = −i/(2π)·∮ f dz`.
pub fn residues_from_contour_integral(integral: Complex64) -> Complex64 {
    let inv_2pi_i = Complex64::new(0.0, -1.0 / (2.0 * std::f64::consts::PI));
    integral * inv_2pi_i
}

/// Formato de display único para un valor complejo: `a + bi` / `a - bi` con
/// tres decimales; con parte imaginaria despreciable muestra solo la real
/// (evita el feo `+ -0.000i`). Puro y compartido por render y la app para que
/// el chip en vivo y la etiqueta persistida digan exactamente lo mismo.
pub fn format_complex_rounded(z: Complex64) -> String {
    if !z.re.is_finite() || !z.im.is_finite() {
        return "no finito".to_string();
    }
    if z.im.abs() < 0.0005 {
        return format!("{:.3}", z.re);
    }
    if z.im < 0.0 {
        format!("{:.3} - {:.3}i", z.re, z.im.abs())
    } else {
        format!("{:.3} + {:.3}i", z.re, z.im)
    }
}

/// Detects the sum of residues (and poles) enclosed by a closed contour.
/// By Cauchy's Residue Theorem: \oint_C f(z) dz = 2 * pi * i * Sum(Residues)
pub fn sum_of_residues(
    expr: &ComplexExpr,
    closed_path: &[Complex64],
    vars: &HashMap<String, Complex64>,
    symbol: &str,
) -> Result<Complex64, String> {
    let integral = contour_integral(expr, closed_path, vars, symbol)?;
    Ok(residues_from_contour_integral(integral))
}

/// Integral de contorno analítica sobre la circunferencia `|z − center| = radius`:
/// `∮ f(z) dz = ∫₀^{2π} f(γ(θ))·γ′(θ) dθ` con `γ(θ) = center + r·e^{iθ}` y
/// `γ′ = i·r·e^{iθ}`, partida en `arcs` paneles con GL16 cada uno.
///
/// Más precisa y barata que muestrear la circunferencia como polígono, y es
/// la que usa el modo círculo del contorno dibujado y el render de `Circle`.
pub fn circle_contour_integral(
    expr: &ComplexExpr,
    center: Complex64,
    radius: f64,
    arcs: usize,
    vars: &HashMap<String, Complex64>,
    symbol: &str,
) -> Result<Complex64, String> {
    if !is_finite(center) || !radius.is_finite() {
        return Err(format!("non-finite circle: center {center}, r = {radius}"));
    }
    if radius <= 0.0 {
        return Ok(Complex64::new(0.0, 0.0));
    }
    let arcs = arcs.clamp(1, 256);
    let step = std::f64::consts::TAU / arcs as f64;
    let mut evaluator = ComplexEvaluator::new(expr, symbol, vars);
    let mut sum = Complex64::new(0.0, 0.0);
    let mut compensation = sum;
    for arc in 0..arcs {
        let theta_0 = arc as f64 * step;
        // Frontera de arco primero: polos justo sobre la circunferencia en
        // múltiplos del paso no se escapan entre nodos interiores.
        let boundary = Complex64::new(theta_0.cos(), theta_0.sin());
        let boundary_value = evaluator.eval(center + boundary * radius)?;
        if !is_finite(boundary_value) {
            return Err(format!("non-finite value on the circle: {boundary_value}"));
        }
        let mid = theta_0 + step * 0.5;
        let half = step * 0.5;
        for (weight, abscissa) in GAUSS_LEGENDRE_16 {
            let theta = mid + half * abscissa;
            let rotation = Complex64::new(theta.cos(), theta.sin());
            let z = center + rotation * radius;
            let value = evaluator.eval(z)?;
            if !is_finite(value) {
                return Err(format!("non-finite value at quadrature node: {value}"));
            }
            // γ′(θ) = i·r·e^{iθ}
            let dz = Complex64::new(0.0, 1.0) * rotation * (radius * half * weight);
            let term = value * dz;
            if !is_finite(term) {
                return Err("non-finite quadrature contribution".to_string());
            }
            kahan_add(&mut sum, &mut compensation, term);
        }
    }
    if !is_finite(sum) {
        return Err("non-finite circle integral".to_string());
    }
    Ok(sum)
}

/// Convierte un complejo `f(z)` en un campo vectorial 2D (Flow).
/// For a complex function f(z) = u(x,y) + i v(x,y), the flow can be interpreted as
/// the vector field F(x,y) = (u(x,y), v(x,y)).
/// Another common interpretation (conjugate flow) is F(x,y) = (u(x,y), -v(x,y)).
/// Here we return the standard velocity vector (u, v).
pub fn evaluate_flow(
    expr: &ComplexExpr,
    x: f64,
    y: f64,
    vars: &HashMap<String, Complex64>,
    symbol: &str,
) -> Result<(f64, f64), String> {
    if !x.is_finite() || !y.is_finite() {
        return Err("non-finite flow coordinates".to_string());
    }
    let mut local_vars = vars.clone();
    local_vars.insert(symbol.to_string(), Complex64::new(x, y));
    let result = expr.eval(&local_vars)?;
    if !result.re.is_finite() || !result.im.is_finite() {
        return Err(format!("non-finite flow result: {result}"));
    }
    Ok((result.re, result.im))
}

#[cfg(test)]
mod coverage_sweep_calculus {
    use super::*;
    use std::collections::HashMap;

    fn expr(source: &str) -> ComplexExpr {
        crate::math::complex_expr::parse(source).expect("parse")
    }

    fn circle(n: usize) -> Vec<Complex64> {
        (0..=n)
            .map(|i| {
                let t = 2.0 * std::f64::consts::PI * i as f64 / n as f64;
                Complex64::new(t.cos(), t.sin())
            })
            .collect()
    }

    fn regular_polygon(sides: usize) -> Vec<Complex64> {
        let mut path = circle(sides);
        if let Some(first) = path.first().copied() {
            path.push(first);
        }
        path
    }

    #[test]
    fn barrido_contour_cauchy_pinnea_2pi() {
        let vars = HashMap::new();
        // ∮ z dz sobre círculo cerrado = 0 (campo conservativo).
        let e = expr("z");
        let r = contour_integral(&e, &circle(64), &vars, "z").expect("integral");
        assert!(r.norm() < 1e-9, "cerrada de z es 0, fue {r}");
        // ∮ 1/z dz = 2πi → residuos = 1.
        let inv = expr("1/z");
        let res = sum_of_residues(&inv, &circle(128), &vars, "z").expect("residuos");
        assert!(
            (res.re - 1.0).abs() < 1e-6 && res.im.abs() < 1e-6,
            "residuo 1, fue {res}"
        );
        // Camino corto / vacío honesto.
        assert_eq!(
            contour_integral(&e, &[], &vars, "z").expect("vacío"),
            Complex64::new(0.0, 0.0)
        );
        assert!(contour_integral(&e, &vec![Complex64::new(0.0, 0.0); 10_001], &vars, "z").is_err());
        assert!(contour_integral(
            &inv,
            &[Complex64::new(0.0, 0.0), Complex64::new(0.0, 0.0)],
            &vars,
            "z"
        )
        .is_err());
    }

    #[test]
    fn gl16_es_exacto_en_polinomios_y_mejora_el_trapecio() {
        let vars = HashMap::new();
        // ∮ z^5 dz sobre el cuadrado unitario = 0 (holomorfa, lazo cerrado).
        let mut square = vec![
            Complex64::new(-1.0, -1.0),
            Complex64::new(1.0, -1.0),
            Complex64::new(1.0, 1.0),
            Complex64::new(-1.0, 1.0),
            Complex64::new(-1.0, -1.0),
        ];
        let z5 = expr("z^5");
        let closed = contour_integral(&z5, &square, &vars, "z").expect("z^5");
        assert!(closed.norm() < 1e-9, "z^5 cerrada, fue {closed}");
        // El tramo inferior (−1−i → 1−i) de ∫ z dz vale exactamente −2i:
        // GL16 integra el polinomio sin error de truncado.
        let identity = expr("z");
        square.truncate(2);
        let open = contour_integral(&identity, &square, &vars, "z").expect("tramo z");
        assert!(
            (open - Complex64::new(0.0, -2.0)).norm() < 1e-12,
            "∫ tramo z dz = −2i, fue {open}"
        );
        // Un octógono simple alrededor de 1/z: el error del trapecio con esa
        // densidad era ~2e-2; GL16 baja a 1e-6 o mejor.
        let inv = expr("1/z");
        let res = sum_of_residues(&inv, &regular_polygon(8), &vars, "z").expect("octógono");
        assert!(
            (res.re - 1.0).abs() < 1e-6 && res.im.abs() < 1e-6,
            "residuo 1 con 8 vértices, fue {res}"
        );
    }

    #[test]
    fn resample_conserva_extremos_y_tope() {
        let dense: Vec<Complex64> = (0..=9_000)
            .map(|i| Complex64::new(i as f64 / 9_000.0, 0.0))
            .collect();
        let reduced = resample_path(&dense, MAX_QUADRATURE_SEGMENTS);
        assert_eq!(reduced.len(), MAX_QUADRATURE_SEGMENTS + 1);
        assert_eq!(reduced[0], dense[0]);
        assert_eq!(
            *reduced.last().expect("último"),
            *dense.last().expect("denso")
        );
        // Equiespaciado en arco: el paso es 1/2048.
        let step = reduced[1].re;
        assert!((step - 1.0 / MAX_QUADRATURE_SEGMENTS as f64).abs() < 1e-9);
        // Camino corto: copia exacta.
        let short = vec![Complex64::new(0.0, 0.0), Complex64::new(1.0, 1.0)];
        assert_eq!(resample_path(&short, 8), short);
        // Sin longitud (todos iguales): extremos sin NaN.
        let degenerate = vec![Complex64::new(2.0, 3.0); 5];
        let out = resample_path(&degenerate, 3);
        assert_eq!(out.len(), 2);
        assert!(out.iter().all(|z| is_finite(*z)));
    }

    #[test]
    fn acumulador_incremental_coincide_con_la_integral_completa() {
        let vars = HashMap::new();
        let inv = expr("1/z");
        let path = circle(32);
        let full = contour_integral(&inv, &path, &vars, "z").expect("full");
        let mut accumulator = ContourAccumulator::new(inv.clone(), "z", vars.clone());
        for z in &path {
            accumulator.push(*z).expect("push");
        }
        let live = accumulator.value();
        assert_eq!(accumulator.segments(), path.len() - 1);
        assert!(
            (live - full).norm() < 1e-12,
            "acumulador {live} vs integral {full}"
        );
        // Reset conserva el programa y vuelve a cero.
        accumulator.reset();
        assert_eq!(accumulator.value(), Complex64::new(0.0, 0.0));
        assert_eq!(accumulator.segments(), 0);
        // Polo en un vértice del contorno: error honesto (no un valor inventado).
        let mut pole = ContourAccumulator::new(inv, "z", vars);
        pole.push(Complex64::new(0.0, 0.0)).expect("inicio");
        assert!(pole.push(Complex64::new(1.0, 0.0)).is_err());
    }

    #[test]
    fn circulo_analitico_pinnea_residuo_y_coincide_con_polilinea() {
        let vars = HashMap::new();
        let inv = expr("1/z");
        let integral = circle_contour_integral(
            &inv,
            Complex64::new(0.0, 0.0),
            2.0,
            CIRCLE_QUADRATURE_ARCS,
            &vars,
            "z",
        )
        .expect("círculo");
        let expected = Complex64::new(0.0, std::f64::consts::TAU);
        assert!(
            (integral - expected).norm() < 1e-9,
            "∮ 1/z con radio 2 = 2πi, fue {integral}"
        );
        // Radio 0: lazo degenerado ⇒ 0 honesto.
        assert_eq!(
            circle_contour_integral(&inv, Complex64::new(1.0, 1.0), 0.0, 32, &vars, "z")
                .expect("radio 0"),
            Complex64::new(0.0, 0.0)
        );
        // Polinomio: ∮ z dz = 0 (exacto por GL16).
        let z = expr("z");
        let zero =
            circle_contour_integral(&z, Complex64::new(0.0, 0.0), 1.0, 8, &vars, "z").expect("z");
        assert!(zero.norm() < 1e-12, "∮ z = 0, fue {zero}");
        // Variables reales y complejas caen al mismo resultado (fast path y walk).
        let mut complex_vars = HashMap::new();
        complex_vars.insert("a".to_string(), Complex64::new(2.0, 1.0));
        let with_var = circle_contour_integral(
            &expr("a/z"),
            Complex64::new(0.0, 0.0),
            1.0,
            32,
            &complex_vars,
            "z",
        )
        .expect("a/z");
        let expected = Complex64::new(0.0, std::f64::consts::TAU) * Complex64::new(2.0, 1.0);
        assert!((with_var - expected).norm() < 1e-9, "fue {with_var}");
    }

    #[test]
    fn formato_compartido_del_valor() {
        assert_eq!(
            format_complex_rounded(Complex64::new(1.5, 2.25)),
            "1.500 + 2.250i"
        );
        assert_eq!(
            format_complex_rounded(Complex64::new(0.0, -std::f64::consts::TAU)),
            "0.000 - 6.283i"
        );
        // Imaginaria despreciable: solo la parte real (sin `+ -0.000i`).
        assert_eq!(
            format_complex_rounded(Complex64::new(-2.0, 0.0001)),
            "-2.000"
        );
        assert_eq!(
            format_complex_rounded(Complex64::new(f64::NAN, 0.0)),
            "no finito"
        );
    }

    #[test]
    fn barrido_flow_pinnea_identidad() {
        let e = expr("z");
        let vars = HashMap::new();
        assert_eq!(
            evaluate_flow(&e, 3.0, -2.0, &vars, "z").expect("flujo"),
            (3.0, -2.0)
        );
        assert!(evaluate_flow(&e, f64::NAN, 0.0, &vars, "z").is_err());
        assert!(evaluate_flow(&e, 0.0, f64::INFINITY, &vars, "z").is_err());
    }
}
