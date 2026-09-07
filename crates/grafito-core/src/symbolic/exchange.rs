//! Intercambio y paridad honesta (frente F10-C).
//!
//! - Capas (`LayerTable`, S): orden + asignación + visibilidad por capa
//!   sobre [`Document`] sin cambiar su esquema.
//! - Tabla viva en lectura (S): filas y celdas de `DataTableObj`.
//! - SVG real (S) y PNG/PDF honestos: SVG serializa geometría 2D básica;
//!   el PDF es un 1.4 mínimo de una página (interino hasta el vectorial
//!   con `printpdf` del lead en `export.rs`); el PNG devuelve error
//!   explicativo porque exige raster (`image`/`tiny-skia`, fuera del frente).
//! - Gráficos de barras/torta (S): stub honesto que valida y deriva a
//!   `Histogram`/`BoxPlot` existentes.
//! - Gruntz/Risch S/M viven en la puerta [`super::cas_motor`] (motor en
//!   `grafito-geometry::{cas,integral}`); aquí el stub solo documenta el L
//!   restante sin expresión que evaluar.
//! - L puro (marching cubes, Net real, iroh P2P, CRDT): solo diseño + stub
//!   que devuelve `Err` explicativo + test.

use std::collections::BTreeMap;

use thiserror::Error;

use crate::{DataTableObj, Document, GeoObject, ObjectId};

use super::csv::{self, CsvError};

/// Máximo de capas (0..=255, GeoGebra no las numera pero el orden importa).
pub const MAX_LAYERS: u32 = 255;
/// Máximo de objetos serializados por SVG/PDF (igual que el documento).
pub const MAX_EXCHANGE_OBJECTS: usize = 5_000;
/// Máximo de filas de una tabla viva exportada.
pub const MAX_TABLE_ROWS: usize = 20_000;

#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum ExchangeError {
    #[error("intercambio supera el máximo de {MAX_EXCHANGE_OBJECTS} objetos (recibidos {got})")]
    TooManyObjects { got: usize },
    #[error("dato inválido para {feature}: {detail}")]
    InvalidData {
        feature: &'static str,
        detail: String,
    },
    #[error("{feature} no implementado: {hint}")]
    NotImplemented { feature: &'static str, hint: String },
}

impl From<CsvError> for ExchangeError {
    fn from(error: CsvError) -> Self {
        Self::InvalidData {
            feature: "CSV",
            detail: error.to_string(),
        }
    }
}

/// Tabla de capas: asigna cada objeto a una capa 0..=255 con visibilidad
/// conjunta. No muta el esquema de [`Document`]; la visibilidad se aplica
/// sobre el flag `visible` existente de cada objeto.
#[derive(Debug, Clone, Default)]
pub struct LayerTable {
    layers: BTreeMap<ObjectId, u32>,
}

impl LayerTable {
    pub fn new() -> Self {
        Self::default()
    }

    /// Asigna un objeto a una capa (error honesto si excede 255).
    pub fn assign(&mut self, id: ObjectId, layer: u32) -> Result<(), ExchangeError> {
        if layer > MAX_LAYERS {
            return Err(ExchangeError::InvalidData {
                feature: "capas",
                detail: format!("capa {layer} excede el máximo {MAX_LAYERS}"),
            });
        }
        self.layers.insert(id, layer);
        Ok(())
    }

    /// Capa de un objeto (0 por defecto, como GeoGebra).
    pub fn layer_of(&self, id: ObjectId) -> u32 {
        self.layers.get(&id).copied().unwrap_or(0)
    }

    /// Objetos del documento en una capa, en orden estable del documento.
    pub fn objects_on_layer(&self, document: &Document, layer: u32) -> Vec<ObjectId> {
        document
            .objects_iter_sorted()
            .filter(|(id, _)| self.layer_of(**id) == layer)
            .map(|(id, _)| *id)
            .collect()
    }

    /// Capas no vacías `(capa, cantidad)` en orden ascendente, en una sola
    /// pasada sobre el documento. La piel la usa para listar sin escanear
    /// 256 capas (O(n) en vez de O(256·n)).
    pub fn used_layers(&self, document: &Document) -> Vec<(u32, usize)> {
        let mut counts: BTreeMap<u32, usize> = BTreeMap::new();
        for (id, _) in document.objects_iter_sorted() {
            *counts.entry(self.layer_of(*id)).or_default() += 1;
        }
        counts.into_iter().collect()
    }

    /// Visibilidad conjunta de una capa: `true` si todos sus objetos están
    /// visibles (vacía = `true` por vacuidad; la piel solo la llama con
    /// capas de [`Self::used_layers`]).
    pub fn is_layer_visible(&self, document: &Document, layer: u32) -> bool {
        document
            .objects_iter_sorted()
            .filter(|(id, _)| self.layer_of(**id) == layer)
            .all(|(_, object)| object.is_visible())
    }

    /// Descarta asignaciones a objetos que ya no existen (recarga de
    /// documento). La piel la llama al dibujar para acotar memoria.
    pub fn prune_missing(&mut self, document: &Document) {
        self.layers
            .retain(|id, _| document.get_object(*id).is_some());
    }

    /// Aplica visibilidad a toda la capa; devuelve cuántos objetos tocó.
    pub fn set_layer_visible(&self, document: &mut Document, layer: u32, visible: bool) -> usize {
        let mut touched = 0;
        for id in self.objects_on_layer(document, layer) {
            if let Some(object) = document.get_object_mut(id) {
                set_visible(object, visible);
                touched += 1;
            }
        }
        touched
    }
}

fn set_visible(object: &mut GeoObject, visible: bool) {
    match object {
        GeoObject::Point(o) => o.visible = visible,
        GeoObject::Line(o) => o.visible = visible,
        GeoObject::Circle(o) => o.visible = visible,
        GeoObject::Polygon(o) => o.visible = visible,
        GeoObject::Function(o) => o.visible = visible,
        GeoObject::Text(o) => o.visible = visible,
        GeoObject::Ellipse(o) => o.visible = visible,
        GeoObject::Parabola(o) => o.visible = visible,
        GeoObject::Hyperbola(o) => o.visible = visible,
        GeoObject::Arc(o) => o.visible = visible,
        GeoObject::Sector(o) => o.visible = visible,
        GeoObject::Histogram(o) => o.visible = visible,
        GeoObject::ScatterPlot(o) => o.visible = visible,
        GeoObject::BoxPlot(o) => o.visible = visible,
        GeoObject::Sphere3D(o) => o.visible = visible,
        GeoObject::Cube3D(o) => o.visible = visible,
        _ => {}
    }
}

/// Filas vivas `(x, y)` de una tabla (solo pares finitos).
pub fn datatable_rows(table: &DataTableObj) -> Vec<(f64, f64)> {
    table
        .xs
        .iter()
        .zip(table.ys.iter())
        .filter(|(x, y)| x.is_finite() && y.is_finite())
        .map(|(x, y)| (*x, *y))
        .collect()
}

/// Celda viva por fila y columna (0 = x, 1 = y). `None` si no existe.
pub fn datatable_cell(table: &DataTableObj, row: usize, column: usize) -> Option<f64> {
    let value = match column {
        0 => table.xs.get(row).copied(),
        1 => table.ys.get(row).copied(),
        _ => None,
    }?;
    value.is_finite().then_some(value)
}

/// Exporta una tabla viva a CSV RFC 4180 con cabeza `x_name,y_name`.
pub fn datatable_to_csv(table: &DataTableObj) -> Result<String, ExchangeError> {
    let rows = datatable_rows(table);
    if rows.len() > MAX_TABLE_ROWS {
        return Err(ExchangeError::TooManyObjects { got: rows.len() });
    }
    let mut string_rows = Vec::with_capacity(rows.len() + 1);
    string_rows.push(vec![table.x_name.clone(), table.y_name.clone()]);
    for (x, y) in &rows {
        string_rows.push(vec![x.to_string(), y.to_string()]);
    }
    Ok(csv::to_csv(&string_rows)?)
}

fn escape_xml(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    for ch in text.chars() {
        match ch {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            '"' => out.push_str("&quot;"),
            _ => out.push(ch),
        }
    }
    out
}

/// Serializa puntos, círculos y polígonos visibles a SVG real con
/// `viewBox="-10 -10 20 20"`. El resto de objetos se cuenta en un
/// comentario honesto en vez de inventar geometría.
pub fn document_to_svg(
    document: &Document,
    width: u32,
    height: u32,
) -> Result<String, ExchangeError> {
    let count = document.objects_iter_sorted().count();
    if count > MAX_EXCHANGE_OBJECTS {
        return Err(ExchangeError::TooManyObjects { got: count });
    }
    let width = width.clamp(64, 4096);
    let height = height.clamp(64, 4096);
    let mut out = format!(
        "<svg xmlns=\"http://www.w3.org/2000/svg\" width=\"{width}\" height=\"{height}\" viewBox=\"-10 -10 20 20\">"
    );
    let mut skipped = 0_usize;
    for (_, object) in document.objects_iter_sorted() {
        match object {
            GeoObject::Point(o) if o.visible => {
                out.push_str(&format!(
                    "<circle cx=\"{}\" cy=\"{}\" r=\"0.08\"/>",
                    o.position.x, o.position.y
                ));
            }
            GeoObject::Circle(o) if o.visible => {
                out.push_str(&format!(
                    "<circle cx=\"{}\" cy=\"{}\" r=\"{}\"/>",
                    o.center.x, o.center.y, o.radius
                ));
            }
            GeoObject::Polygon(o) if o.visible => {
                let points = o
                    .vertices
                    .iter()
                    .map(|v| format!("{},{}", v.x, v.y))
                    .collect::<Vec<_>>()
                    .join(" ");
                out.push_str(&format!(
                    "<polygon points=\"{}\" fill=\"none\" stroke=\"black\"/>",
                    escape_xml(&points)
                ));
            }
            GeoObject::Text(o) if o.visible => {
                out.push_str(&format!("<text>{}</text>", escape_xml(&o.content)));
            }
            _ => {
                skipped += 1;
            }
        }
    }
    if skipped > 0 {
        out.push_str(&format!(
            "<!-- {skipped} objetos no 2D básicos omitidos -->"
        ));
    }
    out.push_str("</svg>");
    Ok(out)
}

/// Contenido SVG listo para el portapapeles (mismo que el export).
pub fn clipboard_svg(document: &Document) -> Result<String, ExchangeError> {
    document_to_svg(document, 800, 600)
}

/// Stub honesto de PNG para portapapeles: exige raster fuera del frente.
pub fn clipboard_png_stub() -> Result<Vec<u8>, ExchangeError> {
    Err(ExchangeError::NotImplemented {
        feature: "portapapeles PNG",
        hint: "requiere raster con image/tiny-skia y wiring en grafito-app (fuera del frente F10-C); usa SVG mientras tanto".to_string(),
    })
}

fn escape_pdf_text(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    for ch in text.chars() {
        match ch {
            '(' | ')' | '\\' => {
                out.push('\\');
                out.push(ch);
            }
            _ if ch.is_control() => {}
            _ => out.push(ch),
        }
    }
    out
}

/// PDF 1.4 mínimo de una página (Helvetica) con el conteo de objetos y
/// hasta 40 etiquetas. Interino hasta el vectorial con `printpdf` del lead;
/// abre en cualquier visor y nunca inventa geometría.
pub fn document_to_pdf(document: &Document) -> Result<Vec<u8>, ExchangeError> {
    let objects: Vec<String> = document
        .objects_iter_sorted()
        .map(|(_, object)| object.name().to_string())
        .collect();
    if objects.len() > MAX_EXCHANGE_OBJECTS {
        return Err(ExchangeError::TooManyObjects { got: objects.len() });
    }
    let mut lines = vec![format!("Grafito - {} objetos", objects.len())];
    for (index, kind) in objects.iter().take(40).enumerate() {
        lines.push(format!("{}. {}", index + 1, kind));
    }
    if objects.len() > 40 {
        lines.push(format!("... y {} mas", objects.len() - 40));
    }
    let mut content = String::from("BT /F1 12 Tf 50 780 Td 14 TL ");
    for line in &lines {
        content.push_str(&format!("({}) Tj T* ", escape_pdf_text(line)));
    }
    content.push_str("ET");
    let objects_pdf = [
        "<< /Type /Catalog /Pages 2 0 R >>".to_string(),
        "<< /Type /Pages /Kids [3 0 R] /Count 1 >>".to_string(),
        "<< /Type /Page /Parent 2 0 R /MediaBox [0 0 595 842] /Resources << /Font << /F1 4 0 R >> >> /Contents 5 0 R >>".to_string(),
        "<< /Type /Font /Subtype /Type1 /BaseFont /Helvetica >>".to_string(),
        format!("<< /Length {} >>\nstream\n{content}\nendstream", content.len()),
    ];
    let mut pdf = String::from("%PDF-1.4\n");
    let mut offsets = Vec::with_capacity(objects_pdf.len());
    for (index, body) in objects_pdf.iter().enumerate() {
        offsets.push(pdf.len());
        pdf.push_str(&format!("{} 0 obj\n{body}\nendobj\n", index + 1));
    }
    let xref_at = pdf.len();
    pdf.push_str(&format!("xref\n0 {}\n", objects_pdf.len() + 1));
    pdf.push_str("0000000000 65535 f \n");
    for offset in &offsets {
        pdf.push_str(&format!("{offset:010} 00000 n \n"));
    }
    pdf.push_str(&format!(
        "trailer\n<< /Size {} /Root 1 0 R >>\nstartxref\n{xref_at}\n%%EOF",
        objects_pdf.len() + 1
    ));
    Ok(pdf.into_bytes())
}

fn check_chart_data(feature: &'static str, data: &[f64]) -> Result<(), ExchangeError> {
    if data.is_empty() {
        return Err(ExchangeError::InvalidData {
            feature,
            detail: "sin datos".to_string(),
        });
    }
    if data.len() > MAX_TABLE_ROWS {
        return Err(ExchangeError::TooManyObjects { got: data.len() });
    }
    if data.iter().any(|v| !v.is_finite()) {
        return Err(ExchangeError::InvalidData {
            feature,
            detail: "los valores deben ser finitos".to_string(),
        });
    }
    Ok(())
}

/// Stub honesto de gráfico de barras: valida y deriva al existente.
pub fn bar_chart_stub(data: &[f64]) -> Result<String, ExchangeError> {
    check_chart_data("BarChart", data)?;
    Err(ExchangeError::NotImplemented {
        feature: "BarChart",
        hint: "usa Histogram[{datos}, bins] o BoxPlot[{datos}] mientras se implementa el render de barras por categoría".to_string(),
    })
}

/// Stub honesto de gráfico de torta: valida y deriva al existente.
pub fn pie_chart_stub(data: &[f64]) -> Result<String, ExchangeError> {
    check_chart_data("PieChart", data)?;
    if data.iter().any(|v| *v < 0.0) {
        return Err(ExchangeError::InvalidData {
            feature: "PieChart",
            detail: "los valores deben ser no negativos".to_string(),
        });
    }
    Err(ExchangeError::NotImplemented {
        feature: "PieChart",
        hint: "usa Histogram[{datos}, bins] mientras se implementa el render de sectores proporcionales".to_string(),
    })
}

/// Diseño + stub de los L de Tasks.md F10.W5: siempre `Err` explicativo.
///
/// `Gruntz`/`Risch` ya tienen motor S/M real (puerta [`super::cas_motor`]
/// sobre `grafito-geometry::{cas,integral}`); aquí el stub persiste porque
/// no recibe expresión que evaluar, y su hint deriva a la puerta con
/// cómputo. El resto (marching cubes, Net, P2P, CRDT) es L puro.
pub fn l_stub(feature: &'static str) -> Result<String, ExchangeError> {
    let hint = match feature {
        "Gruntz" => {
            "límites 0/0, ∞/∞ y jerarquía exp/log/potencia ya implementados en grafito-geometry::cas (gruntz_limit/gruntz_limit_infinite) con puerta cas_motor::cas_limit_gruntz; este stub no recibe expresión"
        }
        "Risch" => "Risch-Norman (polinomios/exponenciales/logaritmos) ya implementado en grafito-geometry::integral con puerta cas_motor::cas_integrate_risch; este stub no recibe integrando (racionales → symbolic::integrate, resto L en F10.W5)",
        "MarchingCubes" => "superficie F(x,y,z)=0 por marching cubes (diseño F10.W5)",
        "Net" => "desarrollo 2D de poliedros por despliegue de caras (diseño F10.W5)",
        "IrohP2P" => "transporte P2P con iroh (diseño F10.W5, hoy Loopback en grafito-classroom)",
        "Crdt" => "fusión pizarra UUID+LWW (diseño F10.W5)",
        _ => "diseño pendiente en Tasks.md F10.W5",
    };
    Err(ExchangeError::NotImplemented {
        feature,
        hint: hint.to_string(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{GeoObject, PointObj};
    use grafito_geometry::Point2;

    fn point_fixture(label: &str) -> GeoObject {
        GeoObject::Point(PointObj::new(Point2::new(1.0, 2.0)).with_label(label))
    }

    #[test]
    fn layers_default_assign_and_toggle() {
        let mut document = Document::new();
        let a = document
            .try_add_object(point_fixture("A"))
            .expect("punto fixture");
        let b = document
            .try_add_object(point_fixture("B"))
            .expect("punto fixture");
        let mut layers = LayerTable::new();
        assert_eq!(layers.layer_of(a), 0);
        layers.assign(b, 2).expect("capa fixture");
        assert_eq!(layers.objects_on_layer(&document, 2), vec![b]);
        let touched = layers.set_layer_visible(&mut document, 2, false);
        assert_eq!(touched, 1);
        assert!(document.get_object(a).is_some());
        assert!(layers.assign(a, MAX_LAYERS + 1).is_err());
    }

    #[test]
    fn used_layers_lists_counts_sorted_in_one_pass() {
        let mut document = Document::new();
        let a = document
            .try_add_object(point_fixture("A"))
            .expect("punto fixture");
        let b = document
            .try_add_object(point_fixture("B"))
            .expect("punto fixture");
        let c = document
            .try_add_object(point_fixture("C"))
            .expect("punto fixture");
        let mut layers = LayerTable::new();
        layers.assign(c, 7).expect("capa fixture");
        layers.assign(b, 2).expect("capa fixture");
        // `a` queda en la capa 0 por defecto.
        assert_eq!(layers.used_layers(&document), vec![(0, 1), (2, 1), (7, 1)]);
        assert_eq!(layers.layer_of(a), 0);
    }

    #[test]
    fn layer_visibility_is_conjunctive_and_prune_drops_missing() {
        let mut document = Document::new();
        let a = document
            .try_add_object(point_fixture("A"))
            .expect("punto fixture");
        let mut layers = LayerTable::new();
        // Capa vacía: vacuamente visible; la piel no la lista.
        assert!(layers.is_layer_visible(&document, 9));
        assert!(layers.is_layer_visible(&document, 0));
        layers.set_layer_visible(&mut document, 0, false);
        assert!(!layers.is_layer_visible(&document, 0));
        layers.set_layer_visible(&mut document, 0, true);
        assert!(layers.is_layer_visible(&document, 0));
        // Tras vaciar el documento por reconstrucción, prune limpia.
        let fresh = Document::new();
        layers.assign(a, 3).expect("capa fixture");
        layers.prune_missing(&fresh);
        assert_eq!(layers.used_layers(&fresh), vec![]);
        assert_eq!(layers.layer_of(a), 0);
    }

    #[test]
    fn datatable_live_read_and_csv() {
        let table = DataTableObj::new("x", "y", vec![1.0, 2.0], vec![3.0, 4.0]);
        assert_eq!(datatable_rows(&table), vec![(1.0, 3.0), (2.0, 4.0)]);
        assert_eq!(datatable_cell(&table, 0, 0), Some(1.0));
        assert_eq!(datatable_cell(&table, 1, 1), Some(4.0));
        assert_eq!(datatable_cell(&table, 5, 0), None);
        assert_eq!(datatable_cell(&table, 0, 2), None);
        let csv_text = datatable_to_csv(&table).expect("csv fixture");
        assert!(csv_text.starts_with("x,y"));
    }

    #[test]
    fn svg_is_real_and_escapes_labels() {
        let mut document = Document::new();
        document
            .try_add_object(point_fixture("A"))
            .expect("punto fixture");
        let svg = clipboard_svg(&document).expect("svg fixture");
        assert!(svg.starts_with("<svg"));
        assert!(svg.contains("<circle"));
        assert!(svg.ends_with("</svg>"));
        assert!(escape_xml("a&b<c>").contains("&amp;"));
    }

    #[test]
    fn pdf_minimal_opens_with_header_and_eof() {
        let mut document = Document::new();
        document
            .try_add_object(point_fixture("A"))
            .expect("punto fixture");
        let pdf = document_to_pdf(&document).expect("pdf fixture");
        assert!(pdf.starts_with(b"%PDF-1.4"));
        assert!(pdf.windows(5).any(|w| w == b"%%EOF"));
        assert!(!pdf.is_empty());
    }

    #[test]
    fn png_clipboard_is_honest_stub() {
        let err = clipboard_png_stub().expect_err("PNG pendiente");
        assert!(err.to_string().contains("portapapeles PNG"));
    }

    #[test]
    fn charts_validate_then_stub() {
        assert!(bar_chart_stub(&[])
            .expect_err("sin datos")
            .to_string()
            .contains("sin datos"));
        assert!(bar_chart_stub(&[f64::NAN])
            .expect_err("no finitos")
            .to_string()
            .contains("finitos"));
        assert!(bar_chart_stub(&[1.0, 2.0])
            .expect_err("BarChart pendiente")
            .to_string()
            .contains("Histogram"));
        assert!(pie_chart_stub(&[-1.0])
            .expect_err("negativos")
            .to_string()
            .contains("no negativos"));
        assert!(pie_chart_stub(&[1.0, 2.0])
            .expect_err("PieChart pendiente")
            .to_string()
            .contains("Histogram"));
    }

    #[test]
    fn l_stubs_are_honest() {
        for feature in ["Gruntz", "Risch", "MarchingCubes", "Net", "IrohP2P", "Crdt"] {
            let err = l_stub(feature).expect_err("L siempre falla honesto");
            assert!(err.to_string().contains(feature));
        }
        // Gruntz/Risch con motor real: el hint deriva a la puerta computable.
        for feature in ["Gruntz", "Risch"] {
            let err = l_stub(feature).expect_err("stub sin expresión");
            assert!(
                err.to_string().contains("cas_motor"),
                "hint debe derivar a cas_motor: {err}"
            );
        }
    }
}
