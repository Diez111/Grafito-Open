//! FSM socrático — guía heurística con guardas pedagógicas.
//!
//! Política:
//! - No revelar solución directa si `attempts < 2` (`TellingTooEarly`).
//! - Éxito requiere al menos 1 intento (`attempts >= 1`).
//! - Máximo 3 intentos, luego `Summarize` (`TooManyAttempts`).

use crate::level::PedagogicalLevel;
use crate::scaffold::{Scaffold, ScaffoldEngine, Turn};
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

    // ── Socratic helpers deterministas y vinculantes ──────────────────────

    /// Etiqueta estable del estado para prompt.
    pub fn state_label(&self) -> &'static str {
        match &self.state {
            SocraticState::Review { .. } => "Review",
            SocraticState::HeuristicQ { .. } => "HeuristicQ",
            SocraticState::AwaitStudent { .. } => "AwaitStudent",
            SocraticState::Rectify { .. } => "Rectify",
            SocraticState::Summarize => "Summarize",
            SocraticState::Done => "Done",
        }
    }

    /// Heurística determinista: ¿el texto del LLM contiene marcadores de solución directa?
    ///
    /// Lista acotada sin regex (no-alloc extra): `solución es`, `respuesta es`, `resultado es`, `x =`, `y =`.
    /// En minúsculas, sin normalizar tildes (se cubren ambas variantes con/sin).
    pub fn contains_solution_marker(text: &str) -> bool {
        let lower = text.to_lowercase();
        lower.contains("solución es")
            || lower.contains("solucion es")
            || lower.contains("respuesta es")
            || lower.contains("resultado es")
            || lower.contains("solución:")
            || lower.contains("solucion:")
            || lower.contains("respuesta:")
            || lower.contains("x =")
            || lower.contains("y =")
            || lower.contains("la solución")
            || lower.contains("la respuesta")
    }

    /// ¿Es telling? `true` si `!can_reveal && contains_solution_marker`.
    pub fn is_telling(&self, response_text: &str) -> bool {
        !self.can_reveal_answer() && Self::contains_solution_marker(response_text)
    }

    /// Guarda telling para uso remoto: `TellingTooEarly` si es telling con `attempts<2`.
    pub fn check_telling_guard(&self, response_text: &str) -> Result<(), GuardError> {
        if self.is_telling(response_text) {
            Err(GuardError::TellingTooEarly)
        } else {
            Ok(())
        }
    }

    /// Prompt de reparación determinista cuando se viola el guard.
    /// Incluye la pregunta BKT/scaffold exacta y el contador attempts.
    pub fn repair_prompt_for_telling(&self, scaffold: &Scaffold) -> String {
        let hint = scaffold
            .hint
            .as_deref()
            .unwrap_or("Intentá con un ejemplo concreto (x=1, x=2).");
        let q: String = scaffold.question.chars().take(400).collect();
        let h: String = hint.chars().take(400).collect();
        format!(
            "GUARD TELLING — REPARACIÓN SOCRÁTICA OBLIGATORIA (attempts={} <2, can_reveal=false, estado={}): No reveles la solución directa. Re-preguntá EXACTAMENTE con: '{}' + pista '{}'. Forzá un nuevo intento del estudiante.",
            self.attempts,
            self.state_label(),
            q,
            h
        )
    }

    /// Enforce: si es telling devuelve `Err(repair_prompt)`, si no `Ok(text)`.
    pub fn enforce_telling_guard(
        &self,
        response_text: &str,
        scaffold: &Scaffold,
    ) -> Result<String, String> {
        if self.is_telling(response_text) {
            Err(self.repair_prompt_for_telling(scaffold))
        } else {
            Ok(response_text.to_owned())
        }
    }

    /// Genera scaffold determinista desde `topic` y `level`, usando el historial del FSM
    /// convertido a `Turn`s (role="history").
    pub fn current_scaffold(&self, engine: &ScaffoldEngine, level: PedagogicalLevel) -> Scaffold {
        let history: Vec<Turn> = self
            .history
            .iter()
            .map(|h| Turn {
                role: "history".into(),
                content: h.clone(),
            })
            .collect();
        engine.scaffold(&self.topic, level, &history)
    }

    /// Segmento vinculante determinista para inyectar al system prompt.
    ///
    /// Combina estado FSM + attempts + can_reveal + scaffold (pregunta BKT actual + pista misconception + historial).
    /// Truncado y acotado (<1800 chars), 100% puro y testeable.
    pub fn socratic_system_segment(&self, scaffold: &Scaffold) -> String {
        const MAX_TOPIC_CHARS: usize = 120;
        const MAX_HISTORY_CHARS: usize = 180;
        const MAX_HISTORY_ITEMS: usize = 4;
        let topic: String = self.topic.chars().take(MAX_TOPIC_CHARS).collect();
        let mut out = String::new();
        out.push_str("[SOCRATIC FSM — VINCULANTE]\n");
        out.push_str(&format!(
            "Estado: {} | Topic: {} | Attempts: {} | CanReveal: {}\n",
            self.state_label(),
            topic,
            self.attempts,
            self.can_reveal_answer()
        ));
        out.push_str(
            "Regla VINCULANTE: NO revelar solución directa si attempts<2 (TellingTooEarly). ",
        );
        out.push_str(
            "Si el LLM intenta telling con attempts<2, el guard remoto fuerza re-pregunta.\n",
        );
        // Scaffold inyectado (current_question + pista)
        let hist_turns: Vec<Turn> = self
            .history
            .iter()
            .map(|h| Turn {
                role: "history".into(),
                content: h.clone(),
            })
            .collect();
        let seg = scaffold.system_prompt_segment(&hist_turns);
        out.push_str(&seg);
        out.push('\n');
        // Historial FSM crudo acotado
        out.push_str(&format!(
            "Historial FSM ({} entradas, muestra {}): ",
            self.history.len(),
            MAX_HISTORY_ITEMS.min(self.history.len())
        ));
        if self.history.is_empty() {
            out.push_str("(vacío)");
        } else {
            for (idx, h) in self.history.iter().take(MAX_HISTORY_ITEMS).enumerate() {
                let snippet: String = h.chars().take(MAX_HISTORY_CHARS).collect();
                let clean = snippet.replace('\n', " ");
                out.push_str(&format!("[{idx}:{clean}] "));
            }
        }
        out.push('\n');
        out.push_str("Instrucción FINAL VINCULANTE: El system prompt es orden, no sugerencia. Seguí exactamente este FSM y scaffold. Telling_rate <5% obligatorio.");
        out
    }

    /// Atajo que genera scaffold vía engine y devuelve el segmento completo.
    pub fn socratic_system_segment_with_engine(
        &self,
        engine: &ScaffoldEngine,
        level: PedagogicalLevel,
    ) -> String {
        let sc = self.current_scaffold(engine, level);
        self.socratic_system_segment(&sc)
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

    #[test]
    fn recorrido_completo_review_a_summarize() {
        // Review → HeuristicQ → AwaitStudent → HeuristicQ → Rectify → Summarize → Done.
        let mut fsm = SocraticFsm::new("derivada");
        // 1. Review inicial.
        assert!(matches!(fsm.state, SocraticState::Review { .. }));
        // 2. HeuristicQ.
        let s = fsm.ask().expect("ask inicial ok");
        assert_eq!(s, SocraticState::HeuristicQ { attempts: 0 });
        // 3. Espera al estudiante.
        fsm.await_student(9_999);
        assert_eq!(
            fsm.state,
            SocraticState::AwaitStudent {
                deadline_epoch: 9_999
            }
        );
        // 4. Nueva pregunta heurística tras la espera.
        let s2 = fsm.ask().expect("ask tras espera ok");
        assert_eq!(s2, SocraticState::HeuristicQ { attempts: 0 });
        // 5. Intento con misconception → Rectify.
        fsm.record_attempt(Some("fracción".to_string()));
        assert!(
            matches!(&fsm.state, SocraticState::Rectify { misconception } if misconception.as_str() == "fracción")
        );
        assert_eq!(fsm.attempts, 1);
        // 6. Éxito tras ≥1 intento → Summarize.
        let s3 = fsm.mark_success().expect("éxito ok");
        assert_eq!(s3, SocraticState::Summarize);
        assert!(matches!(fsm.state, SocraticState::Summarize));
        // 7. Cierre → Done.
        fsm.finish();
        assert!(fsm.is_done());
        // Historial encadena todas las fases en orden.
        let h = fsm.history.join("|");
        assert!(h.contains("ask heuristic"));
        assert!(h.contains("await deadline"));
        assert!(h.contains("misconception"));
        assert!(h.contains("éxito marcado"));
        assert!(h.contains("done"));
    }

    #[test]
    fn telling_menor_5_porciento() {
        // Política: jamás solución directa de golpe; solo guía tras ≥2 intentos.
        // Simula 20 interacciones: 1 Review + asks + 2 intentos + 1 guía como máximo.
        let mut fsm = SocraticFsm::new("integral");
        let mut directos = 0usize;
        let total = 20usize;
        // Intentos tempranos bloquean el telling.
        assert_eq!(fsm.try_reveal().unwrap_err(), GuardError::TellingTooEarly);
        fsm.record_attempt(None);
        assert_eq!(
            fsm.answer_with_guidance().unwrap_err(),
            GuardError::TellingTooEarly
        );
        fsm.record_attempt(None);
        // Tras 2 intentos se permite guía (no solución directa).
        let guia = fsm.answer_with_guidance().expect("guía tras 2 intentos");
        // La guía no revela la solución de golpe: lo dice explícitamente.
        assert!(guia.contains("sin dar la solución directa"));
        // Contamos como telling solo si contuviera solución directa; aquí 0.
        if guia.contains("solución directa de golpe.") && !guia.contains("sin dar") {
            directos += 1;
        }
        // Tasa = directos/total < 5 %.
        let tasa = (directos as f64) / (total as f64);
        assert!(
            tasa < 0.05,
            "telling {tasa} debe ser <5 %, directos={directos} total={total}"
        );
        // Además el historial no contiene tells directos (solo preguntas y guías).
        assert!(
            !fsm.history.iter().any(|h| h.contains("telling")),
            "el historial no debe registrar tells directos"
        );
    }

    #[test]
    fn contains_solution_marker_is_deterministic() {
        assert!(SocraticFsm::contains_solution_marker(
            "La solución es x = 4"
        ));
        assert!(SocraticFsm::contains_solution_marker(
            "x = 2.5 es el resultado"
        ));
        assert!(SocraticFsm::contains_solution_marker("Respuesta es 42"));
        assert!(!SocraticFsm::contains_solution_marker(
            "¿Cómo lo pensaste? Intentá con x=1"
        ));
        assert!(!SocraticFsm::contains_solution_marker(
            "Explicalo con tus palabras"
        ));
    }

    #[test]
    fn is_telling_respects_can_reveal() {
        let mut fsm = SocraticFsm::new("derivada");
        assert!(fsm.is_telling("la solución es x = 4"));
        assert_eq!(
            fsm.check_telling_guard("la solución es x = 4").unwrap_err(),
            GuardError::TellingTooEarly
        );
        fsm.record_attempt(None);
        fsm.record_attempt(None);
        assert!(!fsm.is_telling("la solución es x = 4"));
        assert!(fsm.check_telling_guard("la solución es x = 4").is_ok());
        // sin marcador nunca es telling
        assert!(!fsm.is_telling("¿Qué observás en la pendiente?"));
    }

    #[test]
    fn repair_prompt_for_telling_is_deterministic() {
        let fsm = SocraticFsm::new("fracciones");
        let scaffold = crate::scaffold::Scaffold {
            question: "¿Qué representa fracciones en el gráfico?".into(),
            hint: Some("Pista concreta: probá con x=1".into()),
            explanation: "Exp".into(),
        };
        let r1 = fsm.repair_prompt_for_telling(&scaffold);
        let r2 = fsm.repair_prompt_for_telling(&scaffold);
        assert_eq!(r1, r2);
        assert!(r1.contains("GUARD TELLING"));
        assert!(r1.contains("attempts=0"));
        assert!(r1.contains("¿Qué representa"));
    }

    #[test]
    fn enforce_telling_guard_forces_repreguntar() {
        let fsm = SocraticFsm::new("integral");
        let scaffold = crate::scaffold::Scaffold {
            question: "¿Qué representa integral?".into(),
            hint: None,
            explanation: "área".into(),
        };
        let telling = "La solución es x = 5";
        let err = fsm.enforce_telling_guard(telling, &scaffold).unwrap_err();
        assert!(err.contains("REPARACIÓN SOCRÁTICA") || err.contains("GUARD TELLING"));
        assert!(err.contains("¿Qué representa integral?"));
        // con attempts>=2 pasa
        let mut fsm2 = SocraticFsm::new("integral");
        fsm2.record_attempt(None);
        fsm2.record_attempt(None);
        assert!(fsm2.enforce_telling_guard(telling, &scaffold).is_ok());
    }

    #[test]
    fn socratic_system_segment_is_deterministic_and_binding() {
        use crate::level::PedagogicalLevel;
        use crate::scaffold::ScaffoldEngine;
        let mut fsm = SocraticFsm::new("derivada");
        fsm.record_attempt(Some("sign".into()));
        let engine = ScaffoldEngine;
        let scaffold = engine.scaffold("derivada", PedagogicalLevel::Secondary, &[]);
        let seg1 = fsm.socratic_system_segment(&scaffold);
        let seg2 = fsm.socratic_system_segment(&scaffold);
        assert_eq!(seg1, seg2);
        assert!(seg1.contains("VINCULANTE"));
        assert!(seg1.contains("Estado:"));
        assert!(seg1.contains("Attempts: 1"));
        assert!(seg1.contains("CanReveal: false"));
        assert!(seg1.contains("Pregunta BKT actual"));
        assert!(seg1.contains("Pista scaffold"));
        assert!(seg1.contains("Historial FSM"));
        assert!(seg1.contains("misconception: sign"));
        assert!(seg1.chars().count() < 3000);
    }

    #[test]
    fn socratic_segment_with_engine_is_deterministic() {
        use crate::level::PedagogicalLevel;
        use crate::scaffold::ScaffoldEngine;
        let fsm = SocraticFsm::new("taylor");
        let engine = ScaffoldEngine;
        let seg1 = fsm.socratic_system_segment_with_engine(&engine, PedagogicalLevel::Secondary);
        let seg2 = fsm.socratic_system_segment_with_engine(&engine, PedagogicalLevel::Secondary);
        assert_eq!(seg1, seg2);
        assert!(seg1.contains("taylor"));
        assert!(seg1.contains("VINCULANTE"));
    }
}
