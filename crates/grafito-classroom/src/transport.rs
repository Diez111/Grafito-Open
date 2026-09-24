//! Transporte del aula: contrato + Loopback puro (sin red).
//!
//! Cerebro puro: sin I/O, sin spawn, sin threads. `LoopbackTransport` es una
//! cola acotada en memoria (128 mensajes, 2048 bytes por mensaje) que nunca
//! sale del proceso — PII siempre local. El P2P real (iroh) queda como stub
//! honesto en [`crate::stubs`] (L, solo diseño).

use serde::{Deserialize, Serialize};
use std::collections::VecDeque;

use crate::session::{ClassroomError, LearnerName, MAX_LEARNER_NAME_LEN};

/// Tope de mensajes encolados (igual que el canal del agente: 128).
pub const MAX_TRANSPORT_QUEUE: usize = 128;
/// Tope por mensaje (cuerpo + remitente acotados; coherente con los 2000 chars
/// del ejercicio activo [`crate::MAX_EXERCISE_CHARS`], más holgura de framing).
pub const MAX_MESSAGE_BYTES: usize = 2_048;
/// Intentos máximos de un reintento acotado (1..=8, default 3).
pub const MAX_SEND_ATTEMPTS: u8 = 8;
/// Intentos mínimos (un solo intento = `send` clásico).
pub const MIN_SEND_ATTEMPTS: u8 = 1;
/// Reintentos default (1 intento + 2 reintentos, igual que cascada S/M).
pub const DEFAULT_SEND_ATTEMPTS: u8 = 3;

/// Tipo de mensaje de aula (serializable, sin PII extra).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ClassroomMessageKind {
    /// Unión al roster.
    Join,
    /// Salida del roster.
    Leave,
    /// Mano levantada.
    HandRaise,
    /// Mano bajada.
    HandLower,
    /// Ejercicio activo actualizado.
    Exercise,
    /// Digest de snapshot compartido.
    Snapshot,
    /// Chat corto de aula (acotado).
    Chat,
}

/// Mensaje inmutable del aula (remitente + tipo + cuerpo acotado).
///
/// Todo mensaje que cruza el borde (constructor **o** deserialización) pasa
/// por [`ClassroomMessage::validate`]: remitente `1..=64` chars sin controles,
/// cuerpo `<= MAX_MESSAGE_BYTES` sin controles salvo `\n\t` (anti inyección
/// en UI). `Deserialize` es estricto vía `try_from` (nunca trunca: rechaza).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(try_from = "RawClassroomMessage")]
pub struct ClassroomMessage {
    /// Remitente display (ya saneado `1..=64`).
    pub from: String,
    /// Tipo de mensaje.
    pub kind: ClassroomMessageKind,
    /// Cuerpo acotado (`<= MAX_MESSAGE_BYTES` en bytes).
    pub body: String,
}

/// Forma cruda entrante (JSON remoto/hostil) que se revalida en `try_from`.
#[derive(Debug, Deserialize)]
struct RawClassroomMessage {
    from: String,
    kind: ClassroomMessageKind,
    body: String,
}

impl TryFrom<RawClassroomMessage> for ClassroomMessage {
    type Error = ClassroomError;

    fn try_from(raw: RawClassroomMessage) -> Result<Self, Self::Error> {
        let msg = Self {
            from: raw.from,
            kind: raw.kind,
            body: raw.body,
        };
        msg.validate()?;
        Ok(msg)
    }
}

impl ClassroomMessage {
    /// Construye validando remitente y cuerpo (todo `Result`, sin pánicos).
    ///
    /// El remitente se sanea con [`LearnerName::try_new`] (trim + cap 64
    /// chars); después todo pasa por [`Self::validate`], la MISMA validación
    /// que usan `send()` y el `pump()` entrante (sin fail-open en el borde).
    pub fn try_new(
        from: &str,
        kind: ClassroomMessageKind,
        body: &str,
    ) -> Result<Self, ClassroomError> {
        let clean_from = LearnerName::try_new(from)
            .map_err(|_| ClassroomError::InvalidName(from.trim().to_string()))?;
        let msg = Self {
            from: clean_from.as_str().to_string(),
            kind,
            body: body.to_string(),
        };
        msg.validate()?;
        Ok(msg)
    }

    /// Valida un mensaje ya construido (remitente + cuerpo + controles).
    ///
    /// Única puerta de verdad del borde wire: `send()` (salida) y el `pump()`
    /// entrante (P2P) llaman solo esto, así que el chequeo anti-inyección de
    /// UI vive ACÁ y no solo en el constructor.
    pub fn validate(&self) -> Result<(), ClassroomError> {
        let trimmed = self.from.trim();
        if trimmed.is_empty() {
            return Err(ClassroomError::InvalidMessage(
                "remitente inválido".to_string(),
            ));
        }
        // Presupuesto en chars (igual que `LearnerName` / dashboard): 64
        // ideogramas son válidos aunque ocupen 256 bytes.
        if self.from.chars().count() > MAX_LEARNER_NAME_LEN {
            return Err(ClassroomError::InvalidMessage(
                "remitente inválido".to_string(),
            ));
        }
        if self.from.chars().any(char::is_control) {
            return Err(ClassroomError::InvalidMessage(
                "remitente con caracteres de control".to_string(),
            ));
        }
        if self.body.len() > MAX_MESSAGE_BYTES {
            return Err(ClassroomError::InvalidMessage(format!(
                "cuerpo excede {MAX_MESSAGE_BYTES} bytes"
            )));
        }
        // Rechazar controles (salvo \n\t) para evitar inyección en UI.
        if self
            .body
            .chars()
            .any(|c| c.is_control() && c != '\n' && c != '\t')
        {
            return Err(ClassroomError::InvalidMessage(
                "cuerpo con caracteres de control".to_string(),
            ));
        }
        Ok(())
    }
}

/// Contrato de transporte (la sesión no depende de una red concreta).
///
/// Política de cola llena (una sola para todas las implementaciones):
/// - **salida** (`send`): rechazo honesto `Err(QueueFull)` — nunca evicción
///   silenciosa de mensajes ya encolados;
/// - **entrada** (bombeo de red): rechazo del mensaje nuevo + contador de
///   descartes ([`Self::dropped_count`]) — jamás se descarta el más viejo sin
///   aviso.
pub trait ClassroomTransport {
    /// Encola un mensaje para entrega local. `Err(QueueFull)` si está llena.
    fn send(&mut self, msg: ClassroomMessage) -> Result<(), ClassroomError>;
    /// Saca el mensaje más antiguo, o `None` si vacía.
    fn poll(&mut self) -> Option<ClassroomMessage>;
    /// Mensajes encolados.
    fn len(&self) -> usize;
    /// ¿Cola vacía?
    fn is_empty(&self) -> bool;
    /// ¿Transporte conectado? Loopback siempre `true` hasta `disconnect`.
    fn is_connected(&self) -> bool;
    /// Desconecta (vacía la cola; `send` posterior falla honesto).
    fn disconnect(&mut self);
    /// Mensajes entrantes descartados por cola llena (aviso honesto, nunca
    /// pérdida silenciosa). Default `0` cuando toda descarga va por `Err`.
    fn dropped_count(&self) -> usize {
        0
    }
}

/// Loopback en memoria: cola `VecDeque` acotada, sin red, sin threads.
///
/// Ideal para tests headless y para el frente F10 (aula sin P2P): el QR de
/// `grafito-app/src/classroom.rs` comparte código, pero los mensajes nunca
/// salen del proceso.
#[derive(Debug, Clone)]
pub struct LoopbackTransport {
    queue: VecDeque<ClassroomMessage>,
    connected: bool,
}

/// Presupuesto de reintento acotado: newtype `1..=8` intentos.
///
/// Garantiza que ningún `send_with_retry` loopea infinito (fail-closed).
/// Solo `QueueFull` se reintenta; `InvalidMessage`/desconectado fallan rápido
/// sin consumir intentos extra.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(try_from = "u8")]
pub struct RetryBudget(u8);

impl TryFrom<u8> for RetryBudget {
    type Error = ClassroomError;

    fn try_from(attempts: u8) -> Result<Self, Self::Error> {
        Self::try_new(attempts)
    }
}

impl RetryBudget {
    /// Valida `1..=8`. `Err(InvalidMessage)` si fuera de rango (sin nuevo variante).
    pub fn try_new(attempts: u8) -> Result<Self, ClassroomError> {
        if (MIN_SEND_ATTEMPTS..=MAX_SEND_ATTEMPTS).contains(&attempts) {
            Ok(Self(attempts))
        } else {
            Err(ClassroomError::InvalidMessage(format!(
                "intentos {attempts} fuera de {MIN_SEND_ATTEMPTS}..={MAX_SEND_ATTEMPTS}"
            )))
        }
    }

    /// Presupuesto default (3 intentos).
    #[must_use]
    pub fn default_budget() -> Self {
        Self(DEFAULT_SEND_ATTEMPTS)
    }

    /// Intentos validados.
    #[must_use]
    pub fn as_u8(&self) -> u8 {
        self.0
    }
}

impl LoopbackTransport {
    /// Loopback conectado y vacío.
    #[must_use]
    pub fn new() -> Self {
        Self {
            queue: VecDeque::new(),
            connected: true,
        }
    }

    /// Envía con reintento acotado (solo `QueueFull` se reintenta).
    ///
    /// Retorna `Ok(n)` con el intento exitoso (1-indexed) o `Err` honesto:
    /// - `QueueFull` si los N intentos encuentran la cola llena (el Loopback
    ///   nunca libera espacio solo: el caller debe `poll` entre llamadas o en
    ///   otro hilo; el bound evita loop infinito y prepara paridad con P2P
    ///   futuro donde un lleno transitorio sí puede limpiarse).
    /// - `InvalidMessage`/desconectado fallan rápido en el primer intento.
    ///
    /// Puro en memoria, sin sleep ni spawn (cerebro puro).
    pub fn send_with_retry(
        &mut self,
        msg: ClassroomMessage,
        budget: RetryBudget,
    ) -> Result<usize, ClassroomError> {
        let max = budget.as_u8();
        let mut last_err = ClassroomError::QueueFull;
        for attempt in 1..=max {
            match self.send(msg.clone()) {
                Ok(()) => return Ok(usize::from(attempt)),
                Err(ClassroomError::QueueFull) => {
                    last_err = ClassroomError::QueueFull;
                    continue;
                }
                Err(other) => return Err(other),
            }
        }
        Err(last_err)
    }
}

impl Default for LoopbackTransport {
    fn default() -> Self {
        Self::new()
    }
}

impl ClassroomTransport for LoopbackTransport {
    fn send(&mut self, msg: ClassroomMessage) -> Result<(), ClassroomError> {
        if !self.connected {
            return Err(ClassroomError::InvalidMessage(
                "transporte desconectado".to_string(),
            ));
        }
        msg.validate()?;
        if self.queue.len() >= MAX_TRANSPORT_QUEUE {
            return Err(ClassroomError::QueueFull);
        }
        self.queue.push_back(msg);
        Ok(())
    }

    fn poll(&mut self) -> Option<ClassroomMessage> {
        self.queue.pop_front()
    }

    fn len(&self) -> usize {
        self.queue.len()
    }

    fn is_empty(&self) -> bool {
        self.queue.is_empty()
    }

    fn is_connected(&self) -> bool {
        self.connected
    }

    fn disconnect(&mut self) {
        self.connected = false;
        self.queue.clear();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn msg_fixture(from: &str, body: &str) -> ClassroomMessage {
        ClassroomMessage::try_new(from, ClassroomMessageKind::Chat, body).expect("fixture")
    }

    #[test]
    fn message_validates_sender_and_size() {
        assert!(ClassroomMessage::try_new("", ClassroomMessageKind::Chat, "hola").is_err());
        assert!(ClassroomMessage::try_new("Ana", ClassroomMessageKind::Chat, "hola").is_ok());
        let big = "x".repeat(MAX_MESSAGE_BYTES + 1);
        assert!(ClassroomMessage::try_new("Ana", ClassroomMessageKind::Chat, &big).is_err());
        assert!(ClassroomMessage::try_new("Ana", ClassroomMessageKind::Chat, "a\x00b").is_err());
        // \n y \t sí pasan (chat multilínea corto).
        assert!(ClassroomMessage::try_new("Ana", ClassroomMessageKind::Chat, "a\nb\tc").is_ok());
    }

    #[test]
    fn loopback_send_poll_fifo_and_bounded() {
        let mut transport = LoopbackTransport::new();
        assert!(transport.is_connected());
        assert!(transport.is_empty());
        transport.send(msg_fixture("Ana", "uno")).expect("send");
        transport.send(msg_fixture("Luis", "dos")).expect("send");
        assert_eq!(transport.len(), 2);
        let first = transport.poll().expect("poll");
        assert_eq!(first.from, "Ana");
        assert_eq!(first.body, "uno");
        let second = transport.poll().expect("poll");
        assert_eq!(second.from, "Luis");
        assert!(transport.poll().is_none());
    }

    #[test]
    fn loopback_queue_full_is_honest_error() {
        let mut transport = LoopbackTransport::new();
        for index in 0..MAX_TRANSPORT_QUEUE {
            let body = format!("m{index}");
            transport.send(msg_fixture("Ana", &body)).expect("send");
        }
        let err = transport
            .send(msg_fixture("Ana", "overflow"))
            .expect_err("cola llena");
        assert_eq!(err, ClassroomError::QueueFull);
        assert_eq!(transport.len(), MAX_TRANSPORT_QUEUE);
    }

    #[test]
    fn loopback_disconnect_clears_and_rejects_send() {
        let mut transport = LoopbackTransport::new();
        transport.send(msg_fixture("Ana", "hola")).expect("send");
        transport.disconnect();
        assert!(!transport.is_connected());
        assert!(transport.is_empty());
        assert!(transport.poll().is_none());
        assert!(transport.send(msg_fixture("Ana", "otro")).is_err());
    }

    #[test]
    fn message_serde_roundtrip() {
        let msg = msg_fixture("Mia", "ejercicio 1");
        let json = serde_json::to_string(&msg).expect("serialize");
        let back: ClassroomMessage = serde_json::from_str(&json).expect("deserialize");
        assert_eq!(msg, back);
    }

    #[test]
    fn retry_budget_validates_range() {
        assert!(RetryBudget::try_new(0).is_err());
        assert!(RetryBudget::try_new(1).is_ok());
        assert!(RetryBudget::try_new(3).is_ok());
        assert!(RetryBudget::try_new(8).is_ok());
        assert!(RetryBudget::try_new(9).is_err());
        assert_eq!(RetryBudget::default_budget().as_u8(), DEFAULT_SEND_ATTEMPTS);
    }

    #[test]
    fn send_with_retry_succeeds_first_attempt_when_space() {
        let mut transport = LoopbackTransport::new();
        let attempts = transport
            .send_with_retry(msg_fixture("Ana", "hola"), RetryBudget::default_budget())
            .expect("espacio libre");
        assert_eq!(attempts, 1);
        assert_eq!(transport.len(), 1);
    }

    #[test]
    fn send_with_retry_fails_bounded_when_full() {
        let mut transport = LoopbackTransport::new();
        for index in 0..MAX_TRANSPORT_QUEUE {
            transport
                .send(msg_fixture("Ana", &format!("m{index}")))
                .expect("fill");
        }
        let budget = RetryBudget::try_new(3).expect("budget");
        let err = transport
            .send_with_retry(msg_fixture("Ana", "overflow"), budget)
            .expect_err("cola llena aun con reintento");
        assert_eq!(err, ClassroomError::QueueFull);
        assert_eq!(transport.len(), MAX_TRANSPORT_QUEUE);
        // Tras liberar un slot, el reintento sí entra (el caller drena entre llamadas).
        transport.poll().expect("drena uno");
        let attempts = transport
            .send_with_retry(msg_fixture("Ana", "ahora sí"), budget)
            .expect("tras drenar");
        assert_eq!(attempts, 1);
    }

    #[test]
    fn send_with_retry_fails_fast_when_disconnected() {
        let mut transport = LoopbackTransport::new();
        transport.disconnect();
        let err = transport
            .send_with_retry(msg_fixture("Ana", "x"), RetryBudget::default_budget())
            .expect_err("desconectado");
        assert!(matches!(err, ClassroomError::InvalidMessage(_)));
    }

    #[test]
    fn validate_rejects_control_chars_on_the_wire_path() {
        // Regresión A2: el filtro de controles vivía SOLO en `try_new`
        // ("para evitar inyección en UI") pero `send()` y el `pump()` entrante
        // solo llaman `validate()`: un mensaje remoto viajaba con `\x1b`/ANSI
        // al chat de la UI. Fail-open en el borde exacto donde importa.
        let hostile_body = ClassroomMessage {
            from: "Ana".to_string(),
            kind: ClassroomMessageKind::Chat,
            body: "limpiar\x1b[31mrojo".to_string(),
        };
        assert!(hostile_body.validate().is_err());
        let hostile_from = ClassroomMessage {
            from: "\u{1b}[31mAna".to_string(),
            kind: ClassroomMessageKind::Chat,
            body: "hola".to_string(),
        };
        assert!(hostile_from.validate().is_err());
    }

    #[test]
    fn wire_deserialization_rejects_hostile_messages() {
        // Regresión A3: campos `pub` + `Deserialize` sin validación = el
        // constructor validado se evadía con un JSON remoto.
        assert!(serde_json::from_str::<ClassroomMessage>(
            r#"{"from":"Ana","kind":"chat","body":"a\u0000b"}"#
        )
        .is_err());
        assert!(serde_json::from_str::<ClassroomMessage>(
            r#"{"from":"\u001b[31mAna","kind":"chat","body":"hola"}"#
        )
        .is_err());
        let big_body = "x".repeat(MAX_MESSAGE_BYTES + 1);
        assert!(serde_json::from_str::<ClassroomMessage>(
            &serde_json::json!({"from": "Ana", "kind": "chat", "body": big_body}).to_string()
        )
        .is_err());
        let big_from = "x".repeat(65);
        assert!(serde_json::from_str::<ClassroomMessage>(
            &serde_json::json!({"from": big_from, "kind": "chat", "body": "hola"}).to_string()
        )
        .is_err());
    }

    #[test]
    fn sender_budget_unifies_to_chars() {
        // Regresión A3 [PRUEBA chars-vs-bytes]: `LearnerName::try_new` capaba
        // a 64 CHARS pero `validate` exigía 64 BYTES: 40 ideogramas (120
        // bytes) pasaban el constructor y `send()` fallaba SIEMPRE.
        let name = "数".repeat(40);
        let msg = ClassroomMessage::try_new(&name, ClassroomMessageKind::Chat, "hola")
            .expect("constructor acepta 40 chars");
        assert!(msg.validate().is_ok());
        let mut transport = LoopbackTransport::new();
        assert!(transport.send(msg).is_ok());
    }
}
