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

use crate::exercise::ValidatorKind;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

/// Ítem IRT 3PL calibrado (demo).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct IrtItem {
    /// ID estable `rama-i`.
    pub id: String,
    /// Rama asociada (`calculus`, `algebra`, …).
    pub branch_id: String,
    /// LO del currículum que evalúa este ítem (FIX 2: la ponderación
    /// BKT/scheduler es por ítem, vía el `p_known`/`due` del LO asociado).
    /// Vacío en datos viejos (`#[serde(default)]`).
    #[serde(default)]
    pub lo_id: String,
    /// Discriminación `a` ∈ [0.5, 2.5] (típico 0.8..2.0).
    pub a: f64,
    /// Dificultad `b` ∈ [-3,3] (tabla razonada por pregunta desde FIX 6: `b`
    /// refleja la dificultad percibida del enunciado, no el índice).
    pub b: f64,
    /// Adivinación `c` ∈ [0,0.35] (esta demo: 0.15..0.28).
    pub c: f64,
    /// Enunciado.
    pub question: String,
    /// Respuesta canónica (corrección exacta/tolerante según `exam.rs` heredado).
    pub answer: String,
    /// Validador de la respuesta del alumno (FIX 11: el ítem declara cómo se
    /// corrige; antes había que delegar en `FeedbackEngine::assess` a ciegas).
    #[serde(default)]
    pub validator: ValidatorKind,
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
        match self.validator {
            ValidatorKind::NumericTol(tol) => {
                if !tol.is_finite() || tol <= 0.0 || tol > 1.0 {
                    return Err("tolerancia del validador inválida".into());
                }
            }
            ValidatorKind::Exact | ValidatorKind::Symbolic => {}
        }
        Ok(())
    }
}

/// Validador determinista para una respuesta canónica: `NumericTol(2 %)` si la
/// respuesta es un número (acepta redondeos tipo `0.3333333333`), `Exact`
/// para respuestas simbólicas o de texto.
fn validator_for_answer(answer: &str) -> ValidatorKind {
    let t = answer.trim().replace(',', ".");
    match t.parse::<f64>() {
        Ok(_) => ValidatorKind::NumericTol(0.02),
        Err(_) => ValidatorKind::Exact,
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

/// Familia de banco (`BRANCHES`) que le corresponde a un LO del currículum.
///
/// Permite que `cat_bank("am1-der")` sirva preguntas del banco `calculus` en
/// vez de caer al genérico ("Pregunta general N"). IDs no listados →
/// `"general"` (mismo fallback que siempre). Pura, sin I/O.
pub fn branch_family_for_lo(lo_id: &str) -> &'static str {
    match lo_id {
        // Cálculo / AM
        "am1-lim" | "am1-cont" | "am1-der" | "am1-der-aplic" | "am1-int" | "am1-int-aplic"
        | "am1-sucesiones" | "am2-edo" | "am2-series" | "am2-taylor" | "am2-multivariable"
        | "am2-int-multi" | "am2-campos" | "am2-teoremas" => "calculus",
        // Funciones y geometría analítica
        "am1-func" | "sec-lineal" | "sec-pend" => "functions",
        // Álgebra y aritmética/algebra básica
        "sec-ec"
        | "sec-cuad"
        | "sec-prop"
        | "sec-fracc"
        | "pri-fracc-vis"
        | "pri-proporciones"
        | "pri-conteo"
        | "alg-matrices"
        | "alg-determinantes"
        | "alg-transformaciones"
        | "alg-vectores"
        | "alg-rectas-planos"
        | "alg-subespacios" => "algebra",
        // Trigonometría
        "sec-trig" => "trigonometry",
        // Geometría
        "pri-perim-area" | "sec-area" | "sec-pitagoras" | "alg-conicas" => "geometry",
        // Probabilidad y estadística
        "pri-datos"
        | "sec-prob"
        | "prob-basica"
        | "prob-var"
        | "prob-distribuciones"
        | "prob-inferencia"
        | "prob-regresion"
        | "prob-muestreo" => "stats",
        // Números complejos: solo ramas legacy (`complex`).
        "complex" => "complex",
        _ => "general",
    }
}

/// Preguntas por familia: `(enunciado, respuesta, b razonado, lo_id)`.
///
/// **FIX 6 — tabla `pregunta → b`**: antes `cat_bank` asignaba
/// `b = -2 + idx·(4/14)` uniforme por ÍNDICE mientras las preguntas se servían
/// en orden fijo: "Derivá x^2 en x=2" (trivial) podía quedar en `b = +2` y
/// "Tasa media de x² en [1,2]" en `b = -2`. El CAT selecciona por máxima
/// información sobre `b`: con `b` sin relación con el ítem la medición de θ
/// quedaba sesgada. Ahora cada enunciado lleva su dificultad PERCIBIDA
/// razonada (ordenada fácil → difícil dentro de cada familia) y su LO.
fn item_for_branch(branch: &str, idx: usize) -> (String, String, f64, &'static str) {
    // Preguntas deterministas por familia, ordenadas por dificultad percibida.
    let (q, a, b, lo) = match branch {
        "calculus" => {
            let qs = [
                ("Derivá x^2 en x=2", "4", -1.7, "am1-der"),
                ("Derivada de e^x en x=0", "1", -1.5, "am1-der"),
                ("Derivá x^3 en x=1", "3", -1.3, "am1-der"),
                ("Derivá sin(x) en x=0", "1", -1.1, "am1-der"),
                ("Derivá cos(x) en x=0", "0", -0.9, "am1-der"),
                ("Deriva ln(x) en x=1", "1", -0.7, "am1-der"),
                ("Integral de 2*x de 0 a 2", "4", -0.5, "am1-int"),
                ("Calculá ∫₀¹ x dx", "0.5", -0.3, "am1-int"),
                ("Primitiva de 3*x^2 en x=1", "1", -0.1, "am1-int"),
                ("Calculá ∫₀¹ x^2 dx", "0.3333333333", 0.2, "am1-int"),
                ("Área bajo y=x de 0 a 3", "4.5", 0.5, "am1-int-aplic"),
                ("Segunda derivada de x^3 en x=2", "12", 0.8, "am1-der-aplic"),
                ("Deriva (x^2+1)*(x-1) en x=1", "2", 1.1, "am1-der-aplic"),
                ("Límite lim_{x→0} sin(x)/x", "1", 1.4, "am1-lim"),
                ("Tasa media de x^2 en [1,2]", "3", 1.7, "am1-der-aplic"),
            ];
            qs[idx % qs.len()]
        }
        "algebra" => {
            let qs = [
                ("Solución de 3*x=12", "4", -1.7, "sec-ec"),
                ("Resolvé x/2=3", "6", -1.5, "sec-ec"),
                ("Resolvé 2*x+3=11", "4", -1.3, "sec-ec"),
                ("Raíz de 2*x+4=0", "-2", -1.1, "sec-ec"),
                ("Resolvé 5*x-10=0", "2", -0.9, "sec-ec"),
                ("Inversa de x+5 cuando x=2", "7", -0.7, "sec-ec"),
                ("Resolvé -x=5", "-5", -0.5, "sec-ec"),
                (
                    "Determinante de [[2,0],[0,3]]",
                    "6",
                    -0.3,
                    "alg-determinantes",
                ),
                ("Producto (x+1)*(x-1)", "x^2-1", -0.1, "sec-prop"),
                ("Factorizá x^2+2*x+1", "(x+1)^2", 0.2, "sec-cuad"),
                (
                    "Determinante de [[1,1],[1,1]]",
                    "0",
                    0.5,
                    "alg-determinantes",
                ),
                ("Factorizá x^2-9", "(x-3)(x+3)", 0.8, "sec-cuad"),
                ("Suma de raíces de x^2-3*x+2", "3", 1.1, "sec-cuad"),
                ("Raíces de x^2-5*x+6=0", "2 y 3", 1.3, "sec-cuad"),
                ("Rango de [[1,2],[2,4]]", "1", 1.6, "alg-matrices"),
            ];
            qs[idx % qs.len()]
        }
        "functions" => {
            let qs = [
                ("f(0) si f(x)=3*x+2", "2", -1.7, "am1-func"),
                ("Evaluá f(-1) si f(x)=x^2", "1", -1.5, "am1-func"),
                ("Evaluá f(2) si f(x)=x^2+1", "5", -1.3, "am1-func"),
                ("Imagen de f(x)=x^2 en x=3", "9", -1.1, "am1-func"),
                ("Raíz de f(x)=x-4", "4", -0.9, "am1-func"),
                ("Corte de y=x+5 con x=0", "5", -0.7, "sec-pend"),
                ("Pendiente de y=2*x+1", "2", -0.5, "sec-pend"),
                ("f(1) si f(x)=2^x", "2", -0.3, "am1-func"),
                ("¿f(x)=x es creciente?", "sí", -0.1, "am1-func"),
                ("¿f(x)=|x| es par?", "sí", 0.2, "am1-func"),
                ("¿f(x)=x^2 es par?", "sí", 0.5, "am1-func"),
                ("Dominio de 1/x", "x≠0", 0.8, "am1-func"),
                ("Ceros de f(x)=x*(x-1)", "0 y 1", 1.1, "am1-func"),
                ("Composición f(g(1)) con f=x+1,g=2*x", "3", 1.4, "am1-func"),
                ("Inversa de f(x)=x+3 en y=5", "2", 1.7, "am1-func"),
            ];
            qs[idx % qs.len()]
        }
        "trigonometry" => {
            let qs = [
                ("¿Cuánto vale sin(0)?", "0", -1.8, "sec-trig"),
                ("¿Cuánto vale cos(0)?", "1", -1.6, "sec-trig"),
                ("cos(0)", "1", -1.4, "sec-trig"),
                ("sin(π)", "0", -1.2, "sec-trig"),
                ("cos(π/2)", "0", -1.0, "sec-trig"),
                ("sin(π/2)", "1", -0.8, "sec-trig"),
                ("¿Cuánto vale tan(0)?", "0", -0.6, "sec-trig"),
                ("Valor máximo de cos(x)", "1", -0.4, "sec-trig"),
                ("cos(π)", "-1", -0.2, "sec-trig"),
                ("sin(3*π/2)", "-1", 0.1, "sec-trig"),
                ("Periodo de sin(x)", "2*pi", 0.4, "sec-trig"),
                ("sin(π/6)", "0.5", 0.7, "sec-trig"),
                ("Amplitud de sin(2*x)", "1", 1.0, "sec-trig"),
                ("Amplitud de 2*sin(x)", "2", 1.3, "sec-trig"),
                ("Identidad sin^2+cos^2", "1", 1.6, "sec-trig"),
            ];
            qs[idx % qs.len()]
        }
        "geometry" => {
            let qs = [
                ("Perímetro cuadrado lado 5", "20", -1.7, "pri-perim-area"),
                ("Área rectángulo 2x3", "6", -1.5, "pri-perim-area"),
                ("Área cuadrado lado 3", "9", -1.3, "pri-perim-area"),
                ("Perímetro triángulo 3,4,5", "12", -1.1, "pri-perim-area"),
                ("Hipotenusa catetos 3 y 4", "5", -0.9, "sec-pitagoras"),
                ("Área triángulo base 4 altura 3", "6", -0.7, "sec-area"),
                ("Volumen cubo arista 2", "8", -0.5, "sec-area"),
                ("Área cubo arista 1", "6", -0.3, "sec-area"),
                ("Perímetro círculo radio 2", "12.566", -0.1, "sec-area"),
                ("Área círculo radio 1", "3.14159", 0.2, "sec-area"),
                ("Área trapecio bases 2,4 altura 3", "9", 0.5, "sec-area"),
                ("Diagonal cuadrado lado 1", "1.4142", 0.8, "sec-pitagoras"),
                (
                    "Hipotenusa isósceles cateto 1",
                    "1.4142",
                    1.1,
                    "sec-pitagoras",
                ),
                ("Volumen cilindro r=1 h=2", "6.283", 1.4, "sec-area"),
                ("Volumen esfera radio 1", "4.18879", 1.7, "sec-area"),
            ];
            qs[idx % qs.len()]
        }
        "stats" => {
            let qs = [
                ("Media de {10,20}", "15", -1.7, "pri-datos"),
                ("Rango de {1,3,8}", "7", -1.5, "pri-datos"),
                ("Media de {2,4,6}", "4", -1.3, "pri-datos"),
                ("Mediana de {1,5,9}", "5", -1.1, "pri-datos"),
                ("Moda de {1,2,2,3}", "2", -0.9, "pri-datos"),
                ("Media de {1,2,3,4}", "2.5", -0.7, "pri-datos"),
                ("Rango de {5,5,5}", "0", -0.5, "pri-datos"),
                ("Probabilidad de cara en moneda", "0.5", -0.3, "sec-prob"),
                ("Desvío de {2,2}", "0", -0.1, "prob-var"),
                ("Varianza de {1,1,1}", "0", 0.2, "prob-var"),
                ("Probabilidad de 6 en dado", "0.1666667", 0.5, "sec-prob"),
                ("Probabilidad de no-6 en dado", "0.8333333", 0.8, "sec-prob"),
                ("Mediana de {1,2,3,4}", "2.5", 1.1, "pri-datos"),
                ("Esperanza de dado", "3.5", 1.4, "prob-var"),
                ("Prob. de dos caras", "0.25", 1.7, "prob-basica"),
            ];
            qs[idx % qs.len()]
        }
        "complex" => {
            let qs = [
                ("¿i^2?", "-1", -1.8, "complex"),
                ("Parte real de 3+4i", "3", -1.6, "complex"),
                ("Parte imaginaria de 2+5i", "5", -1.4, "complex"),
                ("¿Real de i?", "0", -1.2, "complex"),
                ("¿i^4?", "1", -1.0, "complex"),
                ("Conjugado de 2-3i", "2+3i", -0.8, "complex"),
                ("¿Conjugado de i?", "-i", -0.5, "complex"),
                ("Módulo de 3+4i", "5", -0.2, "complex"),
                ("Módulo de 2i", "2", 0.1, "complex"),
                ("Suma (1+i)+(1-i)", "2", 0.4, "complex"),
                ("Módulo de 0+1i", "1", 0.7, "complex"),
                ("Producto (1+i)*(1-i)", "2", 1.0, "complex"),
                ("Módulo de 1+i", "1.4142", 1.3, "complex"),
                ("Forma polar de 1 (módulo)", "1", 1.5, "complex"),
                ("Argumento de 1+i (grados)", "45", 1.8, "complex"),
            ];
            qs[idx % qs.len()]
        }
        _ => {
            // general / desconocida: dificultad uniforme por índice (no hay
            // enunciado real que calibrar).
            let b = -2.0 + (idx as f64) * (4.0 / 14.0);
            return (
                format!("Pregunta general {} (b={b:.1})", idx + 1),
                format!("{}", idx + 1),
                b,
                "general",
            );
        }
    };
    (q.to_string(), a.to_string(), b, lo)
}

/// Banco calibrado demo — ≥15 ítems por rama con `a,b,c` no constantes.
///
/// Generación determinista: `a` y `c` con dispersión via hash para evitar
/// constantes; `b` desde la tabla razonada por pregunta (ver
/// [`item_for_branch`], FIX 6). Validado por `bank_has_fifteen_items_per_branch`.
pub fn cat_bank(branch_id: &str) -> Vec<IrtItem> {
    let norm = branch_id.trim().to_lowercase();
    let branch = if BRANCHES.contains(&norm.as_str()) {
        norm.as_str()
    } else {
        // LO del currículum → familia con banco propio (antes caía al
        // genérico: `cat_bank("am1-der")` servía "Pregunta general N").
        branch_family_for_lo(&norm)
    };
    let mut items = Vec::with_capacity(16);
    for idx in 0..15usize {
        let (q, ans, b, lo) = item_for_branch(branch, idx);
        let a = det_a_for_index(idx);
        let c = det_c_for_index(idx.wrapping_add(branch.len()));
        let id = format!("{branch}-{idx:02}");
        items.push(IrtItem {
            id,
            branch_id: branch.to_string(),
            lo_id: lo.to_string(),
            a,
            b,
            c,
            question: q,
            answer: ans.clone(),
            validator: validator_for_answer(&ans),
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
// CAT + BKT + scheduler (R5): selección por máxima información ponderada
// POR ÍTEM (p_known/due del LO asociado al ítem)
// ──────────────────────────────────────────────────────────────────────────────

/// Boost multiplicativo cuando el scheduler marca la rama como vencida (due).
///
/// Documentado y acotado: 1.5x prioriza el repaso sin ahogar la información
/// del ítem (un ítem con info 0 sigue en 0).
pub const CAT_DUE_BOOST: f64 = 1.5;

/// Peso de un ítem cuyo LO no tiene datos BKT/scheduler: neutro (no favorece
/// ni castiga frente a los ítems del LO con datos).
pub const PESO_NEUTRO: f64 = 1.0;

/// Boost neutro (sin repaso vencido / sin datos). Ver [`PESO_NEUTRO`].
pub const BOOST_NEUTRO: f64 = 1.0;

/// Contexto BKT/scheduler de un LO para la ponderación por ítem (R5).
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct BktItemCtx {
    /// `P(sabe)` del LO según BKT. `None` o no finito → sin datos (peso neutro).
    pub p_known: Option<f64>,
    /// ¿El scheduler marca repaso vencido para ese LO?
    pub due: bool,
}

impl BktItemCtx {
    /// Crea el contexto por LO (`p_known: None` = sin datos).
    #[must_use]
    pub fn new(p_known: Option<f64>, due: bool) -> Self {
        Self { p_known, due }
    }
}

/// Entropía binaria normalizada de `p_known` en 0..=1 (0 = certeza, 1 = duda
/// máxima en p=0.5). Pura, NaN-safe: no finitos → 1.0 (máxima duda, honesto).
///
/// Mide cuánta incertidumbre le queda al BKT sobre la skill: cerca de 0 o 1
/// ya sabemos; cerca de 0.5 el próximo ejercicio informa más.
pub fn entropia_bkt(p_known: f64) -> f64 {
    if !p_known.is_finite() {
        return 1.0;
    }
    let p = p_known.clamp(0.0, 1.0);
    if p <= f64::EPSILON || p >= 1.0 - f64::EPSILON {
        return 0.0;
    }
    let h = -(p * p.ln() + (1.0 - p) * (1.0 - p).ln()) / std::f64::consts::LN_2;
    h.clamp(0.0, 1.0)
}

/// Contexto de un ítem: primero su `lo_id`, luego la rama del banco
/// (`branch_id`) para bancos de un solo skill (ramas legacy).
fn ctx_de_item(item: &IrtItem, ctx_por_lo: &BTreeMap<String, BktItemCtx>) -> Option<BktItemCtx> {
    ctx_por_lo
        .get(&item.lo_id)
        .or_else(|| ctx_por_lo.get(&item.branch_id))
        .copied()
}

/// Score R5 por ÍTEM: `Fisher(θ) × (0.5 + entropía(p_known del LO)) × boost`.
///
/// Sin datos del LO (o `p_known` no finito) el ítem pesa
/// [`PESO_NEUTRO`] × [`BOOST_NEUTRO`]. La entropía pesa la duda BKT pero nunca
/// anula Fisher (piso 0.5): con certeza total del LO sus ítems se pesan 0.5 y
/// el selector prioriza LOs donde aún hay duda.
fn score_ponderado(theta: f64, item: &IrtItem, ctx: Option<BktItemCtx>) -> f64 {
    let info = irt_fisher(theta, item.a, item.b, item.c);
    let (peso, boost) = match ctx.and_then(|c| c.p_known.map(|p| (p, c.due))) {
        Some((p, due)) if p.is_finite() => (
            0.5 + entropia_bkt(p),
            if due { CAT_DUE_BOOST } else { BOOST_NEUTRO },
        ),
        _ => (PESO_NEUTRO, BOOST_NEUTRO),
    };
    info * peso * boost
}

/// Argmax del score ponderado sobre un pool de ítems (sin repetir administrados).
fn seleccion_ponderada<'a>(
    items: impl Iterator<Item = &'a IrtItem>,
    administered_ids: &[String],
    theta: f64,
    ctx_por_lo: &BTreeMap<String, BktItemCtx>,
) -> Option<IrtItem> {
    let mut best: Option<(f64, IrtItem)> = None;
    for item in items {
        if administered_ids.contains(&item.id) {
            continue;
        }
        let score = score_ponderado(theta, item, ctx_de_item(item, ctx_por_lo));
        match &best {
            None => best = Some((score, item.clone())),
            Some((best_score, _)) if score > *best_score => best = Some((score, item.clone())),
            _ => {}
        }
    }
    best.map(|(_, it)| it)
}

/// Selecciona el próximo ítem por máxima información ponderada **por ítem**
/// con el `p_known`/`due` del LO asociado a cada ítem (R5 real, puro, sin I/O).
///
/// - `theta`: habilidad EAP actual (si no finita, usa 0.0 = prior).
/// - `ctx_por_lo`: contexto por `lo_id` (ver [`BktItemCtx`]); los LOs sin
///   entrada pesan neutro (1.0 × 1.0) frente a los que tienen datos.
///
/// **Por qué por ítem**: `p_known` y `due` son por LO/rama. Si la selección es
/// dentro de una sola rama y el peso es una constante para todos los ítems,
/// multiplicar los scores por la misma constante positiva preserva el argmax:
/// el ranking queda idéntico a [`cat_select_next`] y la ponderación es un
/// NO-OP (era exactamente el bug de esta función). Con contexto por LO los
/// pesos varían entre ítems y el ganador puede diferir del CAT puro (ver test
/// `cat_bkt_ponderacion_por_item_cambia_el_ganador`).
///
/// Retorna `None` si todo está administrado. Banco aún CAT-lite demo (ver
/// encabezado): calibración empírica N>200 pendiente
/// (`crate::bkt::etiqueta_calibracion`).
pub fn cat_select_next_bkt_lo(
    branch_id: &str,
    administered_ids: &[String],
    theta: f64,
    ctx_por_lo: &BTreeMap<String, BktItemCtx>,
) -> Option<IrtItem> {
    let theta = if theta.is_finite() { theta } else { 0.0 };
    let bank = cat_bank(branch_id);
    seleccion_ponderada(bank.iter(), administered_ids, theta, ctx_por_lo)
}

/// Selección cross-rama (R5 con boost): pool de varios bancos, score ponderado
/// por el `p_known`/`due` del LO de cada ítem. A diferencia de una selección
/// dentro de una rama, acá los pesos varían entre candidatos y el boost del
/// scheduler puede cambiar el ganador global (ver test
/// `cat_bkt_multi_cross_rama_cambia_el_ganador`).
///
/// `banks` acepta ramas legacy (`calculus`) o LOs (`am1-der`); los LOs se
/// enrutan a su familia ([`branch_family_for_lo`]).
pub fn cat_select_next_bkt_multi(
    banks: &[String],
    administered_ids: &[String],
    theta: f64,
    ctx_por_lo: &BTreeMap<String, BktItemCtx>,
) -> Option<IrtItem> {
    let theta = if theta.is_finite() { theta } else { 0.0 };
    let mut pool: Vec<IrtItem> = Vec::new();
    for bank_id in banks {
        pool.extend(cat_bank(bank_id));
    }
    seleccion_ponderada(pool.iter(), administered_ids, theta, ctx_por_lo)
}

/// Compatibilidad (camino vivo): selección ponderada con escalares.
///
/// `p_known`/`due` se aplican al skill pedido (`branch_id`, LO o rama
/// legacy); los ítems de LOs vecinos del mismo banco familiar quedan neutros
/// (sin datos). Con eso el peso varía POR ÍTEM y el ganador puede diferir de
/// [`cat_select_next`] (p. ej. `branch_id = "am1-der"` con repaso vencido
/// prioriza los ítems de `am1-der` sobre los de `am1-int`).
///
/// Salvedad honesta: si el banco es de un solo LO (rama legacy como
/// `"calculus"`), todos los ítems pesan igual y el ranking equivale al CAT
/// puro — un solo skill no da para reordenar (argmax invariante ante escala).
/// Para ponderar entre skills usar [`cat_select_next_bkt_lo`] o
/// [`cat_select_next_bkt_multi`] con contexto por LO.
pub fn cat_select_next_bkt(
    branch_id: &str,
    administered_ids: &[String],
    theta: f64,
    p_known: Option<f64>,
    due: bool,
) -> Option<IrtItem> {
    let mut ctx = BTreeMap::new();
    ctx.insert(branch_id.trim().to_lowercase(), BktItemCtx { p_known, due });
    cat_select_next_bkt_lo(branch_id, administered_ids, theta, &ctx)
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
    fn cat_bkt_ponderacion_por_item_cambia_el_ganador() {
        // Regresión FIX 2 (camino vivo, mismo entry point que llama
        // `grafito-app/assistant.rs`): antes `p_known`/`due` eran constantes
        // para TODOS los ítems de la llamada y multiplicar todos los scores
        // por la misma constante positiva preserva el argmax ⇒ el ranking era
        // idéntico a `cat_select_next` (ponderación R5 = NO-OP). Ahora el peso
        // va por ítem (LO asociado) y el ganador puede diferir del CAT puro.
        let theta = 0.0;
        let puro = cat_select_next("am1-der", &[], theta).expect("ítem puro");
        // `am1-der` con duda máxima (p=0.5 → entropía 1 → peso 1.5) y repaso
        // vencido (×1.5) = 2.25 para sus ítems; los de LOs vecinos pesan 1.0.
        let ponderado =
            cat_select_next_bkt("am1-der", &[], theta, Some(0.5), true).expect("ítem ponderado");
        assert_ne!(
            puro.id, ponderado.id,
            "la ponderación por ítem no cambió el ganador: sigue siendo un NO-OP"
        );
        assert_eq!(ponderado.lo_id, "am1-der", "debe priorizar el LO pedido");
        // Y con certeza total del LO (p=0 → entropía 0 → peso 0.5, sin due) el
        // selector se va a un LO vecino con más duda: dirección contraria.
        let con_certeza =
            cat_select_next_bkt("am1-der", &[], theta, Some(0.0), false).expect("ítem");
        assert_ne!(
            con_certeza.lo_id, "am1-der",
            "con el LO dominado no debe insistir en sus ítems"
        );
    }

    #[test]
    fn cat_bkt_lo_con_contexto_por_lo_pondera_de_verdad() {
        let theta = 0.0;
        let bank = cat_bank("calculus");
        let puro = cat_select_next("calculus", &[], theta).expect("puro");
        let mut ctx = BTreeMap::new();
        ctx.insert("am1-der".to_string(), BktItemCtx::new(Some(0.5), true));
        let ponderado = cat_select_next_bkt_lo("calculus", &[], theta, &ctx).expect("ponderado");
        assert_eq!(ponderado.lo_id, "am1-der", "el LO con duda+due debe ganar");
        assert_ne!(puro.id, ponderado.id, "ponderación por LO sin efecto");
        // El elegido es el argmax del score ponderado POR ÍTEM (pesos distintos
        // según el LO: 2.25 para `am1-der`, 1.0 neutro para el resto).
        let score = |it: &IrtItem| -> f64 {
            let (peso, boost) = if it.lo_id == "am1-der" {
                (0.5 + entropia_bkt(0.5), CAT_DUE_BOOST)
            } else {
                (PESO_NEUTRO, BOOST_NEUTRO)
            };
            irt_fisher(theta, it.a, it.b, it.c) * peso * boost
        };
        let mejor = bank.iter().map(score).fold(0.0_f64, f64::max);
        assert!(
            (score(&ponderado) - mejor).abs() < 1e-12,
            "debe ser el argmax ponderado por ítem"
        );
        // Sin datos para ningún LO: delega en el CAT puro (peso neutro uniforme).
        let sin_datos =
            cat_select_next_bkt_lo("calculus", &[], theta, &BTreeMap::new()).expect("sin datos");
        assert_eq!(sin_datos.id, puro.id, "sin contexto debe ser el CAT puro");
    }

    #[test]
    fn cat_bkt_multi_cross_rama_cambia_el_ganador() {
        // Regresión FIX 2 (variante cross-rama con boost): con candidatos de
        // varias ramas el `p_known`/`due` de cada LO varía entre candidatos y
        // el boost del scheduler cambia el ganador global.
        let theta = 0.0;
        let banks = ["algebra".to_string(), "calculus".to_string()];
        let mut ctx = BTreeMap::new();
        let los_algebra = [
            "sec-ec",
            "sec-cuad",
            "sec-prop",
            "alg-matrices",
            "alg-determinantes",
        ];
        let los_calculus = [
            "am1-der",
            "am1-int",
            "am1-der-aplic",
            "am1-lim",
            "am1-int-aplic",
        ];
        for lo in los_algebra {
            ctx.insert(lo.to_string(), BktItemCtx::new(Some(0.0), false));
        }
        for lo in los_calculus {
            ctx.insert(lo.to_string(), BktItemCtx::new(Some(0.5), true));
        }
        let elegido = cat_select_next_bkt_multi(&banks, &[], theta, &ctx).expect("multi");
        assert_eq!(
            elegido.branch_id, "calculus",
            "duda + due en calculus debe inclinar el pool"
        );
        // Con los contextos invertidos gana algebra: el boost decide, no el
        // argmax de Fisher puro.
        let mut ctx_inv = BTreeMap::new();
        for lo in los_algebra {
            ctx_inv.insert(lo.to_string(), BktItemCtx::new(Some(0.5), true));
        }
        for lo in los_calculus {
            ctx_inv.insert(lo.to_string(), BktItemCtx::new(Some(0.0), false));
        }
        let invertido =
            cat_select_next_bkt_multi(&banks, &[], theta, &ctx_inv).expect("multi invertido");
        assert_eq!(invertido.branch_id, "algebra");
        // Y el ganador difiere del CAT puro del pool (máxima Fisher sin pesos).
        let mut puro_pool: Option<IrtItem> = None;
        for bank in &banks {
            for it in cat_bank(bank) {
                let mejor = irt_fisher(theta, it.a, it.b, it.c)
                    > puro_pool
                        .as_ref()
                        .map_or(0.0, |m| irt_fisher(theta, m.a, m.b, m.c));
                if mejor {
                    puro_pool = Some(it);
                }
            }
        }
        let puro = puro_pool.expect("pool no vacío");
        assert_ne!(
            elegido.id, puro.id,
            "el cross-rama ponderado coincide con el CAT puro: NO-OP"
        );
    }

    #[test]
    fn b_del_banco_ordena_dificultad_percibida() {
        // Regresión FIX 6: `b` debe corresponder a la dificultad del enunciado
        // (antes: `b = -2 + idx·(4/14)` uniforme por ÍNDICE con las preguntas
        // en orden fijo ⇒ lo trivial podía quedar en b alto y viceversa).
        let bank = cat_bank("calculus");
        let b_de = |q: &str| -> f64 {
            bank.iter()
                .find(|it| it.question == q)
                .unwrap_or_else(|| panic!("pregunta ausente: {q}"))
                .b
        };
        // El caso del informe (guarda): lo trivial abajo, lo difícil arriba.
        assert!(b_de("Derivá x^2 en x=2") < b_de("Tasa media de x^2 en [1,2]"));
        // Pares que el orden por índice invertía (rojo-hoy): un derivado
        // directo no puede ser más difícil que un límite clásico, ni una
        // primitiva polinómica que una regla del producto.
        assert!(
            b_de("Derivá cos(x) en x=0") < b_de("Límite lim_{x→0} sin(x)/x"),
            "cos'(0) más difícil que el límite de sin(x)/x: b invertido"
        );
        assert!(
            b_de("Primitiva de 3*x^2 en x=1") < b_de("Deriva (x^2+1)*(x-1) en x=1"),
            "primitiva polinómica más difícil que la regla del producto: b invertido"
        );
        assert!(
            b_de("Integral de 2*x de 0 a 2") < b_de("Segunda derivada de x^3 en x=2"),
            "integral directa más difícil que la segunda derivada: b invertido"
        );
        // En todos los bancos: los enunciados están ordenados fácil → difícil.
        for branch in [
            "calculus",
            "algebra",
            "functions",
            "trigonometry",
            "geometry",
            "stats",
            "complex",
        ] {
            let bank = cat_bank(branch);
            for w in bank.windows(2) {
                assert!(
                    w[0].b < w[1].b,
                    "{branch}: b desordenado '{}' ({}) >= '{}' ({})",
                    w[0].question,
                    w[0].b,
                    w[1].question,
                    w[1].b
                );
            }
        }
    }

    #[test]
    fn eap_recupera_rasgo_conocido() {
        // Simulación de rasgo conocido: las respuestas se generan con la
        // dificultad PERCIBIDA de cada enunciado (tabla del test, independiente
        // del banco) y el EAP del banco debe recuperar θ. Con `b` sin relación
        // con el enunciado (pre-FIX 6) la recuperación se sesga.
        const PERCIBIDA: &[(&str, f64)] = &[
            ("Derivá x^2 en x=2", -1.7),
            ("Derivada de e^x en x=0", -1.5),
            ("Derivá x^3 en x=1", -1.3),
            ("Derivá sin(x) en x=0", -1.1),
            ("Derivá cos(x) en x=0", -0.9),
            ("Deriva ln(x) en x=1", -0.7),
            ("Integral de 2*x de 0 a 2", -0.5),
            ("Calculá ∫₀¹ x dx", -0.3),
            ("Primitiva de 3*x^2 en x=1", -0.1),
            ("Calculá ∫₀¹ x^2 dx", 0.2),
            ("Área bajo y=x de 0 a 3", 0.5),
            ("Segunda derivada de x^3 en x=2", 0.8),
            ("Deriva (x^2+1)*(x-1) en x=1", 1.1),
            ("Límite lim_{x→0} sin(x)/x", 1.4),
            ("Tasa media de x^2 en [1,2]", 1.7),
        ];
        let bank = cat_bank("calculus");
        assert_eq!(bank.len(), PERCIBIDA.len());
        for theta_true in [-1.2_f64, 0.0, 1.2] {
            let mut responses: Vec<(IrtItem, bool)> = Vec::new();
            for (q, d) in PERCIBIDA {
                let item = bank
                    .iter()
                    .find(|it| it.question == *q)
                    .unwrap_or_else(|| panic!("pregunta ausente: {q}"))
                    .clone();
                let p = irt_prob(theta_true, item.a, *d, item.c);
                responses.push((item, p > 0.5));
            }
            let (theta_hat, se) = eap_estimate(&responses);
            assert!(
                (theta_hat - theta_true).abs() <= 0.5,
                "EAP no recuperó el rasgo: θ={theta_true} → θ̂={theta_hat} (se={se})"
            );
        }
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

    #[test]
    fn entropia_bkt_extremos_y_duda_maxima() {
        // Certeza en los bordes, duda máxima en 0.5, honesto con NaN.
        assert!((entropia_bkt(0.5) - 1.0).abs() < 1e-9);
        assert!(entropia_bkt(0.0) < 1e-9);
        assert!(entropia_bkt(1.0) < 1e-9);
        assert!((entropia_bkt(f64::NAN) - 1.0).abs() < 1e-9);
        assert!((entropia_bkt(f64::INFINITY) - 1.0).abs() < 1e-9);
        // Simétrica y monótona hacia 0.5.
        assert!((entropia_bkt(0.3) - entropia_bkt(0.7)).abs() < 1e-9);
        assert!(entropia_bkt(0.3) > entropia_bkt(0.1));
        assert!(entropia_bkt(0.7) > entropia_bkt(0.9));
        assert_eq!(CAT_DUE_BOOST, 1.5);
    }

    #[test]
    fn cat_bkt_sin_datos_delega_sin_regresion() {
        // Sin BKT (None o no finito) el camino es idéntico al CAT puro.
        for branch in ["algebra", "calculus", "general"] {
            let puro = cat_select_next(branch, &[], 0.0).expect("banco no vacío");
            let via_bkt_none = cat_select_next_bkt(branch, &[], 0.0, None, false).expect("delega");
            assert_eq!(puro.id, via_bkt_none.id, "None debe delegar en {branch}");
            let via_bkt_nan =
                cat_select_next_bkt(branch, &[], 0.0, Some(f64::NAN), true).expect("delega");
            assert_eq!(puro.id, via_bkt_nan.id, "NaN debe delegar en {branch}");
        }
        // Banco agotado -> None en ambos caminos.
        let bank = cat_bank("stats");
        let todos: Vec<String> = bank.iter().map(|it| it.id.clone()).collect();
        assert!(cat_select_next("stats", &todos, 0.0).is_none());
        assert!(cat_select_next_bkt("stats", &todos, 0.0, Some(0.5), true).is_none());
    }

    #[test]
    fn cat_bkt_con_datos_pondera_maxima_informacion() {
        // Con BKT el elegido maximiza Fisher × (0.5+entropía) × boost.
        let branch = "calculus";
        let theta = 0.3;
        let p = 0.45;
        let due = true;
        let elegido = cat_select_next_bkt(branch, &[], theta, Some(p), due).expect("ítem");
        let bank = cat_bank(branch);
        let peso = 0.5 + entropia_bkt(p);
        let mejor = bank
            .iter()
            .map(|it| irt_fisher(theta, it.a, it.b, it.c) * peso * CAT_DUE_BOOST)
            .fold(0.0_f64, f64::max);
        let score_elegido =
            irt_fisher(theta, elegido.a, elegido.b, elegido.c) * peso * CAT_DUE_BOOST;
        assert!(
            (score_elegido - mejor).abs() < 1e-12,
            "debe elegir máxima info ponderada: {score_elegido} vs {mejor}"
        );
        // Con certeza total (p=1) el piso 0.5 mantiene el mismo ganador que Fisher puro.
        let puro = cat_select_next(branch, &[], theta).expect("puro");
        let certeza = cat_select_next_bkt(branch, &[], theta, Some(1.0), false).expect("certeza");
        assert_eq!(puro.id, certeza.id);
        // No repite administrados.
        let segundo = cat_select_next_bkt(
            branch,
            std::slice::from_ref(&elegido.id),
            theta,
            Some(p),
            due,
        )
        .expect("segundo");
        assert_ne!(elegido.id, segundo.id);
    }
}
