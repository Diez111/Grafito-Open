//! Cola offline-first en memoria acotada (funcional, sin red).
//!
//! Cerebro puro: sin I/O, sin spawn. `OfflineOutbox` es la outbox volátil que
//! el Loopback usa cuando no hay P2P: encola hasta 128 envelopes de 2048 bytes
//! con reintento exponencial acotado (5 intentos, backoff `2^attempts`, cap 1h).
//! Al superar los intentos se descarta honesto (`false`, sin pánico).
//!
//! Persistencia en disco (D2): este módulo aporta el codec acotado puro
//! (`encode_persist` / `decode_persist`); el archivo vive en la app
//! (`user-data dir/outbox.json`, I/O en background/arranque, jamás en `Ui::`).
//! Corrupción o sobre-tamaño → outbox vacía + aviso honesto (`PersistLoad::notice`),
//! nunca pánico ni dato a medias. PII siempre local: nada sale del proceso.

use serde::{Deserialize, Serialize};
use std::collections::VecDeque;

use crate::session::ClassroomError;

/// Tope de envelopes (igual que `MAX_TRANSPORT_QUEUE`).
pub const MAX_OFFLINE_QUEUE: usize = 128;
/// Tope por cuerpo (igual que `MAX_MESSAGE_BYTES`).
pub const MAX_OFFLINE_BODY_BYTES: usize = 2_048;
/// Tope por `kind` (igual que nombres de tool, 64).
pub const MAX_OFFLINE_KIND_LEN: usize = 64;
/// Intentos máximos antes de descartar (1 envío + 4 reintentos).
pub const MAX_OFFLINE_ATTEMPTS: u8 = 5;
/// Backoff máximo entre reintentos (1h, evita esperas eternas).
pub const MAX_OFFLINE_BACKOFF_SECS: u64 = 3_600;
/// Tope del JSON persistido en disco (128×2048 + holgura de framing ≈ 320 KiB).
/// Fail-closed: lo que exceda se descarta honesto al cargar, jamás se trunca a medias.
pub const MAX_OFFLINE_PERSIST_BYTES: usize = 327_680;
/// Horizonte de `next_retry_epoch` (año 3000): un retry más lejos está
/// atascado para siempre en la práctica (`u64::MAX` jamás vence) y se rechaza
/// como inválido. Mismo orden de magnitud que `crdt::MAX_HLC_WALL_SECS`.
pub const MAX_OFFLINE_RETRY_EPOCH_SECS: u64 = 32_503_680_000;

/// Envelope offline: qué reintentar + cuándo.
///
/// `Deserialize` es estricto (vía `try_from`): mismas invariantes que
/// `enqueue` (`kind` saneado `1..=64`, cuerpo acotado sin controles,
/// `attempts <= MAX_OFFLINE_ATTEMPTS`, `next_retry_epoch` dentro del horizonte).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(try_from = "RawEnvelope")]
pub struct OfflineEnvelope {
    /// ID monótono local (para dedup en la UI).
    pub id: u64,
    /// Tipo (`chat`, `exercise`, `snapshot`, …) ya saneado `1..=64`.
    pub kind: String,
    /// Cuerpo acotado (`<= 2048` bytes, sin controles salvo `\n\t`).
    pub body: String,
    /// Intentos ya consumidos (`0` = recién encolado).
    pub attempts: u8,
    /// Próximo reintento (`epoch` secs, reloj del caller).
    pub next_retry_epoch: u64,
}

/// Forma cruda entrante de un envelope (se revalida en `try_from`).
#[derive(Debug, Deserialize)]
struct RawEnvelope {
    id: u64,
    kind: String,
    body: String,
    attempts: u8,
    next_retry_epoch: u64,
}

impl TryFrom<RawEnvelope> for OfflineEnvelope {
    type Error = ClassroomError;

    fn try_from(raw: RawEnvelope) -> Result<Self, Self::Error> {
        let kind = sanitize_kind(&raw.kind)?;
        validate_body(&raw.body)?;
        if raw.attempts > MAX_OFFLINE_ATTEMPTS {
            return Err(ClassroomError::InvalidMessage(format!(
                "attempts {} excede {MAX_OFFLINE_ATTEMPTS}",
                raw.attempts
            )));
        }
        if raw.next_retry_epoch > MAX_OFFLINE_RETRY_EPOCH_SECS {
            return Err(ClassroomError::InvalidMessage(format!(
                "next_retry_epoch {} excede el horizonte {MAX_OFFLINE_RETRY_EPOCH_SECS}",
                raw.next_retry_epoch
            )));
        }
        Ok(Self {
            id: raw.id,
            kind,
            body: raw.body,
            attempts: raw.attempts,
            next_retry_epoch: raw.next_retry_epoch,
        })
    }
}

/// Outbox volátil acotada (FIFO por `next_retry_epoch`, estable por `id`).
///
/// `Deserialize` es estricto (vía `try_from`): cada envelope se revalida y la
/// cola respeta `MAX_OFFLINE_QUEUE` (para la carga tolerante a lo parcial
/// está [`decode_persist`]).
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(try_from = "RawOutbox")]
pub struct OfflineOutbox {
    queue: VecDeque<OfflineEnvelope>,
    next_id: u64,
    /// Envelopes descartados por política acotada (evicción en `mark_failed`).
    dropped: usize,
}

/// Forma cruda entrante de la outbox (la usa `decode_persist`, que decide
/// por-envelope con tolerancia a lo parcial).
#[derive(Debug, Deserialize)]
struct RawOutbox {
    #[serde(default)]
    queue: Vec<RawEnvelope>,
    #[serde(default = "default_next_id")]
    next_id: u64,
}

fn default_next_id() -> u64 {
    1
}

impl TryFrom<RawOutbox> for OfflineOutbox {
    type Error = ClassroomError;

    fn try_from(raw: RawOutbox) -> Result<Self, Self::Error> {
        if raw.queue.len() > MAX_OFFLINE_QUEUE {
            return Err(ClassroomError::QueueFull);
        }
        let mut queue = VecDeque::with_capacity(raw.queue.len());
        for envelope in raw.queue {
            queue.push_back(OfflineEnvelope::try_from(envelope)?);
        }
        Ok(Self {
            queue,
            next_id: raw.next_id,
            dropped: 0,
        })
    }
}

impl OfflineOutbox {
    /// Outbox vacía.
    #[must_use]
    pub fn new() -> Self {
        Self {
            queue: VecDeque::new(),
            next_id: 1,
            dropped: 0,
        }
    }

    /// Encolados.
    #[must_use]
    pub fn len(&self) -> usize {
        self.queue.len()
    }

    /// ¿Vacía?
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.queue.is_empty()
    }

    /// Envelopes descartados por política acotada (aviso honesto, nunca
    /// pérdida silenciosa): hoy solo la evicción de [`Self::mark_failed`]
    /// cuando la cola se llenó entre `pop_ready` y el reencolado.
    #[must_use]
    pub fn dropped_count(&self) -> usize {
        self.dropped
    }

    /// Encola (`now` = reloj del caller para `next_retry` inicial).
    ///
    /// `Err(QueueFull)` si hay 128 (fail-closed). `Err(InvalidMessage)` si
    /// `kind`/`body` inválidos. Retorna el `id` asignado. El retry inicial se
    /// acota al horizonte `MAX_OFFLINE_RETRY_EPOCH_SECS` (nunca un envelope
    /// atascado para siempre).
    pub fn enqueue(&mut self, kind: &str, body: &str, now: u64) -> Result<u64, ClassroomError> {
        let clean_kind = sanitize_kind(kind)?;
        validate_body(body)?;
        if self.queue.len() >= MAX_OFFLINE_QUEUE {
            return Err(ClassroomError::QueueFull);
        }
        let id = self.next_id;
        self.next_id = self.next_id.saturating_add(1);
        if self.next_id == 0 {
            self.next_id = 1;
        }
        self.queue.push_back(OfflineEnvelope {
            id,
            kind: clean_kind,
            body: body.to_string(),
            attempts: 0,
            next_retry_epoch: now.min(MAX_OFFLINE_RETRY_EPOCH_SECS),
        });
        Ok(id)
    }

    /// Saca el envelope listo más antiguo (`next_retry <= now`), o `None`.
    ///
    /// FIFO estable: entre listos, el de menor `(next_retry, id)` primero.
    /// La cola interna se mantiene ordenada por inserción; se busca lineal
    /// (128 máximo, O(n) barato y determinista).
    pub fn pop_ready(&mut self, now: u64) -> Option<OfflineEnvelope> {
        let position = self
            .queue
            .iter()
            .enumerate()
            .filter(|(_, e)| e.next_retry_epoch <= now)
            .min_by_key(|(_, e)| (e.next_retry_epoch, e.id))
            .map(|(index, _)| index)?;
        self.queue.remove(position)
    }

    /// Marca un fallo y lo reencola con backoff, o lo descarta si agotó intentos.
    ///
    /// `Ok(true)` = reencolado con `attempts+1` y `next = now + backoff` (el
    /// retry se acota al horizonte `MAX_OFFLINE_RETRY_EPOCH_SECS`).
    /// `Ok(false)` = descartado honesto (ya consumió `MAX_OFFLINE_ATTEMPTS`).
    /// Backoff: `2^attempts` secs (1,2,4,8,16…), cap 1h. Si el `id` no existe,
    /// retorna `Ok(false)` (no-op honesto, sin crear nada).
    ///
    /// Política si la cola se llenó entre `pop_ready` y este reencolado: el
    /// reintento NUNCA se pierde — se evicciona el envelope menos urgente
    /// (mayor `(next_retry, id)`) y el descarte se cuenta en
    /// [`Self::dropped_count`] (aviso honesto, jamás pérdida silenciosa).
    pub fn mark_failed(
        &mut self,
        envelope: OfflineEnvelope,
        now: u64,
    ) -> Result<bool, ClassroomError> {
        if envelope.attempts >= MAX_OFFLINE_ATTEMPTS {
            return Ok(false);
        }
        if self.queue.len() >= MAX_OFFLINE_QUEUE {
            let least_urgent = self
                .queue
                .iter()
                .enumerate()
                .max_by_key(|(_, e)| (e.next_retry_epoch, e.id))
                .map(|(index, _)| index);
            if let Some(index) = least_urgent {
                self.queue.remove(index);
                self.dropped = self.dropped.saturating_add(1);
            }
        }
        let next_attempts = envelope.attempts.saturating_add(1);
        let backoff = backoff_secs(next_attempts);
        self.queue.push_back(OfflineEnvelope {
            attempts: next_attempts,
            next_retry_epoch: now
                .saturating_add(backoff)
                .min(MAX_OFFLINE_RETRY_EPOCH_SECS),
            ..envelope
        });
        Ok(true)
    }

    /// Limpia todo (al cerrar el aula, PII no persiste).
    pub fn clear(&mut self) {
        self.queue.clear();
    }

    /// Serializa para disco (JSON acotado a `MAX_OFFLINE_PERSIST_BYTES`).
    ///
    /// Puro, sin I/O: el caller escribe el `String` donde corresponda.
    /// `Err` si el JSON excede el cap (fail-closed; con los caps de
    /// `enqueue` no debería pasar, pero se chequea igual).
    pub fn encode_persist(&self) -> Result<String, ClassroomError> {
        let json = serde_json::to_string(self)
            .map_err(|_| ClassroomError::InvalidMessage("outbox no serializable".to_string()))?;
        if json.len() > MAX_OFFLINE_PERSIST_BYTES {
            return Err(ClassroomError::InvalidMessage(format!(
                "outbox persistida excede {MAX_OFFLINE_PERSIST_BYTES} bytes"
            )));
        }
        Ok(json)
    }
}

/// Resultado honesto de cargar la outbox desde disco.
///
/// Nunca falla con `Err`: lo corrupto se descarta y se avisa
/// (`notice()`), jamás se inventa un envelope.
#[derive(Debug, Clone)]
pub struct PersistLoad {
    /// Outbox válida (vacía si todo estaba corrupto).
    pub outbox: OfflineOutbox,
    /// Envelopes individuales descartados por inválidos (el resto se conserva).
    pub discarded: usize,
    /// `true` si el documento entero era ilegible o excedía el cap.
    pub corrupt: bool,
}

impl PersistLoad {
    /// Aviso honesto para toast/panel, o `None` si la carga salió limpia.
    #[must_use]
    pub fn notice(&self) -> Option<String> {
        if self.corrupt {
            return Some(
                "La cola offline guardada estaba corrupta y se descartó, che.".to_string(),
            );
        }
        if self.discarded > 0 {
            return Some(format!(
                "Se descartaron {} mensajes offline inválidos al cargar, che.",
                self.discarded
            ));
        }
        None
    }
}

/// Deserializa la outbox desde disco (puro, sin I/O).
///
/// Reglas fail-closed, en orden:
/// 1. vacío/en blanco → outbox vacía limpia (primera corrida, sin aviso);
/// 2. `len > MAX_OFFLINE_PERSIST_BYTES` → todo corrupto (vacía + `corrupt`);
/// 3. JSON inválido → todo corrupto;
/// 4. JSON válido → se revalida cada envelope (`kind`/`body`/`attempts`/
///    `next_retry_epoch` con las mismas reglas de `enqueue`); los inválidos,
///    los `id` duplicados y los retries atascados se cuentan en `discarded`
///    y el resto se conserva (cap 128, los de menor `id` primero);
/// 5. `next_id` se recalcula como `max(id)+1` (mínimo 1) para no reusar ids.
#[must_use]
pub fn decode_persist(json: &str) -> PersistLoad {
    if json.trim().is_empty() {
        return PersistLoad {
            outbox: OfflineOutbox::new(),
            discarded: 0,
            corrupt: false,
        };
    }
    if json.len() > MAX_OFFLINE_PERSIST_BYTES {
        return PersistLoad {
            outbox: OfflineOutbox::new(),
            discarded: 0,
            corrupt: true,
        };
    }
    let raw: RawOutbox = match serde_json::from_str(json) {
        Ok(raw) => raw,
        Err(_) => {
            return PersistLoad {
                outbox: OfflineOutbox::new(),
                discarded: 0,
                corrupt: true,
            };
        }
    };
    let mut kept: Vec<OfflineEnvelope> = Vec::new();
    let mut discarded = 0_usize;
    for envelope in raw.queue {
        // Dedup real por `id`: el contrato de la UI lo asume (nunca dos filas
        // con el mismo id en el panel).
        match OfflineEnvelope::try_from(envelope) {
            Ok(clean) => {
                if kept.iter().any(|existing| existing.id == clean.id) {
                    discarded = discarded.saturating_add(1);
                } else {
                    kept.push(clean);
                }
            }
            Err(_) => discarded = discarded.saturating_add(1),
        }
    }
    kept.sort_by_key(|e| e.id);
    kept.truncate(MAX_OFFLINE_QUEUE);
    let next_id = kept
        .iter()
        .map(|e| e.id)
        .max()
        .unwrap_or(0)
        .saturating_add(1);
    let next_id = if next_id == 0 { 1 } else { next_id };
    PersistLoad {
        outbox: OfflineOutbox {
            queue: kept.into_iter().collect(),
            next_id,
            dropped: 0,
        },
        discarded,
        corrupt: false,
    }
}

impl Default for OfflineOutbox {
    fn default() -> Self {
        Self::new()
    }
}

/// Backoff exponencial `2^attempts` con cap 1h (puro, sin pánico).
fn backoff_secs(attempts: u8) -> u64 {
    // `shift <= 12` por el `min`: `1 << shift` nunca desborda `u64`.
    let shift = u32::from(attempts.min(12));
    let raw = 1_u64.checked_shl(shift).unwrap_or(u64::MAX);
    raw.min(MAX_OFFLINE_BACKOFF_SECS)
}

fn sanitize_kind(raw: &str) -> Result<String, ClassroomError> {
    let trimmed = raw.trim();
    if trimmed.is_empty() {
        return Err(ClassroomError::InvalidMessage("kind vacío".to_string()));
    }
    if trimmed.len() > MAX_OFFLINE_KIND_LEN {
        return Err(ClassroomError::InvalidMessage(format!(
            "kind excede {MAX_OFFLINE_KIND_LEN} bytes"
        )));
    }
    if !trimmed
        .chars()
        .all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '-' || c == '.')
    {
        return Err(ClassroomError::InvalidMessage(
            "kind solo admite [A-Za-z0-9_.-]".to_string(),
        ));
    }
    Ok(trimmed.to_string())
}

fn validate_body(body: &str) -> Result<(), ClassroomError> {
    if body.len() > MAX_OFFLINE_BODY_BYTES {
        return Err(ClassroomError::InvalidMessage(format!(
            "cuerpo offline excede {MAX_OFFLINE_BODY_BYTES} bytes"
        )));
    }
    if body
        .chars()
        .any(|c| c.is_control() && c != '\n' && c != '\t')
    {
        return Err(ClassroomError::InvalidMessage(
            "cuerpo offline con caracteres de control".to_string(),
        ));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn enqueue_and_pop_ready_fifo() {
        let mut outbox = OfflineOutbox::new();
        assert!(outbox.is_empty());
        let id1 = outbox.enqueue("chat", "hola", 100).expect("enqueue");
        let id2 = outbox.enqueue("exercise", "x+2", 100).expect("enqueue");
        assert!(id2 > id1);
        assert_eq!(outbox.len(), 2);
        let first = outbox.pop_ready(100).expect("listo");
        assert_eq!(first.id, id1);
        let second = outbox.pop_ready(100).expect("listo");
        assert_eq!(second.id, id2);
        assert!(outbox.pop_ready(100).is_none());
    }

    #[test]
    fn pop_ready_respects_next_retry() {
        let mut outbox = OfflineOutbox::new();
        outbox.enqueue("chat", "futuro", 1_000).expect("enqueue");
        assert!(outbox.pop_ready(999).is_none());
        assert!(outbox.pop_ready(1_000).is_some());
    }

    #[test]
    fn enqueue_validates_and_fails_full_honestly() {
        let mut outbox = OfflineOutbox::new();
        assert!(outbox.enqueue("", "x", 0).is_err());
        assert!(outbox.enqueue("bad kind!", "x", 0).is_err());
        let big = "x".repeat(MAX_OFFLINE_BODY_BYTES + 1);
        assert!(outbox.enqueue("chat", &big, 0).is_err());
        assert!(outbox.enqueue("chat", "a\x00b", 0).is_err());
        for _ in 0..MAX_OFFLINE_QUEUE {
            outbox.enqueue("chat", "m", 0).expect("fill");
        }
        assert_eq!(
            outbox.enqueue("chat", "overflow", 0).expect_err("llena"),
            ClassroomError::QueueFull
        );
    }

    #[test]
    fn mark_failed_backoffs_and_discards_after_max() {
        let mut outbox = OfflineOutbox::new();
        outbox.enqueue("chat", "flaky", 0).expect("enqueue");
        let mut envelope = outbox.pop_ready(0).expect("listo");
        // 5 intentos: reencola 5 veces con backoff creciente, la 6ª descarta.
        let mut now = 0_u64;
        for expected_attempt in 1..=MAX_OFFLINE_ATTEMPTS {
            let kept = outbox.mark_failed(envelope, now).expect("mark");
            assert!(kept, "intento {expected_attempt} debe reencolar");
            // Backoff esperado: 2^attempt cap 1h.
            let next = outbox.pop_ready(now);
            assert!(next.is_none(), "aún no listo (backoff)");
            now = now.saturating_add(backoff_secs(expected_attempt));
            envelope = outbox.pop_ready(now).expect("listo tras backoff");
            assert_eq!(envelope.attempts, expected_attempt);
        }
        let dropped = outbox.mark_failed(envelope, now).expect("drop");
        assert!(!dropped, "tras 5 intentos se descarta honesto");
        assert!(outbox.is_empty());
    }

    #[test]
    fn backoff_is_bounded() {
        assert_eq!(backoff_secs(1), 2);
        assert_eq!(backoff_secs(5), 32);
        assert!(backoff_secs(20) <= MAX_OFFLINE_BACKOFF_SECS);
        assert_eq!(backoff_secs(255), MAX_OFFLINE_BACKOFF_SECS);
    }

    #[test]
    fn offline_serde_roundtrip() {
        let mut outbox = OfflineOutbox::new();
        outbox.enqueue("chat", "hola", 7).expect("enqueue");
        let json = serde_json::to_string(&outbox).expect("serialize");
        let back: OfflineOutbox = serde_json::from_str(&json).expect("deserialize");
        assert_eq!(back.len(), 1);
    }

    #[test]
    fn persist_roundtrip_conserva_cola_y_next_id() {
        let mut outbox = OfflineOutbox::new();
        outbox.enqueue("chat", "hola", 7).expect("enqueue");
        outbox.enqueue("exercise", "x+2", 9).expect("enqueue");
        let json = outbox.encode_persist().expect("encode");
        assert!(json.len() <= MAX_OFFLINE_PERSIST_BYTES);
        let loaded = decode_persist(&json);
        assert!(!loaded.corrupt);
        assert_eq!(loaded.discarded, 0);
        assert!(loaded.notice().is_none());
        assert_eq!(loaded.outbox.len(), 2);
        // Los ids no se reusan tras recargar.
        let mut back = loaded.outbox;
        let id = back.enqueue("chat", "nuevo", 10).expect("enqueue");
        assert!(id > 2);
    }

    #[test]
    fn persist_vacio_es_primera_corrida_sin_aviso() {
        for blank in ["", "   ", "\n\t "] {
            let loaded = decode_persist(blank);
            assert!(loaded.outbox.is_empty());
            assert!(!loaded.corrupt);
            assert!(loaded.notice().is_none());
        }
    }

    #[test]
    fn persist_corrupto_descarta_honesto_con_aviso() {
        for bad in ["{no json", "{\"queue\": [", "null", "[1,2]", "\"hola\""] {
            let loaded = decode_persist(bad);
            assert!(loaded.outbox.is_empty(), " `{bad}` debe vaciar");
            assert!(loaded.corrupt, " `{bad}` debe marcar corrupto");
            assert!(loaded.notice().is_some(), " `{bad}` debe avisar");
        }
    }

    #[test]
    fn persist_sobretamano_descarta_honesto() {
        let big = "x".repeat(MAX_OFFLINE_PERSIST_BYTES + 1);
        let loaded = decode_persist(&big);
        assert!(loaded.outbox.is_empty());
        assert!(loaded.corrupt);
        assert!(loaded.notice().is_some());
    }

    #[test]
    fn persist_filtra_envelopes_invalidos_y_conserva_resto() {
        let json = serde_json::json!({
            "queue": [
                {"id": 1, "kind": "chat", "body": "ok", "attempts": 0, "next_retry_epoch": 5},
                {"id": 2, "kind": "mal kind!", "body": "x", "attempts": 0, "next_retry_epoch": 5},
                {"id": 3, "kind": "chat", "body": "a\x00b", "attempts": 0, "next_retry_epoch": 5},
            ],
            "next_id": 4,
        })
        .to_string();
        let loaded = decode_persist(&json);
        assert!(!loaded.corrupt);
        assert_eq!(loaded.discarded, 2);
        assert_eq!(loaded.outbox.len(), 1);
        assert!(loaded.notice().is_some());
        let mut back = loaded.outbox;
        let first = back.pop_ready(5);
        assert_eq!(first.map(|e| e.id), Some(1));
    }

    #[test]
    fn serde_rejects_hostile_envelope_and_oversized_outbox() {
        // Regresión A3: `enqueue` validaba pero el `Deserialize` derivado no.
        assert!(serde_json::from_str::<OfflineEnvelope>(
            r#"{"id":1,"kind":"bad kind!","body":"x","attempts":0,"next_retry_epoch":5}"#
        )
        .is_err());
        assert!(serde_json::from_str::<OfflineEnvelope>(
            r#"{"id":1,"kind":"chat","body":"a\u0000b","attempts":0,"next_retry_epoch":5}"#
        )
        .is_err());
        // Regresión A8: `next_retry_epoch = u64::MAX` dejaba el envelope
        // atascado para siempre (nunca `pop_ready`).
        assert!(serde_json::from_str::<OfflineEnvelope>(
            r#"{"id":1,"kind":"chat","body":"x","attempts":0,"next_retry_epoch":18446744073709551615}"#
        )
        .is_err());
        assert!(serde_json::from_str::<OfflineEnvelope>(
            r#"{"id":1,"kind":"chat","body":"x","attempts":200,"next_retry_epoch":5}"#
        )
        .is_err());
        let big = "x".repeat(MAX_OFFLINE_BODY_BYTES + 1);
        assert!(serde_json::from_str::<OfflineOutbox>(
            &serde_json::json!({
                "queue": [{"id": 1, "kind": "chat", "body": big, "attempts": 0, "next_retry_epoch": 5}],
                "next_id": 2
            })
            .to_string()
        )
        .is_err());
    }

    #[test]
    fn decode_persist_dedups_ids_and_unsticks_retries() {
        // Regresión A8: ids duplicados rompían el "dedup en la UI" del doc y
        // `next_retry_epoch = u64::MAX` dejaba el envelope pegado para siempre.
        let json = serde_json::json!({
            "queue": [
                {"id": 1, "kind": "chat", "body": "uno", "attempts": 0, "next_retry_epoch": 5},
                {"id": 1, "kind": "chat", "body": "duplicado", "attempts": 0, "next_retry_epoch": 5},
                {"id": 2, "kind": "chat", "body": "pegado", "attempts": 3, "next_retry_epoch": u64::MAX},
            ],
            "next_id": 3,
        })
        .to_string();
        let loaded = decode_persist(&json);
        assert!(!loaded.corrupt);
        assert_eq!(loaded.discarded, 2);
        assert_eq!(loaded.outbox.len(), 1);
    }

    #[test]
    fn mark_failed_keeps_the_retrying_envelope_when_full() {
        // Regresión A8: la cola podía llenarse entre `pop_ready` y
        // `mark_failed` y el reintento se perdía sin el "descarte honesto"
        // con aviso. Política única: se evicciona el menos urgente (mayor
        // `(next_retry, id)`), el reintento entra y el descarte se cuenta.
        let mut outbox = OfflineOutbox::new();
        for index in 0..MAX_OFFLINE_QUEUE {
            outbox
                .enqueue("chat", &format!("m{index}"), 0)
                .expect("fill");
        }
        let envelope = outbox.pop_ready(0).expect("listo");
        outbox.enqueue("chat", "llenó el hueco", 0).expect("refill");
        let kept = outbox.mark_failed(envelope, 0).expect("reintento");
        assert!(kept);
        assert_eq!(outbox.len(), MAX_OFFLINE_QUEUE);
        assert_eq!(outbox.dropped_count(), 1);
    }
}
