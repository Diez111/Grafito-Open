//! Stubs L honestos del aula/P2P (diseño + `Err` explicativo + test).
//!
//! Alcance F10: `iroh P2P`, `CRDT UUID+LWW completo`, `sesiones cifradas` y
//! `offline-first cola` son L — aquí solo vive su diseño documentado y un
//! stub que siempre falla honesto. El frente útil (S/M) es
//! [`crate::session`] + [`crate::transport::LoopbackTransport`], que ya
//! compilan y pasan tests sin red.
//!
//! - PII siempre local: ningún stub toca red ni disco; todos retornan `Err`
//!   sin efectos.
//! - Presupuestos: los hints citan los topes que el diseño real debería
//!   respetar (128 mensajes, 2048 bytes/mensaje, roster 5000).

use crate::session::ClassroomError;

/// Diseño iroh P2P (L): transporte QUIC con `iroh` (`Endpoint` + `ALPN`
/// `grafito-aula/1`), descubrimiento local (mDNS) + relay público opt-in,
/// códigos `AULA-XXXXXX` como ticket de admisión. Requiere dep `iroh`
/// (fuera del frente), handshake con timeout 10s y backpressure 128.
/// Hoy: Loopback en [`crate::transport::LoopbackTransport`].
pub fn iroh_p2p_stub() -> Result<String, ClassroomError> {
    Err(ClassroomError::NotImplemented {
        feature: "IrohP2P",
        hint: "diseño F10.W5: endpoint iroh QUIC + ALPN grafito-aula/1 + mDNS opt-in (cola 128, timeout 10s); hoy Loopback local sin red"
            .to_string(),
    })
}

/// Diseño CRDT pizarra `UUID+LWW` completo (L): cada trazo/objeto con `Uuid`
/// v4 + `HybridLogicalClock` + `Last-Writer-Wins` por campo, fusión
/// conmutativa/idempotente y GC de tombstones con cota 5000. Requiere deps
/// `uuid` + reloj híbrido (fuera del frente). Hoy: sin fusión (un solo
/// documento local).
pub fn crdt_merge_stub() -> Result<String, ClassroomError> {
    Err(ClassroomError::NotImplemented {
        feature: "Crdt",
        hint: "diseño F10.W5: UUID v4 + HLC + LWW por campo con fusión conmutativa y tombstones acotados (5000); hoy documento local único"
            .to_string(),
    })
}

/// Diseño sesiones cifradas (M-L): `X25519` + `ChaCha20-Poly1305` por sala,
/// clave efímera derivada del código (`HKDF`) y rotación por sesión
/// (`Idle→Lobby` genera clave, `close` la borra con `zeroize`). Requiere
/// `chacha20poly1305`+`x25519-dalek` (fuera del frente). Hoy: Loopback sin
/// red, nada que cifrar (PII nunca sale del proceso).
pub fn encrypted_session_stub() -> Result<String, ClassroomError> {
    Err(ClassroomError::NotImplemented {
        feature: "EncryptedSession",
        hint: "diseño F10.W5: X25519 + ChaCha20-Poly1305 por sala con HKDF del código y zeroize al cerrar; hoy Loopback local sin red"
            .to_string(),
    })
}

/// Diseño offline-first cola (L): outbox persistente acotada (128 mensajes,
/// 2048 bytes/mensaje) con reintento exponencial y descarte honesto
/// (`QueueFull`) al reconectar. Requiere persistencia local + transporte
/// (fuera del frente). Hoy: [`crate::transport::LoopbackTransport`] volátil
/// en memoria (se vacía al desconectar).
pub fn offline_queue_stub() -> Result<String, ClassroomError> {
    Err(ClassroomError::NotImplemented {
        feature: "OfflineQueue",
        hint: "diseño F10.W5: outbox persistente 128x2048 con backoff y descarte honesto; hoy Loopback volátil en memoria"
            .to_string(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn l_stubs_always_fail_honestly() {
        for (result, feature) in [
            (iroh_p2p_stub(), "IrohP2P"),
            (crdt_merge_stub(), "Crdt"),
            (encrypted_session_stub(), "EncryptedSession"),
            (offline_queue_stub(), "OfflineQueue"),
        ] {
            let err = result.expect_err("L siempre falla honesto");
            let text = err.to_string();
            assert!(text.contains(feature), "debe nombrar {feature}: {text}");
            assert!(text.contains("diseño"), "debe explicar el diseño: {text}");
        }
    }

    #[test]
    fn l_stub_errors_are_not_implemented_variant() {
        let err = iroh_p2p_stub().expect_err("stub");
        assert!(matches!(
            err,
            ClassroomError::NotImplemented {
                feature: "IrohP2P",
                ..
            }
        ));
    }
}
