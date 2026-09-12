//! Parseo en streaming de `geogebra.xml` con `quick-xml`.
use crate::error::GgbError;
use crate::model::{Construccion, GgbComando, GgbElemento, GgbExpresion, ItemOrden};
use crate::{MAX_ATTR_BYTES, MAX_ELEMS, MAX_XML_ATTRS_PER_ELEMENT, MAX_XML_DEPTH};
use quick_xml::events::{BytesStart, Event};
use quick_xml::Reader;
use quick_xml::XmlVersion;
const MAX_IO_ATTRS: usize = 64;
fn es_celda_hoja(etiqueta: &str) -> bool {
    let bytes = etiqueta.as_bytes();
    if bytes.len() < 2 || bytes.len() > 6 {
        return false;
    }
    let mut i = 0;
    let mut letras = 0;
    while i < bytes.len() && bytes[i].is_ascii_alphabetic() {
        letras += 1;
        i += 1;
    }
    if letras == 0 || letras > 2 {
        return false;
    }
    if i == bytes.len() {
        return false;
    }
    while i < bytes.len() {
        if !bytes[i].is_ascii_digit() {
            return false;
        }
        i += 1;
    }
    true
}
pub(crate) fn parsear(xml: &[u8]) -> Result<Construccion, GgbError> {
    // Fail-closed ante XML hostil: `quick-xml` no expande DTD ni entidades
    // externas (los eventos `DocType` se ignorarían), pero se rechaza el
    // DOCTYPE explícito aquí y en `zip_read` para no depender de un solo punto.
    rechazar_doctype(xml)?;
    let mut lector = Reader::from_reader(xml);
    lector.config_mut().trim_text(true);
    let mut c = Construccion::default();
    let mut en_construccion = false;
    let mut conteo: usize = 0;
    let mut profundidad: u32 = 0;
    let mut prof_cas: u32 = 0;
    let mut elem: Option<GgbElemento> = None;
    let mut cmd: Option<GgbComando> = None;
    let mut buf = Vec::new();
    loop {
        let evento = lector
            .read_event_into(&mut buf)
            .map_err(|e| GgbError::XmlMalformado {
                detalle: GgbError::recorta(&e.to_string()),
            })?;
        match evento {
            Event::Eof => break,
            Event::Start(ref e) => {
                profundidad = profundidad.saturating_add(1);
                if profundidad > MAX_XML_DEPTH {
                    return Err(GgbError::XmlMalformado {
                        detalle: format!("profundidad XML excede {MAX_XML_DEPTH} niveles"),
                    });
                }
                manejar_apertura(
                    e,
                    &mut c,
                    &mut en_construccion,
                    &mut conteo,
                    &mut prof_cas,
                    &mut elem,
                    &mut cmd,
                    false,
                )?;
            }
            Event::Empty(ref e) => {
                if profundidad.saturating_add(1) > MAX_XML_DEPTH {
                    return Err(GgbError::XmlMalformado {
                        detalle: format!("profundidad XML excede {MAX_XML_DEPTH} niveles"),
                    });
                }
                manejar_apertura(
                    e,
                    &mut c,
                    &mut en_construccion,
                    &mut conteo,
                    &mut prof_cas,
                    &mut elem,
                    &mut cmd,
                    true,
                )?;
            }
            Event::End(ref e) => {
                profundidad = profundidad.saturating_sub(1);
                let qname = e.name();
                let nombre: &str = qname.as_ref();
                match nombre {
                    "construction" => en_construccion = false,
                    "cascell" => prof_cas = prof_cas.saturating_sub(1),
                    "element" => {
                        if let Some(mut g) = elem.take() {
                            g.es_celda_hoja = es_celda_hoja(&g.etiqueta);
                            c.orden.push(ItemOrden::Elemento(c.elementos.len()));
                            c.elementos.push(g);
                        }
                    }
                    "command" => {
                        if let Some(g) = cmd.take() {
                            c.orden.push(ItemOrden::Comando(c.comandos.len()));
                            c.comandos.push(g);
                        }
                    }
                    _ => {}
                }
            }
            _ => {}
        }
        buf.clear();
    }
    if let Some(mut g) = elem.take() {
        g.es_celda_hoja = es_celda_hoja(&g.etiqueta);
        c.orden.push(ItemOrden::Elemento(c.elementos.len()));
        c.elementos.push(g);
    }
    if let Some(g) = cmd.take() {
        c.orden.push(ItemOrden::Comando(c.comandos.len()));
        c.comandos.push(g);
    }
    Ok(c)
}
fn contar(conteo: &mut usize) -> Result<(), GgbError> {
    *conteo = conteo.checked_add(1).ok_or(GgbError::LimiteElementos {
        encontrados: usize::MAX,
        limite: MAX_ELEMS,
    })?;
    if *conteo > MAX_ELEMS {
        return Err(GgbError::LimiteElementos {
            encontrados: *conteo,
            limite: MAX_ELEMS,
        });
    }
    Ok(())
}
#[allow(clippy::too_many_arguments)]
fn manejar_apertura(
    e: &BytesStart<'_>,
    c: &mut Construccion,
    en_construccion: &mut bool,
    conteo: &mut usize,
    prof_cas: &mut u32,
    elem: &mut Option<GgbElemento>,
    cmd: &mut Option<GgbComando>,
    autocerrado: bool,
) -> Result<(), GgbError> {
    let qname = e.name();
    let nombre: &str = qname.as_ref();
    // Cota anti-quadratic-blowup por elemento: falla antes de normalizar
    // valores cuando un tag trae una lluvia de atributos.
    let mut n_attrs: usize = 0;
    for resultado in e.attributes() {
        resultado.map_err(|e| GgbError::XmlMalformado {
            detalle: GgbError::recorta(&e.to_string()),
        })?;
        n_attrs = n_attrs.saturating_add(1);
        if n_attrs > MAX_XML_ATTRS_PER_ELEMENT {
            return Err(GgbError::XmlMalformado {
                detalle: format!(
                    "demasiados atributos en <{nombre}: límite {MAX_XML_ATTRS_PER_ELEMENT}"
                ),
            });
        }
    }
    if *prof_cas > 0 {
        return Ok(());
    }
    match nombre {
        "construction" => *en_construccion = true,
        "element" if *en_construccion => {
            contar(conteo)?;
            if let Some(mut g) = elem.take() {
                g.es_celda_hoja = es_celda_hoja(&g.etiqueta);
                c.orden.push(ItemOrden::Elemento(c.elementos.len()));
                c.elementos.push(g);
            }
            let tipo = attr(e, "type")?.unwrap_or_default();
            let etiqueta = attr(e, "label")?.unwrap_or_default();
            *elem = Some(GgbElemento {
                tipo,
                etiqueta,
                coords: None,
                valor: None,
                deslizador: None,
                matrix: None,
                eigen: None,
                vector_start: None,
                texto: None,
                es_celda_hoja: false,
            });
            if autocerrado {
                if let Some(mut g) = elem.take() {
                    g.es_celda_hoja = es_celda_hoja(&g.etiqueta);
                    c.orden.push(ItemOrden::Elemento(c.elementos.len()));
                    c.elementos.push(g);
                }
            }
        }
        "command" if *en_construccion => {
            contar(conteo)?;
            if let Some(g) = cmd.take() {
                c.orden.push(ItemOrden::Comando(c.comandos.len()));
                c.comandos.push(g);
            }
            let nombre_cmd = attr(e, "name")?.unwrap_or_default();
            *cmd = Some(GgbComando {
                nombre: nombre_cmd,
                entradas: Vec::new(),
                salidas: Vec::new(),
            });
            if autocerrado {
                if let Some(g) = cmd.take() {
                    c.orden.push(ItemOrden::Comando(c.comandos.len()));
                    c.comandos.push(g);
                }
            }
        }
        "expression" if *en_construccion => {
            contar(conteo)?;
            c.expresiones.push(GgbExpresion {
                etiqueta: attr(e, "label")?.unwrap_or_default(),
                exp: attr(e, "exp")?.unwrap_or_default(),
                tipo: attr(e, "type")?.unwrap_or_default(),
            });
        }
        "coords" => {
            if let Some(g) = elem.as_mut() {
                let x = num_attr(e, "x")?;
                let y = num_attr(e, "y")?;
                if let (Some(x), Some(y)) = (x, y) {
                    let z = num_attr(e, "z")?.unwrap_or(1.0);
                    let w = num_attr(e, "w")?.unwrap_or(1.0);
                    g.coords = Some([x, y, z, w]);
                }
            }
        }
        "value" => {
            if let Some(g) = elem.as_mut() {
                g.valor = num_attr(e, "val")?;
            }
        }
        "slider" => {
            if let Some(g) = elem.as_mut() {
                let min = num_attr(e, "min")?;
                let max = num_attr(e, "max")?;
                if let (Some(min), Some(max)) = (min, max) {
                    g.deslizador = Some((min, max));
                }
            }
        }
        "matrix" => {
            if let Some(g) = elem.as_mut() {
                let a0 = num_attr(e, "A0")?;
                let a1 = num_attr(e, "A1")?;
                let a2 = num_attr(e, "A2")?;
                let a3 = num_attr(e, "A3")?;
                let a4 = num_attr(e, "A4")?;
                let a5 = num_attr(e, "A5")?;
                if let (Some(a0), Some(a1), Some(a2), Some(a3), Some(a4), Some(a5)) =
                    (a0, a1, a2, a3, a4, a5)
                {
                    if a0.is_finite()
                        && a1.is_finite()
                        && a2.is_finite()
                        && a3.is_finite()
                        && a4.is_finite()
                        && a5.is_finite()
                    {
                        g.matrix = Some([a0, a1, a2, a3, a4, a5]);
                    }
                }
            }
        }
        "eigenvectors" => {
            if let Some(g) = elem.as_mut() {
                let x0 = num_attr(e, "x0")?;
                let y0 = num_attr(e, "y0")?;
                let x1 = num_attr(e, "x1")?;
                let y1 = num_attr(e, "y1")?;
                if let (Some(x0), Some(y0), Some(x1), Some(y1)) = (x0, y0, x1, y1) {
                    if x0.is_finite() && y0.is_finite() && x1.is_finite() && y1.is_finite() {
                        g.eigen = Some([x0, y0, x1, y1]);
                    }
                }
            }
        }
        "coefficients" => {
            if let Some(g) = elem.as_mut() {
                if let Some(data) = attr(e, "data")? {
                    if data.len() <= MAX_ATTR_BYTES {
                        if let Some(mat) = parse_coefficients_data(&data) {
                            g.matrix = Some(mat);
                        }
                    }
                }
            }
        }
        "startPoint" => {
            if let Some(g) = elem.as_mut() {
                let x = num_attr(e, "x")?;
                let y = num_attr(e, "y")?;
                if let (Some(x), Some(y)) = (x, y) {
                    if x.is_finite() && y.is_finite() {
                        g.vector_start = Some([x, y]);
                    }
                } else if let Some(exp) = attr(e, "exp")? {
                    let _ = exp;
                }
            }
        }
        "caption" => {
            if let Some(g) = elem.as_mut() {
                if let Some(val) = attr(e, "val")? {
                    if val.len() <= MAX_ATTR_BYTES {
                        g.texto = Some(val);
                    }
                }
            }
        }
        "cell" => {
            let mut fila: Vec<String> = Vec::new();
            for key in ["val", "value", "content", "exp", "input"] {
                if let Some(v) = attr(e, key)? {
                    if !v.is_empty() && v.len() <= super::MAX_ATTR_BYTES {
                        fila.push(v);
                    }
                }
            }
            if let Some(n) = num_attr(e, "val")? {
                fila.push(format!("{n}"));
            }
            if !fila.is_empty() && c.hoja_celdas.len() < crate::MAX_DATA_TABLE_ROWS {
                c.hoja_celdas.push(fila);
            }
        }
        "input" => {
            if let Some(g) = cmd.as_mut() {
                g.entradas = io_attrs(e)?;
            }
        }
        "output" => {
            if let Some(g) = cmd.as_mut() {
                g.salidas = io_attrs(e)?;
            }
        }
        "ggbscript" if *en_construccion => c.con_script = true,
        "cascell" if *en_construccion => {
            c.con_cas = true;
            if !autocerrado {
                *prof_cas = prof_cas.saturating_add(1);
            }
        }
        _ => {}
    }
    Ok(())
}
fn parse_coefficients_data(data: &str) -> Option<[f64; 6]> {
    let trimmed = data
        .trim()
        .trim_start_matches('[')
        .trim_end_matches(']')
        .trim();
    if trimmed.is_empty() {
        return None;
    }
    let mut vals: Vec<f64> = Vec::new();
    for tok in trimmed.split([',', ' ', ';']) {
        let t = tok.trim();
        if t.is_empty() {
            continue;
        }
        match t.parse::<f64>() {
            Ok(v) if v.is_finite() => vals.push(v),
            _ => return None,
        }
        if vals.len() >= 6 {
            break;
        }
    }
    if vals.len() < 6 {
        return None;
    }
    Some([vals[0], vals[1], vals[2], vals[3], vals[4], vals[5]])
}
fn attr(e: &BytesStart<'_>, clave: &str) -> Result<Option<String>, GgbError> {
    for resultado in e.attributes() {
        let a = resultado.map_err(|e| GgbError::XmlMalformado {
            detalle: GgbError::recorta(&e.to_string()),
        })?;
        if a.key.as_ref() == clave {
            if a.value.len() > MAX_ATTR_BYTES {
                return Err(GgbError::XmlMalformado {
                    detalle: "atributo sobredimensionado".to_string(),
                });
            }
            let v =
                a.normalized_value(XmlVersion::default())
                    .map_err(|e| GgbError::XmlMalformado {
                        detalle: GgbError::recorta(&e.to_string()),
                    })?;
            return Ok(Some(v.into_owned()));
        }
    }
    Ok(None)
}
fn num_attr(e: &BytesStart<'_>, clave: &str) -> Result<Option<f64>, GgbError> {
    let texto = match attr(e, clave)? {
        Some(t) => t,
        None => return Ok(None),
    };
    match texto.trim().parse::<f64>() {
        Ok(v) => Ok(Some(v)),
        Err(_) => Ok(None),
    }
}
fn io_attrs(e: &BytesStart<'_>) -> Result<Vec<String>, GgbError> {
    let mut pares: Vec<(u32, String)> = Vec::new();
    for resultado in e.attributes() {
        let a = resultado.map_err(|e| GgbError::XmlMalformado {
            detalle: GgbError::recorta(&e.to_string()),
        })?;
        let clave: &str = a.key.as_ref();
        let resto = match clave.strip_prefix('a') {
            Some(r) if !r.is_empty() => r,
            _ => continue,
        };
        let indice: u32 = match resto.parse() {
            Ok(i) => i,
            Err(_) => continue,
        };
        if a.value.len() > MAX_ATTR_BYTES {
            return Err(GgbError::XmlMalformado {
                detalle: "atributo sobredimensionado".to_string(),
            });
        }
        let v = a
            .normalized_value(XmlVersion::default())
            .map_err(|e| GgbError::XmlMalformado {
                detalle: GgbError::recorta(&e.to_string()),
            })?;
        pares.push((indice, v.into_owned()));
        if pares.len() > MAX_IO_ATTRS {
            return Err(GgbError::XmlMalformado {
                detalle: "demasiadas entradas/salidas en un comando".to_string(),
            });
        }
    }
    pares.sort_by_key(|(i, _)| *i);
    Ok(pares.into_iter().map(|(_, v)| v).collect())
}
fn rechazar_doctype(xml: &[u8]) -> Result<(), GgbError> {
    // Defensa en profundidad junto a `zip_read::extraer`: `quick-xml` 0.42 con
    // `default-features = false` no expande DTD ni entidades externas (el
    // lector solo emite `DocType` como evento, que este parser ignora), pero
    // un `<!DOCTYPE` explícito se rechaza fail-closed sin llegar a parsear.
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
