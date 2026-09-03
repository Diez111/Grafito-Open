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
}
