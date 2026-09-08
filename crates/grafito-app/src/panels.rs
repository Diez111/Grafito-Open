//! Paneles laterales removibles e inspectores (CAS, vista, estadística, propiedades).

use crate::export::{
    copy_png_to_os_clipboard, datatable_csv_text, sanitize_export_stem, spawn_csv_export,
    spawn_pdf_export,
};
use crate::GrafitoApp;
use egui::Color32;
use grafito_core::symbolic::{clipboard_svg, series as spreadsheet_series, LayerTable, MAX_LAYERS};
use grafito_core::{
    CasWorksheetStatus, ChangeSet, DataTableObj, Document, GeoObject, ObjectId,
    RegularPolytopeNDObj, ScatterPlotObj,
};
use grafito_geometry::{Color, RegularPolychoron, RegularPolytopeFamily};
use grafito_ui::icons::{action_icon_button, Icon};
use grafito_ui::theme::{current_theme, DARK, LIGHT};
use grafito_ui::tokens::{
    CARD_SPACING, DRAWER_RIGHT_MAX, DRAWER_RIGHT_MIN, PANEL_LEFT_DEFAULT, PANEL_LEFT_MAX_FRACTION,
    PANEL_LEFT_MIN, RADIUS_LG, RADIUS_MD, RADIUS_PILL, RADIUS_SM, SPACE_LG, SPACE_MD, SPACE_SM,
    SPACE_XS, TYPE_BASE, TYPE_LG, TYPE_MD, TYPE_SM, TYPE_XS, ZOOM_ICON_HIT,
};
use std::collections::{HashMap, VecDeque};
use std::fs::File;
use std::io::Read;
use std::path::Path;

const MAX_LOCAL_DATA_IMPORT_BYTES: usize = 2_000_000;

/// Estado del panel Capas (oleada M): `LayerTable` del core + capa elegida
/// para asignar. Vive en la memoria temporal de egui (sin I/O, sin campos
/// nuevos en `GrafitoApp`); las ids de documentos cerrados se podan al
/// dibujar vía [`LayerTable::prune_missing`].
#[derive(Debug, Clone, Default)]
struct LayerPanelState {
    table: LayerTable,
    selected_layer: u32,
}

/// Botón pill centrado de ancho completo para la sección Exportación
/// (mismo estilo que el histórico "Exportar SVG").
fn export_pill_button(ui: &mut egui::Ui, label: &str) -> egui::Response {
    let theme = current_theme(ui.ctx());
    let btn = egui::Button::new(
        egui::RichText::new(label)
            .size(TYPE_SM)
            .strong()
            .color(theme.keyboard_enter_text),
    )
    .fill(theme.keyboard_enter_bg)
    .stroke(egui::Stroke::NONE)
    .rounding(RADIUS_PILL);
    ui.add_sized([ui.available_width(), ZOOM_ICON_HIT], btn)
}

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

pub(crate) fn load_local_xy_table(path: &Path) -> Result<LocalXYTable, String> {
    // La lectura vive en worker (`spawn_csv_import`); esta función es pura I/O
    // acotada: symlink_metadata + O_NOFOLLOW, take() 2MB, sin unwrap/panic.
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

pub(crate) fn commit_local_xy_table(
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

fn import_local_xy_table(app: &mut GrafitoApp, ctx: &egui::Context) {
    // FileController: el diálogo rfd queda en UI thread (modal nativo);
    // la lectura ≤2MB va a worker y el commit se aplica en `poll_background_jobs`.
    let Some(path) = rfd::FileDialog::new()
        .add_filter("Datos CSV o TSV", &["csv", "tsv", "txt"])
        .pick_file()
    else {
        return;
    };
    app.pending_import_job = Some(crate::app::PendingImportJob {
        receiver: crate::app::spawn_csv_import(path, ctx),
    });
    app.cas_result = "Importando tabla local…".to_string();
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

/// Ecuación/fórmula display-only del objeto para el Inspector (frente D1).
/// Pura y testeable headless. D3 alimenta el campo de EDICIÓN; este getter
/// solo expone lo que el objeto YA trae para mostrarlo en grande.
/// `None` → placeholder honesto, jamás el nombre del tipo en grande.
pub(crate) fn inspector_equation_text(obj: &GeoObject) -> Option<String> {
    fn non_empty(raw: &str) -> Option<String> {
        let trimmed = raw.trim();
        (!trimmed.is_empty()).then(|| trimmed.to_string())
    }
    match obj {
        GeoObject::Function(f) => non_empty(&f.expr).map(|expr| format!("y = {expr}")),
        GeoObject::ImplicitCurve(c) => {
            let lhs = c.expr_lhs.trim();
            let rhs = c.expr_rhs.trim();
            let op = match c.operator {
                grafito_core::RelationOperator::Eq => "=",
                grafito_core::RelationOperator::Less => "<",
                grafito_core::RelationOperator::Greater => ">",
                grafito_core::RelationOperator::LessEq => "≤",
                grafito_core::RelationOperator::GreaterEq => "≥",
            };
            match (lhs.is_empty(), rhs.is_empty()) {
                (true, true) => None,
                (true, false) => Some(rhs.to_string()),
                (false, true) => Some(lhs.to_string()),
                (false, false) => Some(format!("{lhs} {op} {rhs}")),
            }
        }
        GeoObject::ParametricCurve2D(c) => {
            let (x, y) = (c.expr_x.trim(), c.expr_y.trim());
            if x.is_empty() && y.is_empty() {
                None
            } else {
                Some(format!("({x}, {y})"))
            }
        }
        GeoObject::ParametricCurve3D(c) => {
            let (x, y, z) = (c.expr_x.trim(), c.expr_y.trim(), c.expr_z.trim());
            if x.is_empty() && y.is_empty() && z.is_empty() {
                None
            } else {
                Some(format!("({x}, {y}, {z})"))
            }
        }
        GeoObject::PolarCurve(c) => non_empty(&c.expr_r).map(|expr| format!("r = {expr}")),
        GeoObject::Surface3D(s) => {
            if s.is_parametric {
                let (x, y, z) = (s.expr_x.trim(), s.expr_y.trim(), s.expr_z.trim());
                if x.is_empty() && y.is_empty() && z.is_empty() {
                    None
                } else {
                    Some(format!("({x}, {y}, {z})"))
                }
            } else if s.is_complex {
                non_empty(&s.expr).map(|expr| format!("|{expr}|"))
            } else {
                non_empty(&s.expr).map(|expr| format!("z = {expr}"))
            }
        }
        _ => None,
    }
}

/// Caption terciario del tipo de objeto, en español rioplatense.
/// El nombre muerto (`Surface3D`, …) nunca va en grande: solo acá,
/// en `TYPE_XS`. Puro y testeable headless.
pub(crate) fn inspector_type_caption(obj: &GeoObject) -> &'static str {
    match obj {
        GeoObject::Point(_) => "punto",
        GeoObject::Line(_) => "recta",
        GeoObject::Circle(_) => "círculo",
        GeoObject::Polygon(_) => "polígono",
        GeoObject::Polyline(_) => "polilínea",
        GeoObject::Pencil(p) if p.is_dynamic_locus() => "lugar geométrico",
        GeoObject::Pencil(_) => "trazo",
        GeoObject::Function(_) => "función",
        GeoObject::Text(_) => "texto",
        GeoObject::Ellipse(_) => "elipse",
        GeoObject::Parabola(_) => "parábola",
        GeoObject::Hyperbola(_) => "hipérbola",
        GeoObject::Arc(_) => "arco",
        GeoObject::Sector(_) => "sector",
        GeoObject::BezierCurve(_) => "curva Bézier",
        GeoObject::Spline(_) => "spline",
        GeoObject::Point3D(_) => "punto 3D",
        GeoObject::Segment3D(_) => "segmento 3D",
        GeoObject::Plane3D(_) => "plano 3D",
        GeoObject::Line3D(_) => "recta 3D",
        GeoObject::Sphere3D(_) => "esfera 3D",
        GeoObject::Cube3D(_) => "cubo 3D",
        GeoObject::Tetrahedron3D(_) => "tetraedro 3D",
        GeoObject::Pyramid3D(_) => "pirámide 3D",
        GeoObject::Cone3D(_) => "cono 3D",
        GeoObject::Cylinder3D(_) => "cilindro 3D",
        GeoObject::Torus3D(_) => "toro 3D",
        GeoObject::MoebiusStrip(_) => "cinta de Möbius",
        GeoObject::Surface3D(s) if s.is_complex => "superficie compleja 3D",
        GeoObject::Surface3D(_) => "superficie 3D",
        GeoObject::Prism3D(_) => "prisma 3D",
        GeoObject::Quadric3D(quadric) if grafito_render::quadric_uses_placeholder(quadric) => {
            "cuádrica 3D · vista aproximada"
        }
        GeoObject::Quadric3D(_) => "cuádrica 3D",
        GeoObject::ImplicitSurface3D(_) => "superficie implícita 3D",
        GeoObject::ParametricCurve2D(_) => "curva paramétrica 2D",
        GeoObject::ParametricCurve3D(_) => "curva paramétrica 3D",
        GeoObject::PolarCurve(_) => "curva polar",
        GeoObject::ImplicitCurve(_) => "curva implícita",
        GeoObject::VectorField2D(_) => "campo vectorial 2D",
        GeoObject::ComplexGrid(_) => "grilla compleja",
        GeoObject::ComplexMapping(_) => "mapeo complejo",
        GeoObject::ComplexIntegral(_) => "integral compleja",
        GeoObject::Attractor3D(_) => "atractor 3D",
        GeoObject::Fractal2D(_) => "fractal 2D",
        GeoObject::RegularPolychoron4D(_) => "polícoro regular 4D",
        GeoObject::RegularPolytopeND(_) => "politopo regular N-D",
        GeoObject::HyperSurface4D(_) => "hiperficie 4D",
        GeoObject::VectorField3D(_) => "campo vectorial 3D",
        GeoObject::Histogram(_) => "histograma",
        GeoObject::BarChart(_) => "gráfico de barras",
        GeoObject::PieChart(_) => "gráfico de torta",
        GeoObject::ScatterPlot(_) => "dispersión",
        GeoObject::BoxPlot(_) => "diagrama de caja",
        GeoObject::RegressionLine(_) => "recta de regresión",
        GeoObject::DataTable(_) => "tabla de datos",
        GeoObject::PhasePortrait(_) => "retrato de fase",
        GeoObject::Transformed(_) => "objeto transformado",
        // `GeoObject` es non-exhaustive: variantes futuras caen al nombre
        // honesto en inglés hasta tener caption en español.
        &_ => obj.name(),
    }
}

/// Texto de la fila de etiqueta del Inspector. Nunca devuelve una letra
/// suelta: la etiqueta cruda (p. ej. `S`, autolabel de `Surface3D` según
/// `document.rs:1193-1202` — primera letra del nombre del tipo) se presenta
/// con prefijo `Etiqueta: ` para que no parezca texto truncado.
/// `None` si no hay etiqueta que mostrar. Puro y testeable headless.
pub(crate) fn inspector_label_text(label: &str) -> Option<String> {
    let trimmed = label.trim();
    if trimmed.is_empty() {
        None
    } else {
        Some(format!("Etiqueta: {trimmed}"))
    }
}

/// Tooltip honesto de la card de identidad: texto completo sin truncar.
/// `headline` es la ecuación en grande (o el caption si no hay ecuación).
pub(crate) fn inspector_identity_tooltip(headline: &str, label: &str, visible: bool) -> String {
    let state = if visible { "Visible" } else { "Oculto" };
    match inspector_label_text(label) {
        Some(labeled) => format!("{headline} · {labeled} · {state}"),
        None => format!("{headline} · {state}"),
    }
}

fn draw_inspector_identity(ui: &mut egui::Ui, obj: &GeoObject) {
    let theme = current_theme(ui.ctx());
    let visible = obj.is_visible();
    let label = obj.label().to_string();
    let caption = inspector_type_caption(obj);
    // Titular: ecuación en grande; sin ecuación, placeholder honesto.
    // El nombre del tipo vive solo como caption terciario.
    let headline = inspector_equation_text(obj).unwrap_or_else(|| caption.to_string());
    let equation_shown = inspector_equation_text(obj).is_some();
    egui::Frame::none()
        .fill(theme.input_bg)
        .stroke(egui::Stroke::NONE)
        .rounding(egui::Rounding::same(RADIUS_LG))
        .inner_margin(egui::Margin::same(SPACE_SM))
        .show(ui, |ui| {
            ui.set_min_width(ui.available_width());
            ui.style_mut().wrap_mode = Some(egui::TextWrapMode::Wrap);
            // Título + badge en la misma fila: el badge va a la derecha
            // del título, nunca flotando.
            ui.horizontal(|ui| {
                ui.add(
                    egui::Label::new(
                        egui::RichText::new("Ecuación")
                            .color(theme.text_tertiary)
                            .size(TYPE_SM)
                            .strong(),
                    )
                    .truncate(),
                );
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    ui.add(
                        egui::Label::new(
                            egui::RichText::new(if visible { "Visible" } else { "Oculto" })
                                .color(if visible {
                                    theme.success
                                } else {
                                    theme.text_tertiary
                                })
                                .size(TYPE_SM),
                        )
                        .truncate(),
                    )
                    .on_hover_text(inspector_identity_tooltip(&headline, &label, visible));
                });
            });
            ui.add_space(SPACE_XS);
            let tooltip = inspector_identity_tooltip(&headline, &label, visible);
            if equation_shown {
                ui.add(
                    egui::Label::new(
                        egui::RichText::new(&headline)
                            .color(theme.text_primary)
                            .size(TYPE_MD)
                            .monospace(),
                    )
                    .truncate(),
                )
                .on_hover_text(tooltip.as_str());
            } else {
                ui.add(
                    egui::Label::new(
                        egui::RichText::new("Ecuación no disponible para este tipo")
                            .color(theme.text_secondary)
                            .size(TYPE_SM),
                    )
                    .truncate(),
                )
                .on_hover_text(tooltip.as_str());
            }
            ui.add_space(SPACE_XS);
            if let Some(labeled) = inspector_label_text(&label) {
                ui.add(
                    egui::Label::new(
                        egui::RichText::new(labeled)
                            .color(theme.text_secondary)
                            .size(TYPE_SM),
                    )
                    .truncate(),
                )
                .on_hover_text(tooltip.as_str());
            }
            ui.add(
                egui::Label::new(
                    egui::RichText::new(caption)
                        .color(theme.text_tertiary)
                        .size(TYPE_XS),
                )
                .truncate(),
            )
            .on_hover_text(tooltip.as_str());
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
                            egui::RichText::new("Tocá un objeto del lienzo para editarlo acá.")
                                .color(theme.text_primary)
                                .size(TYPE_BASE)
                                .strong(),
                        );
                        ui.add_space(SPACE_XS);
                        ui.label(
                            egui::RichText::new(
                                "Geometría, apariencia y controles avanzados aparecen acá.",
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

// ── Frente W-C: paso-a-paso visual (moat donde GeoGebra es débil) ─────────
// Tres secciones del panel CAS que cablean UI a comandos reales existentes:
// FunctionStudy (recorrido + tabla de signos), RiemannSum (slider n +
// exacta para comparar) y Taylor (slider orden 1..=10 + resto observado).
// El estado vive en la memoria temporal de egui como `LayerPanelState`:
// sin campos nuevos en `GrafitoApp`, sin I/O. Cero fantasma: cada botón
// arma `app.input_text` y llama a `submit_cas_worksheet_cell`, igual que
// el botón Ejecutar del panel.

/// Métodos de cuadratura del selector W-C: (comando, etiqueta visible).
const WC_QUADRATURE_METHODS: [(&str, &str); 3] = [
    ("midpoint", "Punto medio"),
    ("trapecio", "Trapecio"),
    ("simpson", "Simpson"),
];

/// Presupuesto UX del slider n (el comando banca hasta 1_000_000).
const WC_RIEMANN_SLIDER_MAX: f32 = 10_000.0;

/// Estado efímero de las 3 secciones W-C (temp egui; `Clone` para el
/// get/insert de `IdTypeMap`, `Default` con ejemplos que evalúan).
#[derive(Debug, Clone)]
struct WcVisualState {
    estudio_label: String,
    integral_expr: String,
    integral_a: String,
    integral_b: String,
    integral_n: f32,
    integral_metodo: usize,
    taylor_expr: String,
    taylor_centro: String,
    taylor_orden: i32,
    taylor_x: String,
}

impl Default for WcVisualState {
    fn default() -> Self {
        Self {
            estudio_label: "f".to_string(),
            integral_expr: "x^2".to_string(),
            integral_a: "0".to_string(),
            integral_b: "1".to_string(),
            integral_n: 100.0,
            integral_metodo: 0,
            taylor_expr: "sin(x)".to_string(),
            taylor_centro: "0".to_string(),
            taylor_orden: 5,
            taylor_x: "0.5".to_string(),
        }
    }
}

/// Texto del comando T1 (`None` si la etiqueta está vacía). Puro, testeable.
pub(crate) fn wc_study_command_text(label: &str) -> Option<String> {
    let label = label.trim();
    if label.is_empty() {
        None
    } else {
        Some(format!("FunctionStudy[{label}]"))
    }
}

/// Texto del comando T2 (`None` si falta tramo o n fuera de 1..=1_000_000).
/// La variable es `x` (se dice en el hint de la sección). Puro, testeable.
pub(crate) fn wc_riemann_command_text(
    expr: &str,
    a: &str,
    b: &str,
    n: f32,
    metodo: &str,
) -> Option<String> {
    let (expr, a, b, metodo) = (expr.trim(), a.trim(), b.trim(), metodo.trim());
    if expr.is_empty() || a.is_empty() || b.is_empty() || metodo.is_empty() {
        return None;
    }
    if !n.is_finite() {
        return None;
    }
    let n = n.round() as usize;
    if !(1..=1_000_000).contains(&n) {
        return None;
    }
    Some(format!("RiemannSum[{expr}, x, {a}, {b}, {n}, {metodo}]"))
}

/// Texto del comando de la integral exacta T2 (para comparar en la hoja).
/// Puro, testeable.
pub(crate) fn wc_exact_integral_command_text(expr: &str, a: &str, b: &str) -> Option<String> {
    let (expr, a, b) = (expr.trim(), a.trim(), b.trim());
    if expr.is_empty() || a.is_empty() || b.is_empty() {
        None
    } else {
        Some(format!("Integral[{expr}, x, {a}, {b}]"))
    }
}

/// Texto del comando T3 (`None` si la expresión no cierra o el orden sale
/// del rango del slider 1..=10). Puro, testeable.
pub(crate) fn wc_taylor_command_text(
    expr: &str,
    centro: &str,
    orden: i32,
    x: &str,
) -> Option<String> {
    let (expr, centro, x) = (expr.trim(), centro.trim(), x.trim());
    if expr.is_empty() || centro.is_empty() || x.is_empty() {
        return None;
    }
    if !(1..=10).contains(&orden) {
        return None;
    }
    if centro.parse::<f64>().is_err() || x.parse::<f64>().is_err() {
        return None;
    }
    Some(format!("Taylor[{expr}, x, {centro}, {orden}]"))
}

/// Línea de resto observado T3 con el motor existente. `None` honesto si
/// algo no evalúa (la sección muestra "revisá la expresión").
pub(crate) fn wc_taylor_remainder_line(
    expr: &str,
    centro: &str,
    orden: i32,
    x: &str,
    vars: &HashMap<String, f64>,
) -> Option<String> {
    let centro: f64 = centro.trim().parse().ok()?;
    let x: f64 = x.trim().parse().ok()?;
    let orden_usize: usize = usize::try_from(orden).ok()?;
    let r = grafito_command::commands::taylor_remainder_observed(
        expr.trim(),
        "x",
        centro,
        orden_usize,
        x,
        vars,
    )?;
    let siguiente = r
        .termino_siguiente
        .map(|t| format!("{t:.2e}"))
        .unwrap_or_else(|| "—".to_string());
    Some(format!(
        "P{orden}({x}) ≈ {:.6} · f = {:.6} · resto {:.2e} · sig. {siguiente}",
        r.approx, r.exact, r.resto_observado
    ))
}

/// Ejecuta un comando W-C como si se escribiera en la Entrada: arma
/// `input_text` y dispara la celda (cero fantasma, comando real).
fn wc_submit(app: &mut GrafitoApp, ui: &egui::Ui, command: String) {
    app.input_text = command;
    app.submit_cas_worksheet_cell(ui.ctx().input(|i| i.time));
}

fn draw_wc_study_contents(app: &mut GrafitoApp, ui: &mut egui::Ui, wc: &mut WcVisualState) {
    ui.horizontal(|ui| {
        ui.label("Función:");
        ui.text_edit_singleline(&mut wc.estudio_label);
        if ui.small_button("Estudiar").clicked() {
            if let Some(cmd) = wc_study_command_text(&wc.estudio_label) {
                wc_submit(app, ui, cmd);
            }
        }
    });
    ui.label(
        egui::RichText::new(
            "Ceros, extremos, asíntotas + tabla de signos. Marca puntos en el canvas.",
        )
        .color(current_theme(ui.ctx()).text_tertiary)
        .size(TYPE_XS),
    );
}

fn draw_wc_integral_contents(app: &mut GrafitoApp, ui: &mut egui::Ui, wc: &mut WcVisualState) {
    ui.text_edit_singleline(&mut wc.integral_expr)
        .on_hover_text("Integrando en x, ej. x^2");
    ui.horizontal(|ui| {
        ui.label("a:");
        ui.text_edit_singleline(&mut wc.integral_a);
        ui.label("b:");
        ui.text_edit_singleline(&mut wc.integral_b);
    });
    ui.add(egui::Slider::new(&mut wc.integral_n, 1.0..=WC_RIEMANN_SLIDER_MAX).text("n"));
    let metodo_idx = wc.integral_metodo.min(WC_QUADRATURE_METHODS.len() - 1);
    wc.integral_metodo = metodo_idx;
    egui::ComboBox::from_id_salt("wc_metodo")
        .selected_text(WC_QUADRATURE_METHODS[metodo_idx].1)
        .show_ui(ui, |ui| {
            for (i, (_, nombre)) in WC_QUADRATURE_METHODS.iter().enumerate() {
                ui.selectable_value(&mut wc.integral_metodo, i, *nombre);
            }
        });
    let n = wc.integral_n.round() as usize;
    if WC_QUADRATURE_METHODS[wc.integral_metodo].0 == "simpson" && n % 2 == 1 {
        ui.label(
            egui::RichText::new("Simpson requiere n par (el comando lo rechaza honesto).")
                .color(current_theme(ui.ctx()).text_tertiary)
                .size(TYPE_XS),
        );
    }
    ui.horizontal(|ui| {
        if ui.small_button("Aproximar").clicked() {
            if let Some(cmd) = wc_riemann_command_text(
                &wc.integral_expr,
                &wc.integral_a,
                &wc.integral_b,
                wc.integral_n,
                WC_QUADRATURE_METHODS[wc.integral_metodo].0,
            ) {
                wc_submit(app, ui, cmd);
            }
        }
        if ui.small_button("Exacta").clicked() {
            if let Some(cmd) =
                wc_exact_integral_command_text(&wc.integral_expr, &wc.integral_a, &wc.integral_b)
            {
                wc_submit(app, ui, cmd);
            }
        }
    });
    ui.label(
        egui::RichText::new("La hoja muestra ambas celdas: aproximado vs exacto.")
            .color(current_theme(ui.ctx()).text_tertiary)
            .size(TYPE_XS),
    );
}

fn draw_wc_taylor_contents(app: &mut GrafitoApp, ui: &mut egui::Ui, wc: &mut WcVisualState) {
    ui.text_edit_singleline(&mut wc.taylor_expr)
        .on_hover_text("Función en x, ej. sin(x)");
    ui.horizontal(|ui| {
        ui.label("centro:");
        ui.text_edit_singleline(&mut wc.taylor_centro);
        ui.label("x:");
        ui.text_edit_singleline(&mut wc.taylor_x);
    });
    ui.add(egui::Slider::new(&mut wc.taylor_orden, 1..=10).text("orden"));
    if ui.small_button("Polinomio").clicked() {
        if let Some(cmd) = wc_taylor_command_text(
            &wc.taylor_expr,
            &wc.taylor_centro,
            wc.taylor_orden,
            &wc.taylor_x,
        ) {
            wc_submit(app, ui, cmd);
        }
    }
    match wc_taylor_remainder_line(
        &wc.taylor_expr,
        &wc.taylor_centro,
        wc.taylor_orden,
        &wc.taylor_x,
        &app.document.variables,
    ) {
        Some(line) => {
            ui.label(egui::RichText::new(line).monospace().size(TYPE_XS));
        }
        None => {
            ui.label(
                egui::RichText::new("Sin resto: revisá la expresión.")
                    .color(current_theme(ui.ctx()).text_tertiary)
                    .size(TYPE_XS),
            );
        }
    }
}

/// Las 3 secciones W-C dentro del panel CAS (tras la Entrada, antes de la
/// Hoja). Estado en temp egui; cada botón ejecuta un comando real.
fn draw_wc_visual_sections(app: &mut GrafitoApp, ui: &mut egui::Ui) {
    let state_id = ui.id().with("wc_visual");
    let mut wc = ui.ctx().memory_mut(|m| {
        m.data
            .get_temp_mut_or_default::<WcVisualState>(state_id)
            .clone()
    });
    draw_inspector_section(
        ui,
        "Recorrido de función",
        "Paso a paso visual: signos, ceros, extremos, asíntotas.",
        |ui| draw_wc_study_contents(app, ui, &mut wc),
    );
    ui.add_space(SPACE_MD);
    draw_inspector_section(
        ui,
        "Integral numérica",
        "Riemann, trapecio o Simpson con n barras.",
        |ui| draw_wc_integral_contents(app, ui, &mut wc),
    );
    ui.add_space(SPACE_MD);
    draw_inspector_section(
        ui,
        "Taylor visual",
        "Orden 1..10 con resto observado.",
        |ui| draw_wc_taylor_contents(app, ui, &mut wc),
    );
    ui.ctx().memory_mut(|m| m.data.insert_temp(state_id, wc));
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
        .min_width(PANEL_LEFT_MIN)
        .max_width((ctx.available_rect().width() * PANEL_LEFT_MAX_FRACTION).max(200.0))
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

                            // Frente W-C: paso-a-paso visual (cablea a
                            // comandos reales; cero fantasma).
                            draw_wc_visual_sections(&mut *app, ui);
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

    // Estado Capas (oleada M): vive en temp egui; se poda y escribe de vuelta
    // alrededor del frame. Solo lectura del documento fuera de los handlers.
    let layer_panel_id = egui::Id::new("grafito_layer_panel_state");
    let mut layer_state: LayerPanelState = ctx
        .data_mut(|data| data.get_temp::<LayerPanelState>(layer_panel_id))
        .unwrap_or_default();
    layer_state.table.prune_missing(&app.document);
    let selection: Option<(ObjectId, String, u32)> = app.selected_object.and_then(|id| {
        app.document.get_object(id).map(|object| {
            (
                id,
                object.label().to_string(),
                layer_state.table.layer_of(id),
            )
        })
    });

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
                                // D2: salida de examen con confirmación (nunca directo).
                                app.exam_mode_checkbox(ui);
                            });
                            ui.add_space(CARD_SPACING);

                            // Ejes — escala logarítmica + plano numerado (F2c)
                            draw_inspector_section(
                                ui,
                                "Ejes",
                                "Escalas logarítmicas y plano numerado.",
                                |ui| {
                                    ui.checkbox(
                                        &mut app.document.view_mut().x_log,
                                        "Eje X logarítmico",
                                    );
                                    ui.checkbox(
                                        &mut app.document.view_mut().y_log,
                                        "Eje Y logarítmico",
                                    );
                                    ui.checkbox(
                                        &mut app.document.number_plane_labels,
                                        "Plano numerado (ticks + etiquetas + origen)",
                                    )
                                    .on_hover_text(
                                        "Muestra los números de los ejes con pasos lindos 1/2/5×10^n y skip anti-solape",
                                    );
                                },
                            );
                            ui.add_space(CARD_SPACING);

                            // Parámetro vivo (F2c · ValueTracker→slider).
                            // Un parámetro nombrado (`p`) bound al slider y a la
                            // animación paramétrica: el slider escribe vía
                            // `set_live_param` (re-evalúa dependientes y sube
                            // versión; las funciones se re-muestrean por clave
                            // con hash de variables) y la animación lo lee con
                            // `live_param_value("p", fallback)` si existe.
                            draw_inspector_section(
                                ui,
                                "Parámetro vivo",
                                "Slider bound a `p`: mueve la geometría que lo usa.",
                                |ui| {
                                    if let Some(live) = app.document.live_param("p") {
                                        let mut value = live.value;
                                        let slider = egui::Slider::new(
                                            &mut value,
                                            live.min..=live.max,
                                        )
                                        .text("p")
                                        .clamping(egui::SliderClamping::Edits)
                                        .trailing_fill(true);
                                        let response = ui.add(slider).on_hover_text(format!(
                                            "p = {value:.3} en [{:.3}, {:.3}] · las funciones con `p` se re-muestrean solas",
                                            live.min, live.max
                                        ));
                                        if response.changed() {
                                            let mut snapshot =
                                                crate::app::DeferredPanelSnapshot::new(
                                                    app.undo_stack.len(),
                                                );
                                            snapshot.capture(&app.document);
                                            if let Err(error) =
                                                app.document.set_live_param("p", value)
                                            {
                                                let message =
                                                    format!("Parámetro vivo: {error}");
                                                app.cas_result = message.clone();
                                                app.notify(
                                                    message,
                                                    grafito_ui::toast::ToastKind::Error,
                                                );
                                            }
                                            snapshot.save_if_semantically_changed(
                                                &mut app.document,
                                                &mut app.undo_stack,
                                                &mut app.redo_stack,
                                            );
                                        } else {
                                            ui.label(
                                                egui::RichText::new(format!(
                                                    "p = {value:.3} en [{:.3}, {:.3}]",
                                                    live.min, live.max
                                                ))
                                                .size(TYPE_XS)
                                                .color(
                                                    current_theme(ui.ctx()).text_secondary,
                                                ),
                                            );
                                        }
                                    } else {
                                        ui.label(
                                            egui::RichText::new(
                                                "Todavía no hay parámetro `p`: las funciones con `p` usan su valor por defecto.",
                                            )
                                            .size(TYPE_XS)
                                            .color(current_theme(ui.ctx()).text_secondary),
                                        );
                                        if ui
                                            .small_button("Crear parámetro p en [-5, 5]")
                                            .on_hover_text(
                                                "Crea la variable `p` con slider y la deja lista para la animación paramétrica",
                                            )
                                            .clicked()
                                        {
                                            let mut snapshot =
                                                crate::app::DeferredPanelSnapshot::new(
                                                    app.undo_stack.len(),
                                                );
                                            snapshot.capture(&app.document);
                                            match app.document.ensure_live_param(
                                                "p", -5.0, 5.0, 0.0,
                                            ) {
                                                Ok(_) => {
                                                    app.cas_result =
                                                        "Parámetro `p` creado en [-5, 5]".to_string();
                                                }
                                                Err(error) => {
                                                    let message =
                                                        format!("Parámetro vivo: {error}");
                                                    app.cas_result = message.clone();
                                                    app.notify(
                                                        message,
                                                        grafito_ui::toast::ToastKind::Error,
                                                    );
                                                }
                                            }
                                            snapshot.save_if_semantically_changed(
                                                &mut app.document,
                                                &mut app.undo_stack,
                                                &mut app.redo_stack,
                                            );
                                        }
                                    }
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

                            // Capas — orden + visibilidad (oleada M, API F10-C).
                            // Piel pura: lee &Estado, muta documento con snapshot
                            // de undo; el diálogo/trabajo pesado no aplica aquí
                            // (solo toggles y assigns en memoria).
                            draw_inspector_section(
                                ui,
                                "Capas",
                                "Ordená objetos en capas 0..=255 y alterná su visibilidad conjunta.",
                                |ui| {
                                    let used = layer_state.table.used_layers(&app.document);
                                    if used.is_empty() {
                                        ui.label(
                                            egui::RichText::new(
                                                "Sin objetos: todo lo nuevo entra en la capa 0.",
                                            )
                                            .size(TYPE_XS)
                                            .color(current_theme(ui.ctx()).text_secondary),
                                        );
                                    }
                                    for (layer, count) in used {
                                        let visible = layer_state.table.is_layer_visible(
                                            &app.document,
                                            layer,
                                        );
                                        ui.horizontal(|ui| {
                                            ui.label(
                                                egui::RichText::new(format!(
                                                    "Capa {layer} · {count} obj"
                                                ))
                                                .size(TYPE_SM),
                                            );
                                            ui.with_layout(
                                                egui::Layout::right_to_left(egui::Align::Center),
                                                |ui| {
                                                    let action =
                                                        if visible { "Ocultar" } else { "Mostrar" };
                                                    if ui
                                                        .small_button(action)
                                                        .on_hover_text(format!(
                                                            "Cambia la visibilidad de los {count} objetos de la capa {layer} (con deshacer)"
                                                        ))
                                                        .clicked()
                                                    {
                                                        let mut snap =
                                                            crate::app::DeferredPanelSnapshot::new(
                                                                app.undo_stack.len(),
                                                            );
                                                        snap.capture(&app.document);
                                                        let touched = layer_state
                                                            .table
                                                            .set_layer_visible(
                                                                &mut app.document,
                                                                layer,
                                                                !visible,
                                                            );
                                                        if touched > 0 {
                                                            app.cas_result = format!(
                                                                "Capa {layer} {} ({touched} obj)",
                                                                if visible {
                                                                    "oculta"
                                                                } else {
                                                                    "visible"
                                                                }
                                                            );
                                                        }
                                                        snap.save_if_semantically_changed(
                                                            &mut app.document,
                                                            &mut app.undo_stack,
                                                            &mut app.redo_stack,
                                                        );
                                                    }
                                                },
                                            );
                                        });
                                    }
                                    ui.add_space(SPACE_XS);
                                    let selection_text = selection
                                        .as_ref()
                                        .map(|(_, label, layer)| {
                                            let name = if label.is_empty() {
                                                "<sin etiqueta>"
                                            } else {
                                                label
                                            };
                                            format!("Selección: {name} (capa {layer})")
                                        })
                                        .unwrap_or_else(|| {
                                            "Sin selección: elegí un objeto en el lienzo."
                                                .to_string()
                                        });
                                    ui.label(
                                        egui::RichText::new(selection_text)
                                            .size(TYPE_XS)
                                            .color(current_theme(ui.ctx()).text_secondary),
                                    );
                                    ui.horizontal(|ui| {
                                        ui.label(
                                            egui::RichText::new("Asignar a capa:").size(TYPE_SM),
                                        );
                                        ui.add(
                                            egui::DragValue::new(
                                                &mut layer_state.selected_layer,
                                            )
                                            .range(0..=MAX_LAYERS)
                                            .speed(1),
                                        );
                                        if ui
                                            .add_enabled(
                                                selection.is_some(),
                                                egui::Button::new(
                                                    egui::RichText::new("Asignar").size(TYPE_SM),
                                                ),
                                            )
                                            .on_hover_text(
                                                "Mueve el objeto seleccionado a la capa elegida",
                                            )
                                            .clicked()
                                        {
                                            if let Some((id, label, _)) = selection.as_ref() {
                                                match layer_state
                                                    .table
                                                    .assign(*id, layer_state.selected_layer)
                                                {
                                                    Ok(()) => {
                                                        let name = if label.is_empty() {
                                                            "<sin etiqueta>"
                                                        } else {
                                                            label
                                                        };
                                                        app.cas_result = format!(
                                                            "'{name}' → capa {}",
                                                            layer_state.selected_layer
                                                        );
                                                    }
                                                    Err(error) => {
                                                        app.cas_result = format!(
                                                            "No se pudo asignar: {error}"
                                                        );
                                                        app.notify(
                                                            app.cas_result.clone(),
                                                            grafito_ui::toast::ToastKind::Error,
                                                        );
                                                    }
                                                }
                                            }
                                        }
                                    });
                                },
                            );
                            ui.add_space(CARD_SPACING);

                            // Exportación — vectorial pill centered + oleada M:
                            // PNG/TikZ/PDF/CSV y portapapeles. Piel pura: el
                            // diálogo rfd queda en UI thread (modal nativo);
                            // render+write van a workers y el summary se aplica
                            // en `poll_background_jobs`.
                            draw_inspector_section(
                                ui,
                                "Exportación",
                                "Generá un archivo del lienzo actual o copialo al portapapeles.",
                                |ui| {
                                    for format in [
                                        crate::export::ExportFormat::Svg,
                                        crate::export::ExportFormat::Png,
                                        crate::export::ExportFormat::Tikz,
                                    ] {
                                        if export_pill_button(
                                            ui,
                                            &format!("Exportar {}", format.display_name()),
                                        )
                                        .clicked()
                                        {
                                            app.export_with_dialog(format, Some(ui.ctx()));
                                        }
                                    }
                                    if export_pill_button(ui, "Exportar PDF").on_hover_text(
                                        "Vectorial de 1 página: rectas, círculos, polígonos y texto (Helvetica)",
                                    ).clicked()
                                    {
                                        if let Some(path) = rfd::FileDialog::new()
                                            .add_filter("PDF", &["pdf"])
                                            .set_file_name("grafito_export.pdf")
                                            .save_file()
                                        {
                                            app.pending_export_job = Some(
                                                crate::app::PendingExportJob {
                                                    receiver: spawn_pdf_export(
                                                        app.document.clone(),
                                                        path,
                                                        ui.ctx(),
                                                    ),
                                                },
                                            );
                                            app.notify(
                                                "Exportando PDF…",
                                                grafito_ui::toast::ToastKind::Info,
                                            );
                                        }
                                    }
                                    let tables: Vec<(ObjectId, String, usize)> = app
                                        .document
                                        .objects_iter_sorted()
                                        .filter_map(|(id, object)| match object {
                                            GeoObject::DataTable(table) => Some((
                                                *id,
                                                table.label.clone(),
                                                table.xs.len(),
                                            )),
                                            _ => None,
                                        })
                                        .collect();
                                    if !tables.is_empty() {
                                        ui.add_space(SPACE_XS);
                                        ui.label(
                                            egui::RichText::new("Tablas (CSV RFC 4180)")
                                                .size(TYPE_XS)
                                                .color(
                                                    current_theme(ui.ctx()).text_secondary,
                                                ),
                                        );
                                    }
                                    for (id, label, rows) in tables {
                                        ui.horizontal(|ui| {
                                            let name = if label.is_empty() {
                                                "<sin etiqueta>"
                                            } else {
                                                &label
                                            };
                                            ui.label(
                                                egui::RichText::new(format!(
                                                    "{name} · {rows} filas"
                                                ))
                                                .size(TYPE_SM),
                                            );
                                            ui.with_layout(
                                                egui::Layout::right_to_left(egui::Align::Center),
                                                |ui| {
                                                    if ui
                                                        .small_button("CSV")
                                                        .on_hover_text(
                                                            "Descarga honesta: escribe el CSV real o informa el error",
                                                        )
                                                        .clicked()
                                                    {
                                                        match datatable_csv_text(&app.document, id)
                                                        {
                                                            Ok((table_label, csv)) => {
                                                                let stem = sanitize_export_stem(
                                                                    &table_label,
                                                                );
                                                                if let Some(path) =
                                                                    rfd::FileDialog::new()
                                                                        .add_filter(
                                                                            "CSV",
                                                                            &["csv"],
                                                                        )
                                                                        .set_file_name(format!(
                                                                            "{stem}.csv"
                                                                        ))
                                                                        .save_file()
                                                                {
                                                                    app.pending_export_job = Some(
                                                                        crate::app::PendingExportJob {
                                                                            receiver:
                                                                                spawn_csv_export(
                                                                                    csv,
                                                                                    table_label,
                                                                                    path,
                                                                                    ui.ctx(),
                                                                                ),
                                                                        },
                                                                    );
                                                                    app.notify(
                                                                        "Exportando CSV…",
                                                                        grafito_ui::toast::ToastKind::Info,
                                                                    );
                                                                }
                                                            }
                                                            Err(error) => {
                                                                app.cas_result = error.clone();
                                                                app.notify(
                                                                    error,
                                                                    grafito_ui::toast::ToastKind::Error,
                                                                );
                                                            }
                                                        }
                                                    }
                                                },
                                            );
                                        });
                                    }
                                    ui.add_space(SPACE_XS);
                                    ui.horizontal(|ui| {
                                        if ui
                                            .small_button("Copiar SVG")
                                            .on_hover_text(
                                                "Copia el SVG real del lienzo al portapapeles",
                                            )
                                            .clicked()
                                        {
                                            match clipboard_svg(&app.document) {
                                                Ok(svg) => {
                                                    let bytes = svg.len();
                                                    ui.ctx().output_mut(|out| {
                                                        out.copied_text = svg;
                                                    });
                                                    app.cas_result = format!(
                                                        "SVG copiado al portapapeles ({bytes} bytes)"
                                                    );
                                                    app.notify(
                                                        app.cas_result.clone(),
                                                        grafito_ui::toast::ToastKind::Success,
                                                    );
                                                }
                                                Err(error) => {
                                                    app.cas_result = format!(
                                                        "No se pudo copiar SVG: {error}"
                                                    );
                                                    app.notify(
                                                        app.cas_result.clone(),
                                                        grafito_ui::toast::ToastKind::Error,
                                                    );
                                                }
                                            }
                                        }
                                        if ui
                                            .small_button("Guardar PNG…")
                                            .on_hover_text(
                                                "Rasteriza el lienzo a PNG real (tiny-skia) y lo guarda donde elijas",
                                            )
                                            .clicked()
                                        {
                                            app.export_with_dialog(
                                                crate::export::ExportFormat::Png,
                                                Some(ui.ctx()),
                                            );
                                        }
                                        if ui
                                            .small_button("Copiar PNG")
                                            .on_hover_text(
                                                "Copia el PNG real del lienzo al portapapeles del sistema (para Word/Moodle)",
                                            )
                                            .clicked()
                                        {
                                            match copy_png_to_os_clipboard(&app.document) {
                                                Ok(summary) => {
                                                    app.cas_result = summary.clone();
                                                    app.notify(
                                                        summary,
                                                        grafito_ui::toast::ToastKind::Success,
                                                    );
                                                }
                                                Err(error) => {
                                                    app.cas_result = error.clone();
                                                    app.notify(
                                                        error,
                                                        grafito_ui::toast::ToastKind::Error,
                                                    );
                                                }
                                            }
                                        }
                                    });
                                    // Texto (G-C): MathML / TikZ-eje / HTML puros al portapapeles.
                                    // Piel pura: builders sin I/O + egui output (igual que Copiar SVG);
                                    // el guardado a archivo queda para P2 (cableado menú en app.rs/ui.rs).
                                    ui.add_space(SPACE_XS);
                                    ui.horizontal(|ui| {
                                        for (label, tip, build) in [
                                            (
                                                "MathML",
                                                "Copia las funciones y puntos como MathML al portapapeles",
                                                crate::export::document_to_mathml as fn(
                                                    &Document,
                                                )
                                                    -> Result<String, String>,
                                            ),
                                            (
                                                "TikZ-eje",
                                                "Copia un entorno pgfplots axis con tus funciones al portapapeles",
                                                crate::export::document_to_tikz_axis as fn(
                                                    &Document,
                                                )
                                                    -> Result<String, String>,
                                            ),
                                            (
                                                "HTML",
                                                "Copia una página autónoma con el SVG del lienzo al portapapeles",
                                                crate::export::document_to_html as fn(
                                                    &Document,
                                                )
                                                    -> Result<String, String>,
                                            ),
                                        ] {
                                            if ui.small_button(label).on_hover_text(tip).clicked() {
                                                match build(&app.document) {
                                                    Ok(text) => {
                                                        let bytes = text.len();
                                                        ui.ctx().output_mut(|out| {
                                                            out.copied_text = text;
                                                        });
                                                        app.cas_result = format!(
                                                            "{label} copiado al portapapeles ({bytes} bytes)"
                                                        );
                                                        app.notify(
                                                            app.cas_result.clone(),
                                                            grafito_ui::toast::ToastKind::Success,
                                                        );
                                                    }
                                                    Err(error) => {
                                                        app.cas_result = error;
                                                        app.notify(
                                                            app.cas_result.clone(),
                                                            grafito_ui::toast::ToastKind::Error,
                                                        );
                                                    }
                                                }
                                            }
                                        }
                                    });
                                },
                            );
                            ui.add_space(CARD_SPACING);

                            // Datos — hoja viva en lectura (G-C): tablas del documento.
                            draw_inspector_section(
                                ui,
                                "Datos",
                                "Hoja viva en lectura: valores de las tablas del documento.",
                                |ui| {
                                    draw_spreadsheet_section(ui, app);
                                },
                            );
                            ui.add_space(CARD_SPACING);

                            // Probabilidad — Normal/Binomial/Poisson (G-C).
                            draw_inspector_section(
                                ui,
                                "Probabilidad",
                                "Densidad y acumulada honestas en f64.",
                                |ui| {
                                    draw_probability_section(ui, ctx);
                                },
                            );
                        });
                });
        });

    // Escribe de vuelta el estado Capas (asigns del frame).
    ctx.data_mut(|data| data.insert_temp(layer_panel_id, layer_state));
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
                            .size(grafito_ui::tokens::TYPE_XS),
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
                                    // W3 — la exportación deja de ser muda: revela
                                    // la carpeta de la última exportación exitosa.
                                    if app.last_export_dir.is_some() {
                                        ui.add_space(SPACE_XS);
                                        ui.horizontal(|ui| {
                                            if ui
                                                .small_button("Mostrar en carpeta")
                                                .on_hover_text(
                                                    "Abre la carpeta de tu última exportación.",
                                                )
                                                .clicked()
                                            {
                                                app.reveal_last_export();
                                            }
                                        });
                                    }

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
                        // Prompt auto (G-C): palabras o números → velocidad honesta.
                        draw_trig_speed_prompt(ui, app);
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
                        .rounding(egui::Rounding::same(grafito_ui::tokens::RADIUS_SM))
                        .inner_margin(egui::Margin::same(grafito_ui::tokens::SPACE_SM))
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
                                    .size(grafito_ui::tokens::TYPE_XS),
                            );
                            // Lectura de traza (G-C): el punto vivo bajo el "puntero" animado.
                            grafito_ui::trace::draw_trace_readout(
                                ui,
                                grafito_ui::trace::trace_from_hover(cos_t, sin_t, spec.name)
                                    .as_ref(),
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
                            .size(grafito_ui::tokens::TYPE_XS),
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
                            .size(grafito_ui::tokens::TYPE_BASE)
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
                                    | GeoObject::BarChart(_)
                                    | GeoObject::PieChart(_)
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
                            // Clip explícito: los conteos y min/max se pintan
                            // a mano y sin esto dejan slivers en docks
                            // angostos (misma clase que la tira del drawer).
                            let painter = ui.painter().with_clip_rect(hist_rect);
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
                                        egui::FontId::proportional(grafito_ui::tokens::TYPE_XS),
                                        txt_dim,
                                    );
                                }
                            }
                            // Etiquetas min/max en el eje
                            painter.text(
                                egui::pos2(plot.min.x, plot_bot + 2.0),
                                egui::Align2::LEFT_TOP,
                                format_statistic(summary.minimum),
                                egui::FontId::proportional(grafito_ui::tokens::TYPE_XS),
                                txt_dim,
                            );
                            painter.text(
                                egui::pos2(plot.max.x, plot_bot + 2.0),
                                egui::Align2::RIGHT_TOP,
                                format_statistic(summary.maximum),
                                egui::FontId::proportional(grafito_ui::tokens::TYPE_XS),
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
                    .size(grafito_ui::tokens::TYPE_BASE)
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
                    .size(grafito_ui::tokens::TYPE_BASE)
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
        .min_width(DRAWER_RIGHT_MIN)
        .max_width(DRAWER_RIGHT_MAX)
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
                    // Wrap honesto: sin esto, las filas de texto del Inspector
                    // (p. ej. la ecuación en `TYPE_MD`) se clipan en el borde
                    // del dock y dejan slivers de 1-2px. Las filas
                    // horizontales de controles (DragValue/Slider) no se ven
                    // afectadas.
                    ui.style_mut().wrap_mode = Some(egui::TextWrapMode::Wrap);
                    let Some(id) = app.selected_object else {
                        draw_inspector_empty_state(ui);
                        return;
                    };
                    let Some(mut edited_object) = app.document.get_object(id).cloned() else {
                        ui.label(egui::RichText::new("Objeto inexistente.").color(txt_dim));
                        return;
                    };
                    draw_inspector_identity(ui, &edited_object);
                    ui.add_space(SPACE_XS);
                    // D1-bis: editor de ecuación (pipeline D3). El display vive
                    // en `draw_inspector_identity`; acá el campo editable con
                    // validación en vivo y commit atómico + undo. El borrador
                    // vive en memoria temporal egui (sobrevive frames, no se
                    // persiste) claveado por objeto.
                    {
                        use crate::inspector_edit as ieq;
                        if let Some(live_now) = app.document.get_object(id).cloned() {
                            if ieq::is_editable(&live_now) {
                                let eq_key = egui::Id::new(("inspector_equation", id));
                                let mut eq_state: ieq::InspectorEditState = ui
                                    .ctx()
                                    .memory(|mem| mem.data.get_temp(eq_key))
                                    .unwrap_or_else(|| {
                                        ieq::begin_edit(&live_now).unwrap_or(
                                            ieq::InspectorEditState {
                                                draft: String::new(),
                                                error: None,
                                                editing: false,
                                            },
                                        )
                                    });
                                // Sin edición en curso, re-sincroniza con la
                                // canónica (cambios desde Álgebra u otros).
                                if !eq_state.editing {
                                    if let Some(fresh) = ieq::begin_edit(&live_now) {
                                        eq_state = fresh;
                                    }
                                }
                                ui.label(
                                    egui::RichText::new("Ecuación (editable)")
                                        .color(txt_dim)
                                        .size(TYPE_XS),
                                );
                                let mut draft = eq_state.draft.clone();
                                let eq_resp = ui.add(
                                    egui::TextEdit::singleline(&mut draft)
                                        .hint_text(ieq::hint_for(&live_now))
                                        .desired_width(f32::INFINITY),
                                );
                                if eq_resp.changed() {
                                    ieq::update_draft(&mut eq_state, &draft);
                                    ieq::revalidate_state(&mut eq_state, &live_now);
                                }
                                if let Some(err) = eq_state.error.clone() {
                                    ui.label(
                                        egui::RichText::new(err)
                                            .color(current_theme(ui.ctx()).danger)
                                            .size(TYPE_XS),
                                    );
                                    // W-A: inválido mantiene el último válido
                                    // (el documento no se tocó) + "Revertir"
                                    // vuelve a la canónica + "Usar ejemplo"
                                    // inserta sintaxis válida clicable.
                                    ui.horizontal(|ui| {
                                        if ui.small_button("Revertir").clicked() {
                                            ieq::cancel_edit(&mut eq_state, &live_now);
                                        }
                                        if let Some(example) = ieq::example_for(&live_now) {
                                            if ui
                                                .small_button("Usar ejemplo")
                                                .on_hover_text(format!(
                                                    "Inserta sintaxis válida: {example}"
                                                ))
                                                .clicked()
                                            {
                                                ieq::update_draft(&mut eq_state, example);
                                                ieq::revalidate_state(&mut eq_state, &live_now);
                                            }
                                        }
                                    });
                                }
                                let canonical_now = live_now
                                    .canonical_equation_text()
                                    .unwrap_or_default();
                                let can_apply = eq_state.error.is_none()
                                    && eq_state.draft != canonical_now;
                                let apply_clicked = ui
                                    .add_enabled(
                                        can_apply,
                                        egui::Button::new("Aplicar ecuación"),
                                    )
                                    .clicked();
                                let enter_pressed = eq_resp.lost_focus()
                                    && ui.input(|i| i.key_pressed(egui::Key::Enter));
                                if apply_clicked || enter_pressed {
                                    match ieq::commit_draft_with_previous(
                                        &mut app.document,
                                        id,
                                        &eq_state.draft,
                                    ) {
                                        Ok(Some(before)) => {
                                            snapshot.capture_successful_replacement(before);
                                            if let Some(updated) =
                                                app.document.get_object(id).cloned()
                                            {
                                                ieq::cancel_edit(&mut eq_state, &updated);
                                            }
                                        }
                                        Ok(None) => {}
                                        Err(message) => {
                                            eq_state.error = Some(message.clone());
                                            app.notify(
                                                message,
                                                grafito_ui::toast::ToastKind::Error,
                                            );
                                        }
                                    }
                                }
                                ui.ctx().memory_mut(|mem| {
                                    mem.data.insert_temp(eq_key, eq_state);
                                });
                            }
                        }
                    }
                    // Medida exacta del sólido (volumen/área) cuando el motor
                    // la calcula en forma cerrada; cuádricas → `None` honesto.
                    if let Some(measure) =
                        crate::render_3d::solid_measure_text(&edited_object)
                    {
                        ui.label(
                            egui::RichText::new(format!("Medida: {measure}"))
                                .color(txt_dim)
                                .size(TYPE_XS),
                        );
                    }
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
                _ => {
                    // Display-only: la ecuación ya va en grande en la card de
                    // identidad. Sin nombre muerto ni texto que no sirve.
                    // D3 agrega el campo de edición; este dock no edita.
                    ui.add_space(SPACE_XS);
                    ui.separator();
                    ui.add_space(SPACE_SM);
                    ui.label(
                        egui::RichText::new("Sin controles dedicados en este dock.")
                            .color(theme.text_secondary)
                            .size(TYPE_SM),
                    );
                    ui.label(
                        egui::RichText::new("Editá este objeto desde el panel de Álgebra.")
                            .color(txt_dim)
                            .size(TYPE_XS),
                    );
                    ui.add_space(SPACE_MD);
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
                        .inner_margin(egui::Margin::same(grafito_ui::tokens::SPACE_SM))
                        .show(ui, |ui| {
                            ui.vertical(|ui| {
                                ui.label(egui::RichText::new("Colores y Magnitud:").strong().color(hdr_col).size(TYPE_XS));
                                ui.label(egui::RichText::new("- Tono: Fase o angulo arg(f(z)).\n- Brillo: Magnitud |f(z)|. Negro = Raiz (0), Blanco = Polo (inf).").color(txt_dim).size(grafito_ui::tokens::TYPE_XS));

                                ui.add_space(6.0);
                                ui.label(egui::RichText::new("Derivabilidad y Wirtinger:").strong().color(hdr_col).size(TYPE_XS));
                                ui.label(egui::RichText::new("- Al graficar deriv_z_conj(f), f(z) es holomorfa solo en zonas negras (donde d/dzbar = 0, Cauchy-Riemann).\n- Las zonas coloreadas representan donde NO es derivable.").color(txt_dim).size(grafito_ui::tokens::TYPE_XS));
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
                                                .size(grafito_ui::tokens::TYPE_XS),
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
                        import_local_xy_table(app, ctx);
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
                            // FileController: el diálogo rfd queda en UI thread (modal
                            // nativo); el write va a worker y se notifica en el poll.
                            let latex = construction_log_to_latex(&app.construction_log);
                            if let Some(path) =
                                rfd::FileDialog::new().add_filter("TeX", &["tex"]).save_file()
                            {
                                app.pending_text_job = Some(crate::app::PendingTextWriteJob {
                                    receiver: crate::app::spawn_text_write(
                                        path,
                                        latex,
                                        ctx,
                                    ),
                                });
                                app.cas_result = "Exportando protocolo a LaTeX…".to_string();
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
                                                            .size(grafito_ui::tokens::TYPE_XS),
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

#[cfg(test)]
mod layer_panel_tests {
    use super::LayerPanelState;
    use grafito_core::Document;

    #[test]
    fn layer_panel_state_defaults_empty_on_layer_zero() {
        let state = LayerPanelState::default();
        assert_eq!(state.selected_layer, 0);
        let document = Document::new();
        assert_eq!(state.table.used_layers(&document), vec![]);
        assert!(state.table.is_layer_visible(&document, 0));
    }
}

// ══════════════════════════════════════════════════════════════════════════
// Frente G-C · Piel vs GeoGebra UI (2026-09-05) — ADITIVO
// ══════════════════════════════════════════════════════════════════════════
// Sliders Play + prompt auto, spreadsheet viva (lectura) y panel de
// probabilidad (Normal/Binomial/Poisson/T/Chi²/F con PDF/CDF honestos en f64).
//
// Reglas del frente: `fn render(&Estado) -> Frame` — estado efímero en
// `ctx.data` (cero campos nuevos en `GrafitoApp`), cero I/O/spawn en Ui::
// (los botones mutan estado en memoria o copian al portapapeles vía egui
// output, como el «Copiar SVG» existente), tokens TYPE/SPACE/RADIUS,
// Scandinavian 5/8/17, sin botones mudos. Cableado en paneles existentes
// y alcanzables (vista / trig) para que nada quede colgado.

/// Cota de `n` en Binomial: CDF suma `n` términos por frame.
const MAX_BINOMIAL_N: u64 = 2_000;
/// Cota de `λ` en Poisson: `exp(-λ)` subfluye igual, pero el loop es O(k).
const MAX_POISSON_LAMBDA: f64 = 1_000_000.0;
/// Cota de `k` en Poisson.
const MAX_POISSON_K: u64 = 100_000;
/// Filas por tabla en la hoja viva (lectura): el resto se avisa.
const MAX_SPREADSHEET_ROWS: usize = 50;
/// Tablas visibles en la hoja viva: el resto se avisa.
const MAX_SPREADSHEET_TABLES: usize = 8;

/// Motor único de probabilidad (F3b, extendido a T/Chi²/F en C2): este
/// panel NO duplica matemática. Delega en `grafito_geometry::statistics`
/// —el mismo motor que usan los comandos `Normal`/`Binomial`/`Poisson`/
/// `InverseNormal`/`InverseT`/`InverseChiSquared`/`InverseF`— y solo agrega
/// el `Err` honesto en español + las cotas de UI. `NaN` del motor ⇒ `Err`.
fn check_probability_point(value: f64, what: &str) -> Result<f64, String> {
    if !value.is_finite() {
        return Err(format!("{what} debe ser un número finito"));
    }
    Ok(value)
}

fn finish_probability_scalar(value: f64, what: &str) -> Result<f64, String> {
    if !value.is_finite() {
        return Err(format!("{what} no es finito con estos parámetros"));
    }
    Ok(value)
}

/// Densidad Normal(μ, σ) en x. `Err` honesto si σ ≤ 0 o hay no-finitos.
pub(crate) fn normal_pdf(x: f64, mu: f64, sigma: f64) -> Result<f64, String> {
    let (x, mu, sigma) = (
        check_probability_point(x, "x")?,
        check_probability_point(mu, "μ")?,
        check_probability_point(sigma, "σ")?,
    );
    if sigma <= 0.0 {
        return Err("σ debe ser mayor que 0".to_string());
    }
    finish_probability_scalar(
        grafito_geometry::statistics::normal_pdf(x, mu, sigma),
        "La densidad",
    )
}

/// Acumulada Normal(μ, σ) en x: P(X ≤ x).
pub(crate) fn normal_cdf(x: f64, mu: f64, sigma: f64) -> Result<f64, String> {
    let (x, mu, sigma) = (
        check_probability_point(x, "x")?,
        check_probability_point(mu, "μ")?,
        check_probability_point(sigma, "σ")?,
    );
    if sigma <= 0.0 {
        return Err("σ debe ser mayor que 0".to_string());
    }
    finish_probability_scalar(
        grafito_geometry::statistics::normal_cdf(x, mu, sigma),
        "La acumulada",
    )
}

/// Cuantil Normal(μ, σ) en p: el x con P(X ≤ x) = p. Misma inversa que
/// `InverseNormal[p, μ, σ]` (bisección honesta del motor, sin duplicar).
pub(crate) fn normal_quantile_honest(p: f64, mu: f64, sigma: f64) -> Result<f64, String> {
    if !(p.is_finite() && 0.0 < p && p < 1.0) {
        return Err("p debe estar en el intervalo (0, 1)".to_string());
    }
    check_probability_point(mu, "μ")?;
    check_probability_point(sigma, "σ")?;
    if sigma <= 0.0 {
        return Err("σ debe ser mayor que 0".to_string());
    }
    finish_probability_scalar(
        grafito_geometry::statistics::normal_quantile(p, mu, sigma),
        "El cuantil",
    )
}

fn check_binomial_args(k: u64, n: u64, p: f64) -> Result<(u32, f64, u32), String> {
    if !p.is_finite() {
        return Err("p debe ser un número finito".to_string());
    }
    if !(0.0..=1.0).contains(&p) {
        return Err("p debe estar entre 0 y 1".to_string());
    }
    if n > MAX_BINOMIAL_N {
        return Err(format!("n ≤ {} en este panel (cota de UI)", MAX_BINOMIAL_N));
    }
    if k > n {
        return Err("k no puede superar a n".to_string());
    }
    let n32 = u32::try_from(n).map_err(|_| "n fuera de rango".to_string())?;
    let k32 = u32::try_from(k).map_err(|_| "k fuera de rango".to_string())?;
    Ok((n32, p, k32))
}

/// Masa Binomial(n, p) en k: P(X = k). Delega en el motor.
pub(crate) fn binomial_pmf(k: u64, n: u64, p: f64) -> Result<f64, String> {
    let (n32, p, k32) = check_binomial_args(k, n, p)?;
    finish_probability_scalar(
        grafito_geometry::statistics::binomial_pmf(n32, p, k32),
        "La puntual",
    )
}

/// Acumulada Binomial(n, p) en k: P(X ≤ k). Delega en el motor.
pub(crate) fn binomial_cdf(k: u64, n: u64, p: f64) -> Result<f64, String> {
    let (n32, p, k32) = check_binomial_args(k, n, p)?;
    finish_probability_scalar(
        grafito_geometry::statistics::binomial_cdf(n32, p, k32),
        "La acumulada",
    )
}

/// Cuantil Binomial: el menor k con P(X ≤ k) ≥ p. Acumula `pmf` una sola vez
/// (O(n), n ≤ 2000 por cota) en vez de sumar CDFs anidadas.
pub(crate) fn binomial_quantile_honest(p: f64, n: u64, prob: f64) -> Result<u64, String> {
    if !(p.is_finite() && 0.0 < p && p < 1.0) {
        return Err("p debe estar en el intervalo (0, 1)".to_string());
    }
    if n > MAX_BINOMIAL_N {
        return Err(format!("n ≤ {} en este panel (cota de UI)", MAX_BINOMIAL_N));
    }
    if !prob.is_finite() || !(0.0..=1.0).contains(&prob) {
        return Err("p del modelo debe estar entre 0 y 1".to_string());
    }
    let mut acc = 0.0;
    for k in 0..=n {
        acc += binomial_pmf(k, n, prob)?;
        if acc >= p {
            return Ok(k);
        }
    }
    Ok(n)
}

fn check_poisson_args(k: u64, lambda: f64) -> Result<(f64, u32), String> {
    if !lambda.is_finite() {
        return Err("λ debe ser un número finito".to_string());
    }
    if lambda <= 0.0 {
        return Err("λ debe ser mayor que 0".to_string());
    }
    if lambda > MAX_POISSON_LAMBDA {
        return Err(format!(
            "λ ≤ {} en este panel (cota de UI)",
            MAX_POISSON_LAMBDA
        ));
    }
    if k > MAX_POISSON_K {
        return Err(format!("k ≤ {} en este panel (cota de UI)", MAX_POISSON_K));
    }
    let k32 =
        u32::try_from(k.min(u64::from(u32::MAX))).map_err(|_| "k fuera de rango".to_string())?;
    Ok((lambda, k32))
}

/// Masa Poisson(λ) en k: P(X = k). Delega en el motor.
pub(crate) fn poisson_pmf(k: u64, lambda: f64) -> Result<f64, String> {
    let (lambda, k32) = check_poisson_args(k, lambda)?;
    finish_probability_scalar(
        grafito_geometry::statistics::poisson_pmf(lambda, k32),
        "La puntual",
    )
}

/// Acumulada Poisson(λ) en k: P(X ≤ k). Delega en el motor.
pub(crate) fn poisson_cdf(k: u64, lambda: f64) -> Result<f64, String> {
    let (lambda, k32) = check_poisson_args(k, lambda)?;
    finish_probability_scalar(
        grafito_geometry::statistics::poisson_cdf(lambda, k32),
        "La acumulada",
    )
}

/// Cuantil Poisson: el menor k con P(X ≤ k) ≥ p. Itera `pmf` con cota de
/// 10 001 pasos (la del motor discreto); si no alcanza, `Err` honesto.
pub(crate) fn poisson_quantile_honest(p: f64, lambda: f64) -> Result<u64, String> {
    if !(p.is_finite() && 0.0 < p && p < 1.0) {
        return Err("p debe estar en el intervalo (0, 1)".to_string());
    }
    if !lambda.is_finite() || lambda <= 0.0 {
        return Err("λ debe ser mayor que 0".to_string());
    }
    if lambda > MAX_POISSON_LAMBDA {
        return Err(format!(
            "λ ≤ {} en este panel (cota de UI)",
            MAX_POISSON_LAMBDA
        ));
    }
    let mut acc = 0.0;
    let cap = MAX_POISSON_K.min(10_000);
    for k in 0..=cap {
        acc += poisson_pmf(k, lambda)?;
        if acc >= p {
            return Ok(k);
        }
    }
    Err("La cola supera la cota de 10 001 pasos en este panel".to_string())
}

/// Cota de grados de libertad en el panel (frente C2): el motor acepta
/// cualquier gl > 0, pero la curva se dibuja con 61 muestras por frame.
const MAX_PANEL_DF: f64 = 500.0;

fn check_panel_df(value: f64, what: &str) -> Result<f64, String> {
    if !value.is_finite() || value <= 0.0 {
        return Err(format!("{what} debe ser positivo y finito"));
    }
    if value > MAX_PANEL_DF {
        return Err(format!(
            "{what} ≤ {MAX_PANEL_DF} en este panel (cota de UI)"
        ));
    }
    Ok(value)
}

fn check_quantile_p(p: f64) -> Result<f64, String> {
    if p.is_finite() && 0.0 < p && p < 1.0 {
        Ok(p)
    } else {
        Err("p debe estar en el intervalo (0, 1)".to_string())
    }
}

/// Densidad t-Student(gl) en x. Mismo motor que `InverseT`.
pub(crate) fn student_t_pdf(x: f64, df: f64) -> Result<f64, String> {
    let x = check_probability_point(x, "x")?;
    let df = check_panel_df(df, "gl")?;
    finish_probability_scalar(
        grafito_geometry::statistics::student_t_pdf(x, df),
        "La densidad",
    )
}

/// Acumulada t-Student(gl) en x: P(X ≤ x).
pub(crate) fn student_t_cdf(x: f64, df: f64) -> Result<f64, String> {
    let x = check_probability_point(x, "x")?;
    let df = check_panel_df(df, "gl")?;
    finish_probability_scalar(
        grafito_geometry::statistics::student_t_cdf(x, df),
        "La acumulada",
    )
}

/// Cuantil t-Student: el x con P(X ≤ x) = p. Misma inversa que
/// `InverseT[p, gl]` (bisección honesta del motor, sin duplicar).
pub(crate) fn student_t_quantile_honest(p: f64, df: f64) -> Result<f64, String> {
    let p = check_quantile_p(p)?;
    let df = check_panel_df(df, "gl")?;
    finish_probability_scalar(
        grafito_geometry::statistics::student_t_quantile(p, df),
        "El cuantil",
    )
}

/// Densidad χ²(gl) en x. Mismo motor que `InverseChiSquared`.
pub(crate) fn chi_squared_pdf(x: f64, df: f64) -> Result<f64, String> {
    let x = check_probability_point(x, "x")?;
    let df = check_panel_df(df, "gl")?;
    finish_probability_scalar(
        grafito_geometry::statistics::chi_squared_pdf(x, df),
        "La densidad",
    )
}

/// Acumulada χ²(gl) en x: P(X ≤ x).
pub(crate) fn chi_squared_cdf(x: f64, df: f64) -> Result<f64, String> {
    let x = check_probability_point(x, "x")?;
    let df = check_panel_df(df, "gl")?;
    finish_probability_scalar(
        grafito_geometry::statistics::chi_squared_cdf(x, df),
        "La acumulada",
    )
}

/// Cuantil χ²: el x con P(X ≤ x) = p. Misma inversa que
/// `InverseChiSquared[p, gl]`.
pub(crate) fn chi_squared_quantile_honest(p: f64, df: f64) -> Result<f64, String> {
    let p = check_quantile_p(p)?;
    let df = check_panel_df(df, "gl")?;
    finish_probability_scalar(
        grafito_geometry::statistics::chi_squared_quantile(p, df),
        "El cuantil",
    )
}

/// Densidad F(gl1, gl2) en x. Mismo motor que `InverseF`.
pub(crate) fn f_distribution_pdf(x: f64, df1: f64, df2: f64) -> Result<f64, String> {
    let x = check_probability_point(x, "x")?;
    let df1 = check_panel_df(df1, "gl1")?;
    let df2 = check_panel_df(df2, "gl2")?;
    finish_probability_scalar(
        grafito_geometry::statistics::f_distribution_pdf(x, df1, df2),
        "La densidad",
    )
}

/// Acumulada F(gl1, gl2) en x: P(X ≤ x).
pub(crate) fn f_distribution_cdf(x: f64, df1: f64, df2: f64) -> Result<f64, String> {
    let x = check_probability_point(x, "x")?;
    let df1 = check_panel_df(df1, "gl1")?;
    let df2 = check_panel_df(df2, "gl2")?;
    finish_probability_scalar(
        grafito_geometry::statistics::f_distribution_cdf(x, df1, df2),
        "La acumulada",
    )
}

/// Cuantil F: el x con P(X ≤ x) = p. Misma inversa que
/// `InverseF[p, gl1, gl2]`.
pub(crate) fn f_quantile_honest(p: f64, df1: f64, df2: f64) -> Result<f64, String> {
    let p = check_quantile_p(p)?;
    let df1 = check_panel_df(df1, "gl1")?;
    let df2 = check_panel_df(df2, "gl2")?;
    finish_probability_scalar(
        grafito_geometry::statistics::f_quantile(p, df1, df2),
        "El cuantil",
    )
}

/// Cota de `k` en Geométrica: el loop del cuantil es O(k).
const MAX_GEOMETRIC_K: u64 = 100_000;

fn check_geometric_args(k: u64, p: f64) -> Result<(f64, u32), String> {
    if !p.is_finite() {
        return Err("p debe ser un número finito".to_string());
    }
    if !(0.0 < p && p <= 1.0) {
        return Err("p debe estar en el intervalo (0, 1]".to_string());
    }
    if k > MAX_GEOMETRIC_K {
        return Err(format!("k ≤ {MAX_GEOMETRIC_K} en este panel (cota de UI)"));
    }
    let k32 =
        u32::try_from(k.min(u64::from(u32::MAX))).map_err(|_| "k fuera de rango".to_string())?;
    Ok((p, k32))
}

/// Masa Geométrica(p) en k: P(X = k), k = fallos antes del primer éxito.
/// Delega en el motor (`geometric_pmf`), que no valida: el `Err` honesto
/// en español + las cotas viven acá.
pub(crate) fn geometric_pmf(k: u64, p: f64) -> Result<f64, String> {
    let (p, k32) = check_geometric_args(k, p)?;
    finish_probability_scalar(
        grafito_geometry::statistics::geometric_pmf(p, k32),
        "La puntual",
    )
}

/// Acumulada Geométrica(p) en k: P(X ≤ k). Delega en el motor.
pub(crate) fn geometric_cdf(k: u64, p: f64) -> Result<f64, String> {
    let (p, k32) = check_geometric_args(k, p)?;
    finish_probability_scalar(
        grafito_geometry::statistics::geometric_cdf(p, k32),
        "La acumulada",
    )
}

/// Cuantil Geométrica: el menor k con P(X ≤ k) ≥ p. Acumula `pmf` con la
/// misma cota del panel discreto; si no alcanza, `Err` honesto.
pub(crate) fn geometric_quantile_honest(p: f64, prob: f64) -> Result<u64, String> {
    let p = check_quantile_p(p)?;
    if !(prob.is_finite() && 0.0 < prob && prob <= 1.0) {
        return Err("p del modelo debe estar en el intervalo (0, 1]".to_string());
    }
    let cap = MAX_GEOMETRIC_K.min(10_000);
    let mut acc = 0.0;
    for k in 0..=cap {
        acc += geometric_pmf(k, prob)?;
        if acc >= p {
            return Ok(k);
        }
    }
    Err("La cola supera la cota de 10 001 pasos en este panel".to_string())
}

fn check_uniform_args(x: f64, a: f64, b: f64) -> Result<(f64, f64, f64), String> {
    let x = check_probability_point(x, "x")?;
    let a = check_probability_point(a, "a")?;
    let b = check_probability_point(b, "b")?;
    if a >= b {
        return Err("se requiere a < b".to_string());
    }
    Ok((x, a, b))
}

/// Densidad Uniforme(a, b) en x: 1/(b−a) dentro de [a, b], 0 fuera.
/// Delega en el motor; el `Err` honesto (a < b finitos) vive acá.
pub(crate) fn uniform_pdf(x: f64, a: f64, b: f64) -> Result<f64, String> {
    let (x, a, b) = check_uniform_args(x, a, b)?;
    finish_probability_scalar(
        grafito_geometry::statistics::uniform_pdf(x, a, b),
        "La densidad",
    )
}

/// Acumulada Uniforme(a, b) en x: P(X ≤ x). Delega en el motor.
pub(crate) fn uniform_cdf(x: f64, a: f64, b: f64) -> Result<f64, String> {
    let (x, a, b) = check_uniform_args(x, a, b)?;
    finish_probability_scalar(
        grafito_geometry::statistics::uniform_cdf(x, a, b),
        "La acumulada",
    )
}

/// Cuantil Uniforme: forma cerrada a + p·(b−a), sin iterar.
pub(crate) fn uniform_quantile_honest(p: f64, a: f64, b: f64) -> Result<f64, String> {
    let p = check_quantile_p(p)?;
    let a = check_probability_point(a, "a")?;
    let b = check_probability_point(b, "b")?;
    if a >= b {
        return Err("se requiere a < b".to_string());
    }
    finish_probability_scalar(a + p * (b - a), "El cuantil")
}

/// Prompt auto de velocidad para sliders Play. Acepta:
/// número («1.5»), palabra («lento/medio/rápido») o «N vueltas en S s».
/// Todo lo demás es `Err` honesto (nunca se inventa una velocidad).
pub(crate) fn parse_slider_prompt(input: &str) -> Result<f64, String> {
    let text = input.trim().to_lowercase();
    if text.is_empty() {
        return Err("Escribí una velocidad: 1.5, «medio» o «2 vueltas en 10 s»".to_string());
    }
    if let Ok(value) = text.replace(',', ".").parse::<f64>() {
        if !value.is_finite() {
            return Err("La velocidad debe ser finita".to_string());
        }
        if value.abs() > 6.0 {
            return Err("La velocidad vive en ±6 rad/s (mové el slider si querés más)".to_string());
        }
        return Ok(value);
    }
    match text.as_str() {
        "lento" | "lenta" | "slow" => return Ok(0.2),
        "medio" | "media" | "normal" => return Ok(0.5),
        "rapido" | "rápido" | "rapida" | "rápida" | "fast" => return Ok(2.0),
        _ => {}
    }
    // «N vueltas en S s»: números con punto o coma decimal.
    let numbers: Vec<f64> = text
        .split(|c: char| !(c.is_ascii_digit() || c == '.' || c == ',' || c == '-'))
        .filter(|token| !token.is_empty())
        .filter_map(|token| token.replace(',', ".").parse::<f64>().ok())
        .filter(|value| value.is_finite() && *value > 0.0)
        .collect();
    if (text.contains("vuelta") || text.contains("lap") || text.contains("giro"))
        && numbers.len() >= 2
    {
        let (turns, seconds) = (numbers[0], numbers[1]);
        if seconds <= 0.0 {
            return Err("Los segundos deben ser mayores que 0".to_string());
        }
        let speed = turns * 2.0 * std::f64::consts::PI / seconds;
        if speed > 6.0 {
            return Err(format!(
                "Eso da {speed:.2} rad/s: más de 6 rad/s no se sigue a ojo (bajá vueltas o subí segundos)"
            ));
        }
        return Ok(speed);
    }
    Err("No entendí: probá 1.5, «medio» o «2 vueltas en 10 s»".to_string())
}

/// Estado efímero del panel de probabilidad (vive en `ctx.data`, sin I/O).
/// `dist`: 0 Normal, 1 Binomial, 2 Poisson, 3 t-Student, 4 χ², 5 F (C2),
/// 6 Geométrica, 7 Uniforme (W-E: el motor ya las trae).
#[derive(Debug, Clone)]
struct ProbabilityPanelState {
    dist: u8,
    mu: f64,
    sigma: f64,
    x: f64,
    n: f64,
    p: f64,
    k: f64,
    lambda: f64,
    p_inv: f64,
    df: f64,
    df1: f64,
    df2: f64,
    ua: f64,
    ub: f64,
}

impl Default for ProbabilityPanelState {
    fn default() -> Self {
        Self {
            dist: 0,
            mu: 0.0,
            sigma: 1.0,
            x: 0.0,
            n: 10.0,
            p: 0.5,
            k: 5.0,
            lambda: 3.0,
            p_inv: 0.95,
            df: 10.0,
            df1: 5.0,
            df2: 10.0,
            ua: 0.0,
            ub: 1.0,
        }
    }
}

/// Estado efímero del prompt auto (texto + último error honesto).
#[derive(Debug, Clone, Default)]
struct TrigPromptState {
    text: String,
    error: Option<String>,
}

/// Curva continua + área P(X ≤ x_sel) del panel de probabilidad (C2).
/// Puro render: `density` viene del motor único; loop acotado (61 muestras).
/// Lo usan Normal, t-Student, χ² y F con el mismo sombreado.
#[allow(clippy::too_many_arguments)]
fn paint_continuous_density(
    painter: &egui::Painter,
    plot: egui::Rect,
    lo: f64,
    hi: f64,
    density: impl Fn(f64) -> f64,
    x_sel: f64,
    accent: egui::Color32,
    txt_dim: egui::Color32,
    right_label: String,
) {
    let plot_top = plot.min.y;
    let plot_bot = plot.max.y - 14.0;
    let plot_h = (plot_bot - plot_top).max(1.0);
    let plot_w = plot.width().max(1.0);
    const SAMPLES: usize = 61;
    let mut pts: Vec<(f64, f64)> = Vec::with_capacity(SAMPLES);
    for i in 0..SAMPLES {
        let x = lo + (hi - lo) * i as f64 / (SAMPLES - 1) as f64;
        let y = density(x).max(0.0);
        if y.is_finite() {
            pts.push((x, y));
        }
    }
    if pts.is_empty() {
        return;
    }
    let shade = accent.gamma_multiply(0.25);
    let ymax = pts
        .iter()
        .map(|(_, y)| *y)
        .fold(0.0f64, f64::max)
        .max(1e-12);
    let to_px = |x: f64| plot.min.x + ((x - lo) / (hi - lo)) as f32 * plot_w;
    let to_py = |y: f64| plot_bot - (y / ymax) as f32 * plot_h;
    for window in pts.windows(2) {
        let [(x0, y0), (x1, y1)] = [window[0], window[1]];
        if x1 <= x_sel {
            let bar = egui::Rect::from_min_max(
                egui::pos2(to_px(x0), to_py(y0.max(y1))),
                egui::pos2(to_px(x1), plot_bot),
            );
            painter.rect_filled(bar, 0.0, shade);
        }
        painter.line_segment(
            [
                egui::pos2(to_px(x0), to_py(y0)),
                egui::pos2(to_px(x1), to_py(y1)),
            ],
            egui::Stroke::new(1.5, accent),
        );
    }
    if x_sel.is_finite() && x_sel >= lo && x_sel <= hi {
        let px = to_px(x_sel);
        painter.line_segment(
            [egui::pos2(px, plot_top), egui::pos2(px, plot_bot)],
            egui::Stroke::new(1.0, accent.gamma_multiply(0.6)),
        );
    }
    painter.text(
        egui::pos2(plot.min.x, plot_bot + 2.0),
        egui::Align2::LEFT_TOP,
        format_statistic(lo),
        egui::FontId::proportional(TYPE_XS),
        txt_dim,
    );
    painter.text(
        egui::pos2(plot.max.x, plot_bot + 2.0),
        egui::Align2::RIGHT_TOP,
        right_label,
        egui::FontId::proportional(TYPE_XS),
        txt_dim,
    );
}

/// Resuelve gl para el plot sin `clamp` silencioso: fuera de cota es `Err`
/// honesto (el caller dibuja "fuera de cota" en vez de una curva falsa).
/// W-A: `df = MAX_PANEL_DF + 1` → `Err`, no curva clampada a 500.
pub(crate) fn plot_df_or_fuera_de_cota(value: f64, what: &str) -> Result<f64, String> {
    check_panel_df(value, what)
}

/// Etiqueta honesta dentro del plot cuando no hay curva para dibujar
/// (fuera de cota, parámetro inválido). Piel pura: solo pinta texto.
fn paint_plot_message(
    painter: &egui::Painter,
    plot: egui::Rect,
    message: &str,
    txt_dim: egui::Color32,
) {
    painter.text(
        plot.center(),
        egui::Align2::CENTER_CENTER,
        format!("fuera de cota: {message}"),
        egui::FontId::proportional(TYPE_XS),
        txt_dim,
    );
}

/// Curva + área P(X ≤ x) del panel de probabilidad (F3b, extendido a
/// T/Chi²/F en C2). Puro render sobre el estado efímero: sin I/O, sin
/// spawn, loops acotados (≤61 muestras continuas, ≤256 barras discretas).
/// Reusa los wrappers de este archivo (motor único, sin duplicar).
fn draw_probability_plot(
    ui: &mut egui::Ui,
    state: &ProbabilityPanelState,
    accent: egui::Color32,
    txt_dim: egui::Color32,
) {
    let plot_h = 84.0;
    let (rect, _) = ui.allocate_exact_size(
        egui::vec2(ui.available_width(), plot_h + 14.0),
        egui::Sense::hover(),
    );
    if !ui.is_rect_visible(rect) {
        return;
    }
    let painter = ui.painter().with_clip_rect(rect);
    let plot = rect.shrink2(egui::vec2(2.0, 2.0));
    let plot_bot = plot.max.y - 14.0;
    let plot_h = (plot_bot - plot.min.y).max(1.0);
    let plot_w = plot.width().max(1.0);
    painter.line_segment(
        [
            egui::pos2(plot.min.x, plot_bot),
            egui::pos2(plot.max.x, plot_bot),
        ],
        egui::Stroke::new(1.0, txt_dim.gamma_multiply(0.35)),
    );
    match state.dist {
        0 => {
            if !state.mu.is_finite() || !state.sigma.is_finite() || state.sigma <= 0.0 {
                paint_plot_message(&painter, plot, "σ debe ser mayor que 0", txt_dim);
                return;
            }
            let (lo, hi) = (state.mu - 4.0 * state.sigma, state.mu + 4.0 * state.sigma);
            if !(lo.is_finite() && hi.is_finite() && hi > lo) {
                paint_plot_message(&painter, plot, "μ/σ fuera de rango", txt_dim);
                return;
            }
            let (mu, sigma) = (state.mu, state.sigma);
            paint_continuous_density(
                &painter,
                plot,
                lo,
                hi,
                // W-A: `NaN` ante error (el filtro de `paint_continuous_density`
                // lo salta) en vez de `0.0` que dibujaba un plano falso en 0.
                |x| normal_pdf(x, mu, sigma).unwrap_or(f64::NAN),
                state.x,
                accent,
                txt_dim,
                "P(X≤x) sombreada · μ±4σ".to_string(),
            );
        }
        1 => {
            let n = state.n.round().clamp(1.0, MAX_BINOMIAL_N as f64) as u64;
            let k_sel = (state.k.round().max(0.0) as u64).min(n);
            let mean = n as f64 * state.p.clamp(0.0, 1.0);
            let sd = (n as f64 * state.p.clamp(0.0, 1.0) * (1.0 - state.p.clamp(0.0, 1.0))).sqrt();
            let (mut lo, mut hi) = if n <= 61 {
                (0u64, n)
            } else {
                let span = (4.0 * sd).ceil().max(8.0) as u64;
                let lo = (mean.floor().max(0.0) as u64)
                    .saturating_sub(span / 2)
                    .min(n);
                let hi = (lo + 60).min(n);
                (hi.saturating_sub(60).min(lo), hi)
            };
            lo = lo.min(k_sel);
            hi = hi.max(k_sel);
            if hi < lo {
                return;
            }
            let count = (hi - lo + 1) as usize;
            if count == 0 || count > 256 {
                return;
            }
            let mut masses: Vec<(u64, f64)> = Vec::with_capacity(count);
            for k in lo..=hi {
                let pmf = binomial_pmf(k, n, state.p).unwrap_or(0.0).max(0.0);
                if pmf.is_finite() {
                    masses.push((k, pmf));
                }
                if masses.len() >= 256 {
                    break;
                }
            }
            if masses.is_empty() {
                return;
            }
            let ymax = masses
                .iter()
                .map(|(_, y)| *y)
                .fold(0.0f64, f64::max)
                .max(1e-12);
            let bar_w = plot_w / masses.len() as f32;
            for (i, (k, pmf)) in masses.iter().enumerate() {
                let h = (*pmf / ymax) as f32 * plot_h;
                let bar = egui::Rect::from_min_size(
                    egui::pos2(plot.min.x + i as f32 * bar_w + 1.0, plot_bot - h),
                    egui::vec2((bar_w - 2.0).max(1.0), h.max(1.0)),
                );
                let selected = *k <= k_sel;
                painter.rect_filled(
                    bar,
                    2.0,
                    if selected {
                        accent
                    } else {
                        txt_dim.gamma_multiply(0.35)
                    },
                );
            }
            painter.text(
                egui::pos2(plot.min.x, plot_bot + 2.0),
                egui::Align2::LEFT_TOP,
                format!("k={lo}"),
                egui::FontId::proportional(TYPE_XS),
                txt_dim,
            );
            painter.text(
                egui::pos2(plot.max.x, plot_bot + 2.0),
                egui::Align2::RIGHT_TOP,
                if n > 61 {
                    format!("ventana {lo}..={hi} de 0..={n} · ≤k sombreado")
                } else {
                    "k ≤ seleccionado sombreado".to_string()
                },
                egui::FontId::proportional(TYPE_XS),
                txt_dim,
            );
        }
        2 => {
            let k_sel = state.k.round().max(0.0) as u64;
            let lambda = state.lambda;
            if !lambda.is_finite() || lambda <= 0.0 {
                return;
            }
            let hi_default = (lambda * 3.0 + 10.0).ceil().max(8.0) as u64;
            let hi = hi_default.min(60).max(k_sel).min(MAX_POISSON_K);
            let masses: Vec<(u64, f64)> = (0..=hi)
                .filter_map(|k| {
                    let pmf = poisson_pmf(k, lambda).unwrap_or(0.0).max(0.0);
                    pmf.is_finite().then_some((k, pmf))
                })
                .collect();
            if masses.is_empty() {
                return;
            }
            let ymax = masses
                .iter()
                .map(|(_, y)| *y)
                .fold(0.0f64, f64::max)
                .max(1e-12);
            let bar_w = plot_w / masses.len() as f32;
            for (i, (k, pmf)) in masses.iter().enumerate() {
                let h = (*pmf / ymax) as f32 * plot_h;
                let bar = egui::Rect::from_min_size(
                    egui::pos2(plot.min.x + i as f32 * bar_w + 1.0, plot_bot - h),
                    egui::vec2((bar_w - 2.0).max(1.0), h.max(1.0)),
                );
                let selected = *k <= k_sel;
                painter.rect_filled(
                    bar,
                    2.0,
                    if selected {
                        accent
                    } else {
                        txt_dim.gamma_multiply(0.35)
                    },
                );
            }
            painter.text(
                egui::pos2(plot.min.x, plot_bot + 2.0),
                egui::Align2::LEFT_TOP,
                "k=0".to_string(),
                egui::FontId::proportional(TYPE_XS),
                txt_dim,
            );
            painter.text(
                egui::pos2(plot.max.x, plot_bot + 2.0),
                egui::Align2::RIGHT_TOP,
                format!("0..={hi} · ≤k sombreado"),
                egui::FontId::proportional(TYPE_XS),
                txt_dim,
            );
        }
        3 => {
            // W-A: sin `clamp` silencioso — fuera de cota se avisa, no se
            // dibuja la curva de gl=500 como si fuera la pedida.
            let df = match plot_df_or_fuera_de_cota(state.df, "gl") {
                Ok(df) => df,
                Err(message) => {
                    paint_plot_message(&painter, plot, &message, txt_dim);
                    return;
                }
            };
            if !state.x.is_finite() {
                return;
            }
            paint_continuous_density(
                &painter,
                plot,
                -6.0,
                6.0,
                |x| student_t_pdf(x, df).unwrap_or(f64::NAN),
                state.x,
                accent,
                txt_dim,
                format!("P(X≤x) sombreada · t(gl={}) ±6", format_statistic(df)),
            );
        }
        4 => {
            let df = match plot_df_or_fuera_de_cota(state.df, "gl") {
                Ok(df) => df,
                Err(message) => {
                    paint_plot_message(&painter, plot, &message, txt_dim);
                    return;
                }
            };
            if !state.x.is_finite() {
                return;
            }
            let hi = (df * 4.0 + 8.0).max(state.x).max(8.0);
            if !hi.is_finite() {
                return;
            }
            paint_continuous_density(
                &painter,
                plot,
                0.0,
                hi,
                |x| chi_squared_pdf(x, df).unwrap_or(f64::NAN),
                state.x,
                accent,
                txt_dim,
                format!("P(X≤x) sombreada · χ²(gl={})", format_statistic(df)),
            );
        }
        5 => {
            let (df1, df2) = match (
                plot_df_or_fuera_de_cota(state.df1, "gl1"),
                plot_df_or_fuera_de_cota(state.df2, "gl2"),
            ) {
                (Ok(df1), Ok(df2)) => (df1, df2),
                (Err(message), _) | (_, Err(message)) => {
                    paint_plot_message(&painter, plot, &message, txt_dim);
                    return;
                }
            };
            if !state.x.is_finite() {
                return;
            }
            let hi = 6.0f64.max(state.x).max(1.0);
            if !hi.is_finite() {
                return;
            }
            paint_continuous_density(
                &painter,
                plot,
                0.0,
                hi,
                |x| f_distribution_pdf(x, df1, df2).unwrap_or(f64::NAN),
                state.x,
                accent,
                txt_dim,
                format!(
                    "P(X≤x) sombreada · F({},{})",
                    format_statistic(df1),
                    format_statistic(df2)
                ),
            );
        }
        6 => {
            let k_sel = state.k.round().max(0.0) as u64;
            let prob = state.p;
            if !(prob.is_finite() && 0.0 < prob && prob <= 1.0) {
                paint_plot_message(&painter, plot, "p debe estar en (0, 1]", txt_dim);
                return;
            }
            let mean = (1.0 - prob) / prob;
            let hi_default = (mean * 3.0 + 10.0).ceil().max(8.0) as u64;
            let hi = hi_default.min(60).max(k_sel).min(MAX_GEOMETRIC_K);
            let masses: Vec<(u64, f64)> = (0..=hi)
                .filter_map(|k| {
                    let pmf = geometric_pmf(k, prob).unwrap_or(0.0).max(0.0);
                    pmf.is_finite().then_some((k, pmf))
                })
                .collect();
            if masses.is_empty() {
                return;
            }
            let ymax = masses
                .iter()
                .map(|(_, y)| *y)
                .fold(0.0f64, f64::max)
                .max(1e-12);
            let bar_w = plot_w / masses.len() as f32;
            for (i, (k, pmf)) in masses.iter().enumerate() {
                let h = (*pmf / ymax) as f32 * plot_h;
                let bar = egui::Rect::from_min_size(
                    egui::pos2(plot.min.x + i as f32 * bar_w + 1.0, plot_bot - h),
                    egui::vec2((bar_w - 2.0).max(1.0), h.max(1.0)),
                );
                let selected = *k <= k_sel;
                painter.rect_filled(
                    bar,
                    2.0,
                    if selected {
                        accent
                    } else {
                        txt_dim.gamma_multiply(0.35)
                    },
                );
            }
            painter.text(
                egui::pos2(plot.min.x, plot_bot + 2.0),
                egui::Align2::LEFT_TOP,
                "k=0".to_string(),
                egui::FontId::proportional(TYPE_XS),
                txt_dim,
            );
            painter.text(
                egui::pos2(plot.max.x, plot_bot + 2.0),
                egui::Align2::RIGHT_TOP,
                format!("0..={hi} · ≤k sombreado"),
                egui::FontId::proportional(TYPE_XS),
                txt_dim,
            );
        }
        7 => {
            let (a, b) = (state.ua, state.ub);
            if !(a.is_finite() && b.is_finite() && a < b) {
                paint_plot_message(&painter, plot, "se requiere a < b", txt_dim);
                return;
            }
            if !state.x.is_finite() {
                return;
            }
            paint_continuous_density(
                &painter,
                plot,
                a,
                b,
                |x| uniform_pdf(x, a, b).unwrap_or(f64::NAN),
                state.x,
                accent,
                txt_dim,
                format!(
                    "P(X≤x) sombreada · U({},{})",
                    format_statistic(a),
                    format_statistic(b)
                ),
            );
        }
        _ => {}
    }
}

/// Sección Probabilidad: Normal / Binomial / Poisson / t-Student / χ² / F
/// / Geométrica / Uniforme con PDF/CDF honestos. Llamada desde el panel Vista (alcanzable) — sin
/// botones mudos: el selector cambia la distribución y cada parámetro
/// recalcula en vivo.
pub(crate) fn draw_probability_section(ui: &mut egui::Ui, ctx: &egui::Context) {
    let id = egui::Id::new("gc_probability_state");
    let mut state: ProbabilityPanelState = ctx
        .data_mut(|data| data.get_temp::<ProbabilityPanelState>(id))
        .unwrap_or_default();
    let (_is_dark, accent, _fill, _sep, txt_col, txt_dim, hdr_col) = panel_theme_local(ctx);

    ui.label(
        egui::RichText::new("Probabilidad")
            .color(hdr_col)
            .size(TYPE_SM)
            .strong(),
    );
    ui.horizontal_wrapped(|ui| {
        for (index, name, tip) in [
            (0u8, "Normal", "Gaussiana μ, σ: densidad y acumulada"),
            (1u8, "Binomial", "n ensayos, p éxito: P(X = k) y P(X ≤ k)"),
            (2u8, "Poisson", "Tasa λ: P(X = k) y P(X ≤ k)"),
            (
                3u8,
                "t-Student",
                "gl grados de libertad: densidad y acumulada",
            ),
            (4u8, "χ²", "gl grados de libertad: densidad y acumulada"),
            (5u8, "F", "gl1, gl2: densidad y acumulada"),
            (
                6u8,
                "Geométrica",
                "p éxito: fallos antes del primer éxito, P(X = k) y P(X ≤ k)",
            ),
            (7u8, "Uniforme", "a, b: densidad 1/(b−a) y acumulada"),
        ] {
            let selected = state.dist == index;
            if ui
                .selectable_label(selected, name)
                .on_hover_text(tip)
                .clicked()
            {
                state.dist = index;
                ctx.request_repaint();
            }
        }
    });
    ui.add_space(SPACE_XS);

    // Parámetros por distribución (sliders acotados + lectura honesta).
    match state.dist {
        0 => {
            ui.add(egui::Slider::new(&mut state.mu, -10.0..=10.0).text("μ media"));
            ui.add(egui::Slider::new(&mut state.sigma, 0.1..=5.0).text("σ desvío"));
            ui.add(egui::Slider::new(&mut state.x, -10.0..=10.0).text("x punto"));
        }
        1 => {
            ui.add(egui::Slider::new(&mut state.n, 1.0..=MAX_BINOMIAL_N as f64).text("n ensayos"));
            ui.add(egui::Slider::new(&mut state.p, 0.0..=1.0).text("p éxito"));
            ui.add(egui::Slider::new(&mut state.k, 0.0..=MAX_BINOMIAL_N as f64).text("k éxitos"));
        }
        2 => {
            ui.add(
                egui::Slider::new(&mut state.lambda, 0.1..=20.0)
                    .logarithmic(true)
                    .text("λ tasa"),
            );
            ui.add(egui::Slider::new(&mut state.k, 0.0..=50.0).text("k eventos"));
        }
        3 => {
            ui.add(egui::Slider::new(&mut state.df, 1.0..=30.0).text("gl libertad"));
            ui.add(egui::Slider::new(&mut state.x, -6.0..=6.0).text("x punto"));
        }
        4 => {
            ui.add(egui::Slider::new(&mut state.df, 1.0..=30.0).text("gl libertad"));
            ui.add(egui::Slider::new(&mut state.x, 0.0..=20.0).text("x punto"));
        }
        5 => {
            ui.add(egui::Slider::new(&mut state.df1, 1.0..=30.0).text("gl1"));
            ui.add(egui::Slider::new(&mut state.df2, 1.0..=30.0).text("gl2"));
            ui.add(egui::Slider::new(&mut state.x, 0.0..=10.0).text("x punto"));
        }
        6 => {
            ui.add(egui::Slider::new(&mut state.p, 0.01..=1.0).text("p éxito"));
            ui.add(egui::Slider::new(&mut state.k, 0.0..=50.0).text("k fallos"));
        }
        _ => {
            ui.add(egui::Slider::new(&mut state.ua, -10.0..=10.0).text("a mínimo"));
            ui.add(egui::Slider::new(&mut state.ub, -10.0..=10.0).text("b máximo"));
            ui.add(egui::Slider::new(&mut state.x, -10.0..=10.0).text("x punto"));
        }
    }
    ui.add_space(SPACE_XS);

    let result: Result<(f64, f64, String), String> = (|| {
        Ok(match state.dist {
            0 => {
                let (pdf, cdf) = (
                    normal_pdf(state.x, state.mu, state.sigma)?,
                    normal_cdf(state.x, state.mu, state.sigma)?,
                );
                (pdf, cdf, format!("f({})", format_statistic(state.x)))
            }
            1 => {
                let (n, k) = (
                    state.n.round() as u64,
                    state.k.round().min(state.n.round()) as u64,
                );
                let (pmf, cdf) = (binomial_pmf(k, n, state.p)?, binomial_cdf(k, n, state.p)?);
                (pmf, cdf, format!("P(X = {k}) · n = {n}"))
            }
            2 => {
                let k = state.k.round().max(0.0) as u64;
                let (pmf, cdf) = (poisson_pmf(k, state.lambda)?, poisson_cdf(k, state.lambda)?);
                (
                    pmf,
                    cdf,
                    format!("P(X = {k}) · λ = {}", format_statistic(state.lambda)),
                )
            }
            3 => {
                let (pdf, cdf) = (
                    student_t_pdf(state.x, state.df)?,
                    student_t_cdf(state.x, state.df)?,
                );
                (
                    pdf,
                    cdf,
                    format!(
                        "f({}) · t(gl={})",
                        format_statistic(state.x),
                        format_statistic(state.df)
                    ),
                )
            }
            4 => {
                let (pdf, cdf) = (
                    chi_squared_pdf(state.x, state.df)?,
                    chi_squared_cdf(state.x, state.df)?,
                );
                (
                    pdf,
                    cdf,
                    format!(
                        "f({}) · χ²(gl={})",
                        format_statistic(state.x),
                        format_statistic(state.df)
                    ),
                )
            }
            5 => {
                let (pdf, cdf) = (
                    f_distribution_pdf(state.x, state.df1, state.df2)?,
                    f_distribution_cdf(state.x, state.df1, state.df2)?,
                );
                (
                    pdf,
                    cdf,
                    format!(
                        "f({}) · F({},{})",
                        format_statistic(state.x),
                        format_statistic(state.df1),
                        format_statistic(state.df2)
                    ),
                )
            }
            6 => {
                let k = state.k.round().max(0.0) as u64;
                let (pmf, cdf) = (geometric_pmf(k, state.p)?, geometric_cdf(k, state.p)?);
                (
                    pmf,
                    cdf,
                    format!("P(X = {k}) · p = {}", format_statistic(state.p),),
                )
            }
            _ => {
                let (pdf, cdf) = (
                    uniform_pdf(state.x, state.ua, state.ub)?,
                    uniform_cdf(state.x, state.ua, state.ub)?,
                );
                (
                    pdf,
                    cdf,
                    format!(
                        "f({}) · U({},{})",
                        format_statistic(state.x),
                        format_statistic(state.ua),
                        format_statistic(state.ub)
                    ),
                )
            }
        })
    })();
    match result {
        Ok((pdf, cdf, detail)) => {
            egui::Grid::new("gc_prob_grid")
                .num_columns(2)
                .striped(true)
                .spacing([10.0, 4.0])
                .show(ui, |ui| {
                    ui.label(egui::RichText::new("Caso").color(txt_dim).size(TYPE_SM));
                    ui.label(
                        egui::RichText::new(detail)
                            .color(txt_col)
                            .size(TYPE_SM)
                            .strong(),
                    );
                    ui.end_row();
                    ui.label(
                        egui::RichText::new(
                            if state.dist == 1 || state.dist == 2 || state.dist == 6 {
                                "Puntual"
                            } else {
                                "Densidad"
                            },
                        )
                        .color(txt_dim)
                        .size(TYPE_SM),
                    );
                    ui.label(
                        egui::RichText::new(format!("{pdf:.6}"))
                            .color(accent)
                            .size(TYPE_SM)
                            .strong(),
                    );
                    ui.end_row();
                    ui.label(
                        egui::RichText::new("Acumulada")
                            .color(txt_dim)
                            .size(TYPE_SM),
                    );
                    ui.label(
                        egui::RichText::new(format!("{cdf:.6}"))
                            .color(accent)
                            .size(TYPE_SM)
                            .strong(),
                    );
                    ui.end_row();
                });
            ui.label(
                egui::RichText::new("f64 exacto; colas extremas pueden subfluir a 0.")
                    .color(txt_dim)
                    .size(TYPE_XS),
            );
            ui.add_space(SPACE_XS);
            draw_probability_plot(ui, &state, accent, txt_dim);
        }
        Err(error) => {
            ui.label(
                egui::RichText::new(error)
                    .color(current_theme(ctx).danger)
                    .size(TYPE_XS),
            );
        }
    }
    ui.add_space(SPACE_XS);
    ui.label(
        egui::RichText::new("Cuantil (inversa)")
            .color(txt_dim)
            .size(TYPE_SM)
            .strong(),
    );
    ui.add(
        egui::Slider::new(&mut state.p_inv, 0.001..=0.999)
            .text("p cuantil")
            .fixed_decimals(3),
    );
    let quantile: Result<String, String> = (|| {
        Ok(match state.dist {
            0 => {
                let q = normal_quantile_honest(state.p_inv, state.mu, state.sigma)?;
                format!("x con P(X≤x)={:.3} → {}", state.p_inv, format_statistic(q))
            }
            1 => {
                let n = state.n.round().clamp(1.0, MAX_BINOMIAL_N as f64) as u64;
                let q = binomial_quantile_honest(state.p_inv, n, state.p)?;
                format!("menor k con P(X≤k)≥{:.3} → {q} (n={n})", state.p_inv)
            }
            2 => {
                let q = poisson_quantile_honest(state.p_inv, state.lambda)?;
                format!(
                    "menor k con P(X≤k)≥{:.3} → {q} (λ={})",
                    state.p_inv,
                    format_statistic(state.lambda)
                )
            }
            3 => {
                let q = student_t_quantile_honest(state.p_inv, state.df)?;
                format!(
                    "x con P(X≤x)={:.3} → {} (gl={})",
                    state.p_inv,
                    format_statistic(q),
                    format_statistic(state.df)
                )
            }
            4 => {
                let q = chi_squared_quantile_honest(state.p_inv, state.df)?;
                format!(
                    "x con P(X≤x)={:.3} → {} (gl={})",
                    state.p_inv,
                    format_statistic(q),
                    format_statistic(state.df)
                )
            }
            5 => {
                let q = f_quantile_honest(state.p_inv, state.df1, state.df2)?;
                format!(
                    "x con P(X≤x)={:.3} → {} (gl1={}, gl2={})",
                    state.p_inv,
                    format_statistic(q),
                    format_statistic(state.df1),
                    format_statistic(state.df2)
                )
            }
            6 => {
                let q = geometric_quantile_honest(state.p_inv, state.p)?;
                format!(
                    "menor k con P(X≤k)≥{:.3} → {q} (p={})",
                    state.p_inv,
                    format_statistic(state.p)
                )
            }
            _ => {
                let q = uniform_quantile_honest(state.p_inv, state.ua, state.ub)?;
                format!(
                    "x con P(X≤x)={:.3} → {} (a={}, b={})",
                    state.p_inv,
                    format_statistic(q),
                    format_statistic(state.ua),
                    format_statistic(state.ub)
                )
            }
        })
    })();
    match quantile {
        Ok(text) => {
            ui.label(
                egui::RichText::new(text)
                    .color(txt_col)
                    .size(TYPE_SM)
                    .strong(),
            );
            ui.label(
                egui::RichText::new(
                    "Misma inversa que InverseNormal/InverseT/InverseChiSquared/InverseF (motor único); Geométrica acumula pmf y Uniforme usa a+p·(b−a).",
                )
                .color(txt_dim)
                .size(TYPE_XS),
            );
        }
        Err(error) => {
            ui.label(
                egui::RichText::new(error)
                    .color(current_theme(ctx).danger)
                    .size(TYPE_XS),
            );
        }
    }
    ctx.data_mut(|data| data.insert_temp(id, state));
}

/// Ventana visible de la hoja vinculada (F3b). La hoja real vive en el
/// documento (`Document::MAX_SPREADSHEET_ROWS/COLS = 400×400`,
/// `MAX_SPREADSHEET_RECOMPUTE_CELLS = 10_000`): la UI solo muestra esta
/// ventana por rendimiento; el resto se edita con `FillColumn`/`FillCells`/
/// `FillRow` o la serie de abajo (`FillSeries`, mismo motor).
const SHEET_VIEW_COLS: usize = 6;
const SHEET_VIEW_ROWS: usize = 8;

/// Borradores + errores por celda de la hoja editable. Vive en `ctx.data`
/// (cero campos nuevos en `GrafitoApp`, cero I/O): la fuente canónica sigue
/// en `Document.spreadsheet`; acá solo el texto en edición y el último error
/// honesto por celda (una fórmula rota muestra `—` sin voltear la hoja).
#[derive(Debug, Clone, Default)]
struct SheetEditState {
    drafts: HashMap<(usize, usize), String>,
    errors: HashMap<(usize, usize), String>,
}

fn sheet_col_label(col: usize) -> String {
    let mut column = col;
    let mut letters = String::new();
    loop {
        letters.push(char::from(b'A' + (column % 26) as u8));
        if column < 26 {
            break;
        }
        column = column / 26 - 1;
    }
    letters.chars().rev().collect()
}

/// Grilla editable A1:F8 sobre `Document.spreadsheet` (F3b). Cada celda edita
/// su fuente (`=A1+B1`, `=x(A)`, `(A1, B1*2)`); el commit es por celda vía
/// `stage_spreadsheet_cell_edits` (atómico, con undo) y el error queda en esa
/// celda sin voltear la hoja (una fórmula rota muestra `—`). Ventana acotada
/// por rendimiento; los presupuestos reales (400×400/10k) los impone el core.
fn draw_sheet_editable_grid(ui: &mut egui::Ui, app: &mut GrafitoApp) {
    let ctx = ui.ctx().clone();
    let (_is_dark, _accent, _fill, _sep, txt_col, txt_dim, hdr_col) = panel_theme_local(&ctx);
    let id = egui::Id::new("gc_sheet_edit_state");
    let mut edit: SheetEditState = ctx
        .data_mut(|data| data.get_temp::<SheetEditState>(id))
        .unwrap_or_default();
    let mut snapshot = crate::app::DeferredPanelSnapshot::new(app.undo_stack.len());
    let mut dirty: Vec<(usize, usize, String)> = Vec::new();

    egui::Grid::new("gc_sheet_editable")
        .num_columns(SHEET_VIEW_COLS + 1)
        .striped(true)
        .spacing([4.0, 2.0])
        .show(ui, |ui| {
            ui.label(egui::RichText::new("").size(TYPE_XS));
            for col in 0..SHEET_VIEW_COLS {
                ui.label(
                    egui::RichText::new(sheet_col_label(col))
                        .color(hdr_col)
                        .size(TYPE_XS)
                        .strong(),
                );
            }
            ui.end_row();
            for row in 0..SHEET_VIEW_ROWS {
                ui.label(
                    egui::RichText::new(format!("{}", row + 1))
                        .color(txt_dim)
                        .size(TYPE_XS),
                );
                for col in 0..SHEET_VIEW_COLS {
                    let source = app.document.get_spreadsheet_cell(row, col);
                    let draft = edit
                        .drafts
                        .entry((row, col))
                        .or_insert_with(|| source.clone());
                    if !edit.errors.contains_key(&(row, col)) && *draft != source {
                        // Fuente cambió por fuera (otra celda/undo): re-sincroniza.
                        *draft = source.clone();
                    }
                    let mut text = draft.clone();
                    let resp = ui.add_sized(
                        [64.0, 18.0],
                        egui::TextEdit::singleline(&mut text)
                            .hint_text("—")
                            .font(egui::FontId::proportional(TYPE_XS)),
                    );
                    if resp.changed() {
                        *draft = text.clone();
                        edit.errors.remove(&(row, col));
                    }
                    let enter =
                        resp.lost_focus() && ui.input(|input| input.key_pressed(egui::Key::Enter));
                    if enter && *draft != source {
                        dirty.push((row, col, draft.clone()));
                    }
                    // Valor computado honesto debajo de la fuente.
                    let computed = app.document.eval_spreadsheet_cell(row, col);
                    let value_text = match (computed, source.trim().is_empty()) {
                        (_, true) => String::new(),
                        (Some(v), false) => format!("= {}", format_statistic(v)),
                        (None, false) => "—".to_string(),
                    };
                    if !value_text.is_empty() {
                        ui.label(
                            egui::RichText::new(value_text)
                                .color(if computed.is_some() { txt_col } else { txt_dim })
                                .size(TYPE_XS),
                        );
                    }
                    if let Some(error) = edit.errors.get(&(row, col)) {
                        ui.label(
                            egui::RichText::new(error)
                                .color(current_theme(&ctx).danger)
                                .size(TYPE_XS),
                        );
                    }
                }
                ui.end_row();
            }
        });
    // “Aplicar” compromete todas las celdas sucias (ordenadas, una por vez
    // para error por celda); Enter ya encoló la celda actual arriba.
    let mut apply_all = ui
        .small_button("Aplicar hoja")
        .on_hover_text("Compromete las celdas editadas (una por vez, con undo)")
        .clicked();
    for ((row, col), draft) in &edit.drafts {
        let source = app.document.get_spreadsheet_cell(*row, *col);
        if *draft != source && !dirty.iter().any(|(r, c, _)| r == row && c == col) {
            apply_all = true;
            break;
        }
    }
    if apply_all {
        let mut batch: Vec<(usize, usize, String)> = edit
            .drafts
            .iter()
            .filter_map(|((row, col), draft)| {
                let source = app.document.get_spreadsheet_cell(*row, *col);
                (*draft != source).then(|| (*row, *col, draft.clone()))
            })
            .collect();
        batch.sort_unstable_by_key(|(row, col, _)| (*row, *col));
        dirty.extend(batch);
        dirty.sort_unstable_by_key(|(row, col, _)| (*row, *col));
        dirty.dedup_by_key(|(row, col, _)| (*row, *col));
    }
    if !dirty.is_empty() {
        snapshot.capture(&app.document);
        let mut ok = 0usize;
        for (row, col, value) in dirty {
            let label = format!("{}{}", sheet_col_label(col), row + 1);
            match app
                .document
                .stage_spreadsheet_cell_edits(&[(row, col, value)])
            {
                Ok(staged) => {
                    app.document = staged;
                    edit.drafts.remove(&(row, col));
                    edit.errors.remove(&(row, col));
                    ok += 1;
                }
                Err(error) => {
                    edit.errors.insert((row, col), format!("{label}: {error}"));
                }
            }
        }
        if ok > 0 {
            app.cas_result = format!("Hoja: {ok} celda(s) actualizada(s)");
        } else if let Some(((row, col), _)) = edit.drafts.iter().next() {
            let _ = (row, col);
        }
        snapshot.save_if_semantically_changed(
            &mut app.document,
            &mut app.undo_stack,
            &mut app.redo_stack,
        );
        ctx.request_repaint();
    }
    ui.label(
        egui::RichText::new(format!(
            "Ventana A1:{}{} de 400×400 · 10 000 celdas recomputables (core). `—` = fórmula sin resolver, la hoja sigue viva.",
            sheet_col_label(SHEET_VIEW_COLS - 1),
            SHEET_VIEW_ROWS
        ))
        .color(txt_dim)
        .size(TYPE_XS),
    );
    ctx.data_mut(|data| data.insert_temp(id, edit));
}

/// Fila de la hoja viva: una tabla del documento con sus columnas clonadas
/// (lectura puntual por frame; las celdas editables están arriba).
struct SheetTable {
    id: ObjectId,
    label: String,
    x_name: String,
    y_name: String,
    xs: Vec<f64>,
    ys: Vec<f64>,
}

/// Estado efímero de la fila de autorrelleno por serie (vive en
/// `ctx.data`, sin I/O): rango 1D + inicio/paso como texto + modo.
#[derive(Debug, Clone)]
struct SheetSeriesState {
    range: String,
    start: String,
    step: String,
    geometric: bool,
    error: Option<String>,
}

impl Default for SheetSeriesState {
    fn default() -> Self {
        Self {
            range: "A1:A8".to_string(),
            start: "1".to_string(),
            step: "1".to_string(),
            geometric: false,
            error: None,
        }
    }
}

/// Escalar numérico del formulario de serie (acepta coma decimal).
/// `Err` honesto en español, nunca se inventa un valor.
fn parse_series_scalar(text: &str, what: &str) -> Result<f64, String> {
    let value: f64 = text
        .trim()
        .replace(',', ".")
        .parse()
        .map_err(|_| format!("Serie: {what} debe ser un número"))?;
    if value.is_finite() {
        Ok(value)
    } else {
        Err(format!("Serie: {what} debe ser finito"))
    }
}

/// Fila de autorrelleno por serie (frente C2): mismo motor que el comando
/// `FillSeries` (lineal `inicio+paso·i` o geométrica `inicio·pasoⁱ` sobre
/// un rango 1D). Commit atómico con undo vía `stage_spreadsheet_cell_edits`;
/// el error queda en la fila sin voltear la hoja.
fn draw_sheet_series_row(ui: &mut egui::Ui, app: &mut GrafitoApp) {
    let ctx = ui.ctx().clone();
    let (_is_dark, _accent, _fill, _sep, _txt_col, txt_dim, _hdr_col) = panel_theme_local(&ctx);
    let id = egui::Id::new("gc_sheet_series_state");
    let mut series: SheetSeriesState = ctx
        .data_mut(|data| data.get_temp::<SheetSeriesState>(id))
        .unwrap_or_default();
    ui.horizontal_wrapped(|ui| {
        ui.label(
            egui::RichText::new("Serie:")
                .color(txt_dim)
                .size(TYPE_XS)
                .strong(),
        );
        ui.add_sized(
            [72.0, 18.0],
            egui::TextEdit::singleline(&mut series.range)
                .hint_text("A1:A8")
                .font(egui::FontId::proportional(TYPE_XS)),
        );
        ui.label(egui::RichText::new("inicio").color(txt_dim).size(TYPE_XS));
        ui.add_sized(
            [52.0, 18.0],
            egui::TextEdit::singleline(&mut series.start)
                .hint_text("1")
                .font(egui::FontId::proportional(TYPE_XS)),
        );
        ui.label(egui::RichText::new("paso").color(txt_dim).size(TYPE_XS));
        ui.add_sized(
            [52.0, 18.0],
            egui::TextEdit::singleline(&mut series.step)
                .hint_text("1")
                .font(egui::FontId::proportional(TYPE_XS)),
        );
        ui.checkbox(&mut series.geometric, "geom.");
        let apply = ui
            .small_button("Aplicar serie")
            .on_hover_text(
                "Rellena el rango con serie lineal (inicio+paso·i) o geométrica (inicio·pasoⁱ); mismo motor que FillSeries",
            )
            .clicked();
        if apply {
            series.error = None;
            let outcome: Result<usize, String> = (|| {
                let start = parse_series_scalar(&series.start, "inicio")?;
                let step = parse_series_scalar(&series.step, "paso")?;
                let kind = if series.geometric {
                    spreadsheet_series::SeriesKind::Geometric
                } else {
                    spreadsheet_series::SeriesKind::Linear
                };
                let edits =
                    spreadsheet_series::build_fill_series(&series.range, start, step, kind)?;
                let count = edits.len();
                let mut snapshot =
                    crate::app::DeferredPanelSnapshot::new(app.undo_stack.len());
                snapshot.capture(&app.document);
                let staged = app.document.stage_spreadsheet_cell_edits(&edits)?;
                app.document = staged;
                snapshot.save_if_semantically_changed(
                    &mut app.document,
                    &mut app.undo_stack,
                    &mut app.redo_stack,
                );
                Ok(count)
            })();
            match outcome {
                Ok(count) => {
                    app.cas_result =
                        format!("Serie: {count} celda(s) rellenadas en {}", series.range.trim());
                }
                Err(error) => {
                    series.error = Some(error);
                }
            }
            ctx.request_repaint();
        }
    });
    if let Some(error) = &series.error {
        ui.label(
            egui::RichText::new(error)
                .color(current_theme(&ctx).danger)
                .size(TYPE_XS),
        );
    }
    ctx.data_mut(|data| data.insert_temp(id, series));
}

/// Sección Datos: hoja vinculada editable (celdas) + tablas en lectura.
/// La hoja edita `Document.spreadsheet` con validación por celda; las tablas
/// (`DataTable`) siguen en lectura con botón de ejemplo real (nunca mudo).
pub(crate) fn draw_spreadsheet_section(ui: &mut egui::Ui, app: &mut GrafitoApp) {
    let ctx = ui.ctx().clone();
    let (_is_dark, _accent, _fill, _sep, txt_col, txt_dim, hdr_col) = panel_theme_local(&ctx);
    ui.label(
        egui::RichText::new("Datos · hoja vinculada (editable)")
            .color(hdr_col)
            .size(TYPE_SM)
            .strong(),
    );
    ui.label(
        egui::RichText::new(
            "Celda con `=A1+B1` o `=x(A)` se recomputa al cambiar la fuente; `(x, y)` con fórmulas crea punto.",
        )
        .color(txt_dim)
        .size(TYPE_XS),
    );
    draw_sheet_editable_grid(ui, app);
    ui.add_space(SPACE_XS);
    draw_sheet_series_row(ui, app);
    ui.add_space(SPACE_XS);

    let tables: Vec<SheetTable> = app
        .document
        .objects_iter_sorted()
        .filter_map(|(id, object)| match object {
            GeoObject::DataTable(table) => Some(SheetTable {
                id: *id,
                label: table.label.clone(),
                x_name: table.x_name.clone(),
                y_name: table.y_name.clone(),
                xs: table.xs.clone(),
                ys: table.ys.clone(),
            }),
            _ => None,
        })
        .collect();

    if tables.is_empty() {
        ui.label(
            egui::RichText::new(
                "No hay tablas. Creá una con DataTable[{1,2,3}, {2,4,6}] en la entrada.",
            )
            .color(txt_dim)
            .size(TYPE_XS),
        );
        if ui
            .small_button("Cargar ejemplo")
            .on_hover_text("Inserta una tabla de ejemplo (x: 0..4, y: x²) al documento")
            .clicked()
        {
            let example = GeoObject::DataTable(
                DataTableObj::new(
                    "x",
                    "y",
                    vec![0.0, 1.0, 2.0, 3.0, 4.0],
                    vec![0.0, 1.0, 4.0, 9.0, 16.0],
                )
                .with_label("ejemplo"),
            );
            match app.document.try_add_object(example) {
                Ok(_) => {
                    app.cas_result = "Tabla «ejemplo» cargada".to_string();
                    app.notify(
                        app.cas_result.clone(),
                        grafito_ui::toast::ToastKind::Success,
                    );
                }
                Err(error) => {
                    app.cas_result = format!("No se pudo cargar el ejemplo: {error}");
                    app.notify(app.cas_result.clone(), grafito_ui::toast::ToastKind::Error);
                }
            }
        }
        return;
    }

    let hidden_tables = tables.len().saturating_sub(MAX_SPREADSHEET_TABLES);
    for table in tables.into_iter().take(MAX_SPREADSHEET_TABLES) {
        let SheetTable {
            id,
            label,
            x_name,
            y_name,
            xs,
            ys,
        } = table;
        let _ = id;
        let title = if label.is_empty() {
            "<sin etiqueta>".to_string()
        } else {
            label
        };
        ui.label(
            egui::RichText::new(format!("{title} · {} filas", xs.len().min(ys.len())))
                .color(txt_col)
                .size(TYPE_XS)
                .strong(),
        );
        let rows = xs.len().min(ys.len());
        let shown = rows.min(MAX_SPREADSHEET_ROWS);
        egui::Grid::new(format!("gc_sheet_{title}"))
            .num_columns(3)
            .striped(true)
            .spacing([8.0, 2.0])
            .show(ui, |ui| {
                ui.label(egui::RichText::new("#").color(txt_dim).size(TYPE_XS));
                ui.label(egui::RichText::new(x_name).color(txt_dim).size(TYPE_XS));
                ui.label(egui::RichText::new(y_name).color(txt_dim).size(TYPE_XS));
                ui.end_row();
                for row in 0..shown {
                    ui.label(
                        egui::RichText::new(format!("{}", row + 1))
                            .color(txt_dim)
                            .size(TYPE_XS),
                    );
                    ui.label(
                        egui::RichText::new(format_statistic(xs[row]))
                            .color(txt_col)
                            .size(TYPE_XS),
                    );
                    ui.label(
                        egui::RichText::new(format_statistic(ys[row]))
                            .color(txt_col)
                            .size(TYPE_XS),
                    );
                    ui.end_row();
                }
            });
        if rows > shown {
            ui.label(
                egui::RichText::new(format!(
                    "…y {} filas más (vista acotada a {MAX_SPREADSHEET_ROWS})",
                    rows - shown
                ))
                .color(txt_dim)
                .size(TYPE_XS),
            );
        }
        ui.add_space(SPACE_XS);
    }
    if hidden_tables > 0 {
        ui.label(
            egui::RichText::new(format!(
                "…y {hidden_tables} tablas más (vista acotada a {MAX_SPREADSHEET_TABLES})"
            ))
            .color(txt_dim)
            .size(TYPE_XS),
        );
    }
    ui.label(
        egui::RichText::new("Tablas en lectura; las celdas se editan arriba.")
            .color(txt_dim)
            .size(TYPE_XS),
    );
}

/// Prompt auto de velocidad: texto libre → `trig_speed` honesto.
/// Llamado desde el panel trig (alcanzable); Aplicar parsea o explica.
pub(crate) fn draw_trig_speed_prompt(ui: &mut egui::Ui, app: &mut GrafitoApp) {
    let ctx = ui.ctx().clone();
    let id = egui::Id::new("gc_trig_prompt_state");
    let mut prompt: TrigPromptState = ctx
        .data_mut(|data| data.get_temp::<TrigPromptState>(id))
        .unwrap_or_default();
    let (_is_dark, _accent, _fill, _sep, _txt_col, txt_dim, _hdr_col) = panel_theme_local(&ctx);

    ui.horizontal(|ui| {
        ui.label(egui::RichText::new("Auto").color(txt_dim).size(TYPE_XS));
        // Ancho acotado inferior: en pantallas mínimas el disponible puede
        // achicarse y egui exige desired_size ≥ 0.
        let field_w = (ui.available_width() - 76.0).max(80.0);
        let resp = ui.add_sized(
            [field_w, TYPE_SM + SPACE_SM],
            egui::TextEdit::singleline(&mut prompt.text)
                .hint_text("1.5 · medio · 2 vueltas en 10 s"),
        );
        let enter_pressed =
            resp.lost_focus() && ui.input(|input| input.key_pressed(egui::Key::Enter));
        resp.on_hover_text("Pedí una velocidad con palabras o números y pulsa Aplicar");
        if ui
            .add_sized(
                [68.0, TYPE_SM + SPACE_SM],
                egui::Button::new(egui::RichText::new("Aplicar").size(TYPE_SM)),
            )
            .on_hover_text("Convierte el pedido en velocidad del slider")
            .clicked()
            || enter_pressed
        {
            match parse_slider_prompt(&prompt.text) {
                Ok(speed) => {
                    app.trig_speed = speed;
                    prompt.error = None;
                    ctx.request_repaint();
                }
                Err(error) => prompt.error = Some(error),
            }
        }
    });
    if let Some(error) = &prompt.error {
        ui.label(
            egui::RichText::new(error)
                .color(current_theme(&ctx).danger)
                .size(TYPE_XS),
        );
    }
    ctx.data_mut(|data| data.insert_temp(id, prompt));
}

#[cfg(test)]
mod gc_piel_tests {
    use super::{
        binomial_cdf, binomial_pmf, binomial_quantile_honest, chi_squared_cdf, chi_squared_pdf,
        chi_squared_quantile_honest, f_distribution_cdf, f_distribution_pdf, f_quantile_honest,
        geometric_cdf, geometric_pmf, geometric_quantile_honest, normal_cdf, normal_pdf,
        normal_quantile_honest, parse_series_scalar, parse_slider_prompt, plot_df_or_fuera_de_cota,
        poisson_cdf, poisson_pmf, poisson_quantile_honest, sheet_col_label, student_t_cdf,
        student_t_pdf, student_t_quantile_honest, uniform_cdf, uniform_pdf,
        uniform_quantile_honest, wc_exact_integral_command_text, wc_riemann_command_text,
        wc_study_command_text, wc_taylor_command_text, wc_taylor_remainder_line, MAX_BINOMIAL_N,
        MAX_GEOMETRIC_K, MAX_PANEL_DF, SHEET_VIEW_COLS, SHEET_VIEW_ROWS,
    };
    use grafito_core::Document;
    use std::collections::HashMap;

    #[test]
    fn normal_standard_values_are_honest() {
        let pdf = normal_pdf(0.0, 0.0, 1.0).expect("normal válida");
        assert!(
            (pdf - 0.398_942_280_401_432_7).abs() < 1e-6,
            "pdf(0) = {pdf}"
        );
        let cdf = normal_cdf(0.0, 0.0, 1.0).expect("normal válida");
        assert!((cdf - 0.5).abs() < 1e-6, "cdf(0) = {cdf}");
        let tail = normal_cdf(1.959_963_984_540_054, 0.0, 1.0).expect("cola 97.5%");
        assert!((tail - 0.975).abs() < 1e-3, "cola = {tail}");
    }

    #[test]
    fn normal_rejects_bad_sigma_and_non_finite() {
        assert!(normal_pdf(0.0, 0.0, 0.0).is_err());
        assert!(normal_pdf(0.0, 0.0, -1.0).is_err());
        assert!(normal_cdf(f64::NAN, 0.0, 1.0).is_err());
        assert!(normal_cdf(0.0, 0.0, f64::INFINITY).is_err());
    }

    #[test]
    fn binomial_fair_coin_matches_textbook() {
        // n=10, p=0.5, k=5 → 252/1024.
        let pmf = binomial_pmf(5, 10, 0.5).expect("binomial válida");
        assert!((pmf - 0.246_093_75).abs() < 1e-9, "pmf = {pmf}");
        let cdf = binomial_cdf(5, 10, 0.5).expect("cdf válida");
        assert!((cdf - 0.623_046_875).abs() < 1e-9, "cdf = {cdf}");
        assert_eq!(binomial_pmf(0, 5, 0.0).expect("p=0"), 1.0);
        assert_eq!(binomial_pmf(1, 5, 0.0).expect("p=0"), 0.0);
        assert_eq!(binomial_pmf(5, 5, 1.0).expect("p=1"), 1.0);
    }

    #[test]
    fn binomial_rejects_out_of_bounds() {
        assert!(binomial_pmf(6, 5, 0.5).is_err());
        assert!(binomial_pmf(1, 5, 1.5).is_err());
        assert!(binomial_pmf(1, MAX_BINOMIAL_N + 1, 0.5).is_err());
        assert!(binomial_cdf(6, 5, 0.5).is_err());
    }

    #[test]
    fn poisson_matches_textbook() {
        // λ=3, k=2 → 9/(2e³) ≈ 0.2240.
        let pmf = poisson_pmf(2, 3.0).expect("poisson válida");
        assert!((pmf - 0.224_041_807_655_387_75).abs() < 1e-9, "pmf = {pmf}");
        let cdf = poisson_cdf(2, 3.0).expect("cdf válida");
        assert!((cdf - 0.423_190_081_126_843_53).abs() < 1e-9, "cdf = {cdf}");
        assert!((poisson_pmf(0, 1.0).expect("k=0") - 0.367_879_441_171_442_33).abs() < 1e-9);
    }

    #[test]
    fn poisson_rejects_bad_lambda_and_caps() {
        assert!(poisson_pmf(1, 0.0).is_err());
        assert!(poisson_pmf(1, -2.0).is_err());
        assert!(poisson_pmf(1, f64::NAN).is_err());
        assert!(poisson_pmf(1, 1e9).is_err());
    }

    #[test]
    fn f3b_area_coincide_con_comando_y_cuantil_consistente() {
        // Normal: cuantil 97.5% ≈ 1.96 y su acumulada vuelve a 0.975.
        let q = normal_quantile_honest(0.975, 0.0, 1.0).expect("cuantil normal");
        assert!((q - 1.959_963_984_540_054).abs() < 1e-3, "q = {q}");
        let area = normal_cdf(q, 0.0, 1.0).expect("área en el cuantil");
        assert!((area - 0.975).abs() < 1e-3, "área = {area}");
        // Binomial: el panel (motor estadístico) coincide con la cuenta
        // combinatoria del comando `Binomial[10, 0.5, 5]` = 252/1024.
        let pmf = binomial_pmf(5, 10, 0.5).expect("pmf panel");
        assert!((pmf - 252.0 / 1024.0).abs() < 1e-9, "pmf = {pmf}");
        let q_bin = binomial_quantile_honest(0.5, 10, 0.5).expect("cuantil binomial");
        let cdf_q = binomial_cdf(q_bin, 10, 0.5).expect("cdf en cuantil");
        assert!(cdf_q >= 0.5, "cdf({q_bin}) = {cdf_q}");
        if q_bin > 0 {
            let cdf_prev = binomial_cdf(q_bin - 1, 10, 0.5).expect("cdf previa");
            assert!(cdf_prev < 0.5, "cdf({}) = {cdf_prev}", q_bin - 1);
        }
        // Poisson: coincide con `Poisson[3, 2]` = 9/(2e³) y el cuantil es
        // el menor k con acumulada ≥ p.
        let pmf_p = poisson_pmf(2, 3.0).expect("pmf poisson");
        assert!(
            (pmf_p - 9.0 / (2.0 * 3.0_f64.exp())).abs() < 1e-9,
            "pmf = {pmf_p}"
        );
        let q_pois = poisson_quantile_honest(0.5, 3.0).expect("cuantil poisson");
        let cdf_q = poisson_cdf(q_pois, 3.0).expect("cdf en cuantil");
        assert!(cdf_q >= 0.5, "cdf({q_pois}) = {cdf_q}");
        if q_pois > 0 {
            let cdf_prev = poisson_cdf(q_pois - 1, 3.0).expect("cdf previa");
            assert!(cdf_prev < 0.5, "cdf({}) = {cdf_prev}", q_pois - 1);
        }
        assert!(normal_quantile_honest(0.0, 0.0, 1.0).is_err());
        assert!(binomial_quantile_honest(1.0, 10, 0.5).is_err());
        assert!(poisson_quantile_honest(0.5, -1.0).is_err());
    }

    #[test]
    fn c2_student_t_matches_textbook() {
        // t(10): pdf(0) ≈ 0.3891, cdf(0) = 0.5, cuantil 97.5% ≈ 2.228.
        let pdf = student_t_pdf(0.0, 10.0).expect("t válida");
        assert!((pdf - 0.389_11).abs() < 1e-4, "pdf = {pdf}");
        let cdf = student_t_cdf(0.0, 10.0).expect("cdf válida");
        assert!((cdf - 0.5).abs() < 1e-9, "cdf = {cdf}");
        let q = student_t_quantile_honest(0.975, 10.0).expect("cuantil t");
        assert!((q - 2.228_14).abs() < 1e-3, "q = {q}");
        let area = student_t_cdf(q, 10.0).expect("área en el cuantil");
        assert!((area - 0.975).abs() < 1e-3, "área = {area}");
    }

    #[test]
    fn c2_chi_squared_matches_textbook() {
        // χ²(5): pdf(5) ≈ 0.1222, cuantil 95% ≈ 11.0705.
        let pdf = chi_squared_pdf(5.0, 5.0).expect("chi² válida");
        assert!((pdf - 0.122_2).abs() < 1e-3, "pdf = {pdf}");
        let q = chi_squared_quantile_honest(0.95, 5.0).expect("cuantil chi²");
        assert!((q - 11.070_5).abs() < 1e-2, "q = {q}");
        let area = chi_squared_cdf(q, 5.0).expect("área en el cuantil");
        assert!((area - 0.95).abs() < 1e-3, "área = {area}");
    }

    #[test]
    fn c2_f_distribution_matches_textbook() {
        // F(5,10): cuantil 95% ≈ 3.3258.
        let q = f_quantile_honest(0.95, 5.0, 10.0).expect("cuantil F");
        assert!((q - 3.325_8).abs() < 1e-2, "q = {q}");
        let area = f_distribution_cdf(q, 5.0, 10.0).expect("área en el cuantil");
        assert!((area - 0.95).abs() < 2e-3, "área = {area}");
        let pdf = f_distribution_pdf(1.0, 5.0, 10.0).expect("pdf F");
        assert!(pdf > 0.0 && pdf < 2.0, "pdf = {pdf}");
    }

    #[test]
    fn c2_continuous_panels_reject_bad_params() {
        assert!(student_t_pdf(0.0, 0.0).is_err());
        assert!(student_t_pdf(0.0, -2.0).is_err());
        assert!(student_t_pdf(f64::NAN, 10.0).is_err());
        assert!(student_t_pdf(0.0, MAX_PANEL_DF + 1.0).is_err());
        assert!(student_t_quantile_honest(0.0, 10.0).is_err());
        assert!(student_t_quantile_honest(1.0, 10.0).is_err());
        assert!(chi_squared_pdf(1.0, 0.0).is_err());
        assert!(chi_squared_quantile_honest(0.95, f64::NAN).is_err());
        assert!(f_distribution_pdf(1.0, 5.0, 0.0).is_err());
        assert!(f_distribution_cdf(1.0, -1.0, 10.0).is_err());
        assert!(f_quantile_honest(1.5, 5.0, 10.0).is_err());
    }

    #[test]
    fn we_geometric_matches_textbook_and_rejects() {
        // p=0.5: P(X=0)=0.5, P(X≤1)=0.75; cuantil 0.5 → 0, cuantil 0.9 → 3.
        let pmf = geometric_pmf(0, 0.5).expect("geométrica válida");
        assert!((pmf - 0.5).abs() < 1e-12, "pmf = {pmf}");
        let cdf = geometric_cdf(1, 0.5).expect("cdf válida");
        assert!((cdf - 0.75).abs() < 1e-12, "cdf = {cdf}");
        assert_eq!(geometric_quantile_honest(0.5, 0.5).expect("cuantil"), 0);
        assert_eq!(geometric_quantile_honest(0.9, 0.5).expect("cuantil"), 3);
        // p=1 degenera honesto: todo el peso en k=0.
        assert_eq!(geometric_pmf(0, 1.0).expect("p=1"), 1.0);
        assert_eq!(geometric_cdf(0, 1.0).expect("p=1"), 1.0);
        assert!(geometric_pmf(0, 0.0).is_err());
        assert!(geometric_pmf(0, 1.5).is_err());
        assert!(geometric_pmf(0, f64::NAN).is_err());
        assert!(geometric_pmf(MAX_GEOMETRIC_K + 1, 0.5).is_err());
        assert!(geometric_quantile_honest(0.0, 0.5).is_err());
        assert!(geometric_quantile_honest(0.5, 0.0).is_err());
    }

    #[test]
    fn we_uniform_matches_textbook_and_rejects() {
        // U(0,10): f(5)=0.1, P(X≤5)=0.5, cuantil 0.25 → 2.5.
        let pdf = uniform_pdf(5.0, 0.0, 10.0).expect("uniforme válida");
        assert!((pdf - 0.1).abs() < 1e-12, "pdf = {pdf}");
        assert_eq!(uniform_pdf(-1.0, 0.0, 10.0).expect("fuera"), 0.0);
        let cdf = uniform_cdf(5.0, 0.0, 10.0).expect("cdf válida");
        assert!((cdf - 0.5).abs() < 1e-12, "cdf = {cdf}");
        let q = uniform_quantile_honest(0.25, 0.0, 10.0).expect("cuantil");
        assert!((q - 2.5).abs() < 1e-12, "q = {q}");
        // Round-trip: la acumulada en el cuantil vuelve a p.
        let back = uniform_cdf(q, 0.0, 10.0).expect("área en el cuantil");
        assert!((back - 0.25).abs() < 1e-12, "área = {back}");
        assert!(uniform_pdf(5.0, 10.0, 0.0).is_err());
        assert!(uniform_pdf(5.0, 3.0, 3.0).is_err());
        assert!(uniform_cdf(f64::NAN, 0.0, 1.0).is_err());
        assert!(uniform_quantile_honest(1.0, 0.0, 1.0).is_err());
        assert!(uniform_quantile_honest(0.5, 2.0, 1.0).is_err());
    }

    #[test]
    fn wa_plot_df_fuera_de_cota_no_dibuja_curva() {
        // W-A red: `df = MAX_PANEL_DF + 1` → mensaje con "cota", no curva.
        // Antes el caller hacía `clamp` a 500 y dibujaba la curva falsa.
        let err = plot_df_or_fuera_de_cota(MAX_PANEL_DF + 1.0, "gl").expect_err("cota");
        assert!(err.contains("cota"), "mensaje honesto: {err}");
        let err1 = plot_df_or_fuera_de_cota(MAX_PANEL_DF + 1.0, "gl1").expect_err("cota");
        assert!(err1.contains("cota"), "mensaje honesto: {err1}");
        assert!(plot_df_or_fuera_de_cota(MAX_PANEL_DF, "gl").is_ok());
        assert!(plot_df_or_fuera_de_cota(0.0, "gl").is_err());
        assert!(plot_df_or_fuera_de_cota(f64::NAN, "gl").is_err());
    }

    #[test]
    fn c2_series_scalar_parses_spanish_decimals() {
        assert_eq!(parse_series_scalar("1,5", "inicio").expect("coma"), 1.5);
        assert_eq!(parse_series_scalar(" -2 ", "paso").expect("espacios"), -2.0);
        assert!(parse_series_scalar("abc", "inicio").is_err());
        assert!(parse_series_scalar("", "paso").is_err());
        assert!(parse_series_scalar("inf", "inicio").is_err());
    }

    #[test]
    fn f3b_sheet_view_respeta_presupuestos_del_core() {
        const { assert!(Document::MAX_SPREADSHEET_ROWS == 400) };
        const { assert!(Document::MAX_SPREADSHEET_COLS == 400) };
        const { assert!(Document::MAX_SPREADSHEET_RECOMPUTE_CELLS == 10_000) };
        const { assert!(SHEET_VIEW_COLS * SHEET_VIEW_ROWS <= Document::MAX_SPREADSHEET_RECOMPUTE_CELLS) };
        assert_eq!(sheet_col_label(0), "A");
        assert_eq!(sheet_col_label(5), "F");
        assert_eq!(sheet_col_label(26), "AA");
    }

    #[test]
    fn slider_prompt_parses_numbers_words_and_turns() {
        assert_eq!(parse_slider_prompt("1.5").expect("número"), 1.5);
        assert_eq!(parse_slider_prompt("medio").expect("palabra"), 0.5);
        assert_eq!(parse_slider_prompt("RÁPIDO").expect("mayúscula"), 2.0);
        let turns = parse_slider_prompt("2 vueltas en 10 s").expect("vueltas");
        assert!(
            (turns - 1.256_637_061_435_917_2).abs() < 1e-9,
            "velocidad = {turns}"
        );
        assert!(parse_slider_prompt("").is_err());
        assert!(parse_slider_prompt("100").is_err());
        assert!(parse_slider_prompt("verde").is_err());
        assert!(parse_slider_prompt("100 vueltas en 1 s").is_err());
    }

    #[test]
    fn wc_command_texts_cablean_a_comandos_reales() {
        // T1: FunctionStudy[f] existe en el registry (verificado en
        // grafito-command); acá solo se pinnea el texto que arma el botón.
        assert_eq!(
            wc_study_command_text("f").as_deref(),
            Some("FunctionStudy[f]")
        );
        assert_eq!(wc_study_command_text("  ").as_deref(), None);
        // T2: RiemannSum con n redondeado + exacta definida.
        assert_eq!(
            wc_riemann_command_text("x^2", "0", "1", 100.0, "trapecio").as_deref(),
            Some("RiemannSum[x^2, x, 0, 1, 100, trapecio]")
        );
        assert_eq!(
            wc_riemann_command_text("x^2", "0", "1", 99.6, "simpson").as_deref(),
            Some("RiemannSum[x^2, x, 0, 1, 100, simpson]")
        );
        assert_eq!(
            wc_riemann_command_text("", "0", "1", 100.0, "trapecio").as_deref(),
            None
        );
        assert_eq!(
            wc_riemann_command_text("x^2", "0", "1", f32::NAN, "trapecio").as_deref(),
            None
        );
        assert_eq!(
            wc_riemann_command_text("x^2", "0", "1", 0.4, "trapecio").as_deref(),
            None
        );
        assert_eq!(
            wc_exact_integral_command_text("x^2", "0", "1").as_deref(),
            Some("Integral[x^2, x, 0, 1]")
        );
        assert_eq!(
            wc_exact_integral_command_text("", "0", "1").as_deref(),
            None
        );
        // T3: Taylor[expr, x, centro, orden] existe; orden del slider 1..=10.
        assert_eq!(
            wc_taylor_command_text("sin(x)", "0", 5, "0.5").as_deref(),
            Some("Taylor[sin(x), x, 0, 5]")
        );
        assert_eq!(
            wc_taylor_command_text("sin(x)", "0", 0, "0.5").as_deref(),
            None
        );
        assert_eq!(
            wc_taylor_command_text("sin(x)", "0", 11, "0.5").as_deref(),
            None
        );
        assert_eq!(
            wc_taylor_command_text("sin(x)", "c", 5, "0.5").as_deref(),
            None
        );
        assert_eq!(wc_taylor_command_text("", "0", 5, "0.5").as_deref(), None);
    }

    #[test]
    fn wc_taylor_remainder_line_muestra_resto_y_falla_honesto() {
        let vars = HashMap::new();
        let line = wc_taylor_remainder_line("sin(x)", "0", 5, "0.5", &vars).expect("resto");
        assert!(line.contains("P5(0.5)"), "rotula orden y punto: {line}");
        assert!(line.contains("resto"), "muestra resto: {line}");
        assert!(line.contains("sig."), "muestra término siguiente: {line}");
        // Expresión inválida o punto no numérico: None honesto.
        assert!(wc_taylor_remainder_line("[[[", "0", 5, "0.5", &vars).is_none());
        assert!(wc_taylor_remainder_line("sin(x)", "0", 5, "c", &vars).is_none());
    }
}
