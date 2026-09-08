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
//! - L puro (iroh P2P, CRDT): solo diseño + stub
//!   que devuelve `Err` explicativo + test.

use std::collections::BTreeMap;

use thiserror::Error;

use crate::{DataTableObj, Document, GeoObject, ObjectId};

use super::csv::{self, CsvError};
use super::solids;

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
        GeoObject::BarChart(o) => o.visible = visible,
        GeoObject::PieChart(o) => o.visible = visible,
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

/// PDF 1.4 mínimo multipágina (Helvetica) con el conteo de objetos y
/// 40 etiquetas por página. P1a-4: pagina de verdad en vez de truncar a 1
/// página en silencio; abre en cualquier visor y nunca inventa geometría.
/// Interino hasta el vectorial con `printpdf` del lead.
pub fn document_to_pdf(document: &Document) -> Result<Vec<u8>, ExchangeError> {
    const ROWS_PER_PAGE: usize = 40;
    let objects: Vec<String> = document
        .objects_iter_sorted()
        .map(|(_, object)| object.name().to_string())
        .collect();
    if objects.len() > MAX_EXCHANGE_OBJECTS {
        return Err(ExchangeError::TooManyObjects { got: objects.len() });
    }
    // Paginación simple: 40 filas por página, numeración global continua.
    let page_count = objects.len().max(1).div_ceil(ROWS_PER_PAGE).max(1);
    let mut contents: Vec<String> = Vec::with_capacity(page_count);
    // Construir contenidos por página (caso 0 objetos = 1 página solo conteo).
    if objects.is_empty() {
        let content = format!(
            "BT /F1 12 Tf 50 780 Td 14 TL ({}) Tj T* ET",
            escape_pdf_text("Grafito - 0 objetos (página 1 de 1)")
        );
        contents.push(content);
    } else {
        for (page_idx, chunk) in objects.chunks(ROWS_PER_PAGE).enumerate() {
            let mut lines = vec![format!(
                "Grafito - {} objetos (página {} de {})",
                objects.len(),
                page_idx + 1,
                page_count
            )];
            let base = page_idx * ROWS_PER_PAGE;
            for (offset, kind) in chunk.iter().enumerate() {
                lines.push(format!("{}. {}", base + offset + 1, kind));
            }
            let mut content = String::from("BT /F1 12 Tf 50 780 Td 14 TL ");
            for line in &lines {
                content.push_str(&format!("({}) Tj T* ", escape_pdf_text(line)));
            }
            content.push_str("ET");
            contents.push(content);
        }
    }
    debug_assert_eq!(contents.len(), page_count);
    // Objetos PDF: 1=Catalog, 2=Pages, luego (Page, Contents) por página, N=Font.
    let mut kids = String::new();
    for idx in 0..page_count {
        let page_obj = 3 + idx * 2;
        if idx > 0 {
            kids.push(' ');
        }
        kids.push_str(&format!("{page_obj} 0 R"));
    }
    let font_obj = 3 + page_count * 2;
    let mut objects_pdf: Vec<String> = Vec::with_capacity(font_obj);
    objects_pdf.push("<< /Type /Catalog /Pages 2 0 R >>".to_string());
    objects_pdf.push(format!(
        "<< /Type /Pages /Kids [{kids}] /Count {page_count} >>"
    ));
    for (idx, content) in contents.iter().enumerate() {
        let page_obj = 3 + idx * 2;
        let content_obj = 4 + idx * 2;
        objects_pdf.push(format!(
            "<< /Type /Page /Parent 2 0 R /MediaBox [0 0 595 842] /Resources << /Font << /F1 {font_obj} 0 R >> >> /Contents {content_obj} 0 R >>"
        ));
        debug_assert_eq!(objects_pdf.len(), page_obj);
        objects_pdf.push(format!(
            "<< /Length {} >>\nstream\n{content}\nendstream",
            content.len()
        ));
        debug_assert_eq!(objects_pdf.len(), content_obj);
    }
    objects_pdf.push("<< /Type /Font /Subtype /Type1 /BaseFont /Helvetica >>".to_string());
    debug_assert_eq!(objects_pdf.len(), font_obj);
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

/// Barra propia mínima (frente W4, sin registry): valida como el stub y
/// devuelve una barra por dato con fracciones listas para renderizar.
///
/// - `fraction_of_max`: `value / max|v|` (rango `-1..=1`; `1` es la mayor).
/// - `fraction_of_total`: `value / suma` (`0` si la suma es `0`).
///
/// Puro, sin I/O. El comando `BarChart[...]` (registry, fuera de este frente)
/// consume estas fracciones sin revalidar de más.
#[derive(Debug, Clone, PartialEq)]
pub struct BarSegment {
    /// Índice del dato en el slice de entrada.
    pub index: usize,
    /// Valor original (finito, tal cual entró).
    pub value: f64,
    /// Proporción contra el mayor `|v|` (`-1..=1`).
    pub fraction_of_max: f64,
    /// Proporción contra la suma (`0` si la suma es `0`).
    pub fraction_of_total: f64,
}

/// Calcula las barras propias de `BarChart` (valida, nunca inventa).
pub fn bar_chart_bars(data: &[f64]) -> Result<Vec<BarSegment>, ExchangeError> {
    check_chart_data("BarChart", data)?;
    let mut max_abs = 0.0_f64;
    for value in data {
        let magnitude = value.abs();
        if magnitude > max_abs {
            max_abs = magnitude;
        }
    }
    if !max_abs.is_finite() || max_abs <= 0.0 {
        return Err(ExchangeError::InvalidData {
            feature: "BarChart",
            detail: "sin escala: todos los valores son cero".to_string(),
        });
    }
    let mut total = 0.0_f64;
    for value in data {
        total += *value;
    }
    let total_is_usable = total.is_finite() && total != 0.0;
    let mut bars = Vec::with_capacity(data.len());
    for (index, value) in data.iter().enumerate() {
        let fraction_of_max = *value / max_abs;
        let fraction_of_total = if total_is_usable { *value / total } else { 0.0 };
        if !fraction_of_max.is_finite() || !fraction_of_total.is_finite() {
            return Err(ExchangeError::InvalidData {
                feature: "BarChart",
                detail: "las fracciones deben ser finitas".to_string(),
            });
        }
        bars.push(BarSegment {
            index,
            value: *value,
            fraction_of_max,
            fraction_of_total,
        });
    }
    Ok(bars)
}

/// Sector propio mínimo (frente W4, sin registry): valida (finitos +
/// no negativos + total `> 0` finito) y devuelve un sector por dato con
/// ángulos acumulados en radianes (`0..=TAU`, como GeoGebra).
#[derive(Debug, Clone, PartialEq)]
pub struct PieSlice {
    /// Índice del dato en el slice de entrada.
    pub index: usize,
    /// Valor original (finito, no negativo).
    pub value: f64,
    /// Proporción contra el total (`0..=1`).
    pub fraction: f64,
    /// Ángulo inicial acumulado (radianes, `0..=TAU`).
    pub start_angle: f64,
    /// Barrido del sector (radianes, `>= 0`, suma `TAU`).
    pub sweep_angle: f64,
}

/// Calcula los sectores propios de `PieChart` (valida, nunca inventa).
pub fn pie_chart_slices(data: &[f64]) -> Result<Vec<PieSlice>, ExchangeError> {
    check_chart_data("PieChart", data)?;
    if data.iter().any(|v| *v < 0.0) {
        return Err(ExchangeError::InvalidData {
            feature: "PieChart",
            detail: "los valores deben ser no negativos".to_string(),
        });
    }
    let mut total = 0.0_f64;
    for value in data {
        total += *value;
    }
    if !total.is_finite() || total <= 0.0 {
        return Err(ExchangeError::InvalidData {
            feature: "PieChart",
            detail: "sin total positivo que repartir".to_string(),
        });
    }
    let tau = std::f64::consts::TAU;
    let mut slices = Vec::with_capacity(data.len());
    let mut start_angle = 0.0_f64;
    for (index, value) in data.iter().enumerate() {
        let fraction = *value / total;
        let sweep_angle = fraction * tau;
        if !fraction.is_finite() || !sweep_angle.is_finite() {
            return Err(ExchangeError::InvalidData {
                feature: "PieChart",
                detail: "las fracciones deben ser finitas".to_string(),
            });
        }
        slices.push(PieSlice {
            index,
            value: *value,
            fraction,
            start_angle,
            sweep_angle,
        });
        start_angle += sweep_angle;
    }
    Ok(slices)
}

/// Medida exacta de un sólido 3D (frente W4, sin registry): expone el motor
/// [`solids`](super::solids) sin tocar comandos ni paneles.
///
/// `Ok` trae `(volumen, área)` finitos con estado `"exacto"`; si el objeto no
/// tiene forma cerrada (cuádrica, superficies) devuelve `Err::NotImplemented`
/// con el estado honesto de [`solids::solid_measure_status`] para que la piel
/// lo muestre tal cual en vez de inventar un número.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct SolidMeasure {
    /// Volumen exacto del sólido.
    pub volume: f64,
    /// Área total exacta del sólido.
    pub area: f64,
    /// Estado del motor (`"exacto"` en `Ok`).
    pub status: &'static str,
}

/// Resume volumen/área exactos o falla honesto si no hay forma cerrada.
pub fn solid_measure_summary(object: &GeoObject) -> Result<SolidMeasure, ExchangeError> {
    let status = solids::solid_measure_status(object);
    let (Some(volume), Some(area)) = (solids::solid_volume(object), solids::solid_area(object))
    else {
        return Err(ExchangeError::NotImplemented {
            feature: "Volumen/Área 3D",
            hint: status.to_string(),
        });
    };
    if !volume.is_finite() || !area.is_finite() {
        return Err(ExchangeError::InvalidData {
            feature: "Volumen/Área 3D",
            detail: "la medida debe ser finita".to_string(),
        });
    }
    Ok(SolidMeasure {
        volume,
        area,
        status,
    })
}

/// Diseño + stub de los L de Tasks.md F10.W5: siempre `Err` explicativo.
///
/// `Gruntz`/`Risch` ya tienen motor S/M real (puerta [`super::cas_motor`]
/// sobre `grafito-geometry::{cas,integral}`); aquí el stub persiste porque
/// no recibe expresión que evaluar, y su hint deriva a la puerta con
/// cómputo. `MarchingCubes`/`Net` ya tienen motor real (A1+A3+A4):
/// superficie por `grafito-geometry::polytopes::implicit_surface_mesh` con
/// puerta `ImplicitSurface3DObj::compute_mesh` + comando `ImplicitSurface`,
/// y desarrollo por `PolyhedronNet::unfold` + comando `Net`; aquí el stub
/// persiste porque no recibe campo ni objeto que desplegar, y su hint deriva
/// al motor. El resto (P2P, CRDT) es L puro.
pub fn l_stub(feature: &'static str) -> Result<String, ExchangeError> {
    let hint = match feature {
        "Gruntz" => {
            "límites 0/0, ∞/∞ y jerarquía exp/log/potencia ya implementados en grafito-geometry::cas (gruntz_limit/gruntz_limit_infinite) con puerta cas_motor::cas_limit_gruntz; este stub no recibe expresión"
        }
        "Risch" => "Risch-Norman (polinomios/exponenciales/logaritmos) ya implementado en grafito-geometry::integral con puerta cas_motor::cas_integrate_risch; este stub no recibe integrando (racionales → symbolic::integrate, resto L en F10.W5)",
        "MarchingCubes" => "superficie F(x,y,z)=0 por marching-tetra ya implementada en grafito-geometry::polytopes::implicit_surface_mesh con puerta ImplicitSurface3DObj::compute_mesh y comando ImplicitSurface; este stub no recibe campo que isosuperficiar",
        "Net" => "desarrollo 2D de poliedros por PolyhedronNet::unfold ya implementado con comando Net (Cube/Tetrahedron/Pyramid/Prism → polígonos 2D); este stub no recibe objeto que desplegar",
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
        // 1 objeto = 1 página.
        let text = String::from_utf8_lossy(&pdf);
        assert!(text.contains("/Count 1"), "1 página esperada: {text}");
    }

    #[test]
    fn pdf_pagina_de_verdad_con_41_objetos() {
        // P1a-4: 41 objetos ya no se truncan a 1 página; van a 2 páginas reales.
        let mut document = Document::new();
        for idx in 0..41 {
            document
                .try_add_object(point_fixture(&format!("P{idx}")))
                .expect("punto fixture");
        }
        let pdf = document_to_pdf(&document).expect("pdf multipágina");
        let text = String::from_utf8_lossy(&pdf);
        assert!(text.contains("/Count 2"), "2 páginas esperadas: {text}");
        assert!(text.contains("página 1 de 2"), "numeración p1: {text}");
        assert!(text.contains("página 2 de 2"), "numeración p2: {text}");
        assert!(
            text.contains("41. "),
            "la fila 41 debe existir (sin truncar)"
        );
        assert!(!text.contains("... y"), "ya no se trunca con '... y N mas'");
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
    fn bar_chart_bars_are_proportional_and_honest() {
        let bars = bar_chart_bars(&[1.0, 2.0, 3.0]).expect("barras fixture");
        assert_eq!(bars.len(), 3);
        assert_eq!(bars[2].index, 2);
        assert!((bars[2].fraction_of_max - 1.0).abs() < 1e-12);
        assert!((bars[0].fraction_of_max - 1.0 / 3.0).abs() < 1e-12);
        let total: f64 = bars.iter().map(|bar| bar.fraction_of_total).sum();
        assert!((total - 1.0).abs() < 1e-12);
        // Sin escala (todo cero) falla honesto, no divide por cero.
        assert!(bar_chart_bars(&[0.0, 0.0])
            .expect_err("sin escala")
            .to_string()
            .contains("sin escala"));
        assert!(bar_chart_bars(&[]).is_err());
        assert!(bar_chart_bars(&[f64::INFINITY]).is_err());
    }

    #[test]
    fn pie_chart_slices_cover_tau_and_reject_empty_total() {
        let slices = pie_chart_slices(&[1.0, 1.0, 2.0]).expect("torta fixture");
        assert_eq!(slices.len(), 3);
        assert!((slices[0].fraction - 0.25).abs() < 1e-12);
        assert!((slices[2].fraction - 0.5).abs() < 1e-12);
        let swept: f64 = slices.iter().map(|slice| slice.sweep_angle).sum();
        assert!((swept - std::f64::consts::TAU).abs() < 1e-9);
        assert_eq!(slices[0].start_angle, 0.0);
        assert!(slices[1].start_angle > 0.0);
        // Total cero o negativos: honesto, sin NaN.
        assert!(pie_chart_slices(&[0.0, 0.0])
            .expect_err("sin total")
            .to_string()
            .contains("sin total"));
        assert!(pie_chart_slices(&[-1.0]).is_err());
        assert!(pie_chart_slices(&[]).is_err());
    }

    #[test]
    fn solid_measure_summary_is_exact_or_honest() {
        use crate::{Quadric3DObj, Sphere3DObj};
        use grafito_geometry::Point3D;
        let sphere = GeoObject::Sphere3D(Sphere3DObj::new(Point3D::new(0.0, 0.0, 0.0), 1.0));
        let measure = solid_measure_summary(&sphere).expect("esfera mide exacto");
        assert!((measure.volume - 4.188_790_204_786_390_5).abs() < 1e-9);
        assert!((measure.area - 12.566_370_614_359_172).abs() < 1e-9);
        assert_eq!(measure.status, "exacto");
        let quadric = GeoObject::Quadric3D(Quadric3DObj::from_coeffs([
            1.0, 1.0, 1.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, -1.0,
        ]));
        let err = solid_measure_summary(&quadric).expect_err("cuádrica sin forma cerrada");
        assert!(err.to_string().contains("Volumen/Área 3D"));
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
        // A4: MarchingCubes/Net con motor real — el hint deriva al motor,
        // ya no miente con "diseño F10.W5".
        let err = l_stub("MarchingCubes").expect_err("stub sin campo");
        assert!(
            err.to_string().contains("implicit_surface_mesh"),
            "hint debe derivar a implicit_surface_mesh: {err}"
        );
        let err = l_stub("Net").expect_err("stub sin objeto");
        assert!(
            err.to_string().contains("PolyhedronNet::unfold"),
            "hint debe derivar a PolyhedronNet::unfold: {err}"
        );
    }
}
