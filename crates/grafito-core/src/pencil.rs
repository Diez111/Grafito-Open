//! Pencil — dibujo a mano alzada.
//!
//! Un [`PencilObj`] almacena la polilínea capturada durante un arrastre del
//! ratón: una secuencia de puntos en coordenadas del mundo, su color y grosor.
//! El render convierte los puntos en segmentos contiguos que reusan el
//! pipeline de líneas existente.

use grafito_geometry::{Color, Point2};
use serde::{Deserialize, Serialize};

use crate::id::ObjectId;

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
}

impl PencilObj {
    /// Crea un `PencilObj` vacío. El usuario añade los puntos durante el
    /// arrastre; el grosor y color por defecto se pueden cambiar después.
    ///
    /// Por defecto usamos un azul oscuro semitransparente y un grosor
    /// moderado, para que el trazo se distinga claramente de una
    /// `LineObj` técnica y no parezca una "línea negra gigante" cuando el
    /// PencilObj persiste.
    pub fn new(points: Vec<Point2>) -> Self {
        Self {
            id: ObjectId::new(),
            label: String::new(),
            points,
            color: Color::new(0.2, 0.3, 0.6, 0.85),
            visible: true,
            width: 1.5,
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

    /// Añade un punto al final del trazo. Usado durante el arrastre.
    pub fn push(&mut self, p: Point2) {
        self.points.push(p);
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
