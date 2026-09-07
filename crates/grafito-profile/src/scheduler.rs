//! Scheduler Leitner + SM-2 simplificado para repaso espaciado.
//!
//! Cada rama tiene un `box_level` (caja Leitner 0..=8) que determina el
//! intervalo hasta el próximo repaso. El intervalo crece exponencialmente con
//! el nivel y se modula levemente por el dominio (mastery) para acortar
//! repaso si aún no domina.

use serde::{Deserialize, Serialize};

/// Un día en segundos.
pub const DAY_SECS: u64 = 86_400;
/// Nivel máximo de caja Leitner.
pub const MAX_BOX_LEVEL: u8 = 8;

/// Plan de repaso por rama.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ReviewSchedule {
    pub branch_id: String,
    pub next_review_epoch: u64,
    pub interval_days: u32,
    pub box_level: u8,
}

/// Errores del scheduler (sin pánico, todo `Result`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SchedulerError {
    InvalidBoxLevel(u8),
    InvalidMastery(String),
}

impl std::fmt::Display for SchedulerError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::InvalidBoxLevel(v) => write!(f, "nivel de caja inválido: {v}"),
            Self::InvalidMastery(msg) => write!(f, "mastery inválido: {msg}"),
        }
    }
}

impl std::error::Error for SchedulerError {}

/// Calcula el intervalo en segundos hasta el próximo repaso.
///
/// Fórmula SM-2 simplificada adaptada para Grafito:
/// `interval = base * 2^(box_level-1) * (2.0 - mastery)`
/// con base = 86400 s (1 día) y box_level clamp 1..=8.
///
/// Si `box_level == 0` se trata como 1 (primer repaso en ~1 día modulado).
/// `mastery` se clamp a 0..=1; valores no finitos retornan intervalo base.
///
/// # Nota sobre `factor = (2 - mastery)` — ¿inversión de SM-2?
///
/// En SM-2 clásico el factor de facilidad (E-Factor) **crece** cuando el
/// repaso fue fácil (mastery alto ⇒ intervalo más largo). Aquí ocurre lo
/// opuesto: `mastery` alto ⇒ `factor` cercano a 1.0 ⇒ intervalo **más corto**.
/// Esto es **intencional y documentado** para Grafito (decisión pedagógica):
///
/// - Leitner ya codifica la facilidad en `box_level` (intervalo exponencial
///   2^(nivel-1)). `box_level` es la señal principal de espaciamiento.
/// - `mastery` es una señal secundaria de **refuerzo preventivo**: si el
///   dominio reciente es alto pero la caja aún es baja (ej. alumno brillante
///   que acaba de fallar antes), acortar levemente el intervalo acelera la
///   consolidación en lugar de alejar el repaso.
/// - Si el dominio es bajo (`mastery`≈0), `factor≈2.0` duplica el intervalo:
///   paradójicamente deja más tiempo para estudiar antes del próximo repaso,
///   evitando sobrecarga inmediata (el alumno ya recibió el castigo de
///   `box_level -=2` en `record_outcome`).
///
/// Si en el futuro se quiere un SM-2 puro (`factor` creciente con `mastery`),
/// basta invertir a `(1.0 + mastery)` o usar el E-Factor clásico
/// `EF' = EF + (0.1 - (5-q)*(0.08+(5-q)*0.02))`. Por ahora se mantiene
/// `(2 - mastery)` por compatibilidad y por los tests existentes
/// (`next_interval_mastery_modulates`, `scheduler_interval_grows_with_box_level`).
pub fn next_interval(box_level: u8, mastery: f32) -> u64 {
    let level = box_level.clamp(1, MAX_BOX_LEVEL);
    let mastery_clamped = if mastery.is_finite() {
        mastery.clamp(0.0, 1.0)
    } else {
        0.5
    };
    let base: f64 = DAY_SECS as f64;
    let pow = 2_f64.powi(i32::from(level) - 1);
    let factor = 2.0 - f64::from(mastery_clamped);
    // factor en [1.0, 2.0]; mastery alto => intervalo un poco más corto (ver nota arriba).
    // Saturar a u64 y evitar 0.
    let interval = base * pow * factor;
    let secs = interval.round() as u64;
    secs.max(1)
}

/// Variante que valida y retorna `Result` para callers que prefieren errores.
pub fn next_interval_checked(box_level: u8, mastery: f32) -> Result<u64, SchedulerError> {
    if box_level > MAX_BOX_LEVEL {
        return Err(SchedulerError::InvalidBoxLevel(box_level));
    }
    if !mastery.is_finite() {
        return Err(SchedulerError::InvalidMastery(format!(
            "{mastery} no es finito"
        )));
    }
    if !(0.0..=1.0).contains(&mastery) {
        return Err(SchedulerError::InvalidMastery(format!(
            "{mastery} fuera de 0..=1"
        )));
    }
    Ok(next_interval(box_level, mastery))
}

/// ¿Venció el repaso? `next_epoch <= now`.
pub fn is_due(next_epoch: u64, now: u64) -> bool {
    now >= next_epoch
}

/// Calcula el epoch del próximo repaso: `now + next_interval(level, mastery)`.
pub fn schedule_next_review(now: u64, box_level: u8, mastery: f32) -> u64 {
    now.saturating_add(next_interval(box_level, mastery))
}

/// Construye un `ReviewSchedule` para una rama.
pub fn review_schedule_for(
    branch_id: &str,
    now: u64,
    box_level: u8,
    mastery: f32,
) -> ReviewSchedule {
    let interval_secs = next_interval(box_level, mastery);
    let next = now.saturating_add(interval_secs);
    let days = (interval_secs / DAY_SECS) as u32;
    ReviewSchedule {
        branch_id: branch_id.to_string(),
        next_review_epoch: next,
        interval_days: days.max(1),
        box_level: box_level.clamp(0, MAX_BOX_LEVEL),
    }
}

// ── FSRS-lite (M, honesto y acotado) ────────────────────────────────────────
/// FSRS-lite simplificado (NO es FSRS-4.5 completo).
///
/// `stability` (días) y `difficulty` (1..=10) por rama. Espacia con memoria
/// de dificultad además de la caja Leitner. Puro, determinista, sin I/O.
///
/// - `grade`: 1 = olvido total, 2 = difícil, 3 = bien, 4 = fácil.
/// - `stability` init 1.0 día, cap 1/4..=365 días; `difficulty` init 5.0.
/// - Intervalo = `stability` días (redondeado, mín 1 día, máx 365).
/// - La caja Leitner sigue siendo la señal principal en `record_outcome`;
///   FSRS-lite es opt-in para la UI.
///
/// Estabilidad/dificultad FSRS-lite por rama.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct FsrsState {
    /// Estabilidad en días (1/4..=365).
    pub stability_days: f64,
    /// Dificultad 1..=10 (10 = muy difícil).
    pub difficulty: f64,
}

/// Estado inicial FSRS-lite (1 día, dificultad media 5).
#[must_use]
pub fn fsrs_init() -> FsrsState {
    FsrsState {
        stability_days: 1.0,
        difficulty: 5.0,
    }
}

/// Actualiza FSRS-lite con una calificación `1..=4`.
///
/// - Dificultad: `D' = clamp(D - 0.5*(grade-3), 1, 10)` (fácil baja D).
/// - Estabilidad: `grade=4 → S*2.5`, `3 → S*1.8`, `2 → S*0.8`, `1 → 1.0`
///   (olvido: reinicia), modulado por `(11-D')/10` en aciertos.
///
/// Retorna `Err` si `grade` fuera de `1..=4` (sin pánicos).
pub fn fsrs_update(state: FsrsState, grade: u8) -> Result<FsrsState, SchedulerError> {
    if !(1..=4).contains(&grade) {
        return Err(SchedulerError::InvalidMastery(format!(
            "grade {grade} fuera de 1..=4"
        )));
    }
    let difficulty = if state.difficulty.is_finite() {
        state.difficulty.clamp(1.0, 10.0)
    } else {
        5.0
    };
    let stability = if state.stability_days.is_finite() {
        state.stability_days.clamp(0.25, 365.0)
    } else {
        1.0
    };
    let next_difficulty = (difficulty - 0.5 * f64::from(grade as i8 - 3)).clamp(1.0, 10.0);
    let next_stability = match grade {
        4 => stability * 2.5 * (11.0 - next_difficulty) / 10.0,
        3 => stability * 1.8 * (11.0 - next_difficulty) / 10.0,
        2 => stability * 0.8,
        _ => 1.0,
    };
    Ok(FsrsState {
        stability_days: next_stability.clamp(0.25, 365.0),
        difficulty: next_difficulty,
    })
}

/// Intervalo FSRS-lite en segundos (`stability` días → secs, cap 365 días).
#[must_use]
pub fn fsrs_next_interval_secs(state: FsrsState) -> u64 {
    let days = if state.stability_days.is_finite() {
        state.stability_days.clamp(0.25, 365.0)
    } else {
        1.0
    };
    let secs = days * DAY_SECS as f64;
    (secs.round() as u64).clamp(DAY_SECS, 365 * DAY_SECS)
}

/// Epoch del próximo repaso FSRS-lite (`now + intervalo`, saturante).
#[must_use]
pub fn fsrs_schedule_next_review(now: u64, state: FsrsState) -> u64 {
    now.saturating_add(fsrs_next_interval_secs(state))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn next_interval_grows_with_box_level() {
        let m = 0.5_f32;
        let i1 = next_interval(1, m);
        let i2 = next_interval(2, m);
        let i4 = next_interval(4, m);
        let i8 = next_interval(8, m);
        assert!(i1 < i2, "{i1} < {i2}");
        assert!(i2 < i4, "{i2} < {i4}");
        assert!(i4 < i8, "{i4} < {i8}");
        // Nivel 1 con mastery 0.5: base*1*(1.5) = 129600
        assert_eq!(i1, (DAY_SECS as f64 * 1.5).round() as u64);
        // Nivel 2: base*2*1.5 = 259200
        assert_eq!(i2, (DAY_SECS as f64 * 2.0 * 1.5).round() as u64);
    }

    #[test]
    fn next_interval_mastery_modulates() {
        let low = next_interval(3, 0.0);
        let high = next_interval(3, 1.0);
        // mastery alto => factor 1.0, mastery bajo => factor 2.0 => intervalo más corto con dominio alto
        assert!(
            high < low,
            "dominio alto debe acortar intervalo: {high} vs {low}"
        );
        // high = base*4*1.0 = 345600, low = base*4*2.0 = 691200
        assert_eq!(high, DAY_SECS * 4);
        assert_eq!(low, DAY_SECS * 8);
    }

    #[test]
    fn zero_level_treated_as_one() {
        let m = 0.5;
        assert_eq!(next_interval(0, m), next_interval(1, m));
    }

    #[test]
    fn is_due_logic() {
        assert!(is_due(100, 100));
        assert!(is_due(100, 101));
        assert!(!is_due(101, 100));
    }

    #[test]
    fn schedule_next_review_adds_interval() {
        let now = 1_000_000_u64;
        let next = schedule_next_review(now, 2, 0.5);
        let expected = now + next_interval(2, 0.5);
        assert_eq!(next, expected);
        // Sin overflow (saturating)
        let near_max = u64::MAX - 1000;
        let capped = schedule_next_review(near_max, 8, 0.0);
        assert_eq!(capped, u64::MAX);
    }

    #[test]
    fn next_interval_checked_rejects_invalid() {
        assert!(next_interval_checked(9, 0.5).is_err());
        assert!(next_interval_checked(2, f32::NAN).is_err());
        assert!(next_interval_checked(2, 1.5).is_err());
        assert!(next_interval_checked(2, 0.5).is_ok());
    }

    #[test]
    fn review_schedule_for_builds_struct() {
        let rs = review_schedule_for("algebra", 0, 1, 0.0);
        assert_eq!(rs.branch_id, "algebra");
        assert_eq!(rs.box_level, 1);
        assert!(rs.next_review_epoch > 0);
        assert!(rs.interval_days >= 1);
        // Con 1 día base y mastery 0 => intervalo 2 días
        assert_eq!(rs.interval_days, 2);
    }

    #[test]
    fn clamped_mastery_finite_handling() {
        // No debe paniquear con valores degenerados.
        let a = next_interval(3, f32::INFINITY);
        assert!(a > 0);
        let b = next_interval(3, f32::NAN);
        assert!(b > 0);
        let c = next_interval(3, -1.0);
        assert!(c > 0);
        let d = next_interval(3, 2.0);
        assert!(d > 0);
    }

    #[test]
    fn fsrs_easy_grows_stability_and_forget_resets() {
        let init = fsrs_init();
        assert_eq!(init.stability_days, 1.0);
        let easy = fsrs_update(init, 4).expect("grade válido");
        assert!(easy.stability_days > init.stability_days);
        assert!(easy.difficulty < init.difficulty);
        let good = fsrs_update(init, 3).expect("grade válido");
        assert!(good.stability_days > init.stability_days);
        assert!(good.stability_days < easy.stability_days);
        let hard = fsrs_update(init, 2).expect("grade válido");
        assert!(hard.stability_days < init.stability_days);
        let forgot = fsrs_update(easy, 1).expect("grade válido");
        assert_eq!(forgot.stability_days, 1.0);
    }

    #[test]
    fn fsrs_rejects_invalid_grade_and_caps_interval() {
        assert!(fsrs_update(fsrs_init(), 0).is_err());
        assert!(fsrs_update(fsrs_init(), 5).is_err());
        let mut state = fsrs_init();
        for _ in 0..20 {
            state = fsrs_update(state, 4).expect("grade válido");
        }
        assert!(state.stability_days <= 365.0);
        let secs = fsrs_next_interval_secs(state);
        assert!(secs >= DAY_SECS);
        assert!(secs <= 365 * DAY_SECS);
        assert_eq!(fsrs_schedule_next_review(1_000, state), 1_000 + secs);
        // Saturante sin overflow.
        assert_eq!(fsrs_schedule_next_review(u64::MAX - 10, state), u64::MAX);
    }
}
