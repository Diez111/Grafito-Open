//! BKT pedagógico — estimación honesta offline.
//!
//! # Estado demo vs real (TODO honesto — no tocar docs en esta oleada)
//!
//! - **Real / implementado**: `bkt_update` (forward bayesiano estándar), `fit_params_em`
//!   con Baum-Welch forward-backward offline sobre `&[Vec<bool>]`, `predict_correct_prob`,
//!   métricas `auc_score` y `expected_calibration_error` offline para validar el modelo
//!   en datos históricos, y `ajuste_heuristico_p_learn` renombrado honestamente.
//! - **Demo / heurístico (legado honesto)**: `ajuste_heuristico_p_learn` solo toca
//!   `p_learn` con `p_learn*0.3 + tasa*0.2 + 0.15` — NO es EM, es ajuste rápido online.
//!   Mantenerlo marcado como heurístico hasta tener datos reales calibrados.
//! - **Calibración**: los `BktParams` por LO en `grafito-profile/src/bkt.rs` siguen siendo
//!   priors heurísticos por nivel (primaria 0.40 → AM2 0.17). La función EM aquí permite
//!   refinarlos offline con historial real, pero el banco aún no tiene calibración empírica
//!   con N>200 respuestas por LO (requisito IRT/BKT serio). Documentar ECE/AUC antes de
//!   promocionar a "calibrado".
//! - **No tocar `docs/architecture.md` en esta oleada**: dejar TODO aquí.
//!
//! # Referencia matemática
//!
//! Modelo BKT binario: estado latente `K in {0=desconocido,1=conocido}`.
//! - `p_init = P(K0=1)`
//! - `p_learn = P(K_t=1 | K_{t-1}=0)` (transición 0→1; 1→1 es 1.0, 1→0 es 0.0)
//! - `p_guess = P(correct | K=0)`
//! - `p_slip = P(incorrect | K=1)`
//! - `P(correct|K=1)=1-p_slip`
//!
//! EM (Baum-Welch) offline: forward α, backward β, γ y ξ para reestimar los 4
//! parámetros via máxima verosimilitud sobre múltiples secuencias.

use serde::{Deserialize, Serialize};

/// Parámetros BKT por habilidad (idem `grafito-profile::BktParams` pero local para
/// no crear ciclo pedagogy→profile).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct BktParams {
    pub p_init: f64,
    pub p_learn: f64,
    pub p_guess: f64,
    pub p_slip: f64,
}

impl Default for BktParams {
    fn default() -> Self {
        Self {
            p_init: 0.3,
            p_learn: 0.3,
            p_guess: 0.2,
            p_slip: 0.1,
        }
    }
}

impl BktParams {
    pub fn new(p_init: f64, p_learn: f64, p_guess: f64, p_slip: f64) -> Result<Self, String> {
        let c = Self {
            p_init,
            p_learn,
            p_guess,
            p_slip,
        };
        c.validate()?;
        Ok(c)
    }

    pub fn validate(&self) -> Result<(), String> {
        for (name, v) in [
            ("p_init", self.p_init),
            ("p_learn", self.p_learn),
            ("p_guess", self.p_guess),
            ("p_slip", self.p_slip),
        ] {
            if !v.is_finite() {
                return Err(format!("{name} no es finito"));
            }
            if !(0.0..=1.0).contains(&v) {
                return Err(format!("{name}={v} fuera de rango 0..=1"));
            }
        }
        if self.p_guess >= 1.0 || self.p_slip >= 1.0 {
            return Err("p_guess/p_slip deben ser < 1.0".into());
        }
        Ok(())
    }
}

/// Actualización bayesiana estándar BKT (pura, sin mutar externo).
pub fn bkt_update(p_known: f64, correct: bool, params: &BktParams) -> f64 {
    let p = p_known.clamp(0.0, 1.0);
    let guess = params.p_guess.clamp(0.0, 1.0);
    let slip = params.p_slip.clamp(0.0, 1.0);
    let learn = params.p_learn.clamp(0.0, 1.0);
    let posterior = if correct {
        let num = p * (1.0 - slip);
        let denom = num + (1.0 - p) * guess;
        if denom <= f64::EPSILON {
            p
        } else {
            num / denom
        }
    } else {
        let num = p * slip;
        let denom = num + (1.0 - p) * (1.0 - guess);
        if denom <= f64::EPSILON {
            p
        } else {
            num / denom
        }
    };
    let posterior = posterior.clamp(0.0, 1.0);
    let next = posterior + (1.0 - posterior) * learn;
    next.clamp(0.0, 1.0)
}

/// Probabilidad predicha de acierto dado `p_known` y `params`.
///
/// `P(correct) = p_known*(1-p_slip) + (1-p_known)*p_guess`
pub fn predict_correct_prob(p_known: f64, params: &BktParams) -> f64 {
    let p = p_known.clamp(0.0, 1.0);
    let guess = params.p_guess.clamp(0.0, 1.0);
    let slip = params.p_slip.clamp(0.0, 1.0);
    let prob = p * (1.0 - slip) + (1.0 - p) * guess;
    prob.clamp(0.0, 1.0)
}

// ──────────────────────────────────────────────────────────────────────────────
// Heurístico honesto (legado) — NO es EM.
// ──────────────────────────────────────────────────────────────────────────────

/// Ajuste heurístico **honesto** (demo) — solo toca `p_learn`.
///
/// Fórmula legada: `p_learn' = clamp(p_learn*0.3 + tasa*0.2 + 0.15, 0.05..0.95)`.
///
/// - `tasa_acierto` es proporción de aciertos en historia reciente (0..1).
/// - Retorna nuevos `BktParams` clonados con solo `p_learn` modificado.
/// - **No usar como sustituto de EM**; es atajo online sin E-step.
///
/// TODO(demo-vs-real): reemplazar por `fit_params_em` cuando haya historial
/// offline suficiente.
pub fn ajuste_heuristico_p_learn(params: &BktParams, tasa_acierto: f64) -> BktParams {
    let rate = if tasa_acierto.is_finite() {
        tasa_acierto.clamp(0.0, 1.0)
    } else {
        0.5
    };
    let new_learn = (params.p_learn * 0.3 + rate * 0.2 + 0.15).clamp(0.05, 0.95);
    let mut out = params.clone();
    out.p_learn = new_learn;
    out
}

/// Alias legado honesto para quien buscaba `fit_params_em_priors` heurístico.
///
/// Conservado por compatibilidad de grep; delega en `ajuste_heuristico_p_learn`
/// y documenta que **no es Baum-Welch**.
#[deprecated(
    note = "Renombrado honesto a ajuste_heuristico_p_learn; usar fit_params_em para Baum-Welch real"
)]
pub fn fit_params_em_priors_heuristico(params: &BktParams, tasa_acierto: f64) -> BktParams {
    ajuste_heuristico_p_learn(params, tasa_acierto)
}

// ──────────────────────────────────────────────────────────────────────────────
// EM real offline — Baum-Welch forward-backward
// ──────────────────────────────────────────────────────────────────────────────

/// EM real offline (Baum-Welch) sobre historial de respuestas.
///
/// - `histories`: slice de secuencias, cada una `Vec<bool>` donde `true=correcto`.
///   Múltiples secuencias = múltiples estudiantes o sesiones del mismo LO.
/// - `initial`: prior inicial (ej. `BktParams::default()` o por LO).
/// - `max_iter`: tope de iteraciones (típico 20..50); corta antes si Δ<1e-4.
///
/// Retorna `BktParams` reestimados. Si `histories` vacío o todo vacío, retorna
/// `initial` clonado sin cambios.
///
/// **Implementación**: forward α, backward β sin escalado log (suficiente para
/// secuencias cortas ≤200). Emisiones clamp 0.05..0.95 para evitar 0. Reestimación
/// cerrada: `p_init = mean(γ0=1)`, `p_learn = Σ ξ(0→1)/ Σ γ(0)`, `p_guess =
/// Σ_{correct} γ(0)/ Σ γ(0)`, `p_slip = Σ_{incorrect} γ(1)/ Σ γ(1)`.
///
/// **Demo vs real**: la matemática es real, pero la calibración requiere datos
/// empíricos; con datos sintéticos / demo el resultado solo valida la convergencia.
pub fn fit_params_em(histories: &[Vec<bool>], initial: &BktParams, max_iter: usize) -> BktParams {
    if histories.is_empty() {
        return initial.clone();
    }
    let filtered: Vec<&Vec<bool>> = histories.iter().filter(|v| !v.is_empty()).collect();
    if filtered.is_empty() {
        return initial.clone();
    }
    let mut params = initial.clone();
    // Clamp inicial para evitar degenerados
    params.p_init = params.p_init.clamp(0.05, 0.95);
    params.p_learn = params.p_learn.clamp(0.05, 0.95);
    params.p_guess = params.p_guess.clamp(0.05, 0.95);
    params.p_slip = params.p_slip.clamp(0.05, 0.95);

    let max_iter = max_iter.clamp(1, 100);

    for _ in 0..max_iter {
        let mut num_init = 0.0_f64;
        let mut den_init = 0.0_f64;
        let mut num_learn = 0.0_f64;
        let mut den_learn = 0.0_f64;
        let mut num_guess = 0.0_f64;
        let mut den_guess = 0.0_f64;
        let mut num_slip = 0.0_f64;
        let mut den_slip = 0.0_f64;

        let p_init = params.p_init;
        let p_learn = params.p_learn;
        let p_guess = params.p_guess;
        let p_slip = params.p_slip;

        for seq in &filtered {
            let n = seq.len();
            // forward
            let mut alpha = vec![[0.0_f64; 2]; n];
            let obs0 = seq[0];
            let emit_unk0 = if obs0 { p_guess } else { 1.0 - p_guess };
            let emit_known0 = if obs0 { 1.0 - p_slip } else { p_slip };
            alpha[0][0] = (1.0 - p_init) * emit_unk0;
            alpha[0][1] = p_init * emit_known0;

            for t in 1..n {
                let obs = seq[t];
                let emit_unk = if obs { p_guess } else { 1.0 - p_guess };
                let emit_known = if obs { 1.0 - p_slip } else { p_slip };
                let prev_u = alpha[t - 1][0];
                let prev_k = alpha[t - 1][1];
                // trans: unk->unk =1-p_learn, unk->known=p_learn, known->known=1
                let to_u = prev_u * (1.0 - p_learn) * emit_unk;
                let to_k = (prev_u * p_learn + prev_k) * emit_known;
                alpha[t][0] = to_u;
                alpha[t][1] = to_k;
            }

            // backward
            let mut beta = vec![[0.0_f64; 2]; n];
            beta[n - 1][0] = 1.0;
            beta[n - 1][1] = 1.0;
            for t in (0..n - 1).rev() {
                let obs_next = seq[t + 1];
                let emit_unk_next = if obs_next { p_guess } else { 1.0 - p_guess };
                let emit_known_next = if obs_next { 1.0 - p_slip } else { p_slip };
                beta[t][0] = (1.0 - p_learn) * emit_unk_next * beta[t + 1][0]
                    + p_learn * emit_known_next * beta[t + 1][1];
                beta[t][1] = emit_known_next * beta[t + 1][1];
            }

            let likelihood = alpha[n - 1][0] + alpha[n - 1][1];
            if !likelihood.is_finite() || likelihood <= f64::EPSILON {
                continue;
            }

            // gamma
            let mut gamma = vec![[0.0_f64; 2]; n];
            for t in 0..n {
                // suma alpha*beta = likelihood para HMM binario (sin escalado)
                gamma[t][0] = (alpha[t][0] * beta[t][0] / likelihood).clamp(0.0, 1.0);
                gamma[t][1] = (alpha[t][1] * beta[t][1] / likelihood).clamp(0.0, 1.0);
                // normalizar por seguridad (debe sumar 1)
                let s = gamma[t][0] + gamma[t][1];
                if s > f64::EPSILON {
                    gamma[t][0] /= s;
                    gamma[t][1] /= s;
                }
            }

            // init
            num_init += gamma[0][1];
            den_init += 1.0;

            // guess / slip
            for t in 0..n {
                den_guess += gamma[t][0];
                den_slip += gamma[t][1];
                if seq[t] {
                    num_guess += gamma[t][0];
                } else {
                    num_slip += gamma[t][1];
                }
            }

            // learn via xi (unk->known)
            for t in 0..n - 1 {
                let obs_next = seq[t + 1];
                let emit_known_next = if obs_next { 1.0 - p_slip } else { p_slip };
                let xi_unk_known =
                    alpha[t][0] * p_learn * emit_known_next * beta[t + 1][1] / likelihood;
                den_learn += gamma[t][0];
                num_learn += xi_unk_known.clamp(0.0, 1.0);
            }
        }

        if den_init < f64::EPSILON
            || den_learn < f64::EPSILON
            || den_guess < f64::EPSILON
            || den_slip < f64::EPSILON
        {
            break;
        }

        let new_p_init = (num_init / den_init).clamp(0.05, 0.95);
        let new_p_learn = (num_learn / den_learn).clamp(0.05, 0.95);
        let new_p_guess = (num_guess / den_guess).clamp(0.05, 0.95);
        let new_p_slip = (num_slip / den_slip).clamp(0.05, 0.95);

        let delta = (new_p_init - params.p_init).abs()
            + (new_p_learn - params.p_learn).abs()
            + (new_p_guess - params.p_guess).abs()
            + (new_p_slip - params.p_slip).abs();

        params.p_init = new_p_init;
        params.p_learn = new_p_learn;
        params.p_guess = new_p_guess;
        params.p_slip = new_p_slip;

        if delta < 1e-4 {
            break;
        }
    }

    // Validar sin pánico: si falla, retorna clamp
    if params.validate().is_err() {
        params.p_init = params.p_init.clamp(0.05, 0.95);
        params.p_learn = params.p_learn.clamp(0.05, 0.95);
        params.p_guess = params.p_guess.clamp(0.05, 0.95);
        params.p_slip = params.p_slip.clamp(0.05, 0.95);
    }
    params
}

// ──────────────────────────────────────────────────────────────────────────────
// Verosimilitud offline — para validar que EM no empeora el ajuste
// ──────────────────────────────────────────────────────────────────────────────

/// Log-verosimilitud total de `histories` bajo `params` (forward pass puro).
///
/// Retorna `None` si no hay secuencias no vacías o ninguna verosimilitud es
/// finita y positiva. Sirve para verificar offline la garantía monótona de EM:
/// `LL(fit_params_em(h, init)) >= LL(h, init) - 1e-9`.
///
/// Misma dinámica que `fit_params_em` (transición 0→1 con `p_learn`, 1→1 con
/// 1.0) y mismos clamps 0.05..0.95, para que ambas funciones hablen del mismo
/// modelo.
pub fn bkt_log_likelihood(histories: &[Vec<bool>], params: &BktParams) -> Option<f64> {
    if histories.is_empty() {
        return None;
    }
    let p_init = params.p_init.clamp(0.05, 0.95);
    let p_learn = params.p_learn.clamp(0.05, 0.95);
    let p_guess = params.p_guess.clamp(0.05, 0.95);
    let p_slip = params.p_slip.clamp(0.05, 0.95);
    let emit = |obs: bool, known: bool| {
        if known {
            if obs {
                1.0 - p_slip
            } else {
                p_slip
            }
        } else if obs {
            p_guess
        } else {
            1.0 - p_guess
        }
    };
    let mut total = 0.0_f64;
    let mut any = false;
    for seq in histories.iter().filter(|v| !v.is_empty()) {
        let mut alpha_u = (1.0 - p_init) * emit(seq[0], false);
        let mut alpha_k = p_init * emit(seq[0], true);
        for obs in seq.iter().skip(1) {
            let next_u = alpha_u * (1.0 - p_learn) * emit(*obs, false);
            let next_k = (alpha_u * p_learn + alpha_k) * emit(*obs, true);
            alpha_u = next_u;
            alpha_k = next_k;
        }
        let likelihood = alpha_u + alpha_k;
        if !likelihood.is_finite() || likelihood <= 0.0 {
            continue;
        }
        total += likelihood.ln();
        any = true;
    }
    if any && total.is_finite() {
        Some(total)
    } else {
        None
    }
}

// ──────────────────────────────────────────────────────────────────────────────
// Métricas offline — AUC / ECE
// ──────────────────────────────────────────────────────────────────────────────

/// AUC ROC para ranking de `probs` vs `labels`.
///
/// Retorna `None` si `probs.len() != labels.len()` o si no hay positivos/negativos
/// o si algún prob no finito. Rango 0..1 (0.5 = azar, 1.0 = perfecto).
///
/// Cálculo pairwise exacto O(P*N) sin aproximaciones.
pub fn auc_score(probs: &[f64], labels: &[bool]) -> Option<f64> {
    if probs.len() != labels.len() || probs.is_empty() {
        return None;
    }
    for &p in probs {
        if !p.is_finite() {
            return None;
        }
    }
    let mut pos = Vec::new();
    let mut neg = Vec::new();
    for (p, &lab) in probs.iter().zip(labels.iter()) {
        if lab {
            pos.push(*p);
        } else {
            neg.push(*p);
        }
    }
    if pos.is_empty() || neg.is_empty() {
        return None;
    }
    let mut concordant = 0.0_f64;
    let mut total = 0.0_f64;
    for &pp in &pos {
        for &pn in &neg {
            total += 1.0;
            if pp > pn {
                concordant += 1.0;
            } else if (pp - pn).abs() < f64::EPSILON {
                concordant += 0.5;
            }
        }
    }
    if total <= f64::EPSILON {
        return None;
    }
    Some((concordant / total).clamp(0.0, 1.0))
}

/// Expected Calibration Error (ECE) con `n_bins` bins uniformes en [0,1].
///
/// Retorna `None` si `probs.len() != labels.len()` o vacío o n_bins==0 o probs no finitos.
/// Rango 0..1 (0 = perfectamente calibrado).
pub fn expected_calibration_error(probs: &[f64], labels: &[bool], n_bins: usize) -> Option<f64> {
    if probs.len() != labels.len() || probs.is_empty() || n_bins == 0 {
        return None;
    }
    for &p in probs {
        if !p.is_finite() || !(0.0..=1.0).contains(&p) {
            return None;
        }
    }
    let n_bins = n_bins.clamp(1, 50);
    let mut bin_sum_prob = vec![0.0_f64; n_bins];
    let mut bin_sum_label = vec![0.0_f64; n_bins];
    let mut bin_cnt = vec![0usize; n_bins];

    for (p, &lab) in probs.iter().zip(labels.iter()) {
        let mut idx = (p * n_bins as f64).floor() as usize;
        if idx >= n_bins {
            idx = n_bins - 1;
        }
        bin_sum_prob[idx] += *p;
        bin_sum_label[idx] += if lab { 1.0 } else { 0.0 };
        bin_cnt[idx] += 1;
    }

    let total = probs.len() as f64;
    let mut ece = 0.0_f64;
    for i in 0..n_bins {
        if bin_cnt[i] == 0 {
            continue;
        }
        let cnt = bin_cnt[i] as f64;
        let acc = bin_sum_label[i] / cnt;
        let conf = bin_sum_prob[i] / cnt;
        let weight = cnt / total;
        ece += weight * (acc - conf).abs();
    }
    Some(ece.clamp(0.0, 1.0))
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod tests {
    use super::*;

    /// Generador sintético honesto: muestrea el proceso latente BKT real
    /// (estado conocido/desconocido + emisión + transición 0→1) con PRNG
    /// determinista xorshift64*. NO usa `bkt_update` para simular: el filtro
    /// bayesiano es inferencia, no generador (la versión anterior mezclaba
    /// ambos y reutilizaba la misma muestra para estado y transición).
    fn sample_histories(
        truth: &BktParams,
        n_seq: usize,
        len: usize,
        seed_base: u64,
    ) -> Vec<Vec<bool>> {
        let mut histories = Vec::with_capacity(n_seq);
        for s in 0..n_seq {
            let mut state =
                seed_base.wrapping_add((s as u64).wrapping_add(1).wrapping_mul(0x9E3779B97F4A7C15));
            if state == 0 {
                state = 0x2545_F491_4F6C_DD1D;
            }
            let mut next_u01 = || {
                state ^= state >> 12;
                state ^= state << 25;
                state ^= state >> 27;
                state = state.wrapping_mul(0x2545_F491_4F6C_DD1D);
                ((state >> 11) as f64) / ((1u64 << 53) as f64)
            };
            let mut known = next_u01() < truth.p_init;
            let mut seq = Vec::with_capacity(len);
            for _ in 0..len {
                let r_emit = next_u01();
                let correct = if known {
                    r_emit >= truth.p_slip
                } else {
                    r_emit < truth.p_guess
                };
                seq.push(correct);
                if !known && next_u01() < truth.p_learn {
                    known = true;
                }
            }
            histories.push(seq);
        }
        histories
    }

    #[test]
    fn ajuste_heuristico_solo_toca_p_learn() {
        let p = BktParams {
            p_init: 0.3,
            p_learn: 0.3,
            p_guess: 0.2,
            p_slip: 0.1,
        };
        let q = ajuste_heuristico_p_learn(&p, 0.8);
        assert!((q.p_init - p.p_init).abs() < 1e-12);
        assert!((q.p_guess - p.p_guess).abs() < 1e-12);
        assert!((q.p_slip - p.p_slip).abs() < 1e-12);
        // p_learn*0.3 + 0.8*0.2 +0.15 = 0.09+0.16+0.15=0.40
        assert!((q.p_learn - 0.40).abs() < 1e-12);
        // tasa clamp
        let r = ajuste_heuristico_p_learn(&p, 2.0);
        let s = ajuste_heuristico_p_learn(&p, -1.0);
        assert!(r.p_learn.is_finite() && s.p_learn.is_finite());
        // no debe colapsar a borde
        assert!((0.05..=0.95).contains(&r.p_learn));
    }

    #[test]
    fn bkt_update_monotono() {
        let params = BktParams::default();
        let up = bkt_update(0.3, true, &params);
        assert!(up > 0.3);
        let down = bkt_update(up, false, &params);
        assert!(down < up);
    }

    #[test]
    fn predict_correct_prob_en_rango() {
        let p = BktParams::default();
        for pk in [0.0, 0.3, 1.0] {
            let pr = predict_correct_prob(pk, &p);
            assert!((0.0..=1.0).contains(&pr));
        }
        // si sabe todo, prob =1-slip
        let full = predict_correct_prob(1.0, &p);
        assert!((full - (1.0 - p.p_slip)).abs() < 1e-12);
        // si nada sabe, prob=guess
        let none = predict_correct_prob(0.0, &p);
        assert!((none - p.p_guess).abs() < 1e-12);
    }

    #[test]
    fn fit_params_em_converge_sobre_datos_sinteticos() {
        // Garantía real que verifica este test: EM es monótono en
        // verosimilitud (no empeora el ajuste del prior). NO afirma
        // recuperación exacta de `truth`: eso exige N≫ e identificabilidad
        // (par guess/slip) fuera del alcance de un unit test. La calibración
        // predictiva se verifica en `em_calibra_y_auc_supera_azar_end_to_end`.
        let truth = BktParams {
            p_init: 0.2,
            p_learn: 0.5,
            p_guess: 0.25,
            p_slip: 0.15,
        };
        let histories = sample_histories(&truth, 40, 12, 0x1234_5678_9ABC_DEF0);
        let initial = BktParams::default();
        let ll_before = bkt_log_likelihood(&histories, &initial).expect("ll inicial finita");
        let fitted = fit_params_em(&histories, &initial, 30);
        assert!(fitted.validate().is_ok());
        let ll_after = bkt_log_likelihood(&histories, &fitted).expect("ll ajustada finita");
        assert!(
            ll_after >= ll_before - 1e-9,
            "EM no debe empeorar verosimilitud: {ll_before} -> {ll_after}"
        );
        // Debe estar en rango y no colapsar a borde extremo sin datos suficientes
        assert!((0.05..=0.95).contains(&fitted.p_learn));
        assert!((0.05..=0.95).contains(&fitted.p_guess));
        assert!((0.05..=0.95).contains(&fitted.p_slip));
        // vacíos -> retorna initial clonado
        let empty: Vec<Vec<bool>> = Vec::new();
        let same = fit_params_em(&empty, &initial, 10);
        assert_eq!(same, initial);
        let with_empty = vec![Vec::new(), Vec::new()];
        let same2 = fit_params_em(&with_empty, &initial, 10);
        assert_eq!(same2, initial);
        // verosimilitud sin datos -> None (sin pánico, sin NaN)
        assert!(bkt_log_likelihood(&[], &initial).is_none());
        assert!(bkt_log_likelihood(&[Vec::new()], &initial).is_none());
    }

    #[test]
    fn em_calibra_y_auc_supera_azar_end_to_end() {
        // Calibración end-to-end: historial sintético del proceso BKT real →
        // EM offline desde prior genérico → predicciones secuenciales con los
        // params ajustados → AUC debe superar el azar (0.5) con margen.
        // El `truth` se elige lejos del `default()` para que el ajuste tenga
        // trabajo real que hacer (antes ambos casi coincidían y el test no
        // discriminaba nada).
        let truth = BktParams {
            p_init: 0.2,
            p_learn: 0.5,
            p_guess: 0.25,
            p_slip: 0.15,
        };
        let histories = sample_histories(&truth, 60, 12, 0x0BAD_F00D_CAFE_1234);
        let initial = BktParams::default();
        let fitted = fit_params_em(&histories, &initial, 30);
        assert!(fitted.validate().is_ok());

        // Predicción secuencial honesta: p_known arranca en fitted.p_init y se
        // actualiza SOLO con el pasado observado (sin mirar el label actual).
        let mut probs = Vec::new();
        let mut labels = Vec::new();
        for seq in &histories {
            let mut p_known = fitted.p_init;
            for &correct in seq {
                probs.push(predict_correct_prob(p_known, &fitted));
                labels.push(correct);
                p_known = bkt_update(p_known, correct, &fitted);
            }
        }
        let auc = auc_score(&probs, &labels).expect("auc con ambas clases");
        assert!(
            auc > 0.55,
            "modelo calibrado debe rankear mejor que azar, AUC={auc}"
        );

        // Y el ajuste EM no debe degradar el ranking del prior genérico
        // (EM optimiza verosimilitud, no AUC: tolerancia 0.02).
        let mut probs_init = Vec::with_capacity(probs.len());
        for seq in &histories {
            let mut p_known = initial.p_init;
            for &correct in seq {
                probs_init.push(predict_correct_prob(p_known, &initial));
                p_known = bkt_update(p_known, correct, &initial);
            }
        }
        let auc_init = auc_score(&probs_init, &labels).expect("auc inicial");
        assert!(
            auc >= auc_init - 0.02,
            "EM no debe degradar ranking: init={auc_init} fitted={auc}"
        );
    }

    #[test]
    fn fit_params_em_no_panico_con_secuencias_cortas_y_extremos() {
        let initial = BktParams::default();
        let histories = vec![
            vec![true, true, true],
            vec![false, false, false],
            vec![true, false, true, false],
            vec![true],
            vec![false],
        ];
        let fitted = fit_params_em(&histories, &initial, 5);
        assert!(fitted.validate().is_ok());
        assert!(fitted.p_learn.is_finite());
    }

    #[test]
    fn auc_score_perfecto_y_azar() {
        // perfecto ranking: pos tienen prob alta
        let probs = vec![0.9, 0.8, 0.3, 0.2];
        let labels = vec![true, true, false, false];
        let auc = auc_score(&probs, &labels).expect("auc");
        assert!((auc - 1.0).abs() < 1e-9, "auc perfecto {auc}");

        // azar invertido debe dar 0
        let probs2 = vec![0.2, 0.3, 0.8, 0.9];
        let labels2 = vec![true, true, false, false];
        let auc2 = auc_score(&probs2, &labels2).expect("auc2");
        assert!((auc2 - 0.0).abs() < 1e-9, "auc invertido {auc2}");

        // caso con empates
        let probs3 = vec![0.5, 0.5, 0.5, 0.5];
        let labels3 = vec![true, true, false, false];
        let auc3 = auc_score(&probs3, &labels3).expect("auc3");
        assert!((auc3 - 0.5).abs() < 1e-9);

        // edge: sin positivos
        assert!(auc_score(&[0.9, 0.8], &[false, false]).is_none());
        // edge: longitud distinta
        assert!(auc_score(&[0.5], &[true, false]).is_none());
        // no finito
        assert!(auc_score(&[f64::NAN, 0.5], &[true, false]).is_none());
    }

    #[test]
    fn ece_perfectamente_calibrado_vs_descalibrado() {
        // perfectamente calibrado: prob == label (0 o 1)
        let probs = vec![1.0, 1.0, 0.0, 0.0];
        let labels = vec![true, true, false, false];
        let ece = expected_calibration_error(&probs, &labels, 10).expect("ece");
        assert!(ece < 1e-9, "ece perfecto {ece}");

        // descalibrado: prob 0.9 pero label 0 siempre
        let probs2 = vec![0.9, 0.9, 0.9, 0.9];
        let labels2 = vec![false, false, false, false];
        let ece2 = expected_calibration_error(&probs2, &labels2, 10).expect("ece2");
        assert!(ece2 > 0.8, "ece descalibrado {ece2}");

        // bins vacíos manejados
        let probs3 = vec![0.1, 0.1, 0.9, 0.9];
        let labels3 = vec![false, false, true, true];
        let ece3 = expected_calibration_error(&probs3, &labels3, 10).expect("ece3");
        assert!(ece3 < 0.2);

        // longitud distinta -> None
        assert!(expected_calibration_error(&[0.5], &[true, false], 10).is_none());
        assert!(expected_calibration_error(&[], &[], 10).is_none());
        assert!(expected_calibration_error(&[0.5], &[true], 0).is_none());
        // prob fuera rango
        assert!(expected_calibration_error(&[2.0], &[true], 5).is_none());
    }

    #[test]
    fn auc_y_ece_sobre_bkt_predicciones_reales() {
        // Simular BKT con 20 pasos y computar AUC/ECE sobre predicciones vs realidad
        let params = BktParams::default();
        let mut p_known = 0.3;
        let mut probs = Vec::new();
        let mut labels = Vec::new();
        let seq = vec![
            true, true, false, true, true, false, true, false, true, true, true, false, true, true,
            false, true, true, true, false, true,
        ];
        for &lab in &seq {
            let prob = predict_correct_prob(p_known, &params);
            probs.push(prob);
            labels.push(lab);
            p_known = bkt_update(p_known, lab, &params);
        }
        let auc = auc_score(&probs, &labels);
        let ece = expected_calibration_error(&probs, &labels, 10);
        assert!(auc.is_some());
        assert!(ece.is_some());
        let aucv = auc.expect("auc");
        let ecev = ece.expect("ece");
        assert!((0.0..=1.0).contains(&aucv));
        assert!((0.0..=1.0).contains(&ecev));
    }
}
