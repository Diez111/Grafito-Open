//! Máquina de estado del arrastre sobre la pizarra y selección por marco.

use crate::{WhiteboardDoc, WhiteboardElement};

/// Herramienta de pizarra activa.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WhiteboardTool {
    Select,
    Pencil,
    Rectangle,
    Ellipse,
    Arrow,
    Text,
    Eraser,
}

/// Estado del arrastre del puntero.
#[derive(Debug, Clone, PartialEq, Default)]
pub enum WhiteboardInteraction {
    #[default]
    Idle,
    /// Creando una forma desde `from` hasta `current`.
    Creating {
        from: (f64, f64),
        current: (f64, f64),
        tool: WhiteboardTool,
    },
    /// Borrando objetos bajo un trazo.
    Erasing { path: Vec<(f64, f64)> },
}

impl WhiteboardInteraction {
    pub fn begin(from: (f64, f64), tool: WhiteboardTool) -> Self {
        match tool {
            WhiteboardTool::Eraser => Self::Erasing { path: vec![from] },
            other => Self::Creating {
                from,
                current: from,
                tool: other,
            },
        }
    }

    pub fn update(&mut self, point: (f64, f64)) {
        match self {
            Self::Creating { current, .. } => *current = point,
            Self::Erasing { path } => path.push(point),
            Self::Idle => {}
        }
    }

    pub fn preview(&self) -> Option<WhiteboardElement> {
        let Self::Creating {
            from,
            current,
            tool,
        } = self
        else {
            return None;
        };
        make_element(*tool, *from, *current)
    }

    pub fn end(&self) -> Option<WhiteboardElement> {
        self.preview()
    }

    /// Devuelve el trazo de borrado y lo consume.
    pub fn take_erase_path(&mut self) -> Option<Vec<(f64, f64)>> {
        match self {
            Self::Erasing { path } if path.len() >= 2 => {
                let path = std::mem::take(path);
                *self = Self::Erasing { path: Vec::new() };
                Some(path)
            }
            _ => None,
        }
    }
}

/// Construye el elemento a partir de dos esquinas del arrastre.
pub fn make_element(
    tool: WhiteboardTool,
    from: (f64, f64),
    to: (f64, f64),
) -> Option<WhiteboardElement> {
    // Guard contra coordenadas no finitas (evita crash al tocar “Pizarra” con drag NaN)
    if !from.0.is_finite() || !from.1.is_finite() || !to.0.is_finite() || !to.1.is_finite() {
        return None;
    }
    let min_drag = 1e-6;
    if (from.0 - to.0).abs() < min_drag && (from.1 - to.1).abs() < min_drag {
        return None;
    }
    match tool {
        WhiteboardTool::Rectangle => {
            let min = (from.0.min(to.0), from.1.min(to.1));
            let max = (from.0.max(to.0), from.1.max(to.1));
            Some(WhiteboardElement::Rectangle {
                min,
                max,
                fill: None,
            })
        }
        WhiteboardTool::Ellipse => {
            let width = (from.0 - to.0).abs();
            let height = (from.1 - to.1).abs();
            Some(WhiteboardElement::Ellipse {
                center: ((from.0 + to.0) / 2.0, (from.1 + to.1) / 2.0),
                rx: width / 2.0,
                ry: height / 2.0,
            })
        }
        WhiteboardTool::Arrow => Some(WhiteboardElement::Arrow { from, to }),
        WhiteboardTool::Text => Some(WhiteboardElement::Text {
            at: to,
            text: String::new(),
            size: 14.0,
        }),
        WhiteboardTool::Pencil | WhiteboardTool::Select | WhiteboardTool::Eraser => None,
    }
}

/// Selecciona por marco (intersección de cajas).
pub fn select_in_marquee(
    doc: &WhiteboardDoc,
    rect_min: (f64, f64),
    rect_max: (f64, f64),
) -> Vec<usize> {
    let (rmin, rmax) = (
        (rect_min.0.min(rect_max.0), rect_min.1.min(rect_max.1)),
        (rect_min.0.max(rect_max.0), rect_min.1.max(rect_max.1)),
    );
    doc.elements()
        .iter()
        .enumerate()
        .filter_map(|(index, element)| {
            let (min, max) = element.bounds()?;
            let overlaps = min.0 <= rmax.0 && max.0 >= rmin.0 && min.1 <= rmax.1 && max.1 >= rmin.1;
            overlaps.then_some(index)
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rectangle_is_normalized_even_when_dragged_reversed() {
        let rectangle = make_element(WhiteboardTool::Rectangle, (8.0, 8.0), (2.0, 2.0)).unwrap();
        let (min, max) = rectangle.bounds().unwrap();
        assert_eq!(min, (2.0, 2.0));
        assert_eq!(max, (8.0, 8.0));
    }

    #[test]
    fn interaction_creates_previews_and_ending_discards_tiny_drags() {
        let mut interaction = WhiteboardInteraction::begin((0.0, 0.0), WhiteboardTool::Ellipse);
        interaction.update((4.0, 2.0));
        assert!(interaction.preview().is_some());
        let tiny = WhiteboardInteraction::begin((1.0, 1.0), WhiteboardTool::Arrow);
        assert!(tiny.end().is_none());
    }

    #[test]
    fn eraser_interaction_accumulates_a_path() {
        let mut eraser = WhiteboardInteraction::begin((0.0, 0.0), WhiteboardTool::Eraser);
        eraser.update((1.0, 0.0));
        assert!(eraser.take_erase_path().is_some());
    }

    #[test]
    fn marquee_selects_only_overlapping_elements() {
        let mut doc = WhiteboardDoc::new();
        doc.add(WhiteboardElement::Rectangle {
            min: (0.0, 0.0),
            max: (2.0, 2.0),
            fill: None,
        });
        doc.add(WhiteboardElement::Rectangle {
            min: (10.0, 10.0),
            max: (12.0, 12.0),
            fill: None,
        });
        let selected = select_in_marquee(&doc, (-1.0, -1.0), (3.0, 3.0));
        assert_eq!(selected, vec![0]);
    }
}
