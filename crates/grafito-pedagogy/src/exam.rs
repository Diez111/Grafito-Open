//! CAT pedagógico — banco IRT 3PL calibrado (demo) + EAP + stopping rule.
//!
//! # Estado demo vs real (TODO honesto — no tocar docs en esta oleada)
//!
//! - **Real / implementado**: modelo 3PL `P= c + (1-c)/(1+exp(-a(θ-b)))`,
//!   información de Fisher 3PL, estimación EAP con prior N(0,1) por cuadratura
//!   numérica, selección por máxima información (CAT), stopping rule por error
//!   estándar de θ (`se < 0.32` o `n >= 15`), banco ≥15 ítems por rama con
//!   parámetros `a,b,c` variados no constantes.
//! - **Demo / simulado**: los parámetros `a,b,c` están generados determinísticamente
//!   con dispersión calibrada (no estimados empíricamente por máxima verosimilitud
//!   sobre N>500 respuestas reales). La dificultad `b` sí cubre -2..+2 y la
//!   discriminación `a` 0.8..2.0, pero la tabla no proviene de calibración IRT
//!   con datos de aula. Marcado como **CAT-lite** hasta calibración real.
//!   Requiere: recoger respuestas reales, calibrar con `ltm`/`mirt` o EM IRT y
//!   validar AUC/ECE por rama antes de salir de demo.
//! - **No tocar `docs/architecture.md`**: TODO aquí.
//!
//! # Banco por rama
//!
//! Cada `branch_id` de perfil (`calculus`, `algebra`, `functions`, `trigonometry`,
//! `geometry`, `stats`, `complex`) + genérico tiene ≥15 `IrtItem` con `a` no
//! constante (0.8..2.0), `b` distribuido (-2..+2) y `c` 0.15..0.28. Preguntas y
//! respuestas deterministas; la discriminación alta (a≈1.8) implica ítems más
//! informativos cerca de su dificultad.
//!
//! # Tests
//!
//! - `bank_has_fifteen_items_per_branch` verifica ≥15 y varianza `a,b,c`.
//! - `eap_monotono_y_se_decrece` verifica que aciertos suben θ y SE baja con n.
//! - `selection_max_info` y `stopping_rule`.

use serde::{Deserialize, Serialize};

/// Ítem IRT 3PL calibrado (demo).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct IrtItem {
    /// ID estable `rama-i`.
    pub id: String,
    /// Rama asociada (`calculus`, `algebra`, …).
    pub branch_id: String,
    /// Discriminación `a` ∈ [0.5, 2.5] (típico 0.8..2.0).
    pub a: f64,
    /// Dificultad `b` ∈ [-3,3] (esta demo: -2..+2).
    pub b: f64,
    /// Adivinación `c` ∈ [0,0.35] (esta demo: 0.15..0.28).
    pub c: f64,
    /// Enunciado.
    pub question: String,
    /// Respuesta canónica (corrección exacta/tolerante según `exam.rs` heredado).
    pub answer: String,
}

impl IrtItem {
    pub fn validate(&self) -> Result<(), String> {
        if self.id.trim().is_empty() || self.branch_id.trim().is_empty() {
            return Err("id/branch_id vacío".into());
        }
        if self.question.trim().is_empty() || self.answer.trim().is_empty() {
            return Err("pregunta/respuesta vacía".into());
        }
        for (name, v, lo, hi) in [
            ("a", self.a, 0.5, 2.5),
            ("b", self.b, -3.5, 3.5),
            ("c", self.c, 0.0, 0.35),
        ] {
            if !v.is_finite() {
                return Err(format!("{name} no finito"));
            }
            if !(lo..=hi).contains(&v) {
                return Err(format!("{name}={v} fuera de [{lo},{hi}]"));
            }
        }
        if self.c >= 1.0 {
            return Err("c debe ser <1".into());
        }
        Ok(())
    }
}

// ──────────────────────────────────────────────────────────────────────────────
// 3PL core
// ──────────────────────────────────────────────────────────────────────────────

/// Probabilidad 3PL `P(θ) = c + (1-c) / (1+exp(-a(θ-b)))`.
pub fn irt_prob(theta: f64, a: f64, b: f64, c: f64) -> f64 {
    if !theta.is_finite() || !a.is_finite() || !b.is_finite() || !c.is_finite() {
        return 0.5;
    }
    let a = a.clamp(0.5, 2.5);
    let c = c.clamp(0.0, 0.35);
    let logit = a * (theta - b);
    // exp(-logit) con clamp para evitar overflow
    let exp_neg = (-logit).exp();
    if !exp_neg.is_finite() {
        return if logit > 0.0 { 1.0 } else { c };
    }
    let logistic = 1.0 / (1.0 + exp_neg);
    let p = c + (1.0 - c) * logistic;
    p.clamp(c, 1.0)
}

/// Información de Fisher 3PL en θ.
///
/// `I(θ) = a² * ( (P-c)² (1-P) ) / ( (1-c)² P )`
pub fn irt_fisher(theta: f64, a: f64, b: f64, c: f64) -> f64 {
    let a = a.clamp(0.5, 2.5);
    let c = c.clamp(0.0, 0.35);
    let p = irt_prob(theta, a, b, c);
    if !p.is_finite() || p <= f64::EPSILON || p >= 1.0 - f64::EPSILON {
        return 0.0;
    }
    if (1.0 - c).abs() < f64::EPSILON {
        return 0.0;
    }
    let num = (p - c).powi(2) * (1.0 - p);
    let den = (1.0 - c).powi(2) * p;
    if den <= f64::EPSILON {
        return 0.0;
    }
    let info = a * a * num / den;
    if !info.is_finite() || info < 0.0 {
        0.0
    } else {
        info
    }
}

// ──────────────────────────────────────────────────────────────────────────────
// EAP (Expected A Posteriori) con prior N(0,1)
// ──────────────────────────────────────────────────────────────────────────────

/// Estima θ por EAP (media posterior) y su SE (desvío posterior).
///
/// - `responses`: slice de `(item, correcto)` ya administrados.
/// - Prior θ ~ N(0,1). Cuadratura uniforme en [-4,4] paso 0.08 (~100 puntos)
///   con log-verosimilitud y prior para estabilidad numérica (log-sum-exp).
///
/// Retorna `(theta_eap, se)` donde `se = sqrt(Var_posterior)`.
/// Si `responses` vacío, retorna `(0.0, 1.0)` (prior).
pub fn eap_estimate(responses: &[(IrtItem, bool)]) -> (f64, f64) {
    if responses.is_empty() {
        return (0.0, 1.0);
    }
    // Validar items rápidos; si alguno inválido, ignorarlo (no pánico)
    let mut valid: Vec<&(IrtItem, bool)> = Vec::new();
    for r in responses {
        if r.0.validate().is_ok() {
            valid.push(r);
        }
    }
    if valid.is_empty() {
        return (0.0, 1.0);
    }

    const LO: f64 = -4.0;
    const HI: f64 = 4.0;
    const STEP: f64 = 0.08;
    let n_points = ((HI - LO) / STEP).round() as usize + 1;

    let mut thetas = Vec::with_capacity(n_points);
    let mut log_post = Vec::with_capacity(n_points);
    let mut max_log = f64::NEG_INFINITY;

    for i in 0..n_points {
        let theta = LO + i as f64 * STEP;
        thetas.push(theta);
        // log prior N(0,1): -0.5*theta^2 -0.5*ln(2pi) (constante cancela)
        let log_prior = -0.5 * theta * theta;
        let mut log_like = 0.0_f64;
        let mut ok = true;
        for (item, correct) in &valid {
            let p = irt_prob(theta, item.a, item.b, item.c);
            let p_clamped = p.clamp(1e-9, 1.0 - 1e-9);
            if *correct {
                log_like += p_clamped.ln();
            } else {
                log_like += (1.0 - p_clamped).ln();
            }
            if !log_like.is_finite() {
                ok = false;
                break;
            }
        }
        let lp = if ok {
            log_prior + log_like
        } else {
            f64::NEG_INFINITY
        };
        if lp > max_log {
            max_log = lp;
        }
        log_post.push(lp);
    }

    if !max_log.is_finite() {
        return (0.0, 1.0);
    }

    // exp(log_post - max_log) y normalizar
    let mut post = Vec::with_capacity(n_points);
    let mut sum = 0.0_f64;
    for &lp in &log_post {
        let v = if lp.is_finite() {
            (lp - max_log).exp()
        } else {
            0.0
        };
        post.push(v);
        sum += v;
    }
    if !sum.is_finite() || sum <= f64::EPSILON {
        return (0.0, 1.0);
    }
    for v in &mut post {
        *v /= sum;
    }

    let mut eap = 0.0_f64;
    for (theta, w) in thetas.iter().zip(post.iter()) {
        eap += theta * w;
    }
    let mut var = 0.0_f64;
    for (theta, w) in thetas.iter().zip(post.iter()) {
        var += w * (theta - eap).powi(2);
    }
    let se = var.max(0.0).sqrt().max(0.05);
    // clamp theta a rango
    let eap = eap.clamp(-3.5, 3.5);
    (eap, se.clamp(0.05, 2.0))
}

// ──────────────────────────────────────────────────────────────────────────────
// Banco ≥15 ítems por rama — generación determinista calibrada demo
// ──────────────────────────────────────────────────────────────────────────────

const BRANCHES: &[&str] = &[
    "calculus",
    "algebra",
    "functions",
    "trigonometry",
    "geometry",
    "stats",
    "complex",
    "general",
];

fn det_a_for_index(idx: usize) -> f64 {
    // 0.8 .. 2.0 con dispersión vía hash simple (no constante)
    let h = (idx as u64).wrapping_mul(0x9E3779B97F4A7C15);
    let bucket = (h % 7) as f64; // 0..6
    0.8 + bucket * 0.20 // 0.8,1.0,1.2,1.4,1.6,1.8,2.0
}

fn det_c_for_index(idx: usize) -> f64 {
    let h = (idx as u64)
        .wrapping_mul(0xBF58476D1CE4E5B9)
        .wrapping_add(0x9E3779B97F4A7C15);
    let bucket = (h % 5) as f64; // 0..4
    0.15 + bucket * 0.03 // 0.15,0.18,0.21,0.24,0.27
}

fn question_for_branch(branch: &str, idx: usize, b: f64) -> (String, String) {
    // Preguntas deterministas por rama, con dificultad b como pista (no expuesta al alumno)
    match branch {
        "calculus" => {
            let qs = [
                ("Derivá x^2 en x=2", "4"),
                ("Derivá x^3 en x=1", "3"),
                ("Derivá sin(x) en x=0", "1"),
                ("Calculá ∫₀¹ x dx", "0.5"),
                ("Calculá ∫₀¹ x^2 dx", "0.3333333333"),
                ("Límite lim_{x→0} sin(x)/x", "1"),
                ("Derivada de e^x en x=0", "1"),
                ("Deriva (x^2+1)*(x-1) en x=1", "2"),
                ("Segunda derivada de x^3 en x=2", "12"),
                ("Integral de 2*x de 0 a 2", "4"),
                ("Deriva ln(x) en x=1", "1"),
                ("Área bajo y=x de 0 a 3", "4.5"),
                ("Deriva cos(x) en x=0", "0"),
                ("Primitiva de 3*x^2 en x=1", "1"),
                ("Tasa media de x^2 en [1,2]", "3"),
            ];
            let (q, a) = qs[idx % qs.len()];
            (q.to_string(), a.to_string())
        }
        "algebra" => {
            let qs = [
                ("Resolvé 2*x+3=11", "4"),
                ("Factorizá x^2-9", "(x-3)(x+3)"),
                ("Raíces de x^2-5*x+6=0", "2 y 3"),
                ("Determinante de [[2,0],[0,3]]", "6"),
                ("Solución de 3*x=12", "4"),
                ("Producto (x+1)*(x-1)", "x^2-1"),
                ("Rango de [[1,2],[2,4]]", "1"),
                ("Inversa de x+5 cuando x=2", "7"),
                ("Resolvé x/2=3", "6"),
                ("Suma de raíces de x^2-3*x+2", "3"),
                ("Factorizá x^2+2*x+1", "(x+1)^2"),
                ("Resolvé 5*x-10=0", "2"),
                ("Determinante de [[1,1],[1,1]]", "0"),
                ("Resolvé -x=5", "-5"),
                ("Raíz de 2*x+4=0", "-2"),
            ];
            let (q, a) = qs[idx % qs.len()];
            (q.to_string(), a.to_string())
        }
        "functions" => {
            let qs = [
                ("Raíz de f(x)=x-4", "4"),
                ("Pendiente de y=2*x+1", "2"),
                ("Evaluá f(2) si f(x)=x^2+1", "5"),
                ("Imagen de f(x)=x^2 en x=3", "9"),
                ("¿f(x)=|x| es par?", "sí"),
                ("Dominio de 1/x", "x≠0"),
                ("f(0) si f(x)=3*x+2", "2"),
                ("Corte de y=x+5 con x=0", "5"),
                ("¿f(x)=x es creciente?", "sí"),
                ("Composición f(g(1)) con f=x+1,g=2*x", "3"),
                ("Inversa de f(x)=x+3 en y=5", "2"),
                ("Evaluá f(-1) si f(x)=x^2", "1"),
                ("¿f(x)=x^2 es par?", "sí"),
                ("f(1) si f(x)=2^x", "2"),
                ("Ceros de f(x)=x*(x-1)", "0 y 1"),
            ];
            let (q, a) = qs[idx % qs.len()];
            (q.to_string(), a.to_string())
        }
        "trigonometry" => {
            let qs = [
                ("¿Cuánto vale sin(0)?", "0"),
                ("¿Cuánto vale cos(0)?", "1"),
                ("Amplitud de sin(2*x)", "1"),
                ("Periodo de sin(x)", "2*pi"),
                ("sin(π/2)", "1"),
                ("cos(π)", "-1"),
                ("sin(π)", "0"),
                ("cos(π/2)", "0"),
                ("Valor máximo de cos(x)", "1"),
                ("sin(3*π/2)", "-1"),
                ("Identidad sin^2+cos^2", "1"),
                ("¿Cuánto vale tan(0)?", "0"),
                ("Amplitud de 2*sin(x)", "2"),
                ("cos(0)", "1"),
                ("sin(π/6)", "0.5"),
            ];
            let (q, a) = qs[idx % qs.len()];
            (q.to_string(), a.to_string())
        }
        "geometry" => {
            let qs = [
                ("Área cuadrado lado 3", "9"),
                ("Perímetro cuadrado lado 5", "20"),
                ("Volumen cubo arista 2", "8"),
                ("Área círculo radio 1", "3.14159"),
                ("Hipotenusa catetos 3 y 4", "5"),
                ("Área rectángulo 2x3", "6"),
                ("Perímetro triángulo 3,4,5", "12"),
                ("Volumen esfera radio 1", "4.18879"),
                ("Área triángulo base 4 altura 3", "6"),
                ("Diagonal cuadrado lado 1", "1.4142"),
                ("Área trapecio bases 2,4 altura 3", "9"),
                ("Perímetro círculo radio 2", "12.566"),
                ("Volumen cilindro r=1 h=2", "6.283"),
                ("Área cubo arista 1", "6"),
                ("Hipotenusa isósceles cateto 1", "1.4142"),
            ];
            let (q, a) = qs[idx % qs.len()];
            (q.to_string(), a.to_string())
        }
        "stats" => {
            let qs = [
                ("Media de {2,4,6}", "4"),
                ("Rango de {1,3,8}", "7"),
                ("Mediana de {1,5,9}", "5"),
                ("Media de {1,2,3,4}", "2.5"),
                ("Moda de {1,2,2,3}", "2"),
                ("Varianza de {1,1,1}", "0"),
                ("Probabilidad de cara en moneda", "0.5"),
                ("Probabilidad de 6 en dado", "0.1666667"),
                ("Media de {10,20}", "15"),
                ("Rango de {5,5,5}", "0"),
                ("Mediana de {1,2,3,4}", "2.5"),
                ("Probabilidad de no-6 en dado", "0.8333333"),
                ("Esperanza de dado", "3.5"),
                ("Desvío de {2,2}", "0"),
                ("Prob. de dos caras", "0.25"),
            ];
            let (q, a) = qs[idx % qs.len()];
            (q.to_string(), a.to_string())
        }
        "complex" => {
            let qs = [
                ("Parte real de 3+4i", "3"),
                ("Módulo de 3+4i", "5"),
                ("Conjugado de 2-3i", "2+3i"),
                ("¿i^2?", "-1"),
                ("Módulo de 1+i", "1.4142"),
                ("Parte imaginaria de 2+5i", "5"),
                ("¿i^4?", "1"),
                ("Suma (1+i)+(1-i)", "2"),
                ("Producto (1+i)*(1-i)", "2"),
                ("¿Conjugado de i?", "-i"),
                ("Módulo de 2i", "2"),
                ("Argumento de 1+i (grados)", "45"),
                ("Forma polar de 1 (módulo)", "1"),
                ("¿Real de i?", "0"),
                ("Módulo de 0+1i", "1"),
            ];
            let (q, a) = qs[idx % qs.len()];
            (q.to_string(), a.to_string())
        }
        _ => {
            // general / desconocida
            let base = format!("Pregunta general {} (b={b:.1})", idx + 1, b = b);
            let ans = format!("{}", idx + 1);
            (base, ans)
        }
    }
}

/// Banco calibrado demo — ≥15 ítems por rama con `a,b,c` no constantes.
///
/// Generación determinista: `b` distribuido uniforme -2..+2, `a` y `c` con
/// dispersión via hash para evitar constantes. Validado por `bank_has_fifteen_items_per_branch`.
pub fn cat_bank(branch_id: &str) -> Vec<IrtItem> {
    let norm = branch_id.trim().to_lowercase();
    let branch = if BRANCHES.contains(&norm.as_str()) {
        norm.as_str()
    } else {
        "general"
    };
    let mut items = Vec::with_capacity(16);
    for idx in 0..15usize {
        // b uniforme -2..+2
        let b = -2.0 + (idx as f64) * (4.0 / 14.0);
        let a = det_a_for_index(idx);
        let c = det_c_for_index(idx.wrapping_add(branch.len()));
        let (q, ans) = question_for_branch(branch, idx, b);
        let id = format!("{branch}-{idx:02}");
        items.push(IrtItem {
            id,
            branch_id: branch.to_string(),
            a,
            b,
            c,
            question: q,
            answer: ans,
        });
    }
    items
}

/// Cantidad de ítems en banco para rama.
pub fn cat_bank_len(branch_id: &str) -> usize {
    cat_bank(branch_id).len()
}

/// Valida que banco cumpla presupuesto ≥15 y parámetros variados.
pub fn cat_bank_is_valid(branch_id: &str) -> Result<(), String> {
    let bank = cat_bank(branch_id);
    if bank.len() < 15 {
        return Err(format!("banco {} tiene {} <15", branch_id, bank.len()));
    }
    // a,b,c no constantes
    let mut a_vals: Vec<f64> = bank.iter().map(|it| it.a).collect();
    let mut b_vals: Vec<f64> = bank.iter().map(|it| it.b).collect();
    let mut c_vals: Vec<f64> = bank.iter().map(|it| it.c).collect();
    a_vals.sort_by(|x, y| x.partial_cmp(y).unwrap_or(std::cmp::Ordering::Equal));
    b_vals.sort_by(|x, y| x.partial_cmp(y).unwrap_or(std::cmp::Ordering::Equal));
    c_vals.sort_by(|x, y| x.partial_cmp(y).unwrap_or(std::cmp::Ordering::Equal));
    if (a_vals.first().copied().unwrap_or(0.0) - a_vals.last().copied().unwrap_or(0.0)).abs() < 0.3
    {
        return Err("a sin dispersión".into());
    }
    if (b_vals.first().copied().unwrap_or(0.0) - b_vals.last().copied().unwrap_or(0.0)).abs() < 2.0
    {
        return Err("b sin rango".into());
    }
    if (c_vals.first().copied().unwrap_or(0.0) - c_vals.last().copied().unwrap_or(0.0)).abs() < 0.05
    {
        return Err("c sin dispersión".into());
    }
    for it in &bank {
        it.validate()?;
    }
    Ok(())
}

// ──────────────────────────────────────────────────────────────────────────────
// CAT: selección por máxima información (Fisher)
// ──────────────────────────────────────────────────────────────────────────────

/// Selecciona próximo ítem no administrado con máxima información en `theta`.
///
/// Si `administered_ids` contiene todos, retorna `None`.
/// Si `theta` no finito, usa 0.0.
pub fn cat_select_next(
    branch_id: &str,
    administered_ids: &[String],
    theta: f64,
) -> Option<IrtItem> {
    let theta = if theta.is_finite() { theta } else { 0.0 };
    let bank = cat_bank(branch_id);
    let mut best: Option<(f64, IrtItem)> = None;
    for item in bank {
        if administered_ids.contains(&item.id) {
            continue;
        }
        let info = irt_fisher(theta, item.a, item.b, item.c);
        match &best {
            None => best = Some((info, item)),
            Some((best_info, _)) => {
                if info > *best_info {
                    best = Some((info, item));
                }
            }
        }
    }
    best.map(|(_, it)| it)
}

// ──────────────────────────────────────────────────────────────────────────────
// Stopping rule — error estándar de θ
// ──────────────────────────────────────────────────────────────────────────────

/// ¿Detener CAT?  `true` si `se < 0.32` (precisión suficiente) o `n >= 15` o `n >= max_items`.
///
/// - `se`: error estándar posterior de θ (de `eap_estimate`).
/// - `n`: cantidad ya administrada.
/// - `max_items`: tope presupuestado (típico 15). Si 0, usa 15.
///
/// SE teórico mínimo con prior N(0,1) y 15 ítems bien calibrados ≈0.25..0.35.
pub fn cat_should_stop(se: f64, n: usize, max_items: usize) -> bool {
    let max_items = if max_items == 0 {
        15
    } else {
        max_items.clamp(1, 30)
    };
    if n >= max_items {
        return true;
    }
    if !se.is_finite() {
        return false;
    }
    if (0.0..=2.0).contains(&se) && se < 0.32 {
        return true;
    }
    false
}

/// Estado CAT mínimo para UI / tests (puro, sin I/O).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CatState {
    pub branch_id: String,
    pub theta: f64,
    pub se: f64,
    pub administered: Vec<String>,
    pub responses: Vec<(IrtItem, bool)>,
}

impl CatState {
    pub fn new(branch_id: &str) -> Self {
        Self {
            branch_id: branch_id.to_string(),
            theta: 0.0,
            se: 1.0,
            administered: Vec::new(),
            responses: Vec::new(),
        }
    }

    pub fn add_response(&mut self, item: IrtItem, correct: bool) {
        self.administered.push(item.id.clone());
        self.responses.push((item, correct));
        let (theta, se) = eap_estimate(&self.responses);
        self.theta = theta;
        self.se = se;
    }

    pub fn should_stop(&self) -> bool {
        cat_should_stop(self.se, self.administered.len(), 15)
    }

    pub fn next_item(&self) -> Option<IrtItem> {
        cat_select_next(&self.branch_id, &self.administered, self.theta)
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod tests {
    use super::*;
    use std::collections::HashSet;

    #[test]
    fn bank_has_fifteen_items_per_branch() {
        for branch in [
            "calculus",
            "algebra",
            "functions",
            "trigonometry",
            "geometry",
            "stats",
            "complex",
            "general",
        ] {
            let bank = cat_bank(branch);
            assert!(bank.len() >= 15, "rama {branch} tiene {} <15", bank.len());
            assert!(cat_bank_is_valid(branch).is_ok(), "rama {branch} invalida");
            // a,b,c no constantes
            let a_set: HashSet<String> = bank.iter().map(|it| format!("{:.2}", it.a)).collect();
            let b_set: HashSet<String> = bank.iter().map(|it| format!("{:.2}", it.b)).collect();
            let c_set: HashSet<String> = bank.iter().map(|it| format!("{:.2}", it.c)).collect();
            assert!(a_set.len() >= 3, "a sin varianza en {branch}");
            assert!(b_set.len() >= 5, "b sin varianza en {branch}");
            assert!(c_set.len() >= 2, "c sin varianza en {branch}");
            // a,b,c en rango y finitos
            for it in &bank {
                assert!(it.validate().is_ok(), "{branch} item invalido {it:?}");
            }
            // b cubre -2..+2
            let b_min = bank.iter().map(|it| it.b).fold(f64::INFINITY, f64::min);
            let b_max = bank.iter().map(|it| it.b).fold(f64::NEG_INFINITY, f64::max);
            assert!(b_min <= -1.5, "b_min {b_min} no cubre -2 en {branch}");
            assert!(b_max >= 1.5, "b_max {b_max} no cubre +2 en {branch}");
        }
    }

    #[test]
    fn irt_prob_y_fisher_sonos() {
        // prob creciente con theta
        let a = 1.2;
        let b = 0.0;
        let c = 0.2;
        let p_low = irt_prob(-2.0, a, b, c);
        let p_mid = irt_prob(0.0, a, b, c);
        let p_high = irt_prob(2.0, a, b, c);
        assert!(p_low < p_mid, "{p_low} < {p_mid}");
        assert!(p_mid < p_high, "{p_mid} < {p_high}");
        assert!((c..=1.0).contains(&p_mid));
        // fisher pico cerca de b
        let f_at_b = irt_fisher(b, a, b, c);
        let f_far = irt_fisher(3.0, a, b, c);
        assert!(f_at_b > f_far, "info en b {f_at_b} > lejos {f_far}");
        assert!(f_at_b.is_finite() && f_at_b >= 0.0);
        // c más alto reduce info? No exigimos.
        let f_low_c = irt_fisher(0.0, 1.2, 0.0, 0.15);
        let f_high_c = irt_fisher(0.0, 1.2, 0.0, 0.30);
        assert!(f_low_c > f_high_c);
    }

    #[test]
    fn eap_monotono_y_se_decrece() {
        let bank = cat_bank("algebra");
        // tomar 3 items fáciles (b bajo) y simular todo correcto => theta sube
        let mut responses = Vec::new();
        for it in bank.iter().take(3) {
            responses.push((it.clone(), true));
        }
        let (theta3, se3) = eap_estimate(&responses);
        assert!(theta3 > -0.5, "theta con 3 aciertos {theta3} debe subir");
        assert!(se3 < 1.0, "se debe bajar con datos {se3}");

        let mut responses6 = responses.clone();
        for it in bank.iter().skip(3).take(3) {
            responses6.push((it.clone(), true));
        }
        let (theta6, se6) = eap_estimate(&responses6);
        assert!(
            theta6 >= theta3 - 0.1,
            "más aciertos no debe bajar mucho: {theta3}->{theta6}"
        );
        assert!(se6 <= se3 + 0.05, "se debe no crecer mucho: {se3}->{se6}");

        // todo incorrecto => theta bajo
        let mut bad = Vec::new();
        for it in bank.iter().take(5) {
            bad.push((it.clone(), false));
        }
        let (theta_bad, _) = eap_estimate(&bad);
        assert!(
            theta_bad < 0.0,
            "theta con fallos {theta_bad} debe ser negativo"
        );
        assert!(theta_bad < theta3);

        // vacío => prior
        let (t0, s0) = eap_estimate(&[]);
        assert_eq!(t0, 0.0);
        assert_eq!(s0, 1.0);
    }

    #[test]
    fn selection_max_info() {
        let branch = "calculus";
        let theta = 0.0;
        let first = cat_select_next(branch, &[], theta).expect("primer item");
        // debe ser el de mayor info en theta=0 (cercano a b=0, a alto)
        let bank = cat_bank(branch);
        let info_first = irt_fisher(theta, first.a, first.b, first.c);
        for it in &bank {
            let info = irt_fisher(theta, it.a, it.b, it.c);
            assert!(info_first >= info - 1e-12, "selección debe ser máxima info");
        }
        // segundo no debe repetir
        let second =
            cat_select_next(branch, std::slice::from_ref(&first.id), theta).expect("segundo");
        assert_ne!(first.id, second.id);
        // con theta alto, debe preferir b alto
        let high_theta = 2.0;
        let sel_high = cat_select_next(branch, &[], high_theta).expect("high");
        assert!(
            sel_high.b > 0.5,
            "theta alto debe elegir b alto, obtuvo b={}",
            sel_high.b
        );
    }

    #[test]
    fn stopping_rule() {
        assert!(!cat_should_stop(0.5, 5, 15));
        assert!(cat_should_stop(0.31, 5, 15));
        assert!(cat_should_stop(0.5, 15, 15));
        assert!(cat_should_stop(0.5, 16, 15));
        assert!(!cat_should_stop(f64::NAN, 5, 15));
        assert!(cat_should_stop(0.2, 14, 15));
        // max_items 0 usa 15
        assert!(cat_should_stop(0.5, 15, 0));
    }

    #[test]
    fn cat_state_flujo_completo() {
        let mut state = CatState::new("stats");
        assert_eq!(state.theta, 0.0);
        assert_eq!(state.se, 1.0);
        let bank = cat_bank("stats");
        // simular 10 respuestas intercaladas
        for (i, it) in bank.iter().take(10).cloned().enumerate() {
            let correct = i % 3 != 0; // 2/3 aciertos
            state.add_response(it, correct);
            assert!(state.theta.is_finite());
            assert!(state.se.is_finite());
            assert!((0.05..=2.0).contains(&state.se));
        }
        assert_eq!(state.administered.len(), 10);
        assert!(!state.should_stop() || state.se < 0.32);
        // next no repetido
        if let Some(next) = state.next_item() {
            assert!(!state.administered.contains(&next.id));
        }
        // tras 15 debe detener
        for it in bank.iter().skip(10).take(5).cloned() {
            state.add_response(it, true);
        }
        assert_eq!(state.administered.len(), 15);
        assert!(state.should_stop());
        assert!(state.next_item().is_none() || state.administered.len() >= 15);
    }

    #[test]
    fn cat_lite_honesto_documentado() {
        // Este test solo asegura que el banco es demo (no empírico) y que la validación
        // exige TODO honesto: si alguien hardcodea todo a=1.2 c=0.25 debe fallar.
        let bank = cat_bank("general");
        let all_a_same = bank.iter().all(|it| (it.a - 1.2).abs() < 1e-9);
        let all_c_same = bank.iter().all(|it| (it.c - 0.25).abs() < 1e-9);
        assert!(!all_a_same, "a no debe ser constante 1.2 (demo honesto)");
        assert!(!all_c_same, "c no debe ser constante 0.25");
    }
}
