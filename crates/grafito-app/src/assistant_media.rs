//! Media del asistente (slice 5 del split de god objects).
//!
//! `AssistantMediaController` es dueño de los 6 slots vivos del turno de
//! animación/export (`anim`/`anim_ia`/`gif`/`png`/`mp4`/`webm`). Núcleos
//! canónicos puros (`cancel_media_jobs`, `any_media_export_in_flight`,
//! `take_ready_*`, `signal_exports_cancel`, presupuestos) viven acá;
//! `Context`, `set_media`, historial Thumb+Replay + trim y spawns quedan en
//! `assistant.rs` (shims finos, mismo patrón slice 2/3/4 con
//! `FileController`). El controller nunca guarda `egui::Context` ni hace
//! I/O: solo slots/tokens + transiciones.
//!
//! `AssistantRuntime` (`assistant.rs`) conserva compat vía
//! `Deref/DerefMut` al controller, así `runtime.anim_job` sigue compilando
//! sin cambiar aserciones de tests.

use crate::manim_orchestrator::AnimHistoryCoords;

// ── Jobs (movidos verbatim desde `assistant.rs`, solo cambia visibilidad) ──

/// W-B — render listo desde el worker IA-primero (media + prosa coherentes).
///
/// `media` y `prosa` vienen del MISMO spec validado (o IA o canónico de
/// fallback, nunca mezclados). `aviso` es `Some` solo en fallback.
pub(crate) struct AnimIaRender {
    pub media: grafito_ui::assistant::AssistantMedia,
    pub prosa: String,
    pub aviso: Option<String>,
    /// Coords efectivas del replay (plantilla+concepto renderizados, no el
    /// pedido crudo).
    pub template: String,
    pub concept: String,
}

/// AS4: con `cancellation` como `AssistantAgentJob` (cancel real: descartar
/// señala el token, no solo dropea el receiver).
pub(crate) struct AssistantAnimJob {
    pub(crate) cancellation: grafito_assistant::CancellationToken,
    pub(crate) receiver:
        std::sync::mpsc::Receiver<Result<grafito_ui::assistant::AssistantMedia, String>>,
    /// Coords W1 para historiar Thumb+Replay en el turno recién creado.
    ///
    /// `Some` solo en el single normal; `None` en playlist multi-step y replay.
    pub(crate) history: Option<AnimHistoryCoords>,
}

/// W-B — job del worker IA-primero (SPEC de la IA + render en un solo hilo).
/// Mismo contrato de cancel que `AssistantAnimJob`.
pub(crate) struct AssistantAnimIaJob {
    pub(crate) cancellation: grafito_assistant::CancellationToken,
    pub(crate) receiver: std::sync::mpsc::Receiver<Result<AnimIaRender, String>>,
}

/// Export a GIF de la card en vuelo (B5).
pub(crate) struct GifExportJob {
    pub(crate) handle:
        std::thread::JoinHandle<Result<std::path::PathBuf, crate::anim_native::GifExportError>>,
    pub(crate) frame_count: usize,
    pub(crate) cancel: grafito_assistant::CancellationToken,
    pub(crate) path: std::path::PathBuf,
}

/// Export a PNG-sequence de la card en vuelo (mismo contrato que GIF).
pub(crate) struct PngDirExportJob {
    pub(crate) handle:
        std::thread::JoinHandle<Result<std::path::PathBuf, crate::anim_native::PngDirExportError>>,
    pub(crate) frame_count: usize,
    pub(crate) cancel: grafito_assistant::CancellationToken,
    pub(crate) path: std::path::PathBuf,
}

/// Export a MP4 de la card en vuelo (vía ffmpeg-sidecar).
pub(crate) struct Mp4ExportJob {
    pub(crate) handle:
        std::thread::JoinHandle<Result<std::path::PathBuf, crate::anim_native::Mp4ExportError>>,
    pub(crate) frame_count: usize,
    pub(crate) cancel: grafito_assistant::CancellationToken,
    pub(crate) path: std::path::PathBuf,
}

/// Export a WebM de la card en vuelo (vía ffmpeg-sidecar).
pub(crate) struct WebmExportJob {
    pub(crate) handle:
        std::thread::JoinHandle<Result<std::path::PathBuf, crate::anim_native::WebmExportError>>,
    pub(crate) frame_count: usize,
    pub(crate) cancel: grafito_assistant::CancellationToken,
    pub(crate) path: std::path::PathBuf,
}

// ── Puros (movidos verbatim) ──

/// ¿La plantilla del título soporta vista órbita? (puro, sin I/O).
pub(crate) fn export_orbit_supported_for_title(title: &str) -> bool {
    let norm = title.to_lowercase();
    [
        "orbita",
        "órbita",
        "3d",
        "cubo",
        "esfera",
        "toro",
        "cono",
        "cilindro",
        "piramide",
        "pirámide",
        "prisma",
        "tetra",
    ]
    .iter()
    .any(|pista| norm.contains(pista))
}

/// Cota del reaper GIF R1-4: el `join` en el path de cancel nunca bloquea
/// más que esto (poll `is_finished` cada 50 ms).
pub(crate) const GIF_REAPER_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(5);

/// `join` acotado R1-4: espera hasta `timeout` (poll 50 ms) y devuelve
/// `Some(resultado)` si el hilo terminó, `None` si dio timeout (hilo
/// detached).
pub(crate) fn join_gif_handle_bounded(
    handle: std::thread::JoinHandle<Result<std::path::PathBuf, crate::anim_native::GifExportError>>,
    timeout: std::time::Duration,
) -> Option<Result<std::path::PathBuf, crate::anim_native::GifExportError>> {
    let inicio = std::time::Instant::now();
    while !handle.is_finished() {
        if inicio.elapsed() >= timeout {
            std::mem::forget(handle);
            return None;
        }
        std::thread::sleep(std::time::Duration::from_millis(50));
    }
    match handle.join() {
        Ok(res) => Some(res),
        Err(_) => Some(Err(crate::anim_native::GifExportError::Encode(
            "la exportación terminó inesperadamente".to_string(),
        ))),
    }
}

/// R2-V2 (puro, testeable): valida `len <= PLAYLIST_MAX_STEPS` (8) con
/// `Err` acotado. El worker la llama antes de renderizar para que el
/// struct literal con 64 steps no acumule OOM.
pub(crate) fn playlist_len_budget_ok(len: usize) -> Result<(), String> {
    if len > grafito_anim::protocol::PLAYLIST_MAX_STEPS {
        return Err(format!(
            "la playlist trae {len} steps y excede el tope de {}: partila en dos",
            grafito_anim::protocol::PLAYLIST_MAX_STEPS
        ));
    }
    Ok(())
}

// ── Controller ──

/// Dueño de los 6 slots vivos de media del turno.
///
/// Nunca guarda `egui::Context`, nunca llama `set_media`, nunca toca el
/// historial/trim (eso queda en `assistant.rs`). Solo slots/tokens +
/// transiciones puras (`poll_*` por parámetro/devolución).
#[derive(Default)]
pub(crate) struct AssistantMediaController {
    pub(crate) anim_job: Option<AssistantAnimJob>,
    /// W-B: worker IA-primero. Cero doble render: o `anim_job` o este, nunca ambos.
    pub(crate) anim_ia_job: Option<AssistantAnimIaJob>,
    /// Export a GIF en vuelo (`JoinHandle` que el poll drena sin bloquear).
    pub(crate) gif_export_job: Option<GifExportJob>,
    /// Export a PNG-sequence en vuelo.
    pub(crate) png_export_job: Option<PngDirExportJob>,
    /// Export a MP4 en vuelo.
    pub(crate) mp4_export_job: Option<Mp4ExportJob>,
    /// Export a WebM en vuelo.
    pub(crate) webm_export_job: Option<WebmExportJob>,
}

impl AssistantMediaController {
    /// Controller idle (los 6 slots `None`). API para el hundimiento del
    /// slice 6 (`AssistantRuntime` aún usa `Default` + `Deref`).
    #[allow(dead_code)]
    pub(crate) fn new() -> Self {
        Self::default()
    }

    /// Sin ningún job de media en vuelo. API para el hundimiento slice 6.
    #[allow(dead_code)]
    pub(crate) fn is_idle(&self) -> bool {
        self.anim_job.is_none()
            && self.anim_ia_job.is_none()
            && self.gif_export_job.is_none()
            && self.png_export_job.is_none()
            && self.mp4_export_job.is_none()
            && self.webm_export_job.is_none()
    }

    /// ¿Hay algún export de la card en vuelo (cualquier formato)?
    /// Puro sobre los slots, sin I/O.
    pub(crate) fn any_media_export_in_flight(&self) -> bool {
        self.gif_export_job.is_some()
            || self.png_export_job.is_some()
            || self.mp4_export_job.is_some()
            || self.webm_export_job.is_some()
    }

    /// ¿Hay cualquier export en vuelo incluyendo los LaTeX (PDF/SVG) que
    /// viven fuera del controller (dueño: `AssistantRuntime` en
    /// `assistant.rs`)?
    ///
    /// El llamante pasa `latex_in_flight`
    /// (`runtime.any_latex_export_in_flight()`): el controller no toca esos
    /// slots, solo los cuenta para que el reset `Exporting → Idle` del cancel
    /// no deje la card colgada cuando solo vuela un PDF/SVG. Puro, sin I/O.
    ///
    /// Puente B2 para el dueño de `assistant.rs` (aún no adoptado en prod;
    /// el test lo pinea hasta el hunk de adopción).
    #[allow(dead_code)]
    pub(crate) fn any_media_export_in_flight_with_latex(&self, latex_in_flight: bool) -> bool {
        self.any_media_export_in_flight() || latex_in_flight
    }

    /// Cancela TODO lo vivo de media del turno (núcleo canónico, movido
    /// verbatim desde `AssistantRuntime::cancel_anim_job` — solo la parte
    /// de media; remote/proposal/agent/model quedan en `assistant.rs`).
    ///
    /// - `anim`/`anim_ia` tienen token (AS4): se señalan y se dropea el slot.
    /// - `gif`/`png`/`mp4`/`webm` (`JoinHandle`, no cancelable): se señala el
    ///   token, se suelta el slot y un reaper en background hace `join` +
    ///   borra el temporal (sin basura ni éxito de turno cancelado).
    ///
    /// Retorna `true` si había algún job de media en vuelo. Sin I/O en el
    /// llamante salvo spawns de reapers acotados.
    pub(crate) fn cancel_media_jobs(&mut self) -> bool {
        let mut hubo = false;
        if let Some(job) = self.anim_job.as_ref() {
            job.cancellation.cancel();
        }
        if let Some(job) = self.anim_ia_job.as_ref() {
            job.cancellation.cancel();
        }
        if self.anim_job.is_some() {
            self.anim_job = None;
            hubo = true;
        }
        if self.anim_ia_job.is_some() {
            self.anim_ia_job = None;
            hubo = true;
        }
        if let Some(job) = self.gif_export_job.take() {
            job.cancel.cancel();
            let ruta_conocida = job.path.clone();
            let _ = std::thread::Builder::new()
                .name("gif-export-reaper".into())
                .spawn(
                    move || match join_gif_handle_bounded(job.handle, GIF_REAPER_TIMEOUT) {
                        Some(Ok(path)) => {
                            let _ = std::fs::remove_file(path);
                        }
                        Some(Err(_)) => {
                            let _ = std::fs::remove_file(&ruta_conocida);
                        }
                        None => {
                            eprintln!(
                                "[gif-reaper] timeout tras {:?}, hilo detached; borro {}",
                                GIF_REAPER_TIMEOUT,
                                ruta_conocida.display()
                            );
                            let _ = std::fs::remove_file(&ruta_conocida);
                        }
                    },
                );
            hubo = true;
        }
        if let Some(job) = self.png_export_job.take() {
            job.cancel.cancel();
            let ruta_conocida = job.path.clone();
            let _ = std::thread::Builder::new()
                .name("pngdir-export-reaper".into())
                .spawn(move || {
                    let _ = job.handle.join();
                    let _ = std::fs::remove_dir_all(&ruta_conocida);
                });
            hubo = true;
        }
        if let Some(job) = self.mp4_export_job.take() {
            job.cancel.cancel();
            let ruta_conocida = job.path.clone();
            let _ = std::thread::Builder::new()
                .name("mp4-export-reaper".into())
                .spawn(move || {
                    let _ = job.handle.join();
                    let _ = std::fs::remove_file(&ruta_conocida);
                });
            hubo = true;
        }
        if let Some(job) = self.webm_export_job.take() {
            job.cancel.cancel();
            let ruta_conocida = job.path.clone();
            let _ = std::thread::Builder::new()
                .name("webm-export-reaper".into())
                .spawn(move || {
                    let _ = job.handle.join();
                    let _ = std::fs::remove_file(&ruta_conocida);
                });
            hubo = true;
        }
        hubo
    }

    /// Cancela lo vivo de media del turno + señala tokens LaTeX externos
    /// (PDF/SVG, dueño `assistant.rs`).
    ///
    /// Puente del Cancel global: corre [`cancel_media_jobs`] (suelta los 6
    /// slots propios con sus reapers) y además señala cada token de `extra`
    /// SIN soltar sus slots (el poll de `assistant.rs` los drena honesto vía
    /// `take_ready_pdf`/`take_ready_svg`, igual que los granulares
    /// remote/proposal/agent/model que conservan su slot hasta el drain).
    /// Retorna `true` si había algo propio o algún extra sin señalar antes.
    ///
    /// Sin I/O en el llamante salvo los reapers acotados de
    /// [`cancel_media_jobs`]. Hunk para el dueño de `assistant.rs` (ver
    /// reporte B2): el Cancel global debe pasar
    /// `&[pdf.cancel, svg.cancel]` (o llamar a esto desde
    /// `cancel_anim_job`/`AssistantJobsController::cancel_turno_anim`).
    ///
    /// Puente B2 para el dueño de `assistant.rs` (aún no adoptado en prod;
    /// el test lo pinea hasta el hunk de adopción).
    #[allow(dead_code)]
    pub(crate) fn cancel_media_jobs_with_extra_cancels(
        &mut self,
        extra: &[grafito_assistant::CancellationToken],
    ) -> bool {
        let mut hubo = self.cancel_media_jobs();
        for token in extra {
            // `cancel()` es idempotente (store `true`): señalar dos veces no
            // pierde nada y evita la carrera señal-antes-que-poll.
            token.cancel();
            hubo = true;
        }
        hubo
    }

    /// Señala el `CancellationToken` de cada export en vuelo sin soltar slots
    /// (núcleo del botón `Cancel` del diálogo; el poll drena el resultado
    /// honesto). Retorna si había algo que señalar. Sin I/O, sin `Context`.
    ///
    /// Cubre SOLO los 4 slots clásicos del controller. Los LaTeX (PDF/SVG)
    /// viven en `AssistantRuntime` (`assistant.rs`, fuera de este archivo):
    /// el Cancel global debe señalarlos vía
    /// [`cancel_media_jobs_with_extra_cancels`] o
    /// `AssistantRuntime::signal_latex_exports_cancel`; si no, quedan
    /// huérfanos (hilo vivo + card reseteada que su poll ya no drena).
    pub(crate) fn signal_exports_cancel(&self) -> bool {
        let mut hubo = false;
        if let Some(job) = self.gif_export_job.as_ref() {
            job.cancel.cancel();
            hubo = true;
        }
        if let Some(job) = self.png_export_job.as_ref() {
            job.cancel.cancel();
            hubo = true;
        }
        if let Some(job) = self.mp4_export_job.as_ref() {
            job.cancel.cancel();
            hubo = true;
        }
        if let Some(job) = self.webm_export_job.as_ref() {
            job.cancel.cancel();
            hubo = true;
        }
        hubo
    }

    /// Saca el job GIF solo si su hilo ya terminó (`is_finished`).
    /// El `join` + aviso quedan en `assistant.rs` (necesitan panel/notify/ctx).
    pub(crate) fn take_ready_gif(&mut self) -> Option<GifExportJob> {
        if self
            .gif_export_job
            .as_ref()
            .is_some_and(|job| job.handle.is_finished())
        {
            self.gif_export_job.take()
        } else {
            None
        }
    }

    /// Saca el job PNG-dir solo si su hilo ya terminó.
    pub(crate) fn take_ready_png(&mut self) -> Option<PngDirExportJob> {
        if self
            .png_export_job
            .as_ref()
            .is_some_and(|job| job.handle.is_finished())
        {
            self.png_export_job.take()
        } else {
            None
        }
    }

    /// Saca el job MP4 solo si su hilo ya terminó.
    pub(crate) fn take_ready_mp4(&mut self) -> Option<Mp4ExportJob> {
        if self
            .mp4_export_job
            .as_ref()
            .is_some_and(|job| job.handle.is_finished())
        {
            self.mp4_export_job.take()
        } else {
            None
        }
    }

    /// Saca el job WebM solo si su hilo ya terminó.
    pub(crate) fn take_ready_webm(&mut self) -> Option<WebmExportJob> {
        if self
            .webm_export_job
            .as_ref()
            .is_some_and(|job| job.handle.is_finished())
        {
            self.webm_export_job.take()
        } else {
            None
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn media_controller_idle_y_export_gate() {
        let ctl = AssistantMediaController::new();
        assert!(ctl.is_idle());
        assert!(!ctl.any_media_export_in_flight());
    }

    #[test]
    fn media_controller_cancel_sin_jobs_es_noop() {
        let mut ctl = AssistantMediaController::new();
        assert!(!ctl.cancel_media_jobs());
        assert!(!ctl.signal_exports_cancel());
        assert!(ctl.take_ready_gif().is_none());
        assert!(ctl.take_ready_png().is_none());
        assert!(ctl.take_ready_mp4().is_none());
        assert!(ctl.take_ready_webm().is_none());
    }

    #[test]
    fn media_controller_orbit_y_playlist_budget_puros() {
        assert!(export_orbit_supported_for_title("Cubo orbitando"));
        assert!(!export_orbit_supported_for_title("derivada como pendiente"));
        assert!(playlist_len_budget_ok(8).is_ok());
        assert!(playlist_len_budget_ok(64).is_err());
    }

    #[test]
    fn media_controller_extra_cancels_seniala_latex_externo() {
        let mut ctl = AssistantMediaController::new();
        let pdf_cancel = grafito_assistant::CancellationToken::default();
        let svg_cancel = grafito_assistant::CancellationToken::default();
        // Sin nada propio pero con extras: igual retorna true y los señala.
        assert!(ctl.cancel_media_jobs_with_extra_cancels(&[pdf_cancel.clone(), svg_cancel.clone()]));
        assert!(pdf_cancel.is_cancelled());
        assert!(svg_cancel.is_cancelled());
        // Sin nada en absoluto: noop honesto.
        let mut vacio = AssistantMediaController::new();
        assert!(!vacio.cancel_media_jobs_with_extra_cancels(&[]));
    }

    #[test]
    fn media_controller_any_export_incluye_latex() {
        let ctl = AssistantMediaController::new();
        assert!(!ctl.any_media_export_in_flight_with_latex(false));
        assert!(ctl.any_media_export_in_flight_with_latex(true));
    }
}
