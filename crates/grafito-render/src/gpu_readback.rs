//! Background GPU readback (frente B6).
//!
//! El `map_async` + espera de los pipelines `*_compute` bloqueaba el hilo UI
//! (~16 ms por frame según auditoría). Este módulo distribuye la espera en
//! frames: el dispatch retorna inmediatamente y el resolve ocurre en un frame
//! posterior, cuando el buffer ya está mapeado en RAM.
//!
//! División honesta del trabajo:
//! - El **dispatch** (`write_buffer` + `submit` + `map_async`) sigue en el hilo
//!   del frame: es barato (µs, no espera a la GPU) y `wgpu` lo permite.
//! - La **espera** ya no es un bucle bloqueante: cada frame hace un único
//!   `device.poll(Maintain::Poll)` no-bloqueante (µs) que dispara los
//!   callbacks `map_async` pendientes, y el frame consulta el flag sin
//!   esperar. La GPU trabaja en paralelo mientras la UI sigue pintando.
//! - El **resolve** (`get_mapped_range` + copia + `unmap`) ocurre solo cuando
//!   el poll ya reportó `Mapped`: es un `memcpy` de RAM, sin espera GPU.
//!
//! Por qué sin waiter thread (decisión verificada por compilador): en
//! wgpu 22.1.0 `Device` no implementa `Clone`, así que un thread background
//! no puede poseer el device para el `poll`. Pasar `&Device` exigiría
//! `thread::scope` (bloquea al llamante hasta el `join`) o un device
//! duplicado (doble VRAM/init). El poll-por-frame es el patrón correcto aquí;
//! el patrón `thread::spawn` + `sync_channel(1)` del repo
//! (`assistant.rs:1922-1923`, `app.rs:372`) se aplica donde sí encaja —datos
//! `owned`/`Send`, como el marching-squares del resolve implícito—.
//!
//! Presupuestos (ver `docs/architecture.md:8`):
//! - [`GPU_READBACK_TIMEOUT`]: 250 ms, mismo origen único que el path
//!   síncrono legacy (`SYNC_GPU_READBACK_TIMEOUT` en `lib.rs`).
//! - [`MAX_GPU_READBACK_JOBS_IN_FLIGHT`]: 1. Si llega otro job, se descarta el
//!   viejo por generación: nunca hay cola infinita.
//!
//! Cancelación honesta: cancelar = descartar por generación/key + `unmap` del
//! buffer implicado (idempotente, no-op si el map sigue pendiente).
//!
//! Sin flicker negro: mientras el job está en vuelo el frame pinta el
//! último-frame-válido (fallback CPU + `completed_key` viejo en `canvas.rs`);
//! el readiness queda `Pending` = "calculando…", observable vía
//! `GpuComputeSlot::is_pending` para futuro wiring UI. Sin spinner infinito:
//! el timeout convierte `Pending` en fallback CPU, garantizado.

use std::sync::{
    atomic::{AtomicBool, Ordering},
    Arc,
};
use std::time::{Duration, Instant};

/// Timeout de un readback GPU antes de caer al fallback CPU (honesto, acotado).
/// Origen único del presupuesto 250 ms compartido con el path síncrono legacy.
pub const GPU_READBACK_TIMEOUT: Duration = Duration::from_millis(250);

/// Cap de jobs de readback en vuelo: 1. Un segundo dispatch descarta el viejo
/// por generación en vez de encolar (nunca cola infinita).
pub const MAX_GPU_READBACK_JOBS_IN_FLIGHT: usize = 1;

/// Resultado non-blocking de [`PendingGpuReadback::poll`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ReadbackPoll {
    /// La GPU aún no terminó; el hilo del frame sigue libre.
    Pending,
    /// El buffer ya está mapeado: el resolve (`get_mapped_range` + copia) es
    /// inmediato y no bloquea.
    Mapped,
    /// Fallo o timeout: fallback CPU honesto + `unmap`.
    Failed,
}

/// Espera de un `map_async` distribuida en frames.
///
/// Se crea con [`PendingGpuReadback::submit`] justo después del `map_async`
/// (guarda el `Arc<AtomicBool>` que el callback marca en éxito) y se consulta
/// con [`PendingGpuReadback::poll`] (non-blocking) en frames posteriores,
/// después del `device.poll(Maintain::Poll)` no-bloqueante del frame.
/// El hilo del frame nunca espera a la GPU.
#[derive(Debug)]
pub struct PendingGpuReadback {
    map_ok: Arc<AtomicBool>,
    started_at: Instant,
    timeout: Duration,
    /// `try_recv` no aplica aquí (el flag es consultable sin consumo), pero el
    /// resultado terminal se cachea igual para estabilidad del slot.
    settled: Option<ReadbackPoll>,
}

impl PendingGpuReadback {
    /// Registra la espera y retorna **inmediatamente** (no toca la GPU).
    /// `map_ok` es el mismo `Arc<AtomicBool>` que el callback de `map_async`
    /// marca en éxito.
    pub fn submit(map_ok: &Arc<AtomicBool>) -> Self {
        Self::submit_with_timeout(map_ok, GPU_READBACK_TIMEOUT)
    }

    /// Variante con timeout explícito (tests).
    pub fn submit_with_timeout(map_ok: &Arc<AtomicBool>, timeout: Duration) -> Self {
        Self {
            map_ok: Arc::clone(map_ok),
            started_at: Instant::now(),
            timeout,
            settled: None,
        }
    }

    /// Poll non-blocking: nunca bloquea al llamante. Flag marcado → `Mapped`;
    /// deadline superada → `Failed` (timeout honesto); si no, `Pending`.
    pub fn poll(&mut self) -> ReadbackPoll {
        if let Some(settled) = self.settled {
            return settled;
        }
        let result = if self.map_ok.load(Ordering::SeqCst) {
            ReadbackPoll::Mapped
        } else if self.started_at.elapsed() >= self.timeout {
            ReadbackPoll::Failed
        } else {
            ReadbackPoll::Pending
        };
        if result != ReadbackPoll::Pending {
            self.settled = Some(result);
        }
        result
    }

    /// Tiempo desde el `submit`; el UI lo usa para el umbral "calculando…".
    pub fn elapsed(&self) -> Duration {
        self.started_at.elapsed()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// El submit + poll inicial retornan en µs aunque la GPU simulada tarde:
    /// el hilo del frame nunca espera (test de no-bloqueo con mock).
    #[test]
    fn submit_and_first_poll_never_block_while_gpu_is_slow() {
        // Flag jamás marcado = "GPU lenta que no terminó".
        let map_ok = Arc::new(AtomicBool::new(false));
        let mut pending =
            PendingGpuReadback::submit_with_timeout(&map_ok, Duration::from_millis(200));
        let start = Instant::now();
        assert_eq!(pending.poll(), ReadbackPoll::Pending);
        assert!(
            start.elapsed() < Duration::from_millis(50),
            "poll must not block the frame thread"
        );
    }

    /// Cuando el callback marca el flag, el poll lo refleja sin espera.
    #[test]
    fn mapped_flag_is_visible_without_blocking() {
        let map_ok = Arc::new(AtomicBool::new(false));
        let mut pending =
            PendingGpuReadback::submit_with_timeout(&map_ok, Duration::from_millis(250));
        map_ok.store(true, Ordering::SeqCst);
        assert_eq!(pending.poll(), ReadbackPoll::Mapped);
        // El resultado terminal persiste: repreguntar no lo pierde.
        assert_eq!(pending.poll(), ReadbackPoll::Mapped);
    }

    /// Timeout honesto: aunque la GPU nunca responda, pasado el deadline el
    /// poll reporta `Failed` (fallback CPU) en vez de `Pending` eterno. Sin
    /// spinner infinito por diseño.
    #[test]
    fn elapsed_deadline_reports_failed_without_callback() {
        let map_ok = Arc::new(AtomicBool::new(false));
        let mut pending = PendingGpuReadback::submit_with_timeout(&map_ok, Duration::ZERO);
        // `Duration::ZERO`: la deadline ya pasó en el primer poll.
        assert_eq!(pending.poll(), ReadbackPoll::Failed);
    }

    /// El cap es 1 por diseño: el slot (`canvas.rs`) reemplaza por generación
    /// en vez de encolar; aquí se fija el origen del presupuesto.
    #[test]
    fn in_flight_cap_is_one_by_budget() {
        assert_eq!(MAX_GPU_READBACK_JOBS_IN_FLIGHT, 1);
        assert_eq!(GPU_READBACK_TIMEOUT, Duration::from_millis(250));
    }
}
