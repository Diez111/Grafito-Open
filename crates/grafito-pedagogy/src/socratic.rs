//! FSM socrático — guía heurística con guardas pedagógicas.
//!
//! Política:
//! - No revelar solución directa si `attempts < 2` (`TellingTooEarly`).
//! - Éxito requiere al menos 1 intento (`attempts >= 1`).
//! - Máximo 3 intentos, luego `Summarize` (`TooManyAttempts`).

use serde::{Deserialize, Serialize};

/// Estado del diálogo socrático.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum SocraticState {
    /// Revisión del objetivo de aprendizaje.
    Review { lo_id: String },
    /// Pregunta heurística — el estudiante debe razonar.
    HeuristicQ { attempts: u8 },
    /// Esperando respuesta del estudiante hasta un deadline.
    AwaitStudent { deadline_epoch: u64 },
    /// Rectificación de un misconception detectado.
    Rectify { misconception: String },
    /// Resumen y consolidación.
    Summarize,
    /// Finalizado.
    Done,
}

/// Errores de guarda del FSM.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum GuardError {
    #[error("revelar solución demasiado temprano: se requieren al menos 2 intentos")]
    TellingTooEarly,
    #[error("demasiados intentos: máximo 3, ahora en resumen")]
    TooManyAttempts,
    #[error("transición inválida: {0}")]
    InvalidTransition(String),
    #[error("ya finalizado")]
    AlreadyDone,
    #[error("se requiere al menos 1 intento para marcar éxito")]
    NotEnoughAttempts,
}

/// FSM socrático puro, sin I/O.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SocraticFsm {
    pub state: SocraticState,
    pub topic: String,
    pub attempts: u8,
    pub history: Vec<String>,
}

impl SocraticFsm {
    /// Crea un FSM en estado `Review` para el tema dado.
    pub fn new(topic: impl Into<String>) -> Self {
        let t = topic.into();
        let lo = t.clone();
        Self {
            state: SocraticState::Review { lo_id: lo },
            topic: t,
            attempts: 0,
            history: Vec::new(),
        }
    }

    /// ¿Se puede revelar la respuesta directa? (`attempts >= 2`)
    pub fn can_reveal_answer(&self) -> bool {
        self.attempts >= 2
    }

    /// Intenta revelar con guía. Falla con `TellingTooEarly` si `attempts < 2`.
    pub fn answer_with_guidance(&self) -> Result<String, GuardError> {
        if !self.can_reveal_answer() {
            return Err(GuardError::TellingTooEarly);
        }
        Ok(format!(
            "Guía para '{}': repasemos los pasos clave sin dar la solución directa de golpe. ¿Qué probaste en el intento {}? Revisemos juntos el razonamiento.",
            self.topic,
            self.attempts
        ))
    }

    /// Alias de `answer_with_guidance` para compatibilidad.
    pub fn try_reveal(&self) -> Result<String, GuardError> {
        self.answer_with_guidance()
    }

    /// Genera la siguiente pregunta heurística.
    ///
    /// - Si ya está en `Done` → `AlreadyDone`.
    /// - Si `attempts >= 3` → transiciona a `Summarize` y retorna `TooManyAttempts`.
    /// - En otro caso avanza a `HeuristicQ {attempts}` o mantiene `Review` → `HeuristicQ`.
    pub fn ask(&mut self) -> Result<SocraticState, GuardError> {
        if matches!(self.state, SocraticState::Done) {
            return Err(GuardError::AlreadyDone);
        }
        if self.attempts >= 3 {
            self.state = SocraticState::Summarize;
            return Err(GuardError::TooManyAttempts);
        }
        // Si está en Review, pasa a HeuristicQ; si ya está en HeuristicQ/AwaitStudent/Rectify, regenera pregunta.
        let next = SocraticState::HeuristicQ {
            attempts: self.attempts,
        };
        self.state = next.clone();
        self.history
            .push(format!("ask heuristic attempts={}", self.attempts));
        Ok(next)
    }

    /// Registra un intento del estudiante.
    ///
    /// - Incrementa `attempts` (cap 255).
    /// - Guarda el `misconception` si existe.
    /// - Si `misconception` presente y `attempts < 3` → `Rectify`.
    /// - Si `attempts >= 3` → `Summarize`.
    /// - Si no, pasa a `AwaitStudent` o `HeuristicQ`.
    pub fn record_attempt(&mut self, misconception: Option<String>) {
        self.attempts = self.attempts.saturating_add(1);
        if let Some(ref m) = misconception {
            if !m.is_empty() {
                self.history.push(format!("misconception: {m}"));
            }
        } else {
            self.history.push("intento sin misconception".to_string());
        }

        if self.attempts >= 3 {
            self.state = SocraticState::Summarize;
            return;
        }
        if let Some(m) = misconception {
            if !m.trim().is_empty() {
                self.state = SocraticState::Rectify { misconception: m };
                return;
            }
        }
        // Por defecto espera respuesta del estudiante con deadline heurístico
        // (ahora + 5 min) si no hay misconception.
        // Mantenemos HeuristicQ para siguiente pregunta.
        self.state = SocraticState::HeuristicQ {
            attempts: self.attempts,
        };
    }

    /// Marca éxito. Requiere `attempts >= 1`, caso contrario `TellingTooEarly`/`NotEnoughAttempts`.
    /// En éxito transiciona a `Summarize`.
    pub fn mark_success(&mut self) -> Result<SocraticState, GuardError> {
        if matches!(self.state, SocraticState::Done) {
            return Err(GuardError::AlreadyDone);
        }
        if self.attempts < 1 {
            return Err(GuardError::NotEnoughAttempts);
        }
        self.state = SocraticState::Summarize;
        self.history.push("éxito marcado".to_string());
        Ok(self.state.clone())
    }

    /// Alias para `mark_success` con nombre alternativo.
    pub fn succeed(&mut self) -> Result<SocraticState, GuardError> {
        self.mark_success()
    }

    /// Avanza a resumen.
    pub fn summarize(&mut self) -> SocraticState {
        self.state = SocraticState::Summarize;
        self.history.push("summarize".to_string());
        self.state.clone()
    }

    /// Finaliza el FSM.
    pub fn finish(&mut self) -> SocraticState {
        self.state = SocraticState::Done;
        self.history.push("done".to_string());
        self.state.clone()
    }

    /// Pone al FSM en espera de estudiante con deadline.
    pub fn await_student(&mut self, deadline_epoch: u64) {
        self.state = SocraticState::AwaitStudent { deadline_epoch };
        self.history
            .push(format!("await deadline {deadline_epoch}"));
    }

    /// Transiciona a rectificación explícita.
    pub fn rectify(&mut self, misconception: String) {
        self.state = SocraticState::Rectify {
            misconception: misconception.clone(),
        };
        self.history.push(format!("rectify {misconception}"));
    }

    /// Revela con guía solo si `can_reveal_answer` es verdadero.
    pub fn reveal_with_guidance(&self) -> Result<String, GuardError> {
        self.answer_with_guidance()
    }

    /// Devuelve el estado actual.
    pub fn current_state(&self) -> &SocraticState {
        &self.state
    }

    /// Indica si está en `Done`.
    pub fn is_done(&self) -> bool {
        matches!(self.state, SocraticState::Done)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn new_starts_in_review() {
        let fsm = SocraticFsm::new("derivada");
        assert_eq!(fsm.topic, "derivada");
        assert_eq!(fsm.attempts, 0);
        assert!(matches!(fsm.state, SocraticState::Review { .. }));
        assert!(!fsm.can_reveal_answer());
    }

    #[test]
    fn can_reveal_requires_two_attempts() {
        let mut fsm = SocraticFsm::new("integral");
        assert!(!fsm.can_reveal_answer());
        fsm.record_attempt(None);
        assert!(!fsm.can_reveal_answer());
        fsm.record_attempt(None);
        assert!(fsm.can_reveal_answer());
        assert!(fsm.answer_with_guidance().is_ok());
    }

    #[test]
    fn telling_too_early_on_reveal() {
        let fsm = SocraticFsm::new("límite");
        let err = fsm.answer_with_guidance().unwrap_err();
        assert_eq!(err, GuardError::TellingTooEarly);
        // también via try_reveal y reveal_with_guidance
        assert_eq!(fsm.try_reveal().unwrap_err(), GuardError::TellingTooEarly);
        assert_eq!(
            fsm.reveal_with_guidance().unwrap_err(),
            GuardError::TellingTooEarly
        );
    }

    #[test]
    fn ask_heuristics_and_attempts() {
        let mut fsm = SocraticFsm::new("función");
        // primer ask debe dar HeuristicQ attempts 0
        let s = fsm.ask().expect("primer ask ok");
        assert_eq!(s, SocraticState::HeuristicQ { attempts: 0 });
        fsm.record_attempt(None);
        assert_eq!(fsm.attempts, 1);
        let s2 = fsm.ask().expect("segundo ask ok");
        assert_eq!(s2, SocraticState::HeuristicQ { attempts: 1 });
    }

    #[test]
    fn success_requires_at_least_one_attempt() {
        let mut fsm = SocraticFsm::new("pitágoras");
        assert_eq!(
            fsm.mark_success().unwrap_err(),
            GuardError::NotEnoughAttempts
        );
        fsm.record_attempt(None);
        assert!(fsm.mark_success().is_ok());
        assert!(matches!(fsm.state, SocraticState::Summarize));
    }

    #[test]
    fn max_three_attempts_then_summarize() {
        let mut fsm = SocraticFsm::new("vectores");
        fsm.record_attempt(None); // 1 -> HeuristicQ 1
        fsm.record_attempt(Some("sign".to_string())); // 2 -> Rectify
        assert!(matches!(fsm.state, SocraticState::Rectify { .. }));
        fsm.record_attempt(None); // 3 -> Summarize
        assert!(matches!(fsm.state, SocraticState::Summarize));
        // cuarto intento ya en summarize, ask debe dar TooManyAttempts
        let err = fsm.ask().unwrap_err();
        assert_eq!(err, GuardError::TooManyAttempts);
        assert!(matches!(fsm.state, SocraticState::Summarize));
    }

    #[test]
    fn rectify_on_misconception() {
        let mut fsm = SocraticFsm::new("fracciones");
        fsm.record_attempt(Some("fracción".to_string()));
        assert!(
            matches!(fsm.state, SocraticState::Rectify { misconception } if misconception == "fracción")
        );
        assert_eq!(
            fsm.history
                .iter()
                .filter(|h| h.contains("misconception"))
                .count(),
            1
        );
    }

    #[test]
    fn await_and_finish() {
        let mut fsm = SocraticFsm::new("probabilidad");
        fsm.await_student(9999);
        assert_eq!(
            fsm.state,
            SocraticState::AwaitStudent {
                deadline_epoch: 9999
            }
        );
        fsm.finish();
        assert!(fsm.is_done());
        assert_eq!(fsm.ask().unwrap_err(), GuardError::AlreadyDone);
    }

    #[test]
    fn answer_with_guidance_ok_after_two() {
        let mut fsm = SocraticFsm::new("series");
        fsm.record_attempt(None);
        fsm.record_attempt(None);
        let g = fsm.answer_with_guidance().expect("debe revelar con guía");
        assert!(g.contains("series") || g.contains("Guía"));
    }

    #[test]
    fn ask_after_done_is_already_done() {
        let mut fsm = SocraticFsm::new("matrices");
        fsm.finish();
        assert_eq!(fsm.ask().unwrap_err(), GuardError::AlreadyDone);
        assert_eq!(fsm.mark_success().unwrap_err(), GuardError::AlreadyDone);
    }

    #[test]
    fn too_many_attempts_guard() {
        let mut fsm = SocraticFsm::new("taylor");
        for _ in 0..3 {
            fsm.record_attempt(None);
        }
        assert!(matches!(fsm.state, SocraticState::Summarize));
        // ask ahora debe fallar
        assert_eq!(fsm.ask().unwrap_err(), GuardError::TooManyAttempts);
    }

    #[test]
    fn succeed_alias_works() {
        let mut fsm = SocraticFsm::new("edo");
        fsm.record_attempt(None);
        assert!(fsm.succeed().is_ok());
    }
}
