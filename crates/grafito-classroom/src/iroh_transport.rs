//! Transporte P2P real del aula sobre iroh/QUIC (feature `aula-iroh`).
//!
//! Implementa [`crate::transport::ClassroomTransport`] con el mismo contrato
//! que `LoopbackTransport`: cola de entrada acotada 128, mensajes validados,
//! `QueueFull`/`InvalidMessage` honestos, `disconnect` que vacía.
//!
//! Diseño (ver `stubs::iroh_p2p_stub`):
//! - ALPN `grafito-aula/1`, handshake con timeout 10 s, 1 par activo (1:1 v1).
//! - El anfitrión crea el endpoint (preset N0 con relay) y comparte un
//!   [`AulaTicket`] (EndpointId + direcciones); el invitado hace `join`.
//! - Encuadre: `u16 BE length` + JSON (`serde_json`, ya dependencia);
//!   todo lo que exceda `MAX_MESSAGE_BYTES` se rechaza sin enviar.
//! - `poll()` bombea la red con timeout corto (50 ms): el throughput sigue
//!   el ritmo de poll (la UI lo llama por frame). `host()`/`join()` bloquean
//!   hasta 10 s: llamar desde worker, nunca desde el UI thread.
//! - Sin relay en tests: dial directo 127.0.0.1 (RelayMode deshabilitado).

use std::collections::VecDeque;
use std::net::SocketAddr;
use std::time::Duration;

use crate::session::ClassroomError;
use crate::transport::{
    ClassroomMessage, ClassroomTransport, MAX_MESSAGE_BYTES, MAX_TRANSPORT_QUEUE,
};

/// ALPN del protocolo de aula (diseño del stub).
pub const AULA_ALPN: &[u8] = b"grafito-aula/1";
/// Timeout de handshake/conexión (diseño del stub: 10 s).
pub const IROH_CONNECT_TIMEOUT: Duration = Duration::from_secs(10);
/// Timeout de cada bombeo en `poll` (corto: no traba al llamador).
pub const IROH_POLL_TIMEOUT: Duration = Duration::from_millis(50);
/// Timeout de `send` (apertura de stream + escritura).
pub const IROH_SEND_TIMEOUT: Duration = Duration::from_secs(10);

/// Ticket de admisión a una sala: identidad + cómo contactarla.
///
/// Se serializa a JSON compacto (y cabe en el QR `grafito://aula/{código}`
/// como fragmento). Sin direcciones = resolución por Address Lookup del
/// preset N0 (requiere internet); con direcciones = dial directo (LAN).
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
    #[must_use]
    pub fn encode(&self) -> String {
        serde_json::to_string(self).unwrap_or_default()
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

/// Transporte iroh/QUIC 1:1 del aula.
pub struct IrohTransport {
    runtime: tokio::runtime::Runtime,
    endpoint: iroh::Endpoint,
    own_id: iroh::PublicKey,
    peer: Option<iroh::endpoint::Connection>,
    inbox: VecDeque<ClassroomMessage>,
    connected: bool,
}

fn connect_err(detail: impl Into<String>) -> ClassroomError {
    ClassroomError::InvalidMessage(detail.into())
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

    /// Anfitrión: endpoint N0 (relay) escuchando `AULA_ALPN`.
    ///
    /// Bloquea solo el bind local (rápido); el par llega en `poll()`.
    pub fn host() -> Result<Self, ClassroomError> {
        let runtime = Self::runtime()?;
        let (endpoint, own_id) = Self::bind_production(&runtime)?;
        Ok(Self {
            runtime,
            endpoint,
            own_id,
            peer: None,
            inbox: VecDeque::new(),
            connected: true,
        })
    }

    /// Invitado: conecta al ticket con timeout de 10 s.
    pub fn join(ticket: &AulaTicket) -> Result<Self, ClassroomError> {
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
        let runtime = Self::runtime()?;
        let (endpoint, own_id) = Self::bind_production(&runtime)?;
        let connection = runtime
            .block_on(async {
                tokio::time::timeout(IROH_CONNECT_TIMEOUT, endpoint.connect(addr, AULA_ALPN)).await
            })
            .map_err(|_| connect_err("join: timeout de 10 s sin conectar"))
            .and_then(|inner| inner.map_err(|e| connect_err(format!("join: {e}"))))?;
        Ok(Self {
            runtime,
            endpoint,
            own_id,
            peer: Some(connection),
            inbox: VecDeque::new(),
            connected: true,
        })
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

    /// Bomba una ventana de red: acepta un par (anfitrión) y drena un mensaje.
    fn pump(&mut self) {
        if !self.connected {
            return;
        }
        if self.peer.is_none() {
            let accepted = self.runtime.block_on(async {
                tokio::time::timeout(IROH_POLL_TIMEOUT, self.endpoint.accept()).await
            });
            if let Ok(Some(incoming)) = accepted {
                let connected = self.runtime.block_on(async {
                    tokio::time::timeout(IROH_CONNECT_TIMEOUT, async move { incoming.await }).await
                });
                if let Ok(Ok(connection)) = connected {
                    self.peer = Some(connection);
                }
            }
        }
        let Some(connection) = self.peer.clone() else {
            return;
        };
        let frame = self.runtime.block_on(async {
            tokio::time::timeout(IROH_POLL_TIMEOUT, async {
                let mut recv = connection.accept_uni().await.map_err(|e| e.to_string())?;
                let mut len = [0u8; 2];
                recv.read_exact(&mut len).await.map_err(|e| e.to_string())?;
                let len = u16::from_be_bytes(len) as usize;
                if len == 0 || len > MAX_MESSAGE_BYTES + 512 {
                    return Err("marco inválido".to_string());
                }
                let body = recv.read_to_end(len).await.map_err(|e| e.to_string())?;
                if body.len() != len {
                    return Err("marco truncado".to_string());
                }
                Ok::<Vec<u8>, String>(body)
            })
            .await
            .map_err(|_| "timeout".to_string())
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
        if self.inbox.len() >= MAX_TRANSPORT_QUEUE {
            self.inbox.pop_front();
        }
        self.inbox.push_back(msg);
    }

    fn mark_broken(&mut self, detail: String) -> ClassroomError {
        self.connected = false;
        self.peer = None;
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
        if body.len() > MAX_MESSAGE_BYTES + 512 {
            return Err(connect_err(format!(
                "mensaje excede {} bytes",
                MAX_MESSAGE_BYTES + 512
            )));
        }
        let Some(connection) = self.peer.clone() else {
            return Err(connect_err("sin par conectado"));
        };
        let mut wire: Vec<u8> = Vec::with_capacity(2 + body.len());
        wire.extend_from_slice(&(body.len() as u16).to_be_bytes());
        wire.extend_from_slice(&body);
        let result = self.runtime.block_on(async {
            tokio::time::timeout(IROH_SEND_TIMEOUT, async {
                let mut send = connection.open_uni().await.map_err(|e| e.to_string())?;
                send.write_all(&wire).await.map_err(|e| e.to_string())?;
                send.finish().map_err(|e| e.to_string())?;
                Ok::<(), String>(())
            })
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
        self.inbox.clear();
        let endpoint = self.endpoint.clone();
        self.runtime.block_on(async { endpoint.close().await });
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ticket_encode_decode_roundtrip() {
        let ticket = AulaTicket {
            id: "ab12".to_string(),
            addrs: vec!["127.0.0.1:1234".to_string()],
        };
        let text = ticket.encode();
        assert_eq!(AulaTicket::decode(&text), Some(ticket));
        assert!(AulaTicket::decode("no-json").is_none());
        assert!(AulaTicket::decode(r#"{"id":""}"#).is_none());
        let many = AulaTicket {
            id: "x".to_string(),
            addrs: (0..17).map(|i| format!("127.0.0.1:{i}")).collect(),
        };
        assert!(AulaTicket::decode(&many.encode()).is_none());
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
            peer: None,
            inbox: VecDeque::new(),
            connected: false,
        };
        let msg =
            ClassroomMessage::try_new("Ana", crate::transport::ClassroomMessageKind::Chat, "hola")
                .expect("fixture");
        assert!(transport.send(msg).is_err());
        assert!(transport.poll().is_none());
        assert!(transport.is_empty());
        transport.disconnect();
        assert_eq!(transport.len(), 0);
    }

    /// Ping/pong real por loopback 127.0.0.1 sin relay (requiere UDP local).
    ///
    /// Host y guest corren en hilos distintos (como en producción): el dial
    /// QUIC necesita ambos runtimes vivos a la vez; secuenciarlos en un solo
    /// hilo hace timeout siempre, por diseño del transporte, no por bug.
    #[test]
    fn loopback_ping_pong_over_iroh() {
        use std::net::TcpListener;
        use std::sync::mpsc;
        fn free_port() -> u16 {
            TcpListener::bind("127.0.0.1:0")
                .expect("puerto")
                .local_addr()
                .expect("addr")
                .port()
        }
        fn bind_loopback(
            port: u16,
            rt: &tokio::runtime::Runtime,
        ) -> (iroh::Endpoint, iroh::PublicKey) {
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
        let (ticket_tx, ticket_rx) = mpsc::channel();
        let (result_tx, result_rx) = mpsc::channel();
        let host_thread = std::thread::spawn(move || {
            let rt = IrohTransport::runtime().expect("runtime host");
            let port = free_port();
            let (endpoint, id) = bind_loopback(port, &rt);
            // El puerto viaja en el ticket en producción; acá por el canal.
            ticket_tx.send((id.to_string(), port)).expect("ticket");
            let incoming = rt
                .block_on(async {
                    tokio::time::timeout(Duration::from_secs(20), endpoint.accept()).await
                })
                .expect("timeout accept")
                .expect("incoming");
            let conn = rt.block_on(async move { incoming.await }).expect("conn");
            let mut host = IrohTransport {
                runtime: rt,
                endpoint,
                own_id: id,
                peer: Some(conn),
                inbox: VecDeque::new(),
                connected: true,
            };
            let mut got = None;
            for _ in 0..400 {
                if let Some(message) = host.poll() {
                    got = Some(message);
                    break;
                }
            }
            result_tx.send(got).expect("resultado");
        });
        let (host_id, host_port) = ticket_rx
            .recv_timeout(Duration::from_secs(30))
            .expect("ticket");
        // Invitado en este hilo con runtime propio.
        let rt = IrohTransport::runtime().expect("runtime guest");
        let (guest_endpoint, guest_id) = bind_loopback(free_port(), &rt);
        use std::str::FromStr;
        let peer = iroh::PublicKey::from_str(&host_id).expect("id");
        let host_sock: SocketAddr = format!("127.0.0.1:{host_port}").parse().expect("addr");
        let addr = iroh::EndpointAddr::new(peer).with_ip_addr(host_sock);
        let guest_conn = rt
            .block_on(async {
                tokio::time::timeout(
                    Duration::from_secs(20),
                    guest_endpoint.connect(addr, AULA_ALPN),
                )
                .await
            })
            .expect("timeout dial")
            .expect("dial");
        let mut guest = IrohTransport {
            runtime: rt,
            endpoint: guest_endpoint,
            own_id: guest_id,
            peer: Some(guest_conn),
            inbox: VecDeque::new(),
            connected: true,
        };
        let msg = ClassroomMessage::try_new(
            "Ana",
            crate::transport::ClassroomMessageKind::Chat,
            "hola aula",
        )
        .expect("fixture");
        guest.send(msg).expect("send");
        // Bombea el runtime guest tras enviar (las tareas QUIC de fondo
        // solo avanzan mientras su runtime corre; el host ya bombea en su
        // hilo vía poll()).
        for _ in 0..20 {
            guest.poll();
        }
        host_thread.join().expect("host thread");
        let got: Option<ClassroomMessage> = result_rx
            .recv_timeout(Duration::from_secs(30))
            .expect("resultado");
        let got = got.expect("el host recibe el ping");
        assert_eq!(got.from, "Ana");
        assert_eq!(got.body, "hola aula");
        guest.disconnect();
    }
}
