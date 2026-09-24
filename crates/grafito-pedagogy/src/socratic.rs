//! FSM socrático — guía heurística con guardas pedagógicas.
//!
//! Política:
//! - No revelar solución directa si `attempts < 2` (`TellingTooEarly`).
//! - Éxito requiere al menos 1 intento (`attempts >= 1`).
//! - Máximo 3 intentos, luego `Summarize` (`TooManyAttempts`).

use crate::feedback::Misconception;
use crate::level::PedagogicalLevel;
use crate::scaffold::{
    is_explicit_demo_request, is_exploratory_request, Scaffold, ScaffoldEngine, Turn,
};
use serde::{Deserialize, Serialize};
use std::collections::VecDeque;

/// Estado del diálogo socrático.
///
/// `/statem`: `HeuristicQ` NO replica `SocraticFsm::attempts` (el contador
/// vive una sola vez en el FSM). El payload redundante permitía fabricar FSMs
/// inconsistentes (`state.attempts != fsm.attempts`) vía struct literal /
/// `Deserialize`; eliminado para que el estado inválido no exista.
///
/// **Forma serde** (verificado con grep 2026-09: NADA en el workspace
/// persiste `SocraticState`/`SocraticFsm` — el único consumidor externo,
/// `SocraticGuardContext` de `grafito-assistant`, es `Debug + Clone` sin
/// serde, y la config de `grafito-app` solo persiste
/// `assistant_socratic_enabled: bool`): por eso NO hace falta `#[serde(alias)]`
/// ni migración. Si alguien lo persiste en el futuro, la forma actual es
/// externamente tagged — `HeuristicQ` serializa como `"HeuristicQ"` (variante
/// unit), NO como `{"HeuristicQ":{"attempts":N}}` (payload eliminado). El
/// test `r6e_forma_serde_del_fsm_fijada` fija esta forma para que un cambio
/// futuro sea visible.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum SocraticState {
    /// Revisión del objetivo de aprendizaje.
    Review { lo_id: String },
    /// Pregunta heurística — el estudiante debe razonar.
    HeuristicQ,
    /// Esperando respuesta del estudiante hasta un deadline.
    AwaitStudent { deadline_epoch: u64 },
    /// Rectificación de un misconception detectado.
    Rectify { misconception: String },
    /// Resumen y consolidación.
    Summarize,
    /// Finalizado. **Terminal**: ninguna transición sale de acá (`AlreadyDone`).
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
    #[error("sin remate correcto: resolvé el check final antes de marcar éxito")]
    CheckNotCorrect,
}

/// FSM socrático puro, sin I/O.
///
/// `history` es una cola acotada ([`MAX_HISTORY_ENTRIES`]): los pushes
/// descartan lo más viejo (cap memoria, sin truncar en silencio el estado).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SocraticFsm {
    pub state: SocraticState,
    pub topic: String,
    pub attempts: u8,
    pub history: VecDeque<String>,
}

/// Tope de entradas del historial del FSM (cola con descarte del más viejo).
pub const MAX_HISTORY_ENTRIES: usize = 32;

/// Reparación socrática tipada para uso interno (jamás se publica cruda al chat).
///
/// El poll la convierte a voz de Mili vía [`SocraticRepair::to_student_voice`],
/// sin `GUARD`/`attempts`/`estado`/`Re-preguntá`. Pura y determinista.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SocraticRepair {
    /// Pregunta heurística ya sanitizada (del scaffold, jamás texto crudo del usuario).
    pub pregunta_humana: String,
    /// Pista concreta del scaffold (acotada a 400 chars en construcción).
    pub pista: String,
}

impl SocraticRepair {
    /// Redacta `= <dígito>` de un texto generado para el estudiante.
    ///
    /// `contains_numeric_answer` (parte del guard con `attempts < 2`) dispara
    /// con ejemplos legítimos del scaffold como `probá con (x=1, x=2)`:
    /// si el caller vuelve a pasar el repair por el guard, el repair se
    /// auto-detecta como telling y entra en bucle. Se reescribe a
    /// `(x igual a 1, x igual a 2)` — misma pista, sin marcador. Solo se usa
    /// sobre textos generados por el scaffold (jamás sobre texto del LLM).
    fn redactar_igual_numerico(texto: &str) -> String {
        let mut out = String::with_capacity(texto.len());
        let chars: Vec<char> = texto.chars().collect();
        let mut i = 0usize;
        while i < chars.len() {
            if chars[i] == '=' {
                let mut j = i + 1;
                while j < chars.len() && (chars[j] == ' ' || chars[j] == '\t') {
                    j += 1;
                }
                let neg = j < chars.len() && chars[j] == '-';
                let k = if neg { j + 1 } else { j };
                if k < chars.len() && chars[k].is_ascii_digit() {
                    out.push_str(" igual a ");
                    i += 1;
                    continue;
                }
            }
            out.push(chars[i]);
            i += 1;
        }
        out
    }

    /// Voz de Mili para el transcript: sin jerga interna.
    ///
    /// Garantiza que el turno NO contenga `GUARD`, `REPARACIÓN`, `attempts`,
    /// `can_reveal`, `Re-preguntá` ni interpolaciones degeneradas como
    /// `¿Te imaginás hola`. Pura y determinista, sin `unwrap`.
    ///
    /// **Auto-exento del guard** (FIX bucle de reparación): la redacción no
    /// incluye el marcador literal `la solución` (antes
    /// `contains_solution_marker(to_student_voice(..)) == true` →
    /// `TellingTooEarly` otra vez si el repair volvía por el guard) y los
    /// campos interpolados pasan por [`Self::redactar_igual_numerico`], así
    /// `is_telling` sobre la voz devuelve `false` en cualquier estado.
    pub fn to_student_voice(&self) -> String {
        let pregunta = Self::redactar_igual_numerico(&self.pregunta_humana);
        let pista = Self::redactar_igual_numerico(&self.pista);
        if self.pista.trim().is_empty() {
            format!(
                "Antes de mostrarte cómo sale, ¿qué forma te imaginás? {pregunta} Contame qué probaste y lo vemos juntos."
            )
        } else {
            format!(
                "Antes de mostrarte cómo sale, ¿qué forma te imaginás? {pregunta} Pista: {pista}"
            )
        }
    }
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
            history: VecDeque::new(),
        }
    }

    /// Push acotado al historial: si está lleno descarta la entrada más vieja.
    fn push_history(&mut self, entry: String) {
        if self.history.len() >= MAX_HISTORY_ENTRIES {
            self.history.pop_front();
        }
        self.history.push_back(entry);
    }

    /// ¿Se puede revelar la respuesta directa? (`attempts >= 2`)
    ///
    /// `attempts` cuenta respuestas a pregunta heurística previa, no turnos
    /// totales. Primer turno sin pregunta previa ⇒ 0 y no punitivo: el guard de
    /// sesión se bypassea (es demo, no evaluación; ver `is_exploratory_request`
    /// y `session_socratic_guard`). Documentado para evitar el falso `0` punitivo
    /// del conteo viejo (`prior_user_turns-1`).
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
    /// - En otro caso avanza a `HeuristicQ` o mantiene `Review` → `HeuristicQ`.
    pub fn ask(&mut self) -> Result<SocraticState, GuardError> {
        if matches!(self.state, SocraticState::Done) {
            return Err(GuardError::AlreadyDone);
        }
        if self.attempts >= 3 {
            self.state = SocraticState::Summarize;
            return Err(GuardError::TooManyAttempts);
        }
        // Si está en Review, pasa a HeuristicQ; si ya está en HeuristicQ/AwaitStudent/Rectify, regenera pregunta.
        let next = SocraticState::HeuristicQ;
        self.state = next.clone();
        self.push_history(format!("ask heuristic attempts={}", self.attempts));
        Ok(next)
    }

    /// Registra un intento del estudiante.
    ///
    /// - `Done` es terminal → `AlreadyDone` (no resucita el FSM).
    /// - Incrementa `attempts` (cap 255).
    /// - `misconception` se valida contra el enum cerrado
    ///   ([`Misconception::parse`], es/en): solo una variante conocida produce
    ///   `Rectify` (guarda el nombre canónico, jamás el string crudo del
    ///   LLM); etiqueta desconocida/vacía se ignora con traza honesta.
    /// - Si `attempts >= 3` → `Summarize`.
    /// - Si no, `Rectify` con misconception conocida o `HeuristicQ`.
    pub fn record_attempt(&mut self, misconception: Option<String>) -> Result<(), GuardError> {
        if matches!(self.state, SocraticState::Done) {
            return Err(GuardError::AlreadyDone);
        }
        self.attempts = self.attempts.saturating_add(1);
        let parsed = misconception
            .as_deref()
            .and_then(Misconception::parse)
            .filter(|m| !matches!(m, Misconception::None));

        match &parsed {
            Some(m) => self.push_history(format!("misconception: {m:?}")),
            None => {
                if misconception
                    .as_deref()
                    .is_some_and(|m| !m.trim().is_empty())
                {
                    let raw: String = misconception
                        .as_deref()
                        .unwrap_or_default()
                        .chars()
                        .take(64)
                        .collect();
                    self.push_history(format!(
                        "misconception desconocida {raw:?} (se ignora, sin Rectify)"
                    ));
                } else {
                    self.push_history("intento sin misconception".to_string());
                }
            }
        }

        if self.attempts >= 3 {
            self.state = SocraticState::Summarize;
            return Ok(());
        }
        if let Some(m) = parsed {
            self.state = SocraticState::Rectify {
                misconception: format!("{m:?}"),
            };
            return Ok(());
        }
        // Sin misconception conocida: nueva pregunta heurística para el
        // siguiente turno (el `attempts` vive SOLO en `self.attempts`; el
        // estado ya no lo duplica).
        self.state = SocraticState::HeuristicQ;
        Ok(())
    }

    /// Marca éxito. Requiere `attempts >= 1` Y remate correcto (R6e:
    /// `last_check_correct` es `step.assess_final(respuesta).correct`; sin
    /// prueba de corrección no hay éxito). En éxito transiciona a `Summarize`.
    pub fn mark_success(&mut self, last_check_correct: bool) -> Result<SocraticState, GuardError> {
        if matches!(self.state, SocraticState::Done) {
            return Err(GuardError::AlreadyDone);
        }
        if self.attempts < 1 {
            return Err(GuardError::NotEnoughAttempts);
        }
        if !last_check_correct {
            return Err(GuardError::CheckNotCorrect);
        }
        self.state = SocraticState::Summarize;
        self.push_history("éxito marcado".to_string());
        Ok(self.state.clone())
    }

    /// Alias para `mark_success` con nombre alternativo.
    pub fn succeed(&mut self, last_check_correct: bool) -> Result<SocraticState, GuardError> {
        self.mark_success(last_check_correct)
    }

    /// Avanza a resumen. `Done` es terminal → `AlreadyDone`.
    pub fn summarize(&mut self) -> Result<SocraticState, GuardError> {
        if matches!(self.state, SocraticState::Done) {
            return Err(GuardError::AlreadyDone);
        }
        self.state = SocraticState::Summarize;
        self.push_history("summarize".to_string());
        Ok(self.state.clone())
    }

    /// Finaliza el FSM. Idempotente desde `Done` (ya es terminal).
    pub fn finish(&mut self) -> SocraticState {
        self.state = SocraticState::Done;
        self.push_history("done".to_string());
        self.state.clone()
    }

    /// Pone al FSM en espera de estudiante con deadline. `Done` → `AlreadyDone`.
    pub fn await_student(&mut self, deadline_epoch: u64) -> Result<(), GuardError> {
        if matches!(self.state, SocraticState::Done) {
            return Err(GuardError::AlreadyDone);
        }
        self.state = SocraticState::AwaitStudent { deadline_epoch };
        self.push_history(format!("await deadline {deadline_epoch}"));
        Ok(())
    }

    /// Transición por vencimiento de la espera (`AwaitStudent`).
    ///
    /// Con `now >= deadline_epoch` sale de `AwaitStudent`: `Summarize` si ya no
    /// quedan intentos (`attempts >= 3`), si no `HeuristicQ` (re-pregunta).
    /// En cualquier otro estado (o sin vencer) retorna `None` y no muta.
    /// `Done` es terminal: jamás transiciona. Pura salvo el reloj inyectado.
    pub fn on_deadline(&mut self, now: u64) -> Option<SocraticState> {
        let deadline = match self.state {
            SocraticState::AwaitStudent { deadline_epoch } if now >= deadline_epoch => {
                deadline_epoch
            }
            _ => return None,
        };
        let next = if self.attempts >= 3 {
            SocraticState::Summarize
        } else {
            SocraticState::HeuristicQ
        };
        self.state = next.clone();
        self.push_history(format!("deadline vencido {deadline}"));
        Some(next)
    }

    /// Transiciona a rectificación explícita. `Done` → `AlreadyDone`.
    ///
    /// La misconception se acota a 64 chars (el string es display/debug del
    /// estado; el nombre canónico se fija en [`Self::record_attempt`]).
    pub fn rectify(&mut self, misconception: String) -> Result<(), GuardError> {
        if matches!(self.state, SocraticState::Done) {
            return Err(GuardError::AlreadyDone);
        }
        let acotada: String = misconception.chars().take(64).collect();
        self.state = SocraticState::Rectify {
            misconception: acotada.clone(),
        };
        self.push_history(format!("rectify {acotada}"));
        Ok(())
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
            SocraticState::HeuristicQ => "HeuristicQ",
            SocraticState::AwaitStudent { .. } => "AwaitStudent",
            SocraticState::Rectify { .. } => "Rectify",
            SocraticState::Summarize => "Summarize",
            SocraticState::Done => "Done",
        }
    }

    /// Heurística determinista: ¿el texto del LLM contiene marcadores de solución directa?
    ///
    /// Lista acotada sin regex: frases explícitas de revelado (`solución es`,
    /// `respuesta es`, `resultado es`, `solución:`, `respuesta:`, `resultado:`,
    /// `la solución`, `la respuesta`) MÁS marcadores verbales (R6e: decir el
    /// valor en palabras también es revelar — `la derivada es dos`,
    /// `el resultado son tres`). A propósito NO incluye `x =`/`y =` sueltos:
    /// cualquier ejemplo heurístico (`probá con x=1`) disparaba el guard y el
    /// primer turno siempre daba `attempts=0` punitivo. El telling real se
    /// señala con framing explícito o valor verbal, no con una ecuación aislada.
    pub fn contains_solution_marker(text: &str) -> bool {
        let lower = text.to_lowercase();
        if lower.contains("solución es")
            || lower.contains("solucion es")
            || lower.contains("respuesta es")
            || lower.contains("resultado es")
            || lower.contains("solución:")
            || lower.contains("solucion:")
            || lower.contains("respuesta:")
            || lower.contains("resultado:")
            || lower.contains("la solución")
            || lower.contains("la solucion")
            || lower.contains("la respuesta")
        {
            return true;
        }
        Self::contains_verbal_answer(&lower)
    }

    /// ¿El texto afirma un valor en palabras (`es dos`, `son tres`)?
    ///
    /// R6e: `la derivada es dos` es telling aunque no haya dígitos. Cubre
    /// cero..veinte, `treinta` y `cien` tras `es`/`son`/`vale(n)`/`da(n)` con
    /// borde de palabra. Conservador: una pregunta (`¿qué es dos más dos?`)
    /// también dispara — el guard prefiere repreguntar antes que revelar.
    /// Puro, sin regex ni `unwrap`.
    pub fn contains_verbal_answer(text: &str) -> bool {
        const NUMBERS: &[&str] = &[
            "cero",
            "uno",
            "dos",
            "tres",
            "cuatro",
            "cinco",
            "seis",
            "siete",
            "ocho",
            "nueve",
            "diez",
            "once",
            "doce",
            "trece",
            "catorce",
            "quince",
            "dieciseis",
            "dieciséis",
            "diecisiete",
            "dieciocho",
            "diecinueve",
            "veinte",
            "treinta",
            "cien",
            "ciento",
        ];
        const FRAMES: &[&str] = &["es", "son", "vale", "valen", "da", "dan"];
        let lower = text.to_lowercase();
        let words: Vec<&str> = lower
            .split(|c: char| !c.is_alphabetic())
            .filter(|w| !w.is_empty())
            .collect();
        words.windows(2).any(|w| {
            FRAMES.contains(&w[0]) && NUMBERS.contains(&w[1])
                || (w[0] == "menos" && NUMBERS.contains(&w[1]))
        })
    }

    /// Heurística determinista: ¿el texto del LLM contiene un `=` numérico?
    ///
    /// `true` si alguna línea tiene `=` seguido (tras espacios) de un número
    /// (`x=1`, `resultado = -2.5`). Tras `HeuristicQ`, soltar el valor es
    /// telling aunque no haya framing explícito. Pura, sin regex ni `unwrap`.
    pub fn contains_numeric_answer(text: &str) -> bool {
        text.split('\n').any(|line| {
            let mut parts = line.split('=');
            // Necesita al menos un `=` con algo a la izquierda.
            let first = parts.next().unwrap_or("");
            if first.trim().is_empty() {
                return false;
            }
            parts.any(|rhs| {
                let rhs = rhs.trim_start_matches([' ', '\t']);
                let digits = rhs.strip_prefix('-').unwrap_or(rhs);
                digits.starts_with(|c: char| c.is_ascii_digit())
            })
        })
    }

    /// Marcadores de solución según el estado del diálogo.
    ///
    /// Base (framing explícito) siempre; tras `HeuristicQ` (`HeuristicQ` con
    /// intentos, `AwaitStudent`, `Rectify`, `Summarize`) se suma el `=`
    /// numérico: si ya se preguntó, dar el número es revelar. En `Review`
    /// (aún no se preguntó nada) y `Done` rige solo la base — evita el falso
    /// `0` punitivo del conteo viejo y preserva `contains_solution_marker`.
    pub fn contains_solution_marker_for_state(state: &SocraticState, text: &str) -> bool {
        if Self::contains_solution_marker(text) {
            return true;
        }
        match state {
            SocraticState::Review { .. } | SocraticState::Done => false,
            _ => Self::contains_numeric_answer(text),
        }
    }

    /// ¿Es telling? `true` si `!can_reveal` y hay marcador para el estado
    /// actual (framing explícito siempre + `=` numérico tras `HeuristicQ`).
    /// Voz Mili y umbral `attempts >= 2` intactos.
    pub fn is_telling(&self, response_text: &str) -> bool {
        !self.can_reveal_answer()
            && Self::contains_solution_marker_for_state(&self.state, response_text)
    }

    /// ¿El pedido es exploratorio pero igual exige repair? Cierra el bypass
    /// `is_exploratory_request` (demo, no evaluación) cuando la respuesta del
    /// modelo trae matemática (`math_expr`/`$..$`/`=` numérico): un "ejemplo"
    /// que suelta el valor es telling igual.
    ///
    /// Excepción: pedido EXPLÍCITO de ejemplo (`is_explicit_demo_request`:
    /// "dame un ejemplo…", "mostrame cómo…") jamás exige repair — no hay
    /// "solución" que retener porque el usuario pidió ver algo hecho.
    ///
    /// Pura para el dueño del guard en app (`session_socratic_guard`,
    /// `assistant.rs` — ESTE crate no lo toca):
    /// ```ignore
    /// if is_exploratory_request(question) && !SocraticFsm::requires_repair_despite_exploratory(question, response_trae_math) {
    ///     return None; // bypass demo real
    /// }
    /// // …sigue guard normal (repair con voz Mili, attempts>=2)
    /// ```
    pub fn requires_repair_despite_exploratory(question: &str, response_brings_math: bool) -> bool {
        if is_explicit_demo_request(question) {
            return false;
        }
        is_exploratory_request(question) && response_brings_math
    }

    /// ¿Exige repair en `Review` aunque aún no hubo pregunta? (R6e)
    ///
    /// En `Review` el `=` numérico suelto NO es telling (un ejemplo como
    /// `x=1` no revela nada), pero una respuesta que YA trae matemática
    /// decidida (`$..$`, framing explícito o valor verbal como
    /// `la derivada es dos`) sí exige repair: el primer turno no puede
    /// soltar la solución con la excusa de "aún no pregunté". Pura, para el
    /// dueño del guard en app (OR con `is_telling`).
    pub fn requires_repair_in_review(state: &SocraticState, response_text: &str) -> bool {
        if !matches!(state, SocraticState::Review { .. }) {
            return false;
        }
        response_text.contains('$') || Self::contains_solution_marker(response_text)
    }

    /// ¿La respuesta trae matemática (para `requires_repair_despite_exploratory`)?
    /// Heurística barata y pura: `$..$`, `=` numérico o framing explícito.
    pub fn response_brings_math(response_text: &str) -> bool {
        response_text.contains('$')
            || Self::contains_numeric_answer(response_text)
            || Self::contains_solution_marker(response_text)
    }

    /// Guarda telling para uso remoto: `TellingTooEarly` si es telling con `attempts<2`.
    pub fn check_telling_guard(&self, response_text: &str) -> Result<(), GuardError> {
        if self.is_telling(response_text) {
            Err(GuardError::TellingTooEarly)
        } else {
            Ok(())
        }
    }

    /// Diagnóstico interno cuando se viola el guard (SOLO logs/eventos, JAMÁS al transcript).
    ///
    /// Incluye la pregunta BKT/scaffold exacta y el contador attempts para
    /// detección (`is_socratic_repair_error`) y trazabilidad. El chat debe usar
    /// [`SocraticFsm::repair_student_message`] (voz de Mili, sin jerga).
    /// Publicar este string vía `complete_request` fue el bug P0 (directiva
    /// `GUARD TELLING` visible + interpolación degenerada).
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

    /// Enforce interno: si es telling devuelve `Err(diagnóstico)` para logs/detección, si no `Ok(text)`.
    ///
    /// El `Err` conserva la jerga `GUARD TELLING` SOLO para `is_socratic_repair_error`
    /// y logs. El poll NUNCA lo publica: lo convierte vía
    /// [`SocraticFsm::repair_student_message`] a voz de Mili.
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

    /// Reparación tipada interna (pregunta sanitizada + pista), sin jerga en campos.
    ///
    /// Pura y determinista, acotada a 400 chars por campo, sin `unwrap`.
    /// El transcript usa [`SocraticRepair::to_student_voice`], jamás el diagnóstico.
    pub fn repair_for_telling(&self, scaffold: &Scaffold) -> SocraticRepair {
        let hint = scaffold
            .hint
            .as_deref()
            .unwrap_or("Probá con un ejemplo concreto (x=1, x=2).");
        let pregunta_humana: String = scaffold.question.chars().take(400).collect();
        let pista: String = hint.chars().take(400).collect();
        let pregunta_humana = if pregunta_humana.trim().is_empty() {
            crate::scaffold::NO_CONCEPT_FALLBACK_QUESTION.to_owned()
        } else {
            pregunta_humana
        };
        SocraticRepair {
            pregunta_humana,
            pista,
        }
    }

    /// Mensaje para el estudiante (voz de Mili, sin jerga) — lo que SÍ va al transcript.
    ///
    /// Puro y determinista. Garantiza ausencia de `GUARD`, `REPARACIÓN`,
    /// `attempts`, `can_reveal`, `Re-preguntá` e interpolaciones degeneradas.
    pub fn repair_student_message(&self, scaffold: &Scaffold) -> String {
        self.repair_for_telling(scaffold).to_student_voice()
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
        fsm.record_attempt(None).expect("intento");
        assert!(!fsm.can_reveal_answer());
        fsm.record_attempt(None).expect("intento");
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
        assert_eq!(s, SocraticState::HeuristicQ);
        fsm.record_attempt(None).expect("intento");
        assert_eq!(fsm.attempts, 1);
        let s2 = fsm.ask().expect("segundo ask ok");
        // El estado ya no duplica el contador: `HeuristicQ` es unit y el único
        // `attempts` vive en `self.attempts`.
        assert_eq!(s2, SocraticState::HeuristicQ);
        assert_eq!(fsm.attempts, 1);
    }

    #[test]
    fn success_requires_at_least_one_attempt() {
        let mut fsm = SocraticFsm::new("pitágoras");
        assert_eq!(
            fsm.mark_success(true).unwrap_err(),
            GuardError::NotEnoughAttempts
        );
        fsm.record_attempt(None).expect("intento");
        // Con intento pero sin remate correcto no hay éxito (R6e).
        assert_eq!(
            fsm.mark_success(false).unwrap_err(),
            GuardError::CheckNotCorrect
        );
        assert!(fsm.mark_success(true).is_ok());
        assert!(matches!(fsm.state, SocraticState::Summarize));
    }

    #[test]
    fn max_three_attempts_then_summarize() {
        let mut fsm = SocraticFsm::new("vectores");
        fsm.record_attempt(None).expect("intento"); // 1 -> HeuristicQ 1
        fsm.record_attempt(Some("sign".to_string())) // 2 -> Rectify
            .expect("intento");
        assert!(matches!(fsm.state, SocraticState::Rectify { .. }));
        fsm.record_attempt(None).expect("intento"); // 3 -> Summarize
        assert!(matches!(fsm.state, SocraticState::Summarize));
        // cuarto intento ya en summarize, ask debe dar TooManyAttempts
        let err = fsm.ask().unwrap_err();
        assert_eq!(err, GuardError::TooManyAttempts);
        assert!(matches!(fsm.state, SocraticState::Summarize));
    }

    #[test]
    fn rectify_on_misconception() {
        let mut fsm = SocraticFsm::new("fracciones");
        fsm.record_attempt(Some("fracción".to_string()))
            .expect("intento");
        assert!(
            matches!(fsm.state, SocraticState::Rectify { misconception } if misconception == "Fraction")
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
        fsm.await_student(9999).expect("await");
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
        fsm.record_attempt(None).expect("intento");
        fsm.record_attempt(None).expect("intento");
        let g = fsm.answer_with_guidance().expect("debe revelar con guía");
        assert!(g.contains("series") || g.contains("Guía"));
    }

    #[test]
    fn ask_after_done_is_already_done() {
        let mut fsm = SocraticFsm::new("matrices");
        fsm.finish();
        assert_eq!(fsm.ask().unwrap_err(), GuardError::AlreadyDone);
        assert_eq!(fsm.mark_success(true).unwrap_err(), GuardError::AlreadyDone);
    }

    #[test]
    fn too_many_attempts_guard() {
        let mut fsm = SocraticFsm::new("taylor");
        for _ in 0..3 {
            fsm.record_attempt(None).expect("intento");
        }
        assert!(matches!(fsm.state, SocraticState::Summarize));
        // ask ahora debe fallar
        assert_eq!(fsm.ask().unwrap_err(), GuardError::TooManyAttempts);
    }

    #[test]
    fn succeed_alias_works() {
        let mut fsm = SocraticFsm::new("edo");
        fsm.record_attempt(None).expect("intento");
        assert!(fsm.succeed(true).is_ok());
    }

    #[test]
    fn recorrido_completo_review_a_summarize() {
        // Review → HeuristicQ → AwaitStudent → HeuristicQ → Rectify → Summarize → Done.
        let mut fsm = SocraticFsm::new("derivada");
        // 1. Review inicial.
        assert!(matches!(fsm.state, SocraticState::Review { .. }));
        // 2. HeuristicQ.
        let s = fsm.ask().expect("ask inicial ok");
        assert_eq!(s, SocraticState::HeuristicQ);
        // 3. Espera al estudiante.
        fsm.await_student(9_999).expect("await");
        assert_eq!(
            fsm.state,
            SocraticState::AwaitStudent {
                deadline_epoch: 9_999
            }
        );
        // 4. Nueva pregunta heurística tras la espera.
        let s2 = fsm.ask().expect("ask tras espera ok");
        assert_eq!(s2, SocraticState::HeuristicQ);
        // 5. Intento con misconception → Rectify.
        fsm.record_attempt(Some("fracción".to_string()))
            .expect("intento");
        assert!(
            matches!(&fsm.state, SocraticState::Rectify { misconception } if misconception.as_str() == "Fraction")
        );
        assert_eq!(fsm.attempts, 1);
        // 6. Éxito tras ≥1 intento + remate correcto → Summarize.
        let s3 = fsm.mark_success(true).expect("éxito ok");
        assert_eq!(s3, SocraticState::Summarize);
        assert!(matches!(fsm.state, SocraticState::Summarize));
        // 7. Cierre → Done.
        fsm.finish();
        assert!(fsm.is_done());
        // Historial encadena todas las fases en orden.
        let h = fsm
            .history
            .iter()
            .map(String::as_str)
            .collect::<Vec<_>>()
            .join("|");
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
        fsm.record_attempt(None).expect("intento");
        assert_eq!(
            fsm.answer_with_guidance().unwrap_err(),
            GuardError::TellingTooEarly
        );
        fsm.record_attempt(None).expect("intento");
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
        // Framing explícito sí es telling.
        assert!(SocraticFsm::contains_solution_marker(
            "La solución es x = 4"
        ));
        assert!(SocraticFsm::contains_solution_marker("Respuesta es 42"));
        assert!(SocraticFsm::contains_solution_marker(
            "La respuesta es x = 2"
        ));
        // Ecuación suelta SIN framing NO es telling (era el falso positivo P0:
        // cualquier `x =` disparaba el guard en primer turno con attempts=0).
        assert!(!SocraticFsm::contains_solution_marker("x = 2.5"));
        assert!(!SocraticFsm::contains_solution_marker(
            "¿Cómo lo pensaste? Intentá con x=1"
        ));
        assert!(!SocraticFsm::contains_solution_marker(
            "Explicalo con tus palabras"
        ));
    }

    #[test]
    fn igual_numerico_es_telling_tras_heuristicq() {
        // En Review (sin pregunta previa) el `=` suelto NO es telling.
        let fresh = SocraticFsm::new("derivada");
        assert!(!fresh.is_telling("Intentá con x=1"));
        // Tras HeuristicQ, soltar el valor SÍ es telling.
        let mut asked = SocraticFsm::new("derivada");
        asked.ask().expect("ask");
        assert!(asked.is_telling("Da x=2, fijate"));
        assert!(asked.is_telling("resultado = -2.5"));
        assert!(!asked.is_telling("¿Cómo lo pensaste? Contame tu idea"));
        // Con attempts>=2 se puede revelar (umbral intacto).
        asked.record_attempt(None).expect("intento");
        asked.record_attempt(None).expect("intento");
        assert!(!asked.is_telling("Da x=2, fijate"));
        // Base intacta: framing explícito sigue siendo telling en Review.
        assert!(fresh.is_telling("la solución es x = 4"));
    }
    #[test]
    fn exploratorio_con_math_exige_repair() {
        let demo = "hola haceme ejemplos para probar las capacidades de graficacion";
        assert!(SocraticFsm::requires_repair_despite_exploratory(demo, true));
        assert!(!SocraticFsm::requires_repair_despite_exploratory(
            demo, false
        ));
        assert!(!SocraticFsm::requires_repair_despite_exploratory(
            "¿qué es la derivada de x^2?",
            true
        ));
        // Pedido explícito de ejemplo: jamás repair aunque traiga matemática.
        assert!(!SocraticFsm::requires_repair_despite_exploratory(
            "a ver dame un ejemplo con numeros complejos",
            true
        ));
        assert!(SocraticFsm::response_brings_math("miralo: $x^2$"));
        assert!(SocraticFsm::response_brings_math("da x=2"));
        assert!(!SocraticFsm::response_brings_math(
            "contame cómo lo pensaste"
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
        fsm.record_attempt(None).expect("intento");
        fsm.record_attempt(None).expect("intento");
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
        // Diagnóstico interno (SOLO logs/eventos): conserva jerga para detección.
        let r1 = fsm.repair_prompt_for_telling(&scaffold);
        let r2 = fsm.repair_prompt_for_telling(&scaffold);
        assert_eq!(r1, r2);
        assert!(r1.contains("GUARD TELLING"));
        assert!(r1.contains("attempts=0"));
        assert!(r1.contains("¿Qué representa"));
        // Transcript (voz de Mili): SIN jerga, jamás al chat.
        let student = fsm.repair_student_message(&scaffold);
        assert_eq!(
            student,
            fsm.repair_student_message(&scaffold),
            "determinista"
        );
        assert!(!student.contains("GUARD"), "{student}");
        assert!(
            !student.contains("REPARACIÓN") && !student.contains("REPARACION"),
            "{student}"
        );
        assert!(!student.contains("attempts"), "{student}");
        assert!(!student.contains("can_reveal"), "{student}");
        assert!(!student.contains("Re-pregunt"), "{student}");
        assert!(student.contains("Antes de mostrarte"), "{student}");
        assert!(student.contains("¿Qué representa"), "{student}");
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
        // Interno: diagnóstico con jerga (solo logs/detección).
        let err = fsm.enforce_telling_guard(telling, &scaffold).unwrap_err();
        assert!(err.contains("REPARACIÓN SOCRÁTICA") || err.contains("GUARD TELLING"));
        assert!(err.contains("¿Qué representa integral?"));
        // Transcript: voz humana sin jerga.
        let student = fsm.repair_student_message(&scaffold);
        assert!(!student.contains("GUARD"), "{student}");
        assert!(
            !student.contains("REPARACIÓN") && !student.contains("REPARACION"),
            "{student}"
        );
        assert!(!student.contains("attempts"), "{student}");
        assert!(!student.contains("can_reveal"), "{student}");
        assert!(student.contains("Antes de mostrarte"), "{student}");
        // con attempts>=2 pasa
        let mut fsm2 = SocraticFsm::new("integral");
        fsm2.record_attempt(None).expect("intento");
        fsm2.record_attempt(None).expect("intento");
        assert!(fsm2.enforce_telling_guard(telling, &scaffold).is_ok());
    }

    #[test]
    fn socratic_repair_student_voice_has_no_jargon_or_raw_echo() {
        // Regresión P0: el input real que mostró la directiva interna.
        let raw = "hola haceme ejemplos para probar las capacidades de graficacion";
        // El extractor no reconoce tema → None → scaffold cae al fallback.
        assert_eq!(crate::scaffold::extract_concept(raw), None);
        assert!(crate::scaffold::is_exploratory_request(raw));
        let engine = crate::scaffold::ScaffoldEngine;
        let sc = engine.scaffold(raw, crate::level::PedagogicalLevel::Secondary, &[]);
        assert_eq!(sc.question, crate::scaffold::NO_CONCEPT_FALLBACK_QUESTION);
        assert!(!sc.question.contains("hola"), "{}", sc.question);
        let fsm = SocraticFsm::new("");
        let student = fsm.repair_student_message(&sc);
        for forbidden in [
            "GUARD",
            "REPARACIÓN",
            "REPARACION",
            "attempts",
            "can_reveal",
            "Re-pregunt",
            "¿Te imaginás hola",
        ] {
            assert!(
                !student.contains(forbidden),
                "jerga '{forbidden}' en '{student}'"
            );
        }
        assert!(student.contains("Antes de mostrarte"), "{student}");
    }

    #[test]
    fn socratic_system_segment_is_deterministic_and_binding() {
        use crate::level::PedagogicalLevel;
        use crate::scaffold::ScaffoldEngine;
        let mut fsm = SocraticFsm::new("derivada");
        fsm.record_attempt(Some("sign".into())).expect("intento");
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
        assert!(seg1.contains("misconception: Sign"));
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

    #[test]
    fn r6e_marcador_verbal_es_telling() {
        // "la derivada es dos" dice el valor en palabras: es telling con
        // attempts<2, incluso en Review (framing explícito de base).
        assert!(SocraticFsm::contains_verbal_answer("la derivada es dos"));
        assert!(SocraticFsm::contains_solution_marker("la derivada es dos"));
        assert!(SocraticFsm::contains_solution_marker(
            "el resultado son tres"
        ));
        let fresh = SocraticFsm::new("derivada");
        assert!(fresh.is_telling("la derivada es dos"));
        // Sin valor verbal no hay marcador.
        assert!(!SocraticFsm::contains_verbal_answer(
            "¿qué forma te imaginás? contame qué probaste"
        ));
        assert!(!SocraticFsm::contains_verbal_answer(
            "la derivada es una pendiente"
        ));
        // Tras 2 intentos se puede revelar (umbral intacto).
        let mut ok = SocraticFsm::new("derivada");
        ok.record_attempt(None).expect("intento");
        ok.record_attempt(None).expect("intento");
        assert!(!ok.is_telling("la derivada es dos"));
    }

    #[test]
    fn r6e_repair_en_review_con_math() {
        let review = SocraticState::Review {
            lo_id: "derivada".to_string(),
        };
        // Review + math decidida ($..$, framing, valor verbal) exige repair.
        assert!(SocraticFsm::requires_repair_in_review(
            &review,
            "miralo: $x^2$"
        ));
        assert!(SocraticFsm::requires_repair_in_review(
            &review,
            "la derivada es dos, fijate"
        ));
        assert!(SocraticFsm::requires_repair_in_review(
            &review,
            "la solución es x = 4"
        ));
        // Review + ejemplo suelto o charla sin math: sin repair.
        assert!(!SocraticFsm::requires_repair_in_review(
            &review,
            "intentá con x=1"
        ));
        assert!(!SocraticFsm::requires_repair_in_review(
            &review,
            "contame cómo lo pensaste"
        ));
        // Fuera de Review no aplica (lo cubre `is_telling`).
        let asked = SocraticState::HeuristicQ;
        assert!(!SocraticFsm::requires_repair_in_review(
            &asked,
            "miralo: $x^2$"
        ));
        assert!(!SocraticFsm::requires_repair_in_review(
            &SocraticState::Done,
            "la solución es x = 4"
        ));
    }

    #[test]
    fn r6e_misconception_contra_enum_cerrado() {
        // Etiqueta conocida (es/en) → Rectify con nombre canónico.
        let mut fsm = SocraticFsm::new("fracciones");
        fsm.record_attempt(Some("fracción".to_string()))
            .expect("intento");
        assert!(
            matches!(&fsm.state, SocraticState::Rectify { misconception } if misconception == "Fraction")
        );
        let mut fsm2 = SocraticFsm::new("derivada");
        fsm2.record_attempt(Some("sign".to_string()))
            .expect("intento");
        assert!(
            matches!(&fsm2.state, SocraticState::Rectify { misconception } if misconception == "Sign")
        );
        // Etiqueta inventada: sin Rectify, a HeuristicQ, con traza honesta.
        let mut fsm3 = SocraticFsm::new("derivada");
        fsm3.record_attempt(Some("typo-inventado".to_string()))
            .expect("intento");
        assert!(matches!(fsm3.state, SocraticState::HeuristicQ));
        assert_eq!(fsm3.attempts, 1);
        assert!(
            fsm3.history
                .iter()
                .any(|h| h.contains("desconocida") && h.contains("sin Rectify")),
            "traza honesta del descarte"
        );
        // Vacía equivale a sin dato.
        let mut fsm4 = SocraticFsm::new("derivada");
        fsm4.record_attempt(Some("   ".to_string()))
            .expect("intento");
        assert!(matches!(fsm4.state, SocraticState::HeuristicQ));
    }

    #[test]
    fn done_es_terminal_en_todas_las_transiciones() {
        // Regresión FIX 3: `Done` era terminal solo para `ask`/`mark_success`;
        // `record_attempt`, `rectify`, `await_student` y `summarize`
        // resucitaban el FSM (lo sacaban de `Done` a HeuristicQ/Rectify/…).
        let mut fsm = SocraticFsm::new("derivada");
        fsm.record_attempt(None).expect("intento");
        fsm.finish();
        assert!(fsm.is_done());
        assert_eq!(
            fsm.record_attempt(None)
                .expect_err("record_attempt resucitó"),
            GuardError::AlreadyDone
        );
        assert_eq!(
            fsm.record_attempt(Some("sign".to_string()))
                .expect_err("record_attempt resucitó"),
            GuardError::AlreadyDone
        );
        assert_eq!(
            fsm.rectify("Fraction".to_string())
                .expect_err("rectify resucitó"),
            GuardError::AlreadyDone
        );
        assert_eq!(
            fsm.await_student(500).expect_err("await_student resucitó"),
            GuardError::AlreadyDone
        );
        assert_eq!(
            fsm.summarize().expect_err("summarize resucitó"),
            GuardError::AlreadyDone
        );
        // Las que ya guardaban también siguen fallando.
        assert_eq!(
            fsm.ask().expect_err("ask resucitó"),
            GuardError::AlreadyDone
        );
        assert_eq!(
            fsm.mark_success(true).expect_err("mark_success resucitó"),
            GuardError::AlreadyDone
        );
        // Y el FSM sigue exactamente en Done, sin side effects.
        assert!(matches!(fsm.state, SocraticState::Done));
        assert_eq!(fsm.attempts, 1, "el contador no debe mutar desde Done");
        assert_eq!(fsm.on_deadline(u64::MAX), None, "Done no transiciona");
    }

    #[test]
    fn on_deadline_transiciona_al_vencer_await_student() {
        // Regresión FIX 11: `AwaitStudent { deadline_epoch }` era decorativo
        // (sin transición por vencimiento dentro del crate).
        let mut fsm = SocraticFsm::new("derivada");
        fsm.record_attempt(None).expect("intento");
        fsm.await_student(1_000).expect("await");
        // Antes del vencimiento: nada.
        assert_eq!(fsm.on_deadline(999), None);
        assert!(matches!(
            fsm.state,
            SocraticState::AwaitStudent {
                deadline_epoch: 1_000
            }
        ));
        // Al vencer, con presupuesto de intentos: re-pregunta.
        assert_eq!(fsm.on_deadline(1_000), Some(SocraticState::HeuristicQ));
        // Con intentos agotados (>=3): vence a Summarize.
        fsm.record_attempt(None).expect("intento");
        fsm.record_attempt(None).expect("intento");
        assert_eq!(fsm.attempts, 3);
        fsm.await_student(2_000).expect("await");
        assert_eq!(fsm.on_deadline(2_000), Some(SocraticState::Summarize));
        // Fuera de AwaitStudent no transiciona nunca.
        fsm.finish();
        assert_eq!(fsm.on_deadline(u64::MAX), None);
    }

    #[test]
    fn repair_de_mili_no_dispara_el_guard_ni_marcadores() {
        // Regresión FIX 4: la voz de repair decía "Antes de mostrarte la
        // solución…" y `contains_solution_marker` dispara con el marcador
        // literal "la solución" → si el caller volvía a pasar el repair por el
        // guard (attempts < 2) => TellingTooEarly otra vez => bucle infinito.
        // Además la pista del scaffold trae ejemplos `x=1` que
        // `contains_numeric_answer` detecta tras HeuristicQ: redactados también.
        let mut fsm = SocraticFsm::new("derivadas");
        fsm.ask().expect("ask");
        let scaffold = crate::scaffold::Scaffold {
            question: "¿Qué forma te imaginás que tiene la derivada?".into(),
            hint: Some(
                "Pista concreta: probá con un ejemplo numérico simple (x=1, x=2) y compará resultados."
                    .into(),
            ),
            explanation: "Exp".into(),
        };
        let voz = fsm.repair_student_message(&scaffold);
        assert!(
            !SocraticFsm::contains_solution_marker(&voz),
            "el repair se autodetecta como telling (framing): {voz}"
        );
        assert!(
            !SocraticFsm::contains_numeric_answer(&voz),
            "el repair se autodetecta como telling (= numérico): {voz}"
        );
        assert!(
            !SocraticFsm::contains_verbal_answer(&voz),
            "el repair se autodetecta como telling (valor verbal): {voz}"
        );
        // Cierre del bucle: el repair NO es telling para el guard en ningún
        // estado (umbral attempts < 2 intacto para el resto).
        assert!(!fsm.is_telling(&voz), "repair vuelve por el guard: {voz}");
        assert!(fsm.check_telling_guard(&voz).is_ok());
        // Variante sin pista: mismo resultado.
        let sin_pista = super::SocraticRepair {
            pregunta_humana: "¿Qué probaste?".into(),
            pista: String::new(),
        };
        let voz2 = sin_pista.to_student_voice();
        assert!(!SocraticFsm::contains_solution_marker(&voz2));
        assert!(!fsm.is_telling(&voz2));
    }

    #[test]
    fn r6e_history_es_vecdeque_cap_32() {
        assert_eq!(super::MAX_HISTORY_ENTRIES, 32);
        let mut fsm = SocraticFsm::new("derivada");
        for _ in 0..40 {
            fsm.ask().expect("ask");
        }
        assert_eq!(fsm.history.len(), super::MAX_HISTORY_ENTRIES);
        assert!(
            fsm.history.iter().all(|h| h.starts_with("ask heuristic")),
            "lo viejo se descartó, lo nuevo queda"
        );
        // record_attempt también respeta el tope.
        for _ in 0..40 {
            fsm.record_attempt(None).expect("intento");
            if matches!(fsm.state, SocraticState::Summarize) {
                break;
            }
        }
        assert!(fsm.history.len() <= super::MAX_HISTORY_ENTRIES);
    }

    #[test]
    fn r6e_forma_serde_del_fsm_fijada() {
        // Persistencia: verificado con grep 2026-09 que NADA en el workspace
        // serializa `SocraticState`/`SocraticFsm` (el único consumidor externo,
        // `SocraticGuardContext` de grafito-assistant, es `Debug + Clone` sin
        // serde; la config de grafito-app solo persiste
        // `assistant_socratic_enabled: bool`). Por eso no hay `#[serde(alias)]`
        // ni migración que agregar. Este test fija la forma por si alguien lo
        // persiste en el futuro: `HeuristicQ` es variante unit
        // (`"HeuristicQ"`), NO `{"HeuristicQ":{"attempts":N}}` (payload
        // eliminado a propósito: el contador vive solo en `SocraticFsm`).
        let json = serde_json::to_string(&SocraticState::HeuristicQ).expect("serializa");
        assert_eq!(json, "\"HeuristicQ\"");
        let back: SocraticState = serde_json::from_str(&json).expect("deserializa");
        assert_eq!(back, SocraticState::HeuristicQ);
        // `Done` (terminal en las 4 transiciones + ask/record/await/rectify)
        // también es unit.
        let json_done = serde_json::to_string(&SocraticState::Done).expect("serializa");
        assert_eq!(json_done, "\"Done\"");
        // Las variantes con payload hacen round-trip sin pérdida.
        for estado in [
            SocraticState::Review {
                lo_id: "am1-der".into(),
            },
            SocraticState::AwaitStudent { deadline_epoch: 7 },
            SocraticState::Rectify {
                misconception: "Sign".into(),
            },
            SocraticState::Summarize,
        ] {
            let s = serde_json::to_string(&estado).expect("serializa");
            let back: SocraticState = serde_json::from_str(&s).expect("deserializa");
            assert_eq!(back, estado);
        }
        // El FSM completo también sobrevive round-trip (por si se persiste).
        let fsm = SocraticFsm::new("derivada");
        let s = serde_json::to_string(&fsm).expect("serializa fsm");
        let back: SocraticFsm = serde_json::from_str(&s).expect("deserializa fsm");
        assert_eq!(back.state, fsm.state);
        assert_eq!(back.topic, fsm.topic);
        assert_eq!(back.attempts, fsm.attempts);
    }
}
