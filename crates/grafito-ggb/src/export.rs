//! Exportador `.ggb` (GeoGebra) desde objetos Grafito.
//!
//! Espejo del importador (`map.rs`): emite exactamente las etiquetas que el
//! importador entiende (`point`/`numeric`/`slider`/`circle` como elementos y
//! `Segment`/`Vector`/`Circle`/`Polygon` como comandos con `input a0..`).
//! Regla de oro: **todo lo que exporta debe re-importar** (test de
//! roundtrip `export_roundtrip_labels_survive`). Lo no soportado se omite
//! con motivo en [`ExportReport`], jamás se inventa.
//!
//! Precisión: números con 6 decimales recortados (igual que `fmt_num` del
//! importador); ida y vuelta dentro de 1e-6.

use crate::error::GgbError;
use crate::GGB_XML_NAME;
use crate::{MAX_ELEMS, MAX_EXPR_CHARS};
use quick_xml::events::{BytesDecl, BytesEnd, BytesStart, Event};
use quick_xml::Writer;
use std::collections::BTreeSet;
use std::io::{Cursor, Write};
use zip::write::SimpleFileOptions;

/// Etiqueta máxima (las de GeoGebra son cortas; más allá se omite honesto).
pub const MAX_EXPORT_LABEL_CHARS: usize = 128;

/// Ítem exportable a `.ggb`.
#[derive(Debug, Clone)]
pub enum GgbExportItem {
    /// `<element type="point">` + coords (pobla el mapa de puntos).
    Point { label: String, x: f64, y: f64 },
    /// `<command name="Segment">` sobre etiquetas de puntos ya emitidos.
    Segment {
        label: String,
        from: String,
        to: String,
    },
    /// `<command name="Vector">` sobre etiquetas de puntos ya emitidos.
    Vector {
        label: String,
        from: String,
        to: String,
    },
    /// `<element type="circle">` + coords + value (directo, sin comando).
    Circle {
        label: String,
        cx: f64,
        cy: f64,
        r: f64,
    },
    /// `<command name="Polygon">` sobre etiquetas de puntos ya emitidos.
    Polygon {
        label: String,
        vertices: Vec<String>,
    },
    /// `<element type="numeric">` + value + slider (los numéricos sin rango
    /// no tienen mapeo de importación: no existe esta variante a propósito).
    Slider {
        label: String,
        value: f64,
        min: f64,
        max: f64,
    },
}

/// Resultado de una exportación.
#[derive(Debug, Clone, Default)]
pub struct ExportReport {
    /// Ítems escritos al XML.
    pub escritos: usize,
    /// `(etiqueta, motivo)` de lo omitido.
    pub omitidos: Vec<(String, String)>,
}

fn fmt_num(v: f64) -> String {
    if !v.is_finite() {
        return "0".to_string();
    }
    let s = format!("{v:.6}");
    let s = s.trim_end_matches('0').trim_end_matches('.');
    if s.is_empty() || s == "-0" {
        "0".to_string()
    } else {
        s.to_string()
    }
}

fn clean_label(raw: &str) -> Result<String, String> {
    let label = raw.trim().to_string();
    if label.is_empty() {
        return Err("etiqueta vacía".to_string());
    }
    if label.len() > MAX_EXPORT_LABEL_CHARS {
        return Err(format!(
            "etiqueta excede {MAX_EXPORT_LABEL_CHARS} caracteres"
        ));
    }
    if label.contains('\0') {
        return Err("etiqueta con NUL".to_string());
    }
    if label.len() > MAX_EXPR_CHARS {
        return Err(format!("etiqueta excede {MAX_EXPR_CHARS} caracteres"));
    }
    Ok(label)
}

fn finite(value: f64, what: &str) -> Result<f64, String> {
    if value.is_finite() {
        Ok(value)
    } else {
        Err(format!("{what} no finito"))
    }
}

/// Literal `(x, y)` (lo que el importador acepta vía `parse_point_literal`).
///
/// Permite exportar segmentos/polígonos sin crear puntos intermedios: el
/// importador resuelve literales igual que etiquetas.
fn point_literal(text: &str) -> Option<(f64, f64)> {
    let inner = text.trim().strip_prefix('(')?.strip_suffix(')')?;
    let mut parts = inner.split(',');
    let x: f64 = parts.next()?.trim().parse().ok()?;
    let y: f64 = parts.next()?.trim().parse().ok()?;
    if parts.next().is_some() {
        return None;
    }
    if x.is_finite() && y.is_finite() {
        Some((x, y))
    } else {
        None
    }
}

/// Referencia válida: etiqueta de punto emitido o literal `(x, y)`.
fn point_ref_known(known: &BTreeSet<String>, reference: &str) -> bool {
    known.contains(reference) || point_literal(reference).is_some()
}

/// Serializa ítems a `geogebra.xml` y los empaqueta en un `.ggb` (ZIP).
///
/// Ordena primero los puntos (los comandos los referencian por etiqueta);
/// el resto conserva el orden dado. Etiquetas duplicadas o referencias a
/// puntos no emitidos se omiten con motivo.
pub fn export_ggb_bytes(items: &[GgbExportItem]) -> Result<(Vec<u8>, ExportReport), GgbError> {
    if items.len() > MAX_ELEMS {
        return Err(GgbError::LimiteElementos {
            encontrados: items.len(),
            limite: MAX_ELEMS,
        });
    }
    let mut report = ExportReport::default();
    let mut writer = Writer::new(Cursor::new(Vec::new()));
    writer
        .write_event(Event::Decl(BytesDecl::new("1.0", Some("UTF-8"), None)))
        .map_err(|e| GgbError::XmlMalformado {
            detalle: GgbError::recorta(&e.to_string()),
        })?;
    write_open(&mut writer, "geogebra", &[("format", "5.0")])?;
    write_open(&mut writer, "construction", &[])?;

    // Puntos primero: poblan el mapa que usan los comandos.
    let mut known: BTreeSet<String> = BTreeSet::new();
    let mut seen: BTreeSet<String> = BTreeSet::new();
    for item in items {
        if let GgbExportItem::Point { label, x, y } = item {
            let label = match clean_label(label) {
                Ok(label) => label,
                Err(reason) => {
                    report.omitidos.push((label.clone(), reason));
                    continue;
                }
            };
            let (Ok(x), Ok(y)) = (finite(*x, "x"), finite(*y, "y")) else {
                report
                    .omitidos
                    .push((label, "coordenadas no finitas".to_string()));
                continue;
            };
            if !seen.insert(label.clone()) {
                report
                    .omitidos
                    .push((label, "etiqueta duplicada".to_string()));
                continue;
            }
            write_element_point(&mut writer, &label, x, y)?;
            known.insert(label);
            report.escritos += 1;
        }
    }
    for item in items {
        match item {
            GgbExportItem::Point { .. } => {}
            GgbExportItem::Segment { label, from, to } => {
                let label = match clean_label(label) {
                    Ok(label) => label,
                    Err(reason) => {
                        report.omitidos.push((label.clone(), reason));
                        continue;
                    }
                };
                if !point_ref_known(&known, from) || !point_ref_known(&known, to) {
                    report.omitidos.push((
                        label,
                        "segmento con vértices no emitidos (primero los puntos)".to_string(),
                    ));
                    continue;
                }
                if !seen.insert(label.clone()) {
                    report
                        .omitidos
                        .push((label, "etiqueta duplicada".to_string()));
                    continue;
                }
                write_command(&mut writer, "Segment", &[from, to], &label)?;
                report.escritos += 1;
            }
            GgbExportItem::Vector { label, from, to } => {
                let label = match clean_label(label) {
                    Ok(label) => label,
                    Err(reason) => {
                        report.omitidos.push((label.clone(), reason));
                        continue;
                    }
                };
                if !point_ref_known(&known, from) || !point_ref_known(&known, to) {
                    report.omitidos.push((
                        label,
                        "vector con extremos no emitidos (primero los puntos)".to_string(),
                    ));
                    continue;
                }
                if !seen.insert(label.clone()) {
                    report
                        .omitidos
                        .push((label, "etiqueta duplicada".to_string()));
                    continue;
                }
                write_command(&mut writer, "Vector", &[from, to], &label)?;
                report.escritos += 1;
            }
            GgbExportItem::Circle { label, cx, cy, r } => {
                let label = match clean_label(label) {
                    Ok(label) => label,
                    Err(reason) => {
                        report.omitidos.push((label.clone(), reason));
                        continue;
                    }
                };
                let (cx, cy, r) = match (finite(*cx, "cx"), finite(*cy, "cy"), finite(*r, "r")) {
                    (Ok(cx), Ok(cy), Ok(r)) if r > 1e-12 => (cx, cy, r),
                    _ => {
                        report
                            .omitidos
                            .push((label, "círculo con centro o radio no válido".to_string()));
                        continue;
                    }
                };
                if !seen.insert(label.clone()) {
                    report
                        .omitidos
                        .push((label, "etiqueta duplicada".to_string()));
                    continue;
                }
                write_element_circle(&mut writer, &label, cx, cy, r)?;
                report.escritos += 1;
            }
            GgbExportItem::Polygon { label, vertices } => {
                let label = match clean_label(label) {
                    Ok(label) => label,
                    Err(reason) => {
                        report.omitidos.push((label.clone(), reason));
                        continue;
                    }
                };
                if vertices.len() < 3 {
                    report
                        .omitidos
                        .push((label, "polígono requiere ≥3 vértices".to_string()));
                    continue;
                }
                if vertices.iter().any(|v| !point_ref_known(&known, v)) {
                    report.omitidos.push((
                        label,
                        "polígono con vértices no emitidos (primero los puntos)".to_string(),
                    ));
                    continue;
                }
                let refs: Vec<&str> = vertices.iter().map(String::as_str).collect();
                if !seen.insert(label.clone()) {
                    report
                        .omitidos
                        .push((label, "etiqueta duplicada".to_string()));
                    continue;
                }
                write_command(&mut writer, "Polygon", &refs, &label)?;
                report.escritos += 1;
            }
            GgbExportItem::Slider {
                label,
                value,
                min,
                max,
            } => {
                let label = match clean_label(label) {
                    Ok(label) => label,
                    Err(reason) => {
                        report.omitidos.push((label.clone(), reason));
                        continue;
                    }
                };
                let ok = [value, min, max].iter().all(|v| v.is_finite()) && min < max;
                if !ok {
                    report
                        .omitidos
                        .push((label, "slider con rango inválido".to_string()));
                    continue;
                }
                if !seen.insert(label.clone()) {
                    report
                        .omitidos
                        .push((label, "etiqueta duplicada".to_string()));
                    continue;
                }
                write_element_numeric(&mut writer, &label, *value, Some((*min, *max)))?;
                report.escritos += 1;
            }
        }
    }
    write_close(&mut writer, "construction")?;
    write_close(&mut writer, "geogebra")?;
    let xml = writer.into_inner().into_inner();

    let mut zip = zip::ZipWriter::new(Cursor::new(Vec::new()));
    let options = SimpleFileOptions::default().compression_method(zip::CompressionMethod::Deflated);
    zip.start_file(GGB_XML_NAME, options)
        .map_err(|e| GgbError::ZipInvalido {
            detalle: GgbError::recorta(&e.to_string()),
        })?;
    zip.write_all(&xml).map_err(|e| GgbError::ZipInvalido {
        detalle: GgbError::recorta(&e.to_string()),
    })?;
    let bytes = zip
        .finish()
        .map_err(|e| GgbError::ZipInvalido {
            detalle: GgbError::recorta(&e.to_string()),
        })?
        .into_inner();
    Ok((bytes, report))
}

fn write_open(
    writer: &mut Writer<Cursor<Vec<u8>>>,
    name: &str,
    attrs: &[(&str, &str)],
) -> Result<(), GgbError> {
    let mut elem = BytesStart::new(name);
    for (key, value) in attrs {
        elem.push_attribute((*key, *value));
    }
    writer
        .write_event(Event::Start(elem))
        .map_err(|e| GgbError::XmlMalformado {
            detalle: GgbError::recorta(&e.to_string()),
        })
}

fn write_close(writer: &mut Writer<Cursor<Vec<u8>>>, name: &str) -> Result<(), GgbError> {
    writer
        .write_event(Event::End(BytesEnd::new(name)))
        .map_err(|e| GgbError::XmlMalformado {
            detalle: GgbError::recorta(&e.to_string()),
        })
}

fn write_empty(
    writer: &mut Writer<Cursor<Vec<u8>>>,
    name: &str,
    attrs: &[(&str, String)],
) -> Result<(), GgbError> {
    let mut elem = BytesStart::new(name);
    for (key, value) in attrs {
        elem.push_attribute((*key, value.as_str()));
    }
    writer
        .write_event(Event::Empty(elem))
        .map_err(|e| GgbError::XmlMalformado {
            detalle: GgbError::recorta(&e.to_string()),
        })
}

fn write_element_point(
    writer: &mut Writer<Cursor<Vec<u8>>>,
    label: &str,
    x: f64,
    y: f64,
) -> Result<(), GgbError> {
    write_open(writer, "element", &[("type", "point"), ("label", label)])?;
    write_empty(
        writer,
        "coords",
        &[("x", fmt_num(x)), ("y", fmt_num(y)), ("z", "1".to_string())],
    )?;
    write_close(writer, "element")
}

fn write_element_circle(
    writer: &mut Writer<Cursor<Vec<u8>>>,
    label: &str,
    cx: f64,
    cy: f64,
    r: f64,
) -> Result<(), GgbError> {
    write_open(writer, "element", &[("type", "circle"), ("label", label)])?;
    write_empty(
        writer,
        "coords",
        &[
            ("x", fmt_num(cx)),
            ("y", fmt_num(cy)),
            ("z", "1".to_string()),
        ],
    )?;
    write_empty(writer, "value", &[("val", fmt_num(r))])?;
    write_close(writer, "element")
}

fn write_element_numeric(
    writer: &mut Writer<Cursor<Vec<u8>>>,
    label: &str,
    value: f64,
    slider: Option<(f64, f64)>,
) -> Result<(), GgbError> {
    write_open(writer, "element", &[("type", "numeric"), ("label", label)])?;
    write_empty(writer, "value", &[("val", fmt_num(value))])?;
    if let Some((min, max)) = slider {
        write_empty(
            writer,
            "slider",
            &[("min", fmt_num(min)), ("max", fmt_num(max))],
        )?;
    }
    write_close(writer, "element")
}

fn write_command(
    writer: &mut Writer<Cursor<Vec<u8>>>,
    name: &str,
    inputs: &[&str],
    output: &str,
) -> Result<(), GgbError> {
    write_open(writer, "command", &[("name", name)])?;
    let mut elem = BytesStart::new("input");
    for (i, input) in inputs.iter().enumerate() {
        elem.push_attribute((format!("a{i}").as_str(), *input));
    }
    writer
        .write_event(Event::Empty(elem))
        .map_err(|e| GgbError::XmlMalformado {
            detalle: GgbError::recorta(&e.to_string()),
        })?;
    let mut out = BytesStart::new("output");
    out.push_attribute(("a0", output));
    writer
        .write_event(Event::Empty(out))
        .map_err(|e| GgbError::XmlMalformado {
            detalle: GgbError::recorta(&e.to_string()),
        })?;
    write_close(writer, "command")
}

#[cfg(test)]
#[allow(clippy::expect_used)]
mod export_tests {
    use super::*;
    use crate::import_ggb_bytes;

    fn roundtrip(items: Vec<GgbExportItem>) -> crate::ImportReport {
        let (bytes, report) = export_ggb_bytes(&items).expect("exporta");
        assert!(
            report.omitidos.is_empty(),
            "omitidos: {:?}",
            report.omitidos
        );
        import_ggb_bytes(&bytes).expect("re-importa")
    }

    #[test]
    fn export_roundtrip_labels_survive() {
        let items = vec![
            GgbExportItem::Point {
                label: "A".into(),
                x: 1.0,
                y: 2.0,
            },
            GgbExportItem::Point {
                label: "B".into(),
                x: 4.0,
                y: 6.0,
            },
            GgbExportItem::Point {
                label: "C".into(),
                x: 0.0,
                y: 3.0,
            },
            GgbExportItem::Segment {
                label: "s".into(),
                from: "A".into(),
                to: "B".into(),
            },
            GgbExportItem::Vector {
                label: "v".into(),
                from: "A".into(),
                to: "C".into(),
            },
            GgbExportItem::Circle {
                label: "c".into(),
                cx: 0.0,
                cy: 0.0,
                r: 2.0,
            },
            GgbExportItem::Polygon {
                label: "P".into(),
                vertices: vec!["A".into(), "B".into(), "C".into()],
            },
            GgbExportItem::Slider {
                label: "t".into(),
                value: 5.0,
                min: 0.0,
                max: 10.0,
            },
        ];
        let report = roundtrip(items);
        let labels: Vec<&str> = report.objetos.iter().map(|o| o.etiqueta.as_str()).collect();
        // Los numéricos sin slider no tienen mapeo de importación (diseño).
        for expected in ["A", "B", "C", "s", "v", "c", "P", "t"] {
            assert!(labels.contains(&expected), "falta {expected} en {labels:?}");
        }
    }

    #[test]
    fn export_omits_honestly() {
        let items = vec![
            GgbExportItem::Point {
                label: "".into(),
                x: 0.0,
                y: 0.0,
            },
            GgbExportItem::Point {
                label: "A".into(),
                x: f64::NAN,
                y: 0.0,
            },
            GgbExportItem::Segment {
                label: "s".into(),
                from: "A".into(),
                to: "Z".into(),
            },
            GgbExportItem::Point {
                label: "A".into(),
                x: 1.0,
                y: 1.0,
            },
        ];
        let (bytes, report) = export_ggb_bytes(&items).expect("exporta igual");
        assert_eq!(report.escritos, 1);
        assert_eq!(report.omitidos.len(), 3);
        // El ZIP sigue siendo válido aunque todo se omita parcialmente.
        assert!(!bytes.is_empty());
    }
}
