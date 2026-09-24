//! Transporte P2P real del aula sobre iroh/QUIC (feature `aula-iroh`).
//!
//! Implementa [`crate::transport::ClassroomTransport`] con el mismo contrato
//! que `LoopbackTransport`: cola de entrada acotada 128, mensajes validados,
//! `QueueFull`/`InvalidMessage` honestos, `disconnect` que vacía.
//!
//! Diseño (ver `stubs::iroh_p2p_stub`):
//! - ALPN `grafito-aula/1`, 1 par activo (1:1 v1).
//! - **Admisión**: antes de fijar `self.peer`, todo par debe presentar un
//!   handshake con el token derivado del código de sala `AULA-XXXXXX`
//!   ([`derive_admission_token`], HKDF-SHA256; el código crudo nunca viaja).
//!   Sin token correcto no hay par: el intento se rechaza (y se cuenta en
//!   `rejected_peers`) y se sigue aceptando al siguiente. Nada de
//!   "first-peer-wins": quien obtenga el `EndpointId` no se lleva el aula.
//! - El anfitrión crea el endpoint (preset N0 con relay) y comparte un
//!   [`AulaTicket`] (EndpointId + direcciones) + el código de sala por fuera;
//!   el invitado hace `join(ticket, code)`.
//! - Encuadre: `u16 BE length` + JSON (`serde_json`, ya dependencia);
//!   todo lo que exceda `MAX_MESSAGE_BYTES` se rechaza sin enviar.
//! - `poll()` bombea la red en ventanas de `IROH_POLL_TIMEOUT` (50 ms): el
//!   handshake de admisión es **incremental** (deadline total
//!   [`IROH_ADMISSION_TIMEOUT`]) — jamás un bloqueo de 10 s dentro del
//!   "bombeo corto" que la UI llama por frame.
//! - Cola llena: política única del trait (rechazo del entrante nuevo +
//!   [`Self::dropped_count`], sin evicción silenciosa del más viejo).
//! - Sin relay en tests: dial directo 127.0.0.1 (RelayMode deshabilitado).

use std::collections::VecDeque;
use std::future::Future;
use std::net::SocketAddr;
use std::pin::Pin;
use std::time::{Duration, Instant};

use crate::session::{ClassroomCode, ClassroomError};
use crate::transport::{
    ClassroomMessage, ClassroomTransport, MAX_MESSAGE_BYTES, MAX_TRANSPORT_QUEUE,
};

/// ALPN del protocolo de aula (diseño del stub).
pub const AULA_ALPN: &[u8] = b"grafito-aula/1";
/// Timeout de conexión/dial (diseño del stub: 10 s).
pub const IROH_CONNECT_TIMEOUT: Duration = Duration::from_secs(10);
/// Timeout de cada bombeo en `poll` (corto: no traba al llamador).
pub const IROH_POLL_TIMEOUT: Duration = Duration::from_millis(50);
/// Timeout de `send` (apertura de stream + escritura).
pub const IROH_SEND_TIMEOUT: Duration = Duration::from_secs(10);
/// Deadline total del handshake de admisión (dial + token + ack). Se progresan
/// ventanas de `IROH_POLL_TIMEOUT`: `poll()` nunca bloquea más que una ventana.
pub const IROH_ADMISSION_TIMEOUT: Duration = Duration::from_secs(10);
/// Versión del frame de handshake de admisión.
pub const AULA_HANDSHAKE_VERSION: u8 = 1;
/// `info` del HKDF para el token de admisión (dominio separado a propósito:
/// de acá NO se derivan claves de sesión — el cifrado real sigue siendo el L
/// `EncryptedSession` de `stubs.rs`).
pub const ADMISSION_HKDF_INFO: &[u8] = b"grafito-aula/admission/1";
/// Longitud del token de admisión derivado (128 bits).
pub const ADMISSION_TOKEN_LEN: usize = 16;

/// Token de admisión de una sala.
type AdmissionToken = [u8; ADMISSION_TOKEN_LEN];

/// Deriva el token de admisión de una sala desde su código `AULA-XXXXXX`
/// (HKDF-SHA256 con el código como IKM y [`ADMISSION_HKDF_INFO`] como dominio).
///
/// El código crudo es el ticket de admisión del diseño (`stubs.rs`): viaja por
/// canales de sala (proyectado/QR) pero **nunca por la red** — por la red va
/// solo este derivado, verificable por quien sí conoce el código.
pub fn derive_admission_token(code: &str) -> Result<AdmissionToken, ClassroomError> {
    let hk = hkdf::Hkdf::<sha2::Sha256>::new(None, code.as_bytes());
    let mut token = [0_u8; ADMISSION_TOKEN_LEN];
    hk.expand(ADMISSION_HKDF_INFO, &mut token)
        .map_err(|_| connect_err("admisión: HKDF expand imposible (okm acotado a 16 bytes)"))?;
    Ok(token)
}

fn token_to_hex(token: &AdmissionToken) -> String {
    let mut hex = String::with_capacity(token.len().saturating_mul(2));
    for byte in token {
        hex.push_str(&format!("{byte:02x}"));
    }
    hex
}

fn token_from_hex(text: &str) -> Option<AdmissionToken> {
    if text.len() != ADMISSION_TOKEN_LEN * 2 {
        return None;
    }
    let mut token = [0_u8; ADMISSION_TOKEN_LEN];
    for (index, slot) in token.iter_mut().enumerate() {
        let byte = text.get(index * 2..index.saturating_add(1) * 2)?;
        *slot = u8::from_str_radix(byte, 16).ok()?;
    }
    Some(token)
}

/// Frame de handshake de admisión (JSON dentro del framing `u16 BE + JSON`).
///
/// - invitado → anfitrión: `{ "handshake": 1, "token": "<hex>" }`;
/// - anfitrión → invitado: `{ "handshake": 1, "ok": true|false }`.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
struct HandshakeFrame {
    /// Versión del handshake ([`AULA_HANDSHAKE_VERSION`]).
    handshake: u8,
    /// Token de admisión derivado del código (solo petición del invitado).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    token: Option<String>,
    /// Veredicto del anfitrión (solo ack).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    ok: Option<bool>,
}

impl HandshakeFrame {
    fn request(token: &AdmissionToken) -> Self {
        Self {
            handshake: AULA_HANDSHAKE_VERSION,
            token: Some(token_to_hex(token)),
            ok: None,
        }
    }

    fn ack(ok: bool) -> Self {
        Self {
            handshake: AULA_HANDSHAKE_VERSION,
            token: None,
            ok: Some(ok),
        }
    }

    fn encode(&self) -> Result<Vec<u8>, ClassroomError> {
        serde_json::to_vec(self).map_err(|e| connect_err(format!("handshake: serialización: {e}")))
    }

    fn decode(raw: &[u8]) -> Result<Self, ClassroomError> {
        // Frame de handshake acotado (32 hex + sobre JSON): jamás un KB.
        if raw.len() > 256 {
            return Err(connect_err("handshake: frame excede 256 bytes"));
        }
        let frame: Self =
            serde_json::from_slice(raw).map_err(|_| connect_err("handshake: JSON inválido"))?;
        if frame.handshake != AULA_HANDSHAKE_VERSION {
            return Err(connect_err("handshake: versión no soportada"));
        }
        Ok(frame)
    }

    fn token(&self) -> Option<AdmissionToken> {
        self.token.as_deref().and_then(token_from_hex)
    }
}

/// Ticket de admisión a una sala: identidad + cómo contactarla.
///
/// Se serializa a JSON compacto (y cabe en el QR `grafito://aula/{código}`
/// como fragmento). Sin direcciones = resolución por Address Lookup del
/// preset N0 (requiere internet); con direcciones = dial directo (LAN).
/// El ticket NO contiene el código de sala: el código viaja aparte y se exige
/// en el handshake de admisión ([`derive_admission_token`]).
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct AulaTicket {
    /// EndpointId del anfitrión en hexadecimal.
    pub id: String,
    /// Direcciones directas opcionales (`ip:puerto`).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub addrs: Vec<String>,
}

impl AulaTicket {
    /// Serializa a texto para compartir (QR/portapapeles).
    ///
    /// `Err` honesto si la serialización fallara (jamás ticket vacío silencioso
    /// que se compartiría como "válido").
    pub fn encode(&self) -> Result<String, ClassroomError> {
        serde_json::to_string(self).map_err(|e| connect_err(format!("ticket: serialización: {e}")))
    }

    /// Parsea un ticket compartido; `None` si malformado.
    #[must_use]
    pub fn decode(text: &str) -> Option<Self> {
        let ticket: Self = serde_json::from_str(text.trim()).ok()?;
        if ticket.id.trim().is_empty() || ticket.id.len() > 128 {
            return None;
        }
        if ticket.addrs.len() > 16 {
            return None;
        }
        Some(ticket)
    }
}

/// Futuro de handshake de admisión (avanza en ventanas de `IROH_POLL_TIMEOUT`).
type HandshakeFuture =
    Pin<Box<dyn Future<Output = Result<iroh::endpoint::Connection, String>> + Send>>;

/// Conexión entrante aceptada a nivel QUIC pero aún SIN verificar su token de
/// admisión (todavía no es `peer`).
struct PendingPeer {
    /// Dial + lectura del frame de handshake + ack (todo incremental).
    handshake: HandshakeFuture,
    /// Deadline total de la admisión (se descarta el intento al vencer).
    deadline: Instant,
}

/// Transporte iroh/QUIC 1:1 del aula.
pub struct IrohTransport {
    runtime: tokio::runtime::Runtime,
    endpoint: iroh::Endpoint,
    own_id: iroh::PublicKey,
    /// Token de admisión de la sala (solo anfitrión). `None` = invitado ya
    /// admitido (presentó su token en `join`).
    admission: Option<AdmissionToken>,
    /// Handshake de admisión en curso (aceptado, aún sin verificar).
    pending: Option<PendingPeer>,
    peer: Option<iroh::endpoint::Connection>,
    inbox: VecDeque<ClassroomMessage>,
    dropped: usize,
    rejected: usize,
    connected: bool,
}

fn connect_err(detail: impl Into<String>) -> ClassroomError {
    ClassroomError::InvalidMessage(detail.into())
}

/// Arma el payload del framing `u16 BE length` + payload.
fn wire_frame(payload: &[u8]) -> Result<Vec<u8>, ClassroomError> {
    if payload.len() > MAX_MESSAGE_BYTES + 512 {
        return Err(connect_err(format!(
            "mensaje excede {} bytes",
            MAX_MESSAGE_BYTES + 512
        )));
    }
    let mut wire: Vec<u8> = Vec::with_capacity(2 + payload.len());
    wire.extend_from_slice(&(payload.len() as u16).to_be_bytes());
    wire.extend_from_slice(payload);
    Ok(wire)
}

async fn write_frame(
    connection: &iroh::endpoint::Connection,
    payload: &[u8],
) -> Result<(), String> {
    let wire = wire_frame(payload).map_err(|e| e.to_string())?;
    let mut send = connection.open_uni().await.map_err(|e| e.to_string())?;
    send.write_all(&wire).await.map_err(|e| e.to_string())?;
    send.finish().map_err(|e| e.to_string())?;
    Ok(())
}

async fn read_frame(connection: &iroh::endpoint::Connection) -> Result<Vec<u8>, String> {
    let mut recv = connection.accept_uni().await.map_err(|e| e.to_string())?;
    let mut len = [0_u8; 2];
    recv.read_exact(&mut len).await.map_err(|e| e.to_string())?;
    let len = u16::from_be_bytes(len) as usize;
    if len == 0 || len > MAX_MESSAGE_BYTES + 512 {
        return Err("marco inválido".to_string());
    }
    let body = recv.read_to_end(len).await.map_err(|e| e.to_string())?;
    if body.len() != len {
        return Err("marco truncado".to_string());
    }
    Ok(body)
}

impl IrohTransport {
    fn runtime() -> Result<tokio::runtime::Runtime, ClassroomError> {
        tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .map_err(|e| connect_err(format!("runtime tokio: {e}")))
    }

    fn bind_production(
        runtime: &tokio::runtime::Runtime,
    ) -> Result<(iroh::Endpoint, iroh::PublicKey), ClassroomError> {
        let secret = iroh::SecretKey::generate();
        let id = secret.public();
        let endpoint = runtime
            .block_on(
                iroh::Endpoint::builder(iroh::endpoint::presets::N0)
                    .alpns(vec![AULA_ALPN.to_vec()])
                    .secret_key(secret)
                    .bind(),
            )
            .map_err(|e| connect_err(format!("bind iroh: {e}")))?;
        Ok((endpoint, id))
    }

    fn from_parts(
        runtime: tokio::runtime::Runtime,
        endpoint: iroh::Endpoint,
        own_id: iroh::PublicKey,
        admission: Option<AdmissionToken>,
    ) -> Self {
        Self {
            runtime,
            endpoint,
            own_id,
            admission,
            pending: None,
            peer: None,
            inbox: VecDeque::new(),
            dropped: 0,
            rejected: 0,
            connected: true,
        }
    }

    /// Anfitrión: endpoint N0 (relay) escuchando `AULA_ALPN` y exigiendo el
    /// token del `code` de sala en el handshake de admisión.
    ///
    /// Bloquea solo el bind local (rápido); el par llega en `poll()`.
    pub fn host(code: &ClassroomCode) -> Result<Self, ClassroomError> {
        let runtime = Self::runtime()?;
        let (endpoint, own_id) = Self::bind_production(&runtime)?;
        let token = derive_admission_token(code.as_str())?;
        Ok(Self::from_parts(runtime, endpoint, own_id, Some(token)))
    }

    /// Invitado: conecta al ticket y se presenta con el `code` de sala
    /// (handshake de admisión incluido; timeout total 10 s).
    ///
    /// `Err` honesto si el anfitrión rechaza la admisión (código incorrecto):
    /// jamás un "par" que el anfitrión no admitió.
    pub fn join(ticket: &AulaTicket, code: &ClassroomCode) -> Result<Self, ClassroomError> {
        let addr = Self::ticket_addr(ticket)?;
        let runtime = Self::runtime()?;
        let (endpoint, own_id) = Self::bind_production(&runtime)?;
        Self::join_with_endpoint(runtime, endpoint, own_id, addr, code)
    }

    fn ticket_addr(ticket: &AulaTicket) -> Result<iroh::EndpointAddr, ClassroomError> {
        use std::str::FromStr;
        let id = iroh::PublicKey::from_str(ticket.id.trim())
            .map_err(|_| connect_err("ticket: id inválida"))?;
        let mut addr = iroh::EndpointAddr::new(id);
        for raw in &ticket.addrs {
            let socket: SocketAddr = raw
                .parse()
                .map_err(|_| connect_err(format!("ticket: addr inválida '{raw}'")))?;
            addr = addr.with_ip_addr(socket);
        }
        Ok(addr)
    }

    fn join_with_endpoint(
        runtime: tokio::runtime::Runtime,
        endpoint: iroh::Endpoint,
        own_id: iroh::PublicKey,
        addr: iroh::EndpointAddr,
        code: &ClassroomCode,
    ) -> Result<Self, ClassroomError> {
        let token = derive_admission_token(code.as_str())?;
        let request = HandshakeFrame::request(&token).encode()?;
        let dial_endpoint = endpoint.clone();
        let connection = runtime
            .block_on(async {
                tokio::time::timeout(IROH_ADMISSION_TIMEOUT, async move {
                    let connection = dial_endpoint
                        .connect(addr, AULA_ALPN)
                        .await
                        .map_err(|e| e.to_string())?;
                    write_frame(&connection, &request).await?;
                    let raw = read_frame(&connection).await?;
                    let ack = HandshakeFrame::decode(&raw).map_err(|e| e.to_string())?;
                    if ack.ok != Some(true) {
                        connection.close(0_u32.into(), b"admission rejected");
                        return Err(
                            "el anfitrión rechazó la admisión: código de sala incorrecto"
                                .to_string(),
                        );
                    }
                    Ok::<iroh::endpoint::Connection, String>(connection)
                })
                .await
            })
            .map_err(|_| connect_err("join: timeout de 10 s sin completar la admisión"))
            .and_then(|inner| inner.map_err(connect_err))?;
        let mut transport = Self::from_parts(runtime, endpoint, own_id, None);
        transport.peer = Some(connection);
        Ok(transport)
    }

    /// Ticket propio (id + direcciones directas opcionales dadas por el
    /// operador, p. ej. su IP LAN para aulas sin internet).
    #[must_use]
    pub fn own_ticket(&self, addrs: &[SocketAddr]) -> AulaTicket {
        AulaTicket {
            id: self.own_id.to_string(),
            addrs: addrs.iter().map(|a| a.to_string()).collect(),
        }
    }

    /// ¿Ya hay par admitido? (1:1 v1).
    #[must_use]
    pub fn has_peer(&self) -> bool {
        self.peer.is_some()
    }

    /// Pares rechazados por la admisión (token incorrecto, handshake inválido
    /// o vencido). El intento jamás se confunde con un par legítimo.
    #[must_use]
    pub fn rejected_peers(&self) -> usize {
        self.rejected
    }

    /// Bomba una ventana de red: admisión (accept + handshake incremental) y
    /// drenaje de un mensaje. Nunca bloquea más que una ventana de
    /// `IROH_POLL_TIMEOUT` por fase (la UI lo llama por frame).
    fn pump(&mut self) {
        if !self.connected {
            return;
        }
        self.pump_admission();
        let Some(connection) = self.peer.clone() else {
            return;
        };
        let frame = self.runtime.block_on(async {
            tokio::time::timeout(IROH_POLL_TIMEOUT, read_frame(&connection)).await
        });
        let body = match frame {
            Ok(Ok(body)) => body,
            _ => return,
        };
        let msg: ClassroomMessage = match serde_json::from_slice(&body) {
            Ok(msg) => msg,
            Err(_) => return,
        };
        if msg.validate().is_err() {
            return;
        }
        self.push_inbox(msg);
    }

    /// Política única de cola llena (ver `ClassroomTransport`): rechazo del
    /// mensaje entrante nuevo + contador honesto, jamás evicción silenciosa
    /// del más viejo. `false` = descartado y contado.
    fn push_inbox(&mut self, msg: ClassroomMessage) -> bool {
        if self.inbox.len() >= MAX_TRANSPORT_QUEUE {
            self.dropped = self.dropped.saturating_add(1);
            return false;
        }
        self.inbox.push_back(msg);
        true
    }

    /// Admisión incremental: acepta un par entrante (ventana corta) y avanza
    /// su handshake de token en ventanas cortas hasta admitirlo o rechazarlo.
    fn pump_admission(&mut self) {
        if self.peer.is_some() {
            return;
        }
        // Sin token no hay sala que admitir (el invitado solo habla hacia el anfitrión).
        let Some(expected_token) = self.admission else {
            return;
        };
        if self.pending.is_none() {
            let accepted = self.runtime.block_on(async {
                tokio::time::timeout(IROH_POLL_TIMEOUT, self.endpoint.accept()).await
            });
            if let Ok(Some(incoming)) = accepted {
                let handshake: HandshakeFuture = Box::pin(async move {
                    let connection = incoming.await.map_err(|e| e.to_string())?;
                    let raw = read_frame(&connection).await?;
                    let frame = HandshakeFrame::decode(&raw).map_err(|e| e.to_string())?;
                    let got = frame
                        .token()
                        .ok_or_else(|| "handshake sin token de admisión".to_string())?;
                    if got != expected_token {
                        connection.close(0_u32.into(), b"bad admission code");
                        return Err("código de sala incorrecto".to_string());
                    }
                    let ack = HandshakeFrame::ack(true)
                        .encode()
                        .map_err(|e| e.to_string())?;
                    write_frame(&connection, &ack).await?;
                    Ok(connection)
                });
                self.pending = Some(PendingPeer {
                    handshake,
                    deadline: Instant::now() + IROH_ADMISSION_TIMEOUT,
                });
            }
            return;
        }
        // Handshake incremental: una ventana corta por `poll()`; si vence el
        // deadline total, cae el intento y se sigue aceptando al siguiente.
        let Self {
            runtime,
            pending,
            peer,
            rejected,
            ..
        } = self;
        let Some(in_progress) = pending.as_mut() else {
            return;
        };
        let deadline = in_progress.deadline;
        let progressed = runtime.block_on(async {
            tokio::time::timeout(IROH_POLL_TIMEOUT, in_progress.handshake.as_mut()).await
        });
        match progressed {
            Ok(Ok(connection)) => {
                *pending = None;
                *peer = Some(connection);
            }
            Ok(Err(_detail)) => {
                *pending = None;
                *rejected = rejected.saturating_add(1);
            }
            Err(_elapsed) => {
                if Instant::now() >= deadline {
                    *pending = None;
                    *rejected = rejected.saturating_add(1);
                }
            }
        }
    }

    fn mark_broken(&mut self, detail: String) -> ClassroomError {
        self.connected = false;
        self.peer = None;
        self.pending = None;
        connect_err(format!("red: {detail}"))
    }
}

impl ClassroomTransport for IrohTransport {
    fn send(&mut self, msg: ClassroomMessage) -> Result<(), ClassroomError> {
        if !self.connected {
            return Err(connect_err("transporte desconectado"));
        }
        msg.validate()?;
        let body =
            serde_json::to_vec(&msg).map_err(|e| connect_err(format!("serialización: {e}")))?;
        // Validación del framing antes de tocar la red (fail-closed).
        wire_frame(&body)?;
        let Some(connection) = self.peer.clone() else {
            return Err(connect_err("sin par conectado"));
        };
        let result = self.runtime.block_on(async {
            tokio::time::timeout(IROH_SEND_TIMEOUT, write_frame(&connection, &body))
                .await
                .map_err(|_| "timeout".to_string())
        });
        // Micro-flush: las tareas QUIC de fondo solo avanzan mientras este
        // runtime corre; sin esta cesión un `send` fire-and-forget podría
        // no salir del socket si el llamador no vuelve a bombear.
        self.runtime.block_on(async {
            tokio::task::yield_now().await;
        });
        match result {
            Ok(Ok(())) => Ok(()),
            Ok(Err(detail)) | Err(detail) => Err(self.mark_broken(detail)),
        }
    }

    fn poll(&mut self) -> Option<ClassroomMessage> {
        self.pump();
        self.inbox.pop_front()
    }

    fn len(&self) -> usize {
        self.inbox.len()
    }

    fn is_empty(&self) -> bool {
        self.inbox.is_empty()
    }

    fn is_connected(&self) -> bool {
        self.connected
    }

    fn disconnect(&mut self) {
        self.connected = false;
        self.peer = None;
        self.pending = None;
        self.inbox.clear();
        let endpoint = self.endpoint.clone();
        self.runtime.block_on(async { endpoint.close().await });
    }

    fn dropped_count(&self) -> usize {
        self.dropped
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::transport::ClassroomMessageKind;
    use std::net::TcpListener;
    use std::sync::mpsc;

    fn free_port() -> u16 {
        TcpListener::bind("127.0.0.1:0")
            .expect("puerto")
            .local_addr()
            .expect("addr")
            .port()
    }

    fn bind_loopback(port: u16, rt: &tokio::runtime::Runtime) -> (iroh::Endpoint, iroh::PublicKey) {
        let secret = iroh::SecretKey::generate();
        let id = secret.public();
        let addr: SocketAddr = format!("127.0.0.1:{port}").parse().expect("addr");
        // Builder vacío + preset Minimal (solo crypto, sin lookup/relay).
        let endpoint = rt
            .block_on(
                iroh::endpoint::Builder::empty()
                    .preset(iroh::endpoint::presets::Minimal)
                    .alpns(vec![AULA_ALPN.to_vec()])
                    .secret_key(secret)
                    .bind_addr(addr)
                    .expect("bind addr")
                    .bind(),
            )
            .expect("bind loopback");
        (endpoint, id)
    }

    fn loopback_addr(host_id: &str, host_port: u16) -> iroh::EndpointAddr {
        use std::str::FromStr;
        let peer = iroh::PublicKey::from_str(host_id).expect("id");
        let host_sock: SocketAddr = format!("127.0.0.1:{host_port}").parse().expect("addr");
        iroh::EndpointAddr::new(peer).with_ip_addr(host_sock)
    }

    #[test]
    fn ticket_encode_decode_roundtrip() {
        let ticket = AulaTicket {
            id: "ab12".to_string(),
            addrs: vec!["127.0.0.1:1234".to_string()],
        };
        // `encode` es `Result` honesto: nunca un ticket vacío silencioso (A8).
        let text = ticket.encode().expect("encode");
        assert!(!text.is_empty());
        assert_eq!(AulaTicket::decode(&text), Some(ticket));
        assert!(AulaTicket::decode("no-json").is_none());
        assert!(AulaTicket::decode(r#"{"id":""}"#).is_none());
        let many = AulaTicket {
            id: "x".to_string(),
            addrs: (0..17).map(|i| format!("127.0.0.1:{i}")).collect(),
        };
        assert!(AulaTicket::decode(&many.encode().expect("encode")).is_none());
    }

    /// Regresión A2.2: la cola llena tenía DOS semánticas contradictorias para
    /// el mismo contrato (`LoopbackTransport` devolvía `QueueFull` honesto y el
    /// `pump()` entrante hacía `pop_front()` — descartaba el más viejo sin
    /// aviso). Política única: rechazo del entrante nuevo + contador de
    /// descartes, nunca evicción silenciosa.
    #[test]
    fn full_inbox_rejects_and_counts_never_evicts() {
        let runtime = IrohTransport::runtime().expect("runtime");
        let secret = iroh::SecretKey::generate();
        let id = secret.public();
        let endpoint = runtime
            .block_on(
                iroh::endpoint::Builder::empty()
                    .preset(iroh::endpoint::presets::Minimal)
                    .alpns(vec![AULA_ALPN.to_vec()])
                    .secret_key(secret)
                    .bind(),
            )
            .expect("bind");
        let mut transport = IrohTransport::from_parts(runtime, endpoint, id, None);
        let overflow = ClassroomMessage::try_new("Ana", ClassroomMessageKind::Chat, "no entra")
            .expect("fixture");
        for index in 0..MAX_TRANSPORT_QUEUE {
            let msg =
                ClassroomMessage::try_new("Ana", ClassroomMessageKind::Chat, &format!("m{index}"))
                    .expect("fixture");
            assert!(transport.push_inbox(msg), "entra hasta el tope");
        }
        assert!(!transport.push_inbox(overflow), "llena: rechaza y cuenta");
        assert_eq!(transport.dropped_count(), 1);
        assert_eq!(transport.len(), MAX_TRANSPORT_QUEUE);
        // El más viejo sigue ahí (sin evicción silenciosa).
        let oldest = transport.poll().expect("drena uno");
        assert_eq!(oldest.body, "m0");
    }

    #[test]
    fn admission_token_is_deterministic_per_code() {
        let token = derive_admission_token("AULA-123").expect("token");
        assert_eq!(token, derive_admission_token("AULA-123").expect("token"));
        assert_ne!(token, derive_admission_token("AULA-124").expect("otro"));
        assert_ne!(token, [0_u8; ADMISSION_TOKEN_LEN]);
        assert_eq!(
            token_from_hex(&token_to_hex(&token)),
            Some(token),
            "hex roundtrip"
        );
        assert!(token_from_hex("zz").is_none());
    }

    #[test]
    fn handshake_frame_roundtrip_and_limits() {
        let token = derive_admission_token("AULA-123").expect("token");
        let request = HandshakeFrame::request(&token);
        let raw = request.encode().expect("encode");
        let back = HandshakeFrame::decode(&raw).expect("decode");
        assert_eq!(back, request);
        assert_eq!(back.token(), Some(token));
        let ack = HandshakeFrame::ack(true);
        assert_eq!(ack.ok, Some(true));
        assert!(HandshakeFrame::decode(b"{\"handshake\":9}").is_err());
        assert!(HandshakeFrame::decode(&vec![b'x'; 257]).is_err());
    }

    #[test]
    fn disconnected_transport_fails_like_loopback() {
        let runtime = IrohTransport::runtime().expect("runtime");
        let secret = iroh::SecretKey::generate();
        let endpoint = runtime
            .block_on(
                iroh::endpoint::Builder::empty()
                    .preset(iroh::endpoint::presets::Minimal)
                    .alpns(vec![AULA_ALPN.to_vec()])
                    .secret_key(secret)
                    .bind(),
            )
            .expect("bind");
        let mut transport = IrohTransport {
            runtime,
            endpoint,
            own_id: iroh::SecretKey::generate().public(),
            admission: None,
            pending: None,
            peer: None,
            inbox: VecDeque::new(),
            dropped: 0,
            rejected: 0,
            connected: false,
        };
        let msg =
            ClassroomMessage::try_new("Ana", ClassroomMessageKind::Chat, "hola").expect("fixture");
        assert!(transport.send(msg).is_err());
        assert!(transport.poll().is_none());
        assert!(transport.is_empty());
        transport.disconnect();
        assert_eq!(transport.len(), 0);
        assert_eq!(transport.dropped_count(), 0);
    }

    /// Ping/pong real por loopback 127.0.0.1 sin relay (requiere UDP local),
    /// con la admisión encendida: el invitado se presenta con el código.
    ///
    /// Host y guest corren en hilos distintos (como en producción): el dial
    /// QUIC necesita ambos runtimes vivos a la vez; secuenciarlos en un solo
    /// hilo hace timeout siempre, por diseño del transporte, no por bug.
    #[test]
    fn loopback_ping_pong_over_iroh() {
        let code = ClassroomCode::try_new("AULA-123").expect("código");
        let token = derive_admission_token(code.as_str()).expect("token");
        let (ticket_tx, ticket_rx) = mpsc::channel();
        let (result_tx, result_rx) = mpsc::channel();
        let host_thread = std::thread::spawn(move || {
            let rt = IrohTransport::runtime().expect("runtime host");
            let port = free_port();
            let (endpoint, id) = bind_loopback(port, &rt);
            // El puerto viaja en el ticket en producción; acá por el canal.
            ticket_tx.send((id.to_string(), port)).expect("ticket");
            let mut host = IrohTransport::from_parts(rt, endpoint, id, Some(token));
            let mut got = None;
            for _ in 0..400 {
                if let Some(message) = host.poll() {
                    got = Some(message);
                    break;
                }
            }
            result_tx
                .send((got, host.rejected_peers()))
                .expect("resultado");
        });
        let (host_id, host_port) = ticket_rx
            .recv_timeout(Duration::from_secs(30))
            .expect("ticket");
        // Invitado en este hilo con runtime propio y el código de la sala.
        let rt = IrohTransport::runtime().expect("runtime guest");
        let (guest_endpoint, guest_id) = bind_loopback(free_port(), &rt);
        let addr = loopback_addr(&host_id, host_port);
        let mut guest =
            IrohTransport::join_with_endpoint(rt, guest_endpoint, guest_id, addr, &code)
                .expect("admisión con el código correcto");
        assert!(guest.has_peer());
        let msg = ClassroomMessage::try_new("Ana", ClassroomMessageKind::Chat, "hola aula")
            .expect("fixture");
        guest.send(msg).expect("send");
        // Bombea el runtime guest tras enviar (las tareas QUIC de fondo
        // solo avanzan mientras su runtime corre; el host ya bombea en su
        // hilo vía poll()).
        for _ in 0..20 {
            guest.poll();
        }
        host_thread.join().expect("host thread");
        let (got, rejected) = result_rx
            .recv_timeout(Duration::from_secs(30))
            .expect("resultado");
        let got = got.expect("el host recibe el ping");
        assert_eq!(got.from, "Ana");
        assert_eq!(got.body, "hola aula");
        assert_eq!(rejected, 0, "ningún intento rechazado en el camino feliz");
        guest.disconnect();
    }

    /// Regresión A1: sin verificación de identidad/código, `pump` aceptaba al
    /// PRIMER par que conectaba ("first-peer-wins"): quien obtuviera el
    /// `EndpointId` — o simplemente conectara primero, porque el ticket QR se
    /// comparte en un aula real — se convertía en el par y el usuario
    /// legítimo quedaba excluido (es 1:1 en v1). Ahora el handshake exige el
    /// token del código de sala: el par sin código se rechaza y se sigue
    /// aceptando al siguiente.
    ///
    /// También fija A2.3: cada `poll()` del anfitrión queda acotado a una
    /// ventana corta aunque el handshake del par entrante esté a medias
    /// (antes el handshake podía bloquear el bombeo hasta 10 s).
    #[test]
    fn admission_rejects_wrong_code_and_accepts_the_next_peer() {
        let code_ok = ClassroomCode::try_new("AULA-111").expect("código");
        let code_bad = ClassroomCode::try_new("AULA-222").expect("otro código");
        let token = derive_admission_token(code_ok.as_str()).expect("token");
        let (ticket_tx, ticket_rx) = mpsc::channel();
        let (result_tx, result_rx) = mpsc::channel();
        let host_thread = std::thread::spawn(move || {
            let rt = IrohTransport::runtime().expect("runtime host");
            let port = free_port();
            let (endpoint, id) = bind_loopback(port, &rt);
            ticket_tx.send((id.to_string(), port)).expect("ticket");
            let mut host = IrohTransport::from_parts(rt, endpoint, id, Some(token));
            let mut got = None;
            let mut worst_pump = Duration::ZERO;
            for _ in 0..800 {
                let started = Instant::now();
                let polled = host.poll();
                worst_pump = worst_pump.max(started.elapsed());
                if let Some(message) = polled {
                    got = Some(message);
                    break;
                }
            }
            result_tx
                .send((got, host.rejected_peers(), host.has_peer(), worst_pump))
                .expect("resultado");
        });
        let (host_id, host_port) = ticket_rx
            .recv_timeout(Duration::from_secs(30))
            .expect("ticket");
        let addr = loopback_addr(&host_id, host_port);
        // Par hostil: conecta PRIMERO con un código que no es el de la sala.
        let rt_bad = IrohTransport::runtime().expect("runtime hostil");
        let (bad_endpoint, bad_id) = bind_loopback(free_port(), &rt_bad);
        let joined = IrohTransport::join_with_endpoint(
            rt_bad,
            bad_endpoint,
            bad_id,
            addr.clone(),
            &code_bad,
        );
        assert!(
            joined.is_err(),
            "par sin el código correcto no es aceptado (A1)"
        );
        // Par legítimo: tras el rechazo, entra igual (conectar-al-siguiente).
        let rt = IrohTransport::runtime().expect("runtime guest");
        let (guest_endpoint, guest_id) = bind_loopback(free_port(), &rt);
        let mut guest =
            IrohTransport::join_with_endpoint(rt, guest_endpoint, guest_id, addr, &code_ok)
                .expect("el código correcto entra");
        let msg = ClassroomMessage::try_new("Ana", ClassroomMessageKind::Chat, "hola aula")
            .expect("fixture");
        guest.send(msg).expect("send");
        for _ in 0..20 {
            guest.poll();
        }
        host_thread.join().expect("host thread");
        let (got, rejected, has_peer, worst_pump) = result_rx
            .recv_timeout(Duration::from_secs(90))
            .expect("resultado");
        let got = got.expect("el legítimo llega después del intento hostil");
        assert_eq!(got.body, "hola aula");
        assert!(has_peer);
        assert!(
            rejected >= 1,
            "el intento con código incorrecto quedó contado: {rejected}"
        );
        assert!(
            worst_pump < Duration::from_secs(2),
            "un poll() jamás bloquea segundos con un handshake a medias (A2.3): {worst_pump:?}"
        );
        guest.disconnect();
    }
}
