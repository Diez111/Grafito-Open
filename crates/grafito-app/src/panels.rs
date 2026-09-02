//! Paneles laterales removibles e inspectores (CAS, vista, estadística, propiedades).

use crate::GrafitoApp;
use egui::Color32;
use grafito_core::{
    CasWorksheetStatus, ChangeSet, DataTableObj, Document, GeoObject, ObjectId,
    RegularPolytopeNDObj, ScatterPlotObj,
};
use grafito_geometry::{Color, RegularPolychoron, RegularPolytopeFamily};
use grafito_ui::icons::{action_icon_button, Icon};
use grafito_ui::theme::{current_theme, DARK, LIGHT};
use grafito_ui::tokens::{
    CARD_SPACING, PANEL_LEFT_DEFAULT, PANEL_LEFT_MAX_FRACTION, PANEL_LEFT_MIN, RADIUS_LG,
    RADIUS_MD, RADIUS_PILL, SPACE_LG, SPACE_MD, SPACE_SM, SPACE_XS, TYPE_BASE, TYPE_LG, TYPE_MD,
    TYPE_SM, TYPE_XS, ZOOM_ICON_HIT,
};
use std::collections::VecDeque;
use std::fs::File;
use std::io::Read;
use std::path::Path;

const MAX_LOCAL_DATA_IMPORT_BYTES: usize = 2_000_000;

#[derive(Debug, Clone, PartialEq)]
pub(crate) struct LocalXYTable {
    pub x_name: String,
    pub y_name: String,
    pub xs: Vec<f64>,
    pub ys: Vec<f64>,
}

#[allow(dead_code)] // TODO P2: Panel de estadística sin entrada en la UI (reactivar).
pub(crate) fn parse_statistics_input(input: &str) -> Result<Vec<f64>, String> {
    let input = input.trim();
    if input.is_empty() {
        return Ok(Vec::new());
    }

    input
        .split([',', '\n'])
        .enumerate()
        .map(|(index, token)| {
            let token = token.trim();
            if token.is_empty() {
                return Err(format!("Dato {}: falta un valor", index + 1));
            }
            let value = token
                .parse::<f64>()
                .map_err(|_| format!("Dato {}: '{token}' no es un número válido", index + 1))?;
            if !value.is_finite() {
                return Err(format!("Dato {}: el valor debe ser finito", index + 1));
            }
            Ok(value)
        })
        .collect()
}

/// Parses an explicitly selected local two-column CSV/TSV payload. The result
/// deliberately carries no file path, timestamps, or other source metadata.
pub(crate) fn parse_local_xy_table(input: &str, delimiter: u8) -> Result<LocalXYTable, String> {
    let delimiter = char::from(delimiter);
    let rows = input
        .lines()
        .enumerate()
        .filter_map(|(index, line)| (!line.trim().is_empty()).then_some((index + 1, line)))
        .map(|(line_number, line)| {
            parse_delimited_row(line, delimiter)
                .map_err(|error| format!("Fila {line_number}: {error}"))
        })
        .collect::<Result<Vec<_>, _>>()?;
    if rows.is_empty() {
        return Err("El archivo no contiene filas de datos".to_string());
    }

    let mut rows: VecDeque<_> = rows.into();
    let Some(first) = rows.pop_front() else {
        return Err("El archivo no contiene filas de datos".to_string());
    };
    if first.len() != 2 {
        return Err("Cada fila debe tener exactamente dos columnas".to_string());
    }
    let first_values = parse_local_xy_values(&first);
    let (x_name, y_name, mut xs, mut ys) = match first_values {
        Ok((x, y)) => ("x".to_string(), "y".to_string(), vec![x], vec![y]),
        Err(error) => {
            if first.iter().any(|cell| cell.parse::<f64>().is_ok()) {
                return Err(format!("Fila 1: {error}"));
            }
            if first[0].is_empty() || first[1].is_empty() {
                return Err("Los encabezados de columna no pueden estar vacíos".to_string());
            }
            (first[0].clone(), first[1].clone(), Vec::new(), Vec::new())
        }
    };

    for (index, row) in rows.into_iter().enumerate() {
        if row.len() != 2 {
            return Err(format!(
                "Fila {}: se esperaban exactamente dos columnas",
                index + 2
            ));
        }
        let (x, y) =
            parse_local_xy_values(&row).map_err(|error| format!("Fila {}: {error}", index + 2))?;
        xs.push(x);
        ys.push(y);
        if xs.len() > grafito_core::validation::MAX_DATA_TABLE_ROWS {
            return Err(format!(
                "El archivo supera el máximo de {} filas",
                grafito_core::validation::MAX_DATA_TABLE_ROWS
            ));
        }
    }
    if xs.len() < 2 {
        return Err("Se necesitan al menos dos pares numéricos finitos".to_string());
    }

    Ok(LocalXYTable {
        x_name,
        y_name,
        xs,
        ys,
    })
}

fn parse_delimited_row(line: &str, delimiter: char) -> Result<Vec<String>, String> {
    let mut cells = Vec::new();
    let mut current = String::new();
    let mut chars = line.chars().peekable();
    let mut in_quotes = false;

    while let Some(character) = chars.next() {
        if in_quotes {
            if character == '"' {
                if chars.peek() == Some(&'"') {
                    current.push('"');
                    let _ = chars.next();
                } else {
                    in_quotes = false;
                }
            } else {
                current.push(character);
            }
        } else if character == '"' {
            if !current.trim().is_empty() {
                return Err("las comillas deben iniciar una celda".to_string());
            }
            current.clear();
            in_quotes = true;
        } else if character == delimiter {
            cells.push(current.trim().to_string());
            current.clear();
        } else {
            current.push(character);
        }
    }
    if in_quotes {
        return Err("comillas sin cerrar".to_string());
    }
    cells.push(current.trim().to_string());
    if let Some(first) = cells.first_mut() {
        *first = first.trim_start_matches('\u{feff}').to_string();
    }
    Ok(cells)
}

fn parse_local_xy_values(row: &[String]) -> Result<(f64, f64), String> {
    if row.len() != 2 {
        return Err("se esperaban exactamente dos columnas".to_string());
    }
    let x = row[0]
        .parse::<f64>()
        .map_err(|_| format!("'{}' no es un número válido", row[0]))?;
    let y = row[1]
        .parse::<f64>()
        .map_err(|_| format!("'{}' no es un número válido", row[1]))?;
    if !x.is_finite() || !y.is_finite() {
        return Err("los valores deben ser finitos".to_string());
    }
    Ok((x, y))
}

fn load_local_xy_table(path: &Path) -> Result<LocalXYTable, String> {
    // TODO: mover a background thread con std::thread::spawn + ctx.request_repaint()
    // para no bloquear UI en archivos cercanos a 2MB. Lectura limitada a
    // MAX_LOCAL_DATA_IMPORT_BYTES (2MB) via take() y sin unwrap/panic.
    let metadata = std::fs::symlink_metadata(path)
        .map_err(|error| format!("No se pudo inspeccionar el archivo: {error}"))?;
    if !metadata.file_type().is_file() {
        return Err("La fuente seleccionada debe ser un archivo regular".to_string());
    }

    let mut bytes = Vec::new();
    let file = open_local_data_file(path)?;
    if !file
        .metadata()
        .map_err(|error| format!("No se pudo verificar el archivo abierto: {error}"))?
        .file_type()
        .is_file()
    {
        return Err("La fuente seleccionada debe ser un archivo regular".to_string());
    }
    // I/O limitado a 2MB para no bloquear UI; budgeting verificado via AttachmentLimits
    file.take(MAX_LOCAL_DATA_IMPORT_BYTES as u64 + 1)
        .read_to_end(&mut bytes)
        .map_err(|error| format!("No se pudo leer el archivo: {error}"))?;
    if bytes.len() > MAX_LOCAL_DATA_IMPORT_BYTES {
        return Err(format!(
            "El archivo supera el máximo de {MAX_LOCAL_DATA_IMPORT_BYTES} bytes"
        ));
    }
    let text = String::from_utf8(bytes)
        .map_err(|_| "El archivo debe estar codificado como UTF-8".to_string())?;
    let is_tsv = path
        .extension()
        .and_then(|extension| extension.to_str())
        .is_some_and(|extension| extension.eq_ignore_ascii_case("tsv"));
    parse_local_xy_table(&text, if is_tsv { b'\t' } else { b',' })
}

fn open_local_data_file(path: &Path) -> Result<File, String> {
    #[cfg(unix)]
    {
        use std::fs::OpenOptions;
        use std::os::unix::fs::OpenOptionsExt;

        // Do not follow a path swapped to a symlink, and never block the UI
        // thread on a FIFO substituted after the native file selection.
        OpenOptions::new()
            .read(true)
            .custom_flags(libc::O_NOFOLLOW | libc::O_NONBLOCK)
            .open(path)
            .map_err(|error| format!("No se pudo abrir el archivo: {error}"))
    }
    #[cfg(not(unix))]
    {
        File::open(path).map_err(|error| format!("No se pudo abrir el archivo: {error}"))
    }
}

fn commit_local_xy_table(
    document: &mut Document,
    undo_stack: &mut VecDeque<Document>,
    redo_stack: &mut VecDeque<ChangeSet>,
    table: LocalXYTable,
) -> Result<ObjectId, String> {
    let data_table = DataTableObj::new(
        table.x_name,
        table.y_name,
        table.xs.clone(),
        table.ys.clone(),
    );
    let data_id = data_table.id;
    let scatter = ScatterPlotObj::new(table.xs, table.ys).linked_to(data_id);
    crate::app::commit_object_insertions(
        document,
        undo_stack,
        redo_stack,
        vec![
            GeoObject::DataTable(data_table),
            GeoObject::ScatterPlot(scatter),
        ],
    )?;
    Ok(data_id)
}

fn import_local_xy_table(app: &mut GrafitoApp) {
    // TODO: mover a background thread (std::thread::spawn + canal + poll con ctx.request_repaint())
    // ideal para UI 60fps; por ahora lectura limitada a 2MB sin panic.
    let Some(path) = rfd::FileDialog::new()
        .add_filter("Datos CSV o TSV", &["csv", "tsv", "txt"])
        .pick_file()
    else {
        return;
    };
    let table = match load_local_xy_table(&path) {
        Ok(table) => table,
        Err(error) => {
            app.cas_result = format!("No se pudo importar la tabla: {error}");
            app.notify(app.cas_result.clone(), grafito_ui::toast::ToastKind::Error);
            return;
        }
    };

    let row_count = table.xs.len();
    match commit_local_xy_table(
        &mut app.document,
        &mut app.undo_stack,
        &mut app.redo_stack,
        table,
    ) {
        Ok(data_id) => {
            let label = app
                .document
                .get_object(data_id)
                .map(|object| object.label().to_string())
                .unwrap_or_else(|| "tabla".to_string());
            app.cas_result = format!(
                "Tabla local '{label}' importada con {row_count} pares; la ruta no se guardó."
            );
            app.notify(
                app.cas_result.clone(),
                grafito_ui::toast::ToastKind::Success,
            );
        }
        Err(error) => {
            app.cas_result = format!("No se pudo guardar la tabla local: {error}");
            app.notify(app.cas_result.clone(), grafito_ui::toast::ToastKind::Error);
        }
    }
}

#[cfg(test)]
mod local_data_import_tests {
    use super::{
        commit_local_xy_table, load_local_xy_table, parse_local_xy_table, LocalXYTable,
        MAX_LOCAL_DATA_IMPORT_BYTES,
    };
    use grafito_core::{Document, GeoObject};
    use std::collections::VecDeque;
    use std::sync::atomic::{AtomicU64, Ordering};

    static NEXT_IMPORT_TEST_FILE: AtomicU64 = AtomicU64::new(0);

    #[test]
    fn csv_and_tsv_imports_accept_optional_headers_without_retaining_a_path() {
        let csv = parse_local_xy_table("\"time\",\"distance\"\n0,1\n1,3\n2,5\n", b',')
            .expect("CSV data with a header should parse");
        assert_eq!(csv.x_name, "time");
        assert_eq!(csv.y_name, "distance");
        assert_eq!(csv.xs, vec![0.0, 1.0, 2.0]);
        assert_eq!(csv.ys, vec![1.0, 3.0, 5.0]);

        let tsv = parse_local_xy_table("0\t1\n1\t3\n", b'\t')
            .expect("TSV data without a header should parse");
        assert_eq!(tsv.x_name, "x");
        assert_eq!(tsv.y_name, "y");
        assert_eq!(tsv.xs, vec![0.0, 1.0]);
        assert_eq!(tsv.ys, vec![1.0, 3.0]);
    }

    #[test]
    fn local_data_import_rejects_malformed_or_non_finite_rows() {
        for input in ["x,y\n0,1\n1\n", "x,y\n0,NaN\n", "x,y\n0,1\n"] {
            let error = parse_local_xy_table(input, b',')
                .expect_err("invalid local input must be rejected before mutation");
            assert!(!error.is_empty());
        }
    }

    #[test]
    fn local_data_import_enforces_bounds_and_commits_table_and_scatter_once() {
        let path = std::env::temp_dir().join(format!(
            "grafito-local-data-{}-{}.csv",
            std::process::id(),
            NEXT_IMPORT_TEST_FILE.fetch_add(1, Ordering::Relaxed)
        ));
        std::fs::write(&path, vec![b'x'; MAX_LOCAL_DATA_IMPORT_BYTES + 1])
            .expect("oversized fixture writes");
        let error = load_local_xy_table(&path).expect_err("oversized file must be rejected");
        let _ = std::fs::remove_file(&path);
        assert!(error.contains("máximo"));

        let mut too_many_rows = String::from("x,y\n");
        for index in 0..=grafito_core::validation::MAX_DATA_TABLE_ROWS {
            too_many_rows.push_str(&format!("{index},{}\n", index + 1));
        }
        assert!(parse_local_xy_table(&too_many_rows, b',').is_err());

        let mut document = Document::new();
        let mut undo_stack = VecDeque::new();
        let mut redo_stack = VecDeque::new();
        let data_id = commit_local_xy_table(
            &mut document,
            &mut undo_stack,
            &mut redo_stack,
            LocalXYTable {
                x_name: "time".to_string(),
                y_name: "distance".to_string(),
                xs: vec![0.0, 1.0, 2.0],
                ys: vec![1.0, 3.0, 5.0],
            },
        )
        .expect("valid local data commits");

        assert_eq!(undo_stack.len(), 1);
        assert!(redo_stack.is_empty());
        assert!(matches!(
            document.get_object(data_id),
            Some(GeoObject::DataTable(_))
        ));
        assert!(document.objects_iter().any(|(_, object)| {
            matches!(object, GeoObject::ScatterPlot(scatter) if scatter.source_data == Some(data_id))
        }));
    }
}

#[derive(Debug, Clone, PartialEq)]
#[allow(dead_code)] // TODO P2: Panel de estadística sin entrada en la UI.
pub(crate) struct StatisticsSummary {
    pub sum: Option<f64>,
    pub mean: f64,
    pub median: f64,
    pub variance: f64,
    pub standard_deviation: f64,
    pub minimum: f64,
    pub maximum: f64,
    pub range: f64,
    pub q1: f64,
    pub q3: f64,
    pub iqr: f64,
}

#[allow(dead_code)] // TODO P2: Solo usado por el panel de estadística (sin UI activa).
fn stable_interpolate(a: f64, b: f64, fraction: f64) -> f64 {
    if fraction <= 0.0 || a == b {
        return a;
    }
    if fraction >= 1.0 {
        return b;
    }

    let delta = b - a;
    if delta.is_finite() {
        fraction.mul_add(delta, a)
    } else {
        // Opposite-sign extremes need the weighted form to avoid overflow.
        (1.0 - fraction).mul_add(a, fraction * b)
    }
}

#[allow(dead_code)] // TODO P2: Solo usado por el panel de estadística (sin UI activa).
pub(crate) fn statistics_summary(data: &[f64]) -> Result<StatisticsSummary, String> {
    if data.is_empty() {
        return Err("Estadística: se requiere al menos un dato".to_string());
    }
    if data.iter().any(|value| !value.is_finite()) {
        return Err("Estadística: todos los datos deben ser finitos".to_string());
    }

    let count = data.len() as f64;
    let scale = data.iter().map(|value| value.abs()).fold(0.0, f64::max);
    let (sum, mean, variance, standard_deviation) = if scale == 0.0 {
        (Some(0.0), 0.0, 0.0, 0.0)
    } else {
        let mut normalized_sum = 0.0;
        let mut compensation = 0.0;
        let mut running_mean = 0.0;
        let mut m2 = 0.0;

        for (index, value) in data.iter().enumerate() {
            let normalized = *value / scale;

            let corrected = normalized - compensation;
            let next_sum = normalized_sum + corrected;
            compensation = (next_sum - normalized_sum) - corrected;
            normalized_sum = next_sum;

            let sample_count = (index + 1) as f64;
            let delta = normalized - running_mean;
            running_mean += delta / sample_count;
            let delta_after = normalized - running_mean;
            m2 += delta * delta_after;
        }

        let mean = (normalized_sum / count) * scale;
        if !mean.is_finite() {
            return Err("Estadística: la media no es representable en f64".to_string());
        }

        let normalized_variance = (m2 / count).clamp(0.0, 1.0);
        let standard_deviation = normalized_variance.sqrt() * scale;
        if !standard_deviation.is_finite() {
            return Err("Estadística: el desvío no es representable en f64".to_string());
        }
        let variance = standard_deviation * standard_deviation;
        if normalized_variance > 0.0 && (!variance.is_finite() || variance == 0.0) {
            return Err(
                "Estadística: la varianza verdadera no es representable en f64".to_string(),
            );
        }

        let unscaled_sum = normalized_sum * scale;
        (
            unscaled_sum.is_finite().then_some(unscaled_sum),
            mean,
            variance,
            standard_deviation,
        )
    };

    let mut sorted = data.to_vec();
    sorted.sort_by(f64::total_cmp);
    let minimum = sorted[0];
    let maximum = sorted[sorted.len() - 1];
    let range = maximum - minimum;
    if !range.is_finite() {
        return Err("Estadística: el rango verdadero no es representable en f64".to_string());
    }

    let quantile = |probability: f64| -> Result<f64, String> {
        let position = probability * (sorted.len() as f64 - 1.0);
        let lower = position.floor() as usize;
        let upper = (lower + 1).min(sorted.len() - 1);
        let value = stable_interpolate(sorted[lower], sorted[upper], position - lower as f64);
        value
            .is_finite()
            .then_some(value)
            .ok_or_else(|| "Estadística: un cuantil no es representable en f64".to_string())
    };

    let median = quantile(0.5)?;
    let q1 = quantile(0.25)?;
    let q3 = quantile(0.75)?;
    let iqr = q3 - q1;
    if !iqr.is_finite() {
        return Err("Estadística: el IQR verdadero no es representable en f64".to_string());
    }

    Ok(StatisticsSummary {
        sum,
        mean,
        median,
        variance,
        standard_deviation,
        minimum,
        maximum,
        range,
        q1,
        q3,
        iqr,
    })
}

fn format_statistic(value: f64) -> String {
    let magnitude = value.abs();
    if magnitude >= 1.0e6 || (magnitude > 0.0 && magnitude < 1.0e-4) {
        format!("{value:.3e}")
    } else {
        format!("{value:.3}")
    }
}

#[cfg(test)]
pub(crate) fn apply_object_panel_edit(
    document: &mut grafito_core::Document,
    id: ObjectId,
    changed: bool,
    edit: impl FnOnce(&mut GeoObject),
) -> Result<bool, String> {
    Ok(apply_object_panel_edit_with_previous(document, id, changed, edit)?.is_some())
}

pub(crate) fn apply_object_panel_edit_with_previous(
    document: &mut grafito_core::Document,
    id: ObjectId,
    changed: bool,
    edit: impl FnOnce(&mut GeoObject),
) -> Result<Option<grafito_core::Document>, String> {
    if !changed {
        return Ok(None);
    }
    let Some(object) = document.get_object(id) else {
        return Ok(None);
    };
    let mut edited = object.clone();
    edit(&mut edited);
    document.try_replace_object_with_previous(id, edited)
}

fn color_picker_swatch(ui: &mut egui::Ui, color: Color, label: &str) -> egui::Response {
    let theme = current_theme(ui.ctx());
    let color = Color32::from_rgba_unmultiplied(
        (color.r * 255.0).clamp(0.0, 255.0) as u8,
        (color.g * 255.0).clamp(0.0, 255.0) as u8,
        (color.b * 255.0).clamp(0.0, 255.0) as u8,
        (color.a * 255.0).clamp(0.0, 255.0) as u8,
    );
    let (rect, response) = ui.allocate_exact_size(
        egui::vec2(
            grafito_ui::tokens::ICON_XL - 4.0,
            grafito_ui::tokens::ICON_LG,
        ),
        egui::Sense::click(),
    );
    response.widget_info(|| egui::WidgetInfo::labeled(egui::WidgetType::Button, true, label));
    let border = if response.hovered() {
        theme.accent
    } else {
        theme.separator
    };
    ui.painter().rect_filled(rect.shrink(3.0), RADIUS_MD, color);
    ui.painter()
        .rect_stroke(rect.shrink(3.0), RADIUS_MD, egui::Stroke::new(1.0, border));
    response.on_hover_text(label)
}

/// Helper de retrocompatibilidad. Devuelve la tupla histórica
/// `(is_dark, accent, alg_fill, _sep_col, txt_col, txt_dim, hdr_col)`
/// usando el Theme activo.
#[allow(clippy::type_complexity)]
fn panel_theme_local(
    ctx: &egui::Context,
) -> (bool, Color32, Color32, Color32, Color32, Color32, Color32) {
    let t = current_theme(ctx);
    let is_dark = t.canvas_bg.r() < 100;
    (
        is_dark,
        t.accent,
        t.panel_bg,
        t.separator,
        t.text_primary,
        t.text_tertiary,
        t.text_secondary,
    )
}

fn draw_right_drawer_header(ui: &mut egui::Ui, app: &mut GrafitoApp, title: &str, accent: Color32) {
    let theme = current_theme(ui.ctx());
    ui.horizontal(|ui| {
        ui.label(
            egui::RichText::new(title)
                .color(accent)
                .size(TYPE_MD)
                .strong(),
        );
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            if action_icon_button(
                ui,
                Icon::Close,
                theme.text_secondary,
                "Cerrar panel contextual",
            )
            .clicked()
            {
                app.right_drawer_open = false;
            }
        });
    });
}

fn draw_inspector_identity(ui: &mut egui::Ui, object_name: &str, label: &str, visible: bool) {
    let theme = current_theme(ui.ctx());
    egui::Frame::none()
        .fill(theme.input_bg)
        .stroke(egui::Stroke::NONE)
        .rounding(egui::Rounding::same(RADIUS_LG))
        .inner_margin(egui::Margin::same(SPACE_SM))
        .show(ui, |ui| {
            ui.label(
                egui::RichText::new("Identidad del objeto")
                    .color(theme.text_tertiary)
                    .size(TYPE_SM)
                    .strong(),
            );
            ui.add_space(SPACE_XS);
            ui.horizontal(|ui| {
                ui.label(
                    egui::RichText::new(object_name)
                        .color(theme.text_primary)
                        .size(TYPE_BASE)
                        .strong(),
                );
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    ui.label(
                        egui::RichText::new(if visible { "Visible" } else { "Oculto" })
                            .color(if visible {
                                theme.success
                            } else {
                                theme.text_tertiary
                            })
                            .size(TYPE_SM),
                    );
                });
            });
            if !label.is_empty() {
                ui.label(
                    egui::RichText::new(label)
                        .color(theme.text_secondary)
                        .size(TYPE_SM),
                );
            }
        });
}

fn draw_inspector_section(
    ui: &mut egui::Ui,
    title: &str,
    description: &str,
    contents: impl FnOnce(&mut egui::Ui),
) {
    let theme = current_theme(ui.ctx());
    egui::Frame::none()
        .fill(theme.panel_bg)
        .stroke(theme.hairline_stroke())
        .rounding(egui::Rounding::same(RADIUS_SM))
        .inner_margin(egui::Margin::symmetric(SPACE_SM, SPACE_SM))
        .show(ui, |ui| {
            ui.set_min_width(ui.available_width());
            ui.style_mut().wrap_mode = Some(egui::TextWrapMode::Wrap);
            ui.label(
                egui::RichText::new(title)
                    .color(theme.text_secondary)
                    .size(TYPE_SM)
                    .strong(),
            );
            if !description.is_empty() {
                ui.add_space(SPACE_XS);
                ui.label(
                    egui::RichText::new(description)
                        .color(theme.text_tertiary)
                        .size(TYPE_XS),
                );
            }
            ui.add_space(SPACE_SM);
            ui.spacing_mut().item_spacing.y = SPACE_SM;
            ui.spacing_mut().interact_size.y = ZOOM_ICON_HIT;
            contents(ui);
        });
}

fn draw_inspector_empty_state(ui: &mut egui::Ui) {
    let theme = current_theme(ui.ctx());
    let height = ui.available_height().max(140.0);
    ui.allocate_ui_with_layout(
        egui::vec2(ui.available_width(), height),
        egui::Layout::top_down(egui::Align::Center),
        |ui| {
            ui.add_space(48.0);
            egui::Frame::none()
                .fill(theme.input_bg)
                .stroke(egui::Stroke::NONE)
                .rounding(egui::Rounding::same(RADIUS_LG))
                .inner_margin(egui::Margin::same(SPACE_MD))
                .show(ui, |ui| {
                    ui.vertical_centered(|ui| {
                        ui.label(
                            egui::RichText::new("Inspector listo")
                                .color(theme.text_primary)
                                .size(TYPE_BASE)
                                .strong(),
                        );
                        ui.add_space(SPACE_XS);
                        ui.label(
                            egui::RichText::new(
                                "Seleccioná un objeto del canvas para ajustar su geometría, apariencia y controles avanzados.",
                            )
                            .color(theme.text_secondary)
                            .size(TYPE_SM),
                        );
                    });
                });
        },
    );
}

fn draw_multidimensional_motion_card(
    ui: &mut egui::Ui,
    app: &mut GrafitoApp,
    title: &str,
    description: &str,
    can_animate: bool,
) {
    let theme = current_theme(ui.ctx());
    let mut is_moving = can_animate && app.multidimensional_motion_enabled;
    let card_fill = if is_moving {
        theme.accent_muted
    } else {
        theme.input_bg
    };
    let card_stroke = if is_moving {
        theme.accent
    } else {
        theme.separator
    };

    egui::Frame::none()
        .fill(card_fill)
        .stroke(egui::Stroke::new(1.0, card_stroke))
        .rounding(egui::Rounding::same(RADIUS_LG))
        .inner_margin(egui::Margin::same(SPACE_MD))
        .show(ui, |ui| {
            ui.vertical_centered(|ui| {
                ui.label(
                    egui::RichText::new(title)
                        .color(theme.text_primary)
                        .size(TYPE_BASE)
                        .strong(),
                );
                ui.label(
                    egui::RichText::new(description)
                        .color(theme.text_secondary)
                        .size(TYPE_SM),
                );
            });
            ui.add_space(SPACE_SM);

            let action_label = if is_moving {
                "Pausar animación"
            } else {
                "Iniciar animación"
            };
            let action_button = egui::Button::new(
                egui::RichText::new(action_label)
                    .color(if is_moving {
                        theme.text_primary
                    } else {
                        theme.keyboard_enter_text
                    })
                    .strong(),
            )
            .fill(if is_moving {
                theme.button_bg
            } else {
                theme.keyboard_enter_bg
            })
            .stroke(egui::Stroke::new(1.0, card_stroke));
            let response = ui
                .add_enabled_ui(can_animate, |ui| {
                    ui.add_sized([ui.available_width(), 30.0], action_button)
                })
                .inner;
            if response.clicked() {
                is_moving = crate::app::toggle_default_multidimensional_motion(
                    &mut app.multidimensional_motion_enabled,
                );
                app.notify(
                    if is_moving {
                        "Animación espacial iniciada."
                    } else {
                        "Animación espacial pausada."
                    },
                    grafito_ui::toast::ToastKind::Info,
                );
                ui.ctx().request_repaint();
            }

            ui.add_space(SPACE_SM);
            ui.horizontal(|ui| {
                ui.label(
                    egui::RichText::new(if is_moving {
                        "En reproducción"
                    } else if can_animate {
                        "En pausa"
                    } else {
                        "No disponible"
                    })
                    .color(if is_moving {
                        theme.success
                    } else {
                        theme.text_secondary
                    })
                    .size(TYPE_SM)
                    .strong(),
                );
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    ui.label(
                        egui::RichText::new(format!("{:.2}x", app.multidimensional_motion_speed))
                            .color(theme.text_primary)
                            .size(TYPE_SM)
                            .monospace(),
                    );
                    ui.label(
                        egui::RichText::new("Velocidad")
                            .color(theme.text_secondary)
                            .size(TYPE_SM),
                    );
                });
            });

            let mut speed = app.multidimensional_motion_speed;
            if ui
                .add(
                    egui::Slider::new(
                        &mut speed,
                        crate::app::MIN_MULTIDIMENSIONAL_MOTION_SPEED
                            ..=crate::app::MAX_MULTIDIMENSIONAL_MOTION_SPEED,
                    )
                    .text("Velocidad de animación")
                    .show_value(false)
                    .step_by(0.25)
                    .trailing_fill(true),
                )
                .changed()
            {
                app.set_multidimensional_motion_speed(speed);
                ui.ctx().request_repaint();
            }
            if ui
                .small_button("Restablecer velocidad")
                .on_hover_text("Volver a la velocidad normal (1.00x)")
                .clicked()
            {
                app.set_multidimensional_motion_speed(
                    crate::app::DEFAULT_MULTIDIMENSIONAL_MOTION_SPEED,
                );
                ui.ctx().request_repaint();
            }

            if !can_animate {
                ui.add_space(SPACE_SM);
                ui.label(
                    egui::RichText::new(
                        "Mostrá el objeto en la vista 3D para activar la animación.",
                    )
                    .color(theme.text_tertiary)
                    .size(TYPE_SM),
                );
            }
        });
}

fn draw_object_cards_where(
    ui: &mut egui::Ui,
    app: &mut GrafitoApp,
    title: &str,
    empty_text: &str,
    predicate: impl Fn(&GeoObject) -> bool,
) {
    let theme = current_theme(ui.ctx());
    ui.add_space(SPACE_SM + 2.0);
    ui.label(
        egui::RichText::new(title)
            .color(theme.text_secondary)
            .size(TYPE_SM)
            .strong(),
    );
    ui.add_space(SPACE_XS);

    let ids: Vec<ObjectId> = app
        .document
        .objects_iter()
        .filter_map(|(id, obj)| predicate(obj).then_some(*id))
        .collect();

    if ids.is_empty() {
        ui.label(
            egui::RichText::new(empty_text)
                .color(theme.text_tertiary)
                .size(TYPE_XS),
        );
        return;
    }

    for id in ids {
        crate::algebra::draw_object_card(ui, app, id);
    }
}

#[allow(dead_code)]
pub(crate) fn draw_cas_panel(app: &mut GrafitoApp, ctx: &egui::Context) {
    let theme = current_theme(ctx);
    let accent = theme.accent;
    let panel_bg = theme.panel_bg;
    let txt_col = theme.text_primary;
    let txt_dim = theme.text_tertiary;

    // Panel CAS — Scandinavian quiet: secciones calm con disclosure progresivo
    egui::SidePanel::left("cas_panel")
        .show_separator_line(false)
        .default_width(260.0)
        .min_width(180.0)
        .max_width((ctx.available_rect().width() * 0.45).max(200.0))
        .resizable(true)
        .frame(
            egui::Frame::none()
                .fill(panel_bg)
                .stroke(egui::Stroke::new(1.0, theme.separator)),
        )
        .show(ctx, |ui| {
            ui.add_space(SPACE_LG);
            ui.horizontal(|ui| {
                ui.add_space(SPACE_SM);
                ui.label(
                    egui::RichText::new("Cálculo Simbólico (CAS)")
                        .color(accent)
                        .strong()
                        .size(TYPE_MD),
                );
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    if !app.document.cas_worksheet().is_empty()
                        && ui
                            .small_button("Limpiar")
                            .on_hover_text("Eliminar las celdas CAS guardadas")
                            .clicked()
                    {
                        app.clear_cas_worksheet(ui.ctx().input(|input| input.time));
                    }
                });
            });
            ui.add_space(SPACE_XS);
            ui.separator();
            ui.add_space(SPACE_SM);

            egui::ScrollArea::vertical()
                .id_salt("cas_panel_scroll")
                .auto_shrink([false, false])
                .show(ui, |ui| {
                    egui::Frame::none()
                        .inner_margin(egui::Margin {
                            left: SPACE_SM,
                            right: SPACE_SM,
                            top: SPACE_SM,
                            bottom: SPACE_SM,
                        })
                        .show(ui, |ui| {
                            // Acciones rápidas — pills Scandinavian dentro de sección inspector
                            draw_inspector_section(
                                ui,
                                "Acciones rápidas",
                                "Atajos — inserta sintaxis mínima.",
                                |ui| {
                                    ui.horizontal_wrapped(|ui| {
                                        ui.spacing_mut().item_spacing =
                                            egui::vec2(SPACE_XS, SPACE_XS);
                                        for (label, snippet) in [
                                            ("Derivar", "Derivative["),
                                            ("Integrar", "Integral["),
                                            ("Resolver", "Solve["),
                                            ("Límite", "Limit["),
                                        ] {
                                            let btn = egui::Button::new(
                                                egui::RichText::new(label)
                                                    .size(TYPE_SM)
                                                    .color(theme.text_primary),
                                            )
                                            .rounding(RADIUS_PILL)
                                            .fill(theme.input_bg)
                                            .stroke(egui::Stroke::new(
                                                1.0,
                                                Color32::from_black_alpha(26),
                                            ));
                                            if ui
                                                .add(btn)
                                                .on_hover_text(format!("Insertar {snippet}"))
                                                .clicked()
                                            {
                                                app.input_text = snippet.to_string();
                                            }
                                        }
                                    });
                                },
                            );
                            ui.add_space(SPACE_MD);

                            // Entrada — Scandinavian centered, compact, 32h
                            ui.vertical_centered(|ui| {
                                egui::Frame::none()
                                    .fill(theme.input_bg)
                                    .stroke(egui::Stroke::new(1.0, Color32::from_black_alpha(18)))
                                    .rounding(egui::Rounding::same(RADIUS_LG))
                                    .inner_margin(egui::Margin::symmetric(SPACE_MD, SPACE_SM))
                                    .show(ui, |ui| {
                                        ui.set_max_width(260.0);
                                        ui.label(
                                            egui::RichText::new("Entrada")
                                                .color(theme.text_tertiary)
                                                .size(TYPE_XS)
                                                .strong(),
                                        );
                                        ui.add_space(SPACE_XS);
                                        ui.horizontal(|ui| {
                                            let mut execute_cas = false;
                                            // Input centered, max 200, height 32
                                            let response = crate::ui::draw_command_input(
                                                ui,
                                                app,
                                                "cas_panel",
                                                [200.0, 32.0],
                                                "x^2, x",
                                                true,
                                            );
                                            ui.add_space(SPACE_XS);
                                            if action_icon_button(
                                                ui,
                                                Icon::Play,
                                                accent,
                                                "Ejecutar",
                                            )
                                            .clicked()
                                            {
                                                execute_cas = true;
                                            }
                                            if response.submitted {
                                                execute_cas = true;
                                            }
                                            if execute_cas && !app.input_text.is_empty() {
                                                let time = ui.ctx().input(|i| i.time);
                                                app.submit_cas_worksheet_cell(time);
                                            }
                                        });
                                        ui.add_space(SPACE_XS);
                                        ui.label(
                                            egui::RichText::new("↵  •  Tab")
                                                .color(txt_dim)
                                                .size(TYPE_XS),
                                        );
                                    });
                            });
                            ui.add_space(SPACE_MD);

                            // Hoja de trabajo — empty state sutil y celdas con estados
                            if app.document.cas_worksheet().is_empty() {
                                egui::Frame::none()
                                    .fill(theme.input_bg)
                                    .stroke(egui::Stroke::new(1.0, Color32::from_black_alpha(18)))
                                    .rounding(egui::Rounding::same(RADIUS_LG))
                                    .inner_margin(egui::Margin::same(SPACE_LG))
                                    .show(ui, |ui| {
                                        ui.vertical_centered(|ui| {
                                            ui.label(
                                                egui::RichText::new("Sin cálculos aún")
                                                    .color(txt_col)
                                                    .size(TYPE_SM)
                                                    .strong(),
                                            );
                                            ui.add_space(SPACE_XS);
                                            ui.label(
                                                egui::RichText::new(
                                                    "Escribe arriba y pulsa Enter.",
                                                )
                                                .color(txt_dim)
                                                .size(TYPE_XS),
                                            );
                                            ui.add_space(SPACE_SM);
                                            ui.label(
                                                egui::RichText::new(
                                                    "Ej: Derivative[x^2, x]  ·  Solve[x^2-4, x]",
                                                )
                                                .color(txt_dim)
                                                .size(TYPE_SM)
                                                .monospace(),
                                            );
                                        });
                                    });
                            } else {
                                ui.label(
                                    egui::RichText::new(format!(
                                        "Hoja · {}",
                                        app.document.cas_worksheet().len()
                                    ))
                                    .color(theme.text_secondary)
                                    .size(TYPE_SM),
                                );
                                ui.add_space(SPACE_XS);
                                for (i, entry) in app.document.cas_worksheet().iter().enumerate() {
                                    let output_color = match entry.status {
                                        CasWorksheetStatus::Success => txt_col,
                                        CasWorksheetStatus::Error => theme.danger,
                                    };
                                    egui::Frame::none()
                                        .fill(theme.button_bg)
                                        .stroke(egui::Stroke::new(
                                            1.0,
                                            Color32::from_black_alpha(26),
                                        ))
                                        .rounding(egui::Rounding::same(RADIUS_MD))
                                        .inner_margin(egui::Margin::same(SPACE_SM))
                                        .show(ui, |ui| {
                                            ui.horizontal(|ui| {
                                                ui.label(
                                                    egui::RichText::new(format!("{}", i + 1))
                                                        .color(accent)
                                                        .strong()
                                                        .size(TYPE_SM),
                                                );
                                                ui.add_space(SPACE_XS);
                                                ui.label(
                                                    egui::RichText::new(format!(
                                                        "> {}",
                                                        entry.input
                                                    ))
                                                    .size(TYPE_SM)
                                                    .monospace()
                                                    .color(txt_col),
                                                );
                                            });
                                            ui.add_space(SPACE_XS);
                                            ui.label(
                                                egui::RichText::new(&entry.output)
                                                    .size(TYPE_SM)
                                                    .color(output_color),
                                            );
                                        });
                                    ui.add_space(SPACE_XS);
                                }
                            }
                            ui.add_space(SPACE_SM);
                        });
                });
        });
}

pub(crate) fn draw_view_panel(app: &mut GrafitoApp, ctx: &egui::Context) {
    // Panel Vista — Scandinavian quiet
    let theme = current_theme(ctx);
    let accent = theme.accent;

    egui::SidePanel::left("view_panel")
        .show_separator_line(false)
        .default_width(PANEL_LEFT_DEFAULT)
        .min_width(PANEL_LEFT_MIN)
        .max_width(
            (ctx.available_rect().width() * PANEL_LEFT_MAX_FRACTION)
                .max(PANEL_LEFT_DEFAULT - 40.0),
        )
        .resizable(true)
        .frame(
            egui::Frame::none()
                .fill(theme.panel_bg)
                .stroke(egui::Stroke::NONE),
        )
        .show(ctx, |ui| {
            ui.add_space(SPACE_SM);
            ui.horizontal(|ui| {
                ui.spacing_mut().item_spacing.x = SPACE_SM;
                ui.add_space(SPACE_XS);
                ui.label(
                    egui::RichText::new("Vista")
                        .color(accent)
                        .strong()
                        .size(TYPE_LG),
                );
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    ui.add_space(SPACE_SM);
                    if action_icon_button(
                        ui,
                        Icon::Close,
                        theme.text_secondary,
                        "Ocultar panel Vista",
                    )
                    .clicked()
                    {
                        app.left_drawer_open = false;
                        app.compact_drawer_open = false;
                    }
                });
            });
            ui.add_space(SPACE_SM);
            ui.painter().line_segment(
                [
                    ui.cursor().min,
                    ui.cursor().min + egui::vec2(ui.available_width(), 0.0),
                ],
                theme.hairline_stroke(),
            );
            ui.add_space(SPACE_SM);

            egui::ScrollArea::vertical()
                .id_salt("view_panel_scroll")
                .auto_shrink([false, false])
                .show(ui, |ui| {
                    egui::Frame::none()
                        .inner_margin(egui::Margin {
                            left: SPACE_SM,
                            right: SPACE_SM,
                            top: SPACE_SM,
                            bottom: SPACE_SM,
                        })
                        .show(ui, |ui| {
                            // General — 4 toggles básicos
                            draw_inspector_section(ui, "General", "Cuadrícula y tema.", |ui| {
                                ui.checkbox(&mut app.show_grid, "Mostrar cuadrícula");
                                ui.checkbox(&mut app.dark_mode, "Modo oscuro")
                                    .changed()
                                    .then(|| {
                                        if app.dark_mode {
                                            DARK.apply(ui.ctx());
                                        } else {
                                            LIGHT.apply(ui.ctx());
                                        }
                                    });
                                ui.checkbox(&mut app.snap_to_grid, "Ajustar a cuadrícula");
                                ui.checkbox(&mut app.exam_mode, "Modo examen");
                            });
                            ui.add_space(CARD_SPACING);

                            // Ejes — escala logarítmica
                            draw_inspector_section(
                                ui,
                                "Ejes",
                                "Escalas logarítmicas por eje.",
                                |ui| {
                                    ui.checkbox(
                                        &mut app.document.view_mut().x_log,
                                        "Eje X logarítmico",
                                    );
                                    ui.checkbox(
                                        &mut app.document.view_mut().y_log,
                                        "Eje Y logarítmico",
                                    );
                                },
                            );
                            ui.add_space(CARD_SPACING);

                            // Alta precisión — Double-Double
                            draw_inspector_section(
                                ui,
                                "Alta Precisión",
                                "Double-Double (~106 bits / 32 dígitos).",
                                |ui| {
                                    let mut high_prec =
                                        grafito_geometry::precision::is_high_precision_mode();
                                    ui.style_mut().wrap_mode = Some(egui::TextWrapMode::Wrap);
                                    if ui
                                        .add(egui::Checkbox::new(
                                            &mut high_prec,
                                            egui::RichText::new("Alta Precisión (Double-Double)")
                                                .size(TYPE_SM),
                                        ))
                                        .on_hover_text(
                                            "Usa aritmética Double-Double (~106 bits / 32 dígitos) \
                                             para evaluar expresiones simbólicas sin pérdida de precisión.",
                                        )
                                        .changed()
                                    {
                                        grafito_geometry::precision::set_high_precision_mode(high_prec);
                                        app.document.invalidate_all_caches();
                                        app.document.bump_version();
                                        if let Ok(mut cache) = app.trig_graph_cache.write() {
                                            *cache = None;
                                        }
                                        app.re_evaluate_constraints(&[]);
                                    }
                                },
                            );
                            ui.add_space(CARD_SPACING);

                            // Exportación — vectorial pill centered
                            draw_inspector_section(
                                ui,
                                "Exportación",
                                "Generá un archivo vectorial del lienzo actual.",
                                |ui| {
                                    let theme = current_theme(ui.ctx());
                                    let btn = egui::Button::new(
                                        egui::RichText::new("Exportar SVG")
                                            .size(TYPE_SM)
                                            .strong()
                                            .color(theme.keyboard_enter_text),
                                    )
                                    .fill(theme.keyboard_enter_bg)
                                    .stroke(egui::Stroke::NONE)
                                    .rounding(RADIUS_PILL);
                                    if ui
                                        .add_sized([ui.available_width(), ZOOM_ICON_HIT], btn)
                                        .clicked()
                                    {
                                        app.export_with_dialog(crate::export::ExportFormat::Svg);
                                    }
                                },
                            );
                        });
                });
        });
}

/// Panel derecho: controles de la animación trigonométrica.
///
/// El círculo y la función se dibujan como overlay del canvas 2D para compartir
/// exactamente la grilla, escala y perspectiva de Geometry2D.
pub(crate) fn draw_trig_animation_panel(app: &mut GrafitoApp, ctx: &egui::Context) {
    let (_is_dark, accent, alg_fill, _sep_col, _txt_col, txt_dim, hdr_col) = panel_theme_local(ctx);

    egui::SidePanel::right("right_trig_animation").show_separator_line(false)
        .default_width(280.0)
        .min_width(220.0)
        .max_width((ctx.available_rect().width() * 0.45).max(240.0))
        .resizable(true)
        .frame(
            egui::Frame::none()
                .fill(alg_fill)
                .stroke(egui::Stroke::NONE),
        )
        .show(ctx, |ui| {
            egui::ScrollArea::vertical()
                .auto_shrink([false, true])
                .show(ui, |ui| {
                    ui.add_space(SPACE_SM);
                    ui.horizontal(|ui| {
                        ui.label(
                            egui::RichText::new(if app.perspective == crate::Perspective::Complex {
                                "Animación Compleja"
                            } else {
                                "Explorador Trigonométrico"
                            })
                                .color(accent)
                                .size(TYPE_MD)
                                .strong(),
                        );
                        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                            if ui
                                .small_button("x")
                                .on_hover_text("Cerrar animación")
                                .clicked()
                            {
                                app.set_trig_animation_visible(false);
                            }
                        });
                    });
                    ui.add_space(6.0);

                    if app.perspective == crate::Perspective::Complex {
                        ui.label(
                            egui::RichText::new("z(t) = cos(t) + i sin(t) = e^(it)")
                                .color(hdr_col)
                                .size(TYPE_SM)
                                .strong(),
                        );
                        ui.label(
                            egui::RichText::new(
                                "El punto rojo recorre el círculo unitario; el punto violeta muestra su imagen por la transformación compleja activa.",
                            )
                            .color(txt_dim)
                            .size(10.5),
                        );
                        ui.add_space(SPACE_SM);
                    }

                    ui.label(
                        egui::RichText::new("Función activa")
                            .color(hdr_col)
                            .size(TYPE_SM),
                    );
                    ui.horizontal_wrapped(|ui| {
                        for (idx, spec) in crate::app::TRIG_FUNCTIONS.iter().enumerate() {
                            let label = format!("{}(t)", spec.name);
                            if ui
                                .selectable_label(app.trig_function as usize == idx, label)
                                .clicked()
                            {
                                app.set_trig_function(idx as u8);
                            }
                        }
                    });

                    ui.add_space(6.0);

                    let spec = GrafitoApp::trig_spec(app.trig_function);

                    ui.horizontal_wrapped(|ui| {
                        if action_icon_button(
                            ui,
                            if app.trig_animating { Icon::Pause } else { Icon::Play },
                            if app.trig_animating { accent } else { txt_dim },
                            if app.trig_animating {
                                "Pausar animación"
                            } else {
                                "Iniciar animación"
                            },
                        )
                        .clicked()
                        {
                            app.trig_animating = !app.trig_animating;
                        }
                        ui.label(egui::RichText::new("Velocidad").color(txt_dim).size(TYPE_XS));
                        let speed_changed = ui
                            .add(
                                egui::Slider::new(&mut app.trig_speed, -6.0..=6.0)
                                    .fixed_decimals(1)
                                    .suffix(" rad/s"),
                            )
                            .changed();
                        if speed_changed {
                            ctx.request_repaint();
                        }
                    });

                    ui.horizontal_wrapped(|ui| {
                        ui.label(egui::RichText::new("Vista").color(txt_dim).size(TYPE_XS));
                        if ui
                            .selectable_label(
                                app.trig_view_mode == crate::app::TrigViewMode::Didactic,
                                "Didáctica",
                            )
                            .on_hover_text("Círculo unitario siempre visible en una tarjeta flotante")
                            .clicked()
                        {
                            app.trig_view_mode = crate::app::TrigViewMode::Didactic;
                            ctx.request_repaint();
                        }
                        if ui
                            .selectable_label(
                                app.trig_view_mode == crate::app::TrigViewMode::Grid,
                                "Sobre grilla",
                            )
                            .on_hover_text("Dibuja el círculo unitario en coordenadas reales")
                            .clicked()
                        {
                            app.trig_view_mode = crate::app::TrigViewMode::Grid;
                            ctx.request_repaint();
                        }
                    });

                    let angle_changed = ui
                        .horizontal(|ui| {
                            ui.label(egui::RichText::new("Ángulo").color(txt_dim).size(TYPE_XS));
                            ui.add(
                                egui::Slider::new(
                                    &mut app.trig_angle,
                                    -2.0 * std::f64::consts::PI..=2.0 * std::f64::consts::PI,
                                )
                                .fixed_decimals(2)
                                .suffix(" rad"),
                            )
                            .changed()
                        })
                        .inner;
                    if angle_changed {
                        ctx.request_repaint();
                    }

                    let t = app.trig_angle;
                    let fn_val = GrafitoApp::trig_value(app.trig_function, t);
                    let cos_t = t.cos();
                    let sin_t = t.sin();
                    let value_text = if fn_val.is_finite() {
                        format!("{}({:.2}) = {:.4}", spec.name, t, fn_val)
                    } else {
                        format!("{}({:.2}) no está definido", spec.name, t)
                    };
                    ui.add_space(6.0);
                    egui::Frame::none()
                        .fill(current_theme(ctx).input_bg)
                        .rounding(egui::Rounding::same(8.0))
                        .inner_margin(egui::Margin::same(8.0))
                        .show(ui, |ui| {
                            ui.label(egui::RichText::new(value_text).color(accent).size(TYPE_SM).strong());
                            ui.label(
                                egui::RichText::new(format!(
                                    "Punto: (cos θ, sin θ) = ({:.3}, {:.3})",
                                    cos_t, sin_t
                                ))
                                .color(hdr_col)
                                .size(TYPE_XS),
                            );
                            ui.label(
                                egui::RichText::new(GrafitoApp::trig_identity(app.trig_function))
                                    .color(txt_dim)
                                    .size(10.5),
                            );
                        });

                    if app.perspective == crate::Perspective::Complex {
                        ui.add_space(6.0);
                        ui.label(
                            egui::RichText::new(format!(
                                "z = {:.3} {:+.3}i  |z| = 1  arg(z) = {:.2}",
                                cos_t, sin_t, t
                            ))
                            .color(hdr_col)
                            .size(10.5),
                        );
                    }

                    ui.add_space(SPACE_SM);
                    if ui.button("Centrar vista en la gráfica").clicked() {
                        app.document.set_view(grafito_geometry::ViewTransform::default());
                        app.document.bump_version();
                        if let Ok(mut cache) = app.trig_graph_cache.write() {
                            *cache = None;
                        }
                    }
                });
        });
}

pub(crate) fn draw_empty_panel(_app: &mut GrafitoApp, ctx: &egui::Context) {
    let (_is_dark, _accent, alg_fill, _sep_col, _txt_col, _txt_dim, _hdr_col) =
        panel_theme_local(ctx);
    let theme = current_theme(ctx);

    egui::SidePanel::left("empty_panel")
        .show_separator_line(false)
        .default_width(220.0)
        .min_width(160.0)
        .resizable(true)
        .frame(
            egui::Frame::none()
                .fill(alg_fill)
                .stroke(egui::Stroke::NONE),
        )
        .show(ctx, |ui| {
            let height = ui.available_height().max(120.0);
            ui.allocate_ui_with_layout(
                egui::vec2(ui.available_width(), height),
                egui::Layout::top_down(egui::Align::Center),
                |ui| {
                    ui.add_space(48.0);
                    ui.label(
                        egui::RichText::new("Sin panel aquí")
                            .color(theme.text_tertiary)
                            .size(TYPE_SM),
                    );
                    ui.label(
                        egui::RichText::new(
                            "Cambiá de perspectiva o abrí un panel desde «Paneles».",
                        )
                        .color(theme.text_secondary)
                        .size(TYPE_SM),
                    );
                },
            );
        });
}

// ══════════════════════════════════════════════════════════════════════════
// Paneles izquierdos específicos por perspectiva (Fase 2)
// ══════════════════════════════════════════════════════════════════════════

/// Panel izquierdo de Estadística. Permite ingresar datos y ver resumen.
#[allow(dead_code)] // TODO P2: Panel sin entrada en la UI desde que se quitó la pestaña «Datos».
pub(crate) fn draw_statistics_panel(app: &mut GrafitoApp, ctx: &egui::Context) {
    let (_is_dark, accent, alg_fill, _sep_col, txt_col, txt_dim, hdr_col) = panel_theme_local(ctx);

    egui::SidePanel::left("stats_panel")
        .show_separator_line(false)
        .default_width(240.0)
        .min_width(180.0)
        .resizable(true)
        .frame(
            egui::Frame::none()
                .fill(alg_fill)
                .stroke(egui::Stroke::NONE),
        )
        .show(ctx, |ui| {
            egui::ScrollArea::vertical()
                .id_salt("stats_panel_content")
                .auto_shrink([false, false])
                .show(ui, |ui| {
                    ui.add_space(SPACE_SM);
                    ui.label(
                        egui::RichText::new("Estadística")
                            .color(accent)
                            .size(15.0)
                            .strong(),
                    );
                    ui.add_space(SPACE_SM);

                    draw_object_cards_where(
                        ui,
                        app,
                        "Objetos estadísticos",
                        "Sin gráficos estadísticos.\nProbá Histogram[...] o ScatterPlot[...].",
                        |obj| {
                            matches!(
                                obj,
                                GeoObject::Histogram(_)
                                    | GeoObject::ScatterPlot(_)
                                    | GeoObject::BoxPlot(_)
                                    | GeoObject::RegressionLine(_)
                                    | GeoObject::Function(_)
                            )
                        },
                    );
                    ui.add_space(SPACE_SM);

                    // ── Datos: TextEdit vinculado al buffer persistente ──
                    // El buffer sólo se parsea al perder foco o al apretar "Aplicar"
                    // — antes, el editor reconstruí el string cada frame desde los
                    // valores parseados y destruía la entrada del usuario por cada
                    // coma en blanco o no-número temporal.
                    ui.label(
                        egui::RichText::new("Datos (uno por línea o coma):")
                            .color(hdr_col)
                            .size(TYPE_SM),
                    );
                    let te_resp = ui.add_sized(
                        [ui.available_width(), 80.0],
                        egui::TextEdit::multiline(&mut app.statistics_input_buf).desired_rows(3),
                    );

                    ui.add_space(SPACE_XS);
                    ui.horizontal(|ui| {
                        let apply_clicked = ui.button("Aplicar").clicked();
                        let lost_focus =
                            te_resp.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter));
                        if apply_clicked || lost_focus {
                            match parse_statistics_input(&app.statistics_input_buf) {
                                Ok(parsed) => {
                                    app.statistics_input_error = None;
                                    if parsed != app.statistics_data {
                                        app.statistics_data = parsed;
                                        app.document.bump_version();
                                    }
                                }
                                Err(error) => app.statistics_input_error = Some(error),
                            }
                        }
                        if ui.button("Limpiar").clicked() {
                            app.statistics_input_buf.clear();
                            app.statistics_data.clear();
                            app.statistics_input_error = None;
                            app.document.bump_version();
                        }
                    });

                    if let Some(error) = &app.statistics_input_error {
                        ui.label(
                            egui::RichText::new(error)
                                .color(current_theme(ctx).danger)
                                .size(TYPE_XS),
                        );
                    }

                    ui.add_space(SPACE_SM);
                    if app.statistics_data.is_empty() {
                        // Empty-state
                        ui.label(
                            egui::RichText::new(
                                "Ingresá datos arriba (uno por línea o comas)\n\
                         y pulsá «Aplicar» para ver el resumen y el\n\
                         histograma.\n\
                         Ejemplo: 1, 2, 3, 5, 4, 6",
                            )
                            .color(txt_dim)
                            .size(TYPE_XS),
                        );
                    } else {
                        let data = &app.statistics_data;
                        let summary = match statistics_summary(data) {
                            Ok(summary) => summary,
                            Err(error) => {
                                ui.label(
                                    egui::RichText::new(error)
                                        .color(current_theme(ctx).danger)
                                        .size(TYPE_XS),
                                );
                                return;
                            }
                        };

                        ui.label(
                            egui::RichText::new("Resumen")
                                .color(hdr_col)
                                .size(TYPE_SM)
                                .strong(),
                        );
                        ui.add_space(SPACE_XS);
                        egui::Grid::new("stats_grid")
                            .num_columns(2)
                            .striped(true)
                            .spacing([10.0, 4.0])
                            .show(ui, |ui| {
                                let mut row = |k: &str, v: String| {
                                    ui.label(egui::RichText::new(k).color(txt_dim).size(TYPE_SM));
                                    ui.label(
                                        egui::RichText::new(v)
                                            .color(txt_col)
                                            .size(TYPE_SM)
                                            .strong(),
                                    );
                                    ui.end_row();
                                };
                                row("N", format!("{}", data.len()));
                                row(
                                    "Suma",
                                    summary.sum.map(format_statistic).unwrap_or_else(|| {
                                        "No representable (desbordamiento)".to_string()
                                    }),
                                );
                                row("Media", format_statistic(summary.mean));
                                row("Mediana", format_statistic(summary.median));
                                row("Desvío", format_statistic(summary.standard_deviation));
                                row("Varianza", format_statistic(summary.variance));
                                row("Mín", format_statistic(summary.minimum));
                                row("Máx", format_statistic(summary.maximum));
                                row("Rango", format_statistic(summary.range));
                                row("Q1", format_statistic(summary.q1));
                                row("Q3", format_statistic(summary.q3));
                                row("IQR", format_statistic(summary.iqr));
                            });

                        ui.add_space(SPACE_SM);
                        ui.label(
                            egui::RichText::new("Histograma")
                                .color(hdr_col)
                                .size(TYPE_SM)
                                .strong(),
                        );
                        ui.add_space(2.0);
                        let bins = 10usize;
                        let bw = summary.range.max(1e-9) / bins as f64;
                        let mut counts = vec![0u32; bins];
                        for v in data {
                            let idx = (((v - summary.minimum) / bw).floor() as usize).min(bins - 1);
                            counts[idx] += 1;
                        }
                        let max_c = (*counts.iter().max().unwrap_or(&1)).max(1) as f32;
                        let hist_h = 90.0;
                        let (hist_rect, _) = ui.allocate_exact_size(
                            egui::vec2(ui.available_width(), hist_h + 14.0),
                            egui::Sense::hover(),
                        );
                        if ui.is_rect_visible(hist_rect) {
                            let painter = ui.painter();
                            let plot = hist_rect.shrink2(egui::vec2(2.0, 2.0));
                            // Plot area interna
                            let plot_top = plot.min.y;
                            let plot_bot = plot.max.y - 14.0;
                            let plot_h = plot_bot - plot_top;
                            let plot_w = plot.width();
                            // Ejes: línea base
                            painter.line_segment(
                                [
                                    egui::pos2(plot.min.x, plot_bot),
                                    egui::pos2(plot.max.x, plot_bot),
                                ],
                                egui::Stroke::new(1.0, _sep_col.gamma_multiply(0.10)),
                            );
                            // Barras
                            let bar_w = plot_w / bins as f32;
                            for (i, c) in counts.iter().enumerate() {
                                let h = (*c as f32 / max_c) * plot_h;
                                let bar = egui::Rect::from_min_size(
                                    egui::pos2(plot.min.x + i as f32 * bar_w + 2.0, plot_bot - h),
                                    egui::vec2(bar_w - 4.0, h),
                                );
                                painter.rect_filled(bar, 2.0, accent);
                                // count label encima si > 0
                                if *c > 0 {
                                    painter.text(
                                        egui::pos2(bar.center().x, bar.min.y - 6.0),
                                        egui::Align2::CENTER_BOTTOM,
                                        c.to_string(),
                                        egui::FontId::proportional(9.0),
                                        txt_dim,
                                    );
                                }
                            }
                            // Etiquetas min/max en el eje
                            painter.text(
                                egui::pos2(plot.min.x, plot_bot + 2.0),
                                egui::Align2::LEFT_TOP,
                                format_statistic(summary.minimum),
                                egui::FontId::proportional(9.0),
                                txt_dim,
                            );
                            painter.text(
                                egui::pos2(plot.max.x, plot_bot + 2.0),
                                egui::Align2::RIGHT_TOP,
                                format_statistic(summary.maximum),
                                egui::FontId::proportional(9.0),
                                txt_dim,
                            );
                        }
                    }
                });
        });
}

/// Panel izquierdo de Complejos. Lista objetos complejos y permite cambiar
/// el símbolo base.
pub(crate) fn draw_complex_panel(app: &mut GrafitoApp, ctx: &egui::Context) {
    use grafito_core::{GeoObject, ObjectId};
    let mut snapshot = crate::app::DeferredPanelSnapshot::new(app.undo_stack.len());
    let (_is_dark, accent, alg_fill, _sep_col, txt_col, txt_dim, hdr_col) = panel_theme_local(ctx);

    egui::SidePanel::left("complex_panel").show_separator_line(false)
        .default_width(260.0)
        .min_width(180.0)
        .resizable(true)
        .frame(
            egui::Frame::none()
                .fill(alg_fill)
                .stroke(egui::Stroke::NONE),
        )
        .show(ctx, |ui| {
            ui.add_space(SPACE_SM);
            ui.label(
                egui::RichText::new("Números Complejos")
                    .color(accent)
                    .size(15.0)
                    .strong(),
            );
            ui.add_space(SPACE_SM);

            // ── Barra de entrada in-panel (igual que Álgebra) ──
            egui::Frame::none()
                .fill(current_theme(ctx).input_bg)
                .inner_margin(egui::Margin {
                    left: 8.0,
                    right: 8.0,
                    top: 6.0,
                    bottom: 6.0,
                })
                .show(ui, |ui| {
                    ui.horizontal(|ui| {
                        ui.label(egui::RichText::new("+").color(accent).size(17.0).strong());
                        ui.add_space(SPACE_XS);
                        let response = crate::ui::draw_command_input(
                            ui,
                            app,
                            "complex_panel",
                            [ui.available_width(), 22.0],
                            "DomainColoring[1/z, -2, 2, -2, 2, 160]",
                            false,
                        );
                        if response.submitted && !app.input_text.is_empty() {
                            let time = ui.ctx().input(|i| i.time);
                            app.submit_input_text(time);
                        }
                    });
                });
            ui.add(egui::Separator::default().spacing(0.0));
            ui.add_space(SPACE_SM);

            // ── Símbolo base ──
            ui.label(
                egui::RichText::new("Símbolo base")
                    .color(hdr_col)
                    .size(TYPE_SM),
            );
            let mut sym = app.document.complex_base_symbol.clone();
            let resp = ui.add(
                egui::TextEdit::singleline(&mut sym)
                    .desired_width(ui.available_width())
                    .hint_text("z"),
            );
            if resp.lost_focus() && sym.trim() != app.document.complex_base_symbol {
                let new_sym = sym.trim().to_string();
                if !new_sym.is_empty() {
                    app.document.migrate_complex_symbol(&new_sym);
                    app.document.bump_version();
                }
            }

            ui.add_space(SPACE_SM);
            let content_height = ui.available_height();
            egui::ScrollArea::vertical()
                .id_salt("complex_panel_content")
                .max_height(content_height)
                .auto_shrink([false, false])
                .show(ui, |ui| {
                    ui.label(
                        egui::RichText::new("Objetos")
                            .color(hdr_col)
                            .size(TYPE_SM)
                            .strong(),
                    );
                    ui.add_space(SPACE_XS);
                    let ids: Vec<ObjectId> =
                        app.document.objects_iter().map(|(id, _)| *id).collect();
                    let mut any_object = false;
                    for id in &ids {
                        let Some(obj) = app.document.get_object(*id) else {
                            continue;
                        };
                        if !matches!(
                            obj,
                            GeoObject::Function(_)
                                | GeoObject::ImplicitCurve(_)
                                | GeoObject::ParametricCurve2D(_)
                                | GeoObject::PolarCurve(_)
                                | GeoObject::VectorField2D(_)
                                | GeoObject::ComplexGrid(_)
                                | GeoObject::ComplexMapping(_)
                                | GeoObject::Point(_)
                                | GeoObject::Line(_)
                                | GeoObject::Circle(_)
                                | GeoObject::Polygon(_)
                                | GeoObject::Ellipse(_)
                                | GeoObject::Parabola(_)
                                | GeoObject::Hyperbola(_)
                        ) {
                            continue;
                        }
                        any_object = true;
                        crate::algebra::draw_object_card(ui, app, *id);
                    }
                    if !any_object {
                        ui.label(
                            egui::RichText::new(
                                "Sin objetos.\nProbá: x^2 + y^2 < 1\no: DomainColoring[1/z, -2, 2, -2, 2, 160]",
                            )
                            .color(txt_dim)
                            .size(TYPE_XS),
                        );
                    }

                    ui.add_space(SPACE_SM);
                    ui.label(
                        egui::RichText::new("Comandos rápidos")
                            .color(hdr_col)
                            .size(TYPE_SM)
                            .strong(),
                    );
                    ui.add_space(2.0);
                    // Atajos ejecutables: cada uno crea una visualización cuya
                    // semántica coincide con su etiqueta.
                    let hints: &[(&str, &str)] = &[
                        (
                            "Coloración de dominio: 1/z",
                            "DomainColoring[1/z, -2, 2, -2, 2, 160]",
                        ),
                        (
                            "Rejilla transformada: 1/z",
                            "ComplexGrid[1/z, -2, 2, -2, 2, 16]",
                        ),
                        ("ComplexMapping[1/z, I]", "ComplexMapping[1/z, I]"),
                        (
                            "Coloración de dominio: exp(z)",
                            "DomainColoring[exp(z), -2, 2, -2, 2, 160]",
                        ),
                        ("ComplexSymbol[w]", "ComplexSymbol[w]"),
                    ];
                    for (label, payload) in hints {
                        let b = ui.add(
                            egui::Button::new(
                                egui::RichText::new(*label)
                                    .monospace()
                                    .size(TYPE_XS)
                                    .color(txt_col),
                            )
                            .frame(false),
                        );
                        if b.clicked() {
                            app.input_text = payload.to_string();
                            let time = ui.ctx().input(|input| input.time);
                            app.submit_input_text(time);
                        }
                        b.on_hover_text(format!("Click para ejecutar: {}", payload));
                    }
                });
        });
    snapshot.save_if_semantically_changed(
        &mut app.document,
        &mut app.undo_stack,
        &mut app.redo_stack,
    );
}

/// Panel izquierdo de Atractores y Dinámica.
pub(crate) fn draw_attractor_panel(app: &mut GrafitoApp, ctx: &egui::Context) {
    use grafito_core::GeoObject;
    let (_is_dark, accent, alg_fill, _sep_col, txt_col, txt_dim, hdr_col) = panel_theme_local(ctx);

    egui::SidePanel::left("attractor_panel").show_separator_line(false)
        .default_width(260.0)
        .min_width(180.0)
        .resizable(true)
        .frame(
            egui::Frame::none()
                .fill(alg_fill)
                .stroke(egui::Stroke::NONE),
        )
        .show(ctx, |ui| {
            ui.add_space(SPACE_SM);
            ui.label(
                egui::RichText::new("Dinámica y Atractores")
                    .color(accent)
                    .size(15.0)
                    .strong(),
            );
            ui.add_space(SPACE_SM);

            draw_object_cards_where(
                ui,
                app,
                "Objetos dinámicos",
                "Sin objetos dinámicos.\nProbá Attractor[10, 28, 8/3].",
                |obj| {
                    matches!(
                        obj,
                        GeoObject::Attractor3D(_)
                            | GeoObject::PhasePortrait(_)
                            | GeoObject::VectorField2D(_)
                            | GeoObject::VectorField3D(_)
                    )
                },
            );
            ui.add_space(SPACE_SM);

            let ids: Vec<_> = app.document.objects_iter().map(|(id, _)| *id).collect();
            let mut attractor_id = None;
            for id in &ids {
                if let Some(GeoObject::Attractor3D(_)) = app.document.get_object(*id) {
                    attractor_id = Some(*id);
                    break;
                }
            }

            if let Some(id) = attractor_id {
                ui.label(egui::RichText::new("Attractor activo").color(hdr_col).size(TYPE_SM).strong());
                if let Some(GeoObject::Attractor3D(a)) = app.document.get_object(id) {
                    let sigma = a.params.first().copied().unwrap_or(0.0);
                    let rho = a.params.get(1).copied().unwrap_or(0.0);
                    let beta = a.params.get(2).copied().unwrap_or(0.0);
                    ui.label(format!("sigma = {:.3}", sigma));
                    ui.label(format!("rho = {:.3}", rho));
                    ui.label(format!("beta = {:.3}", beta));
                    ui.label(format!("dt = {:.4}", a.dt));
                    ui.label(format!("pasos = {}", a.steps));
                }
            } else {
                ui.label(
                    egui::RichText::new(
                        "Sin attractor activo.\nCreá uno con:\n  Attractor[σ, ρ, β]\n(Lorenz por defecto)",
                    )
                    .color(txt_dim)
                    .size(TYPE_XS),
                );
                ui.add_space(6.0);
                if ui
                    .button(egui::RichText::new("Crear Lorenz por defecto").color(accent).strong())
                    .clicked()
                {
                    app.save_state();
                    app.execute_command_and_record("Attractor[10, 28, 8/3]", 0.0);
                }
            }

            ui.add_space(10.0);
            ui.label(egui::RichText::new("Comandos").color(hdr_col).size(TYPE_SM).strong());
            ui.label(egui::RichText::new("- Lorenz: Attractor[sigma, rho, beta]").color(txt_dim).size(TYPE_XS).monospace());
            let _ = txt_col;
        });
}

// ══════════════════════════════════════════════════════════════════════════
// Paneles derechos (Fase 3)
// ══════════════════════════════════════════════════════════════════════════

/// Panel derecho: Propiedades del objeto seleccionado (Geometry3D).
pub(crate) fn draw_right_properties_panel(app: &mut GrafitoApp, ctx: &egui::Context) {
    let theme = current_theme(ctx);
    egui::SidePanel::right("right_properties")
        .show_separator_line(false)
        .default_width(340.0)
        .min_width(292.0)
        .resizable(true)
        .frame(
            egui::Frame::none()
                .fill(theme.panel_bg)
                .stroke(egui::Stroke::NONE),
        )
        .show(ctx, |ui| {
            ui.add_space(SPACE_SM);
            draw_right_drawer_header(ui, app, "Inspector", theme.accent);
            ui.add_space(SPACE_SM);
            draw_right_properties_contents(app, ui);
        });
}

/// Contenido reutilizable del Inspector de propiedades para un dock anfitrión.
pub(crate) fn draw_right_properties_contents(app: &mut GrafitoApp, ui: &mut egui::Ui) {
    let mut snapshot = crate::app::DeferredPanelSnapshot::new(app.undo_stack.len());
    use grafito_core::GeoObject;
    let (_is_dark, _accent, _alg_fill, _sep_col, txt_col, txt_dim, _hdr_col) =
        panel_theme_local(ui.ctx());
    let theme = current_theme(ui.ctx());

    egui::ScrollArea::vertical()
                .id_salt("right_properties_scroll")
                .auto_shrink([false, true])
                .show(ui, |ui| {
                    let Some(id) = app.selected_object else {
                        draw_inspector_empty_state(ui);
                        return;
                    };
                    let Some(mut edited_object) = app.document.get_object(id).cloned() else {
                        ui.label(egui::RichText::new("Objeto inexistente.").color(txt_dim));
                        return;
                    };
                    let object_name = edited_object.name().to_string();
                    let object_label = edited_object.label().to_string();
                    let object_visible = edited_object.is_visible();
                    draw_inspector_identity(ui, &object_name, &object_label, object_visible);
                    ui.add_space(SPACE_MD);
                    let mut changed = false;

                    let label_col = theme.text_secondary;
                    match &mut edited_object {
                GeoObject::Cube3D(c) => {
                    ui.label(egui::RichText::new("Cubo 3D").color(label_col).strong());
                    ui.label(egui::RichText::new(format!("Etiqueta: {}", c.label)).color(txt_col));
                    ui.add_space(SPACE_XS);
                    ui.label(egui::RichText::new("Centro").color(txt_dim));
                    ui.horizontal(|ui| {
                        changed |= ui
                            .add(
                                egui::DragValue::new(&mut c.center.x)
                                    .speed(0.1)
                                    .prefix("x="),
                            )
                            .changed();
                        changed |= ui
                            .add(
                                egui::DragValue::new(&mut c.center.y)
                                    .speed(0.1)
                                    .prefix("y="),
                            )
                            .changed();
                        changed |= ui
                            .add(
                                egui::DragValue::new(&mut c.center.z)
                                    .speed(0.1)
                                    .prefix("z="),
                            )
                            .changed();
                    });
                    changed |= ui
                        .add(egui::Slider::new(&mut c.size, 0.1..=10.0).text("tamaño"))
                        .changed();
                }
                GeoObject::Sphere3D(s) => {
                    ui.label(egui::RichText::new("Esfera 3D").color(label_col).strong());
                    ui.label(egui::RichText::new(format!("Etiqueta: {}", s.label)).color(txt_col));
                    ui.add_space(SPACE_XS);
                    ui.label(egui::RichText::new("Centro").color(txt_dim));
                    ui.horizontal(|ui| {
                        changed |= ui
                            .add(
                                egui::DragValue::new(&mut s.center.x)
                                    .speed(0.1)
                                    .prefix("x="),
                            )
                            .changed();
                        changed |= ui
                            .add(
                                egui::DragValue::new(&mut s.center.y)
                                    .speed(0.1)
                                    .prefix("y="),
                            )
                            .changed();
                        changed |= ui
                            .add(
                                egui::DragValue::new(&mut s.center.z)
                                    .speed(0.1)
                                    .prefix("z="),
                            )
                            .changed();
                    });
                    changed |= ui
                        .add(egui::Slider::new(&mut s.radius, 0.1..=10.0).text("radio"))
                        .changed();
                }
                GeoObject::Point3D(p) => {
                    ui.label(egui::RichText::new("Punto 3D").color(label_col).strong());
                    ui.label(egui::RichText::new(format!("Etiqueta: {}", p.label)).color(txt_col));
                    ui.add_space(SPACE_XS);
                    ui.horizontal(|ui| {
                        changed |= ui
                            .add(
                                egui::DragValue::new(&mut p.position.x)
                                    .speed(0.1)
                                    .prefix("x="),
                            )
                            .changed();
                        changed |= ui
                            .add(
                                egui::DragValue::new(&mut p.position.y)
                                    .speed(0.1)
                                    .prefix("y="),
                            )
                            .changed();
                        changed |= ui
                            .add(
                                egui::DragValue::new(&mut p.position.z)
                                    .speed(0.1)
                                    .prefix("z="),
                            )
                            .changed();
                    });
                    changed |= ui
                        .add(egui::Slider::new(&mut p.size, 1.0..=20.0).text("tamaño"))
                        .changed();
                }
                GeoObject::RegularPolychoron4D(polychoron) => {
                    ui.push_id(("regular_polychoron_4d", id), |ui| {
                        draw_inspector_section(
                            ui,
                            "Proyección",
                            "Controlá la vista dinámica sin alterar la construcción.",
                            |ui| {
                                draw_multidimensional_motion_card(
                                    ui,
                                    app,
                                    "Animación de proyección",
                                    "La cámara y el politopo 4D giran sin alterar el documento.",
                                    polychoron.visible && app.current_view == crate::ViewMode::D3,
                                );
                            },
                        );
                        ui.add_space(SPACE_MD);
                        draw_inspector_section(ui, "Geometría", "Forma y escala", |ui| {
                        let mut kind = polychoron.kind;
                        let combo_width = ui.available_width();
                        egui::ComboBox::from_id_salt("regular_polychoron_kind")
                            .width(combo_width)
                            .selected_text(match kind {
                                RegularPolychoron::Pentachoron => "Pentácoron (5-celda)",
                                RegularPolychoron::Tesseract => "Teseracto",
                                RegularPolychoron::SixteenCell => "16-celda",
                                RegularPolychoron::TwentyFourCell => "24-celda",
                                RegularPolychoron::OneTwentyCell => "120-celda",
                                RegularPolychoron::SixHundredCell => "600-celda",
                            })
                            .show_ui(ui, |ui| {
                                ui.selectable_value(
                                    &mut kind,
                                    RegularPolychoron::Pentachoron,
                                    "Pentácoron (5-celda)",
                                );
                                ui.selectable_value(
                                    &mut kind,
                                    RegularPolychoron::Tesseract,
                                    "Teseracto",
                                );
                                ui.selectable_value(
                                    &mut kind,
                                    RegularPolychoron::SixteenCell,
                                    "16-celda",
                                );
                                ui.selectable_value(
                                    &mut kind,
                                    RegularPolychoron::TwentyFourCell,
                                    "24-celda",
                                );
                                ui.selectable_value(
                                    &mut kind,
                                    RegularPolychoron::OneTwentyCell,
                                    "120-celda",
                                );
                                ui.selectable_value(
                                    &mut kind,
                                    RegularPolychoron::SixHundredCell,
                                    "600-celda",
                                );
                            });
                        if kind != polychoron.kind {
                            polychoron.kind = kind;
                            changed = true;
                        }

                        changed |= ui
                            .add(egui::Slider::new(&mut polychoron.scale, 0.01..=10.0).text("Escala"))
                            .changed();
                        changed |= ui
                            .add(
                                egui::Slider::new(&mut polychoron.width, 0.5..=10.0)
                                    .text("Grosor de aristas"),
                            )
                            .changed();
                        });

                        ui.add_space(SPACE_MD);
                        draw_inspector_section(ui, "Apariencia", "Estilo de aristas y relleno", |ui| {
                        ui.horizontal(|ui| {
                            ui.label("Color de aristas");
                            if color_picker_swatch(
                                ui,
                                polychoron.color,
                                "Cambiar color de aristas",
                            )
                            .clicked()
                            {
                                app.open_object_color_picker(id);
                            }
                        });

                        let mut fill_enabled = polychoron.fill_color.is_some();
                        if ui.checkbox(&mut fill_enabled, "Relleno habilitado").changed() {
                            polychoron.fill_color = fill_enabled.then(|| {
                                polychoron
                                    .fill_color
                                    .unwrap_or(Color::new(0.2, 0.5, 0.9, 0.55))
                            });
                            changed = true;
                        }
                        if let Some(fill_color) = polychoron.fill_color {
                            ui.horizontal(|ui| {
                                ui.label("Color de relleno");
                                if color_picker_swatch(
                                    ui,
                                    fill_color,
                                    "Cambiar color de relleno",
                                )
                                .clicked()
                                {
                                    app.open_regular_polychoron_fill_color_picker(id);
                                }
                            });
                        }
                        ui.label(
                            egui::RichText::new(
                                "El relleno se omite en Vista previa y durante el movimiento.",
                            )
                            .color(txt_dim)
                            .size(TYPE_XS),
                        );
                        });

                        ui.add_space(SPACE_MD);
                        egui::CollapsingHeader::new("Rotación manual")
                            .default_open(false)
                            .show(ui, |ui| {
                                ui.label(
                                    egui::RichText::new(
                                        "Ajustá los seis planos sólo cuando necesites una vista específica.",
                                    )
                                    .color(txt_dim)
                                    .size(TYPE_SM),
                                );
                                ui.add_space(SPACE_SM);
                                if ui.button("Restablecer rotaciones").clicked()
                                    && polychoron.rotation_angles != [0.0; 6]
                                {
                                    polychoron.rotation_angles = [0.0; 6];
                                    changed = true;
                                }
                                ui.label(
                                    egui::RichText::new("Planos de rotación")
                                        .color(txt_dim)
                                        .size(TYPE_SM),
                                );
                                egui::Grid::new("regular_polychoron_rotation_planes").show(ui, |ui| {
                                    for (angle, plane) in polychoron.rotation_angles.iter_mut().zip([
                                        "xy (rad)",
                                        "xz (rad)",
                                        "xw (rad)",
                                        "yz (rad)",
                                        "yw (rad)",
                                        "zw (rad)",
                                    ]) {
                                        ui.label(egui::RichText::new(plane).monospace().size(TYPE_SM));
                                        changed |= ui
                                            .add(
                                                egui::Slider::new(
                                                    angle,
                                                    -std::f64::consts::PI..=std::f64::consts::PI,
                                                )
                                                .show_value(false)
                                                .trailing_fill(true),
                                            )
                                            .changed();
                                        changed |= ui
                                            .add(
                                                egui::DragValue::new(angle)
                                                    .speed(0.01)
                                                    .range(
                                                        -std::f64::consts::PI
                                                            ..=std::f64::consts::PI,
                                                    )
                                                    .fixed_decimals(2),
                                            )
                                            .changed();
                                        ui.end_row();
                                    }
                                });
                            });
                    });
                }
                GeoObject::RegularPolytopeND(polytope) => {
                    ui.push_id(("regular_polytope_nd", id), |ui| {
                        draw_inspector_section(ui, "Geometría", "Familia, dimensión y escala", |ui| {
                        let mut family = polytope.family;
                        let combo_width = ui.available_width();
                        egui::ComboBox::from_id_salt("regular_polytope_nd_family")
                            .width(combo_width)
                            .selected_text(match family {
                                RegularPolytopeFamily::Simplex => "Símplex",
                                RegularPolytopeFamily::Hypercube => "Hipercubo",
                                RegularPolytopeFamily::CrossPolytope => "Politopo cruzado",
                            })
                            .show_ui(ui, |ui| {
                                ui.selectable_value(
                                    &mut family,
                                    RegularPolytopeFamily::Simplex,
                                    "Símplex",
                                );
                                ui.selectable_value(
                                    &mut family,
                                    RegularPolytopeFamily::Hypercube,
                                    "Hipercubo",
                                );
                                ui.selectable_value(
                                    &mut family,
                                    RegularPolytopeFamily::CrossPolytope,
                                    "Politopo cruzado",
                                );
                            });
                        if family != polytope.family {
                            polytope.family = family;
                            changed = true;
                        }

                        let mut dimension = polytope.dimension;
                        if ui
                            .add(egui::Slider::new(&mut dimension, 3..=10).text("Dimensión"))
                            .changed()
                        {
                            if let Some(rotation_count) =
                                RegularPolytopeNDObj::expected_rotation_angle_count(dimension)
                            {
                                polytope.dimension = dimension;
                                polytope.rotation_angles = vec![0.0; rotation_count];
                                changed = true;
                            }
                        }
                        changed |= ui
                            .add(egui::Slider::new(&mut polytope.scale, 0.01..=10.0).text("Escala"))
                            .changed();
                        changed |= ui
                            .add(
                                egui::Slider::new(&mut polytope.width, 0.5..=10.0)
                                    .text("Grosor de aristas"),
                            )
                            .changed();
                        });

                        if polytope.dimension == 4 {
                            ui.add_space(SPACE_MD);
                            draw_inspector_section(
                                ui,
                                "Proyección",
                                "La cámara y la proyección 4D comparten velocidad.",
                                |ui| {
                                    draw_multidimensional_motion_card(
                                        ui,
                                        app,
                                        "Animación de proyección",
                                        "La cámara y la proyección 4D usan la misma velocidad.",
                                        polytope.visible && app.current_view == crate::ViewMode::D3,
                                    );
                                },
                            );
                        }

                        ui.add_space(SPACE_MD);
                        draw_inspector_section(ui, "Apariencia", "Estilo de aristas", |ui| {
                        ui.horizontal(|ui| {
                            ui.label("Color de aristas");
                            if color_picker_swatch(
                                ui,
                                polytope.color,
                                "Cambiar color de aristas",
                            )
                            .clicked()
                            {
                                app.open_object_color_picker(id);
                            }
                        });
                        ui.label(
                            egui::RichText::new(
                                "Los politopos N-D genéricos se muestran solo como aristas; el relleno se omite en Vista previa y durante el movimiento.",
                            )
                            .color(txt_dim)
                            .size(TYPE_XS),
                        );
                        });

                        ui.add_space(SPACE_MD);
                        egui::CollapsingHeader::new("Rotación manual")
                            .default_open(false)
                            .show(ui, |ui| {
                                ui.label(
                                    egui::RichText::new(
                                        "Los ajustes avanzados permanecen separados de los controles principales.",
                                    )
                                    .color(txt_dim)
                                    .size(TYPE_SM),
                                );
                                ui.add_space(SPACE_SM);
                                if ui.button("Restablecer rotaciones").clicked()
                                    && polytope.rotation_angles.iter().any(|angle| *angle != 0.0)
                                {
                                    polytope.rotation_angles.fill(0.0);
                                    changed = true;
                                }
                                ui.label(
                                    egui::RichText::new("Planos de rotación")
                                        .color(txt_dim)
                                        .size(TYPE_SM),
                                );
                                let rotation_planes: Vec<_> = (0..polytope.dimension)
                                    .flat_map(|first| {
                                        ((first + 1)..polytope.dimension)
                                            .map(move |second| (first, second))
                                    })
                                    .collect();
                                egui::ScrollArea::vertical()
                                    .id_salt("regular_polytope_nd_rotation_planes")
                                    .max_height(260.0)
                                    .show(ui, |ui| {
                                        for ((first, second), angle) in rotation_planes
                                            .into_iter()
                                            .zip(polytope.rotation_angles.iter_mut())
                                        {
                                            changed |= ui
                                                .add(
                                                    egui::Slider::new(
                                                        angle,
                                                        -std::f64::consts::PI
                                                            ..=std::f64::consts::PI,
                                                    )
                                                    .text(format!(
                                                        "x{}/x{} (rad)",
                                                        first + 1,
                                                        second + 1
                                                    )),
                                                )
                                                .changed();
                                        }
                                    });
                            });
                    });
                }
                other => {
                    ui.label(format!("Tipo: {}", other.name()));
                    ui.label(
                        egui::RichText::new("Propiedades dedicadas en panel de Álgebra.")
                            .color(txt_dim)
                            .size(TYPE_XS),
                    );
                }
                    }
                    match apply_object_panel_edit_with_previous(&mut app.document, id, changed, move |object| {
                        *object = edited_object;
                    }) {
                        Ok(Some(before)) => snapshot.capture_successful_replacement(before),
                        Ok(None) => {}
                        Err(error) => {
                            let message = format!("Propiedades: {error}");
                            ui.label(
                                egui::RichText::new(&message)
                                    .color(current_theme(ui.ctx()).danger)
                                    .size(TYPE_XS),
                            );
                            app.cas_result = message.clone();
                            app.notify(message, grafito_ui::toast::ToastKind::Error);
                        }
                    }
                });
    let _ = snapshot.save_if_semantically_changed(
        &mut app.document,
        &mut app.undo_stack,
        &mut app.redo_stack,
    );
}

fn set_domain_coloring_mode(document: &mut Document, id: ObjectId, mode: u8) -> bool {
    let needs_update = matches!(
        document.get_object(id),
        Some(GeoObject::ComplexGrid(grid)) if grid.domain_coloring_mode != mode
    );
    if !needs_update {
        return false;
    }
    if let Some(GeoObject::ComplexGrid(grid)) = document.get_object_mut(id) {
        grid.domain_coloring_mode = mode;
        true
    } else {
        false
    }
}

fn set_complex_mapping_animation(
    document: &mut Document,
    id: ObjectId,
    animate_homotopy: bool,
    homotopy_speed: f32,
) -> bool {
    let needs_update = matches!(
        document.get_object(id),
        Some(GeoObject::ComplexMapping(mapping))
            if mapping.animate_homotopy != animate_homotopy
                || mapping.homotopy_speed != homotopy_speed
    );
    if !needs_update {
        return false;
    }
    if let Some(GeoObject::ComplexMapping(mapping)) = document.get_object_mut(id) {
        mapping.animate_homotopy = animate_homotopy;
        mapping.homotopy_speed = homotopy_speed;
        true
    } else {
        false
    }
}

/// Panel derecho: Coloración de dominio (Complejos).
pub(crate) fn draw_right_domain_coloring_panel(app: &mut GrafitoApp, ctx: &egui::Context) {
    use grafito_core::GeoObject;
    let (_is_dark, accent, alg_fill, _sep_col, _txt_col, txt_dim, hdr_col) = panel_theme_local(ctx);

    egui::SidePanel::right("right_domain_coloring").show_separator_line(false)
        .default_width(280.0)
        .min_width(200.0)
        .resizable(true)
        .frame(
            egui::Frame::none()
                .fill(alg_fill)
                .stroke(egui::Stroke::NONE),
        )
        .show(ctx, |ui| {
            ui.add_space(SPACE_SM);
            draw_right_drawer_header(ui, app, "Coloración de dominio", accent);
            ui.add_space(6.0);

            let mut has_domain_coloring = false;
            let mut grid_id = None;
            for (id, obj) in app.document.objects_iter() {
                if matches!(obj, GeoObject::ComplexGrid(grid) if grid.render_mode == 1) {
                    has_domain_coloring = true;
                    grid_id = Some(*id);
                    break;
                }
            }

            if !has_domain_coloring {
                ui.label(
                    egui::RichText::new(
                        "Sin coloración de dominio. Creá una con:\n  DomainColoring[1/z, -2, 2, -2, 2, 160]\nFase y módulo de f(z).",
                    )
                    .color(txt_dim)
                    .size(TYPE_XS),
                );
            } else {
                ui.label(egui::RichText::new("Coloración por fase habilitada").color(hdr_col).strong());
                ui.add_space(SPACE_XS);
                ui.label(egui::RichText::new("Tono = arg(f(z)).").color(txt_dim).size(TYPE_XS));

                ui.add_space(6.0);
                ui.collapsing("Guia de Interpretacion", |ui| {
                    let theme = current_theme(ui.ctx());
                    egui::Frame::none()
                        .fill(theme.input_bg)
                        .rounding(egui::Rounding::same(6.0))
                        .inner_margin(egui::Margin::same(8.0))
                        .show(ui, |ui| {
                            ui.vertical(|ui| {
                                ui.label(egui::RichText::new("Colores y Magnitud:").strong().color(hdr_col).size(TYPE_XS));
                                ui.label(egui::RichText::new("- Tono: Fase o angulo arg(f(z)).\n- Brillo: Magnitud |f(z)|. Negro = Raiz (0), Blanco = Polo (inf).").color(txt_dim).size(10.5));

                                ui.add_space(6.0);
                                ui.label(egui::RichText::new("Derivabilidad y Wirtinger:").strong().color(hdr_col).size(TYPE_XS));
                                ui.label(egui::RichText::new("- Al graficar deriv_z_conj(f), f(z) es holomorfa solo en zonas negras (donde d/dzbar = 0, Cauchy-Riemann).\n- Las zonas coloreadas representan donde NO es derivable.").color(txt_dim).size(10.5));
                            });
                        });
                });
            }

            // Selector de modo de coloreado de dominio
            if let Some(id) = grid_id {
                if let Some(GeoObject::ComplexGrid(cg)) = app.document.get_object(id) {
                    ui.add_space(SPACE_SM);
                    ui.label(egui::RichText::new("Modo de coloración").color(hdr_col).size(TYPE_SM).strong());
                    let mut mode = cg.domain_coloring_mode;
                    egui::ComboBox::from_id_salt("dc_mode_combo")
                        .selected_text(match mode {
                            0 => "HSL Clásico (Fase + Módulo)",
                            1 => "Retrato de Fase Puro",
                            2 => "Rejilla Polar Conforme",
                            3 => "Rejilla Cartesiana Conforme",
                            _ => "HSL Clásico",
                        })
                        .show_ui(ui, |ui| {
                            ui.selectable_value(&mut mode, 0, "HSL Clásico (Fase + Módulo)");
                            ui.selectable_value(&mut mode, 1, "Retrato de Fase Puro");
                            ui.selectable_value(&mut mode, 2, "Rejilla Polar Conforme");
                            ui.selectable_value(&mut mode, 3, "Rejilla Cartesiana Conforme");
                        });

                    let _ = set_domain_coloring_mode(&mut app.document, id, mode);
                }
            }

            ui.add_space(10.0);
            ui.label(egui::RichText::new("Símbolo base").color(hdr_col).size(TYPE_SM).strong());
            let mut sym = app.document.complex_base_symbol.clone();
            let r = ui.add(
                egui::TextEdit::singleline(&mut sym)
                    .desired_width(ui.available_width())
                    .hint_text("z"),
            );
            if r.lost_focus() && sym.trim() != app.document.complex_base_symbol {
                let new_sym = sym.trim().to_string();
                if !new_sym.is_empty() {
                    app.document.migrate_complex_symbol(&new_sym);
                    app.document.bump_version();
                }
            }

            // Animación de homotopía si hay algún mapeo complejo
            let mut mapping_id = None;
            for (id, obj) in app.document.objects_iter() {
                if matches!(obj, GeoObject::ComplexMapping(_)) {
                    mapping_id = Some(*id);
                    break;
                }
            }

            if let Some(id) = mapping_id {
                if let Some(GeoObject::ComplexMapping(cm)) = app.document.get_object(id) {
                    ui.add_space(14.0);
                    ui.separator();
                    ui.add_space(SPACE_SM);
                    ui.label(
                        egui::RichText::new("Animación de Mapeo Conforme")
                            .color(accent)
                            .strong(),
                    );
                    ui.add_space(SPACE_XS);

                    let mut anim = cm.animate_homotopy;
                    ui.checkbox(&mut anim, "Animar deformación (homotopía)");

                    let mut speed = cm.homotopy_speed;
                    ui.add(egui::Slider::new(&mut speed, 0.2..=3.0).text("Velocidad"));
                    let _ = set_complex_mapping_animation(&mut app.document, id, anim, speed);
                }
            }
        });
}

/// Panel derecho: Parámetros del attractor activo (Dynamics).
pub(crate) fn draw_right_parameters_panel(app: &mut GrafitoApp, ctx: &egui::Context) {
    use grafito_core::{GeoObject, ObjectId};
    let mut snapshot = crate::app::DeferredPanelSnapshot::new(app.undo_stack.len());
    let (_is_dark, accent, alg_fill, _sep_col, _txt_col, txt_dim, hdr_col) = panel_theme_local(ctx);

    egui::SidePanel::right("right_parameters")
        .show_separator_line(false)
        .default_width(260.0)
        .min_width(180.0)
        .resizable(true)
        .frame(
            egui::Frame::none()
                .fill(alg_fill)
                .stroke(egui::Stroke::NONE),
        )
        .show(ctx, |ui| {
            ui.add_space(SPACE_SM);
            draw_right_drawer_header(ui, app, "Parámetros dinámicos", accent);
            ui.add_space(6.0);

            let mut attractor_id: Option<ObjectId> = None;
            for (id, obj) in app.document.objects_iter() {
                if matches!(obj, GeoObject::Attractor3D(_)) {
                    attractor_id = Some(*id);
                    break;
                }
            }

            let Some(id) = attractor_id else {
                ui.label(
                    egui::RichText::new(
                        "Sin attractor activo. Creá uno con:\n  Attractor[10, 28, 8/3]",
                    )
                    .color(txt_dim)
                    .size(TYPE_XS),
                );
                return;
            };

            let Some(GeoObject::Attractor3D(attractor)) = app.document.get_object(id).cloned()
            else {
                return;
            };
            let mut sigma = attractor.params.first().copied().unwrap_or(0.0);
            let mut rho = attractor.params.get(1).copied().unwrap_or(0.0);
            let mut beta = attractor.params.get(2).copied().unwrap_or(0.0);
            let mut dt = attractor.dt;
            let mut steps = attractor.steps;
            let mut changed = false;

            ui.label(
                egui::RichText::new("Lorenz sigma, rho, beta")
                    .color(hdr_col)
                    .size(TYPE_SM)
                    .strong(),
            );
            ui.add_space(SPACE_XS);
            changed |= ui
                .add(
                    egui::Slider::new(&mut sigma, 0.1..=30.0)
                        .text("σ")
                        .trailing_fill(true),
                )
                .changed();
            changed |= ui
                .add(
                    egui::Slider::new(&mut rho, 0.1..=60.0)
                        .text("ρ")
                        .trailing_fill(true),
                )
                .changed();
            changed |= ui
                .add(
                    egui::Slider::new(&mut beta, 0.1..=10.0)
                        .text("β")
                        .trailing_fill(true),
                )
                .changed();
            ui.add_space(SPACE_XS);
            ui.label(
                egui::RichText::new("Integración")
                    .color(hdr_col)
                    .size(TYPE_SM)
                    .strong(),
            );
            changed |= ui
                .add(
                    egui::Slider::new(&mut dt, 0.001..=0.05)
                        .text("dt")
                        .trailing_fill(true),
                )
                .changed();
            changed |= ui
                .add(
                    egui::Slider::new(&mut steps, 100..=20000)
                        .text("pasos")
                        .trailing_fill(true)
                        .integer(),
                )
                .changed();
            ui.add_space(SPACE_XS);
            ui.label(
                egui::RichText::new("El canvas se regenera cada cambio.")
                    .color(txt_dim)
                    .size(TYPE_XS),
            );

            match apply_object_panel_edit_with_previous(
                &mut app.document,
                id,
                changed,
                move |object| {
                    let GeoObject::Attractor3D(attractor) = object else {
                        return;
                    };
                    attractor.params.resize(3, 0.0);
                    attractor.params[0] = sigma;
                    attractor.params[1] = rho;
                    attractor.params[2] = beta;
                    attractor.dt = dt;
                    attractor.steps = steps;
                },
            ) {
                Ok(Some(before)) => snapshot.capture_successful_replacement(before),
                Ok(None) => {}
                Err(error) => {
                    let message = format!("Parámetros: {error}");
                    ui.label(
                        egui::RichText::new(&message)
                            .color(current_theme(ui.ctx()).danger)
                            .size(TYPE_XS),
                    );
                    app.cas_result = message.clone();
                    app.notify(message, grafito_ui::toast::ToastKind::Error);
                }
            }
        });
    let _ = snapshot.save_if_semantically_changed(
        &mut app.document,
        &mut app.undo_stack,
        &mut app.redo_stack,
    );
}

pub(crate) fn draw_right_regression_panel(app: &mut GrafitoApp, ctx: &egui::Context) {
    let theme = current_theme(ctx);

    egui::SidePanel::right("regression").show_separator_line(false)
        .resizable(true)
        .default_width(280.0)
        .min_width(200.0)
        .frame(
            egui::Frame::none()
                .fill(theme.panel_bg)
                .stroke(egui::Stroke::NONE),
        )
        .show(ctx, |ui| {
            draw_right_drawer_header(ui, app, "Regresión", theme.accent);
            ui.separator();
            egui::ScrollArea::vertical()
                .id_salt("regression_panel_content")
                .auto_shrink([false, false])
                .show(ui, |ui| {
                    draw_object_cards_where(
                        ui,
                        app,
                        "Ajustes del documento",
                        "Sin ajustes todavía.",
                        |object| {
                            matches!(
                                object,
                                GeoObject::DataTable(_)
                                    | GeoObject::ScatterPlot(_)
                                    | GeoObject::RegressionLine(_)
                            ) || matches!(object, GeoObject::Function(function) if function.fit.is_some())
                        },
                    );
                    let fits: Vec<_> = app
                        .document
                        .objects_iter()
                        .filter_map(|(_, object)| match object {
                            GeoObject::Function(function) => function
                                .fit
                                .as_ref()
                                .map(|fit| (function.label.clone(), fit.clone())),
                            _ => None,
                        })
                        .collect();
                    if !fits.is_empty() {
                        ui.add_space(SPACE_SM);
                        ui.label(
                            egui::RichText::new("Diagnósticos locales")
                                .color(theme.text_secondary)
                                .size(TYPE_SM)
                                .strong(),
                        );
                        for (label, fit) in fits {
                            let source_label = app
                                .document
                                .get_object(fit.source)
                                .map(|object| object.label().to_string())
                                .unwrap_or_else(|| "tabla eliminada".to_string());
                            ui.label(
                                egui::RichText::new(format!(
                                    "{label}: {} sobre {source_label} · RMSE={:.6} · R²={:.6}",
                                    fit.kind.display_name(),
                                    fit.diagnostics.rmse,
                                    fit.diagnostics.r_squared
                                ))
                                .color(theme.text_primary)
                                .size(TYPE_XS),
                            );
                            ui.collapsing(
                                format!("Residuales ({})", fit.diagnostics.residuals.len()),
                                |ui| {
                                    let shown = fit.diagnostics.residuals.len().min(24);
                                    for (index, residual) in
                                        fit.diagnostics.residuals.iter().take(shown).enumerate()
                                    {
                                        ui.label(
                                            egui::RichText::new(format!(
                                                "r{} = {:.6}",
                                                index + 1,
                                                residual
                                            ))
                                            .color(theme.text_tertiary)
                                            .size(TYPE_XS),
                                        );
                                    }
                                    if fit.diagnostics.residuals.len() > shown {
                                        ui.label(
                                            egui::RichText::new("Se muestran los primeros 24 valores.")
                                                .color(theme.text_tertiary)
                                                .size(10.0),
                                        );
                                    }
                                },
                            );
                        }
                    }
                    ui.add_space(10.0);
                    ui.label(
                        egui::RichText::new("Crear análisis")
                            .color(theme.text_secondary)
                            .size(TYPE_SM)
                            .strong(),
                    );
                    ui.label(
                        egui::RichText::new(
                            "Importá un CSV/TSV de dos columnas o creá una tabla local desde dos listas. La ruta nunca se guarda.",
                        )
                        .color(theme.text_tertiary)
                        .size(TYPE_XS),
                    );
                    ui.add_space(6.0);

                    if ui.button("Importar CSV/TSV...").clicked() {
                        import_local_xy_table(app);
                    }

                    for (label, template) in [
                        (
                            "Diagrama de dispersión",
                            "ScatterPlot[{1, 2, 3}, {1, 4, 9}]",
                        ),
                        (
                            "Regresión lineal",
                            "LinearRegression[{1, 2, 3}, {1, 4, 9}]",
                        ),
                        (
                            "Tabla local",
                            "DataTable[{0, 1, 2}, {1, 3, 5}]",
                        ),
                    ] {
                        if ui.button(label).clicked() {
                            app.input_text = template.to_string();
                            app.command_input_focus_requested = true;
                        }
                    }

                    let selected_table_label = app.selected_object.and_then(|id| {
                        match app.document.get_object(id) {
                            Some(GeoObject::DataTable(table)) => Some(table.label.clone()),
                            _ => None,
                        }
                    });
                    if let Some(table_label) = selected_table_label {
                        ui.add_space(SPACE_SM);
                        ui.label(
                            egui::RichText::new(format!("Ajustar tabla '{table_label}'"))
                                .color(theme.text_secondary)
                                .size(TYPE_SM)
                                .strong(),
                        );
                        for (label, template) in [
                            ("Lineal", format!("FitLinear[{table_label}]")),
                            ("Polinómico grado 2", format!("FitPoly[{table_label}, 2]")),
                            ("Exponencial", format!("FitExp[{table_label}]")),
                            ("Logarítmico", format!("FitLog[{table_label}]")),
                            ("Potencia", format!("FitPow[{table_label}]")),
                            ("Sinusoidal", format!("FitSin[{table_label}]")),
                        ] {
                            if ui.button(label).clicked() {
                                app.input_text = template;
                                app.command_input_focus_requested = true;
                            }
                        }
                    } else {
                        ui.add_space(6.0);
                        ui.label(
                            egui::RichText::new(
                                "Seleccioná una tabla local para elegir un modelo de ajuste.",
                            )
                            .color(theme.text_tertiary)
                            .size(TYPE_XS),
                        );
                    }
                });
        });
}

// ─────────────────────────────────────────────────────────────────────────
// Protocolo de Construcción (panel derecho, perspectiva Geometry2D)
// ─────────────────────────────────────────────────────────────────────────

/// Escapa caracteres especiales de LaTeX.
fn escape_latex(s: &str) -> String {
    s.replace('\\', "\\textbackslash{}")
        .replace('_', "\\_")
        .replace('%', "\\%")
        .replace('&', "\\&")
        .replace('#', "\\#")
        .replace('$', "\\$")
        .replace('{', "\\{")
        .replace('}', "\\}")
}

/// Genera una lista enumerada en LaTeX a partir del registro de construcción.
fn construction_log_to_latex(log: &[crate::app::ConstructionStep]) -> String {
    let mut s = String::new();
    s.push_str("% Protocolo de Construcción — Grafito\n");
    s.push_str("\\begin{enumerate}\n");
    for step in log {
        let inputs = if step.inputs.is_empty() {
            "\\textemdash".to_string()
        } else {
            step.inputs.join(", ")
        };
        let output = if step.output.is_empty() {
            "\\textemdash".to_string()
        } else {
            step.output.clone()
        };
        let disabled = if step.disabled {
            " (deshabilitado)"
        } else {
            ""
        };
        s.push_str(&format!(
            "  \\item {}{}: {} $\\rightarrow$ {}\n",
            escape_latex(&step.action),
            disabled,
            escape_latex(&inputs),
            escape_latex(&output),
        ));
    }
    s.push_str("\\end{enumerate}\n");
    s
}

pub(crate) fn draw_construction_protocol(app: &mut GrafitoApp, ctx: &egui::Context) {
    if !app.show_construction_protocol {
        return;
    }
    let (_is_dark, accent, alg_fill, _sep_col, txt_col, txt_dim, _hdr_col) = panel_theme_local(ctx);

    egui::SidePanel::right("construction_protocol").show_separator_line(false)
        .resizable(true)
        .default_width(300.0)
        .min_width(200.0)
        .frame(
            egui::Frame::none()
                .fill(alg_fill)
                .stroke(egui::Stroke::NONE),
        )
        .show(ctx, |ui| {
            ui.add_space(SPACE_SM);
            ui.horizontal(|ui| {
                ui.add_space(SPACE_SM);
                draw_right_drawer_header(ui, app, "Protocolo de Construcción", accent);
            });
            ui.add_space(2.0);
            ui.separator();

            // Toolbar: exportar LaTeX + limpiar
            egui::Frame::none()
                .inner_margin(egui::Margin::symmetric(8.0, 4.0))
                .show(ui, |ui| {
                    ui.horizontal(|ui| {
                        if ui.button("Exportar LaTeX").clicked() {
                            let latex = construction_log_to_latex(&app.construction_log);
                            if let Some(path) =
                                rfd::FileDialog::new().add_filter("TeX", &["tex"]).save_file()
                            {
                                if let Err(e) = crate::export::write_text_atomic(&path, &latex) {
                                    app.cas_result =
                                        format!("No se pudo exportar el protocolo: {e}");
                                    app.notify(
                                        format!("Error LaTeX: {}", e),
                                        grafito_ui::toast::ToastKind::Error,
                                    );
                                } else {
                                    app.cas_result = format!(
                                        "Protocolo exportado a LaTeX -> {}",
                                        path.display()
                                    );
                                    app.notify(
                                        app.cas_result.clone(),
                                        grafito_ui::toast::ToastKind::Success,
                                    );
                                }
                            }
                        }
                        if ui.button("Limpiar").clicked() {
                            app.construction_log.clear();
                        }
                    });
                });
            ui.separator();

            // El protocolo es una vista fiel del historial. Reordenar o
            // desactivar sólo su texto no modifica restricciones reales, por
            // eso esos controles no se presentan como acciones disponibles.
            egui::ScrollArea::vertical()
                .max_height(ui.available_height() - 8.0)
                .show(ui, |ui| {
                    if app.construction_log.is_empty() {
                        ui.label(
                            egui::RichText::new(
                                "Sin pasos de construcción.\nCrea objetos o restricciones para verlos aquí.",
                            )
                            .size(TYPE_SM)
                            .color(txt_dim),
                        );
                    } else {
                        let total = app.construction_log.len();
                        for i in 0..total {
                            let (n, action, inputs, output, disabled) = {
                                let step = &app.construction_log[i];
                                (
                                    step.n,
                                    step.action.clone(),
                                    step.inputs.clone(),
                                    step.output.clone(),
                                    step.disabled,
                                )
                            };
                            let inputs_str =
                                if inputs.is_empty() { "—".to_string() } else { inputs.join(", ") };
                            let output_str =
                                if output.is_empty() { "—".to_string() } else { output };
                            let bg = if disabled {
                                _sep_col.gamma_multiply(0.10)
                            } else {
                                Color32::TRANSPARENT
                            };
                            egui::Frame::none()
                                .fill(bg)
                                .inner_margin(egui::Margin::symmetric(8.0, 4.0))
                                .show(ui, |ui| {
                                    ui.horizontal(|ui| {
                                        ui.label(
                                            egui::RichText::new(format!("{}", n))
                                                .color(accent)
                                                .strong(),
                                        );
                                        ui.vertical(|ui| {
                                            ui.horizontal(|ui| {
                                                ui.label(
                                                    egui::RichText::new(&action)
                                                        .color(txt_col)
                                                        .strong()
                                                        .size(TYPE_SM),
                                                );
                                                if disabled {
                                                    ui.label(
                                                        egui::RichText::new("(deshabilitado)")
                                                            .color(txt_dim)
                                                            .size(10.0),
                                                    );
                                                }
                                            });
                                            ui.label(
                                                egui::RichText::new(format!(
                                                    "{} -> {}",
                                                    inputs_str, output_str
                                                ))
                                                .size(TYPE_XS)
                                                .color(txt_dim),
                                            );
                                        });
                                    });
                                });
                            ui.add_space(2.0);
                        }
                    }
                });

        });
}

// Perspectiva Mascota eliminada — avatar personalizable vive en Configuración
#[allow(dead_code)] // TODO: picks legacy tras migración a Configuración unificada (mantener para compat, usado en tests legacy)
pub(crate) fn draw_mascota_panel(_app: &mut GrafitoApp, _ctx: &egui::Context) {}
#[allow(dead_code)] // TODO: picks legacy tras migración a Configuración unificada (mantener para compat, usado en tests legacy)
pub(crate) fn draw_right_mascota_panel(_app: &mut GrafitoApp, _ctx: &egui::Context) {}

#[cfg(test)]
mod statistics_interpolation_tests {
    use super::stable_interpolate;

    #[test]
    fn interpolation_preserves_subnormals_and_opposite_sign_extremes() {
        let minimum_subnormal = f64::from_bits(1);

        assert_eq!(
            stable_interpolate(minimum_subnormal, minimum_subnormal, 0.5),
            minimum_subnormal
        );
        assert_eq!(
            stable_interpolate(minimum_subnormal, f64::from_bits(2), 0.5),
            f64::from_bits(2)
        );
        assert_eq!(stable_interpolate(-2.0, 2.0, 0.5), 0.0);
        assert_eq!(stable_interpolate(-f64::MAX, f64::MAX, 0.5), 0.0);
        assert_eq!(stable_interpolate(f64::MAX, f64::MAX, 0.5), f64::MAX);
    }
}

#[cfg(test)]
mod domain_coloring_mutation_tests {
    use super::{set_complex_mapping_animation, set_domain_coloring_mode};
    use grafito_core::{ComplexGridObj, ComplexMappingObj, Document, GeoObject};

    #[test]
    fn unchanged_domain_coloring_mode_does_not_dirty_the_document() {
        let mut document = Document::new();
        let grid_id = document
            .try_add_object(GeoObject::ComplexGrid(ComplexGridObj::new(
                "z", -5.0, 5.0, -5.0, 5.0,
            )))
            .unwrap();
        let revision = document.version;

        assert!(!set_domain_coloring_mode(&mut document, grid_id, 0));
        assert_eq!(document.version, revision);

        assert!(set_domain_coloring_mode(&mut document, grid_id, 1));
        assert_eq!(document.version, revision + 1);
    }

    #[test]
    fn unchanged_mapping_animation_does_not_dirty_the_document() {
        let mut document = Document::new();
        let grid_id = document
            .try_add_object(GeoObject::ComplexGrid(ComplexGridObj::new(
                "z", -5.0, 5.0, -5.0, 5.0,
            )))
            .unwrap();
        let mapping_id = document
            .try_add_object(GeoObject::ComplexMapping(ComplexMappingObj::new(
                "z^2", grid_id,
            )))
            .unwrap();
        let revision = document.version;

        assert!(!set_complex_mapping_animation(
            &mut document,
            mapping_id,
            false,
            1.0,
        ));
        assert_eq!(document.version, revision);

        assert!(set_complex_mapping_animation(
            &mut document,
            mapping_id,
            true,
            1.5,
        ));
        assert_eq!(document.version, revision + 1);
    }
}
