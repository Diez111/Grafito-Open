//! Lectura de traza al hover — qué valor hay bajo el puntero.
//!
//! Piel pura: `fn render(&Estado) -> Frame`. El llamador convierte la
//! posición del puntero a coordenadas matemáticas (él conoce la vista);
//! acá sólo se valida, formatea y dibuja. Sin I/O, sin spawn, sólo tokens.

use crate::tokens::{RADIUS_SM, SPACE_SM, SPACE_XS, TYPE_SM, TYPE_XS};

/// Punto de traza bajo el puntero, ya en coordenadas matemáticas.
#[derive(Debug, Clone, PartialEq)]
pub struct TraceReadout {
    /// Abscisa matemática.
    pub x: f64,
    /// Ordenada matemática.
    pub y: f64,
    /// Etiqueta de la traza (ej. "f", "A"). Vacía = anónima honesta.
    pub label: String,
}

/// Construye una lectura validando finitud. `None` = nada que mostrar
/// (coordenada no finita o puntero fuera de la traza).
pub fn trace_from_hover(x: f64, y: f64, label: &str) -> Option<TraceReadout> {
    if !x.is_finite() || !y.is_finite() {
        return None;
    }
    // Etiqueta acotada por caracteres (nunca `split_at` por bytes: pánico
    // con multibyte). 48 caracteres bastan para "f", "recta AB", etc.
    let short: String = label.trim().chars().take(48).collect();
    Some(TraceReadout { x, y, label: short })
}

/// Texto honesto de la lectura. 6 decimales como máximo, sin notación
/// inventada: si el valor no es representable, se dice.
pub fn format_trace_readout(readout: &TraceReadout) -> String {
    fn num(value: f64) -> String {
        if value == 0.0 {
            return "0".to_string();
        }
        let abs = value.abs();
        if (1e-3..1e6).contains(&abs) {
            format!("{value:.4}")
                .trim_end_matches('0')
                .trim_end_matches('.')
                .to_owned()
        } else {
            format!("{value:.3e}")
        }
    }
    let point = format!("({}, {})", num(readout.x), num(readout.y));
    if readout.label.is_empty() {
        point
    } else {
        format!("{} {}", readout.label, point)
    }
}

/// Chip de lectura bajo el puntero. Con `None` muestra el vacío honesto
/// (y explica por qué no hay lectura en vez de un botón mudo).
pub fn draw_trace_readout(ui: &mut egui::Ui, readout: Option<&TraceReadout>) {
    ui.add_space(SPACE_XS);
    match readout {
        Some(point) => {
            let text = format_trace_readout(point);
            egui::Frame::none()
                .fill(ui.visuals().extreme_bg_color)
                .rounding(RADIUS_SM)
                .inner_margin(SPACE_SM)
                .show(ui, |ui| {
                    ui.label(egui::RichText::new(text).size(TYPE_SM).strong().monospace());
                })
                .response
                .on_hover_text("Valor de la traza bajo el puntero");
        }
        None => {
            ui.label(
                egui::RichText::new("Acercá el puntero a una traza para leer su valor.")
                    .size(TYPE_XS)
                    .weak(),
            );
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rejects_non_finite() {
        assert!(trace_from_hover(f64::NAN, 0.0, "f").is_none());
        assert!(trace_from_hover(0.0, f64::INFINITY, "f").is_none());
        assert!(trace_from_hover(1.0, 2.0, "f").is_some());
    }

    #[test]
    fn label_is_trimmed_and_bounded() {
        let long = "x".repeat(200);
        let readout = trace_from_hover(1.0, 2.0, &long).expect("finito");
        assert!(readout.label.chars().count() <= 48);
        // Multibyte sin pánico: 200 emojis → 48 caracteres.
        let wide = "π".repeat(200);
        let readout = trace_from_hover(1.0, 2.0, &wide).expect("finito");
        assert_eq!(readout.label.chars().count(), 48);
        let blank = trace_from_hover(1.0, 2.0, "   ").expect("finito");
        assert!(blank.label.is_empty());
    }

    #[test]
    fn format_is_honest() {
        let plain = TraceReadout {
            x: 1.5,
            y: -2.0,
            label: "f".to_string(),
        };
        assert_eq!(format_trace_readout(&plain), "f (1.5, -2)");
        let anon = TraceReadout {
            x: 0.0,
            y: 0.0,
            label: String::new(),
        };
        assert_eq!(format_trace_readout(&anon), "(0, 0)");
        let tiny = TraceReadout {
            x: 1e-9,
            y: 2e9,
            label: String::new(),
        };
        let text = format_trace_readout(&tiny);
        assert!(
            text.contains('e'),
            "notación científica honesta, fue: {text}"
        );
    }
}
