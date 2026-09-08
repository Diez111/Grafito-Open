//! Superficies implícitas en background thread (frente A3).
//!
//! Réplica del patrón repo `grafito-render/src/implicit_compute.rs:635-654`
//! (`thread::spawn` + `sync_channel(1)` + `try_recv` por frame) para
//! `F(x, y, z) = 0` por marching-tetrahedra
//! ([`grafito_geometry::polytopes::implicit_surface_mesh`]).
//!
//! - [`spawn_implicit_surface_job`] mueve un [`ImplicitSurfaceRequest`] con
//!   datos `owned` al worker y devuelve el `Receiver` sin bloquear jamás.
//! - [`ImplicitSurfaceSlot`] es el slot cap-1 del frame: `submit` reemplaza el
//!   job en vuelo, `poll` solo hace `try_recv` (nunca bloquea) y conserva el
//!   último válido (`last_valid`) ante `Failed` para que la UI nunca parpadee.
//!
//! Cableado UI pendiente (seam A3, archivos prohibidos en este frente):
//! quien posea el slot debe llamar `poll` una vez por frame y pedir
//! `ctx.request_repaint()` mientras [`ImplicitSurfaceSlot::has_pending`].
//! Ver `BLOCKERS` en el mensaje de entrega A3.

use std::sync::mpsc::{sync_channel, Receiver, TryRecvError};
use std::sync::Arc;

use grafito_geometry::polytopes::{
    implicit_surface_mesh, MeshError, TriangleMesh3D, GB_MAX_MARCHING_CELLS_PER_AXIS,
};
use grafito_geometry::types3d::Point3D;

/// Campo implícito `F(x, y, z)` compartido con el worker.
///
/// `None` = campo no definido ahí (fail-closed
/// [`MeshError::FieldUndefined`]); el valor debe ser finito o el job falla
/// honesto en vez de mentir. `Arc` para que el request sea `owned` + `Send`.
pub type ImplicitField = Arc<dyn Fn(f64, f64, f64) -> Option<f64> + Send + Sync + 'static>;

/// Celdas por eje por defecto (igual que `ImplicitSurface` sin `res`).
pub const IMPLICIT_SURFACE_DEFAULT_CELLS: usize = 16;

/// Pedido `owned` de superficie implícita, listo para cruzar al worker.
#[derive(Clone)]
pub struct ImplicitSurfaceRequest {
    /// Campo `F(x, y, z)` a isosuperficar (`f < 0` = interior).
    pub field: ImplicitField,
    /// Esquina mínima de la caja de muestreo (finita).
    pub min: Point3D,
    /// Esquina máxima de la caja de muestreo (finita, mayor que `min`).
    pub max: Point3D,
    /// Celdas por eje `1..=32` (`32³ = 32 768` celdas, ver
    /// [`GB_MAX_MARCHING_CELLS_PER_AXIS`]).
    pub cells_per_axis: usize,
}

impl ImplicitSurfaceRequest {
    /// Construye un pedido validando cotas ANTES de spawnear ningún thread.
    pub fn new(
        field: ImplicitField,
        min: Point3D,
        max: Point3D,
        cells_per_axis: usize,
    ) -> Result<Self, MeshError> {
        if !min.is_finite()
            || !max.is_finite()
            || min.x >= max.x
            || min.y >= max.y
            || min.z >= max.z
        {
            return Err(MeshError::NonFiniteInput);
        }
        if cells_per_axis < 1 {
            return Err(MeshError::TooFewPoints {
                found: cells_per_axis,
                minimum: 1,
            });
        }
        if cells_per_axis > GB_MAX_MARCHING_CELLS_PER_AXIS {
            return Err(MeshError::TooManySegments {
                found: cells_per_axis,
                maximum: GB_MAX_MARCHING_CELLS_PER_AXIS,
            });
        }
        Ok(Self {
            field,
            min,
            max,
            cells_per_axis,
        })
    }
}

/// Resultado del worker: malla soldada o error honesto (presupuesto, campo
/// indefinido, geometría degenerada). `Send` porque todo es `owned`.
pub type ImplicitSurfaceOutcome = Result<TriangleMesh3D, MeshError>;

/// Marching-tetrahedra en background con datos `owned` (patrón repo
/// `implicit_compute.rs:635`: `thread::spawn` + `sync_channel(1)`).
///
/// Retorna de inmediato; el `send` del worker nunca bloquea (capacidad 1 +
/// un único mensaje). El error de validación ya salió en
/// [`ImplicitSurfaceRequest::new`], así que aquí no hay `Result`.
pub fn spawn_implicit_surface_job(
    request: ImplicitSurfaceRequest,
) -> Receiver<ImplicitSurfaceOutcome> {
    let (tx, rx) = sync_channel::<ImplicitSurfaceOutcome>(1);
    std::thread::spawn(move || {
        let field = request.field.clone();
        let outcome = implicit_surface_mesh(
            &|x, y, z| field(x, y, z),
            request.min,
            request.max,
            request.cells_per_axis,
        );
        let _ = tx.send(outcome);
    });
    rx
}

/// Paso non-blocking del slot: `Pending` devuelve el job al slot (sigue en
/// vuelo); `Ready`/`Failed` son terminales y liberan el slot.
#[derive(Debug)]
pub enum SurfaceSlotPoll {
    /// Job en vuelo o slot idle sin novedad: re-encolar / seguir mostrando
    /// [`ImplicitSurfaceSlot::last_valid`].
    Pending,
    /// Malla fresca (ya guardada como último válido).
    Ready(TriangleMesh3D),
    /// El worker falló honesto; el último válido se conserva intacto.
    Failed(MeshError),
}

/// Slot cap-1 de superficie implícita para el frame de UI.
///
/// - `submit` reemplaza el job en vuelo (el anterior se suelta; su `send`
///   falla silencioso y su thread termina solo).
/// - `poll` solo hace `try_recv`: jamás bloquea el hilo UI.
/// - `last_valid` es lo que la UI renderiza mientras hay `Pending`.
#[derive(Debug, Default)]
pub struct ImplicitSurfaceSlot {
    receiver: Option<Receiver<ImplicitSurfaceOutcome>>,
    last_valid: Option<TriangleMesh3D>,
    pending_cells: usize,
}

impl ImplicitSurfaceSlot {
    /// Slot idle sin último válido.
    pub fn new() -> Self {
        Self::default()
    }

    /// Valida y spawnea; reemplaza el job en vuelo si lo había.
    pub fn submit_new(
        &mut self,
        field: ImplicitField,
        min: Point3D,
        max: Point3D,
        cells_per_axis: usize,
    ) -> Result<(), MeshError> {
        let request = ImplicitSurfaceRequest::new(field, min, max, cells_per_axis)?;
        self.receiver = Some(spawn_implicit_surface_job(request));
        self.pending_cells = cells_per_axis;
        Ok(())
    }

    /// Avance non-blocking para llamar una vez por frame.
    pub fn poll(&mut self) -> SurfaceSlotPoll {
        let receiver = match self.receiver.take() {
            None => return SurfaceSlotPoll::Pending,
            Some(receiver) => receiver,
        };
        match receiver.try_recv() {
            Err(TryRecvError::Empty) => {
                self.receiver = Some(receiver);
                SurfaceSlotPoll::Pending
            }
            Err(TryRecvError::Disconnected) => {
                SurfaceSlotPoll::Failed(MeshError::DegenerateGeometry {
                    reason: "el job de superficie implícita murió sin resultado",
                })
            }
            Ok(Ok(mesh)) => {
                self.pending_cells = 0;
                // Un solo clone: uno se mueve al slot, la copia sale en
                // `Ready`. Antes había dos (`mesh.clone()` + `last_valid.clone()`).
                let ready = mesh.clone();
                self.last_valid = Some(mesh);
                SurfaceSlotPoll::Ready(ready)
            }
            Ok(Err(error)) => {
                self.pending_cells = 0;
                SurfaceSlotPoll::Failed(error)
            }
        }
    }

    /// Última malla válida (lo que la UI debe renderizar durante `Pending`).
    pub fn last_valid(&self) -> Option<&TriangleMesh3D> {
        self.last_valid.as_ref()
    }

    /// `true` si hay un job en vuelo (la UI debe pedir otro frame).
    pub fn has_pending(&self) -> bool {
        self.receiver.is_some()
    }

    /// Celdas por eje del job en vuelo (`0` si idle).
    pub fn pending_cells(&self) -> usize {
        if self.has_pending() {
            self.pending_cells
        } else {
            0
        }
    }

    /// Suelta el job en vuelo sin tocar el último válido.
    pub fn cancel(&mut self) {
        self.receiver = None;
        self.pending_cells = 0;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::{Duration, Instant};

    fn sphere_field() -> ImplicitField {
        Arc::new(|x: f64, y: f64, z: f64| Some(x * x + y * y + z * z - 1.0))
    }

    fn test_box() -> (Point3D, Point3D) {
        (Point3D::new(-1.5, -1.5, -1.5), Point3D::new(1.5, 1.5, 1.5))
    }

    fn poll_until_done(slot: &mut ImplicitSurfaceSlot) -> SurfaceSlotPoll {
        let deadline = Instant::now() + Duration::from_secs(60);
        loop {
            match slot.poll() {
                SurfaceSlotPoll::Pending => {
                    assert!(Instant::now() < deadline, "el job 24³ no terminó en 60 s");
                    std::thread::sleep(Duration::from_millis(5));
                }
                done => return done,
            }
        }
    }

    #[test]
    fn spawning_returns_immediately_and_24_cubed_completes_via_poll() {
        let (min, max) = test_box();
        // El spawn no computa nada inline: debe volver al acto (24³ en el
        // hilo UI bloquearía ~segundos en debug).
        let started = Instant::now();
        let mut slot = ImplicitSurfaceSlot::new();
        slot.submit_new(sphere_field(), min, max, 24)
            .expect("pedido 24³ válido");
        assert!(
            started.elapsed() < Duration::from_secs(1),
            "submit bloqueó: {:?}",
            started.elapsed()
        );
        assert!(slot.has_pending());
        assert_eq!(slot.pending_cells(), 24);
        assert!(slot.last_valid().is_none());
        // Poll non-blocking: el primer poll sale al acto aunque no haya nada.
        let poll_started = Instant::now();
        let _ = slot.poll();
        assert!(
            poll_started.elapsed() < Duration::from_secs(1),
            "poll bloqueó"
        );
        let outcome = poll_until_done(&mut slot);
        let mesh = match outcome {
            SurfaceSlotPoll::Ready(mesh) => mesh,
            SurfaceSlotPoll::Failed(error) => panic!("job 24³ falló: {error}"),
            SurfaceSlotPoll::Pending => panic!("poll salió Pending tras el deadline"),
        };
        assert!(!mesh.triangles().is_empty());
        assert!(mesh.triangle_count() < grafito_geometry::polytopes::GB_MAX_MESH_TRIANGLES);
        for vertex in mesh.vertices() {
            let value = vertex.x * vertex.x + vertex.y * vertex.y + vertex.z * vertex.z - 1.0;
            assert!(value.abs() < 0.1, "{vertex:?} f={value}");
        }
        assert!(!slot.has_pending());
        assert!(slot.last_valid().is_some());
    }

    #[test]
    fn invalid_cells_rejected_before_spawn() {
        let (min, max) = test_box();
        let mut slot = ImplicitSurfaceSlot::new();
        assert!(matches!(
            slot.submit_new(sphere_field(), min, max, 0),
            Err(MeshError::TooFewPoints { .. })
        ));
        assert!(matches!(
            slot.submit_new(sphere_field(), min, max, GB_MAX_MARCHING_CELLS_PER_AXIS + 1),
            Err(MeshError::TooManySegments { .. })
        ));
        assert!(matches!(
            slot.submit_new(sphere_field(), max, min, 8),
            Err(MeshError::NonFiniteInput)
        ));
        assert!(!slot.has_pending());
    }

    #[test]
    fn last_valid_preserved_on_failure() {
        let (min, max) = test_box();
        let mut slot = ImplicitSurfaceSlot::new();
        slot.submit_new(sphere_field(), min, max, 8)
            .expect("esfera válida");
        let ready = poll_until_done(&mut slot);
        assert!(matches!(ready, SurfaceSlotPoll::Ready(_)));
        let triangles = slot.last_valid().expect("último válido").triangle_count();
        // Campo con un agujero (fail-closed FieldUndefined).
        let hole: ImplicitField = Arc::new(
            |x: f64, _y: f64, _z: f64| {
                if x > 0.0 {
                    None
                } else {
                    Some(x)
                }
            },
        );
        slot.submit_new(hole, min, max, 8).expect("pedido válido");
        let failed = poll_until_done(&mut slot);
        assert!(
            matches!(
                failed,
                SurfaceSlotPoll::Failed(MeshError::FieldUndefined { .. })
            ),
            "{failed:?}"
        );
        // El último válido sigue intacto para la UI.
        assert_eq!(
            slot.last_valid().expect("último válido").triangle_count(),
            triangles
        );
        assert!(!slot.has_pending());
    }

    #[test]
    fn cancel_drops_pending_without_touching_last_valid() {
        let (min, max) = test_box();
        let mut slot = ImplicitSurfaceSlot::new();
        slot.submit_new(sphere_field(), min, max, 32)
            .expect("pedido 32³ válido");
        assert!(slot.has_pending());
        slot.cancel();
        assert!(!slot.has_pending());
        assert_eq!(slot.pending_cells(), 0);
        assert!(matches!(slot.poll(), SurfaceSlotPoll::Pending));
        assert!(slot.last_valid().is_none());
    }

    #[test]
    fn ready_triangle_count_matches_last_valid_sin_residuo() {
        // W-A: un solo clone — lo que sale en `Ready` es lo que queda en
        // `last_valid`, sin pendiente residual ni celdas colgadas.
        let (min, max) = test_box();
        let mut slot = ImplicitSurfaceSlot::new();
        slot.submit_new(sphere_field(), min, max, 8)
            .expect("esfera válida");
        let outcome = poll_until_done(&mut slot);
        let ready_count = match outcome {
            SurfaceSlotPoll::Ready(mesh) => mesh.triangle_count(),
            SurfaceSlotPoll::Failed(error) => panic!("job 8³ falló: {error}"),
            SurfaceSlotPoll::Pending => panic!("poll salió Pending tras el deadline"),
        };
        assert!(ready_count > 0, "la esfera 8³ debe tener triángulos");
        assert_eq!(
            slot.last_valid().expect("último válido").triangle_count(),
            ready_count,
            "Ready y last_valid deben coincidir (un solo clone)"
        );
        assert!(!slot.has_pending(), "sin job residual tras Ready");
        assert_eq!(slot.pending_cells(), 0, "sin celdas residuales tras Ready");
    }
}
