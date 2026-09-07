//! Stubs L honestos del aula/P2P (diseño + `Err` explicativo + test).
//!
//! Alcance F10 G-G: `iroh P2P`, `CRDT completo con uuid/HLC`, `sesiones
//! cifradas` y `outbox persistente` son L — aquí solo vive su diseño y un
//! stub que siempre falla honesto. El frente útil (S/M) ya es funcional:
//! [`crate::session`] (expiración + CSV) + [`crate::transport::LoopbackTransport`]
//! (reintento acotado) + [`crate::crdt::WhiteboardCrdt`] (CRDT mínimo en
//! memoria) + [`crate::offline::OfflineOutbox`] (cola volátil acotada).
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
/// v4 (`uuid` crate) + `HybridLogicalClock` real + `Last-Writer-Wins` por campo,
/// fusión conmutativa/idempotente y GC de tombstones con cota 5000. Requiere
/// deps `uuid` + reloj híbrido (fuera del frente). Hoy: mínimo funcional en
/// [`crate::crdt::WhiteboardCrdt`] (IDs 128 bits std-only + HLC mínimo + LWW
/// en memoria, sin red).
pub fn crdt_merge_stub() -> Result<String, ClassroomError> {
    Err(ClassroomError::NotImplemented {
        feature: "Crdt",
        hint: "diseño F10.W5: UUID v4 + HLC + LWW por campo con fusión conmutativa y tombstones acotados (5000); hoy WhiteboardCrdt mínimo en memoria (ver crdt.rs)"
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

/// Diseño offline-first cola persistente (L): outbox en disco acotada
/// (128 mensajes, 2048 bytes/mensaje) con reintento exponencial y descarte
/// honesto (`QueueFull`) al reconectar. Requiere persistencia local +
/// transporte (fuera del frente). Hoy: [`crate::offline::OfflineOutbox`]
/// volátil en memoria (misma cota, backoff `2^attempts`, descarte tras 5).
pub fn offline_queue_stub() -> Result<String, ClassroomError> {
    Err(ClassroomError::NotImplemented {
        feature: "OfflineQueue",
        hint: "diseño F10.W5: outbox persistente 128x2048 con backoff y descarte honesto; hoy OfflineOutbox volátil en memoria (ver offline.rs)"
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
