//! Motor puro de LISTAS/TEXTO (paridad GeoGebra).
//!
//! Sin I/O, sin `Document`: el cableado resuelve etiquetas → datos
//! (contenido de `Text`, filas de `DataTable`, selección real) y crea los
//! objetos. Todo es determinista y acotado por las consts de este módulo.
//!
//! Convenciones GeoGebra que se respetan acá:
//! - Índices 1-based en `SelectedIndex`/`SelectedElement`/`ColumnName`.
//! - `ColumnName` es biyectivo base-26: 1→A … 26→Z, 27→AA.
//! - `TextObj.rotation` está en radianes (igual que `egui::epaint::TextShape`).

use crate::ast::parse_ast;
use crate::expr::{eval_function_var, evaluate, preprocess_expr, MAX_EXPR_LENGTH};

/// Caracteres máximos de texto de entrada/salida (espeja `MAX_STRING_LENGTH`
/// 10 000 de `grafito_core::validation`; `geometry` no puede depender de core).
pub const MAX_TEXT_LEN: usize = 10_000;
/// Partes máximas de `Split` (igual que `MAX_LIST_LENGTH` 10 000 de listas).
pub const MAX_SPLIT_PARTS: usize = 10_000;
/// Filas máximas de `PointList`/`DataFunction`/`Frequency` (igual que listas).
pub const MAX_TEXT_ROWS: usize = 10_000;
/// Índice de columna máximo de `ColumnName` (1-based; 16 384 = XFD, paridad Excel).
pub const MAX_COLUMN_INDEX: usize = 16_384;

fn require_text_len(text: &str, cmd: &str) -> Result<(), String> {
    if text.chars().count() > MAX_TEXT_LEN {
        return Err(format!(
            "{cmd}: el texto excede el máximo {MAX_TEXT_LEN} caracteres"
        ));
    }
    Ok(())
}

fn check_var_name(var: &str, cmd: &str) -> Result<(), String> {
    let ok = !var.is_empty()
        && var.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'_')
        && var
            .bytes()
            .next()
            .is_some_and(|b| b.is_ascii_alphabetic() || b == b'_');
    if ok {
        Ok(())
    } else {
        Err(format!("{cmd}: variable '{var}' inválida"))
    }
}

/// `ReplaceAll[texto, de, a]`: reemplazo literal de subcadenas (sin regex).
///
/// `to` vacío borra. Error si `from` está vacío o si la salida supera
/// `MAX_TEXT_LEN` (cota anti-amplificación: `from` corto + `to` largo).
pub fn replace_all(text: &str, from: &str, to: &str) -> Result<String, String> {
    require_text_len(text, "ReplaceAll")?;
    require_text_len(from, "ReplaceAll")?;
    require_text_len(to, "ReplaceAll")?;
    if from.is_empty() {
        return Err("ReplaceAll: el patrón no puede estar vacío".to_string());
    }
    let out = text.replace(from, to);
    if out.chars().count() > MAX_TEXT_LEN {
        return Err(format!(
            "ReplaceAll: el resultado excede el máximo {MAX_TEXT_LEN} caracteres"
        ));
    }
    Ok(out)
}

/// `Split[texto, delimitador]`: divide por un separador literal.
///
/// Delimitador vacío → un ítem por carácter (honesto y documentado).
/// Conserva partes vacías (`"a,,b"` → `["a", "", "b"]`, como `str::split`).
/// Cota: `MAX_SPLIT_PARTS` partes.
pub fn split_text(text: &str, delimiter: &str) -> Result<Vec<String>, String> {
    require_text_len(text, "Split")?;
    require_text_len(delimiter, "Split")?;
    let parts: Vec<String> = if delimiter.is_empty() {
        text.chars().map(|c| c.to_string()).collect()
    } else {
        text.split(delimiter).map(str::to_string).collect()
    };
    if parts.len() > MAX_SPLIT_PARTS {
        return Err(format!(
            "Split: {} partes exceden el máximo {MAX_SPLIT_PARTS}",
            parts.len()
        ));
    }
    Ok(parts)
}

/// `Split` con varios delimitadores literales (cualquiera corta).
///
/// Aplica los cortes en orden; la cota `MAX_SPLIT_PARTS` se chequea por ronda
/// para no alojar el blow-up intermedio.
pub fn split_text_any(text: &str, delimiters: &[&str]) -> Result<Vec<String>, String> {
    require_text_len(text, "Split")?;
    if delimiters.is_empty() {
        return Err("Split: se requiere al menos un delimitador".to_string());
    }
    for d in delimiters {
        require_text_len(d, "Split")?;
        if d.is_empty() {
            return Err("Split: los delimitadores no pueden estar vacíos".to_string());
        }
    }
    let mut parts = vec![text.to_string()];
    for delim in delimiters {
        let mut next = Vec::new();
        for part in &parts {
            next.extend(part.split(delim).map(str::to_string));
            if next.len() > MAX_SPLIT_PARTS {
                return Err(format!(
                    "Split: las partes exceden el máximo {MAX_SPLIT_PARTS}"
                ));
            }
        }
        parts = next;
    }
    Ok(parts)
}

/// `ParseToNumber[texto]`: texto → f64 tolerante.
///
/// - Recorta espacios (`trim`).
/// - Coma decimal: `"3,14"` → 3.14. Con punto Y coma (`"1.234,56"`) da error
///   honesto (no se adivina el separador de miles); con más de una coma, error.
/// - Infinito: `"∞"`, `"±∞"`, `"inf"`, `"±inf"`, `"infinity"` (con `+`/`-`).
/// - `"nan"` se rechaza (es indefinido, no un número).
/// - Resto: `f64::from_str` (signo, exponente `1e3`, `+`/`-`); además se tolera
///   el menos Unicode U+2212 (`"−5"` → -5.0).
pub fn parse_to_number(text: &str) -> Result<f64, String> {
    // Menos Unicode → ASCII antes de matchear infinitos con signo.
    let ascii = text.trim().replace('−', "-");
    let t = ascii.as_str();
    if t.is_empty() {
        return Err("ParseToNumber: texto vacío".to_string());
    }
    if t.chars().count() > MAX_TEXT_LEN {
        return Err(format!(
            "ParseToNumber: el texto excede el máximo {MAX_TEXT_LEN} caracteres"
        ));
    }
    match t.to_lowercase().as_str() {
        "∞" | "+∞" | "inf" | "+inf" | "infinity" | "+infinity" => return Ok(f64::INFINITY),
        "-∞" | "-inf" | "-infinity" => return Ok(f64::NEG_INFINITY),
        "nan" | "+nan" | "-nan" => {
            return Err("ParseToNumber: 'NaN' es indefinido".to_string());
        }
        _ => {}
    }
    let normalized: String = if t.contains(',') {
        if t.contains('.') {
            return Err(
                "ParseToNumber: no se admite separador de miles (usá punto o coma decimal)"
                    .to_string(),
            );
        }
        if t.chars().filter(|c| *c == ',').count() != 1 {
            return Err("ParseToNumber: una sola coma decimal".to_string());
        }
        t.replace(',', ".")
    } else {
        t.to_string()
    };
    match normalized.parse::<f64>() {
        Ok(v) if v.is_nan() => Err("ParseToNumber: resultado indefinido".to_string()),
        Ok(v) => Ok(v),
        Err(_) => Err(format!("ParseToNumber: '{text}' no es un número")),
    }
}

/// `ParseToFunction[texto, variable]`: valida que el texto parsee como
/// expresión evaluable con la variable dada y la devuelve canónica.
///
/// Canónica = salida de `preprocess_expr` (normaliza LaTeX/sum/product).
/// Chequeo en dos pasos: (1) `parse_ast` debe aceptar la sintaxis;
/// (2) al menos una sonda (`0, 1, -1, 0.5`) debe dar finito con la variable
/// ligada — así se rechazan variables desconocidas (`"y+1"` con var `"x"`)
/// e indefinidos en todas partes (`"(x-x)/(x-x)"`), pero se aceptan
/// `"1/x"` o `"sqrt(x)"` (finitos en alguna sonda) y constantes (`"5"`).
pub fn parse_to_function(text: &str, var: &str) -> Result<String, String> {
    check_var_name(var, "ParseToFunction")?;
    let trimmed = text.trim();
    if trimmed.is_empty() {
        return Err("ParseToFunction: expresión vacía".to_string());
    }
    if trimmed.len() > MAX_EXPR_LENGTH {
        return Err(format!(
            "ParseToFunction: la expresión excede el máximo {MAX_EXPR_LENGTH}"
        ));
    }
    let canonical = preprocess_expr(trimmed);
    if let Err(e) = parse_ast(&canonical) {
        return Err(format!(
            "ParseToFunction: '{trimmed}' no es una expresión válida: {e}"
        ));
    }
    const PROBES: [f64; 4] = [0.0, 1.0, -1.0, 0.5];
    let mut any_finite = false;
    for p in PROBES {
        match eval_function_var(&canonical, var, p) {
            Ok(v) if v.is_finite() => {
                any_finite = true;
                break;
            }
            Ok(_) | Err(_) => {}
        }
    }
    if !any_finite {
        return Err(format!(
            "ParseToFunction: '{trimmed}' no se evalúa con {var} (¿variable desconocida o indefinida en todas las sondas?)"
        ));
    }
    Ok(canonical)
}

/// `ReadText[texto]`: devuelve el contenido tal cual.
///
/// Función pura sobre `content` a propósito: el cableado resuelve
/// etiqueta → `TextObj.content` en el documento y crea el objeto si hace falta.
pub fn read_text(content: &str) -> String {
    content.to_string()
}

/// `Frequency[lista]`: conteo por valor único, ordenado ascendente.
///
/// Rechaza no-finitos (consistente con el resto de listas). Vacía → `Ok(vec![])`.
pub fn frequency(data: &[f64]) -> Result<Vec<(f64, usize)>, String> {
    if data.len() > MAX_TEXT_ROWS {
        return Err(format!(
            "Frequency: {} datos exceden el máximo {MAX_TEXT_ROWS}",
            data.len()
        ));
    }
    if data.iter().any(|v| !v.is_finite()) {
        return Err("Frequency: la lista contiene valores no finitos".to_string());
    }
    let mut sorted = data.to_vec();
    sorted.sort_by(|a, b| a.total_cmp(b));
    let mut out = Vec::new();
    let mut i = 0;
    while i < sorted.len() {
        let v = sorted[i];
        let mut j = i.saturating_add(1);
        while j < sorted.len() && sorted[j] == v {
            j = j.saturating_add(1);
        }
        out.push((v, j.saturating_sub(i)));
        i = j;
    }
    Ok(out)
}

/// Salida de `PointList`: dimensión uniforme + puntos (`z = 0.0` si 2D).
#[derive(Debug, Clone, PartialEq)]
pub struct PointListOut {
    /// 2 o 3 (todas las filas comparten dimensión).
    pub dim: usize,
    /// Puntos validados y finitos.
    pub points: Vec<[f64; 3]>,
}

/// `PointList[matriz]`: `&[Vec<f64>]` → puntos.
///
/// Exige matriz no vacía, filas de 2 o 3 columnas UNIFORMES y valores finitos.
/// Mezclar 2D con 3D da error honesto (el cableado separa o rechaza).
pub fn point_list(rows: &[Vec<f64>]) -> Result<PointListOut, String> {
    if rows.len() > MAX_TEXT_ROWS {
        return Err(format!(
            "PointList: {} filas exceden el máximo {MAX_TEXT_ROWS}",
            rows.len()
        ));
    }
    let Some(first) = rows.first() else {
        return Err("PointList: matriz vacía".to_string());
    };
    let dim = first.len();
    if dim != 2 && dim != 3 {
        return Err(format!(
            "PointList: cada fila debe tener 2 o 3 columnas (llegó {dim})"
        ));
    }
    let mut points = Vec::with_capacity(rows.len());
    for (i, row) in rows.iter().enumerate() {
        if row.len() != dim {
            return Err(format!(
                "PointList: fila {} tiene {} columnas, se esperaban {dim}",
                i.saturating_add(1),
                row.len()
            ));
        }
        if row.iter().any(|v| !v.is_finite()) {
            return Err(format!(
                "PointList: fila {} con valores no finitos",
                i.saturating_add(1)
            ));
        }
        let z = if dim == 3 { row[2] } else { 0.0 };
        points.push([row[0], row[1], z]);
    }
    Ok(PointListOut { dim, points })
}

/// `RemoveUndefined[lista]`: filtra `NaN` e infinitos.
///
/// Infalible por diseño: nunca aloja más que la entrada.
pub fn remove_undefined(data: &[f64]) -> Vec<f64> {
    data.iter().copied().filter(|v| v.is_finite()).collect()
}

/// `ColumnName[n]`: nombre de columna 1-based (A…Z, AA…).
///
/// Biyectivo base-26: 1→A, 26→Z, 27→AA, 28→AB. Cota `MAX_COLUMN_INDEX` 16 384.
pub fn column_name(index: usize) -> Result<String, String> {
    if !(1..=MAX_COLUMN_INDEX).contains(&index) {
        return Err(format!(
            "ColumnName: índice {index} fuera de [1, {MAX_COLUMN_INDEX}]"
        ));
    }
    let mut n = index;
    let mut buf = Vec::new();
    while n > 0 {
        let rem = (n.saturating_sub(1)) % 26;
        buf.push((b'A'.saturating_add(rem as u8)) as char);
        n = (n.saturating_sub(1)) / 26;
    }
    Ok(buf.iter().rev().collect())
}

/// `DataFunction[expr, filas]`: evalúa la expresión sobre cada fila.
///
/// Liga por fila `x` (= `xs[i]`), `y` (= `ys[i]`, solo si se pasa `ys`) y
/// `n` (= índice 1-based), más `vars` extra del llamador. `x`/`y`/`n`
/// pisan a `vars` si colisionan (reservadas de fila, documentado).
/// `ys` con largo distinto → error. Resultado no finito → error en la fila.
pub fn data_function(
    expr: &str,
    xs: &[f64],
    ys: Option<&[f64]>,
    vars: &[(String, f64)],
) -> Result<Vec<f64>, String> {
    if expr.trim().is_empty() {
        return Err("DataFunction: expresión vacía".to_string());
    }
    if expr.len() > MAX_EXPR_LENGTH {
        return Err(format!(
            "DataFunction: la expresión excede el máximo {MAX_EXPR_LENGTH}"
        ));
    }
    if xs.len() > MAX_TEXT_ROWS {
        return Err(format!(
            "DataFunction: {} filas exceden el máximo {MAX_TEXT_ROWS}",
            xs.len()
        ));
    }
    if xs.is_empty() {
        return Err("DataFunction: sin filas".to_string());
    }
    if let Some(ys) = ys {
        if ys.len() != xs.len() {
            return Err("DataFunction: xs e ys con largos distintos".to_string());
        }
    }
    for (i, x) in xs.iter().enumerate() {
        if !x.is_finite() {
            return Err(format!(
                "DataFunction: fila {} con x no finito",
                i.saturating_add(1)
            ));
        }
        if let Some(ys) = ys {
            if let Some(y) = ys.get(i) {
                if !y.is_finite() {
                    return Err(format!(
                        "DataFunction: fila {} con y no finito",
                        i.saturating_add(1)
                    ));
                }
            }
        }
    }
    let mut out = Vec::with_capacity(xs.len());
    for (i, x) in xs.iter().enumerate() {
        let mut scope: Vec<(String, f64)> = vars.to_vec();
        scope.push(("x".to_string(), *x));
        if let Some(ys) = ys {
            if let Some(y) = ys.get(i) {
                scope.push(("y".to_string(), *y));
            }
        }
        scope.push(("n".to_string(), i.saturating_add(1) as f64));
        let v = evaluate(expr, &scope)
            .map_err(|e| format!("DataFunction: fila {}: {e}", i.saturating_add(1)))?;
        if !v.is_finite() {
            return Err(format!(
                "DataFunction: fila {} dio valor no finito",
                i.saturating_add(1)
            ));
        }
        out.push(v);
    }
    Ok(out)
}

/// `SelectedIndex[lista]`: posición 1-based del primer rótulo seleccionado.
///
/// Recorre `selection` en orden y devuelve el primer rótulo que exista en
/// `labels`. El acceso a la selección real lo hace el cableado (pasa ambas
/// rebanadas ya resueltas).
pub fn selected_index(labels: &[String], selection: &[String]) -> Result<usize, String> {
    if selection.is_empty() {
        return Err("SelectedIndex: no hay selección".to_string());
    }
    if labels.is_empty() {
        return Err("SelectedIndex: lista vacía".to_string());
    }
    for wanted in selection {
        for (i, label) in labels.iter().enumerate() {
            if label == wanted {
                return Ok(i.saturating_add(1));
            }
        }
    }
    Err("SelectedIndex: la selección no está en la lista".to_string())
}

/// `SelectedElement[lista]`: elemento cuyo rótulo fue seleccionado.
///
/// Genérica para servir a listas numéricas y de texto; `items` y `labels`
/// van en paralelo (mismo largo), como salen del documento.
pub fn selected_element<T: Clone>(
    items: &[T],
    labels: &[String],
    selection: &[String],
) -> Result<T, String> {
    if items.len() != labels.len() {
        return Err("SelectedElement: ítems y etiquetas con largos distintos".to_string());
    }
    let idx = selected_index(labels, selection)
        .map_err(|e| e.replace("SelectedIndex", "SelectedElement"))?;
    items
        .get(idx.saturating_sub(1))
        .cloned()
        .ok_or_else(|| "SelectedElement: índice fuera de rango".to_string())
}

/// `VerticalText[texto]`: apila un carácter por línea (`\n`).
///
/// Puro: el render ya parte por `\n`, así que el cableado solo guarda el
/// contenido. Vacío → error (nada que apilar).
pub fn vertical_text(content: &str) -> Result<String, String> {
    if content.is_empty() {
        return Err("VerticalText: texto vacío".to_string());
    }
    require_text_len(content, "VerticalText")?;
    let mut out = String::new();
    for (i, c) in content.chars().enumerate() {
        if i > 0 {
            out.push('\n');
        }
        out.push(c);
    }
    Ok(out)
}

/// Salida de `RotateText`: contenido + ángulo en radianes (igual que
/// `TextObj.rotation` y `egui::epaint::TextShape::angle`, horario).
#[derive(Debug, Clone, PartialEq)]
pub struct RotatedText {
    /// Contenido tal cual (sin recortar: se preservan espacios del usuario).
    pub content: String,
    /// Ángulo validado en radianes (f32, como el render).
    pub angle_rad: f32,
}

/// `RotateText[texto, ángulo]`: valida y empaqueta contenido + ángulo.
///
/// Recibe radianes (los grados los convierte el cableado). No normaliza:
/// preserva el valor del usuario. El objeto `Text` lo crea el cableado.
pub fn rotate_text(content: &str, angle_rad: f64) -> Result<RotatedText, String> {
    if content.is_empty() {
        return Err("RotateText: texto vacío".to_string());
    }
    require_text_len(content, "RotateText")?;
    if !angle_rad.is_finite() {
        return Err("RotateText: ángulo no finito".to_string());
    }
    Ok(RotatedText {
        content: content.to_string(),
        angle_rad: angle_rad as f32,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn replace_all_sustituye_literal_con_cota() {
        assert_eq!(replace_all("hola mundo", "o", "0").unwrap(), "h0la mund0");
        assert_eq!(replace_all("aaa", "aa", "b").unwrap(), "ba");
        assert_eq!(replace_all("abc", "x", "y").unwrap(), "abc");
        assert_eq!(replace_all("abc", "b", "").unwrap(), "ac");
        assert!(replace_all("abc", "", "x").is_err());
        // Anti-amplificación: 5000 "a" → 20000 chars supera la cota.
        let big = "a".repeat(5000);
        assert!(replace_all(&big, "a", "aaaa").is_err());
        assert!(replace_all(&"a".repeat(MAX_TEXT_LEN + 1), "a", "b").is_err());
    }

    #[test]
    fn split_text_corta_por_delimitador() {
        assert_eq!(
            split_text("a,b,c", ",").unwrap(),
            vec!["a".to_string(), "b".to_string(), "c".to_string()]
        );
        assert_eq!(
            split_text("a,,b", ",").unwrap(),
            vec!["a".to_string(), String::new(), "b".to_string()]
        );
        assert_eq!(
            split_text("abc", "").unwrap(),
            vec!["a".to_string(), "b".to_string(), "c".to_string()]
        );
        assert_eq!(split_text("", ",").unwrap(), vec![String::new()]);
        // Cota de partes (con partes vacías: ","×9999 → 10000 partes ok).
        let at_cap = ",".repeat(MAX_SPLIT_PARTS - 1);
        assert_eq!(split_text(&at_cap, ",").unwrap().len(), MAX_SPLIT_PARTS);
        let over_cap = ",".repeat(MAX_SPLIT_PARTS);
        assert!(split_text(&over_cap, ",").is_err());
    }

    #[test]
    fn split_text_any_acepta_varios_delimitadores() {
        assert_eq!(
            split_text_any("a,b;c", &[",", ";"]).unwrap(),
            vec!["a".to_string(), "b".to_string(), "c".to_string()]
        );
        assert!(split_text_any("a,b", &[]).is_err());
        assert!(split_text_any("a,b", &["", ","]).is_err());
    }

    #[test]
    #[allow(clippy::approx_constant)]
    fn parse_to_number_tolera_coma_e_infinito() {
        assert_eq!(parse_to_number("  3,14 ").unwrap(), 3.14);
        assert_eq!(parse_to_number("42").unwrap(), 42.0);
        assert_eq!(parse_to_number("-1e3").unwrap(), -1000.0);
        assert_eq!(parse_to_number("∞").unwrap(), f64::INFINITY);
        assert_eq!(parse_to_number("-∞").unwrap(), f64::NEG_INFINITY);
        assert_eq!(parse_to_number("inf").unwrap(), f64::INFINITY);
        assert_eq!(parse_to_number("−5").unwrap(), -5.0);
        assert!(parse_to_number("").is_err());
        assert!(parse_to_number("nan").is_err());
        assert!(parse_to_number("1.234,56").is_err());
        assert!(parse_to_number("1,2,3").is_err());
        assert!(parse_to_number("hola").is_err());
    }

    #[test]
    fn parse_to_function_valida_y_devuelve_canonica() {
        let c = parse_to_function(" x^2 + 1 ", "x").unwrap();
        assert!(c.contains('x'));
        assert!(parse_to_function("1/x", "x").is_ok());
        assert!(parse_to_function("5", "x").is_ok());
        assert!(parse_to_function("y+1", "x").is_err());
        assert!(parse_to_function("(x-x)/(x-x)", "x").is_err());
        assert!(parse_to_function("2+", "x").is_err());
        assert!(parse_to_function("", "x").is_err());
        assert!(parse_to_function("x", "").is_err());
        assert!(parse_to_function("x", "9mal").is_err());
        assert!(parse_to_function(&"x+".repeat(MAX_EXPR_LENGTH), "x").is_err());
    }

    #[test]
    fn read_text_devuelve_el_contenido() {
        assert_eq!(read_text("hola"), "hola");
        assert_eq!(read_text(""), "");
    }

    #[test]
    fn frequency_cuenta_ordenado() {
        assert_eq!(
            frequency(&[3.0, 1.0, 2.0, 1.0, 3.0, 3.0]).unwrap(),
            vec![(1.0, 2), (2.0, 1), (3.0, 3)]
        );
        assert_eq!(frequency(&[]).unwrap(), vec![]);
        assert!(frequency(&[1.0, f64::NAN]).is_err());
        assert!(frequency(&[1.0, f64::INFINITY]).is_err());
    }

    #[test]
    fn point_list_valida_dimension_uniforme() {
        let out = point_list(&[vec![1.0, 2.0], vec![3.0, 4.0]]).unwrap();
        assert_eq!(out.dim, 2);
        assert_eq!(out.points, vec![[1.0, 2.0, 0.0], [3.0, 4.0, 0.0]]);
        let out = point_list(&[vec![1.0, 2.0, 3.0]]).unwrap();
        assert_eq!(out.dim, 3);
        assert_eq!(out.points, vec![[1.0, 2.0, 3.0]]);
        assert!(point_list(&[]).is_err());
        assert!(point_list(&[vec![1.0]]).is_err());
        assert!(point_list(&[vec![1.0, 2.0], vec![3.0, 4.0, 5.0]]).is_err());
        assert!(point_list(&[vec![1.0, f64::NAN]]).is_err());
    }

    #[test]
    fn remove_undefined_filtra_no_finitos() {
        assert_eq!(
            remove_undefined(&[1.0, f64::NAN, 2.0, f64::INFINITY, f64::NEG_INFINITY, 3.0]),
            vec![1.0, 2.0, 3.0]
        );
        assert_eq!(remove_undefined(&[]), Vec::<f64>::new());
    }

    #[test]
    fn column_name_es_biyectivo_base_26() {
        assert_eq!(column_name(1).unwrap(), "A");
        assert_eq!(column_name(26).unwrap(), "Z");
        assert_eq!(column_name(27).unwrap(), "AA");
        assert_eq!(column_name(28).unwrap(), "AB");
        assert_eq!(column_name(52).unwrap(), "AZ");
        assert_eq!(column_name(53).unwrap(), "BA");
        assert_eq!(column_name(702).unwrap(), "ZZ");
        assert_eq!(column_name(703).unwrap(), "AAA");
        assert!(column_name(0).is_err());
        assert!(column_name(MAX_COLUMN_INDEX + 1).is_err());
        assert!(column_name(MAX_COLUMN_INDEX).is_ok());
    }

    #[test]
    fn data_function_liga_x_y_n_por_fila() {
        let xs = [1.0, 2.0, 3.0];
        assert_eq!(
            data_function("x^2", &xs, None, &[]).unwrap(),
            vec![1.0, 4.0, 9.0]
        );
        let ys = [10.0, 20.0, 30.0];
        assert_eq!(
            data_function("x+y", &xs, Some(&ys), &[]).unwrap(),
            vec![11.0, 22.0, 33.0]
        );
        assert_eq!(
            data_function("n", &xs, None, &[]).unwrap(),
            vec![1.0, 2.0, 3.0]
        );
        // `y` sin `ys` → error honesto (variable no ligada).
        assert!(data_function("y", &xs, None, &[]).is_err());
        assert!(data_function("x", &[], None, &[]).is_err());
        assert!(data_function("x", &xs, Some(&[1.0]), &[]).is_err());
        assert!(data_function("", &xs, None, &[]).is_err());
        assert!(data_function("1/(x-x)", &xs, None, &[]).is_err());
        // Vars extra del llamador disponibles.
        assert_eq!(
            data_function("x+k", &xs, None, &[("k".to_string(), 100.0)]).unwrap(),
            vec![101.0, 102.0, 103.0]
        );
    }

    #[test]
    fn selected_index_es_uno_based_en_orden_de_seleccion() {
        let labels = vec!["a".to_string(), "b".to_string(), "c".to_string()];
        assert_eq!(selected_index(&labels, &["b".to_string()]).unwrap(), 2);
        // Primer rótulo de la selección que exista manda.
        assert_eq!(
            selected_index(&labels, &["zz".to_string(), "c".to_string()]).unwrap(),
            3
        );
        assert!(selected_index(&labels, &[]).is_err());
        assert!(selected_index(&[], &["a".to_string()]).is_err());
        assert!(selected_index(&labels, &["zz".to_string()]).is_err());
    }

    #[test]
    fn selected_element_devuelve_el_item_paralelo() {
        let items = vec![10.0, 20.0, 30.0];
        let labels = vec!["a".to_string(), "b".to_string(), "c".to_string()];
        assert_eq!(
            selected_element(&items, &labels, &["c".to_string()]).unwrap(),
            30.0
        );
        let words = vec!["uno".to_string(), "dos".to_string()];
        let wlabels = vec!["w1".to_string(), "w2".to_string()];
        assert_eq!(
            selected_element(&words, &wlabels, &["w1".to_string()]).unwrap(),
            "uno"
        );
        assert!(selected_element(&[1.0], &labels, &["a".to_string()]).is_err());
        assert!(selected_element(&items, &labels, &["zz".to_string()]).is_err());
    }

    #[test]
    fn vertical_text_apila_con_salto() {
        assert_eq!(vertical_text("abc").unwrap(), "a\nb\nc");
        assert_eq!(vertical_text("x").unwrap(), "x");
        assert_eq!(vertical_text("añ").unwrap(), "a\nñ");
        assert!(vertical_text("").is_err());
    }

    #[test]
    fn rotate_text_valida_y_empaqueta() {
        let r = rotate_text("hola", std::f64::consts::FRAC_PI_2).unwrap();
        assert_eq!(r.content, "hola");
        assert!((r.angle_rad - std::f32::consts::FRAC_PI_2).abs() < 1e-6);
        assert!(rotate_text("", 0.0).is_err());
        assert!(rotate_text("hola", f64::NAN).is_err());
        assert!(rotate_text("hola", f64::INFINITY).is_err());
    }

    #[test]
    fn cotas_rechazan_entradas_gigantes() {
        let long = "a".repeat(MAX_TEXT_LEN + 1);
        assert!(replace_all(&long, "a", "b").is_err());
        assert!(split_text(&long, ",").is_err());
        assert!(split_text_any(&long, &[","]).is_err());
        assert!(parse_to_number(&long).is_err());
        assert!(vertical_text(&long).is_err());
        assert!(rotate_text(&long, 0.0).is_err());
        assert!(frequency(&vec![1.0; MAX_TEXT_ROWS + 1]).is_err());
        assert!(point_list(&vec![vec![1.0, 2.0]; MAX_TEXT_ROWS + 1]).is_err());
        assert!(data_function("x", &vec![1.0; MAX_TEXT_ROWS + 1], None, &[]).is_err());
    }
}
