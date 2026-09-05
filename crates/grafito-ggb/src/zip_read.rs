//! Extracción endurecida de `geogebra.xml` desde el contenedor ZIP.
use crate::error::GgbError;
use crate::{GGB_XML_NAME, MAX_GGB_BYTES, MAX_GGB_XML_BYTES, MAX_ZIP_ENTRIES, MAX_ZIP_RATIO};
use std::io::{Cursor, Read};
const NOMBRES_SCRIPT: [&str; 2] = ["geogebra.js", "geogebra_javascript.js"];
pub(crate) struct XmlExtraido {
    pub xml: Vec<u8>,
    pub con_js: bool,
    pub hoja_csv: Option<Vec<u8>>,
}
pub(crate) fn extraer(bytes: &[u8]) -> Result<XmlExtraido, GgbError> {
    if bytes.is_empty() {
        return Err(GgbError::Vacio);
    }
    let total = bytes.len() as u64;
    if total > MAX_GGB_BYTES {
        return Err(GgbError::DemasiadoGrande {
            bytes: total,
            limite: MAX_GGB_BYTES,
        });
    }
    let lector = Cursor::new(bytes);
    let mut archivo = zip::ZipArchive::new(lector).map_err(|e| GgbError::ZipInvalido {
        detalle: GgbError::recorta(&e.to_string()),
    })?;
    let n = archivo.len();
    if n > MAX_ZIP_ENTRIES {
        return Err(GgbError::DemasiadasEntradas {
            encontradas: n as u64,
            limite: MAX_ZIP_ENTRIES as u64,
        });
    }
    let mut indice_xml: Option<usize> = None;
    let mut con_js = false;
    let mut hoja_csv_idx: Option<usize> = None;
    let mut hoja_csv_name: Option<String> = None;
    for i in 0..n {
        let entrada = archivo.by_index(i).map_err(|e| GgbError::ZipInvalido {
            detalle: GgbError::recorta(&e.to_string()),
        })?;
        let nombre = entrada.name().to_string();
        validar_nombre(&nombre)?;
        validar_no_enlace(&nombre, entrada.unix_mode())?;
        let metodo = entrada.compression();
        if metodo != zip::CompressionMethod::Stored && metodo != zip::CompressionMethod::Deflated {
            return Err(GgbError::MetodoNoSoportado {
                entrada: nombre,
                metodo: format!("{metodo:?}"),
            });
        }
        if entrada.is_dir() {
            continue;
        }
        if nombre == GGB_XML_NAME && indice_xml.is_none() {
            indice_xml = Some(i);
        }
        if NOMBRES_SCRIPT.contains(&nombre.as_str()) {
            con_js = true;
        }
        let lower = nombre.to_ascii_lowercase();
        if (lower.ends_with(".csv") || lower.ends_with(".tsv") || lower.ends_with(".txt"))
            && hoja_csv_idx.is_none()
            && nombre != GGB_XML_NAME
        {
            hoja_csv_idx = Some(i);
            hoja_csv_name = Some(nombre.clone());
        }
    }
    let i = match indice_xml {
        Some(i) => i,
        None => return Err(GgbError::XmlFaltante),
    };
    let entrada = archivo.by_index(i).map_err(|e| GgbError::ZipInvalido {
        detalle: GgbError::recorta(&e.to_string()),
    })?;
    let tam = entrada.size();
    let comp = entrada.compressed_size();
    if tam > MAX_GGB_XML_BYTES {
        return Err(GgbError::XmlDemasiadoGrande {
            bytes: tam,
            limite: MAX_GGB_XML_BYTES,
        });
    }
    if comp > 0 {
        let techo = comp.checked_mul(MAX_ZIP_RATIO).ok_or(GgbError::BombaZip {
            entrada: GGB_XML_NAME.to_string(),
            detalle: "desbordamiento al aplicar ratio-guard".to_string(),
        })?;
        if tam > techo {
            return Err(GgbError::BombaZip {
                entrada: GGB_XML_NAME.to_string(),
                detalle: format!("{tam} B declarados frente a {comp} B comprimidos"),
            });
        }
    }
    let techo_lectura = MAX_GGB_XML_BYTES.saturating_add(1);
    let mut xml = Vec::new();
    entrada
        .take(techo_lectura)
        .read_to_end(&mut xml)
        .map_err(|e| GgbError::ZipInvalido {
            detalle: GgbError::recorta(&e.to_string()),
        })?;
    if xml.len() as u64 > MAX_GGB_XML_BYTES {
        return Err(GgbError::XmlDemasiadoGrande {
            bytes: xml.len() as u64,
            limite: MAX_GGB_XML_BYTES,
        });
    }
    rechazar_doctype(&xml)?;
    let hoja_csv = if let Some(idx) = hoja_csv_idx {
        let e = archivo.by_index(idx).map_err(|e| GgbError::ZipInvalido {
            detalle: GgbError::recorta(&e.to_string()),
        })?;
        const MAX_HOJA_BYTES: u64 = 2 * 1024 * 1024;
        if e.size() > MAX_HOJA_BYTES {
            None
        } else {
            let mut buf = Vec::new();
            let mut limited = e.take(MAX_HOJA_BYTES + 1);
            limited
                .read_to_end(&mut buf)
                .map_err(|e| GgbError::ZipInvalido {
                    detalle: GgbError::recorta(&e.to_string()),
                })?;
            if buf.len() as u64 > MAX_HOJA_BYTES {
                None
            } else if std::str::from_utf8(&buf).is_ok() {
                Some(buf)
            } else {
                None
            }
        }
    } else {
        None
    };
    let _ = hoja_csv_name;
    Ok(XmlExtraido {
        xml,
        con_js,
        hoja_csv,
    })
}
fn validar_nombre(nombre: &str) -> Result<(), GgbError> {
    if nombre.starts_with('/') || nombre.starts_with('\\') {
        return Err(GgbError::EntradaPeligrosa {
            nombre: nombre.to_string(),
            motivo: "ruta absoluta",
        });
    }
    let prefijo = nombre.as_bytes();
    if prefijo.len() >= 2 && prefijo[0].is_ascii_alphabetic() && prefijo[1] == b':' {
        return Err(GgbError::EntradaPeligrosa {
            nombre: nombre.to_string(),
            motivo: "ruta con unidad",
        });
    }
    let normalizado = nombre.replace('\\', "/");
    let mut profundidad: usize = 0;
    for parte in normalizado.split('/') {
        match parte {
            "" | "." => {}
            ".." => {
                profundidad = profundidad
                    .checked_sub(1)
                    .ok_or(GgbError::EntradaPeligrosa {
                        nombre: nombre.to_string(),
                        motivo: "ruta fuera del archivo (..)",
                    })?;
            }
            _ => {
                profundidad = profundidad.saturating_add(1);
            }
        }
    }
    Ok(())
}
fn validar_no_enlace(nombre: &str, modo: Option<u32>) -> Result<(), GgbError> {
    let modo = match modo {
        Some(m) => m,
        None => return Ok(()),
    };
    if modo & 0o170_000 == 0o120_000 {
        return Err(GgbError::EntradaPeligrosa {
            nombre: nombre.to_string(),
            motivo: "enlace simbólico",
        });
    }
    Ok(())
}
fn rechazar_doctype(xml: &[u8]) -> Result<(), GgbError> {
    if contiene(xml, b"<!DOCTYPE") || contiene(xml, b"<!ENTITY") {
        return Err(GgbError::XmlMalformado {
            detalle: "DOCTYPE/ENTITY rechazado (bomba de entidades)".to_string(),
        });
    }
    Ok(())
}
fn contiene(hay: &[u8], aguja: &[u8]) -> bool {
    if aguja.is_empty() || hay.len() < aguja.len() {
        return false;
    }
    hay.windows(aguja.len()).any(|v| v == aguja)
}
