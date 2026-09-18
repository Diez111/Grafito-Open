//! Lectores de flags de vista `__view_*` (Ola 0.5).
//!
//! Los comandos ShowAxes/ShowGrid/AxisStepX/AxisStepY/SetBackgroundColor
//! guardan variables `__view_*` en el documento; estos lectores les dan efecto
//! en el lienzo 2D (`render_2d`). Sin flag presente, vale el estado de la app
//! (compatibilidad total hacia atrás).
//!
//! Las variables `__view_*` no se muestran en el panel de álgebra (filtro en
//! `algebra.rs`, junto a `is_internal_trig_name`): son estado de vista, no
//! variables del usuario.

use egui::Color32;
use grafito_core::Document;

/// Mostrar/ocultar ejes (`ShowAxes[bool]` → 1.0/0.0).
pub(crate) const VIEW_SHOW_AXES: &str = "__view_show_axes";
/// Mostrar/ocultar grilla (`ShowGrid[bool]` → 1.0/0.0).
pub(crate) const VIEW_SHOW_GRID: &str = "__view_show_grid";
/// Paso explícito de grilla en X (`AxisStepX[paso]`).
pub(crate) const VIEW_AXIS_STEP_X: &str = "__view_axis_step_x";
/// Paso explícito de grilla en Y (`AxisStepY[paso]`).
pub(crate) const VIEW_AXIS_STEP_Y: &str = "__view_axis_step_y";
/// Fondo del lienzo empacado r·65536+g·256+b (`SetBackgroundColor[color]`).
pub(crate) const VIEW_BG: &str = "__view_bg";

/// Valor finito de un flag, o `None` si no existe o no es finito.
fn flag_value(document: &Document, name: &str) -> Option<f64> {
    let value = *document.variables.get(name)?;
    value.is_finite().then_some(value)
}

/// Booleano 1.0/0.0 con default cuando no hay flag (0.0 = false, resto = true).
pub(crate) fn view_flag_bool(document: &Document, name: &str, default: bool) -> bool {
    flag_value(document, name)
        .map(|value| value != 0.0)
        .unwrap_or(default)
}

/// Paso de grilla explícito: solo valores finitos > 0.
pub(crate) fn view_axis_step(document: &Document, name: &str) -> Option<f64> {
    flag_value(document, name).filter(|value| *value > 0.0)
}

/// Color de fondo desempaquetado, o `None` si no hay flag válido.
pub(crate) fn view_bg_color(document: &Document) -> Option<Color32> {
    let packed = flag_value(document, VIEW_BG)? as u32;
    if packed > 0xFF_FFFF {
        return None;
    }
    let red = ((packed >> 16) & 0xFF) as u8;
    let green = ((packed >> 8) & 0xFF) as u8;
    let blue = (packed & 0xFF) as u8;
    Some(Color32::from_rgb(red, green, blue))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn doc_with_vars(vars: &[(&str, f64)]) -> Document {
        let mut document = Document::new();
        for (name, value) in vars {
            document
                .try_set_variable((*name).to_string(), *value)
                .expect("variable de prueba");
        }
        document
    }

    #[test]
    fn bool_flag_defaults_without_variable() {
        let document = Document::new();
        assert!(view_flag_bool(&document, VIEW_SHOW_AXES, true));
        assert!(!view_flag_bool(&document, VIEW_SHOW_GRID, false));
    }

    #[test]
    fn bool_flag_reads_unit_values() {
        let document = doc_with_vars(&[(VIEW_SHOW_GRID, 1.0), (VIEW_SHOW_AXES, 0.0)]);
        assert!(view_flag_bool(&document, VIEW_SHOW_GRID, false));
        assert!(!view_flag_bool(&document, VIEW_SHOW_AXES, true));
    }

    #[test]
    fn non_finite_flag_falls_back_to_default() {
        // try_set_variable rechaza no-finitos; esto simula un documento
        // hostil/legacy con inserción directa (el mapa es público).
        let mut document = Document::new();
        document
            .variables
            .insert(VIEW_SHOW_GRID.to_string(), f64::INFINITY);
        assert!(view_flag_bool(&document, VIEW_SHOW_GRID, true));
        assert_eq!(view_axis_step(&document, VIEW_AXIS_STEP_X), None);
        assert_eq!(view_bg_color(&document), None);
    }

    #[test]
    fn axis_step_accepts_only_positive_finite() {
        let document = doc_with_vars(&[
            (VIEW_AXIS_STEP_X, 0.5),
            (VIEW_AXIS_STEP_Y, -2.0),
            ("__view_axis_step_z", 0.0),
        ]);
        assert_eq!(view_axis_step(&document, VIEW_AXIS_STEP_X), Some(0.5));
        assert_eq!(view_axis_step(&document, VIEW_AXIS_STEP_Y), None);
        assert_eq!(view_axis_step(&document, "__view_axis_step_z"), None);
    }

    #[test]
    fn bg_color_roundtrips_packed_rgb() {
        // Empaque de commands.rs (SetBackgroundColor, rama vista).
        let pack = |r: f64, g: f64, b: f64| r * 65536.0 + g * 256.0 + b;
        let document = doc_with_vars(&[(VIEW_BG, pack(18.0, 52.0, 86.0))]);
        assert_eq!(
            view_bg_color(&document),
            Some(Color32::from_rgb(18, 52, 86))
        );
        let out_of_range = doc_with_vars(&[(VIEW_BG, 0x1FF_FFFF as f64)]);
        assert_eq!(view_bg_color(&out_of_range), None);
    }

    /// Integración comando → lector: los comandos escriben `__view_*` y los
    /// lectores los interpretan (incluye precedencia sobre el estado de app).
    #[test]
    fn commands_drive_readers() {
        use crate::commands::process_input;

        let mut document = Document::new();
        // Sin flags: defaults intactos.
        assert!(view_flag_bool(&document, VIEW_SHOW_AXES, true));
        assert!(view_flag_bool(&document, VIEW_SHOW_GRID, true));
        assert_eq!(view_axis_step(&document, VIEW_AXIS_STEP_X), None);
        assert_eq!(view_bg_color(&document), None);

        for command in [
            "ShowGrid[false]",
            "ShowAxes[false]",
            "AxisStepX[0.5]",
            "AxisStepY[0.25]",
            "SetBackgroundColor[red]",
        ] {
            let mut input = command.to_string();
            assert!(
                matches!(
                    process_input(&mut document, &mut input),
                    grafito_command::commands::CommandOutcome::Message(_)
                ),
                "{command} debe tener éxito"
            );
        }
        // El flag gana al default de la app en ambas direcciones.
        assert!(!view_flag_bool(&document, VIEW_SHOW_GRID, true));
        assert!(!view_flag_bool(&document, VIEW_SHOW_AXES, true));
        assert_eq!(view_axis_step(&document, VIEW_AXIS_STEP_X), Some(0.5));
        assert_eq!(view_axis_step(&document, VIEW_AXIS_STEP_Y), Some(0.25));
        // "red" nombrado es (230, 51, 51): el lector hace round-trip exacto
        // de lo que guarda el comando.
        assert_eq!(
            view_bg_color(&document),
            Some(Color32::from_rgb(230, 51, 51))
        );

        // ShowGrid[true] revierte sin borrar el default de la app.
        let mut input = "ShowGrid[true]".to_string();
        let _ = process_input(&mut document, &mut input);
        assert!(view_flag_bool(&document, VIEW_SHOW_GRID, false));
    }
}
