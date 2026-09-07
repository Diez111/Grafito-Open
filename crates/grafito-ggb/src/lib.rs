//! Importador `.ggb` (GeoGebra) a comandos Grafito — Cerebro puro.
mod error;
mod map;
mod model;
mod parse;
#[cfg(test)]
mod tests;
mod zip_read;
pub use error::GgbError;
use std::collections::BTreeMap;
pub const GGB_XML_NAME: &str = "geogebra.xml";
pub const MAX_GGB_BYTES: u64 = 64 * 1024 * 1024;
pub const MAX_GGB_XML_BYTES: u64 = 10 * 1024 * 1024;
pub const MAX_ELEMS: usize = 5000;
pub const MAX_DATA_TABLE_ROWS: usize = 20_000;
pub const MAX_ZIP_ENTRIES: usize = 4096;
pub const MAX_ZIP_RATIO: u64 = 100;
pub const MAX_EXPR_CHARS: usize = 2000;
pub const MAX_ATTR_BYTES: usize = 8192;
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MappedObject {
    pub etiqueta: String,
    pub tipo: String,
    pub comando: String,
}
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OmittedObject {
    pub tipo: String,
    pub label: String,
    pub razon: String,
}
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ImportReport {
    pub mapeados: usize,
    pub objetos: Vec<MappedObject>,
    pub tipos: BTreeMap<String, usize>,
    pub omitidos: Vec<OmittedObject>,
}
impl ImportReport {
    pub fn commands(&self) -> Vec<String> {
        self.objetos.iter().map(|o| o.comando.clone()).collect()
    }
    pub fn script(&self) -> String {
        self.commands().join("\n")
    }
    pub fn summary(&self) -> String {
        let tipos = self
            .tipos
            .iter()
            .map(|(tipo, n)| format!("{tipo} x{n}"))
            .collect::<Vec<_>>()
            .join(", ");
        format!(
            "ggb importado: {} objetos ({}) + {} omitidos",
            self.mapeados,
            if tipos.is_empty() {
                "sin objetos"
            } else {
                &tipos
            },
            self.omitidos.len()
        )
    }
}
pub fn import_ggb_bytes(bytes: &[u8]) -> Result<ImportReport, GgbError> {
    let extraido = zip_read::extraer(bytes)?;
    let construccion = parse::parsear(&extraido.xml)?;
    let mut reporte = map::mapear(&construccion);
    if let Some(csv) = extraido.hoja_csv {
        if let Some((xs, ys)) = map::parse_csv_like_to_xy(&csv) {
            let already_has_table = reporte.tipos.contains_key("DataTable");
            if !already_has_table && xs.len() >= 2 && xs.len() <= MAX_DATA_TABLE_ROWS {
                let xs_str = xs
                    .iter()
                    .map(|v| {
                        if !v.is_finite() {
                            "0".to_string()
                        } else {
                            let s = format!("{v:.6}");
                            let s = s.trim_end_matches('0').trim_end_matches('.');
                            if s.is_empty() || s == "-0" {
                                "0".to_string()
                            } else {
                                s.to_string()
                            }
                        }
                    })
                    .collect::<Vec<_>>()
                    .join(", ");
                let ys_str = ys
                    .iter()
                    .map(|v| {
                        if !v.is_finite() {
                            "0".to_string()
                        } else {
                            let s = format!("{v:.6}");
                            let s = s.trim_end_matches('0').trim_end_matches('.');
                            if s.is_empty() || s == "-0" {
                                "0".to_string()
                            } else {
                                s.to_string()
                            }
                        }
                    })
                    .collect::<Vec<_>>()
                    .join(", ");
                let dt_cmd = format!("DataTable[{{{xs_str}}}, {{{ys_str}}}]");
                let sp_cmd = format!("ScatterPlot[{{{xs_str}}}, {{{ys_str}}}]");
                if dt_cmd.len() <= MAX_EXPR_CHARS
                    && sp_cmd.len() <= MAX_EXPR_CHARS
                    && reporte.objetos.len() + 2 <= MAX_ELEMS
                {
                    reporte
                        .tipos
                        .entry("DataTable".to_string())
                        .and_modify(|e| *e += 1)
                        .or_insert(1);
                    reporte
                        .tipos
                        .entry("ScatterPlot".to_string())
                        .and_modify(|e| *e += 1)
                        .or_insert(1);
                    reporte.objetos.push(MappedObject {
                        etiqueta: "x_y".to_string(),
                        tipo: "DataTable".to_string(),
                        comando: dt_cmd,
                    });
                    reporte.objetos.push(MappedObject {
                        etiqueta: "scatter".to_string(),
                        tipo: "ScatterPlot".to_string(),
                        comando: sp_cmd,
                    });
                    reporte.mapeados = reporte.objetos.len();
                }
            }
        }
    }
    if extraido.con_js || construccion.con_script {
        reporte.omitidos.push(OmittedObject {
            tipo: "script".to_string(),
            label: String::new(),
            razon: "scripts (geogebra.js/ggbscript) no soportados en núcleo aula F2".to_string(),
        });
    }
    if construccion.con_cas {
        reporte.omitidos.push(OmittedObject {
            tipo: "cas".to_string(),
            label: String::new(),
            razon: "CAS (cascell) omitido en núcleo aula".to_string(),
        });
    }
    Ok(reporte)
}
