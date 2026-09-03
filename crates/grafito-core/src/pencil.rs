//! Pencil — dibujo a mano alzada.
//!
//! Un [`PencilObj`] almacena la polilínea capturada durante un arrastre del
//! ratón: una secuencia de puntos en coordenadas del mundo, su color y grosor.
//! El render convierte los puntos en segmentos contiguos que reusan el
//! pipeline de líneas existente.

use grafito_geometry::{Color, Point2};
use serde::{Deserialize, Serialize};

use crate::id::ObjectId;

/// Máxima cantidad de puntos que puede retener un trazo a mano alzada.
pub const MAX_PENCIL_POINTS: usize = 8_192;

/// Relación persistente de un lugar geométrico local.
///
/// Sólo identifica los puntos geométricos que impulsan y producen la traza.
/// No guarda eventos de puntero, tiempo ni coordenadas de pantalla.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct LocusBinding {
    pub driver: ObjectId,
    pub target: ObjectId,
}

/// Trazo de lápiz a mano alzada. Cada `PencilObj` representa **un trazo
/// independiente** dentro del documento, lo que permite al usuario asignarle
/// color y grosor desde el panel de álgebra sin afectar a otros trazos.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PencilObj {
    pub id: ObjectId,
    pub label: String,
    /// Puntos capturados durante el arrastre, en coordenadas del mundo.
    /// El render los conecta como `[p0, p1, p2, …, pn]`, formando una
    /// polilínea de `n` segmentos.
    pub points: Vec<Point2>,
    pub color: Color,
    pub visible: bool,
    pub width: f32,
    /// Ausente para un trazo libre; presente para una trayectoria geométrica
    /// que debe seguir actualizándose con el documento.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub locus_binding: Option<LocusBinding>,
}

impl PencilObj {
    /// Crea un `PencilObj` vacío. El usuario añade los puntos durante el
    /// arrastre; el grosor y color por defecto se pueden cambiar después.
    pub fn new(points: Vec<Point2>) -> Self {
        Self {
            id: ObjectId::new(),
            label: String::new(),
            points,
            color: Color::DEFAULT_STROKE,
            visible: true,
            width: 2.0,
            locus_binding: None,
        }
    }

    /// Constructor fluido: asigna una etiqueta al trazo.
    pub fn with_label(mut self, label: impl Into<String>) -> Self {
        self.label = label.into();
        self
    }

    /// Constructor fluido: cambia el color del trazo.
    pub fn with_color(mut self, color: Color) -> Self {
        self.color = color;
        self
    }

    /// Constructor fluido: cambia el grosor del trazo.
    pub fn with_width(mut self, width: f32) -> Self {
        self.width = width;
        self
    }

    /// Marca el trazo como un lugar geométrico impulsado por dos puntos del
    /// documento. La validez de esas referencias se comprueba en `Document`.
    pub fn with_locus_binding(mut self, driver: ObjectId, target: ObjectId) -> Self {
        self.locus_binding = Some(LocusBinding { driver, target });
        self
    }

    /// Devuelve la relación dinámica cuando este trazo es un locus.
    pub const fn locus_binding(&self) -> Option<LocusBinding> {
        self.locus_binding
    }

    /// Indica si el trazo es una trayectoria geométrica persistente.
    pub const fn is_dynamic_locus(&self) -> bool {
        self.locus_binding.is_some()
    }

    /// Añade un punto al final del trazo. Usado durante el arrastre.
    /// Filtra muestras no finitas (`NaN`/`Inf`) para evitar corromper el
    /// índice espacial y la serialización; los puntos inválidos se descartan
    /// silenciosamente y no cuentan para la cota `MAX_PENCIL_POINTS`.
    pub fn push(&mut self, p: Point2) {
        if !p.x.is_finite() || !p.y.is_finite() {
            return;
        }
        while self.points.len() >= MAX_PENCIL_POINTS {
            let Some(last) = self.points.last().copied() else {
                break;
            };
            let mut decimated: Vec<_> = self.points.iter().step_by(2).copied().collect();
            if decimated.last() != Some(&last) {
                decimated.push(last);
            }
            self.points = decimated;
        }
        self.points.push(p);
    }

    /// Registra una muestra de locus sólo si es finita y distinta de la última.
    /// Devuelve si la polilínea cambió.
    /// Documenta: filtra `!finite` tanto aquí como en [`Self::push`] (defensa
    /// en profundidad — `push` es público y puede ser llamado fuera de locus).
    pub fn capture_locus_sample(&mut self, point: Point2) -> bool {
        if !self.is_dynamic_locus()
            || !point.x.is_finite()
            || !point.y.is_finite()
            || self.points.last() == Some(&point)
        {
            return false;
        }
        self.push(point);
        true
    }

    /// Devuelve la cantidad de puntos almacenados.
    pub fn len(&self) -> usize {
        self.points.len()
    }

    /// `true` si el trazo no contiene puntos.
    pub fn is_empty(&self) -> bool {
        self.points.is_empty()
    }

    /// Número de segmentos que el renderizará (= `points.len() - 1` si hay
    /// al menos dos puntos).
    pub fn segment_count(&self) -> usize {
        if self.points.len() < 2 {
            0
        } else {
            self.points.len() - 1
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn empty_pencil_has_no_segments() {
        let p = PencilObj::new(vec![]);
        assert!(p.is_empty());
        assert_eq!(p.segment_count(), 0);
    }

    #[test]
    fn single_point_yields_no_segments() {
        let p = PencilObj::new(vec![Point2::new(0.0, 0.0)]);
        assert_eq!(p.len(), 1);
        assert_eq!(p.segment_count(), 0);
    }

    #[test]
    fn n_points_yield_n_minus_one_segments() {
        let p = PencilObj::new(vec![
            Point2::new(0.0, 0.0),
            Point2::new(1.0, 1.0),
            Point2::new(2.0, 0.0),
            Point2::new(3.0, -1.0),
        ]);
        assert_eq!(p.len(), 4);
        assert_eq!(p.segment_count(), 3);
    }

    #[test]
    fn push_appends_points() {
        let mut p = PencilObj::new(vec![Point2::new(0.0, 0.0)]);
        p.push(Point2::new(1.0, 1.0));
        p.push(Point2::new(2.0, 2.0));
        assert_eq!(p.len(), 3);
        assert_eq!(p.segment_count(), 2);
    }

    #[test]
    fn push_decimates_full_stroke_and_preserves_endpoints() {
        let mut p = PencilObj::new(vec![Point2::new(0.0, 0.0)]);
        for x in 1..MAX_PENCIL_POINTS {
            p.push(Point2::new(x as f64, 0.0));
        }

        let previous_end = *p.points.last().expect("full stroke has an endpoint");
        let final_end = Point2::new(MAX_PENCIL_POINTS as f64, 0.0);
        p.push(final_end);

        assert!(p.len() <= MAX_PENCIL_POINTS);
        assert_eq!(p.points.first(), Some(&Point2::new(0.0, 0.0)));
        assert_eq!(p.points[p.len() - 2], previous_end);
        assert_eq!(p.points.last(), Some(&final_end));
    }

    #[test]
    fn persistence_rejects_pencil_with_too_many_points() {
        let points = (0..=MAX_PENCIL_POINTS)
            .map(|x| Point2::new(x as f64, 0.0))
            .collect();
        let object = crate::GeoObject::Pencil(PencilObj::new(points));
        let id = object.id();
        let mut raw = serde_json::to_value(crate::Document::new()).expect("serialize document");
        raw["objects"]
            .as_object_mut()
            .expect("objects are represented as a map")
            .insert(
                id.0.to_string(),
                serde_json::to_value(object).expect("serialize unchecked Pencil"),
            );
        let document: crate::Document =
            serde_json::from_value(raw).expect("deserialize unchecked test document");

        let save_error = crate::serialize_document(&document)
            .expect_err("over-cap Pencil must not be serialized");
        assert!(save_error.to_string().contains("Pencil points"));

        let raw_json =
            serde_json::to_string(&document).expect("serialize raw document for load test");
        let load_error = crate::deserialize_document(&raw_json)
            .expect_err("over-cap persisted Pencil must not be loaded");
        assert!(load_error.to_string().contains("Pencil points"));
    }

    #[test]
    fn builder_methods_set_fields() {
        let p = PencilObj::new(vec![Point2::new(0.0, 0.0), Point2::new(1.0, 1.0)])
            .with_label("mi trazo")
            .with_color(Color::new(1.0, 0.0, 0.0, 1.0))
            .with_width(4.0);
        assert_eq!(p.label, "mi trazo");
        assert!((p.color.r - 1.0).abs() < 1e-9);
        assert!((p.width - 4.0).abs() < 1e-6);
    }
}
