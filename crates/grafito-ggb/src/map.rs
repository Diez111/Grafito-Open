#![allow(clippy::type_complexity)]
//! Mapeo de `Construccion` GeoGebra a comandos Grafito — F0+F1+F2+F3.
//! Cobre ~65% aula en F0/F1 (puntos, líneas, círculos, funciones, deslizadores)
//! y ~85% con F2/F3 (cónicas canónicas, polígonos/vector/angle, intersect, tabla).

use crate::model::{Construccion, GgbElemento, ItemOrden};
use crate::{
    ImportReport, MappedObject, OmittedObject, MAX_DATA_TABLE_ROWS, MAX_ELEMS, MAX_EXPR_CHARS,
};
use std::collections::{BTreeMap, HashMap};

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
fn fmt_point(x: f64, y: f64) -> String {
    format!("({}, {})", fmt_num(x), fmt_num(y))
}
fn sanitize_etiqueta(raw: &str) -> String {
    let t = raw.trim();
    if t.is_empty() {
        return String::new();
    }
    let mut out = String::new();
    for ch in t.chars() {
        if ch.is_alphanumeric() || ch == '_' || ch == '\'' {
            out.push(ch);
        } else if ch == ' ' || ch == '-' {
            out.push('_');
        }
        if out.len() >= 64 {
            break;
        }
    }
    out
}
fn is_3d(tipo: &str) -> bool {
    tipo.to_ascii_lowercase().ends_with("3d")
}
fn is_cas_o_script_marker(tipo: &str, etiqueta: &str) -> bool {
    let tl = tipo.to_ascii_lowercase();
    let el = etiqueta.to_ascii_lowercase();
    tl.contains("cas") || tl.contains("script") || el.contains("cas")
}
fn valida_expr(exp: &str) -> Result<String, String> {
    if exp.len() > MAX_EXPR_CHARS {
        return Err(format!("expresión excede {MAX_EXPR_CHARS} caracteres"));
    }
    if exp.trim().is_empty() {
        return Err("expresión vacía".to_string());
    }
    Ok(exp.trim().to_string())
}
const EPS: f64 = 1e-9;
fn is_identity_eigen(eigen: Option<[f64; 4]>) -> bool {
    let Some([x0, y0, x1, y1]) = eigen else {
        return true;
    };
    (x0 - 1.0).abs() < 1e-6 && y0.abs() < 1e-6 && x1.abs() < 1e-6 && (y1 - 1.0).abs() < 1e-6
}
#[allow(clippy::needless_return)]
fn mapear_conica(elem: &GgbElemento) -> Result<(String, String), String> {
    let m = elem.matrix.ok_or_else(|| "cónica sin matriz".to_string())?;
    let [a, b, c, d, e, f] = m;
    if !a.is_finite()
        || !b.is_finite()
        || !c.is_finite()
        || !d.is_finite()
        || !e.is_finite()
        || !f.is_finite()
    {
        return Err("cónica con coeficientes no finitos".to_string());
    }
    if !is_identity_eigen(elem.eigen) {
        return Err(
            "cónica rotada no canónica (requiere eigen identidad) — omitida honesta F2".to_string(),
        );
    }
    if b.abs() > 1e-6 {
        return Err("cónica con término cruzado b≠0 no canónica".to_string());
    }
    let disc = b * b - 4.0 * a * c;
    let _etiqueta = sanitize_etiqueta(&elem.etiqueta);
    if disc < -EPS {
        let (a_n, _b_n, c_n, d_n, e_n, f_n) = if a < 0.0 {
            (-a, -b, -c, -d, -e, -f)
        } else {
            (a, b, c, d, e, f)
        };
        if a_n <= EPS || c_n <= EPS {
            return Err("elipse con semiejes no positivos".to_string());
        }
        if a_n.abs() < EPS || c_n.abs() < EPS {
            return Err("elipse degenerada".to_string());
        }
        let h = -d_n / (2.0 * a_n);
        let k = -e_n / (2.0 * c_n);
        if !h.is_finite() || !k.is_finite() {
            return Err("elipse con centro no finito".to_string());
        }
        let f_prime = a_n * h * h + c_n * k * k + d_n * h + e_n * k + f_n;
        if !f_prime.is_finite() {
            return Err("elipse con f' no finito".to_string());
        }
        if f_prime >= -EPS {
            return Err("elipse degenerada o sin interior".to_string());
        }
        let rx2 = -f_prime / a_n;
        let ry2 = -f_prime / c_n;
        if rx2 <= EPS || ry2 <= EPS {
            return Err("elipse con radios no positivos".to_string());
        }
        let rx = rx2.sqrt();
        let ry = ry2.sqrt();
        if !rx.is_finite() || !ry.is_finite() || rx <= 0.0 || ry <= 0.0 {
            return Err("elipse con radios no finitos".to_string());
        }
        if rx > 1e6 || ry > 1e6 {
            return Err("elipse con radios desbordados".to_string());
        }
        let cmd = format!(
            "Ellipse[{}, {}, {}]",
            fmt_point(h, k),
            fmt_num(rx),
            fmt_num(ry)
        );
        return Ok(("Ellipse".to_string(), cmd));
    } else if disc.abs() <= EPS {
        if c.abs() <= EPS && a.abs() > EPS {
            if e.abs() <= EPS {
                return Err("parábola vertical con E=0".to_string());
            }
            let h = -d / (2.0 * a);
            if !h.is_finite() {
                return Err("parábola con vértice h no finito".to_string());
            }
            let k = -(a * h * h + d * h + f) / e;
            if !k.is_finite() {
                return Err("parábola con vértice k no finito".to_string());
            }
            let p = -e / (4.0 * a);
            if !p.is_finite() || p.abs() <= EPS {
                return Err("parábola con parámetro p no finito o nulo".to_string());
            }
            if p.abs() < 1e-12 {
                return Err("parábola con p singular".to_string());
            }
            let cmd = format!("Parabola[{}, {}]", fmt_point(h, k), fmt_num(p));
            return Ok(("Parabola".to_string(), cmd));
        } else if a.abs() <= EPS && c.abs() > EPS {
            if d.abs() <= EPS {
                return Err("parábola horizontal con D=0".to_string());
            }
            let k = -e / (2.0 * c);
            if !k.is_finite() {
                return Err("parábola horizontal con k no finito".to_string());
            }
            let h = -(c * k * k + e * k + f) / d;
            if !h.is_finite() {
                return Err("parábola horizontal con h no finito".to_string());
            }
            let p = -d / (4.0 * c);
            if !p.is_finite() || p.abs() <= EPS {
                return Err("parábola horizontal con p no finito".to_string());
            }
            let cmd = format!("Parabola[{}, {}]", fmt_point(h, k), fmt_num(p));
            return Ok(("Parabola".to_string(), cmd));
        } else {
            return Err("parábola no canónica (A y C ambos no nulos)".to_string());
        }
    } else {
        let (a_n, c_n, d_n, e_n, f_n) = (a, c, d, e, f);
        if a_n.abs() <= EPS || c_n.abs() <= EPS {
            return Err("hipérbola con A o C nulos".to_string());
        }
        if a_n * c_n >= 0.0 {
            return Err("hipérbola requiere A y C signos opuestos".to_string());
        }
        let h = -d_n / (2.0 * a_n);
        let k = -e_n / (2.0 * c_n);
        if !h.is_finite() || !k.is_finite() {
            return Err("hipérbola con centro no finito".to_string());
        }
        let f_prime = a_n * h * h + c_n * k * k + d_n * h + e_n * k + f_n;
        if !f_prime.is_finite() {
            return Err("hipérbola con f' no finito".to_string());
        }
        if f_prime.abs() <= EPS {
            return Err("hipérbola degenerada (f'≈0)".to_string());
        }
        let (a2, c2, f2) = if f_prime > 0.0 {
            (-a_n, -c_n, -f_prime)
        } else {
            (a_n, c_n, f_prime)
        };
        let (ah, bh): (f64, f64) = if a2 > 0.0 {
            let a2_pos = a2;
            let c2_neg = c2;
            let ah2 = -f2 / a2_pos;
            let bh2 = -f2 / (-c2_neg);
            (ah2.sqrt(), bh2.sqrt())
        } else {
            let a2_neg = a2;
            let c2_pos = c2;
            let ah2 = -f2 / c2_pos;
            let bh2 = -f2 / (-a2_neg);
            (ah2.sqrt(), bh2.sqrt())
        };
        if !ah.is_finite() || !bh.is_finite() || ah <= EPS || bh <= EPS || ah > 1e6 || bh > 1e6 {
            return Err("hipérbola con semiejes no válidos".to_string());
        }
        let cmd = format!(
            "Hyperbola[{}, {}, {}]",
            fmt_point(h, k),
            fmt_num(ah),
            fmt_num(bh)
        );
        return Ok(("Hyperbola".to_string(), cmd));
    }
}
fn polygon_area(vertices: &[(f64, f64)]) -> f64 {
    if vertices.len() < 3 {
        return 0.0;
    }
    let mut s = 0.0;
    for i in 0..vertices.len() {
        let (x1, y1) = vertices[i];
        let (x2, y2) = vertices[(i + 1) % vertices.len()];
        s += x1 * y2 - x2 * y1;
    }
    (s * 0.5).abs()
}
fn angle_at(b: (f64, f64), a: (f64, f64), c: (f64, f64)) -> Option<f64> {
    let v1x = a.0 - b.0;
    let v1y = a.1 - b.1;
    let v2x = c.0 - b.0;
    let v2y = c.1 - b.1;
    let n1 = (v1x * v1x + v1y * v1y).sqrt();
    let n2 = (v2x * v2x + v2y * v2y).sqrt();
    if n1 <= 1e-12 || n2 <= 1e-12 || !n1.is_finite() || !n2.is_finite() {
        return None;
    }
    let dot = v1x * v2x + v1y * v2y;
    let mut cos = dot / (n1 * n2);
    if !cos.is_finite() {
        return None;
    }
    cos = cos.clamp(-1.0, 1.0);
    let ang = cos.acos().to_degrees();
    if ang.is_finite() {
        Some(ang)
    } else {
        None
    }
}
fn line_intersection(
    p1: (f64, f64),
    p2: (f64, f64),
    p3: (f64, f64),
    p4: (f64, f64),
) -> Option<(f64, f64)> {
    let x1 = p1.0;
    let y1 = p1.1;
    let x2 = p2.0;
    let y2 = p2.1;
    let x3 = p3.0;
    let y3 = p3.1;
    let x4 = p4.0;
    let y4 = p4.1;
    let den = (x1 - x2) * (y3 - y4) - (y1 - y2) * (x3 - x4);
    if den.abs() < 1e-12 || !den.is_finite() {
        return None;
    }
    let t = ((x1 - x3) * (y3 - y4) - (y1 - y3) * (x3 - x4)) / den;
    let x = x1 + t * (x2 - x1);
    let y = y1 + t * (y2 - y1);
    if x.is_finite() && y.is_finite() {
        Some((x, y))
    } else {
        None
    }
}
fn is_spreadsheet_col_label(label: &str) -> bool {
    let b = label.as_bytes();
    if b.len() < 2 || b.len() > 5 {
        return false;
    }
    let mut i = 0;
    while i < b.len() && b[i].is_ascii_alphabetic() {
        i += 1;
    }
    if i == 0 || i > 2 {
        return false;
    }
    while i < b.len() {
        if !b[i].is_ascii_digit() {
            return false;
        }
        i += 1;
    }
    true
}
fn col_from_label(label: &str) -> Option<usize> {
    let mut col: usize = 0;
    let mut seen_letter = false;
    for ch in label.chars() {
        if ch.is_ascii_alphabetic() {
            seen_letter = true;
            let v = (ch.to_ascii_uppercase() as u8 - b'A') as usize;
            col = col.checked_mul(26)?.checked_add(v + 1)?;
        } else {
            break;
        }
    }
    if !seen_letter {
        return None;
    }
    Some(col - 1)
}
fn row_from_label(label: &str) -> Option<usize> {
    let digits: String = label.chars().filter(|c| c.is_ascii_digit()).collect();
    if digits.is_empty() {
        return None;
    }
    let n: usize = digits.parse().ok()?;
    if n == 0 {
        return None;
    }
    Some(n)
}
fn extraer_tabla_de_numericos(
    elementos: &[GgbElemento],
) -> Option<(Vec<f64>, Vec<f64>, String, String)> {
    let mut mapa: BTreeMap<(usize, usize), f64> = BTreeMap::new();
    let mut max_row: usize = 0;
    for el in elementos {
        if !el.tipo.eq_ignore_ascii_case("numeric") {
            continue;
        }
        if !el.es_celda_hoja && !is_spreadsheet_col_label(&el.etiqueta) {
            continue;
        }
        if let Some(v) = el.valor {
            if !v.is_finite() {
                continue;
            }
            let col = col_from_label(&el.etiqueta)?;
            let row = row_from_label(&el.etiqueta)?;
            if col > 25 || row > 100000 {
                continue;
            }
            max_row = max_row.max(row);
            if mapa.len() >= MAX_DATA_TABLE_ROWS * 2 {
                break;
            }
            mapa.insert((col, row), v);
        }
    }
    if mapa.is_empty() || max_row < 2 {
        return None;
    }
    let mut xs: Vec<f64> = Vec::new();
    let mut ys: Vec<f64> = Vec::new();
    let mut rows: BTreeMap<usize, (Option<f64>, Option<f64>)> = BTreeMap::new();
    for ((col, row), v) in mapa {
        let entry = rows.entry(row).or_insert((None, None));
        if col == 0 {
            entry.0 = Some(v);
        } else if col == 1 {
            entry.1 = Some(v);
        }
    }
    for (_row, (ox, oy)) in rows {
        if let (Some(x), Some(y)) = (ox, oy) {
            if !x.is_finite() || !y.is_finite() {
                continue;
            }
            xs.push(x);
            ys.push(y);
            if xs.len() > MAX_DATA_TABLE_ROWS {
                return None;
            }
        }
    }
    if xs.len() < 2 {
        return None;
    }
    if xs.len() != ys.len() {
        return None;
    }
    Some((xs, ys, "x".to_string(), "y".to_string()))
}
pub(crate) fn parse_csv_like_to_xy(csv: &[u8]) -> Option<(Vec<f64>, Vec<f64>)> {
    let text = std::str::from_utf8(csv).ok()?;
    if text.len() > 2_000_000 {
        return None;
    }
    let commas = text.matches(',').count();
    let tabs = text.matches('\t').count();
    let delim = if tabs > commas { '\t' } else { ',' };
    let mut rows: Vec<Vec<String>> = Vec::new();
    for line in text.lines() {
        if line.trim().is_empty() {
            continue;
        }
        let mut cells: Vec<String> = Vec::new();
        let mut cur = String::new();
        let mut in_q = false;
        let mut chars = line.chars().peekable();
        while let Some(ch) = chars.next() {
            if in_q {
                if ch == '"' {
                    if chars.peek() == Some(&'"') {
                        cur.push('"');
                        chars.next();
                    } else {
                        in_q = false;
                    }
                } else {
                    cur.push(ch);
                }
            } else if ch == '"' {
                if !cur.trim().is_empty() {
                    return None;
                }
                in_q = true;
            } else if ch == delim {
                cells.push(cur.trim().to_string());
                cur.clear();
            } else {
                cur.push(ch);
            }
        }
        if in_q {
            return None;
        }
        cells.push(cur.trim().to_string());
        if let Some(f) = cells.first_mut() {
            *f = f.trim_start_matches('\u{feff}').to_string();
        }
        if cells.len() != 2 {
            return None;
        }
        rows.push(cells);
        if rows.len() > MAX_DATA_TABLE_ROWS + 1 {
            return None;
        }
    }
    if rows.is_empty() {
        return None;
    }
    let first_vals = (
        rows[0][0].parse::<f64>().ok(),
        rows[0][1].parse::<f64>().ok(),
    );
    let (mut xs, mut ys): (Vec<f64>, Vec<f64>) = (Vec::new(), Vec::new());
    let start = match first_vals {
        (Some(x), Some(y)) if x.is_finite() && y.is_finite() => {
            xs.push(x);
            ys.push(y);
            1
        }
        _ => {
            if rows[0][0].parse::<f64>().is_ok() || rows[0][1].parse::<f64>().is_ok() {
                return None;
            }
            if rows[0][0].is_empty() || rows[0][1].is_empty() {
                return None;
            }
            1
        }
    };
    for row in rows.iter().skip(start) {
        let x: f64 = row[0].parse().ok()?;
        let y: f64 = row[1].parse().ok()?;
        if !x.is_finite() || !y.is_finite() {
            return None;
        }
        xs.push(x);
        ys.push(y);
        if xs.len() > MAX_DATA_TABLE_ROWS {
            return None;
        }
    }
    if xs.len() < 2 {
        return None;
    }
    Some((xs, ys))
}
pub(crate) fn mapear(construccion: &Construccion) -> ImportReport {
    let mut reporte = ImportReport {
        mapeados: 0,
        objetos: Vec::new(),
        tipos: BTreeMap::new(),
        omitidos: Vec::new(),
    };
    let mut puntos: HashMap<String, (f64, f64)> = HashMap::new();
    let mut lineas: HashMap<String, ((f64, f64), (f64, f64))> = HashMap::new();
    let mut circulos: HashMap<String, ((f64, f64), f64)> = HashMap::new();
    for el in &construccion.elementos {
        if el.tipo.eq_ignore_ascii_case("point") {
            if let Some([x, y, _, _]) = el.coords {
                if x.is_finite() && y.is_finite() && !el.etiqueta.trim().is_empty() {
                    puntos.insert(el.etiqueta.clone(), (x, y));
                }
            }
        }
    }
    let try_push_mapeado =
        |reporte: &mut ImportReport, etiqueta: String, tipo: String, comando: String| {
            if reporte.objetos.len() >= MAX_ELEMS {
                reporte.omitidos.push(OmittedObject {
                    tipo: tipo.clone(),
                    label: etiqueta.clone(),
                    razon: format!("presupuesto MAX_ELEMS {MAX_ELEMS} excedido"),
                });
                return;
            }
            *reporte.tipos.entry(tipo.clone()).or_insert(0) += 1;
            reporte.objetos.push(MappedObject {
                etiqueta,
                tipo,
                comando,
            });
            reporte.mapeados = reporte.objetos.len();
        };
    for item in &construccion.orden {
        match *item {
            ItemOrden::Elemento(idx) => {
                let Some(el) = construccion.elementos.get(idx) else {
                    continue;
                };
                let tipo_raw = el.tipo.trim().to_ascii_lowercase();
                let etiqueta = sanitize_etiqueta(&el.etiqueta);
                if is_3d(&el.tipo) {
                    reporte.omitidos.push(OmittedObject {
                        tipo: el.tipo.clone(),
                        label: etiqueta.clone(),
                        razon: "3D omitido en núcleo aula F2 — requiere vista 3D".to_string(),
                    });
                    continue;
                }
                if is_cas_o_script_marker(&el.tipo, &el.etiqueta) {
                    reporte.omitidos.push(OmittedObject {
                        tipo: el.tipo.clone(),
                        label: etiqueta,
                        razon: "CAS/script omitido en núcleo aula".to_string(),
                    });
                    continue;
                }
                if el.es_celda_hoja && el.tipo.eq_ignore_ascii_case("numeric") {
                    continue;
                }
                match tipo_raw.as_str() {
                    "point" => {
                        if let Some([x, y, _, _]) = el.coords {
                            if !x.is_finite() || !y.is_finite() {
                                reporte.omitidos.push(OmittedObject {
                                    tipo: el.tipo.clone(),
                                    label: etiqueta,
                                    razon: "coordenadas no finitas".to_string(),
                                });
                                continue;
                            }
                            let cmd = format!("Point[{}]", fmt_point(x, y));
                            puntos.entry(el.etiqueta.clone()).or_insert((x, y));
                            try_push_mapeado(&mut reporte, etiqueta, "Point".to_string(), cmd);
                        } else {
                            reporte.omitidos.push(OmittedObject {
                                tipo: el.tipo.clone(),
                                label: etiqueta,
                                razon: "point sin coords".to_string(),
                            });
                        }
                    }
                    "numeric" => {
                        if let Some((min, max)) = el.deslizador {
                            if !min.is_finite() || !max.is_finite() || min >= max {
                                reporte.omitidos.push(OmittedObject {
                                    tipo: el.tipo.clone(),
                                    label: etiqueta.clone(),
                                    razon: "slider con rango inválido".to_string(),
                                });
                                continue;
                            }
                            let val = el.valor.unwrap_or((min + max) * 0.5);
                            if !val.is_finite() {
                                reporte.omitidos.push(OmittedObject {
                                    tipo: el.tipo.clone(),
                                    label: etiqueta,
                                    razon: "slider valor no finito".to_string(),
                                });
                                continue;
                            }
                            let paso = ((max - min) / 100.0).abs();
                            let paso = if paso < 1e-9 || !paso.is_finite() {
                                0.1
                            } else {
                                paso
                            };
                            let cmd = format!(
                                "Slider[{}, {}, {}, {}]",
                                if etiqueta.is_empty() {
                                    "a".to_string()
                                } else {
                                    etiqueta.clone()
                                },
                                fmt_num(min),
                                fmt_num(max),
                                fmt_num(paso)
                            );
                            if cmd.len() > MAX_EXPR_CHARS {
                                reporte.omitidos.push(OmittedObject {
                                    tipo: el.tipo.clone(),
                                    label: etiqueta,
                                    razon: "comando excede MAX_EXPR_CHARS".to_string(),
                                });
                                continue;
                            }
                            try_push_mapeado(&mut reporte, etiqueta, "Slider".to_string(), cmd);
                        } else if let Some(v) = el.valor {
                            if !v.is_finite() {
                                reporte.omitidos.push(OmittedObject {
                                    tipo: el.tipo.clone(),
                                    label: etiqueta,
                                    razon: "numeric no finito".to_string(),
                                });
                                continue;
                            }
                            reporte.omitidos.push(OmittedObject { tipo: el.tipo.clone(), label: etiqueta, razon: "numeric sin slider ni celda de hoja — pendiente mapeo variable (TODO F2)".to_string() });
                        } else {
                            reporte.omitidos.push(OmittedObject {
                                tipo: el.tipo.clone(),
                                label: etiqueta,
                                razon: "numeric sin valor".to_string(),
                            });
                        }
                    }
                    "conic" | "conicpart" => match mapear_conica(el) {
                        Ok((tipo, cmd)) => {
                            let et = etiqueta.clone();
                            try_push_mapeado(&mut reporte, et, tipo, cmd);
                        }
                        Err(razon) => {
                            reporte.omitidos.push(OmittedObject {
                                tipo: el.tipo.clone(),
                                label: etiqueta,
                                razon,
                            });
                        }
                    },
                    "polygon" | "polygon3d" => {
                        if tipo_raw == "polygon3d" {
                            reporte.omitidos.push(OmittedObject {
                                tipo: el.tipo.clone(),
                                label: etiqueta,
                                razon: "polígono 3D omitido".to_string(),
                            });
                        } else {
                            reporte.omitidos.push(OmittedObject { tipo: el.tipo.clone(), label: etiqueta, razon: "polygon elemento sin comando — requiere lista de vértices (usar comando Polygon)".to_string() });
                        }
                    }
                    "vector" => {
                        if let (Some([sx, sy]), Some([ex, ey, _, _])) = (el.vector_start, el.coords)
                        {
                            if !sx.is_finite()
                                || !sy.is_finite()
                                || !ex.is_finite()
                                || !ey.is_finite()
                            {
                                reporte.omitidos.push(OmittedObject {
                                    tipo: el.tipo.clone(),
                                    label: etiqueta,
                                    razon: "vector con coords no finitas".to_string(),
                                });
                                continue;
                            }
                            let cmd =
                                format!("Vector[{}, {}]", fmt_point(sx, sy), fmt_point(ex, ey));
                            lineas.insert(el.etiqueta.clone(), ((sx, sy), (ex, ey)));
                            try_push_mapeado(
                                &mut reporte,
                                etiqueta.clone(),
                                "Vector".to_string(),
                                cmd,
                            );
                            let len = ((ex - sx).powi(2) + (ey - sy).powi(2)).sqrt();
                            if len.is_finite() {
                                reporte.omitidos.push(OmittedObject { tipo: "Text".to_string(), label: format!("{etiqueta}_medida"), razon: format!("medida vector len {} — Text genérico sin comando estable (TODO honesto F2)", fmt_num(len)) });
                            }
                        } else {
                            reporte.omitidos.push(OmittedObject {
                                tipo: el.tipo.clone(),
                                label: etiqueta,
                                razon: "vector sin startPoint o coords".to_string(),
                            });
                        }
                    }
                    "angle" => {
                        if let Some(v) = el.valor {
                            if !v.is_finite() {
                                reporte.omitidos.push(OmittedObject {
                                    tipo: el.tipo.clone(),
                                    label: etiqueta,
                                    razon: "angle valor no finito".to_string(),
                                });
                                continue;
                            }
                            reporte.omitidos.push(OmittedObject { tipo: el.tipo.clone(), label: etiqueta.clone(), razon: format!("angle {v}° — requiere 3 puntos, mapeado como Polygon+Text medida pendiente (texto sin comando)") });
                        } else {
                            reporte.omitidos.push(OmittedObject {
                                tipo: el.tipo.clone(),
                                label: etiqueta,
                                razon: "angle sin valor".to_string(),
                            });
                        }
                    }
                    "segment" | "line" | "ray" => {
                        reporte.omitidos.push(OmittedObject {
                            tipo: el.tipo.clone(),
                            label: etiqueta,
                            razon: format!(
                                "{} elemento sin comando — omitido (usar comando {} explícito)",
                                el.tipo, el.tipo
                            ),
                        });
                    }
                    "circle" => {
                        if let (Some([cx, cy, _, _]), Some(r)) = (el.coords, el.valor) {
                            if !cx.is_finite() || !cy.is_finite() || !r.is_finite() || r <= 1e-12 {
                                reporte.omitidos.push(OmittedObject {
                                    tipo: el.tipo.clone(),
                                    label: etiqueta,
                                    razon: "círculo con centro o radio no válido".to_string(),
                                });
                                continue;
                            }
                            let cmd = format!("Circle[{}, {}]", fmt_point(cx, cy), fmt_num(r));
                            circulos.insert(el.etiqueta.clone(), ((cx, cy), r));
                            try_push_mapeado(&mut reporte, etiqueta, "Circle".to_string(), cmd);
                        } else {
                            reporte.omitidos.push(OmittedObject {
                                tipo: el.tipo.clone(),
                                label: etiqueta,
                                razon: "círculo sin coords+valor".to_string(),
                            });
                        }
                    }
                    "text" | "textfield" | "button" | "image" | "penstroke" | "locus" | "list" => {
                        if el.texto.is_some() {
                            reporte.omitidos.push(OmittedObject {
                                tipo: el.tipo.clone(),
                                label: etiqueta,
                                razon:
                                    "Text genérico — no hay comando Text estable (TODO honesto F2)"
                                        .to_string(),
                            });
                        } else {
                            let razon = match tipo_raw.as_str() {
                                "button" => {
                                    "button omitido (interactividad no soportada en núcleo aula)"
                                }
                                "image" => "image omitido (recurso externo no soportado)",
                                "penstroke" => "trazo a mano alzada omitido (requiere pizarra)",
                                "locus" => "locus omitido (dependencia dinámica)",
                                "list" => "list omitido (colección no mapeada en F2)",
                                _ => "texto/objeto decorativo omitido (TODO honesto)",
                            };
                            reporte.omitidos.push(OmittedObject {
                                tipo: el.tipo.clone(),
                                label: etiqueta,
                                razon: razon.to_string(),
                            });
                        }
                    }
                    other
                        if other.contains("3d")
                            || other.contains("quadric")
                            || other.contains("plane")
                            || other.contains("sphere") =>
                    {
                        reporte.omitidos.push(OmittedObject {
                            tipo: el.tipo.clone(),
                            label: etiqueta,
                            razon: "3D/quadric omitido en núcleo aula F2".to_string(),
                        });
                    }
                    other if other == "functionnvar" || other.starts_with("function") => {
                        reporte.omitidos.push(OmittedObject {
                            tipo: el.tipo.clone(),
                            label: etiqueta,
                            razon: "function como elemento — usar expression tipo function"
                                .to_string(),
                        });
                    }
                    _ => {
                        let lower = tipo_raw.to_ascii_lowercase();
                        if lower.contains("slider")
                            || lower.contains("checkbox")
                            || lower.contains("inputbox")
                        {
                            reporte.omitidos.push(OmittedObject {
                                tipo: el.tipo.clone(),
                                label: etiqueta,
                                razon: "control UI omitido en núcleo aula".to_string(),
                            });
                        } else {
                            reporte.omitidos.push(OmittedObject {
                                tipo: el.tipo.clone(),
                                label: etiqueta,
                                razon: "tipo no soportado en F2 (omitido honesto)".to_string(),
                            });
                        }
                    }
                }
            }
            ItemOrden::Comando(idx) => {
                let Some(cmd) = construccion.comandos.get(idx) else {
                    continue;
                };
                let nombre = cmd.nombre.trim().to_ascii_lowercase();
                let entradas = &cmd.entradas;
                let salidas = &cmd.salidas;
                let salida_etiqueta = salidas
                    .first()
                    .map(|s| sanitize_etiqueta(s))
                    .unwrap_or_default();
                if nombre.contains("3d")
                    || nombre.contains("plane")
                    || nombre.contains("sphere")
                    || nombre.contains("quadric")
                {
                    reporte.omitidos.push(OmittedObject {
                        tipo: cmd.nombre.clone(),
                        label: salida_etiqueta,
                        razon: "comando 3D omitido en núcleo aula".to_string(),
                    });
                    continue;
                }
                if nombre.contains("cas") {
                    reporte.omitidos.push(OmittedObject {
                        tipo: cmd.nombre.clone(),
                        label: salida_etiqueta,
                        razon: "CAS omitido".to_string(),
                    });
                    continue;
                }
                match nombre.as_str() {
                    "point" => {
                        if entradas.len() == 1 {
                            let arg = entradas[0].trim();
                            if arg.starts_with('(') {
                                let inner = arg.trim_matches(|c| c == '(' || c == ')');
                                let parts: Vec<&str> = inner.split(',').collect();
                                if parts.len() == 2 {
                                    if let (Ok(x), Ok(y)) = (
                                        parts[0].trim().parse::<f64>(),
                                        parts[1].trim().parse::<f64>(),
                                    ) {
                                        if x.is_finite() && y.is_finite() {
                                            let c = format!("Point[{}]", fmt_point(x, y));
                                            puntos.insert(salida_etiqueta.clone(), (x, y));
                                            try_push_mapeado(
                                                &mut reporte,
                                                salida_etiqueta,
                                                "Point".to_string(),
                                                c,
                                            );
                                            continue;
                                        }
                                    }
                                }
                            }
                            reporte.omitidos.push(OmittedObject {
                                tipo: cmd.nombre.clone(),
                                label: salida_etiqueta,
                                razon: "Point comando con referencia no resuelta".to_string(),
                            });
                        } else {
                            reporte.omitidos.push(OmittedObject {
                                tipo: cmd.nombre.clone(),
                                label: salida_etiqueta,
                                razon: "Point comando con aridad no soportada".to_string(),
                            });
                        }
                    }
                    "segment" | "line" | "ray" => {
                        if entradas.len() >= 2 {
                            let a = entradas[0].trim();
                            let b = entradas[1].trim();
                            let pa = puntos.get(a).copied().or_else(|| parse_point_literal(a));
                            let pb = puntos.get(b).copied().or_else(|| parse_point_literal(b));
                            if let (Some((x1, y1)), Some((x2, y2))) = (pa, pb) {
                                let kind = match nombre.as_str() {
                                    "segment" => "Segment",
                                    "ray" => "Ray",
                                    _ => "Line",
                                };
                                let c =
                                    format!("{kind}[{}, {}]", fmt_point(x1, y1), fmt_point(x2, y2));
                                lineas.insert(salida_etiqueta.clone(), ((x1, y1), (x2, y2)));
                                try_push_mapeado(
                                    &mut reporte,
                                    salida_etiqueta,
                                    kind.to_string(),
                                    c,
                                );
                            } else {
                                reporte.omitidos.push(OmittedObject {
                                    tipo: cmd.nombre.clone(),
                                    label: salida_etiqueta,
                                    razon: format!(
                                        "{} con puntos no encontrados: '{}' '{}'",
                                        cmd.nombre, a, b
                                    ),
                                });
                            }
                        } else {
                            reporte.omitidos.push(OmittedObject {
                                tipo: cmd.nombre.clone(),
                                label: salida_etiqueta,
                                razon: format!("{} requiere 2 puntos", cmd.nombre),
                            });
                        }
                    }
                    "vector" => {
                        if entradas.len() >= 2 {
                            let a = entradas[0].trim();
                            let b = entradas[1].trim();
                            let pa = puntos.get(a).copied().or_else(|| parse_point_literal(a));
                            let pb = puntos.get(b).copied().or_else(|| parse_point_literal(b));
                            if let (Some(s), Some(e_)) = (pa, pb) {
                                let c = format!(
                                    "Vector[{}, {}]",
                                    fmt_point(s.0, s.1),
                                    fmt_point(e_.0, e_.1)
                                );
                                lineas.insert(salida_etiqueta.clone(), (s, e_));
                                try_push_mapeado(
                                    &mut reporte,
                                    salida_etiqueta.clone(),
                                    "Vector".to_string(),
                                    c,
                                );
                                let len = ((e_.0 - s.0).powi(2) + (e_.1 - s.1).powi(2)).sqrt();
                                if len.is_finite() {
                                    reporte.omitidos.push(OmittedObject { tipo: "Text".to_string(), label: format!("{salida_etiqueta}_medida"), razon: format!("medida vector len {} — Text genérico sin comando (TODO honesto F2)", fmt_num(len)) });
                                }
                            } else {
                                reporte.omitidos.push(OmittedObject {
                                    tipo: cmd.nombre.clone(),
                                    label: salida_etiqueta,
                                    razon: format!("Vector puntos no hallados '{a}' '{b}'"),
                                });
                            }
                        } else {
                            reporte.omitidos.push(OmittedObject {
                                tipo: cmd.nombre.clone(),
                                label: salida_etiqueta,
                                razon: "Vector requiere 2 puntos".to_string(),
                            });
                        }
                    }
                    "circle" | "semicircle" => {
                        if entradas.len() >= 2 {
                            let center_label = entradas[0].trim();
                            let second = entradas[1].trim();
                            let pc = puntos
                                .get(center_label)
                                .copied()
                                .or_else(|| parse_point_literal(center_label));
                            if let Some((cx, cy)) = pc {
                                if let Ok(r) = second.parse::<f64>() {
                                    if r.is_finite() && r > 1e-12 {
                                        let c = format!(
                                            "Circle[{}, {}]",
                                            fmt_point(cx, cy),
                                            fmt_num(r)
                                        );
                                        circulos.insert(salida_etiqueta.clone(), ((cx, cy), r));
                                        try_push_mapeado(
                                            &mut reporte,
                                            salida_etiqueta,
                                            "Circle".to_string(),
                                            c,
                                        );
                                    } else {
                                        reporte.omitidos.push(OmittedObject {
                                            tipo: cmd.nombre.clone(),
                                            label: salida_etiqueta,
                                            razon: "radio no finito o nulo".to_string(),
                                        });
                                    }
                                } else if let Some((px, py)) = puntos
                                    .get(second)
                                    .copied()
                                    .or_else(|| parse_point_literal(second))
                                {
                                    let r = ((px - cx).powi(2) + (py - cy).powi(2)).sqrt();
                                    if r.is_finite() && r > 1e-12 {
                                        let c = format!(
                                            "Circle[{}, {}]",
                                            fmt_point(cx, cy),
                                            fmt_num(r)
                                        );
                                        circulos.insert(salida_etiqueta.clone(), ((cx, cy), r));
                                        try_push_mapeado(
                                            &mut reporte,
                                            salida_etiqueta,
                                            "Circle".to_string(),
                                            c,
                                        );
                                    } else {
                                        reporte.omitidos.push(OmittedObject {
                                            tipo: cmd.nombre.clone(),
                                            label: salida_etiqueta,
                                            razon: "radio derivado no finito".to_string(),
                                        });
                                    }
                                } else {
                                    reporte.omitidos.push(OmittedObject {
                                        tipo: cmd.nombre.clone(),
                                        label: salida_etiqueta,
                                        razon: format!("Circle segundo arg no resuelto '{second}'"),
                                    });
                                }
                            } else {
                                reporte.omitidos.push(OmittedObject {
                                    tipo: cmd.nombre.clone(),
                                    label: salida_etiqueta,
                                    razon: format!("centro no hallado '{center_label}'"),
                                });
                            }
                        } else {
                            reporte.omitidos.push(OmittedObject {
                                tipo: cmd.nombre.clone(),
                                label: salida_etiqueta,
                                razon: "Circle requiere centro y radio/punto".to_string(),
                            });
                        }
                    }
                    "polygon" => {
                        if entradas.len() >= 3 {
                            let mut verts: Vec<(f64, f64)> = Vec::new();
                            let mut faltan: Vec<String> = Vec::new();
                            for e in entradas {
                                let t = e.trim();
                                if let Some(p) =
                                    puntos.get(t).copied().or_else(|| parse_point_literal(t))
                                {
                                    verts.push(p);
                                } else {
                                    faltan.push(t.to_string());
                                }
                            }
                            if !faltan.is_empty() {
                                reporte.omitidos.push(OmittedObject {
                                    tipo: cmd.nombre.clone(),
                                    label: salida_etiqueta,
                                    razon: format!(
                                        "Polygon vértices no hallados: {}",
                                        faltan.join(", ")
                                    ),
                                });
                                continue;
                            }
                            if verts.len() < 3 {
                                reporte.omitidos.push(OmittedObject {
                                    tipo: cmd.nombre.clone(),
                                    label: salida_etiqueta,
                                    razon: "Polygon requiere ≥3 vértices".to_string(),
                                });
                                continue;
                            }
                            let mut all_finite = true;
                            for (x, y) in &verts {
                                if !x.is_finite() || !y.is_finite() {
                                    all_finite = false;
                                    break;
                                }
                            }
                            if !all_finite {
                                reporte.omitidos.push(OmittedObject {
                                    tipo: cmd.nombre.clone(),
                                    label: salida_etiqueta,
                                    razon: "Polygon con vértices no finitos".to_string(),
                                });
                                continue;
                            }
                            let pts_str: Vec<String> =
                                verts.iter().map(|(x, y)| fmt_point(*x, *y)).collect();
                            let cmd_str = format!("Polygon[{}]", pts_str.join(", "));
                            if cmd_str.len() > MAX_EXPR_CHARS {
                                reporte.omitidos.push(OmittedObject {
                                    tipo: cmd.nombre.clone(),
                                    label: salida_etiqueta.clone(),
                                    razon: "Polygon comando excede MAX_EXPR_CHARS".to_string(),
                                });
                                continue;
                            }
                            try_push_mapeado(
                                &mut reporte,
                                salida_etiqueta.clone(),
                                "Polygon".to_string(),
                                cmd_str,
                            );
                            let area = polygon_area(&verts);
                            if area.is_finite() && area > 1e-12 {
                                reporte.omitidos.push(OmittedObject {
                                    tipo: "Text".to_string(),
                                    label: format!("{salida_etiqueta}_area"),
                                    razon: format!(
                                        "área {} — Text sin comando (TODO honesto F2)",
                                        fmt_num(area)
                                    ),
                                });
                            }
                        } else {
                            reporte.omitidos.push(OmittedObject {
                                tipo: cmd.nombre.clone(),
                                label: salida_etiqueta,
                                razon: "Polygon requiere ≥3 vértices".to_string(),
                            });
                        }
                    }
                    "angle" => {
                        if entradas.len() >= 3 {
                            let a = entradas[0].trim();
                            let b = entradas[1].trim();
                            let c_ = entradas[2].trim();
                            let pa = puntos.get(a).copied().or_else(|| parse_point_literal(a));
                            let pb = puntos.get(b).copied().or_else(|| parse_point_literal(b));
                            let pc = puntos.get(c_).copied().or_else(|| parse_point_literal(c_));
                            if let (Some(pa_), Some(pb_), Some(pc_)) = (pa, pb, pc) {
                                if let Some(ang) = angle_at(pb_, pa_, pc_) {
                                    let poly_cmd = format!(
                                        "Polygon[{}, {}, {}]",
                                        fmt_point(pa_.0, pa_.1),
                                        fmt_point(pb_.0, pb_.1),
                                        fmt_point(pc_.0, pc_.1)
                                    );
                                    try_push_mapeado(
                                        &mut reporte,
                                        format!("{salida_etiqueta}_poly"),
                                        "Polygon".to_string(),
                                        poly_cmd,
                                    );
                                    reporte.omitidos.push(OmittedObject {
                                        tipo: "Text".to_string(),
                                        label: format!("{salida_etiqueta}_medida"),
                                        razon: format!(
                                            "ángulo {}° — Text sin comando (TODO honesto F2)",
                                            fmt_num(ang)
                                        ),
                                    });
                                } else {
                                    reporte.omitidos.push(OmittedObject {
                                        tipo: cmd.nombre.clone(),
                                        label: salida_etiqueta,
                                        razon:
                                            "Angle no calculable (puntos colineales o no finitos)"
                                                .to_string(),
                                    });
                                }
                            } else {
                                reporte.omitidos.push(OmittedObject {
                                    tipo: cmd.nombre.clone(),
                                    label: salida_etiqueta,
                                    razon: format!("Angle puntos no hallados '{a}' '{b}' '{c_}'"),
                                });
                            }
                        } else if entradas.len() == 2 {
                            reporte.omitidos.push(OmittedObject { tipo: cmd.nombre.clone(), label: salida_etiqueta, razon: "Angle con 2 args (rectas) no soportado canónico — omitido honesto".to_string() });
                        } else {
                            reporte.omitidos.push(OmittedObject {
                                tipo: cmd.nombre.clone(),
                                label: salida_etiqueta,
                                razon: "Angle requiere 3 puntos".to_string(),
                            });
                        }
                    }
                    "intersect" | "intersection" | "interseccion" => {
                        if entradas.len() >= 2 {
                            let o1 = entradas[0].trim();
                            let o2 = entradas[1].trim();
                            let l1 = lineas.get(o1).copied();
                            let l2 = lineas.get(o2).copied();
                            if let (Some((p1, p2)), Some((p3, p4))) = (l1, l2) {
                                if let Some((x, y)) = line_intersection(p1, p2, p3, p4) {
                                    let cmd = format!("Point[{}]", fmt_point(x, y));
                                    puntos.insert(salida_etiqueta.clone(), (x, y));
                                    try_push_mapeado(
                                        &mut reporte,
                                        salida_etiqueta,
                                        "Point".to_string(),
                                        cmd,
                                    );
                                } else {
                                    reporte.omitidos.push(OmittedObject {
                                        tipo: cmd.nombre.clone(),
                                        label: salida_etiqueta,
                                        razon: "Intersect líneas paralelas o no finitas"
                                            .to_string(),
                                    });
                                }
                            } else {
                                reporte.omitidos.push(OmittedObject { tipo: cmd.nombre.clone(), label: salida_etiqueta, razon: format!("Intersect requiere líneas mapeadas previamente; '{o1}' o '{o2}' no es línea conocida (solo line-line soportado en F2 canónico)") });
                            }
                        } else {
                            reporte.omitidos.push(OmittedObject {
                                tipo: cmd.nombre.clone(),
                                label: salida_etiqueta,
                                razon: "Intersect requiere 2 objetos".to_string(),
                            });
                        }
                    }
                    "text" | "formulaes" | "latex" => {
                        reporte.omitidos.push(OmittedObject {
                            tipo: cmd.nombre.clone(),
                            label: salida_etiqueta,
                            razon: "Text genérico — no hay comando Text estable (TODO honesto F2)"
                                .to_string(),
                        });
                    }
                    "slider" | "checkbox" | "inputbox" | "button" => {
                        reporte.omitidos.push(OmittedObject {
                            tipo: cmd.nombre.clone(),
                            label: salida_etiqueta,
                            razon: "control UI omitido en núcleo aula".to_string(),
                        });
                    }
                    _ => {
                        let lower = nombre.to_ascii_lowercase();
                        if lower.contains("conic")
                            || lower.contains("ellipse")
                            || lower.contains("hyperbola")
                            || lower.contains("parabola")
                        {
                            reporte.omitidos.push(OmittedObject {
                                tipo: cmd.nombre.clone(),
                                label: salida_etiqueta,
                                razon: "cónica vía comando — usar elemento conic canónico (F2)"
                                    .to_string(),
                            });
                        } else if lower.contains("locus")
                            || lower.contains("list")
                            || lower.contains("sequence")
                        {
                            reporte.omitidos.push(OmittedObject {
                                tipo: cmd.nombre.clone(),
                                label: salida_etiqueta,
                                razon: "comando no soportado en F2 (omitido honesto)".to_string(),
                            });
                        } else {
                            reporte.omitidos.push(OmittedObject { tipo: cmd.nombre.clone(), label: salida_etiqueta, razon: "comando no soportado en F2/F3 (omitido honesto, nunca fallo silencioso)".to_string() });
                        }
                    }
                }
            }
        }
    }
    for expr in &construccion.expresiones {
        let tipo_l = expr.tipo.to_ascii_lowercase();
        let et = sanitize_etiqueta(&expr.etiqueta);
        if tipo_l == "function"
            || tipo_l == "functionnvar"
            || expr.exp.contains("->")
            || expr.exp.contains('(')
        {
            match valida_expr(&expr.exp) {
                Ok(clean) => {
                    let cmd = if clean.contains('=') {
                        let parts: Vec<&str> = clean.splitn(2, '=').collect();
                        let rhs = parts.get(1).map(|s| s.trim()).unwrap_or(&clean);
                        format!("Function[{rhs}]")
                    } else {
                        format!("Function[{clean}]")
                    };
                    if cmd.len() > MAX_EXPR_CHARS {
                        reporte.omitidos.push(OmittedObject {
                            tipo: "Function".to_string(),
                            label: et,
                            razon: "function excede MAX_EXPR_CHARS".to_string(),
                        });
                        continue;
                    }
                    if reporte.objetos.len() < MAX_ELEMS {
                        try_push_mapeado(&mut reporte, et, "Function".to_string(), cmd);
                    } else {
                        reporte.omitidos.push(OmittedObject {
                            tipo: "Function".to_string(),
                            label: et,
                            razon: format!("presupuesto MAX_ELEMS {MAX_ELEMS}"),
                        });
                    }
                }
                Err(r) => {
                    reporte.omitidos.push(OmittedObject {
                        tipo: "Function".to_string(),
                        label: et,
                        razon: r,
                    });
                }
            }
        } else if tipo_l == "numeric" || tipo_l == "point" {
            continue;
        } else if !expr.exp.trim().is_empty() {
            reporte.omitidos.push(OmittedObject {
                tipo: format!("expression:{}", expr.tipo),
                label: et,
                razon: "tipo de expresión no mapeado en F0/F1 (omitido honesto)".to_string(),
            });
        }
    }
    let mut hoja_xs: Option<Vec<f64>> = None;
    let mut hoja_ys: Option<Vec<f64>> = None;
    let mut hoja_x_name = "x".to_string();
    let mut hoja_y_name = "y".to_string();
    if !construccion.hoja_celdas.is_empty() {
        let mut tmp_rows: Vec<Vec<String>> = Vec::new();
        for fila in &construccion.hoja_celdas {
            if fila.len() == 2 {
                tmp_rows.push(fila.clone());
            } else if fila.len() == 1 {
                continue;
            }
            if tmp_rows.len() > MAX_DATA_TABLE_ROWS + 1 {
                break;
            }
        }
        if tmp_rows.len() >= 2 {
            let first = &tmp_rows[0];
            let is_header = first[0].parse::<f64>().is_err() && first[1].parse::<f64>().is_err();
            let start = if is_header {
                hoja_x_name = first[0].clone();
                hoja_y_name = first[1].clone();
                1
            } else {
                0
            };
            let mut xs: Vec<f64> = Vec::new();
            let mut ys: Vec<f64> = Vec::new();
            let mut ok = true;
            for row in tmp_rows.iter().skip(start) {
                match (row[0].parse::<f64>(), row[1].parse::<f64>()) {
                    (Ok(x), Ok(y)) if x.is_finite() && y.is_finite() => {
                        xs.push(x);
                        ys.push(y);
                    }
                    _ => {
                        ok = false;
                        break;
                    }
                }
                if xs.len() > MAX_DATA_TABLE_ROWS {
                    ok = false;
                    break;
                }
            }
            if ok && xs.len() >= 2 {
                hoja_xs = Some(xs);
                hoja_ys = Some(ys);
            }
        }
    }
    if hoja_xs.is_none() {
        if let Some((xs, ys, xn, yn)) = extraer_tabla_de_numericos(&construccion.elementos) {
            hoja_xs = Some(xs);
            hoja_ys = Some(ys);
            hoja_x_name = xn;
            hoja_y_name = yn;
        }
    }
    if let (Some(xs), Some(ys)) = (hoja_xs, hoja_ys) {
        if xs.len() >= 2 && xs.len() == ys.len() && xs.len() <= MAX_DATA_TABLE_ROWS {
            let all_finite = xs.iter().chain(ys.iter()).all(|v| v.is_finite());
            if all_finite {
                let xs_str = xs
                    .iter()
                    .map(|v| fmt_num(*v))
                    .collect::<Vec<_>>()
                    .join(", ");
                let ys_str = ys
                    .iter()
                    .map(|v| fmt_num(*v))
                    .collect::<Vec<_>>()
                    .join(", ");
                let dt_cmd = format!("DataTable[{{{xs_str}}}, {{{ys_str}}}]");
                let sp_cmd = format!("ScatterPlot[{{{xs_str}}}, {{{ys_str}}}]");
                if dt_cmd.len() <= MAX_EXPR_CHARS && sp_cmd.len() <= MAX_EXPR_CHARS {
                    let dt_label = format!("{}_{}", hoja_x_name, hoja_y_name);
                    if reporte
                        .objetos
                        .len()
                        .checked_add(2)
                        .is_some_and(|n| n <= MAX_ELEMS)
                    {
                        try_push_mapeado(&mut reporte, dt_label, "DataTable".to_string(), dt_cmd);
                        try_push_mapeado(
                            &mut reporte,
                            "scatter".to_string(),
                            "ScatterPlot".to_string(),
                            sp_cmd,
                        );
                    } else {
                        reporte.omitidos.push(OmittedObject {
                            tipo: "DataTable".to_string(),
                            label: hoja_x_name.clone(),
                            razon: format!(
                                "presupuesto MAX_ELEMS {MAX_ELEMS} excedido para tabla F3"
                            ),
                        });
                    }
                } else {
                    reporte.omitidos.push(OmittedObject {
                        tipo: "DataTable".to_string(),
                        label: hoja_x_name,
                        razon: "tabla excede MAX_EXPR_CHARS (omitido honesto)".to_string(),
                    });
                }
            } else {
                reporte.omitidos.push(OmittedObject {
                    tipo: "DataTable".to_string(),
                    label: hoja_x_name,
                    razon: "tabla con valores no finitos — omitida".to_string(),
                });
            }
        } else if xs.len() > MAX_DATA_TABLE_ROWS {
            reporte.omitidos.push(OmittedObject {
                tipo: "DataTable".to_string(),
                label: hoja_x_name,
                razon: format!("tabla supera MAX_DATA_TABLE_ROWS {}", MAX_DATA_TABLE_ROWS),
            });
        }
    }
    reporte.mapeados = reporte.objetos.len();
    reporte
}
fn parse_point_literal(s: &str) -> Option<(f64, f64)> {
    let t = s.trim();
    let inner = if t.starts_with('(') && t.ends_with(')') {
        &t[1..t.len() - 1]
    } else if t.contains(',') {
        t
    } else {
        return None;
    };
    let parts: Vec<&str> = inner.split(',').collect();
    if parts.len() != 2 {
        return None;
    }
    let x: f64 = parts[0].trim().parse().ok()?;
    let y: f64 = parts[1].trim().parse().ok()?;
    if x.is_finite() && y.is_finite() {
        Some((x, y))
    } else {
        None
    }
}
