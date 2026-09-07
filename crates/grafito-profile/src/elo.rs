//! Elo mínimo funcional para dificultad adaptativa (S, puro, sin I/O).
//!
//! Mantiene un rating por alumno y por ítem (`R_alumno`, `R_item`) con
//! `K` acotado. Fórmula clásica:
//! `E = 1 / (1 + 10^((R_item - R_alumno) / 400))`,
//! `R' = R + K * (score - E)` con `score ∈ {0, 0.5, 1}`.
//!
//! La calibración EM con `N≥200` y el banco de ítems calibrado quedan como L
//! en [`crate::stubs::em_calibration_stub`]; aquí vive el S mínimo que
//! `assess_answer` ya puede usar para ordenar dificultad. PII siempre local:
//! solo `f64` + conteos, sin red ni disco.

use serde::{Deserialize, Serialize};

/// Rating default (jugador/ítem nuevo, igual que ajedrez FIDE base).
pub const ELO_DEFAULT_RATING: f64 = 1_500.0;
/// Piso y techo (evitan divergencia con rachas largas).
pub const ELO_MIN_RATING: f64 = 100.0;
/// Techo de rating.
pub const ELO_MAX_RATING: f64 = 3_000.0;
/// `K` default (sensibilidad por partida, igual que FIDE amateur).
pub const ELO_DEFAULT_K: f64 = 32.0;
/// `K` mínimo/máximo (acotan saltos por un solo ejercicio).
pub const ELO_MIN_K: f64 = 1.0;
/// `K` máximo.
pub const ELO_MAX_K: f64 = 64.0;
/// Divisor clásico de la curva logística.
pub const ELO_SCALE: f64 = 400.0;

/// Rating Elo: newtype `100..=3000` (NaN-safe, clamp en construcción fallida no: `Err`).
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct EloRating(f64);

impl EloRating {
    /// Valida `100..=3000` finito. `Err(String)` si fuera de rango.
    pub fn try_new(rating: f64) -> Result<Self, String> {
        if !rating.is_finite() {
            return Err("elo no es finito".to_string());
        }
        if !(ELO_MIN_RATING..=ELO_MAX_RATING).contains(&rating) {
            return Err(format!(
                "elo {rating} fuera de {ELO_MIN_RATING}..={ELO_MAX_RATING}"
            ));
        }
        Ok(Self(rating))
    }

    /// Rating default (1500).
    #[must_use]
    pub fn default_rating() -> Self {
        Self(ELO_DEFAULT_RATING)
    }

    /// Valor validado.
    #[must_use]
    pub fn as_f64(&self) -> f64 {
        self.0
    }

    /// Probabilidad esperada de acertar contra `opponent` (`0..=1`).
    #[must_use]
    pub fn expected_vs(&self, opponent: EloRating) -> f64 {
        elo_expected(self.0, opponent.0)
    }

    /// Actualiza tras un resultado (`score` 0/0.5/1, `k` 1..=64).
    ///
    /// `Err` si `score`/`k` inválidos. Clamp final a `100..=3000`.
    pub fn update_vs(&self, opponent: EloRating, score: f64, k: f64) -> Result<Self, String> {
        Ok(Self(elo_update(self.0, opponent.0, score, k)?))
    }
}

/// Probabilidad esperada (`0..=1`, NaN-safe, clamp).
///
/// `E = 1 / (1 + 10^((item - player) / 400))`. No finitos → `0.5` honesto.
#[must_use]
pub fn elo_expected(player: f64, opponent: f64) -> f64 {
    if !player.is_finite() || !opponent.is_finite() {
        return 0.5;
    }
    let player_c = player.clamp(ELO_MIN_RATING, ELO_MAX_RATING);
    let opponent_c = opponent.clamp(ELO_MIN_RATING, ELO_MAX_RATING);
    let exponent = (opponent_c - player_c) / ELO_SCALE;
    // `10^exp` puede overflow con ratings extremos ya clamp: `powf` da `inf`,
    // que colapsa a 0/1 honestos (no NaN).
    let denom = 1.0 + 10_f64.powf(exponent);
    if !denom.is_finite() || denom <= 0.0 {
        return if exponent > 0.0 { 0.0 } else { 1.0 };
    }
    (1.0 / denom).clamp(0.0, 1.0)
}

/// Actualiza un rating (`R' = R + K*(score - E)`, clamp `100..=3000`).
///
/// - `score` debe ser `0.0`, `0.5` o `1.0` (derrota/tablas/victoria).
/// - `k` en `1..=64` finito.
///
/// `Err(String)` si inválidos; nunca pánico, nunca NaN (no finitos → `Err`).
pub fn elo_update(player: f64, opponent: f64, score: f64, k: f64) -> Result<f64, String> {
    if !player.is_finite() || !opponent.is_finite() {
        return Err("elo player/opponent no finito".to_string());
    }
    if !(ELO_MIN_RATING..=ELO_MAX_RATING).contains(&player)
        || !(ELO_MIN_RATING..=ELO_MAX_RATING).contains(&opponent)
    {
        return Err("elo fuera de 100..=3000".to_string());
    }
    if !matches!(score, 0.0 | 0.5 | 1.0) {
        return Err(format!("score {score} debe ser 0.0, 0.5 o 1.0"));
    }
    if !k.is_finite() || !(ELO_MIN_K..=ELO_MAX_K).contains(&k) {
        return Err(format!("k {k} fuera de {ELO_MIN_K}..={ELO_MAX_K}"));
    }
    let expected = elo_expected(player, opponent);
    let next = player + k * (score - expected);
    Ok(next.clamp(ELO_MIN_RATING, ELO_MAX_RATING))
}

/// Estado Elo mínimo por alumno/ítem (rating + partidas, para `K` adaptativo futuro).
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct EloState {
    /// Rating actual.
    pub rating: EloRating,
    /// Partidas jugadas (saturante, para decaer `K` si se quiere).
    pub games: u32,
}

impl EloState {
    /// Estado inicial (1500, 0 partidas).
    #[must_use]
    pub fn new() -> Self {
        Self {
            rating: EloRating::default_rating(),
            games: 0,
        }
    }

    /// Registra un resultado contra `opponent` con `k` y suma una partida.
    ///
    /// `Err` si `score`/`k` inválidos (no muta en ese caso).
    pub fn record(&mut self, opponent: EloRating, score: f64, k: f64) -> Result<(), String> {
        let next = self.rating.update_vs(opponent, score, k)?;
        self.rating = next;
        self.games = self.games.saturating_add(1);
        Ok(())
    }
}

impl Default for EloState {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rating_validates_range() {
        assert!(EloRating::try_new(1_500.0).is_ok());
        assert!(EloRating::try_new(99.0).is_err());
        assert!(EloRating::try_new(3_001.0).is_err());
        assert!(EloRating::try_new(f64::NAN).is_err());
        assert!((EloRating::default_rating().as_f64() - 1_500.0).abs() < 1e-9);
    }

    #[test]
    fn expected_is_half_when_equal() {
        let a = EloRating::try_new(1_500.0).expect("fixture");
        let b = EloRating::try_new(1_500.0).expect("fixture");
        assert!((a.expected_vs(b) - 0.5).abs() < 1e-9);
        assert!((elo_expected(1_500.0, 1_500.0) - 0.5).abs() < 1e-9);
    }

    #[test]
    fn expected_favors_stronger() {
        let strong = EloRating::try_new(1_800.0).expect("fixture");
        let weak = EloRating::try_new(1_200.0).expect("fixture");
        assert!(strong.expected_vs(weak) > 0.9);
        assert!(weak.expected_vs(strong) < 0.1);
        assert!((0.0..=1.0).contains(&elo_expected(100.0, 3_000.0)));
        assert!((0.0..=1.0).contains(&elo_expected(3_000.0, 100.0)));
    }

    #[test]
    fn update_win_increases_loss_decreases() {
        let player = 1_500.0;
        let item = 1_500.0;
        let win = elo_update(player, item, 1.0, 32.0).expect("win");
        let loss = elo_update(player, item, 0.0, 32.0).expect("loss");
        let draw = elo_update(player, item, 0.5, 32.0).expect("draw");
        assert!(win > player, "{win} > {player}");
        assert!(loss < player, "{loss} < {player}");
        assert!((draw - player).abs() < 1e-9, "tablas contra igual no mueve");
        assert!((win - 1_516.0).abs() < 1e-9, "1500+32*0.5=1516, fue {win}");
        assert!((loss - 1_484.0).abs() < 1e-9, "1500-16=1484, fue {loss}");
    }

    #[test]
    fn update_rejects_bad_score_and_k() {
        assert!(elo_update(1_500.0, 1_500.0, 0.7, 32.0).is_err());
        assert!(elo_update(1_500.0, 1_500.0, 1.0, 0.0).is_err());
        assert!(elo_update(1_500.0, 1_500.0, 1.0, 100.0).is_err());
        assert!(elo_update(f64::NAN, 1_500.0, 1.0, 32.0).is_err());
        assert!(elo_update(50.0, 1_500.0, 1.0, 32.0).is_err());
    }

    #[test]
    fn update_clamps_to_bounds() {
        let top_win = elo_update(2_999.0, 100.0, 1.0, 64.0).expect("clamp alto");
        assert!(top_win <= ELO_MAX_RATING);
        let low_loss = elo_update(101.0, 3_000.0, 0.0, 64.0).expect("clamp bajo");
        assert!(low_loss >= ELO_MIN_RATING);
    }

    #[test]
    fn state_records_and_counts_games() {
        let mut state = EloState::new();
        let item = EloRating::try_new(1_500.0).expect("fixture");
        state.record(item, 1.0, 32.0).expect("win");
        assert_eq!(state.games, 1);
        assert!(state.rating.as_f64() > 1_500.0);
        assert!(state.record(item, 0.7, 32.0).is_err());
        assert_eq!(state.games, 1, "fallo no debe contar partida");
    }

    #[test]
    fn upset_moves_more_than_expected_win() {
        // Ganarle a uno mucho más fuerte mueve más que ganarle a un igual.
        let weak_win_strong = elo_update(1_200.0, 1_800.0, 1.0, 32.0).expect("upset");
        let equal_win = elo_update(1_500.0, 1_500.0, 1.0, 32.0).expect("equal");
        assert!(weak_win_strong - 1_200.0 > equal_win - 1_500.0);
    }

    #[test]
    fn elo_serde_roundtrip() {
        let rating = EloRating::try_new(1_623.5).expect("fixture");
        let json = serde_json::to_string(&rating).expect("serialize");
        let back: EloRating = serde_json::from_str(&json).expect("deserialize");
        assert_eq!(rating, back);
    }
}
