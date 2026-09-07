//! Transporte del aula: contrato + Loopback puro (sin red).
//!
//! Cerebro puro: sin I/O, sin spawn, sin threads. `LoopbackTransport` es una
//! cola acotada en memoria (128 mensajes, 2048 bytes por mensaje) que nunca
//! sale del proceso — PII siempre local. El P2P real (iroh) queda como stub
//! honesto en [`crate::stubs`] (L, solo diseño).

use serde::{Deserialize, Serialize};
use std::collections::VecDeque;

use crate::session::ClassroomError;

/// Tope de mensajes encolados (igual que el canal del agente: 128).
pub const MAX_TRANSPORT_QUEUE: usize = 128;
/// Tope por mensaje (cuerpo + remitente acotados; coherente con 2000 de ejercicio).
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
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ClassroomMessage {
    /// Remitente display (ya saneado `1..=64`).
    pub from: String,
    /// Tipo de mensaje.
    pub kind: ClassroomMessageKind,
    /// Cuerpo acotado (`<= MAX_MESSAGE_BYTES` en bytes).
    pub body: String,
}

impl ClassroomMessage {
    /// Construye validando remitente y cuerpo (todo `Result`, sin pánicos).
    pub fn try_new(
        from: &str,
        kind: ClassroomMessageKind,
        body: &str,
    ) -> Result<Self, ClassroomError> {
        let clean_from = crate::session::LearnerName::try_new(from)
            .map_err(|_| ClassroomError::InvalidName(from.trim().to_string()))?;
        if body.len() > MAX_MESSAGE_BYTES {
            return Err(ClassroomError::InvalidMessage(format!(
                "cuerpo excede {MAX_MESSAGE_BYTES} bytes"
            )));
        }
        // Rechazar controles (salvo \n\t) para evitar inyección en UI.
        if body
            .chars()
            .any(|c| c.is_control() && c != '\n' && c != '\t')
        {
            return Err(ClassroomError::InvalidMessage(
                "cuerpo con caracteres de control".to_string(),
            ));
        }
        Ok(Self {
            from: clean_from.as_str().to_string(),
            kind,
            body: body.to_string(),
        })
    }

    /// Valida un mensaje ya construido (tamaño + remitente no vacío).
    pub fn validate(&self) -> Result<(), ClassroomError> {
        if self.from.trim().is_empty() || self.from.len() > 64 {
            return Err(ClassroomError::InvalidMessage(
                "remitente inválido".to_string(),
            ));
        }
        if self.body.len() > MAX_MESSAGE_BYTES {
            return Err(ClassroomError::InvalidMessage(format!(
                "cuerpo excede {MAX_MESSAGE_BYTES} bytes"
            )));
        }
        Ok(())
    }
}

/// Contrato de transporte (la sesión no depende de una red concreta).
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
pub struct RetryBudget(u8);

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
}
