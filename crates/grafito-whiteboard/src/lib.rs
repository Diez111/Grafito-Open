#![allow(unknown_lints, float_literal_f32_fallback)]
#![cfg_attr(test, allow(clippy::unwrap_used, clippy::expect_used))]
#![allow(clippy::uninlined_format_args)]
//! Pizarra de dibujo libre (inspirada en Excalidraw) como modelo puro y
//! headless: trazos suavizados, formas, flechas, texto, borrado y selección.
//! No depende de egui; la capa de dibujo vive en grafito-app/grafito-ui.

pub mod interaction;
pub mod text;

pub use interaction::{make_element, select_in_marquee, WhiteboardInteraction, WhiteboardTool};
pub use text::TextBuffer;

/// Elemento dibujable de la pizarra.
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub enum WhiteboardElement {
    /// Trazo libre (lista de puntos en coordenadas mundo).
    Stroke {
        points: Vec<(f64, f64)>,
        color: (u8, u8, u8),
        width: f64,
    },
    Rectangle {
        min: (f64, f64),
        max: (f64, f64),
        fill: Option<(u8, u8, u8)>,
    },
    Ellipse {
        center: (f64, f64),
        rx: f64,
        ry: f64,
    },
    Arrow {
        from: (f64, f64),
        to: (f64, f64),
    },
    Text {
        at: (f64, f64),
        text: String,
        size: f64,
    },
}

impl WhiteboardElement {
    /// Caja acotada de un elemento (finita).
    pub fn bounds(&self) -> Option<((f64, f64), (f64, f64))> {
        let (mut min, mut max) = match self {
            Self::Stroke { points, .. } => {
                let first = points.first().copied()?;
                (first, first)
            }
            Self::Rectangle { min, max, .. } => (*min, *max),
            Self::Ellipse { center, rx, ry } => (
                (center.0 - rx, center.1 - ry),
                (center.0 + rx, center.1 + ry),
            ),
            Self::Arrow { from, to } => (*from, *to),
            Self::Text { at, size, .. } => (*at, (at.0 + 0.9 * size, at.1 - size)),
        };
        if let Self::Stroke { points, .. } = self {
            for (x, y) in points {
                min.0 = min.0.min(*x);
                min.1 = min.1.min(*y);
                max.0 = max.0.max(*x);
                max.1 = max.1.max(*y);
            }
        }
        Some((min, max))
    }

    /// Aproximación de distancia al punto para hit-testing.
    pub fn distance_to(&self, pos: (f64, f64)) -> f64 {
        match self {
            Self::Stroke { points, .. } => points
                .iter()
                .map(|point| ((point.0 - pos.0).powi(2) + (point.1 - pos.1).powi(2)).sqrt())
                .fold(f64::INFINITY, f64::min),
            Self::Rectangle { min, max, .. } => {
                let nearest_x = pos.0.clamp(min.0, max.0);
                let nearest_y = pos.1.clamp(min.1, max.1);
                ((nearest_x - pos.0).powi(2) + (nearest_y - pos.1).powi(2)).sqrt()
            }
            Self::Ellipse { center, rx, ry } => {
                let nx = (pos.0 - center.0) / rx.max(1e-9);
                let ny = (pos.1 - center.1) / ry.max(1e-9);
                ((nx * nx + ny * ny).sqrt() - 1.0).abs()
            }
            Self::Arrow { from, to } => point_segment_distance(*from, *to, pos),
            Self::Text { at, .. } => ((pos.0 - at.0).powi(2) + (pos.1 - at.1).powi(2)).sqrt(),
        }
    }
}

fn point_segment_distance(a: (f64, f64), b: (f64, f64), p: (f64, f64)) -> f64 {
    let dx = b.0 - a.0;
    let dy = b.1 - a.1;
    let length_sq = dx * dx + dy * dy;
    if length_sq == 0.0 {
        return ((p.0 - a.0).powi(2) + (p.1 - a.1).powi(2)).sqrt();
    }
    let t = (((p.0 - a.0) * dx + (p.1 - a.1) * dy) / length_sq).clamp(0.0, 1.0);
    let proj = (a.0 + t * dx, a.1 + t * dy);
    ((p.0 - proj.0).powi(2) + (p.1 - proj.1).powi(2)).sqrt()
}

/// Documento de pizarra con elementos y selección.
#[derive(Debug, Clone, Default, serde::Serialize, serde::Deserialize)]
pub struct WhiteboardDoc {
    elements: Vec<WhiteboardElement>,
    selected: Option<usize>,
}

impl WhiteboardDoc {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn len(&self) -> usize {
        self.elements.len()
    }

    pub fn is_empty(&self) -> bool {
        self.elements.is_empty()
    }

    pub fn add(&mut self, element: WhiteboardElement) {
        self.elements.push(element);
    }

    pub fn elements(&self) -> &[WhiteboardElement] {
        &self.elements
    }

    pub fn elements_mut(&mut self) -> &mut [WhiteboardElement] {
        &mut self.elements
    }

    #[allow(clippy::manual_map)]
    pub fn element_mut(&mut self, index: usize) -> Option<&mut WhiteboardElement> {
        self.elements.get_mut(index)
    }

    pub fn selected(&self) -> Option<usize> {
        self.selected
    }

    pub fn select_at(&mut self, pos: (f64, f64), tolerance: f64) -> Option<usize> {
        let index = self
            .elements
            .iter()
            .enumerate()
            .filter_map(|(index, element)| (element.distance_to(pos) <= tolerance).then_some(index))
            .min();
        self.selected = index;
        index
    }

    /// Borra el elemento más cercano a `pos`, devolviendo su índice.
    pub fn erase_at(&mut self, pos: (f64, f64), tolerance: f64) -> Option<usize> {
        let index = self.select_at(pos, tolerance)?;
        self.elements.remove(index);
        self.selected = None;
        Some(index)
    }

    pub fn clear(&mut self) {
        self.elements.clear();
        self.selected = None;
    }

    /// Descripción estructurada compacta del contenido (para análisis con IA).
    /// Esta proyección de texto permite al asistente «ver» la pizarra sin
    /// enviar píxeles; un modelo de visión barato podría sustituirla luego.
    pub fn describe(&self) -> String {
        let mut strokes = 0usize;
        let mut shapes = 0usize;
        let mut arrows = 0usize;
        let mut texts = Vec::new();
        for element in &self.elements {
            match element {
                WhiteboardElement::Stroke { points, .. } => {
                    strokes += 1;
                    let _ = points;
                }
                WhiteboardElement::Rectangle { .. } | WhiteboardElement::Ellipse { .. } => {
                    shapes += 1;
                }
                WhiteboardElement::Arrow { .. } => arrows += 1,
                WhiteboardElement::Text { text, .. } => {
                    if !text.trim().is_empty() {
                        texts.push(format!("\"{text}\""));
                    }
                }
            }
        }
        if self.elements.is_empty() {
            return "(pizarra vacía)".to_string();
        }
        let mut description =
            format!("{strokes} trazos, {shapes} formas (rectángulos/elipses), {arrows} flechas",);
        if !texts.is_empty() {
            description.push_str(", textos: ");
            description.push_str(&texts.join(", "));
        }
        let (min, max) = self
            .elements
            .iter()
            .filter_map(|element| element.bounds())
            .fold(
                (
                    (f64::INFINITY, f64::INFINITY),
                    (f64::NEG_INFINITY, f64::NEG_INFINITY),
                ),
                |acc, (elem_min, elem_max)| {
                    let ((amin, bmin), (amax, bmax)) = acc;
                    (
                        (amin.min(elem_min.0), bmin.min(elem_min.1)),
                        (amax.max(elem_max.0), bmax.max(elem_max.1)),
                    )
                },
            );
        if (max.0 - min.0).is_finite() && (max.1 - min.1).is_finite() {
            description.push_str(&format!(
                " en un área de {}×{} unidades",
                (max.0 - min.0).abs().round(),
                (max.1 - min.1).abs().round()
            ));
        }
        description
    }
}

/// Densifica un trazo con interpolación Catmull-Rom para suavizar la pluma.
/// Cotas defensivas: `subdivisions` se capa a 16 y trazos >4096 puntos se ignoran
/// para evitar DoS por sobre-amplificación (4096*16 ≈ 65k puntos por trazo).
pub fn smooth_stroke(points: &[(f64, f64)], subdivisions: usize) -> Vec<(f64, f64)> {
    let subdivisions = subdivisions.min(16);
    if points.len() > 4096 {
        return Vec::new();
    }
    let mut out = Vec::new();
    if points.is_empty() {
        return out;
    }
    if points.len() < 2 {
        out.push(points[0]);
        return out;
    }
    for index in 0..points.len() - 1 {
        let p0 = points
            .get(index.saturating_sub(1))
            .copied()
            .unwrap_or(points[0]);
        let p1 = points[index];
        let p2 = points[index + 1];
        let p3 = points.get(index + 2).copied().unwrap_or(p2);
        for step in 0..=subdivisions {
            let t = step as f64 / (subdivisions as f64).max(1.0);
            out.push(catmull_rom(p0, p1, p2, p3, t));
        }
    }
    out
}

fn catmull_rom(
    p0: (f64, f64),
    p1: (f64, f64),
    p2: (f64, f64),
    p3: (f64, f64),
    t: f64,
) -> (f64, f64) {
    let t2 = t * t;
    let t3 = t2 * t;
    let x = 0.5
        * ((2.0 * p1.0)
            + (-p0.0 + p2.0) * t
            + (2.0 * p0.0 - 5.0 * p1.0 + 4.0 * p2.0 - p3.0) * t2
            + (-p0.0 + 3.0 * p1.0 - 3.0 * p2.0 + p3.0) * t3);
    let y = 0.5
        * ((2.0 * p1.1)
            + (-p0.1 + p2.1) * t
            + (2.0 * p0.1 - 5.0 * p1.1 + 4.0 * p2.1 - p3.1) * t2
            + (-p0.1 + 3.0 * p1.1 - 3.0 * p2.1 + p3.1) * t3);
    (x, y)
}

/// Puntas de la flecha a 30° respecto al eje, para dibujar el marcador.
pub fn arrow_tip(from: (f64, f64), to: (f64, f64), head_len: f64) -> ((f64, f64), (f64, f64)) {
    let dx = to.0 - from.0;
    let dy = to.1 - from.1;
    let length = (dx * dx + dy * dy).sqrt().max(1e-9);
    let ux = dx / length;
    let uy = dy / length;
    let angle = std::f64::consts::FRAC_PI_6; // ~30°
    let right = (
        to.0 - head_len * (ux * angle.cos() - uy * angle.sin()),
        to.1 - head_len * (uy * angle.cos() + ux * angle.sin()),
    );
    let left = (
        to.0 - head_len * (ux * angle.cos() + uy * angle.sin()),
        to.1 - head_len * (uy * angle.cos() - ux * angle.sin()),
    );
    (right, left)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn doc_add_erase_select_and_clear_work() {
        let mut doc = WhiteboardDoc::new();
        assert!(doc.is_empty());
        doc.add(WhiteboardElement::Ellipse {
            center: (0.0, 0.0),
            rx: 2.0,
            ry: 2.0,
        });
        doc.add(WhiteboardElement::Rectangle {
            min: (5.0, 5.0),
            max: (8.0, 9.0),
            fill: None,
        });
        assert_eq!(doc.len(), 2);
        assert_eq!(doc.select_at((0.5, 0.0), 1.0), Some(0));
        assert_eq!(doc.erase_at((6.5, 7.0), 0.5), Some(1));
        assert_eq!(doc.len(), 1);
        doc.clear();
        assert!(doc.is_empty());
    }

    #[test]
    fn bounds_are_finite_and_contain_points() {
        let element = WhiteboardElement::Stroke {
            points: vec![(-1.0, -1.0), (2.0, 3.0), (5.0, -2.0)],
            color: (0, 0, 0),
            width: 2.0,
        };
        let (min, max) = element.bounds().unwrap();
        assert_eq!(min, (-1.0, -2.0));
        assert_eq!(max, (5.0, 3.0));
    }

    #[test]
    fn smooth_stroke_preserves_endpoints_and_densifies() {
        let raw = vec![(0.0, 0.0), (1.0, 1.0), (2.0, 0.0)];
        let smooth = smooth_stroke(&raw, 3);
        assert!(smooth.len() > raw.len());
        assert_eq!(smooth.first(), Some(&(0.0, 0.0)));
        assert_eq!(smooth.last(), Some(&(2.0, 0.0)));
    }

    #[test]
    fn arrow_tip_places_two_wing_points_backward() {
        let from = (0.0, 0.0);
        let to = (10.0, 0.0);
        let (right, left) = arrow_tip(from, to, 2.0);
        assert!(right.0 < to.0, "wings go backward along the arrow");
        assert!(left.0 < to.0);
        assert!((right.1 - left.1).abs() > 0.01, "wings are symmetric");
    }
}
