//! Parseo en streaming de `geogebra.xml` con `quick-xml`.
use crate::error::GgbError;
use crate::model::{Construccion, GgbComando, GgbElemento, GgbExpresion, ItemOrden};
use crate::{MAX_ATTR_BYTES, MAX_ELEMS, MAX_XML_ATTRS_PER_ELEMENT, MAX_XML_DEPTH};
use quick_xml::events::{BytesStart, Event};
use quick_xml::Reader;
use quick_xml::XmlVersion;
/// Tope de atributos `a*` por `<input>`/`<output>` (anti-quadratic-blowup).
/// Lo usa también el exportador como tope de vértices de `Polygon`: sin
/// paridad, el export emitiría XML que el importador rechaza y rompería la
/// regla de oro "todo lo que exporta debe re-importar".
pub(crate) const MAX_IO_ATTRS: usize = 64;
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
/// Error de sintaxis de atributo → `XmlMalformado` (misma forma que antes).
fn err_attr(e: impl std::fmt::Display) -> GgbError {
    GgbError::XmlMalformado {
        detalle: GgbError::recorta(&e.to_string()),
    }
}
/// Cota anti-quadratic-blowup para tags sin extracción (construction,
/// ggbscript, cascell, desconocidos y contenido CAS salteado): una sola pasada
/// que valida sintaxis y cuenta, sin normalizar valores.
fn verificar_tope(e: &BytesStart<'_>, nombre: &str) -> Result<(), GgbError> {
    let mut n: usize = 0;
    for resultado in e.attributes() {
        resultado.map_err(err_attr)?;
        n = n.saturating_add(1);
        if n > MAX_XML_ATTRS_PER_ELEMENT {
            return Err(GgbError::XmlMalformado {
                detalle: format!(
                    "demasiados atributos en <{nombre}: límite {MAX_XML_ATTRS_PER_ELEMENT}"
                ),
            });
        }
    }
    Ok(())
}
/// Normaliza un valor de atributo (des-escapa entidades) con el mismo error
/// que `attr` original.
fn normalizar(a: &quick_xml::events::attributes::Attribute<'_>) -> Result<String, GgbError> {
    let v = a
        .normalized_value(XmlVersion::default())
        .map_err(err_attr)?;
    Ok(v.into_owned())
}
/// Parsea un numérico desde el atributo ya matcheado: fast-path sin alloc
/// cuando el crudo no trae `&` (99.9%: `x="1.5"`); fallback a normalizado
/// (mismo error que antes) si hay entidades o UTF-8 inválido. Fallo de parse
/// → `None`, igual que `num_attr` original.
fn numero_desde_attr(
    a: &quick_xml::events::attributes::Attribute<'_>,
) -> Result<Option<f64>, GgbError> {
    if a.value.len() > MAX_ATTR_BYTES {
        return Err(GgbError::XmlMalformado {
            detalle: "atributo sobredimensionado".to_string(),
        });
    }
    // Fast-path sin alloc: `value` ya es `&str` prestado del buffer; si parsea
    // directo no hay nada que normalizar (el 99.9%: `x="1.5"`). Sin `&`, el
    // normalizado es idéntico al crudo, así que un fallo de parse es `None`
    // sin llamar a `normalized_value`. Con `&` (`&#49;`) se cae al normalizado
    // para preservar la semántica original exacta.
    if let Ok(v) = a.value.trim().parse::<f64>() {
        return Ok(Some(v));
    }
    if !a.value.contains('&') {
        return Ok(None);
    }
    let v = normalizar(a)?;
    match v.trim().parse::<f64>() {
        Ok(n) => Ok(Some(n)),
        Err(_) => Ok(None),
    }
}
/// Extrae un string opcional en la misma pasada de conteo. Solo normaliza la
/// clave pedida; las demás solo cuentan (misma semántica que `attr`: el tope
/// de tamaño solo aplica a la clave buscada).
fn extraer_str1(e: &BytesStart<'_>, nombre: &str, clave: &str) -> Result<Option<String>, GgbError> {
    let mut n: usize = 0;
    let mut fuera: Option<String> = None;
    for resultado in e.attributes() {
        let a = resultado.map_err(err_attr)?;
        n = n.saturating_add(1);
        if n > MAX_XML_ATTRS_PER_ELEMENT {
            return Err(GgbError::XmlMalformado {
                detalle: format!(
                    "demasiados atributos en <{nombre}: límite {MAX_XML_ATTRS_PER_ELEMENT}"
                ),
            });
        }
        if a.key.as_ref() == clave && fuera.is_none() {
            if a.value.len() > MAX_ATTR_BYTES {
                return Err(GgbError::XmlMalformado {
                    detalle: "atributo sobredimensionado".to_string(),
                });
            }
            fuera = Some(normalizar(&a)?);
        }
    }
    Ok(fuera)
}
/// `type`+`label` de `<element>` en una sola pasada (antes: conteo + 2×`attr`).
fn extraer_element_attrs(e: &BytesStart<'_>, nombre: &str) -> Result<(String, String), GgbError> {
    let mut n: usize = 0;
    let mut tipo: Option<String> = None;
    let mut etiqueta: Option<String> = None;
    for resultado in e.attributes() {
        let a = resultado.map_err(err_attr)?;
        n = n.saturating_add(1);
        if n > MAX_XML_ATTRS_PER_ELEMENT {
            return Err(GgbError::XmlMalformado {
                detalle: format!(
                    "demasiados atributos en <{nombre}: límite {MAX_XML_ATTRS_PER_ELEMENT}"
                ),
            });
        }
        let k: &str = a.key.as_ref();
        if k == "type" && tipo.is_none() {
            if a.value.len() > MAX_ATTR_BYTES {
                return Err(GgbError::XmlMalformado {
                    detalle: "atributo sobredimensionado".to_string(),
                });
            }
            tipo = Some(normalizar(&a)?);
        } else if k == "label" && etiqueta.is_none() {
            if a.value.len() > MAX_ATTR_BYTES {
                return Err(GgbError::XmlMalformado {
                    detalle: "atributo sobredimensionado".to_string(),
                });
            }
            etiqueta = Some(normalizar(&a)?);
        }
    }
    Ok((tipo.unwrap_or_default(), etiqueta.unwrap_or_default()))
}
/// `label`+`exp`+`type` de `<expression>` en una sola pasada (antes: 1+3).
fn extraer_expression_attrs(
    e: &BytesStart<'_>,
    nombre: &str,
) -> Result<(String, String, String), GgbError> {
    let mut n: usize = 0;
    let mut etiqueta: Option<String> = None;
    let mut exp: Option<String> = None;
    let mut tipo: Option<String> = None;
    for resultado in e.attributes() {
        let a = resultado.map_err(err_attr)?;
        n = n.saturating_add(1);
        if n > MAX_XML_ATTRS_PER_ELEMENT {
            return Err(GgbError::XmlMalformado {
                detalle: format!(
                    "demasiados atributos en <{nombre}: límite {MAX_XML_ATTRS_PER_ELEMENT}"
                ),
            });
        }
        let k: &str = a.key.as_ref();
        if k == "label" && etiqueta.is_none() {
            if a.value.len() > MAX_ATTR_BYTES {
                return Err(GgbError::XmlMalformado {
                    detalle: "atributo sobredimensionado".to_string(),
                });
            }
            etiqueta = Some(normalizar(&a)?);
        } else if k == "exp" && exp.is_none() {
            if a.value.len() > MAX_ATTR_BYTES {
                return Err(GgbError::XmlMalformado {
                    detalle: "atributo sobredimensionado".to_string(),
                });
            }
            exp = Some(normalizar(&a)?);
        } else if k == "type" && tipo.is_none() {
            if a.value.len() > MAX_ATTR_BYTES {
                return Err(GgbError::XmlMalformado {
                    detalle: "atributo sobredimensionado".to_string(),
                });
            }
            tipo = Some(normalizar(&a)?);
        }
    }
    Ok((
        etiqueta.unwrap_or_default(),
        exp.unwrap_or_default(),
        tipo.unwrap_or_default(),
    ))
}
/// `x/y/z/w` de `<coords>` en una sola pasada (antes: 1+4 con 4 Strings).
#[allow(clippy::type_complexity)]
fn extraer_coords(
    e: &BytesStart<'_>,
    nombre: &str,
) -> Result<(Option<f64>, Option<f64>, Option<f64>, Option<f64>), GgbError> {
    let mut n: usize = 0;
    let mut x: Option<f64> = None;
    let mut y: Option<f64> = None;
    let mut z: Option<f64> = None;
    let mut w: Option<f64> = None;
    for resultado in e.attributes() {
        let a = resultado.map_err(err_attr)?;
        n = n.saturating_add(1);
        if n > MAX_XML_ATTRS_PER_ELEMENT {
            return Err(GgbError::XmlMalformado {
                detalle: format!(
                    "demasiados atributos en <{nombre}: límite {MAX_XML_ATTRS_PER_ELEMENT}"
                ),
            });
        }
        let k: &str = a.key.as_ref();
        if k == "x" && x.is_none() {
            x = numero_desde_attr(&a)?;
        } else if k == "y" && y.is_none() {
            y = numero_desde_attr(&a)?;
        } else if k == "z" && z.is_none() {
            z = numero_desde_attr(&a)?;
        } else if k == "w" && w.is_none() {
            w = numero_desde_attr(&a)?;
        }
    }
    Ok((x, y, z, w))
}
/// `min/max` de `<slider>` en una pasada.
fn extraer_min_max(
    e: &BytesStart<'_>,
    nombre: &str,
) -> Result<(Option<f64>, Option<f64>), GgbError> {
    let mut n: usize = 0;
    let mut min: Option<f64> = None;
    let mut max: Option<f64> = None;
    for resultado in e.attributes() {
        let a = resultado.map_err(err_attr)?;
        n = n.saturating_add(1);
        if n > MAX_XML_ATTRS_PER_ELEMENT {
            return Err(GgbError::XmlMalformado {
                detalle: format!(
                    "demasiados atributos en <{nombre}: límite {MAX_XML_ATTRS_PER_ELEMENT}"
                ),
            });
        }
        let k: &str = a.key.as_ref();
        if k == "min" && min.is_none() {
            min = numero_desde_attr(&a)?;
        } else if k == "max" && max.is_none() {
            max = numero_desde_attr(&a)?;
        }
    }
    Ok((min, max))
}
/// `A0..A5` de `<matrix>` en una pasada (antes: 1+6).
fn extraer_matrix(e: &BytesStart<'_>, nombre: &str) -> Result<[Option<f64>; 6], GgbError> {
    let mut n: usize = 0;
    let mut fuera: [Option<f64>; 6] = [None, None, None, None, None, None];
    for resultado in e.attributes() {
        let a = resultado.map_err(err_attr)?;
        n = n.saturating_add(1);
        if n > MAX_XML_ATTRS_PER_ELEMENT {
            return Err(GgbError::XmlMalformado {
                detalle: format!(
                    "demasiados atributos en <{nombre}: límite {MAX_XML_ATTRS_PER_ELEMENT}"
                ),
            });
        }
        let k: &str = a.key.as_ref();
        let idx = if k == "A0" {
            Some(0)
        } else if k == "A1" {
            Some(1)
        } else if k == "A2" {
            Some(2)
        } else if k == "A3" {
            Some(3)
        } else if k == "A4" {
            Some(4)
        } else if k == "A5" {
            Some(5)
        } else {
            None
        };
        if let Some(i) = idx {
            if fuera[i].is_none() {
                fuera[i] = numero_desde_attr(&a)?;
            }
        }
    }
    Ok(fuera)
}
/// `x0/y0/x1/y1` de `<eigenvectors>` en una pasada.
fn extraer_eigen(e: &BytesStart<'_>, nombre: &str) -> Result<[Option<f64>; 4], GgbError> {
    let mut n: usize = 0;
    let mut fuera: [Option<f64>; 4] = [None, None, None, None];
    for resultado in e.attributes() {
        let a = resultado.map_err(err_attr)?;
        n = n.saturating_add(1);
        if n > MAX_XML_ATTRS_PER_ELEMENT {
            return Err(GgbError::XmlMalformado {
                detalle: format!(
                    "demasiados atributos en <{nombre}: límite {MAX_XML_ATTRS_PER_ELEMENT}"
                ),
            });
        }
        let k: &str = a.key.as_ref();
        let idx = if k == "x0" {
            Some(0)
        } else if k == "y0" {
            Some(1)
        } else if k == "x1" {
            Some(2)
        } else if k == "y1" {
            Some(3)
        } else {
            None
        };
        if let Some(i) = idx {
            if fuera[i].is_none() {
                fuera[i] = numero_desde_attr(&a)?;
            }
        }
    }
    Ok(fuera)
}
/// Un numérico suelto (`<value val>`) en una pasada con fast-path sin alloc.
fn extraer_num1(e: &BytesStart<'_>, nombre: &str, clave: &str) -> Result<Option<f64>, GgbError> {
    let mut n: usize = 0;
    let mut fuera: Option<f64> = None;
    let mut visto = false;
    for resultado in e.attributes() {
        let a = resultado.map_err(err_attr)?;
        n = n.saturating_add(1);
        if n > MAX_XML_ATTRS_PER_ELEMENT {
            return Err(GgbError::XmlMalformado {
                detalle: format!(
                    "demasiados atributos en <{nombre}: límite {MAX_XML_ATTRS_PER_ELEMENT}"
                ),
            });
        }
        if a.key.as_ref() == clave && !visto {
            visto = true;
            fuera = numero_desde_attr(&a)?;
        }
    }
    Ok(fuera)
}
/// `x/y/exp` de `<startPoint>` en una pasada. `exp` solo se normaliza si
/// viene (antes se pedía solo cuando x/y faltaban; normalizarlo siempre que
/// está es el mismo valor ignorado, sin cambio observable).
#[allow(clippy::type_complexity)]
fn extraer_startpoint(
    e: &BytesStart<'_>,
    nombre: &str,
) -> Result<(Option<f64>, Option<f64>, Option<String>), GgbError> {
    let mut n: usize = 0;
    let mut x: Option<f64> = None;
    let mut y: Option<f64> = None;
    let mut exp: Option<String> = None;
    for resultado in e.attributes() {
        let a = resultado.map_err(err_attr)?;
        n = n.saturating_add(1);
        if n > MAX_XML_ATTRS_PER_ELEMENT {
            return Err(GgbError::XmlMalformado {
                detalle: format!(
                    "demasiados atributos en <{nombre}: límite {MAX_XML_ATTRS_PER_ELEMENT}"
                ),
            });
        }
        let k: &str = a.key.as_ref();
        if k == "x" && x.is_none() {
            x = numero_desde_attr(&a)?;
        } else if k == "y" && y.is_none() {
            y = numero_desde_attr(&a)?;
        } else if k == "exp" && exp.is_none() {
            if a.value.len() > MAX_ATTR_BYTES {
                return Err(GgbError::XmlMalformado {
                    detalle: "atributo sobredimensionado".to_string(),
                });
            }
            exp = Some(normalizar(&a)?);
        }
    }
    Ok((x, y, exp))
}
/// `<cell>`: 5 strings + num `val` en una pasada (antes: 1+6 pasadas).
/// Preserva el orden de claves original (`val,value,content,exp,input`) y
/// "primera ocurrencia gana" por clave, igual que `attr` repetido.
/// `val` aporta doble como antes: string no vacío y, si parsea, `format!(n)`.
fn extraer_cell(e: &BytesStart<'_>, nombre: &str) -> Result<Vec<String>, GgbError> {
    let mut n: usize = 0;
    let mut ranuras: [Option<String>; 5] = [None, None, None, None, None];
    let mut val_num: Option<f64> = None;
    let mut val_visto = false;
    for resultado in e.attributes() {
        let a = resultado.map_err(err_attr)?;
        n = n.saturating_add(1);
        if n > MAX_XML_ATTRS_PER_ELEMENT {
            return Err(GgbError::XmlMalformado {
                detalle: format!(
                    "demasiados atributos en <{nombre}: límite {MAX_XML_ATTRS_PER_ELEMENT}"
                ),
            });
        }
        let k: &str = a.key.as_ref();
        let idx = if k == "val" {
            Some(0)
        } else if k == "value" {
            Some(1)
        } else if k == "content" {
            Some(2)
        } else if k == "exp" {
            Some(3)
        } else if k == "input" {
            Some(4)
        } else {
            None
        };
        if let Some(i) = idx {
            if ranuras[i].is_some() {
                continue; // primera ocurrencia gana, como `attr`
            }
            if a.value.len() > MAX_ATTR_BYTES {
                return Err(GgbError::XmlMalformado {
                    detalle: "atributo sobredimensionado".to_string(),
                });
            }
            let v = normalizar(&a)?;
            if i == 0 && !val_visto {
                val_visto = true;
                if let Ok(num) = v.trim().parse::<f64>() {
                    val_num = Some(num);
                }
            }
            ranuras[i] = Some(v);
        }
    }
    let mut fila: Vec<String> = Vec::new();
    for slot in ranuras.into_iter().flatten() {
        if !slot.is_empty() && slot.len() <= super::MAX_ATTR_BYTES {
            fila.push(slot);
        }
    }
    if let Some(num) = val_num {
        fila.push(format!("{num}"));
    }
    Ok(fila)
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
    // Contenido CAS salteado: igual se aplica la cota anti-quadratic-blowup
    // (antes el conteo corría antes del early-return; se preserva con una
    // pasada barata sin normalizar).
    if *prof_cas > 0 {
        return verificar_tope(e, nombre);
    }
    match nombre {
        "construction" => {
            verificar_tope(e, nombre)?;
            *en_construccion = true;
        }
        "element" if *en_construccion => {
            contar(conteo)?;
            if let Some(mut g) = elem.take() {
                g.es_celda_hoja = es_celda_hoja(&g.etiqueta);
                c.orden.push(ItemOrden::Elemento(c.elementos.len()));
                c.elementos.push(g);
            }
            let (tipo, etiqueta) = extraer_element_attrs(e, nombre)?;
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
            let nombre_cmd = extraer_str1(e, nombre, "name")?.unwrap_or_default();
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
            let (etiqueta, exp, tipo) = extraer_expression_attrs(e, nombre)?;
            c.expresiones.push(GgbExpresion {
                etiqueta,
                exp,
                tipo,
            });
        }
        "coords" => {
            if let Some(g) = elem.as_mut() {
                let (x, y, z, w) = extraer_coords(e, nombre)?;
                if let (Some(x), Some(y)) = (x, y) {
                    let z = z.unwrap_or(1.0);
                    let w = w.unwrap_or(1.0);
                    g.coords = Some([x, y, z, w]);
                }
            }
        }
        "value" => {
            if let Some(g) = elem.as_mut() {
                g.valor = extraer_num1(e, nombre, "val")?;
            }
        }
        "slider" => {
            if let Some(g) = elem.as_mut() {
                let (min, max) = extraer_min_max(e, nombre)?;
                if let (Some(min), Some(max)) = (min, max) {
                    g.deslizador = Some((min, max));
                }
            }
        }
        "matrix" => {
            if let Some(g) = elem.as_mut() {
                let m = extraer_matrix(e, nombre)?;
                if let [Some(a0), Some(a1), Some(a2), Some(a3), Some(a4), Some(a5)] = m {
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
                let ev = extraer_eigen(e, nombre)?;
                if let [Some(x0), Some(y0), Some(x1), Some(y1)] = ev {
                    if x0.is_finite() && y0.is_finite() && x1.is_finite() && y1.is_finite() {
                        g.eigen = Some([x0, y0, x1, y1]);
                    }
                }
            }
        }
        "coefficients" => {
            if let Some(g) = elem.as_mut() {
                if let Some(data) = extraer_str1(e, nombre, "data")? {
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
                let (x, y, exp) = extraer_startpoint(e, nombre)?;
                if let (Some(x), Some(y)) = (x, y) {
                    if x.is_finite() && y.is_finite() {
                        g.vector_start = Some([x, y]);
                    }
                } else if let Some(exp) = exp {
                    let _ = exp;
                }
            }
        }
        "caption" => {
            if let Some(g) = elem.as_mut() {
                if let Some(val) = extraer_str1(e, nombre, "val")? {
                    if val.len() <= MAX_ATTR_BYTES {
                        g.texto = Some(val);
                    }
                }
            }
        }
        "cell" => {
            let fila = extraer_cell(e, nombre)?;
            if !fila.is_empty() && c.hoja_celdas.len() < crate::MAX_DATA_TABLE_ROWS {
                c.hoja_celdas.push(fila);
            }
        }
        "input" => {
            if let Some(g) = cmd.as_mut() {
                g.entradas = io_attrs(e, nombre)?;
            }
        }
        "output" => {
            if let Some(g) = cmd.as_mut() {
                g.salidas = io_attrs(e, nombre)?;
            }
        }
        "ggbscript" if *en_construccion => {
            verificar_tope(e, nombre)?;
            c.con_script = true;
        }
        "cascell" if *en_construccion => {
            verificar_tope(e, nombre)?;
            c.con_cas = true;
            if !autocerrado {
                *prof_cas = prof_cas.saturating_add(1);
            }
        }
        _ => {
            verificar_tope(e, nombre)?;
        }
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
    let mut vals = [0.0f64; 6];
    let mut n = 0usize;
    for tok in trimmed.split([',', ' ', ';']) {
        let t = tok.trim();
        if t.is_empty() {
            continue;
        }
        match t.parse::<f64>() {
            Ok(v) if v.is_finite() => {
                vals[n] = v;
                n += 1;
            }
            _ => return None,
        }
        if n >= 6 {
            break;
        }
    }
    if n < 6 {
        return None;
    }
    Some(vals)
}
fn io_attrs(e: &BytesStart<'_>, nombre: &str) -> Result<Vec<String>, GgbError> {
    let mut pares: Vec<(u32, String)> = Vec::new();
    let mut total: usize = 0;
    for resultado in e.attributes() {
        let a = resultado.map_err(err_attr)?;
        total = total.saturating_add(1);
        if total > MAX_XML_ATTRS_PER_ELEMENT {
            return Err(GgbError::XmlMalformado {
                detalle: format!(
                    "demasiados atributos en <{nombre}: límite {MAX_XML_ATTRS_PER_ELEMENT}"
                ),
            });
        }
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
            .map_err(err_attr)?;
        pares.push((indice, v.into_owned()));
        if pares.len() > MAX_IO_ATTRS {
            return Err(GgbError::XmlMalformado {
                detalle: "demasiadas entradas/salidas en un comando".to_string(),
            });
        }
    }
    // Los comandos bien formados ya vienen `a0,a1,…` en orden: evita el sort.
    let mut ordenado = true;
    for w in pares.windows(2) {
        if w[0].0 > w[1].0 {
            ordenado = false;
            break;
        }
    }
    if !ordenado {
        pares.sort_by_key(|(i, _)| *i);
    }
    Ok(pares.into_iter().map(|(_, v)| v).collect())
}
/// Fail-closed ante `<!DOCTYPE`/`<!ENTITY` en cualquier combinación de
/// mayúsculas (VULN 8: el filtro era case-sensitive y `<!doctype` pasaba).
/// La copia de `zip_read` usa esta misma función (defensa en profundidad).
pub(crate) fn rechazar_doctype(xml: &[u8]) -> Result<(), GgbError> {
    // Defensa en profundidad junto a `zip_read::extraer`: `quick-xml` 0.42 con
    // `default-features = false` no expande DTD ni entidades externas (el
    // lector solo emite `DocType` como evento, que este parser ignora), pero
    // un `<!DOCTYPE` explícito se rechaza fail-closed sin llegar a parsear.
    if contiene_ignorando_mayusculas(xml, b"<!DOCTYPE")
        || contiene_ignorando_mayusculas(xml, b"<!ENTITY")
    {
        return Err(GgbError::XmlMalformado {
            detalle: "DOCTYPE/ENTITY rechazado (bomba de entidades)".to_string(),
        });
    }
    Ok(())
}
fn contiene_ignorando_mayusculas(hay: &[u8], aguja: &[u8]) -> bool {
    if aguja.is_empty() || hay.len() < aguja.len() {
        return false;
    }
    // Fast-path: filtra por primer byte (`<`, sin variante de mayúsculas) y
    // solo compara el resto en las posiciones candidatas. El XML típico trae
    // miles de `<element` pero ningún `<!DOCTYPE`; pasar de O(n·m) con
    // `windows().any()` a O(n) con chequeo barato da ~10-15x en este filtro.
    let primer = aguja[0];
    let mut i = 0;
    let tope = hay.len() - aguja.len();
    while i <= tope {
        if hay[i].eq_ignore_ascii_case(&primer) {
            if hay[i..i + aguja.len()].eq_ignore_ascii_case(aguja) {
                return true;
            }
            i += 1;
        } else {
            i += 1;
        }
    }
    false
}
