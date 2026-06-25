use geo::BooleanOps;
use grafito_core::{
    analyzable::{self, default_analysis_features},
    Attractor3DObj, BoxPlotObj, CircleObj, ComplexGridObj, ComplexMappingObj, Cone3DObj, Cube3DObj,
    Cylinder3DObj, Document, EllipseObj, Fractal2DObj, FunctionObj, GeoObject, HistogramObj,
    HyperSurface4DObj, HyperbolaObj, ImplicitCurveObj, LineKind, LineObj, MoebiusStripObj,
    ObjectId, ParabolaObj, ParametricCurve2DObj, PhasePortraitObj, Point3DObj, PointObj,
    PolarCurveObj, PolygonObj, RegressionLineObj, RelationOperator, ScatterPlotObj, Segment3DObj,
    Sphere3DObj, Surface3DObj, Torus3DObj, VectorField2DObj, VectorField3DObj,
};
use grafito_geometry::analysis::{
    analyze_intersection, arc_length, curvature_at, normal_line_at, surface_of_revolution,
    tangent_line_at, volume_of_revolution, AnalysisFeature, IntersectionCurve,
};
use grafito_geometry::boolean::polygon_to_geo;
use grafito_geometry::expr::{eval_function_with_vars, evaluate};
use grafito_geometry::matrices::{taylor_series, Matrix};
use grafito_geometry::statistics;
use grafito_geometry::symbolic;
use grafito_geometry::Color;
use grafito_geometry::Point2;
use grafito_geometry::Point3D;
use std::collections::{HashMap, HashSet};
use std::sync::OnceLock;

/// Reemplaza una variable por otra solo en límites de palabra (identificadores completos).
/// Evita corromper nombres de funciones: `replace_variable("exp(e)", "e", "x")` → `"exp(x)"`, no `"xxp(x)"`.
fn replace_variable(expr: &str, from: &str, to: &str) -> String {
    let mut result = String::with_capacity(expr.len());
    let bytes = expr.as_bytes();
    let from_bytes = from.as_bytes();
    let mut i = 0;
    while i < bytes.len() {
        if i + from_bytes.len() <= bytes.len() && &bytes[i..i + from_bytes.len()] == from_bytes {
            let before_ok = i == 0 || !is_ident_char(bytes[i - 1]);
            let after_ok =
                i + from_bytes.len() == bytes.len() || !is_ident_char(bytes[i + from_bytes.len()]);
            if before_ok && after_ok {
                result.push_str(to);
                i += from_bytes.len();
                continue;
            }
        }
        // Handle multi-byte UTF-8 by pushing the whole character
        let ch_len = utf8_char_len(bytes[i]);
        result.push_str(&expr[i..i + ch_len]);
        i += ch_len;
    }
    result
}

/// Sustituye las variables de `document.variables` en la expresión, envolviendo
/// cada valor entre paréntesis para preservar la precedencia (p. ej. valores
/// negativos en exponentes). Las variables no finitas se ignoran.
///
/// Esto permite que las herramientas de análisis basadas en derivación
/// simbólica (que operan sobre una expresión pura en `x`) respeten el contexto
/// de variables del documento.
fn substitute_document_vars(expr: &str, document: &Document) -> String {
    let mut out = expr.to_string();
    for (k, v) in &document.variables {
        // `x` es la variable de la función: no se sustituye.
        if k == "x" || !v.is_finite() {
            continue;
        }
        out = replace_variable(&out, k, &format!("({})", v));
    }
    out
}

fn is_ident_char(b: u8) -> bool {
    b.is_ascii_alphanumeric() || b == b'_'
}

fn utf8_char_len(b: u8) -> usize {
    if b < 0x80 {
        1
    } else if b < 0xE0 {
        2
    } else if b < 0xF0 {
        3
    } else {
        4
    }
}

/// Trait para evaluadores GPU de funciones 1D por lotes.
///
/// La aplicación puede registrar una implementación que envuelva el pipeline
/// `function_compute` de `grafito-render` y así habilitar la ruta híbrida
/// (GPU evalúa `f(x)`, CPU reduce) para integrales definidas.
pub trait GpuFunctionEvaluator: Send + Sync {
    /// Evalúa `expr` en `samples` puntos uniformes en `[a, b]`.
    ///
    /// Devuelve `None` si la expresión no es compatible con el bytecode GPU.
    fn evaluate_function_batch(
        &self,
        expr: &str,
        a: f64,
        b: f64,
        samples: usize,
        variables: &HashMap<String, f64>,
    ) -> Option<Vec<f64>>;
}

static GPU_FUNCTION_EVALUATOR: OnceLock<Box<dyn GpuFunctionEvaluator + Send + Sync>> =
    OnceLock::new();

/// Registra el evaluador GPU global usado por la ruta híbrida de integrales.
/// Normalmente se llama una sola vez al inicializar la aplicación.
pub fn register_gpu_function_evaluator(evaluator: Box<dyn GpuFunctionEvaluator + Send + Sync>) {
    let _ = GPU_FUNCTION_EVALUATOR.set(evaluator);
}

/// Resultado de ejecutar un comando de texto.
#[derive(Debug, Clone)]
pub enum CommandOutcome {
    /// Éxito sin mensaje adicional.
    Ok,
    /// Éxito con un mensaje para mostrar (por ejemplo, resultado CAS).
    Message(String),
    /// Error que debe mostrarse al usuario.
    Error(String),
}

pub fn insert_implicit_multiplication(text: &str) -> String {
    let mut res = String::new();
    let chars: Vec<char> = text.chars().collect();
    for i in 0..chars.len() {
        res.push(chars[i]);
        if i + 1 < chars.len() {
            let c1 = chars[i];
            let c2 = chars[i + 1];
            if c1.is_ascii_digit() && c2.is_ascii_alphabetic() {
                res.push('*');
            }
            if c1 == ')' && c2.is_ascii_alphabetic() {
                res.push('*');
            }
            if c1 == ')' && c2.is_ascii_digit() {
                res.push('*');
            }
            if c1.is_ascii_digit() && c2 == '(' && (i == 0 || !chars[i - 1].is_ascii_alphabetic()) {
                res.push('*');
            }
            if c1 == ')' && c2 == '(' {
                res.push('*');
            }
            if (c1 == 'x' || c1 == 'y')
                && c2 == '('
                && (i == 0 || !chars[i - 1].is_ascii_alphabetic())
            {
                res.push('*');
            }
            if (c1 == 'x' || c1 == 'y')
                && c2.is_ascii_alphabetic()
                && (i == 0 || !chars[i - 1].is_ascii_alphabetic())
            {
                res.push('*');
            }
        }
    }
    res
}

/// Parse a numeric command argument supporting `pi`, `2pi`, `π`, `tau`, etc.
///
/// Tries `f64::from_str` first, then applies implicit multiplication and
/// evaluates with `grafito_geometry::expr::evaluate`.
pub fn parse_numeric_arg(s: &str, variables: &HashMap<String, f64>) -> Result<f64, String> {
    let arg = s.trim();
    // Fast path: pure number literal
    if let Ok(val) = arg.parse::<f64>() {
        return Ok(val);
    }
    // Apply implicit multiplication (e.g., "2pi" → "2*pi", "2π" → "2*pi")
    let expanded = insert_implicit_multiplication(arg);
    if let Ok(val) = expanded.parse::<f64>() {
        return Ok(val);
    }
    // Use the expression evaluator which handles pi, tau, etc.
    match evaluate(
        &expanded,
        &variables
            .iter()
            .map(|(k, v)| (k.clone(), *v))
            .collect::<Vec<_>>(),
    ) {
        Ok(val) if val.is_finite() => Ok(val),
        Ok(val) => Err(format!("No es finito: {}", val)),
        Err(e) => Err(format!(
            "No se pudo interpretar como número: '{}' ({})",
            arg, e
        )),
    }
}

/// Parse attractor parameters, supporting key=value syntax.
fn parse_attractor_params(args: &[String]) -> Vec<f64> {
    args.iter()
        .filter_map(|s| {
            // Handle key=value: extract RHS
            let rhs = s.split('=').next_back().unwrap_or(s).trim();
            rhs.parse::<f64>().ok()
        })
        .collect()
}

/// Split an equation/inequality string into (lhs, rhs, operator).
/// Handles: =, <=, >=, ==, !=, <, >
fn split_relation(expr: &str) -> (&str, &str, RelationOperator) {
    if let Some(pos) = split_on_standalone_eq(expr) {
        return (pos.0.trim(), pos.1.trim(), RelationOperator::Eq);
    }
    // Check multi-char operators first
    for (op_str, op) in &[
        ("<=", RelationOperator::LessEq),
        (">=", RelationOperator::GreaterEq),
        ("==", RelationOperator::Eq),
        // "!=" not yet supported by RelationOperator enum
    ] {
        if let Some(pos) = expr.find(op_str) {
            return (expr[..pos].trim(), expr[pos + op_str.len()..].trim(), *op);
        }
    }
    // Single-char operators
    for (op_str, op) in &[
        ("<", RelationOperator::Less),
        (">", RelationOperator::Greater),
    ] {
        if let Some(pos) = expr.find(op_str) {
            // Make sure it's not part of <= or >= (handled above but double-check)
            if pos + 1 < expr.len() && expr.as_bytes()[pos + 1] == b'=' {
                continue;
            }
            return (expr[..pos].trim(), expr[pos + 1..].trim(), *op);
        }
    }
    (expr.trim(), "0", RelationOperator::Eq)
}

/// Split text on a standalone "=" (not part of <=, >=, ==, !=)
fn split_on_standalone_eq(text: &str) -> Option<(&str, &str)> {
    let chars: Vec<char> = text.chars().collect();
    for i in 0..chars.len() {
        if chars[i] == '=' {
            let preceded_by_op = i > 0
                && (chars[i - 1] == '<'
                    || chars[i - 1] == '>'
                    || chars[i - 1] == '='
                    || chars[i - 1] == '!');
            let followed_by_eq = i + 1 < chars.len() && chars[i + 1] == '=';
            if !preceded_by_op && !followed_by_eq {
                let byte_pos = text.chars().take(i).map(|c| c.len_utf8()).sum::<usize>();
                return Some((&text[..byte_pos], &text[byte_pos + 1..]));
            }
        }
    }
    None
}

fn auto_define_variables(text: &str, document: &mut Document) {
    let mut current_word = String::new();
    let mut words = Vec::new();
    for c in text.chars() {
        if c.is_alphabetic() {
            current_word.push(c);
        } else {
            if !current_word.is_empty() {
                words.push(current_word.clone());
                current_word.clear();
            }
        }
    }
    if !current_word.is_empty() {
        words.push(current_word);
    }

    let reserved = [
        "x", "y", "z", "t", "r", "theta", "pi", "tau", "e", "sin", "cos", "tan", "asin", "acos",
        "atan", "sinh", "cosh", "tanh", "sqrt", "log", "ln", "exp", "abs", "mod", "sgn", "step",
        "floor", "ceil", "f", "g", "h",
    ];

    for word in words {
        // Skip reserved words, command names (which usually start with uppercase),
        // or variables that already exist
        if reserved.contains(&word.as_str())
            || word
                .chars()
                .next()
                .map(|c| c.is_uppercase())
                .unwrap_or(false)
            || document.variables.contains_key(&word)
            || document.objects_iter().any(|(_, o)| o.label() == word)
        {
            continue;
        }

        // Auto-define undefined variable to 1.0
        document.set_variable(word, 1.0);
    }
}

pub fn process_input(document: &mut Document, input_text: &mut String) -> CommandOutcome {
    let raw_text = input_text.trim().to_string();
    if raw_text.is_empty() {
        return CommandOutcome::Ok;
    }

    // Sanitize mathematical unicode symbols and uppercase variables from virtual keyboard
    let text = raw_text
        .replace("X", "x")
        .replace("Y", "y")
        .replace("F(x)", "f(x)")
        .replace("G(x)", "g(x)")
        // **Superscripts Unicode**: el usuario escribe `x²` desde el teclado
        // virtual. El parser no entiende `²` (es un caracter Unicode, no un
        // operador). Reemplazamos TODAS las variables comunes elevadas al
        // cuadrado: x², y², z², t², r², a², b², c², n², θ², φ².
        .replace("x²", "x^2")
        .replace("y²", "y^2")
        .replace("z²", "z^2")
        .replace("t²", "t^2")
        .replace("r²", "r^2")
        .replace("a²", "a^2")
        .replace("b²", "b^2")
        .replace("c²", "c^2")
        .replace("n²", "n^2")
        .replace("θ²", "θ^2")
        .replace("φ²", "φ^2")
        .replace("√", "sqrt")
        .replace("|x|", "abs(x)")
        .replace("π", "pi")
        .replace("τ", "tau")
        .replace("÷", "/")
        .replace("×", "*")
        .replace("≤", "<=")
        .replace("≥", ">=")
        // **Cubos y potencias mayores**: x³, x⁴, etc. (sufijos Unicode U+00B3, U+2074, etc.)
        .replace("x³", "x^3")
        .replace("y³", "y^3")
        .replace("z³", "z^3");

    auto_define_variables(&text, document);

    let mut result: CommandOutcome = CommandOutcome::Ok;

    if let Some(mut cmd) = parse_cas_command(&text) {
        cmd.args = cmd
            .args
            .iter()
            .map(|a| insert_implicit_multiplication(a))
            .collect();
        match cmd.command.as_str() {
            "Ellipse" if cmd.args.len() >= 3 => {
                let center_str = cmd.args[0].trim();
                let rest = center_str.trim_start_matches('(').trim_end_matches(')');
                let parts: Vec<f64> = rest
                    .split(',')
                    .filter_map(|s| s.trim().parse().ok())
                    .collect();
                if parts.len() >= 2 {
                    let rx = cmd.args[1].trim().parse().unwrap_or(1.0);
                    let ry = cmd.args[2].trim().parse().unwrap_or(1.0);
                    document.add_object(GeoObject::Ellipse(EllipseObj::new(
                        Point2::new(parts[0], parts[1]),
                        rx,
                        ry,
                    )));
                    input_text.clear();
                    return CommandOutcome::Ok;
                }
            }
            "RegularPolygon" if cmd.args.len() >= 3 => {
                let center_str = cmd.args[0].trim();
                let rest = center_str.trim_start_matches('(').trim_end_matches(')');
                let parts: Vec<f64> = rest
                    .split(',')
                    .filter_map(|s| s.trim().parse().ok())
                    .collect();
                if parts.len() >= 2 {
                    let n = cmd.args[1]
                        .trim()
                        .parse::<usize>()
                        .unwrap_or(4)
                        .clamp(3, 64);
                    let r = parse_numeric_arg(&cmd.args[2], &document.variables).unwrap_or(1.0);
                    let cx = parts[0];
                    let cy = parts[1];
                    let verts: Vec<Point2> = (0..n)
                        .map(|i| {
                            let a = i as f64 / n as f64 * std::f64::consts::TAU;
                            Point2::new(cx + r * a.cos(), cy + r * a.sin())
                        })
                        .collect();
                    document.add_object(GeoObject::Polygon(PolygonObj::new(verts)));
                    input_text.clear();
                    return CommandOutcome::Ok;
                }
            }
            "Distance" if cmd.args.len() >= 2 => {
                if let (Some(a), Some(b)) = (
                    find_object_by_label(document, cmd.args[0].trim()),
                    find_object_by_label(document, cmd.args[1].trim()),
                ) {
                    let target = cmd
                        .args
                        .get(2)
                        .and_then(|s| s.trim().parse().ok())
                        .unwrap_or_else(|| {
                            if let (Some(p1), Some(p2)) =
                                (document.point_position(a), document.point_position(b))
                            {
                                p1.distance(&p2)
                            } else {
                                0.0
                            }
                        });
                    document.add_distance_constraint(a, b, target);
                    document.re_evaluate_constraints(&[]);
                    input_text.clear();
                    return CommandOutcome::Ok;
                } else {
                    return CommandOutcome::Error(format!(
                        "Distance: no se encontraron los objetos '{}' o '{}'",
                        cmd.args[0], cmd.args[1]
                    ));
                }
            }
            "Root" if cmd.args.len() == 1 => {
                return run_analysis_command(
                    document,
                    input_text,
                    cmd.args[0].trim(),
                    &[AnalysisFeature::Root],
                    "Raíz",
                );
            }
            "Extremum" if cmd.args.len() == 1 => {
                return run_analysis_command(
                    document,
                    input_text,
                    cmd.args[0].trim(),
                    &[AnalysisFeature::LocalMaximum, AnalysisFeature::LocalMinimum],
                    "Extremo",
                );
            }
            "Inflection" | "Inflexion" if cmd.args.len() == 1 => {
                return run_analysis_command(
                    document,
                    input_text,
                    cmd.args[0].trim(),
                    &[AnalysisFeature::Inflection],
                    "Inflexión",
                );
            }
            "YIntercept" if cmd.args.len() == 1 => {
                return run_analysis_command(
                    document,
                    input_text,
                    cmd.args[0].trim(),
                    &[AnalysisFeature::YIntercept],
                    "Intersección Y",
                );
            }
            "XIntercept" if cmd.args.len() == 1 => {
                return run_analysis_command(
                    document,
                    input_text,
                    cmd.args[0].trim(),
                    &[AnalysisFeature::XIntercept, AnalysisFeature::Root],
                    "Intersección X",
                );
            }
            "Centroid" if cmd.args.len() == 1 => {
                return run_analysis_command(
                    document,
                    input_text,
                    cmd.args[0].trim(),
                    &[AnalysisFeature::Centroid],
                    "Centroide",
                );
            }
            "Analyze" | "Analizar" if cmd.args.len() == 1 => {
                return run_analysis_command(
                    document,
                    input_text,
                    cmd.args[0].trim(),
                    &default_analysis_features(),
                    "Análisis",
                );
            }
            "Intersect" if cmd.args.len() == 2 => {
                let id1 = find_object_by_label(document, cmd.args[0].trim());
                let id2 = find_object_by_label(document, cmd.args[1].trim());
                if let (Some(i1), Some(i2)) = (id1, id2) {
                    let o1 = document.get_object(i1).cloned();
                    let o2 = document.get_object(i2).cloned();
                    if let (Some(a), Some(b)) = (o1, o2) {
                        let view = *document.view();
                        let world_tl = view.screen_to_world(glam::Vec2::new(0.0, 0.0));
                        let world_br = view.screen_to_world(glam::Vec2::new(
                            view.screen_size.x,
                            view.screen_size.y,
                        ));
                        let view_bounds = (
                            world_tl.x.min(world_br.x),
                            world_tl.x.max(world_br.x),
                            world_tl.y.min(world_br.y),
                            world_tl.y.max(world_br.y),
                        );
                        let vars = document.variables.clone();
                        let curve_a = object_to_intersection_curve(&a);
                        let curve_b = object_to_intersection_curve(&b);
                        if let (Some(ca), Some(cb)) = (curve_a, curve_b) {
                            let pts = analyze_intersection(&ca, &cb, view_bounds, &vars);
                            if pts.is_empty() {
                                input_text.clear();
                                return CommandOutcome::Message(
                                    "Intersect: no se encontraron puntos".into(),
                                );
                            }
                            for p in &pts {
                                let mut pt = PointObj::new(*p);
                                pt.color = grafito_geometry::Color::new(0.9, 0.4, 0.9, 1.0);
                                pt.size = 7.0;
                                document.add_object(GeoObject::Point(pt));
                            }
                            input_text.clear();
                            return CommandOutcome::Message(format!(
                                "Intersect: {} punto(s) creado(s)",
                                pts.len()
                            ));
                        }
                        // Fallback: barrido numérico Function × Function legacy.
                        if let (GeoObject::Function(f1), GeoObject::Function(f2)) = (&a, &b) {
                            let mut inters = Vec::new();
                            let steps = 400;
                            let mut prev_diff: Option<f64> = None;
                            let mut prev_x = 0.0;
                            let mut vars2 = HashMap::new();
                            for i in 0..=steps {
                                let x = -20.0 + (40.0 * i as f64) / steps as f64;
                                vars2.insert("x".to_string(), x);
                                let v: Vec<_> =
                                    vars2.iter().map(|(k, v)| (k.clone(), *v)).collect();
                                if let (Ok(y1), Ok(y2)) =
                                    (evaluate(&f1.expr, &v), evaluate(&f2.expr, &v))
                                {
                                    let diff = y1 - y2;
                                    if let Some(pd) = prev_diff {
                                        if pd * diff < 0.0 {
                                            let root_x = prev_x - pd * (x - prev_x) / (diff - pd);
                                            vars2.insert("x".to_string(), root_x);
                                            let v2: Vec<_> = vars2
                                                .iter()
                                                .map(|(k, v)| (k.clone(), *v))
                                                .collect();
                                            if let Ok(root_y) = evaluate(&f1.expr, &v2) {
                                                inters.push(Point2::new(root_x, root_y));
                                            }
                                        }
                                    }
                                    prev_diff = Some(diff);
                                    prev_x = x;
                                }
                            }
                            let count = inters.len();
                            for r in inters {
                                document.add_object(GeoObject::Point(PointObj::new(r)));
                            }
                            input_text.clear();
                            return CommandOutcome::Message(format!(
                                "Intersect: {} intersección(es) encontrada(s)",
                                count
                            ));
                        }
                    }
                }
                return CommandOutcome::Error(
                    "Intersect: objetos no compatibles o no encontrados".into(),
                );
            }
            "Area" if cmd.args.len() == 1 => {
                // Area[objeto]: crea polígono sombreado + label
                let label = cmd.args[0].trim();
                if let Some(id) = find_object_by_label(document, label) {
                    if let Some(obj) = document.get_object(id).cloned() {
                        let verts_opt: Option<Vec<Point2>> = match &obj {
                            GeoObject::Circle(c) => {
                                let n = 64;
                                let mut v = Vec::with_capacity(n);
                                for k in 0..n {
                                    let theta =
                                        2.0 * std::f64::consts::PI * (k as f64) / (n as f64);
                                    v.push(Point2::new(
                                        c.center.x + c.radius * theta.cos(),
                                        c.center.y + c.radius * theta.sin(),
                                    ));
                                }
                                Some(v)
                            }
                            GeoObject::Polygon(poly) if poly.vertices.len() >= 3 => {
                                Some(poly.vertices.clone())
                            }
                            _ => None,
                        };
                        if let Some(verts) = verts_opt {
                            let area = match &obj {
                                GeoObject::Circle(c) => std::f64::consts::PI * c.radius * c.radius,
                                GeoObject::Polygon(poly) => {
                                    let mut s = 0.0;
                                    for i in 0..poly.vertices.len() {
                                        let j = (i + 1) % poly.vertices.len();
                                        s += poly.vertices[i].x * poly.vertices[j].y
                                            - poly.vertices[j].x * poly.vertices[i].y;
                                    }
                                    s.abs() * 0.5
                                }
                                _ => 0.0,
                            };
                            let n = verts.len() as f64;
                            let cx = verts.iter().map(|v| v.x).sum::<f64>() / n;
                            let cy = verts.iter().map(|v| v.y).sum::<f64>() / n;
                            let mut fill_poly = grafito_core::PolygonObj::new(verts);
                            fill_poly.color = grafito_geometry::Color::new(0.2, 0.5, 0.9, 1.0);
                            fill_poly.width = 1.5;
                            fill_poly.fill_color =
                                Some(grafito_geometry::Color::new(0.2, 0.5, 0.9, 0.3));
                            document.add_object(GeoObject::Polygon(fill_poly));
                            let txt = grafito_core::TextObj::new(
                                format!("A = {:.3}", area),
                                Point2::new(cx, cy),
                            );
                            document.add_object(GeoObject::Text(txt));
                            input_text.clear();
                            return CommandOutcome::Message(format!("Área = {:.3}", area));
                        }
                    }
                }
                return CommandOutcome::Error("Area: objeto no encontrado o no soportado".into());
            }
            "Circumference" if cmd.args.len() == 1 => {
                let label = cmd.args[0].trim();
                if let Some(id) = find_object_by_label(document, label) {
                    let perim = if let Some(obj) = document.get_object(id) {
                        match obj {
                            GeoObject::Circle(c) => 2.0 * std::f64::consts::PI * c.radius,
                            GeoObject::Polygon(poly) => {
                                let mut s = 0.0;
                                for i in 0..poly.vertices.len() {
                                    let a = poly.vertices[i];
                                    let b = poly.vertices[(i + 1) % poly.vertices.len()];
                                    let dx = b.x - a.x;
                                    let dy = b.y - a.y;
                                    s += (dx * dx + dy * dy).sqrt();
                                }
                                s
                            }
                            _ => -1.0,
                        }
                    } else {
                        -1.0
                    };
                    if perim >= 0.0 {
                        return CommandOutcome::Message(format!(
                            "Perímetro({}) = {:.3}",
                            label, perim
                        ));
                    }
                }
                return CommandOutcome::Error("Circumference: objeto no encontrado".into());
            }
            "Center" if cmd.args.len() == 1 => {
                let label = cmd.args[0].trim();
                if let Some(id) = find_object_by_label(document, label) {
                    if let Some(obj) = document.get_object(id) {
                        let center = match obj {
                            GeoObject::Circle(c) => Some(c.center),
                            GeoObject::Ellipse(e) => Some(e.center),
                            GeoObject::Parabola(p) => Some(Point2::new(p.vertex.x, p.vertex.y)),
                            GeoObject::Hyperbola(h) => Some(Point2::new(h.center.x, h.center.y)),
                            _ => None,
                        };
                        if let Some(c) = center {
                            let new_label = next_function_label(document);
                            document.add_object(GeoObject::Point(
                                PointObj::new(c).with_label(&new_label),
                            ));
                            return CommandOutcome::Message(format!(
                                "Centro de {} = ({:.3}, {:.3})",
                                label, c.x, c.y
                            ));
                        }
                    }
                }
                return CommandOutcome::Error("Center: objeto no encontrado o sin centro".into());
            }
            "Sector" if cmd.args.len() == 3 => {
                // Sector[centro, radio, angulo] — crea un sector circular
                if let (Ok((cx, cy)), Ok(r), Ok(deg)) = (
                    parse_point_str(&cmd.args[0]),
                    parse_numeric_arg(&cmd.args[1], &document.variables),
                    parse_numeric_arg(&cmd.args[2], &document.variables),
                ) {
                    let theta = deg.to_radians();
                    let n = 32;
                    let mut verts = Vec::with_capacity(n + 2);
                    verts.push(Point2::new(cx, cy));
                    for k in 0..=n {
                        let t = theta * (k as f64) / (n as f64);
                        verts.push(Point2::new(cx + r * t.cos(), cy + r * t.sin()));
                    }
                    let arc_len = r * theta.abs();
                    let area = 0.5 * r * r * theta.abs();
                    let mut sector_poly = grafito_core::PolygonObj::new(verts);
                    sector_poly.color = grafito_geometry::Color::new(0.9, 0.5, 0.2, 1.0);
                    sector_poly.width = 1.5;
                    sector_poly.fill_color = Some(grafito_geometry::Color::new(0.9, 0.5, 0.2, 0.3));
                    document.add_object(GeoObject::Polygon(sector_poly));
                    let txt = grafito_core::TextObj::new(
                        format!(
                            "Sector: r={:.2}  θ={:.1}°  A={:.3}  L={:.3}",
                            r, deg, area, arc_len
                        ),
                        Point2::new(cx, cy),
                    );
                    document.add_object(GeoObject::Text(txt));
                    input_text.clear();
                    return CommandOutcome::Message(format!(
                        "Sector creado: r={:.2}, θ={:.1}°",
                        r, deg
                    ));
                }
                return CommandOutcome::Error("Sector: argumentos inválidos".into());
            }
            "Arc" if cmd.args.len() == 4 => {
                // Arc[centro, radio, angulo1, angulo2] — arco entre dos ángulos
                if let (Ok((cx, cy)), Ok(r), Ok(deg1), Ok(deg2)) = (
                    parse_point_str(&cmd.args[0]),
                    parse_numeric_arg(&cmd.args[1], &document.variables),
                    parse_numeric_arg(&cmd.args[2], &document.variables),
                    parse_numeric_arg(&cmd.args[3], &document.variables),
                ) {
                    let t1 = deg1.to_radians();
                    let t2 = deg2.to_radians();
                    let n = 48;
                    let mut pts = Vec::with_capacity(n + 1);
                    for k in 0..=n {
                        let t = t1 + (t2 - t1) * (k as f64) / (n as f64);
                        pts.push(Point2::new(cx + r * t.cos(), cy + r * t.sin()));
                    }
                    let arc_len = r * (t2 - t1).abs();
                    let mut arc = LineObj::new(pts[0], *pts.last().unwrap());
                    arc.color = grafito_geometry::Color::new(0.9, 0.5, 0.2, 1.0);
                    arc.width = 2.0;
                    document.add_object(GeoObject::Line(arc));
                    let txt = grafito_core::TextObj::new(
                        format!("Arco: r={:.2}  L={:.3}", r, arc_len),
                        Point2::new(cx, cy),
                    );
                    document.add_object(GeoObject::Text(txt));
                    input_text.clear();
                    return CommandOutcome::Message(format!(
                        "Arco creado: r={:.2}, {:.1}° → {:.1}°",
                        r, deg1, deg2
                    ));
                }
                return CommandOutcome::Error("Arc: argumentos inválidos".into());
            }
            "Angle" if cmd.args.len() >= 2 => {
                if let (Some(a), Some(b)) = (
                    find_object_by_label(document, cmd.args[0].trim()),
                    find_object_by_label(document, cmd.args[1].trim()),
                ) {
                    let target = cmd
                        .args
                        .get(2)
                        .and_then(|s| s.trim().parse().ok())
                        .unwrap_or(0.0);
                    document.add_angle_constraint(a, b, target);
                    document.re_evaluate_constraints(&[]);
                    input_text.clear();
                    return CommandOutcome::Ok;
                } else {
                    return CommandOutcome::Error(format!(
                        "Angle: no se encontraron los objetos '{}' o '{}'",
                        cmd.args[0], cmd.args[1]
                    ));
                }
            }
            "Tangent" if cmd.args.len() == 2 => {
                if let (Some(a), Some(b)) = (
                    find_object_by_label(document, cmd.args[0].trim()),
                    find_object_by_label(document, cmd.args[1].trim()),
                ) {
                    document.add_tangent_constraint(a, b);
                    document.re_evaluate_constraints(&[]);
                    input_text.clear();
                    return CommandOutcome::Ok;
                } else {
                    return CommandOutcome::Error(format!(
                        "Tangent: no se encontraron los objetos '{}' o '{}'",
                        cmd.args[0], cmd.args[1]
                    ));
                }
            }
            "Coincident" if cmd.args.len() == 2 => {
                if let (Some(a), Some(b)) = (
                    find_object_by_label(document, cmd.args[0].trim()),
                    find_object_by_label(document, cmd.args[1].trim()),
                ) {
                    document.add_coincident_constraint(a, b);
                    document.re_evaluate_constraints(&[]);
                    input_text.clear();
                    return CommandOutcome::Ok;
                } else {
                    return CommandOutcome::Error(format!(
                        "Coincident: no se encontraron los objetos '{}' o '{}'",
                        cmd.args[0], cmd.args[1]
                    ));
                }
            }
            "Horizontal" if !cmd.args.is_empty() => {
                if let Some(id) = find_object_by_label(document, cmd.args[0].trim()) {
                    document.add_horizontal_constraint(id);
                    document.re_evaluate_constraints(&[]);
                    input_text.clear();
                    return CommandOutcome::Ok;
                } else {
                    return CommandOutcome::Error(format!(
                        "Horizontal: no se encontró el objeto '{}'",
                        cmd.args[0]
                    ));
                }
            }
            "Vertical" if !cmd.args.is_empty() => {
                if let Some(id) = find_object_by_label(document, cmd.args[0].trim()) {
                    document.add_vertical_constraint(id);
                    document.re_evaluate_constraints(&[]);
                    input_text.clear();
                    return CommandOutcome::Ok;
                } else {
                    return CommandOutcome::Error(format!(
                        "Vertical: no se encontró el objeto '{}'",
                        cmd.args[0]
                    ));
                }
            }
            "EqualLength" if cmd.args.len() == 2 => {
                if let (Some(a), Some(b)) = (
                    find_object_by_label(document, cmd.args[0].trim()),
                    find_object_by_label(document, cmd.args[1].trim()),
                ) {
                    document.add_equal_length_constraint(a, b);
                    document.re_evaluate_constraints(&[]);
                    input_text.clear();
                    return CommandOutcome::Ok;
                } else {
                    return CommandOutcome::Error(format!(
                        "EqualLength: no se encontraron los objetos '{}' o '{}'",
                        cmd.args[0], cmd.args[1]
                    ));
                }
            }
            "Symmetry" if cmd.args.len() == 3 => {
                if let [Some(p), Some(q), Some(line)] = [
                    find_object_by_label(document, cmd.args[0].trim()),
                    find_object_by_label(document, cmd.args[1].trim()),
                    find_object_by_label(document, cmd.args[2].trim()),
                ]
                .as_slice()
                {
                    document.add_symmetry_constraint(*p, *q, *line);
                    document.re_evaluate_constraints(&[]);
                    input_text.clear();
                    return CommandOutcome::Ok;
                } else {
                    return CommandOutcome::Error(format!(
                        "Symmetry: no se encontraron los objetos '{}', '{}' o '{}'",
                        cmd.args[0], cmd.args[1], cmd.args[2]
                    ));
                }
            }
            "EllipseByFoci" if cmd.args.len() == 3 => {
                let ids: Vec<Option<ObjectId>> = cmd
                    .args
                    .iter()
                    .map(|a| find_object_by_label(document, a.trim()))
                    .collect();
                if let [Some(f1), Some(f2), Some(p)] = ids.as_slice() {
                    document.add_ellipse_by_foci_constraint(*f1, *f2, *p);
                    let order = document.propagation_order(&[*f1, *f2, *p]);
                    document.re_evaluate_constraints(&order);
                    input_text.clear();
                    return CommandOutcome::Ok;
                } else {
                    return CommandOutcome::Error(format!(
                        "EllipseByFoci: no se encontraron los objetos '{}', '{}' o '{}'",
                        cmd.args[0], cmd.args[1], cmd.args[2]
                    ));
                }
            }
            "ParabolaByFocusDirectrix" if cmd.args.len() == 2 => {
                if let (Some(focus), Some(directrix)) = (
                    find_object_by_label(document, cmd.args[0].trim()),
                    find_object_by_label(document, cmd.args[1].trim()),
                ) {
                    document.add_parabola_by_focus_directrix_constraint(focus, directrix);
                    let order = document.propagation_order(&[focus, directrix]);
                    document.re_evaluate_constraints(&order);
                    input_text.clear();
                    return CommandOutcome::Ok;
                } else {
                    return CommandOutcome::Error(format!(
                        "ParabolaByFocusDirectrix: no se encontraron los objetos '{}' o '{}'",
                        cmd.args[0], cmd.args[1]
                    ));
                }
            }
            "HyperbolaByFoci" if cmd.args.len() == 3 => {
                let ids: Vec<Option<ObjectId>> = cmd
                    .args
                    .iter()
                    .map(|a| find_object_by_label(document, a.trim()))
                    .collect();
                if let [Some(f1), Some(f2), Some(p)] = ids.as_slice() {
                    document.add_hyperbola_by_foci_constraint(*f1, *f2, *p);
                    let order = document.propagation_order(&[*f1, *f2, *p]);
                    document.re_evaluate_constraints(&order);
                    input_text.clear();
                    return CommandOutcome::Ok;
                } else {
                    return CommandOutcome::Error(format!(
                        "HyperbolaByFoci: no se encontraron los objetos '{}', '{}' o '{}'",
                        cmd.args[0], cmd.args[1], cmd.args[2]
                    ));
                }
            }
            "ConicByFivePoints" if cmd.args.len() == 5 => {
                let ids: Vec<Option<ObjectId>> = cmd
                    .args
                    .iter()
                    .map(|a| find_object_by_label(document, a.trim()))
                    .collect();
                if ids.iter().all(|o| o.is_some()) {
                    let ids: Vec<ObjectId> = ids.into_iter().flatten().collect();
                    document.add_conic_by_five_points_constraint(&ids);
                    let order = document.propagation_order(&ids);
                    document.re_evaluate_constraints(&order);
                    input_text.clear();
                    return CommandOutcome::Ok;
                } else {
                    return CommandOutcome::Error(
                        "ConicByFivePoints: no se encontraron los 5 puntos".into(),
                    );
                }
            }
            "PolygonUnion" if cmd.args.len() == 2 => {
                match resolve_two_polygons(document, &cmd.args[0], &cmd.args[1]) {
                    Ok((a, b)) => {
                        add_boolean_result(document, &a.union(&b), "U");
                    }
                    Err(msg) => result = CommandOutcome::Message(msg),
                }
                input_text.clear();
                return result;
            }
            "PolygonIntersection" if cmd.args.len() == 2 => {
                match resolve_two_polygons(document, &cmd.args[0], &cmd.args[1]) {
                    Ok((a, b)) => {
                        add_boolean_result(document, &a.intersection(&b), "I");
                    }
                    Err(msg) => result = CommandOutcome::Message(msg),
                }
                input_text.clear();
                return result;
            }
            "PolygonDifference" if cmd.args.len() == 2 => {
                match resolve_two_polygons(document, &cmd.args[0], &cmd.args[1]) {
                    Ok((a, b)) => {
                        add_boolean_result(document, &a.difference(&b), "D");
                    }
                    Err(msg) => result = CommandOutcome::Message(msg),
                }
                input_text.clear();
                return result;
            }
            "PolygonXor" if cmd.args.len() == 2 => {
                match resolve_two_polygons(document, &cmd.args[0], &cmd.args[1]) {
                    Ok((a, b)) => {
                        add_boolean_result(document, &a.xor(&b), "X");
                    }
                    Err(msg) => result = CommandOutcome::Message(msg),
                }
                input_text.clear();
                return result;
            }
            "Translate" if cmd.args.len() == 2 => {
                if let (Some(id), Ok((dx, dy))) = (
                    find_object_by_label(document, &cmd.args[0]),
                    parse_point_str(&cmd.args[1]),
                ) {
                    if let Some(obj) = document.get_object(id) {
                        match obj {
                            GeoObject::Point(p) => {
                                let new_pos = Point2::new(p.position.x + dx, p.position.y + dy);
                                let mut params = HashMap::new();
                                params.insert("dx".to_string(), dx);
                                params.insert("dy".to_string(), dy);
                                let (_, _cons_id) = document.add_constructed_object_with_params(
                                    GeoObject::Point(
                                        PointObj::new(new_pos).with_label(format!("{}'", p.label)),
                                    ),
                                    "Translate",
                                    &[id],
                                    params,
                                );
                            }
                            _ => {
                                result =
                                    CommandOutcome::Error("Translate only supports Points".into());
                            }
                        }
                    }
                } else {
                    result = CommandOutcome::Error("Usage: Translate[Object, (dx,dy)]".into());
                }
                input_text.clear();
                return result;
            }
            "Rotate" if cmd.args.len() == 2 => {
                if let (Some(id), Ok(angle)) = (
                    find_object_by_label(document, &cmd.args[0]),
                    parse_numeric_arg(&cmd.args[1], &document.variables),
                ) {
                    if let Some(obj) = document.get_object(id) {
                        match obj {
                            GeoObject::Point(p) => {
                                let c = angle.to_radians();
                                let nx = p.position.x * c.cos() - p.position.y * c.sin();
                                let ny = p.position.x * c.sin() + p.position.y * c.cos();
                                let mut params = HashMap::new();
                                params.insert("angle".to_string(), angle);
                                let (_, _cons_id) = document.add_constructed_object_with_params(
                                    GeoObject::Point(
                                        PointObj::new(Point2::new(nx, ny))
                                            .with_label(format!("{}'", p.label)),
                                    ),
                                    "Rotate",
                                    &[id],
                                    params,
                                );
                            }
                            _ => {
                                result =
                                    CommandOutcome::Error("Rotate only supports Points".into());
                            }
                        }
                    }
                } else {
                    result = CommandOutcome::Error("Usage: Rotate[Object, angle_degrees]".into());
                }
                input_text.clear();
                return result;
            }
            "Surface3D" if cmd.args.len() >= 7 => {
                // Parametric form: Surface3D[x(u,v), y(u,v), z(u,v), umin, umax, vmin, vmax]
                let expr_x = cmd.args[0].trim();
                let expr_y = cmd.args[1].trim();
                let expr_z = cmd.args[2].trim();
                if let (Ok(umin), Ok(umax), Ok(vmin), Ok(vmax)) = (
                    parse_numeric_arg(&cmd.args[3], &document.variables),
                    parse_numeric_arg(&cmd.args[4], &document.variables),
                    parse_numeric_arg(&cmd.args[5], &document.variables),
                    parse_numeric_arg(&cmd.args[6], &document.variables),
                ) {
                    let obj = GeoObject::Surface3D(Surface3DObj::new_parametric(
                        expr_x,
                        expr_y,
                        expr_z,
                        (umin, umax),
                        (vmin, vmax),
                    ));
                    document.add_object(obj);
                    input_text.clear();
                    return CommandOutcome::Ok;
                }
            }
            "Surface3D" if cmd.args.len() >= 5 => {
                let expr = cmd.args[0].trim();
                if let (Ok(x_min), Ok(x_max), Ok(y_min), Ok(y_max)) = (
                    parse_numeric_arg(&cmd.args[1], &document.variables),
                    parse_numeric_arg(&cmd.args[2], &document.variables),
                    parse_numeric_arg(&cmd.args[3], &document.variables),
                    parse_numeric_arg(&cmd.args[4], &document.variables),
                ) {
                    let obj = GeoObject::Surface3D(Surface3DObj::new(
                        expr,
                        (x_min, x_max),
                        (y_min, y_max),
                    ));
                    document.add_object(obj);
                    input_text.clear();
                    return CommandOutcome::Ok;
                }
            }
            "Point3D" if cmd.args.len() == 3 => {
                if let (Ok(x), Ok(y), Ok(z)) = (
                    cmd.args[0].trim().parse(),
                    cmd.args[1].trim().parse(),
                    cmd.args[2].trim().parse(),
                ) {
                    let obj = GeoObject::Point3D(Point3DObj::new(Point3D::new(x, y, z)));
                    document.add_object(obj);
                    input_text.clear();
                    return CommandOutcome::Ok;
                }
            }
            "Segment3D" if cmd.args.len() == 6 => {
                if let (Ok(x1), Ok(y1), Ok(z1), Ok(x2), Ok(y2), Ok(z2)) = (
                    cmd.args[0].trim().parse(),
                    cmd.args[1].trim().parse(),
                    cmd.args[2].trim().parse(),
                    cmd.args[3].trim().parse(),
                    cmd.args[4].trim().parse(),
                    cmd.args[5].trim().parse(),
                ) {
                    let obj = GeoObject::Segment3D(Segment3DObj::new(
                        Point3D::new(x1, y1, z1),
                        Point3D::new(x2, y2, z2),
                    ));
                    document.add_object(obj);
                    input_text.clear();
                    return CommandOutcome::Ok;
                }
            }
            "Sphere" if cmd.args.len() == 4 => {
                if let (Ok(x), Ok(y), Ok(z), Ok(r)) = (
                    cmd.args[0].trim().parse(),
                    cmd.args[1].trim().parse(),
                    cmd.args[2].trim().parse(),
                    cmd.args[3].trim().parse(),
                ) {
                    let obj = GeoObject::Sphere3D(Sphere3DObj::new(Point3D::new(x, y, z), r));
                    document.add_object(obj);
                    input_text.clear();
                    return CommandOutcome::Ok;
                }
            }
            "Cube" if cmd.args.len() == 4 => {
                if let (Ok(x), Ok(y), Ok(z), Ok(s)) = (
                    cmd.args[0].trim().parse(),
                    cmd.args[1].trim().parse(),
                    cmd.args[2].trim().parse(),
                    cmd.args[3].trim().parse(),
                ) {
                    let obj = GeoObject::Cube3D(Cube3DObj::new(Point3D::new(x, y, z), s));
                    document.add_object(obj);
                    input_text.clear();
                    return CommandOutcome::Ok;
                }
            }
            "Cylinder" if cmd.args.len() == 5 => {
                if let (Ok(x), Ok(y), Ok(z), Ok(r), Ok(h)) = (
                    parse_numeric_arg(&cmd.args[0], &document.variables),
                    parse_numeric_arg(&cmd.args[1], &document.variables),
                    parse_numeric_arg(&cmd.args[2], &document.variables),
                    parse_numeric_arg(&cmd.args[3], &document.variables),
                    parse_numeric_arg(&cmd.args[4], &document.variables),
                ) {
                    let obj = GeoObject::Cylinder3D(Cylinder3DObj::new(
                        Point3D::new(x, y, z),
                        Point3D::new(x, y, z + h),
                        r,
                    ));
                    document.add_object(obj);
                    input_text.clear();
                    return CommandOutcome::Ok;
                }
            }
            "Cone" if cmd.args.len() == 5 => {
                if let (Ok(x), Ok(y), Ok(z), Ok(r), Ok(h)) = (
                    parse_numeric_arg(&cmd.args[0], &document.variables),
                    parse_numeric_arg(&cmd.args[1], &document.variables),
                    parse_numeric_arg(&cmd.args[2], &document.variables),
                    parse_numeric_arg(&cmd.args[3], &document.variables),
                    parse_numeric_arg(&cmd.args[4], &document.variables),
                ) {
                    let obj = GeoObject::Cone3D(Cone3DObj::new(
                        Point3D::new(x, y, z),
                        Point3D::new(x, y, z + h),
                        r,
                    ));
                    document.add_object(obj);
                    input_text.clear();
                    return CommandOutcome::Ok;
                }
            }
            "Torus" if cmd.args.len() == 5 => {
                if let (Ok(x), Ok(y), Ok(z), Ok(rmaj), Ok(rmin)) = (
                    parse_numeric_arg(&cmd.args[0], &document.variables),
                    parse_numeric_arg(&cmd.args[1], &document.variables),
                    parse_numeric_arg(&cmd.args[2], &document.variables),
                    parse_numeric_arg(&cmd.args[3], &document.variables),
                    parse_numeric_arg(&cmd.args[4], &document.variables),
                ) {
                    let obj =
                        GeoObject::Torus3D(Torus3DObj::new(Point3D::new(x, y, z), rmaj, rmin));
                    document.add_object(obj);
                    input_text.clear();
                    return CommandOutcome::Ok;
                }
            }
            "Moebius" if cmd.args.len() == 2 => {
                if let (Ok(r), Ok(w)) = (
                    parse_numeric_arg(&cmd.args[0], &document.variables),
                    parse_numeric_arg(&cmd.args[1], &document.variables),
                ) {
                    let obj = GeoObject::MoebiusStrip(MoebiusStripObj::new(
                        Point3D::new(0.0, 0.0, 0.0),
                        r,
                        w,
                    ));
                    document.add_object(obj);
                    input_text.clear();
                    return CommandOutcome::Ok;
                }
            }
            "Point3D" | "Segment3D" | "Sphere" | "Cube" | "Cylinder" | "Cone" | "Torus"
            | "Moebius" | "Surface3D" => {
                return CommandOutcome::Error("Argumentos inválidos para comando 3D".into());
            }
            "Tangent" => {
                if cmd.args.len() >= 3 {
                    if let (Ok((cx, cy)), Ok(r), Ok((px, py))) = (
                        parse_point_str(&cmd.args[0]),
                        parse_numeric_arg(&cmd.args[1], &document.variables),
                        parse_point_str(&cmd.args[2]),
                    ) {
                        let dx = px - cx;
                        let dy = py - cy;
                        let d = (dx * dx + dy * dy).sqrt();
                        if d > r {
                            let a = r * r / d;
                            let h = (r * r - a * a).sqrt();
                            let pm = Point2::new(cx + a * dx / d, cy + a * dy / d);
                            let perp_x = -h * dy / d;
                            let perp_y = h * dx / d;
                            let t1 = Point2::new(pm.x + perp_x, pm.y + perp_y);
                            let t2 = Point2::new(pm.x - perp_x, pm.y - perp_y);
                            document.add_object(GeoObject::Line(
                                LineObj::new_with_kind(Point2::new(px, py), t1, LineKind::Line)
                                    .with_label("T1"),
                            ));
                            document.add_object(GeoObject::Line(
                                LineObj::new_with_kind(Point2::new(px, py), t2, LineKind::Line)
                                    .with_label("T2"),
                            ));
                        } else {
                            input_text.clear();
                            return CommandOutcome::Message(
                                "Tangent: el punto está dentro del círculo, no hay tangentes"
                                    .into(),
                            );
                        }
                    }
                }
                input_text.clear();
                return CommandOutcome::Ok;
            }
            "Intersect" if cmd.args.len() == 2 => {
                let label_a = cmd.args[0].trim();
                let label_b = cmd.args[1].trim();
                let id_a = find_object_by_label(document, label_a);
                let id_b = find_object_by_label(document, label_b);
                if let (Some(id_a), Some(id_b)) = (id_a, id_b) {
                    let obj_a = document.get_object(id_a).cloned();
                    let obj_b = document.get_object(id_b).cloned();
                    if let (Some(obj_a), Some(obj_b)) = (obj_a, obj_b) {
                        let pts = intersect_objects(&obj_a, &obj_b);
                        match pts.len() {
                            0 => result = CommandOutcome::Message("No intersection".into()),
                            1 => {
                                let p = pts[0];
                                document.add_constructed_object(
                                    GeoObject::Point(PointObj::new(p).with_label("I")),
                                    "Intersect",
                                    &[id_a, id_b],
                                );
                                result = CommandOutcome::Message(format!(
                                    "Intersection at ({:.4}, {:.4})",
                                    p.x, p.y
                                ));
                            }
                            2 => {
                                let p1 = pts[0];
                                let p2 = pts[1];
                                document.add_constructed_object(
                                    GeoObject::Point(PointObj::new(p1).with_label("I\u{2081}")),
                                    "Intersect",
                                    &[id_a, id_b],
                                );
                                document.add_constructed_object(
                                    GeoObject::Point(PointObj::new(p2).with_label("I\u{2082}")),
                                    "Intersect",
                                    &[id_a, id_b],
                                );
                                result = CommandOutcome::Message(format!(
                                    "Two intersections: ({:.4}, {:.4}) and ({:.4}, {:.4})",
                                    p1.x, p1.y, p2.x, p2.y
                                ));
                            }
                            _ => {
                                result = CommandOutcome::Message(
                                    "Infinite intersections (coincident)".into(),
                                )
                            }
                        }
                    }
                } else {
                    result = CommandOutcome::Error("Usage: Intersect[obj1, obj2]".into());
                }
                input_text.clear();
                return result;
            }
            "PerpendicularBisector" if cmd.args.len() == 2 => {
                if let (Ok((x1, y1)), Ok((x2, y2))) =
                    (parse_point_str(&cmd.args[0]), parse_point_str(&cmd.args[1]))
                {
                    let mx = (x1 + x2) * 0.5;
                    let my = (y1 + y2) * 0.5;
                    let dx = x2 - x1;
                    let dy = y2 - y1;
                    let p1 = Point2::new(mx - dy * 5.0, my + dx * 5.0);
                    let p2 = Point2::new(mx + dy * 5.0, my - dx * 5.0);
                    document.add_object(GeoObject::Line(
                        LineObj::new_with_kind(p1, p2, LineKind::Line).with_label("B"),
                    ));
                }
                input_text.clear();
                return CommandOutcome::Ok;
            }
            "AngleBisector" if cmd.args.len() == 3 => {
                if let (Ok((x1, y1)), Ok((xv, yv)), Ok((x2, y2))) = (
                    parse_point_str(&cmd.args[0]),
                    parse_point_str(&cmd.args[1]),
                    parse_point_str(&cmd.args[2]),
                ) {
                    let d1 = ((xv - x1).powi(2) + (yv - y1).powi(2)).sqrt();
                    let d2 = ((xv - x2).powi(2) + (yv - y2).powi(2)).sqrt();
                    if d1 > 0.0 && d2 > 0.0 {
                        let ux = (x1 - xv) / d1;
                        let uy = (y1 - yv) / d1;
                        let vx = (x2 - xv) / d2;
                        let vy = (y2 - yv) / d2;
                        let bx = ux + vx;
                        let by = uy + vy;
                        let b_len = (bx * bx + by * by).sqrt();
                        if b_len > 0.0 {
                            let p = Point2::new(xv + bx / b_len * 5.0, yv + by / b_len * 5.0);
                            document.add_object(GeoObject::Line(
                                LineObj::new_with_kind(Point2::new(xv, yv), p, LineKind::Ray)
                                    .with_label("Ab"),
                            ));
                        }
                    }
                }
                input_text.clear();
                return CommandOutcome::Ok;
            }
            "Midpoint" if cmd.args.len() == 2 => {
                let id_a = find_object_by_label(document, cmd.args[0].trim());
                let id_b = find_object_by_label(document, cmd.args[1].trim());
                if let (Some(id_a), Some(id_b)) = (id_a, id_b) {
                    if let (Some(GeoObject::Point(a)), Some(GeoObject::Point(b))) =
                        (document.get_object(id_a), document.get_object(id_b))
                    {
                        let mx = (a.position.x + b.position.x) * 0.5;
                        let my = (a.position.y + b.position.y) * 0.5;
                        document.add_constructed_object(
                            GeoObject::Point(PointObj::new(Point2::new(mx, my)).with_label("M")),
                            "Midpoint",
                            &[id_a, id_b],
                        );
                    }
                } else if let (Ok((x1, y1)), Ok((x2, y2))) =
                    (parse_point_str(&cmd.args[0]), parse_point_str(&cmd.args[1]))
                {
                    let obj = GeoObject::Point(
                        PointObj::new(Point2::new((x1 + x2) * 0.5, (y1 + y2) * 0.5))
                            .with_label("M"),
                    );
                    document.add_object(obj);
                }
                input_text.clear();
                return CommandOutcome::Ok;
            }
            "Perpendicular" if cmd.args.len() == 2 => {
                let line_id = find_object_by_label(document, cmd.args[0].trim());
                let point_id = find_object_by_label(document, cmd.args[1].trim());
                if let (Some(line_id), Some(point_id)) = (line_id, point_id) {
                    if let (Some(GeoObject::Line(_)), Some(GeoObject::Point(_))) =
                        (document.get_object(line_id), document.get_object(point_id))
                    {
                        document.add_constructed_object(
                            GeoObject::Line(
                                LineObj::new_with_kind(
                                    Point2::new(0.0, 0.0),
                                    Point2::new(1.0, 1.0),
                                    LineKind::Line,
                                )
                                .with_label("P"),
                            ),
                            "Perpendicular",
                            &[line_id, point_id],
                        );
                    }
                }
                input_text.clear();
                return CommandOutcome::Ok;
            }
            "Parallel" if cmd.args.len() == 2 => {
                let line_id = find_object_by_label(document, cmd.args[0].trim());
                let point_id = find_object_by_label(document, cmd.args[1].trim());
                if let (Some(line_id), Some(point_id)) = (line_id, point_id) {
                    if let (Some(GeoObject::Line(_)), Some(GeoObject::Point(_))) =
                        (document.get_object(line_id), document.get_object(point_id))
                    {
                        document.add_constructed_object(
                            GeoObject::Line(
                                LineObj::new_with_kind(
                                    Point2::new(0.0, 0.0),
                                    Point2::new(1.0, 1.0),
                                    LineKind::Line,
                                )
                                .with_label("L"),
                            ),
                            "Parallel",
                            &[line_id, point_id],
                        );
                    }
                }
                input_text.clear();
                return CommandOutcome::Ok;
            }
            "PointOnObject" if cmd.args.len() == 2 => {
                let object_id = find_object_by_label(document, cmd.args[0].trim());
                let point_id = find_object_by_label(document, cmd.args[1].trim());
                if let (Some(object_id), Some(point_id)) = (object_id, point_id) {
                    if document.get_object(object_id).is_some()
                        && matches!(document.get_object(point_id), Some(GeoObject::Point(_)))
                    {
                        document.constraints.add_constraint(
                            "PointOnObject",
                            vec![object_id, point_id],
                            vec![point_id],
                            HashMap::new(),
                        );
                        let order = document.propagation_order(&[object_id, point_id]);
                        if !order.is_empty() {
                            document.re_evaluate_constraints(&order);
                        }
                    }
                }
                input_text.clear();
                return CommandOutcome::Ok;
            }
            "CircleByCenterRadius" if cmd.args.len() == 2 => {
                let center_id = find_object_by_label(document, cmd.args[0].trim());
                if let Some(center_id) = center_id {
                    if let Some(GeoObject::Point(_)) = document.get_object(center_id) {
                        let radius =
                            parse_numeric_arg(&cmd.args[1], &document.variables).unwrap_or(1.0);
                        let mut params = HashMap::new();
                        params.insert("radius".to_string(), radius);
                        document.add_constructed_object_with_params(
                            GeoObject::Circle(
                                CircleObj::new(Point2::new(0.0, 0.0), radius).with_label("C"),
                            ),
                            "CircleByCenterRadius",
                            &[center_id],
                            params,
                        );
                    }
                }
                input_text.clear();
                return CommandOutcome::Ok;
            }
            "CircleByThreePoints" if cmd.args.len() == 3 => {
                let ids: Vec<Option<ObjectId>> = cmd
                    .args
                    .iter()
                    .map(|a| find_object_by_label(document, a.trim()))
                    .collect();
                if let [Some(id1), Some(id2), Some(id3)] = ids.as_slice() {
                    if matches!(
                        (
                            document.get_object(*id1),
                            document.get_object(*id2),
                            document.get_object(*id3)
                        ),
                        (
                            Some(GeoObject::Point(_)),
                            Some(GeoObject::Point(_)),
                            Some(GeoObject::Point(_))
                        )
                    ) {
                        document.add_constructed_object(
                            GeoObject::Circle(
                                CircleObj::new(Point2::new(0.0, 0.0), 1.0).with_label("C"),
                            ),
                            "CircleByThreePoints",
                            &[*id1, *id2, *id3],
                        );
                    }
                }
                input_text.clear();
                return CommandOutcome::Ok;
            }
            "PointExpr" if cmd.args.len() == 2 => {
                let x_expr = cmd.args[0].trim().to_string();
                let y_expr = cmd.args[1].trim().to_string();
                let mut point = PointObj::new(Point2::new(0.0, 0.0));
                point.x_expr = Some(x_expr);
                point.y_expr = Some(y_expr);
                document.add_object(GeoObject::Point(point));
                document.recompute_bound_parameters();
                input_text.clear();
                return CommandOutcome::Ok;
            }
            "CircleExpr" if cmd.args.len() == 2 => {
                let center_arg = cmd.args[0].trim();
                let radius_expr = cmd.args[1].trim().to_string();
                let center = if center_arg.starts_with('(') && center_arg.ends_with(')') {
                    parse_point_str(center_arg)
                        .map(|(x, y)| Point2::new(x, y))
                        .unwrap_or_else(|_| Point2::new(0.0, 0.0))
                } else if let Some(id) = find_object_by_label(document, center_arg) {
                    document
                        .get_object(id)
                        .and_then(|obj| match obj {
                            GeoObject::Point(p) => Some(p.position),
                            _ => None,
                        })
                        .unwrap_or_else(|| Point2::new(0.0, 0.0))
                } else {
                    Point2::new(0.0, 0.0)
                };
                let mut circle = CircleObj::new(center, 1.0);
                circle.radius_expr = Some(radius_expr);
                document.add_object(GeoObject::Circle(circle));
                document.recompute_bound_parameters();
                input_text.clear();
                return CommandOutcome::Ok;
            }
            "Vector" if cmd.args.len() == 2 => {
                if let (Ok((x1, y1)), Ok((x2, y2))) =
                    (parse_point_str(&cmd.args[0]), parse_point_str(&cmd.args[1]))
                {
                    let obj = GeoObject::Line(
                        LineObj::new_with_kind(
                            Point2::new(x1, y1),
                            Point2::new(x2, y2),
                            LineKind::Segment,
                        )
                        .with_label("v"),
                    );
                    document.add_object(obj);
                }
                input_text.clear();
                return CommandOutcome::Ok;
            }
            "Ray" if cmd.args.len() == 2 => {
                if let (Ok((x1, y1)), Ok((x2, y2))) =
                    (parse_point_str(&cmd.args[0]), parse_point_str(&cmd.args[1]))
                {
                    document.add_object(GeoObject::Line(
                        LineObj::new_with_kind(
                            Point2::new(x1, y1),
                            Point2::new(x2, y2),
                            LineKind::Ray,
                        )
                        .with_label("r"),
                    ));
                }
                input_text.clear();
                return CommandOutcome::Ok;
            }
            "Line" if cmd.args.len() == 2 => {
                if let (Ok((x1, y1)), Ok((x2, y2))) =
                    (parse_point_str(&cmd.args[0]), parse_point_str(&cmd.args[1]))
                {
                    document.add_object(GeoObject::Line(
                        LineObj::new_with_kind(
                            Point2::new(x1, y1),
                            Point2::new(x2, y2),
                            LineKind::Line,
                        )
                        .with_label("l"),
                    ));
                }
                input_text.clear();
                return CommandOutcome::Ok;
            }
            "Segment" if cmd.args.len() == 2 => {
                if let (Ok((x1, y1)), Ok((x2, y2))) =
                    (parse_point_str(&cmd.args[0]), parse_point_str(&cmd.args[1]))
                {
                    document.add_object(GeoObject::Line(
                        LineObj::new_with_kind(
                            Point2::new(x1, y1),
                            Point2::new(x2, y2),
                            LineKind::Segment,
                        )
                        .with_label("s"),
                    ));
                }
                input_text.clear();
                return CommandOutcome::Ok;
            }
            "Parabola" if cmd.args.len() >= 2 => {
                if let (Ok((vx, vy)), Ok(p)) = (
                    parse_point_str(&cmd.args[0]),
                    parse_numeric_arg(&cmd.args[1], &document.variables),
                ) {
                    document.add_object(GeoObject::Parabola(ParabolaObj::new(
                        Point2::new(vx, vy),
                        p,
                    )));
                }
                input_text.clear();
                return CommandOutcome::Ok;
            }
            "Hyperbola" if cmd.args.len() >= 3 => {
                if let (Ok((cx, cy)), Ok(a), Ok(b)) = (
                    parse_point_str(&cmd.args[0]),
                    parse_numeric_arg(&cmd.args[1], &document.variables),
                    parse_numeric_arg(&cmd.args[2], &document.variables),
                ) {
                    document.add_object(GeoObject::Hyperbola(HyperbolaObj::new(
                        Point2::new(cx, cy),
                        a,
                        b,
                    )));
                }
                input_text.clear();
                return CommandOutcome::Ok;
            }
            "Dilate" if cmd.args.len() == 3 => {
                if let (Ok((px, py)), Ok(factor), Ok((cx, cy))) = (
                    parse_point_str(&cmd.args[0]),
                    parse_numeric_arg(&cmd.args[1], &document.variables),
                    parse_point_str(&cmd.args[2]),
                ) {
                    let nx = cx + (px - cx) * factor;
                    let ny = cy + (py - cy) * factor;
                    document.add_object(GeoObject::Point(
                        PointObj::new(Point2::new(nx, ny)).with_label("D'"),
                    ));
                }
                input_text.clear();
                return CommandOutcome::Ok;
            }
            "Reflect" if cmd.args.len() == 3 => {
                if let (Ok((px, py)), Ok((ax, ay)), Ok((bx, by))) = (
                    parse_point_str(&cmd.args[0]),
                    parse_point_str(&cmd.args[1]),
                    parse_point_str(&cmd.args[2]),
                ) {
                    let dx = bx - ax;
                    let dy = by - ay;
                    let len2 = dx * dx + dy * dy;
                    if len2 > 0.0 {
                        let mirror_point = |p: Point2| {
                            let t = ((p.x - ax) * dx + (p.y - ay) * dy) / len2;
                            let cx = ax + t * dx;
                            let cy = ay + t * dy;
                            Point2::new(2.0 * cx - p.x, 2.0 * cy - p.y)
                        };
                        let id_opt = find_object_by_label(document, &cmd.args[0]);
                        if let Some(id) = id_opt {
                            if let Some(obj) = document.get_object(id).cloned() {
                                let label = obj.label().to_string();
                                let reflected = match obj {
                                    GeoObject::Point(p) => Some(GeoObject::Point(
                                        PointObj::new(mirror_point(p.position))
                                            .with_label(format!("{}'", label)),
                                    )),
                                    GeoObject::Line(l) => {
                                        let s = mirror_point(l.start);
                                        let e = mirror_point(l.end);
                                        Some(GeoObject::Line(
                                            LineObj::new(s, e).with_label(format!("{}'", label)),
                                        ))
                                    }
                                    GeoObject::Circle(c) => {
                                        let mirrored = mirror_point(c.center);
                                        Some(GeoObject::Circle(
                                            CircleObj::new(mirrored, c.radius)
                                                .with_label(format!("{}'", label)),
                                        ))
                                    }
                                    GeoObject::Polygon(poly) => {
                                        let mirrored: Vec<Point2> = poly
                                            .vertices
                                            .iter()
                                            .map(|v| mirror_point(*v))
                                            .collect();
                                        let mut new_poly = PolygonObj::new(mirrored);
                                        new_poly.label = format!("{}'", label);
                                        Some(GeoObject::Polygon(new_poly))
                                    }
                                    _ => None,
                                };
                                if let Some(r) = reflected {
                                    document.add_object(r);
                                } else {
                                    document.add_object(GeoObject::Point(
                                        PointObj::new(mirror_point(Point2::new(px, py)))
                                            .with_label("R'"),
                                    ));
                                }
                            } else {
                                document.add_object(GeoObject::Point(
                                    PointObj::new(mirror_point(Point2::new(px, py)))
                                        .with_label("R'"),
                                ));
                            }
                        } else {
                            document.add_object(GeoObject::Point(
                                PointObj::new(mirror_point(Point2::new(px, py))).with_label("R'"),
                            ));
                        }
                    }
                }
                input_text.clear();
                return CommandOutcome::Ok;
            }
            "Length" if cmd.args.len() == 1 => {
                let label = cmd.args[0].trim();
                if let Some(id) = find_object_by_label(document, label) {
                    if let Some(obj) = document.get_object(id) {
                        let length = match obj {
                            GeoObject::Line(l) => {
                                let dx = l.end.x - l.start.x;
                                let dy = l.end.y - l.start.y;
                                (dx * dx + dy * dy).sqrt()
                            }
                            GeoObject::Segment3D(s) => {
                                let dx = s.b.x - s.a.x;
                                let dy = s.b.y - s.a.y;
                                let dz = s.b.z - s.a.z;
                                (dx * dx + dy * dy + dz * dz).sqrt()
                            }
                            GeoObject::Polygon(poly) => {
                                let mut s = 0.0;
                                for i in 0..poly.vertices.len() {
                                    let a = poly.vertices[i];
                                    let b = poly.vertices[(i + 1) % poly.vertices.len()];
                                    let dx = b.x - a.x;
                                    let dy = b.y - a.y;
                                    s += (dx * dx + dy * dy).sqrt();
                                }
                                s
                            }
                            GeoObject::Circle(c) => 2.0 * std::f64::consts::PI * c.radius,
                            _ => -1.0,
                        };
                        if length >= 0.0 {
                            return CommandOutcome::Message(format!(
                                "Length({}) = {:.3}",
                                label, length
                            ));
                        }
                    }
                }
                return CommandOutcome::Error("Length: objeto no encontrado".into());
            }
            "Slope" if cmd.args.len() == 1 => {
                let label = cmd.args[0].trim();
                if let Some(id) = find_object_by_label(document, label) {
                    if let Some(obj) = document.get_object(id) {
                        let slope = match obj {
                            GeoObject::Line(l) => {
                                if (l.end.x - l.start.x).abs() < 1e-12 {
                                    f64::INFINITY
                                } else {
                                    (l.end.y - l.start.y) / (l.end.x - l.start.x)
                                }
                            }
                            GeoObject::Function(f) => {
                                let x = 0.0;
                                let h = 1e-6;
                                let f1 = grafito_geometry::expr::eval_function_with_vars(
                                    &f.expr,
                                    x + h,
                                    &document.variables,
                                )
                                .unwrap_or(0.0);
                                let fm1 = grafito_geometry::expr::eval_function_with_vars(
                                    &f.expr,
                                    x - h,
                                    &document.variables,
                                )
                                .unwrap_or(0.0);
                                (f1 - fm1) / (2.0 * h)
                            }
                            _ => f64::NAN,
                        };
                        if slope.is_finite() {
                            return CommandOutcome::Message(format!(
                                "Slope({}) = {:.3}",
                                label, slope
                            ));
                        } else if slope.is_infinite() {
                            return CommandOutcome::Message(format!(
                                "Slope({}) = ∞ (vertical)",
                                label
                            ));
                        }
                    }
                }
                return CommandOutcome::Error("Slope: objeto no encontrado".into());
            }
            "Locus" if cmd.args.len() == 2 => {
                let expr = cmd.args[0].trim();
                if let Ok(range) = parse_numeric_arg(&cmd.args[1], &document.variables) {
                    let steps = 200;
                    let mut vertices = Vec::new();
                    for i in 0..=steps {
                        let x = -range + 2.0 * range * i as f64 / steps as f64;
                        let mut vars = HashMap::new();
                        vars.insert("x".to_string(), x);
                        if let Ok(y) = evaluate(
                            expr,
                            &vars
                                .iter()
                                .map(|(k, v)| (k.clone(), *v))
                                .collect::<Vec<_>>(),
                        ) {
                            if y.is_finite() && y.abs() < 1e6 {
                                vertices.push(Point2::new(x, y));
                            }
                        }
                    }
                    if vertices.len() >= 2 {
                        let mut poly = PolygonObj::new(vertices);
                        poly.label = "L".to_string();
                        document.add_object(GeoObject::Polygon(poly));
                    }
                }
                input_text.clear();
                return CommandOutcome::Ok;
            }
            "FunctionInspector" if cmd.args.len() == 1 => {
                let expr = cmd.args[0].trim();
                let v = document.variables.clone();
                let f = |x: f64| {
                    let mut vars: Vec<(String, f64)> =
                        v.iter().map(|(k, val)| (k.clone(), *val)).collect();
                    vars.push(("x".to_string(), x));
                    evaluate(expr, &vars).unwrap_or(f64::NAN)
                };
                let mins = find_extrema(&f, -10.0, 10.0, false);
                let maxs = find_extrema(&f, -10.0, 10.0, true);
                let mut res = String::new();
                if let Some((mx, my)) = root_10(&f) {
                    res.push_str(&format!("Root ≈ ({}: {:.4})", mx, my));
                }
                for (mx, my) in &mins {
                    res.push_str(&format!(" Min@({:.2},{:.2})", mx, my));
                }
                for (mx, my) in &maxs {
                    res.push_str(&format!(" Max@({:.2},{:.2})", mx, my));
                }
                result = CommandOutcome::Message(if res.is_empty() {
                    "No extrema found in [-10,10]".into()
                } else {
                    res
                });
                input_text.clear();
                return result;
            }
            "Normal" if cmd.args.len() == 2 => {
                let mu: f64 = cmd.args[0].trim().parse().unwrap_or(0.0);
                let sigma: f64 = cmd.args[1].trim().parse().unwrap_or(1.0);
                let expr = format!("exp(-(x-{})^2/(2*{}^2))/({}*sqrt(2*pi))", mu, sigma, sigma);
                document.add_object(GeoObject::Function(
                    FunctionObj::new(expr).with_label(format!("N({},{})", mu, sigma)),
                ));
                result = CommandOutcome::Message(format!("Normal N({},{}) added", mu, sigma));
                input_text.clear();
                return result;
            }
            "Binomial" if cmd.args.len() == 3 => {
                let n: usize = cmd.args[0].trim().parse().unwrap_or(10);
                let p: f64 = cmd.args[1].trim().parse().unwrap_or(0.5);
                let k: usize = cmd.args[2].trim().parse().unwrap_or(1);
                let comb = |n: usize, k: usize| -> f64 {
                    if k > n {
                        return 0.0;
                    }
                    let k = k.min(n - k);
                    let mut result = 1.0;
                    for i in 0..k {
                        result = result * (n - i) as f64 / (i + 1) as f64;
                    }
                    result
                };
                let prob = comb(n, k) * p.powi(k as i32) * (1.0 - p).powi((n - k) as i32);
                result = CommandOutcome::Message(format!(
                    "P(X={}) = {:.6} (Binom({},{}))",
                    k, prob, n, p
                ));
                input_text.clear();
                return result;
            }
            "Poisson" if cmd.args.len() == 2 => {
                let lambda: f64 = cmd.args[0].trim().parse().unwrap_or(1.0);
                let k: usize = cmd.args[1].trim().parse().unwrap_or(1);
                let mut prob = (-lambda).exp();
                for i in 1..=k {
                    prob *= lambda / i as f64;
                }
                result = CommandOutcome::Message(format!(
                    "P(X={}) = {:.6} (Poisson({}))",
                    k, prob, lambda
                ));
                input_text.clear();
                return result;
            }
            "Curve3D" if cmd.args.len() >= 3 => {
                let exprs = cmd.args[0].trim();
                let t_min: f64 = cmd.args[1].trim().parse().unwrap_or(0.0);
                let t_max: f64 = parse_numeric_arg(&cmd.args[2], &document.variables)
                    .unwrap_or(std::f64::consts::TAU);
                let steps = 200;
                let mut pts = Vec::new();
                for i in 0..=steps {
                    let t = t_min + (t_max - t_min) * i as f64 / steps as f64;
                    let mut vars = document.variables.clone();
                    vars.insert("t".to_string(), t);
                    let inner = exprs.trim_start_matches('(').trim_end_matches(')');
                    let parts: Vec<&str> = inner.split(',').collect();
                    if parts.len() >= 3 {
                        let vals: Vec<f64> = parts
                            .iter()
                            .filter_map(|s| {
                                let expr = s.trim();
                                eval_function_with_vars(expr, t, &vars).ok().or_else(|| {
                                    evaluate(
                                        expr,
                                        &vars
                                            .iter()
                                            .map(|(k, v)| (k.clone(), *v))
                                            .collect::<Vec<_>>(),
                                    )
                                    .ok()
                                })
                            })
                            .collect();
                        if vals.len() >= 3 {
                            pts.push(Point3D::new(vals[0], vals[1], vals[2]));
                        }
                    }
                }
                if pts.len() >= 2 {
                    let mut segs = Vec::new();
                    for i in 1..pts.len() {
                        segs.push((pts[i - 1], pts[i]));
                    }
                    for (a, b) in &segs {
                        document.add_object(GeoObject::Segment3D(
                            Segment3DObj::new(*a, *b).with_label("C3"),
                        ));
                    }
                }
                input_text.clear();
                return CommandOutcome::Ok;
            }
            "SetValue" if cmd.args.len() == 2 => {
                if let Some(id) = find_object_by_label(document, &cmd.args[0]) {
                    if let Ok(val) = parse_numeric_arg(&cmd.args[1], &document.variables) {
                        document.set_variable(cmd.args[0].trim().to_string(), val);
                    } else if let Ok((x, y)) = parse_point_str(&cmd.args[1]) {
                        if let Some(GeoObject::Point(p)) = document.get_object_mut(id) {
                            p.position = Point2::new(x, y);
                            let order = document.propagation_order(&[id]);
                            if !order.is_empty() {
                                document.re_evaluate_constraints(&order);
                            }
                        }
                    }
                }
                input_text.clear();
                return CommandOutcome::Ok;
            }
            "Extrude" if cmd.args.len() >= 2 => {
                let height: f64 = cmd
                    .args
                    .get(1)
                    .and_then(|s| s.trim().parse().ok())
                    .unwrap_or(1.0);
                let id_opt = find_object_by_label(document, &cmd.args[0]);
                let vertices = id_opt.and_then(|id| {
                    document.get_object(id).and_then(|obj| {
                        if let GeoObject::Polygon(poly) = obj {
                            if poly.vertices.len() >= 3 {
                                Some(poly.vertices.clone())
                            } else {
                                None
                            }
                        } else {
                            None
                        }
                    })
                });
                if let Some(verts) = vertices {
                    let base_y = 0.0;
                    let top_y = height;
                    for i in 0..verts.len() {
                        let v = verts[i];
                        let vn = verts[(i + 1) % verts.len()];
                        let b = Point3D::new(v.x, base_y, v.y);
                        let t = Point3D::new(v.x, top_y, v.y);
                        let bn = Point3D::new(vn.x, base_y, vn.y);
                        let tn = Point3D::new(vn.x, top_y, vn.y);
                        if let Some(poly_id) = id_opt {
                            let mut params = HashMap::new();
                            params.insert("height".to_string(), height);
                            let (_, _c1) = document.add_constructed_object_with_params(
                                GeoObject::Segment3D(Segment3DObj::new(b, t).with_label("E")),
                                "Extrude",
                                &[poly_id],
                                params.clone(),
                            );
                            let (_, _c2) = document.add_constructed_object_with_params(
                                GeoObject::Segment3D(Segment3DObj::new(b, bn).with_label("E")),
                                "Extrude",
                                &[poly_id],
                                params.clone(),
                            );
                            let (_, _c3) = document.add_constructed_object_with_params(
                                GeoObject::Segment3D(Segment3DObj::new(t, tn).with_label("E")),
                                "Extrude",
                                &[poly_id],
                                params,
                            );
                        }
                    }
                } else {
                    result = CommandOutcome::Error(
                        "Extrude only supports Polygons with 3+ vertices".into(),
                    );
                }
                input_text.clear();
                return result;
            }
            "Script" if !cmd.args.is_empty() => {
                const MAX_SCRIPT_COMMANDS: usize = 100;
                const MAX_SCRIPT_DEPTH: u32 = 5;
                // Detectar recursión de Script anidados
                let script_count = cmd.args[0].matches("Script[").count();
                if script_count > MAX_SCRIPT_DEPTH as usize {
                    result =
                        CommandOutcome::Error("Script: profundidad de anidamiento excedida".into());
                    input_text.clear();
                    return result;
                }
                let commands: Vec<String> = cmd.args[0]
                    .split(';')
                    .map(|s| s.trim().to_string())
                    .filter(|s| !s.is_empty())
                    .take(MAX_SCRIPT_COMMANDS)
                    .collect();
                if commands.len() == MAX_SCRIPT_COMMANDS
                    && cmd.args[0]
                        .split(';')
                        .filter(|s| !s.trim().is_empty())
                        .count()
                        > MAX_SCRIPT_COMMANDS
                {
                    result = CommandOutcome::Error("Script contains too many commands".into());
                    input_text.clear();
                    return result;
                }
                let mut output = String::new();
                for c in &commands {
                    let mut temp = c.clone();
                    match process_input(document, &mut temp) {
                        CommandOutcome::Message(msg) | CommandOutcome::Error(msg) => {
                            output.push_str(&msg);
                            output.push('\n');
                        }
                        CommandOutcome::Ok => {}
                    }
                }
                result = if output.is_empty() {
                    CommandOutcome::Message("Script executed".into())
                } else {
                    CommandOutcome::Message(output)
                };
                input_text.clear();
                return result;
            }
            "Simplify" if !cmd.args.is_empty() => {
                let expr = cmd.args[0].trim();
                let vars: Vec<(String, f64)> = document
                    .variables
                    .iter()
                    .map(|(k, v)| (k.clone(), *v))
                    .collect();
                match evaluate(expr, &vars) {
                    Ok(val) => result = CommandOutcome::Message(format!("{} ≈ {}", expr, val)),
                    Err(e) => result = CommandOutcome::Error(format!("Simplify error: {}", e)),
                }
                input_text.clear();
                return result;
            }
            "Lorenz" => {
                let params = if cmd.args.is_empty() || cmd.args[0].trim().is_empty() {
                    vec![10.0, 28.0, 8.0 / 3.0]
                } else {
                    parse_attractor_params(&cmd.args)
                };
                if params.is_empty() {
                    input_text.clear();
                    return CommandOutcome::Error("Error: Invalid parameters for Lorenz. Use: Lorenz[sigma, rho, beta] or Lorenz[sigma=10, rho=28, beta=8/3]".into());
                }
                let obj = GeoObject::Attractor3D(Attractor3DObj::new("lorenz", params));
                document.add_object(obj);
                input_text.clear();
                return CommandOutcome::Message("Lorenz attractor created".into());
            }
            "Rossler" => {
                let params = if cmd.args.is_empty() || cmd.args[0].trim().is_empty() {
                    vec![0.2, 0.2, 5.7]
                } else {
                    parse_attractor_params(&cmd.args)
                };
                let obj = GeoObject::Attractor3D(Attractor3DObj::new("rossler", params));
                document.add_object(obj);
                input_text.clear();
                return CommandOutcome::Message("Rössler attractor created".into());
            }
            "Thomas" | "Butterfly" => {
                let params = if cmd.args.is_empty() || cmd.args[0].trim().is_empty() {
                    vec![0.208186]
                } else {
                    parse_attractor_params(&cmd.args)
                };
                let obj = GeoObject::Attractor3D(Attractor3DObj::new("thomas", params));
                document.add_object(obj);
                input_text.clear();
                return CommandOutcome::Message("Thomas butterfly attractor created".into());
            }
            "Aizawa" => {
                let params = if cmd.args.is_empty() || cmd.args[0].trim().is_empty() {
                    vec![0.95, 0.7, 0.6, 3.5, 0.25, 0.1]
                } else {
                    parse_attractor_params(&cmd.args)
                };
                let obj = GeoObject::Attractor3D(Attractor3DObj::new("aizawa", params));
                document.add_object(obj);
                input_text.clear();
                return CommandOutcome::Message("Aizawa attractor created".into());
            }
            "Chen" => {
                let params = if cmd.args.is_empty() || cmd.args[0].trim().is_empty() {
                    vec![35.0, 3.0, 28.0]
                } else {
                    parse_attractor_params(&cmd.args)
                };
                let obj = GeoObject::Attractor3D(Attractor3DObj::new("chen", params));
                document.add_object(obj);
                input_text.clear();
                return CommandOutcome::Message("Chen attractor created".into());
            }
            "Halvorsen" => {
                let params = if cmd.args.is_empty() || cmd.args[0].trim().is_empty() {
                    vec![1.4, 0.0, 0.0, 0.0]
                } else {
                    parse_attractor_params(&cmd.args)
                };
                let obj = GeoObject::Attractor3D(Attractor3DObj::new("halvorsen", params));
                document.add_object(obj);
                input_text.clear();
                return CommandOutcome::Message("Halvorsen attractor created".into());
            }
            "Dadras" => {
                let params = if cmd.args.is_empty() || cmd.args[0].trim().is_empty() {
                    vec![3.0, 2.7, 1.7, 2.0, 9.0]
                } else {
                    parse_attractor_params(&cmd.args)
                };
                let obj = GeoObject::Attractor3D(Attractor3DObj::new("dadras", params));
                document.add_object(obj);
                input_text.clear();
                return CommandOutcome::Message("Dadras attractor created".into());
            }
            "Chua" => {
                let params = if cmd.args.is_empty() || cmd.args[0].trim().is_empty() {
                    vec![15.6, 28.0, -1.143, -0.714]
                } else {
                    parse_attractor_params(&cmd.args)
                };
                let obj = GeoObject::Attractor3D(Attractor3DObj::new("chua", params));
                document.add_object(obj);
                input_text.clear();
                return CommandOutcome::Message("Chua attractor created".into());
            }
            "Mandelbrot" => {
                let max_iter = cmd
                    .args
                    .first()
                    .and_then(|s| s.trim().parse().ok())
                    .unwrap_or(256);
                let obj = GeoObject::Fractal2D(Fractal2DObj::mandelbrot().with_max_iter(max_iter));
                document.add_object(obj);
                input_text.clear();
                return CommandOutcome::Message("Mandelbrot fractal created".into());
            }
            "Julia" if cmd.args.len() >= 2 => {
                let cr: f64 = cmd.args[0].trim().parse().unwrap_or(-0.70176);
                let ci: f64 = cmd.args[1].trim().parse().unwrap_or(-0.3842);
                let max_iter = cmd
                    .args
                    .get(2)
                    .and_then(|s| s.trim().parse().ok())
                    .unwrap_or(256);
                let obj = GeoObject::Fractal2D(Fractal2DObj::julia(cr, ci).with_max_iter(max_iter));
                document.add_object(obj);
                input_text.clear();
                return CommandOutcome::Message(format!("Julia set c={cr}+{ci}i created"));
            }
            "BurningShip" => {
                let obj = GeoObject::Fractal2D(Fractal2DObj::burning_ship());
                document.add_object(obj);
                input_text.clear();
                return CommandOutcome::Message("Burning Ship fractal created".into());
            }
            "Hypercube" => {
                let angles = if cmd.args.len() >= 3 {
                    parse_attractor_params(&cmd.args)
                } else {
                    vec![0.3, 0.5, 0.7]
                };
                let obj =
                    GeoObject::HyperSurface4D(HyperSurface4DObj::hypercube().with_rotation(angles));
                document.add_object(obj);
                input_text.clear();
                return CommandOutcome::Message("Hipercubo 4D creado (escala=3.0). Botón derecho para orbitar, scroll para zoom.".into());
            }
            "Hypersphere" => {
                let angles = if cmd.args.len() >= 3 {
                    parse_attractor_params(&cmd.args)
                } else {
                    vec![0.3, 0.5, 0.7]
                };
                let obj = GeoObject::HyperSurface4D(
                    HyperSurface4DObj::hypersphere().with_rotation(angles),
                );
                document.add_object(obj);
                input_text.clear();
                return CommandOutcome::Message("Hiperesfera 4D creada (escala=3.0). Botón derecho para orbitar, scroll para zoom.".into());
            }
            "VectorField3D" if cmd.args.len() >= 3 => {
                let obj = GeoObject::VectorField3D(VectorField3DObj::new(
                    cmd.args[0].trim(),
                    cmd.args[1].trim(),
                    cmd.args[2].trim(),
                ));
                document.add_object(obj);
                input_text.clear();
                return CommandOutcome::Message("3D Vector Field created".into());
            }
            "Histogram" if !cmd.args.is_empty() => {
                let data = parse_brace_list(&cmd.args[0]);
                let bins = cmd
                    .args
                    .get(1)
                    .and_then(|s| s.trim().parse().ok())
                    .unwrap_or(10);
                if !data.is_empty() {
                    let obj = GeoObject::Histogram(HistogramObj::new(data, bins));
                    document.add_object(obj);
                    input_text.clear();
                    return CommandOutcome::Message("Histogram created".into());
                }
            }
            "Image" if !cmd.args.is_empty() => {
                let path = cmd.args[0].trim().trim_matches('"').to_string();
                // La decodificación de la imagen se delega al renderer; aquí
                // registramos el path como un TextObj informativo para que
                // aparezca en el panel de álgebra y se pueda referenciar por
                // etiqueta. Cuando se añada `ImageObj` al modelo, esta lógica
                // se sustituye por `document.add_object(ImageObj::new(path))`.
                let mut info = PointObj::new(Point2::new(0.0, 0.0));
                info.label = format!("Image[{}]", path);
                document.add_object(GeoObject::Point(info));
                input_text.clear();
                return CommandOutcome::Message(format!(
                    "Image: '{}' registrada (decode en el renderer)",
                    path
                ));
            }
            "Erase" if cmd.args.len() == 1 => {
                // Erase[label] borra el objeto con la etiqueta dada.
                let label = cmd.args[0].trim();
                if let Some(id) = find_object_by_label(document, label) {
                    document.remove_object(id);
                    input_text.clear();
                    return CommandOutcome::Message(format!("Erase: '{}' borrado", label));
                }
                return CommandOutcome::Error(format!("Erase: objeto '{}' no encontrado", label));
            }
            "EraseAll" => {
                // Borra todos los objetos visibles.
                let ids: Vec<ObjectId> = document.objects_iter().map(|(id, _)| *id).collect();
                let n = ids.len();
                for id in ids {
                    document.remove_object(id);
                }
                input_text.clear();
                return CommandOutcome::Message(format!("EraseAll: {} objeto(s) borrado(s)", n));
            }
            "ErasePencil" => {
                // Borra todos los PencilObj (trazos a mano alzada). Útil
                // cuando un PencilObj persistido genera trazos molestos
                // que el usuario no logra identificar visualmente.
                let ids: Vec<ObjectId> = document
                    .objects_iter()
                    .filter_map(|(id, obj)| {
                        if matches!(obj, GeoObject::Pencil(_)) {
                            Some(*id)
                        } else {
                            None
                        }
                    })
                    .collect();
                let n = ids.len();
                for id in ids {
                    document.remove_object(id);
                }
                input_text.clear();
                return CommandOutcome::Message(format!("ErasePencil: {} trazo(s) borrado(s)", n));
            }
            "EraseLine" => {
                // Borra todas las LineObj con kind=Line (rectas infinitas).
                // Útil para limpiar líneas persistidas que cruzan el canvas
                // de borde a borde.
                let ids: Vec<ObjectId> = document
                    .objects_iter()
                    .filter_map(|(id, obj)| {
                        if let GeoObject::Line(l) = obj {
                            if matches!(l.kind, grafito_core::LineKind::Line) {
                                return Some(*id);
                            }
                        }
                        None
                    })
                    .collect();
                let n = ids.len();
                for id in ids {
                    document.remove_object(id);
                }
                input_text.clear();
                return CommandOutcome::Message(format!(
                    "EraseLine: {} recta(s) infinita(s) borrada(s)",
                    n
                ));
            }
            "EraseVertical" => {
                // Borra LineObj y PencilObj que tengan una dirección
                // vertical pura (x1 ≈ x2). Mantenido por compatibilidad
                // para limpiar documentos con trazos verticales no deseados.
                let ids: Vec<ObjectId> = document
                    .objects_iter()
                    .filter_map(|(id, obj)| match obj {
                        GeoObject::Line(l) => {
                            if (l.start.x - l.end.x).abs() < 1e-6 {
                                Some(*id)
                            } else {
                                None
                            }
                        }
                        GeoObject::Pencil(p) if p.points.len() == 2 => {
                            let a = p.points[0];
                            let b = p.points[1];
                            if (a.x - b.x).abs() < 1e-6 {
                                Some(*id)
                            } else {
                                None
                            }
                        }
                        _ => None,
                    })
                    .collect();
                let n = ids.len();
                for id in ids {
                    document.remove_object(id);
                }
                input_text.clear();
                return CommandOutcome::Message(format!(
                    "EraseVertical: {} línea(s) vertical(es) borrada(s)",
                    n
                ));
            }
            "Clean" => {
                // Atajo de limpieza: borra todos los PencilObj, todas las
                // LineObj con kind=Line, y todas las líneas verticales
                // (cualquier kind) que el usuario no pidió.
                let ids: Vec<ObjectId> = document
                    .objects_iter()
                    .filter_map(|(id, obj)| match obj {
                        GeoObject::Pencil(_) => Some(*id),
                        GeoObject::Line(l) => {
                            // Cualquier LineObj con kind=Line (infinita)
                            // o con dirección vertical/horizontal pura
                            // (línea de borde a borde) se considera
                            // candidata a limpieza.
                            if matches!(l.kind, grafito_core::LineKind::Line)
                                || (l.start.x - l.end.x).abs() < 1e-6
                                || (l.start.y - l.end.y).abs() < 1e-6
                            {
                                Some(*id)
                            } else {
                                None
                            }
                        }
                        _ => None,
                    })
                    .collect();
                let n = ids.len();
                for id in ids {
                    document.remove_object(id);
                }
                input_text.clear();
                return CommandOutcome::Message(format!(
                    "Clean: {} objeto(s) borrado(s) (Pencil + rectas infinitas + verticales)",
                    n
                ));
            }
            "ScatterPlot" if cmd.args.len() >= 2 => {
                let xs = parse_brace_list(&cmd.args[0]);
                let ys = parse_brace_list(&cmd.args[1]);
                if !xs.is_empty() && xs.len() == ys.len() {
                    let obj = GeoObject::ScatterPlot(ScatterPlotObj::new(xs, ys));
                    document.add_object(obj);
                    input_text.clear();
                    return CommandOutcome::Message("Scatter plot created".into());
                }
            }
            "BoxPlot" if !cmd.args.is_empty() => {
                let data = parse_brace_list(&cmd.args[0]);
                if !data.is_empty() {
                    let obj = GeoObject::BoxPlot(BoxPlotObj::new(data));
                    document.add_object(obj);
                    input_text.clear();
                    return CommandOutcome::Message("Box plot created".into());
                }
            }
            "LinearRegression" if cmd.args.len() >= 2 => {
                let xs = parse_brace_list(&cmd.args[0]);
                let ys = parse_brace_list(&cmd.args[1]);
                if !xs.is_empty() && xs.len() == ys.len() {
                    if let Some((slope, intercept, r2)) = statistics::linear_regression(&xs, &ys) {
                        let obj = GeoObject::RegressionLine(RegressionLineObj::linear(
                            xs, ys, slope, intercept, r2,
                        ));
                        document.add_object(obj);
                        input_text.clear();
                        return CommandOutcome::Message(format!(
                            "y = {:.4}x + {:.4}, R²={:.4}",
                            slope, intercept, r2
                        ));
                    }
                }
            }
            "Mean" if !cmd.args.is_empty() => {
                let data = parse_brace_list(&cmd.args[0]);
                if let Some(m) = statistics::mean(&data) {
                    input_text.clear();
                    return CommandOutcome::Message(format!("Mean = {:.6}", m));
                }
            }
            "Median" if !cmd.args.is_empty() => {
                let data = parse_brace_list(&cmd.args[0]);
                if let Some(m) = statistics::median(&data) {
                    input_text.clear();
                    return CommandOutcome::Message(format!("Median = {:.6}", m));
                }
            }
            "StdDev" if !cmd.args.is_empty() => {
                let data = parse_brace_list(&cmd.args[0]);
                if let Some(s) = statistics::std_dev(&data) {
                    input_text.clear();
                    return CommandOutcome::Message(format!("StdDev = {:.6}", s));
                }
            }
            "Correlation" if cmd.args.len() >= 2 => {
                let xs = parse_brace_list(&cmd.args[0]);
                let ys = parse_brace_list(&cmd.args[1]);
                if let Some(r) = statistics::pearson_correlation(&xs, &ys) {
                    input_text.clear();
                    return CommandOutcome::Message(format!("r = {:.6}", r));
                }
            }
            "Determinant" if !cmd.args.is_empty() => {
                if let Some(m) = parse_matrix_arg(&cmd.args[0]) {
                    if let Some(det) = m.determinant() {
                        input_text.clear();
                        return CommandOutcome::Message(format!("det = {:.6}", det));
                    }
                }
            }
            "Inverse" if !cmd.args.is_empty() => {
                if let Some(m) = parse_matrix_arg(&cmd.args[0]) {
                    if let Some(inv) = m.inverse() {
                        input_text.clear();
                        return CommandOutcome::Message(format!("Inverse:\n{}", inv));
                    }
                }
            }
            "Taylor" if cmd.args.len() >= 2 => {
                let expr = cmd.args[0].trim();
                let var = cmd.args.get(1).map(|s| s.trim()).unwrap_or("x");
                let center = cmd
                    .args
                    .get(2)
                    .and_then(|s| s.trim().parse().ok())
                    .unwrap_or(0.0);
                let order = cmd
                    .args
                    .get(3)
                    .and_then(|s| s.trim().parse().ok())
                    .unwrap_or(5);
                if let Some(series) = taylor_series(expr, var, center, order) {
                    let label = next_function_label(document);
                    let obj = GeoObject::Function(FunctionObj::new(&series).with_label(&label));
                    document.add_object(obj);
                    input_text.clear();
                    return CommandOutcome::Message(format!("Taylor: {} → {}", series, label));
                }
            }
            "Cardioid" if !cmd.args.is_empty() => {
                let a: f64 = cmd.args[0].trim().parse().unwrap_or(1.0);
                let steps = 200;
                let points = grafito_geometry::special_curves::cardioid(a, steps);
                if points.len() >= 3 {
                    let mut poly = PolygonObj::new(points);
                    poly.label = "Cardioid".to_string();
                    document.add_object(GeoObject::Polygon(poly));
                }
                input_text.clear();
                return CommandOutcome::Message(format!("Cardioid(a={}) created", a));
            }
            "Rose" if cmd.args.len() >= 3 => {
                let a: f64 = cmd.args[0].trim().parse().unwrap_or(1.0);
                let n: i32 = cmd.args[1].trim().parse().unwrap_or(3);
                let d: i32 = cmd.args[2].trim().parse().unwrap_or(1);
                let steps = 400;
                let points = grafito_geometry::special_curves::rose(a, n, d, steps);
                if points.len() >= 3 {
                    let mut poly = PolygonObj::new(points);
                    poly.label = format!("Rose({}/{})", n, d);
                    document.add_object(GeoObject::Polygon(poly));
                }
                input_text.clear();
                return CommandOutcome::Message(format!("Rose(a={}, n={}, d={}) created", a, n, d));
            }
            "ArchimedeanSpiral" if cmd.args.len() >= 3 => {
                let a: f64 = cmd.args[0].trim().parse().unwrap_or(0.0);
                let b: f64 = cmd.args[1].trim().parse().unwrap_or(0.1);
                let max_theta: f64 = cmd.args[2].trim().parse().unwrap_or(20.0);
                let steps = 300;
                let points =
                    grafito_geometry::special_curves::archimedean_spiral(a, b, max_theta, steps);
                if points.len() >= 3 {
                    let mut poly = PolygonObj::new(points);
                    poly.label = "Spiral".to_string();
                    document.add_object(GeoObject::Polygon(poly));
                }
                input_text.clear();
                return CommandOutcome::Message(format!(
                    "Archimedean Spiral(a={}, b={}, θ={}) created",
                    a, b, max_theta
                ));
            }
            "LogarithmicSpiral" if cmd.args.len() >= 3 => {
                let a: f64 = cmd.args[0].trim().parse().unwrap_or(1.0);
                let b: f64 = cmd.args[1].trim().parse().unwrap_or(0.1);
                let max_theta: f64 = cmd.args[2].trim().parse().unwrap_or(10.0);
                let steps = 300;
                let points =
                    grafito_geometry::special_curves::logarithmic_spiral(a, b, max_theta, steps);
                if points.len() >= 3 {
                    let mut poly = PolygonObj::new(points);
                    poly.label = "LogSpiral".to_string();
                    document.add_object(GeoObject::Polygon(poly));
                }
                input_text.clear();
                return CommandOutcome::Message(format!(
                    "Logarithmic Spiral(a={}, b={}, θ={}) created",
                    a, b, max_theta
                ));
            }
            "Lissajous" if cmd.args.len() >= 5 => {
                let a: f64 = cmd.args[0].trim().parse().unwrap_or(1.0);
                let b: f64 = cmd.args[1].trim().parse().unwrap_or(1.0);
                let freq_x: f64 = cmd.args[2].trim().parse().unwrap_or(3.0);
                let freq_y: f64 = cmd.args[3].trim().parse().unwrap_or(2.0);
                let delta: f64 = cmd.args[4].trim().parse().unwrap_or(0.0);
                let steps = 400;
                let points =
                    grafito_geometry::special_curves::lissajous(a, b, freq_x, freq_y, delta, steps);
                if points.len() >= 3 {
                    let mut poly = PolygonObj::new(points);
                    poly.label = "Lissajous".to_string();
                    document.add_object(GeoObject::Polygon(poly));
                }
                input_text.clear();
                return CommandOutcome::Message(format!(
                    "Lissajous(a={}, b={}, fx={}, fy={}, δ={}) created",
                    a, b, freq_x, freq_y, delta
                ));
            }
            "Epicycloid" if cmd.args.len() >= 2 => {
                let r: f64 = cmd.args[0].trim().parse().unwrap_or(1.0);
                let k: f64 = cmd.args[1].trim().parse().unwrap_or(3.0);
                let steps = 400;
                let points = grafito_geometry::special_curves::epicycloid(r, k, steps);
                if points.len() >= 3 {
                    let mut poly = PolygonObj::new(points);
                    poly.label = "Epicycloid".to_string();
                    document.add_object(GeoObject::Polygon(poly));
                }
                input_text.clear();
                return CommandOutcome::Message(format!("Epicycloid(r={}, k={}) created", r, k));
            }
            "Hypocycloid" if cmd.args.len() >= 2 => {
                let r: f64 = cmd.args[0].trim().parse().unwrap_or(1.0);
                let k: f64 = cmd.args[1].trim().parse().unwrap_or(4.0);
                let steps = 400;
                let points = grafito_geometry::special_curves::hypocycloid(r, k, steps);
                if points.len() >= 3 {
                    let mut poly = PolygonObj::new(points);
                    poly.label = "Hypocycloid".to_string();
                    document.add_object(GeoObject::Polygon(poly));
                }
                input_text.clear();
                return CommandOutcome::Message(format!("Hypocycloid(r={}, k={}) created", r, k));
            }
            "ODE" if cmd.args.len() >= 4 => {
                let expr = cmd.args[0].trim();
                let t0: f64 = cmd.args[1].trim().parse().unwrap_or(0.0);
                let y0: f64 = cmd.args[2].trim().parse().unwrap_or(1.0);
                let t_end: f64 = cmd.args[3].trim().parse().unwrap_or(10.0);
                let steps: usize = cmd
                    .args
                    .get(4)
                    .and_then(|s| s.trim().parse().ok())
                    .unwrap_or(200);
                let method = cmd
                    .args
                    .get(5)
                    .map(|s| s.trim().to_lowercase())
                    .unwrap_or("rk4".to_string());

                let f = |t: f64, y: f64| -> f64 {
                    let mut vars = document.variables.clone();
                    vars.insert("t".to_string(), t);
                    vars.insert("y".to_string(), y);
                    evaluate(
                        expr,
                        &vars
                            .iter()
                            .map(|(k, v)| (k.clone(), *v))
                            .collect::<Vec<_>>(),
                    )
                    .unwrap_or(0.0)
                };

                let solution = if method == "euler" {
                    grafito_geometry::ode::euler(f, t0, y0, t_end, steps)
                } else {
                    grafito_geometry::ode::runge_kutta_4(f, t0, y0, t_end, steps)
                };

                let points = grafito_geometry::ode::solution_to_points(&solution);
                if points.len() >= 3 {
                    let mut poly = PolygonObj::new(points);
                    poly.label = format!("ODE({})", method);
                    document.add_object(GeoObject::Polygon(poly));
                }
                input_text.clear();
                return CommandOutcome::Message(format!(
                    "ODE solved with {} method ({} steps)",
                    method, steps
                ));
            }
            "ODESystem" if cmd.args.len() >= 5 => {
                let expr1 = cmd.args[0].trim();
                let expr2 = cmd.args[1].trim();
                let t0: f64 = parse_numeric_arg(&cmd.args[2], &document.variables).unwrap_or(0.0);
                let y0_1: f64 = parse_numeric_arg(&cmd.args[3], &document.variables).unwrap_or(1.0);
                let y0_2: f64 = parse_numeric_arg(&cmd.args[4], &document.variables).unwrap_or(0.0);
                let t_end: f64 = cmd
                    .args
                    .get(5)
                    .and_then(|s| parse_numeric_arg(s, &document.variables).ok())
                    .unwrap_or(10.0);
                let steps: usize = cmd
                    .args
                    .get(6)
                    .and_then(|s| s.trim().parse().ok())
                    .unwrap_or(200);
                let method = cmd
                    .args
                    .get(7)
                    .map(|s| s.trim().to_lowercase())
                    .unwrap_or("rk4".to_string());

                let f = |_t: f64, state: &[f64]| -> Vec<f64> {
                    let mut vars = document.variables.clone();
                    vars.insert("x".to_string(), state[0]);
                    vars.insert("y".to_string(), state[1]);
                    let dy1 = evaluate(
                        expr1,
                        &vars
                            .iter()
                            .map(|(k, v)| (k.clone(), *v))
                            .collect::<Vec<_>>(),
                    )
                    .unwrap_or(0.0);
                    let dy2 = evaluate(
                        expr2,
                        &vars
                            .iter()
                            .map(|(k, v)| (k.clone(), *v))
                            .collect::<Vec<_>>(),
                    )
                    .unwrap_or(0.0);
                    vec![dy1, dy2]
                };

                let solution = if method == "euler" {
                    grafito_geometry::ode::euler_system(f, t0, vec![y0_1, y0_2], t_end, steps)
                } else {
                    grafito_geometry::ode::runge_kutta_4_system(
                        f,
                        t0,
                        vec![y0_1, y0_2],
                        t_end,
                        steps,
                    )
                };

                // Plot y1 vs y2 (phase portrait)
                let points: Vec<Point2> = solution
                    .iter()
                    .map(|(_, state)| Point2::new(state[0], state[1]))
                    .collect();

                if points.len() >= 3 {
                    let mut poly = PolygonObj::new(points);
                    poly.label = format!("Phase({})", method);
                    document.add_object(GeoObject::Polygon(poly));
                }
                input_text.clear();
                return CommandOutcome::Message(format!(
                    "ODE system solved with {} method ({} steps)",
                    method, steps
                ));
            }
            "Gamma" if !cmd.args.is_empty() => {
                let x: f64 = cmd.args[0].trim().parse().unwrap_or(1.0);
                let result = grafito_geometry::special_functions::gamma(x);
                input_text.clear();
                return CommandOutcome::Message(format!("Γ({}) = {:.6}", x, result));
            }
            "LnGamma" if !cmd.args.is_empty() => {
                let x: f64 = cmd.args[0].trim().parse().unwrap_or(1.0);
                let result = grafito_geometry::special_functions::ln_gamma(x);
                input_text.clear();
                return CommandOutcome::Message(format!("ln(Γ({})) = {:.6}", x, result));
            }
            "Beta" if cmd.args.len() >= 2 => {
                let a: f64 = cmd.args[0].trim().parse().unwrap_or(1.0);
                let b: f64 = cmd.args[1].trim().parse().unwrap_or(1.0);
                let result = grafito_geometry::special_functions::beta(a, b);
                input_text.clear();
                return CommandOutcome::Message(format!("B({}, {}) = {:.6}", a, b, result));
            }
            "BesselJ" if cmd.args.len() >= 2 => {
                let n: i32 = cmd.args[0].trim().parse().unwrap_or(0);
                let x: f64 = cmd.args[1].trim().parse().unwrap_or(1.0);
                let result = grafito_geometry::special_functions::bessel_j(n, x);
                input_text.clear();
                return CommandOutcome::Message(format!("J_{}({}) = {:.6}", n, x, result));
            }
            "BesselY" if cmd.args.len() >= 2 => {
                let n: i32 = cmd.args[0].trim().parse().unwrap_or(0);
                let x: f64 = cmd.args[1].trim().parse().unwrap_or(1.0);
                let result = grafito_geometry::special_functions::bessel_y(n, x);
                input_text.clear();
                return CommandOutcome::Message(format!("Y_{}({}) = {:.6}", n, x, result));
            }
            "BesselI" if cmd.args.len() >= 2 => {
                let n: i32 = cmd.args[0].trim().parse().unwrap_or(0);
                let x: f64 = cmd.args[1].trim().parse().unwrap_or(1.0);
                let result = grafito_geometry::special_functions::bessel_i(n, x);
                input_text.clear();
                return CommandOutcome::Message(format!("I_{}({}) = {:.6}", n, x, result));
            }
            "Erf" if !cmd.args.is_empty() => {
                let x: f64 = cmd.args[0].trim().parse().unwrap_or(0.0);
                let result = grafito_geometry::special_functions::erf(x);
                input_text.clear();
                return CommandOutcome::Message(format!("erf({}) = {:.6}", x, result));
            }
            "Erfc" if !cmd.args.is_empty() => {
                let x: f64 = cmd.args[0].trim().parse().unwrap_or(0.0);
                let result = grafito_geometry::special_functions::erfc(x);
                input_text.clear();
                return CommandOutcome::Message(format!("erfc({}) = {:.6}", x, result));
            }
            "Digamma" if !cmd.args.is_empty() => {
                let x: f64 = cmd.args[0].trim().parse().unwrap_or(1.0);
                let result = grafito_geometry::special_functions::digamma(x);
                input_text.clear();
                return CommandOutcome::Message(format!("ψ({}) = {:.6}", x, result));
            }
            "Uniform" if cmd.args.len() >= 2 => {
                let a: f64 = cmd.args[0].trim().parse().unwrap_or(0.0);
                let b: f64 = cmd.args[1].trim().parse().unwrap_or(1.0);
                let x: f64 = cmd
                    .args
                    .get(2)
                    .and_then(|s| s.trim().parse().ok())
                    .unwrap_or(0.5);
                let pdf = grafito_geometry::statistics::uniform_pdf(x, a, b);
                let cdf = grafito_geometry::statistics::uniform_cdf(x, a, b);
                input_text.clear();
                return CommandOutcome::Message(format!(
                    "U({},{}): PDF({}) = {:.6}, CDF({}) = {:.6}",
                    a, b, x, pdf, x, cdf
                ));
            }
            "GammaDist" if cmd.args.len() >= 2 => {
                let alpha: f64 = cmd.args[0].trim().parse().unwrap_or(1.0);
                let beta: f64 = cmd.args[1].trim().parse().unwrap_or(1.0);
                let x: f64 = cmd
                    .args
                    .get(2)
                    .and_then(|s| s.trim().parse().ok())
                    .unwrap_or(1.0);
                let pdf = grafito_geometry::statistics::gamma_pdf(x, alpha, beta);
                input_text.clear();
                return CommandOutcome::Message(format!(
                    "Gamma({},{}): PDF({}) = {:.6}",
                    alpha, beta, x, pdf
                ));
            }
            "BetaDist" if cmd.args.len() >= 2 => {
                let alpha: f64 = cmd.args[0].trim().parse().unwrap_or(1.0);
                let beta: f64 = cmd.args[1].trim().parse().unwrap_or(1.0);
                let x: f64 = cmd
                    .args
                    .get(2)
                    .and_then(|s| s.trim().parse().ok())
                    .unwrap_or(0.5);
                let pdf = grafito_geometry::statistics::beta_pdf(x, alpha, beta);
                input_text.clear();
                return CommandOutcome::Message(format!(
                    "Beta({},{}): PDF({}) = {:.6}",
                    alpha, beta, x, pdf
                ));
            }
            "Cauchy" if cmd.args.len() >= 2 => {
                let x0: f64 = cmd.args[0].trim().parse().unwrap_or(0.0);
                let gamma: f64 = cmd.args[1].trim().parse().unwrap_or(1.0);
                let x: f64 = cmd
                    .args
                    .get(2)
                    .and_then(|s| s.trim().parse().ok())
                    .unwrap_or(0.0);
                let pdf = grafito_geometry::statistics::cauchy_pdf(x, x0, gamma);
                let cdf = grafito_geometry::statistics::cauchy_cdf(x, x0, gamma);
                input_text.clear();
                return CommandOutcome::Message(format!(
                    "Cauchy({},{}): PDF({}) = {:.6}, CDF({}) = {:.6}",
                    x0, gamma, x, pdf, x, cdf
                ));
            }
            "Pareto" if cmd.args.len() >= 2 => {
                let xm: f64 = cmd.args[0].trim().parse().unwrap_or(1.0);
                let alpha: f64 = cmd.args[1].trim().parse().unwrap_or(1.0);
                let x: f64 = cmd
                    .args
                    .get(2)
                    .and_then(|s| s.trim().parse().ok())
                    .unwrap_or(2.0);
                let pdf = grafito_geometry::statistics::pareto_pdf(x, xm, alpha);
                let cdf = grafito_geometry::statistics::pareto_cdf(x, xm, alpha);
                input_text.clear();
                return CommandOutcome::Message(format!(
                    "Pareto({},{}): PDF({}) = {:.6}, CDF({}) = {:.6}",
                    xm, alpha, x, pdf, x, cdf
                ));
            }
            "Rayleigh" if !cmd.args.is_empty() => {
                let sigma: f64 = cmd.args[0].trim().parse().unwrap_or(1.0);
                let x: f64 = cmd
                    .args
                    .get(1)
                    .and_then(|s| s.trim().parse().ok())
                    .unwrap_or(1.0);
                let pdf = grafito_geometry::statistics::rayleigh_pdf(x, sigma);
                let cdf = grafito_geometry::statistics::rayleigh_cdf(x, sigma);
                input_text.clear();
                return CommandOutcome::Message(format!(
                    "Rayleigh({}): PDF({}) = {:.6}, CDF({}) = {:.6}",
                    sigma, x, pdf, x, cdf
                ));
            }
            "Laplace" if cmd.args.len() >= 2 => {
                let mu: f64 = cmd.args[0].trim().parse().unwrap_or(0.0);
                let b: f64 = cmd.args[1].trim().parse().unwrap_or(1.0);
                let x: f64 = cmd
                    .args
                    .get(2)
                    .and_then(|s| s.trim().parse().ok())
                    .unwrap_or(0.0);
                let pdf = grafito_geometry::statistics::laplace_pdf(x, mu, b);
                let cdf = grafito_geometry::statistics::laplace_cdf(x, mu, b);
                input_text.clear();
                return CommandOutcome::Message(format!(
                    "Laplace({},{}): PDF({}) = {:.6}, CDF({}) = {:.6}",
                    mu, b, x, pdf, x, cdf
                ));
            }
            "NegBinomial" if cmd.args.len() >= 2 => {
                let r: u32 = cmd.args[0].trim().parse().unwrap_or(1);
                let p: f64 = cmd.args[1].trim().parse().unwrap_or(0.5);
                let k: u32 = cmd
                    .args
                    .get(2)
                    .and_then(|s| s.trim().parse().ok())
                    .unwrap_or(0);
                let pmf = grafito_geometry::statistics::negative_binomial_pmf(r, p, k);
                let cdf = grafito_geometry::statistics::negative_binomial_cdf(r, p, k);
                input_text.clear();
                return CommandOutcome::Message(format!(
                    "NegBin({},{}): PMF({}) = {:.6}, CDF({}) = {:.6}",
                    r, p, k, pmf, k, cdf
                ));
            }
            "TTest" if cmd.args.len() >= 2 => {
                let data = parse_brace_list(&cmd.args[0]);
                let mu0: f64 = cmd.args[1].trim().parse().unwrap_or(0.0);
                if let Some((t_stat, p_value)) =
                    grafito_geometry::statistics::t_test_one_sample(&data, mu0)
                {
                    input_text.clear();
                    return CommandOutcome::Message(format!(
                        "t-test: t = {:.4}, p = {:.6}",
                        t_stat, p_value
                    ));
                }
            }
            "TTest2" if cmd.args.len() >= 2 => {
                let data1 = parse_brace_list(&cmd.args[0]);
                let data2 = parse_brace_list(&cmd.args[1]);
                if let Some((t_stat, p_value)) =
                    grafito_geometry::statistics::t_test_two_sample(&data1, &data2)
                {
                    input_text.clear();
                    return CommandOutcome::Message(format!(
                        "t-test (2 samples): t = {:.4}, p = {:.6}",
                        t_stat, p_value
                    ));
                }
            }
            "ZTest" if cmd.args.len() >= 3 => {
                let data = parse_brace_list(&cmd.args[0]);
                let mu0: f64 = cmd.args[1].trim().parse().unwrap_or(0.0);
                let sigma: f64 = cmd.args[2].trim().parse().unwrap_or(1.0);
                if let Some((z_stat, p_value)) =
                    grafito_geometry::statistics::z_test_one_sample(&data, mu0, sigma)
                {
                    input_text.clear();
                    return CommandOutcome::Message(format!(
                        "z-test: z = {:.4}, p = {:.6}",
                        z_stat, p_value
                    ));
                }
            }
            "ChiSqTest" if cmd.args.len() >= 2 => {
                let observed = parse_brace_list(&cmd.args[0]);
                let expected = parse_brace_list(&cmd.args[1]);
                if let Some((chi2, p_value)) =
                    grafito_geometry::statistics::chi_squared_test(&observed, &expected)
                {
                    input_text.clear();
                    return CommandOutcome::Message(format!(
                        "χ²-test: χ² = {:.4}, p = {:.6}",
                        chi2, p_value
                    ));
                }
            }
            "ANOVA" if cmd.args.len() >= 2 => {
                let mut groups: Vec<Vec<f64>> = Vec::new();
                for arg in &cmd.args {
                    groups.push(parse_brace_list(arg));
                }
                let group_refs: Vec<&[f64]> = groups.iter().map(|g| g.as_slice()).collect();
                if let Some((f_stat, p_value)) =
                    grafito_geometry::statistics::anova_one_way(&group_refs)
                {
                    input_text.clear();
                    return CommandOutcome::Message(format!(
                        "ANOVA: F = {:.4}, p = {:.6}",
                        f_stat, p_value
                    ));
                }
            }
            "CIMean" if !cmd.args.is_empty() => {
                let data = parse_brace_list(&cmd.args[0]);
                let confidence: f64 = cmd
                    .args
                    .get(1)
                    .and_then(|s| s.trim().parse().ok())
                    .unwrap_or(0.95);
                if let Some((lower, mean, upper)) =
                    grafito_geometry::statistics::confidence_interval_mean(&data, confidence)
                {
                    input_text.clear();
                    return CommandOutcome::Message(format!(
                        "CI ({:.0}%): [{:.4}, {:.4}, {:.4}]",
                        confidence * 100.0,
                        lower,
                        mean,
                        upper
                    ));
                }
            }
            "CIProportion" if cmd.args.len() >= 2 => {
                let successes: u32 = cmd.args[0].trim().parse().unwrap_or(0);
                let n: u32 = cmd.args[1].trim().parse().unwrap_or(1);
                let confidence: f64 = cmd
                    .args
                    .get(2)
                    .and_then(|s| s.trim().parse().ok())
                    .unwrap_or(0.95);
                if let Some((lower, p_hat, upper)) =
                    grafito_geometry::statistics::confidence_interval_proportion(
                        successes, n, confidence,
                    )
                {
                    input_text.clear();
                    return CommandOutcome::Message(format!(
                        "CI ({:.0}%): [{:.4}, {:.4}, {:.4}]",
                        confidence * 100.0,
                        lower,
                        p_hat,
                        upper
                    ));
                }
            }
            "ComplexGrid" if !cmd.args.is_empty() => {
                let x_min = cmd
                    .args
                    .get(1)
                    .and_then(|s| s.trim().parse().ok())
                    .unwrap_or(-5.0);
                let x_max = cmd
                    .args
                    .get(2)
                    .and_then(|s| s.trim().parse().ok())
                    .unwrap_or(5.0);
                let y_min = cmd
                    .args
                    .get(3)
                    .and_then(|s| s.trim().parse().ok())
                    .unwrap_or(-5.0);
                let y_max = cmd
                    .args
                    .get(4)
                    .and_then(|s| s.trim().parse().ok())
                    .unwrap_or(5.0);
                let density: usize = cmd
                    .args
                    .get(5)
                    .and_then(|s| s.trim().parse().ok())
                    .unwrap_or(10);
                let mut cg = ComplexGridObj::new(
                    &format!("{}->z", cmd.args[0].trim()),
                    x_min,
                    x_max,
                    y_min,
                    y_max,
                );
                cg.density = density;
                // Support expr = "f(z)" syntax: strip "f(z)=" prefix
                let expr = cmd.args[0].trim();
                let expr = expr.strip_prefix("f(z)=").unwrap_or(expr);
                let expr = expr.strip_prefix("w=").unwrap_or(expr);
                cg.expr = expr.to_string();
                document.add_object(GeoObject::ComplexGrid(cg));
                input_text.clear();
                return CommandOutcome::Message(
                    "Complex grid created — scroll/zoom to explore".into(),
                );
            }
            "ComplexMapping" if cmd.args.len() == 2 => {
                let expr = cmd.args[0].trim();
                let target_label = cmd.args[1].trim();
                // Aceptar tanto "x" como "x(t)" como "x" simple para tolerar
                // notación matemática (consistente con Root[...]).
                let base_label = target_label
                    .split_once('(')
                    .map(|(id, _)| id.trim())
                    .unwrap_or(target_label);
                let resolved = find_object_by_label(document, target_label)
                    .or_else(|| find_object_by_label(document, base_label));
                match resolved {
                    Some(id) => {
                        let cm = ComplexMappingObj::new_with_symbol(
                            expr,
                            id,
                            document.complex_base_symbol.as_str(),
                        );
                        document.add_object(GeoObject::ComplexMapping(cm));
                        input_text.clear();
                        return CommandOutcome::Message(format!(
                            "ComplexMapping: {expr} sobre {target_label}"
                        ));
                    }
                    None => {
                        let created_target = if target_label == "I" {
                            Some(
                                document.add_object(GeoObject::ImplicitCurve(
                                    ImplicitCurveObj::new("x^2 + y^2", "1", RelationOperator::Less)
                                        .with_label("I"),
                                )),
                            )
                        } else {
                            let before: HashSet<ObjectId> =
                                document.objects_iter().map(|(id, _)| *id).collect();
                            let mut temp_target = target_label.to_string();
                            let _ = process_input(document, &mut temp_target);
                            document
                                .objects_iter()
                                .find_map(|(id, _)| (!before.contains(id)).then_some(*id))
                        };

                        if let Some(id) = created_target {
                            let cm = ComplexMappingObj::new_with_symbol(
                                expr,
                                id,
                                document.complex_base_symbol.as_str(),
                            );
                            document.add_object(GeoObject::ComplexMapping(cm));
                            input_text.clear();
                            return CommandOutcome::Message(format!(
                                "ComplexMapping: {expr} sobre {target_label}"
                            ));
                        }

                        return CommandOutcome::Error(format!(
                            "ComplexMapping: objeto '{target_label}' no encontrado"
                        ));
                    }
                }
            }
            "DomainColoring" if !cmd.args.is_empty() => {
                let x_min = cmd
                    .args
                    .get(1)
                    .and_then(|s| s.trim().parse().ok())
                    .unwrap_or(-5.0);
                let x_max = cmd
                    .args
                    .get(2)
                    .and_then(|s| s.trim().parse().ok())
                    .unwrap_or(5.0);
                let y_min = cmd
                    .args
                    .get(3)
                    .and_then(|s| s.trim().parse().ok())
                    .unwrap_or(-5.0);
                let y_max = cmd
                    .args
                    .get(4)
                    .and_then(|s| s.trim().parse().ok())
                    .unwrap_or(5.0);
                let res: usize = cmd
                    .args
                    .get(5)
                    .and_then(|s| s.trim().parse().ok())
                    .unwrap_or(200);
                let expr = cmd.args[0].trim();
                let expr = expr.strip_prefix("f(z)=").unwrap_or(expr);
                let expr = expr.strip_prefix("w=").unwrap_or(expr);
                let cg = ComplexGridObj::new(expr, x_min, x_max, y_min, y_max).as_domain_coloring();
                let mut cg2 = cg;
                cg2.density = res;
                document.add_object(GeoObject::ComplexGrid(cg2));
                input_text.clear();
                return CommandOutcome::Message(format!(
                    "Domain coloring ({}x{}) created",
                    res, res
                ));
            }
            "HeatMap" if !cmd.args.is_empty() => {
                let x_min = cmd
                    .args
                    .get(1)
                    .and_then(|s| s.trim().parse().ok())
                    .unwrap_or(-5.0);
                let x_max = cmd
                    .args
                    .get(2)
                    .and_then(|s| s.trim().parse().ok())
                    .unwrap_or(5.0);
                let y_min = cmd
                    .args
                    .get(3)
                    .and_then(|s| s.trim().parse().ok())
                    .unwrap_or(-5.0);
                let y_max = cmd
                    .args
                    .get(4)
                    .and_then(|s| s.trim().parse().ok())
                    .unwrap_or(5.0);
                let res: usize = cmd
                    .args
                    .get(5)
                    .and_then(|s| s.trim().parse().ok())
                    .unwrap_or(150);
                let expr = cmd.args[0].trim();
                let expr = expr.strip_prefix("f(x,y)=").unwrap_or(expr);
                let expr = expr.strip_prefix("z=").unwrap_or(expr);
                let cg = ComplexGridObj::new(expr, x_min, x_max, y_min, y_max).as_heat_map();
                let mut cg2 = cg;
                cg2.density = res;
                document.add_object(GeoObject::ComplexGrid(cg2));
                input_text.clear();
                return CommandOutcome::Message(format!("Heat map ({}x{}) created", res, res));
            }
            "ComplexSymbol" if !cmd.args.is_empty() => {
                // Cambia el símbolo base de los números complejos (default "z")
                // a otro (p.ej. "w"). Migra labels y reescribe exprs de
                // ComplexGrid/ComplexMapping existentes.
                let new_sym = cmd.args[0].trim();
                if new_sym.is_empty() {
                    return CommandOutcome::Error("Símbolo vacío".into());
                }
                document.migrate_complex_symbol(new_sym);
                input_text.clear();
                return CommandOutcome::Message(format!("Símbolo base cambiado a '{}'", new_sym));
            }
            "PolarCurve" if cmd.args.len() >= 3 => {
                let expr = cmd.args[0].trim();
                let t_min = cmd.args[1].trim().parse().unwrap_or(0.0);
                let t_max = parse_numeric_arg(&cmd.args[2], &document.variables)
                    .unwrap_or(std::f64::consts::TAU);
                let obj = GeoObject::PolarCurve(PolarCurveObj::new(expr, t_min, t_max));
                document.add_object(obj);
                input_text.clear();
                return CommandOutcome::Message(format!(
                    "Polar curve r = {} [{}..{}]",
                    expr, t_min, t_max
                ));
            }
            "ParametricCurve2D" if cmd.args.len() >= 4 => {
                let expr_x = cmd.args[0].trim();
                let expr_y = cmd.args[1].trim();
                let t_min = parse_numeric_arg(&cmd.args[2], &document.variables).unwrap_or(0.0);
                let t_max = parse_numeric_arg(&cmd.args[3], &document.variables)
                    .unwrap_or(std::f64::consts::TAU);
                let obj = GeoObject::ParametricCurve2D(ParametricCurve2DObj::new(
                    expr_x, expr_y, t_min, t_max,
                ));
                document.add_object(obj);
                input_text.clear();
                return CommandOutcome::Message("Parametric curve created".into());
            }
            "Function" if !cmd.args.is_empty() => {
                let expr = cmd.args.join(", ");
                if expr.trim().is_empty() {
                    input_text.clear();
                    return CommandOutcome::Error("Function: se requiere una expresión".into());
                }
                let label = next_function_label(document);
                document.add_object(GeoObject::Function(
                    FunctionObj::new(&expr).with_label(&label),
                ));
                input_text.clear();
                return CommandOutcome::Message(format!("Función {} → {}", expr, label));
            }
            "Piecewise" if cmd.args.len() >= 3 => {
                let mut expr = format!("piecewise({}", cmd.args[0].trim());
                for a in &cmd.args[1..] {
                    expr.push_str(", ");
                    expr.push_str(a.trim());
                }
                expr.push(')');
                let label = next_function_label(document);
                document.add_object(GeoObject::Function(
                    FunctionObj::new(&expr).with_label(&label),
                ));
                input_text.clear();
                return CommandOutcome::Message(format!("Piecewise function → {}", label));
            }
            "VectorField2D" if cmd.args.len() >= 2 => {
                let obj = GeoObject::VectorField2D(VectorField2DObj::new(
                    cmd.args[0].trim(),
                    cmd.args[1].trim(),
                ));
                document.add_object(obj);
                input_text.clear();
                return CommandOutcome::Message(
                    "Vector field 2D created — streamlines auto-rendered".into(),
                );
            }
            "PhasePortrait" if cmd.args.len() >= 2 => {
                let mut pp = PhasePortraitObj::new(
                    cmd.args[0].trim(),
                    cmd.args[1].trim(),
                    -10.0,
                    10.0,
                    -10.0,
                    10.0,
                );
                pp.density = 25;
                pp.color = Color::new(0.2, 0.2, 0.8, 1.0);
                document.add_object(GeoObject::PhasePortrait(pp));
                input_text.clear();
                return CommandOutcome::Message("Phase portrait created".into());
            }
            "Contour" if cmd.args.len() >= 6 => {
                let expr = cmd.args[0].trim();
                let _x_min = cmd.args[1].trim().parse().unwrap_or(-5.0);
                let _x_max = cmd.args[2].trim().parse().unwrap_or(5.0);
                let _y_min = cmd.args[3].trim().parse().unwrap_or(-5.0);
                let _y_max = cmd.args[4].trim().parse().unwrap_or(5.0);
                let levels: Vec<f64> = cmd.args[5..]
                    .iter()
                    .filter_map(|s| s.trim().parse::<f64>().ok())
                    .collect();
                if levels.is_empty() {
                    return CommandOutcome::Ok;
                }
                // Split LHS/RHS using relation-aware splitting
                let (lhs, rhs, op) = split_relation(expr);
                let mut obj = ImplicitCurveObj::new(lhs, rhs, op);
                obj.label = next_implicit_label(document);
                obj.contour_levels = Some(levels);
                document.add_object(GeoObject::ImplicitCurve(obj));
                input_text.clear();
                return CommandOutcome::Message("Contour curves created".into());
            }
            "ImplicitCurve" if !cmd.args.is_empty() => {
                let expr = cmd.args[0].trim();
                let (lhs, rhs, op) = split_relation(expr);
                let mut obj = ImplicitCurveObj::new(lhs, rhs, op);
                obj.label = next_implicit_label(document);
                document.add_object(GeoObject::ImplicitCurve(obj));
                input_text.clear();
                return CommandOutcome::Message("Implicit curve created".into());
            }
            _ => {}
        }
        result = match execute_cas_command(document, &cmd) {
            Some(msg) if msg.to_lowercase().contains("error") => CommandOutcome::Error(msg),
            Some(msg) => CommandOutcome::Message(msg),
            None => CommandOutcome::Error(format!(
                "Comando no reconocido o argumentos insuficientes: '{}'",
                cmd.command
            )),
        };
        input_text.clear();
        return result;
    }

    let text_with_implicit = insert_implicit_multiplication(&text);
    let text = text_with_implicit.as_str();

    if let Some((name, rest)) = split_on_standalone_eq(text) {
        let name = name.trim();
        let rest = rest.trim();
        if name.chars().all(|c| c.is_alphabetic()) && !name.is_empty() && !is_function_lhs(name) {
            // Evaluamos la expresión para permitir operaciones (ej: a = 5 + 3)
            let vars: Vec<(String, f64)> = document
                .variables
                .iter()
                .map(|(k, v)| (k.clone(), *v))
                .collect();
            if let Ok(val) = evaluate(rest, &vars) {
                document.set_variable(name.to_string(), val);
                input_text.clear();
                return CommandOutcome::Ok;
            }
        }
        if is_function_lhs(name) {
            let label = name
                .split_once('(')
                .map(|(id, _)| id.trim())
                .unwrap_or(name);
            if let Some(id) = find_object_by_label(document, label) {
                document.remove_object(id);
            }
            let final_expr = expand_all_cas(rest, document);
            let obj = GeoObject::Function(FunctionObj::new(&final_expr).with_label(label));
            document.add_object(obj);
            input_text.clear();
            return CommandOutcome::Ok;
        }
        if rest.starts_with('(') && rest.ends_with(')') {
            let inner = &rest[1..rest.len() - 1];
            let parts: Vec<&str> = inner.split(',').map(|s| s.trim()).collect();
            if parts.len() == 2 {
                let vars_vec: Vec<(String, f64)> = document
                    .variables
                    .iter()
                    .map(|(k, v)| (k.clone(), *v))
                    .collect();
                if let (Ok(x), Ok(y)) =
                    (evaluate(parts[0], &vars_vec), evaluate(parts[1], &vars_vec))
                {
                    let mut p = PointObj::new(Point2::new(x, y)).with_label(name);
                    p.x_expr = Some(parts[0].to_string());
                    p.y_expr = Some(parts[1].to_string());
                    let obj = GeoObject::Point(p);
                    document.add_object(obj);
                    input_text.clear();
                    return CommandOutcome::Ok;
                }
            }
        }
        if rest.starts_with('(') && rest.ends_with(')') {
            let inner = &rest[1..rest.len() - 1];
            let parts: Vec<&str> = inner.split(',').map(|s| s.trim()).collect();
            if parts.len() == 3 {
                if let (Ok(x), Ok(y), Ok(z)) = (
                    parts[0].parse::<f64>(),
                    parts[1].parse::<f64>(),
                    parts[2].parse::<f64>(),
                ) {
                    let obj =
                        GeoObject::Point3D(Point3DObj::new(Point3D::new(x, y, z)).with_label(name));
                    document.add_object(obj);
                    input_text.clear();
                    return CommandOutcome::Ok;
                }
            }
        }

        if name == "y" {
            let label = next_function_label(document);
            let obj = GeoObject::Function(FunctionObj::new(rest).with_label(&label));
            document.add_object(obj);
            input_text.clear();
            return CommandOutcome::Ok;
        }

        // Polar curve: r = f(theta) or r(theta) = f(theta)
        if name == "r" || name == "r(θ)" || name == "r(t)" || name == "r(theta)" {
            let t_min = 0.0;
            let t_max = 2.0 * std::f64::consts::PI;
            let obj = GeoObject::PolarCurve(PolarCurveObj::new(rest, t_min, t_max));
            document.add_object(obj);
            input_text.clear();
            return CommandOutcome::Ok;
        }

        // Parametric 2D: (x(t), y(t)) = (f(t), g(t))
        if let Some(inner) = name.strip_prefix('(').and_then(|s| s.strip_suffix(')')) {
            let name_parts: Vec<&str> = inner.split(',').map(|s| s.trim()).collect();
            if name_parts.len() == 2
                && name_parts[0].ends_with("(t)")
                && name_parts[1].ends_with("(t)")
            {
                let rest_clean = rest.trim_matches(|c| c == '(' || c == ')');
                let rest_parts: Vec<&str> = rest_clean.split(',').map(|s| s.trim()).collect();
                if rest_parts.len() == 2 {
                    let obj = GeoObject::ParametricCurve2D(ParametricCurve2DObj::new(
                        rest_parts[0],
                        rest_parts[1],
                        0.0,
                        std::f64::consts::TAU,
                    ));
                    document.add_object(obj);
                    input_text.clear();
                    return CommandOutcome::Ok;
                }
            }
        }

        if rest == "y" {
            let label = next_function_label(document);
            let obj = GeoObject::Function(FunctionObj::new(name).with_label(&label));
            document.add_object(obj);
            input_text.clear();
            return CommandOutcome::Ok;
        }

        // Contour: f(x,y) = [c1, c2, c3] → multi-level implicit
        if rest.starts_with('[') && rest.ends_with(']') {
            if let Ok(levels) = rest[1..rest.len() - 1]
                .split(',')
                .map(|s| s.trim().parse::<f64>())
                .collect::<Result<Vec<f64>, _>>()
            {
                if levels.len() >= 2 {
                    let mut obj = ImplicitCurveObj::new(name, "0", RelationOperator::Eq);
                    obj.label = next_implicit_label(document);
                    obj.contour_levels = Some(levels);
                    document.add_object(GeoObject::ImplicitCurve(obj));
                    input_text.clear();
                    return CommandOutcome::Ok;
                }
            }
        }

        let mut obj = ImplicitCurveObj::new(name, rest, RelationOperator::Eq);
        obj.label = next_implicit_label(document);
        document.add_object(GeoObject::ImplicitCurve(obj));
        input_text.clear();
        return CommandOutcome::Ok;
    } else if let Some((lhs, rhs)) = text.split_once("<=") {
        let mut obj = ImplicitCurveObj::new(lhs.trim(), rhs.trim(), RelationOperator::LessEq);
        obj.label = next_implicit_label(document);
        document.add_object(GeoObject::ImplicitCurve(obj));
        input_text.clear();
        return CommandOutcome::Ok;
    } else if let Some((lhs, rhs)) = text.split_once(">=") {
        let mut obj = ImplicitCurveObj::new(lhs.trim(), rhs.trim(), RelationOperator::GreaterEq);
        obj.label = next_implicit_label(document);
        document.add_object(GeoObject::ImplicitCurve(obj));
        input_text.clear();
        return CommandOutcome::Ok;
    } else if let Some((lhs, rhs)) = text.split_once('<') {
        let mut obj = ImplicitCurveObj::new(lhs.trim(), rhs.trim(), RelationOperator::Less);
        obj.label = next_implicit_label(document);
        document.add_object(GeoObject::ImplicitCurve(obj));
        input_text.clear();
        return CommandOutcome::Ok;
    } else if let Some((lhs, rhs)) = text.split_once('>') {
        let mut obj = ImplicitCurveObj::new(lhs.trim(), rhs.trim(), RelationOperator::Greater);
        obj.label = next_implicit_label(document);
        document.add_object(GeoObject::ImplicitCurve(obj));
        input_text.clear();
        return CommandOutcome::Ok;
    } else {
        let vars_vec: Vec<(String, f64)> = document
            .variables
            .iter()
            .map(|(k, v)| (k.clone(), *v))
            .collect();
        if contains_var(text, 'x') {
            let label = next_function_label(document);
            let obj = GeoObject::Function(FunctionObj::new(text).with_label(label));
            document.add_object(obj);
            input_text.clear();
            return CommandOutcome::Ok;
        } else if let Ok(val) = evaluate(text, &vars_vec) {
            let mut name = String::new();
            for c in b'a'..=b'z' {
                let letter = (c as char).to_string();
                if !document.variables.contains_key(&letter)
                    && find_object_by_label(document, &letter).is_none()
                {
                    name = letter;
                    break;
                }
            }
            if !name.is_empty() {
                document.set_variable(name.clone(), val);
                document.variable_meta.insert(
                    name,
                    grafito_core::VariableMeta {
                        position: grafito_geometry::Point2::new(0.0, 0.0),
                        min: -5.0,
                        max: 5.0,
                        step: 0.1,
                        visible: true,
                        animating: false,
                        animation_speed: 1.0,
                    },
                );
                input_text.clear();
                return CommandOutcome::Ok;
            }
        }
        if text.starts_with('(') && text.ends_with(')') {
            let inner = &text[1..text.len() - 1];
            let parts: Vec<&str> = inner.split(',').map(|s| s.trim()).collect();
            if parts.len() == 3 {
                if let (Ok(x), Ok(y), Ok(z)) = (
                    parts[0].parse::<f64>(),
                    parts[1].parse::<f64>(),
                    parts[2].parse::<f64>(),
                ) {
                    let obj = GeoObject::Point3D(Point3DObj::new(Point3D::new(x, y, z)));
                    document.add_object(obj);
                    input_text.clear();
                    return CommandOutcome::Ok;
                }
            }
            if parts.len() == 2 {
                let vars_vec: Vec<(String, f64)> = document
                    .variables
                    .iter()
                    .map(|(k, v)| (k.clone(), *v))
                    .collect();
                if let (Ok(x), Ok(y)) =
                    (evaluate(parts[0], &vars_vec), evaluate(parts[1], &vars_vec))
                {
                    let mut p = PointObj::new(Point2::new(x, y));
                    p.x_expr = Some(parts[0].to_string());
                    p.y_expr = Some(parts[1].to_string());
                    let obj = GeoObject::Point(p);
                    document.add_object(obj);
                    input_text.clear();
                    return CommandOutcome::Ok;
                }
            }
        }
    }

    input_text.clear();
    match result {
        CommandOutcome::Ok => CommandOutcome::Error(format!(
            "Comando no reconocido o argumentos inválidos: '{}'",
            raw_text
        )),
        other => other,
    }
}

fn intersect_objects(obj_a: &GeoObject, obj_b: &GeoObject) -> Vec<Point2> {
    use grafito_geometry::intersections::{self, IntersectionResult};

    match (obj_a, obj_b) {
        (GeoObject::Line(a), GeoObject::Line(b)) => {
            match intersections::line_line(a.start, a.end, b.start, b.end) {
                IntersectionResult::One(p) => {
                    let t_a = a.param_at_point(p);
                    let t_b = b.param_at_point(p);
                    if a.kind_contains_t(t_a) && b.kind_contains_t(t_b) {
                        vec![p]
                    } else {
                        vec![]
                    }
                }
                _ => vec![],
            }
        }
        (GeoObject::Line(l), GeoObject::Circle(c)) | (GeoObject::Circle(c), GeoObject::Line(l)) => {
            match intersections::line_circle(l.start, l.end, c.center, c.radius) {
                IntersectionResult::One(p) => {
                    if l.kind_contains_t(l.param_at_point(p)) {
                        vec![p]
                    } else {
                        vec![]
                    }
                }
                IntersectionResult::Two(p1, p2) => {
                    let mut pts = Vec::new();
                    for p in [p1, p2] {
                        if l.kind_contains_t(l.param_at_point(p)) {
                            pts.push(p);
                        }
                    }
                    pts
                }
                _ => vec![],
            }
        }
        (GeoObject::Circle(c1), GeoObject::Circle(c2)) => {
            match intersections::circle_circle(c1.center, c1.radius, c2.center, c2.radius) {
                IntersectionResult::One(p) => vec![p],
                IntersectionResult::Two(p1, p2) => vec![p1, p2],
                IntersectionResult::Infinite => vec![],
                IntersectionResult::None => vec![],
            }
        }
        (GeoObject::Function(f), GeoObject::Line(l))
        | (GeoObject::Line(l), GeoObject::Function(f)) => {
            let slope = if (l.end.x - l.start.x).abs() < 1e-12 {
                0.0
            } else {
                (l.end.y - l.start.y) / (l.end.x - l.start.x)
            };
            let intercept = l.start.y - slope * l.start.x;
            let x_min = f.domain_min.unwrap_or(-10.0);
            let x_max = f.domain_max.unwrap_or(10.0);
            intersections::function_line(&f.expr, slope, intercept, x_min, x_max)
                .into_iter()
                .filter(|p| l.kind_contains_t(l.param_at_point(*p)))
                .collect()
        }
        (GeoObject::Function(f1), GeoObject::Function(f2)) => {
            let x_min = f1
                .domain_min
                .unwrap_or(-10.0)
                .max(f2.domain_min.unwrap_or(-10.0));
            let x_max = f1
                .domain_max
                .unwrap_or(10.0)
                .min(f2.domain_max.unwrap_or(10.0));
            intersections::function_function(&f1.expr, &f2.expr, x_min, x_max)
        }
        (GeoObject::Segment3D(a), GeoObject::Segment3D(b)) => {
            match intersections::segment_segment(
                Point2::new(a.a.x, a.a.y),
                Point2::new(a.b.x, a.b.y),
                Point2::new(b.a.x, b.a.y),
                Point2::new(b.b.x, b.b.y),
            ) {
                IntersectionResult::One(p) => vec![p],
                IntersectionResult::Two(p1, p2) => vec![p1, p2],
                _ => vec![],
            }
        }
        _ => vec![],
    }
}

#[derive(Debug)]
pub struct CasCmd {
    pub command: String,
    pub args: Vec<String>,
}

pub fn extract_cas_command(text: &str) -> Option<(String, String, std::ops::Range<usize>)> {
    let keywords = [
        "Derivative",
        "Integral",
        "Solve",
        "Limit",
        "Factor",
        "Expand",
        "Simplify",
        "Taylor",
        "deriv",
        "diff",
        "int",
        "nsolve",
        "lim",
        "derivada",
        "integrar",
        "resolver",
        "limite",
        "factorizar",
        "expandir",
        "simplificar",
    ];

    for &kw in &keywords {
        let mut start_idx = 0;
        while let Some(idx) = text[start_idx..].find(kw) {
            let actual_idx = start_idx + idx;
            let after_kw = &text[actual_idx + kw.len()..];
            let trimmed = after_kw.trim_start();
            if trimmed.starts_with('[') {
                let bracket_start = actual_idx + kw.len() + (after_kw.len() - trimmed.len());
                let mut depth = 0;
                let mut bracket_end = None;
                for (i, c) in text[bracket_start..].char_indices() {
                    if c == '[' {
                        depth += 1;
                    } else if c == ']' {
                        depth -= 1;
                        if depth == 0 {
                            bracket_end = Some(bracket_start + i);
                            break;
                        }
                    }
                }

                if let Some(end) = bracket_end {
                    let cmd_name = kw.to_string();
                    let inner = text[bracket_start + 1..end].to_string();
                    return Some((cmd_name, inner, actual_idx..end + 1));
                }
            }
            start_idx = actual_idx + kw.len();
        }
    }
    None
}

pub fn expand_all_cas(text: &str, document: &Document) -> String {
    let mut current = text.to_string();
    let mut iterations = 0;
    const MAX_ITERATIONS: usize = 50;
    while let Some((cmd, inner, range)) = extract_cas_command(&current) {
        iterations += 1;
        if iterations > MAX_ITERATIONS {
            break;
        }
        let expanded_inner = expand_all_cas(&inner, document);
        let args: Vec<String> = split_args(&expanded_inner)
            .into_iter()
            .map(|s| s.trim().to_string())
            .collect();
        let mut resolved_expr = String::new();

        let normalized = match cmd.to_lowercase().as_str() {
            "derivative" | "derivada" | "deriv" | "diff" => "Derivative",
            "integral" | "integrar" | "int" => "Integral",
            "solve" | "nsolve" | "resolver" => "Solve",
            "limit" | "limite" | "lim" => "Limit",
            "factor" | "factorizar" => "Factor",
            "expand" | "expandir" => "Expand",
            "simplify" | "simplificar" => "Simplify",
            "taylor" => "Taylor",
            "tangentat" | "tangenteen" => "TangentAt",
            "normalat" | "normalen" => "NormalAt",
            "arclength" | "longitudarco" => "ArcLength",
            "curvatureat" | "curvaturaen" => "CurvatureAt",
            "volumeofrevolution" | "volumenrevolucion" => "VolumeOfRevolution",
            "surfaceofrevolution" | "superficierevolucion" => "SurfaceOfRevolution",
            _ => "Unknown",
        };

        let mut expr_arg = args.first().cloned().unwrap_or_default();

        // Try full expr_arg first (e.g. "f(x)")
        let mut found_func = false;
        if let Some(id) = find_object_by_label(document, &expr_arg) {
            if let Some(GeoObject::Function(f)) = document.get_object(id) {
                expr_arg = format!("({})", f.expr.clone());
                found_func = true;
            }
        }
        // If not found, try stripping (x)
        if !found_func {
            if let Some(pos) = expr_arg.find('(') {
                let fname = &expr_arg[..pos];
                if let Some(id) = find_object_by_label(document, fname) {
                    if let Some(GeoObject::Function(f)) = document.get_object(id) {
                        expr_arg = format!("({})", f.expr.clone());
                    }
                }
            }
        }

        match normalized {
            "Derivative" => {
                let var = args.get(1).map(|s| s.as_str()).unwrap_or("x");
                resolved_expr = symbolic::derivative(&expr_arg, var)
                    .unwrap_or_else(|_| current[range.clone()].to_string());
            }
            "Integral" => {
                let var = args.get(1).map(|s| s.as_str()).unwrap_or("x");
                if args.len() == 4 || args.len() == 3 {
                    let a_str = if args.len() == 4 {
                        args.get(2)
                    } else {
                        args.get(1)
                    };
                    let b_str = if args.len() == 4 {
                        args.get(3)
                    } else {
                        args.get(2)
                    };
                    if let (Some(a), Some(b)) = (a_str, b_str) {
                        if let (Ok(a_val), Ok(b_val)) = (a.parse::<f64>(), b.parse::<f64>()) {
                            resolved_expr =
                                symbolic::integrate_definite(&expr_arg, var, a_val, b_val)
                                    .unwrap_or_else(|_| current[range.clone()].to_string());
                        } else {
                            resolved_expr = symbolic::integrate(&expr_arg, var)
                                .unwrap_or_else(|_| current[range.clone()].to_string());
                        }
                    }
                } else {
                    resolved_expr = symbolic::integrate(&expr_arg, var)
                        .unwrap_or_else(|_| current[range.clone()].to_string());
                }
            }
            "Taylor" => {
                let var = args.get(1).map(|s| s.as_str()).unwrap_or("x");
                let center = args.get(2).and_then(|s| s.parse().ok()).unwrap_or(0.0);
                let order = args.get(3).and_then(|s| s.parse().ok()).unwrap_or(5);
                resolved_expr = symbolic::taylor_series(&expr_arg, var, center, order)
                    .unwrap_or_else(|_| current[range.clone()].to_string());
            }
            "Expand" => {
                resolved_expr = symbolic::expand(&expr_arg)
                    .unwrap_or_else(|_| current[range.clone()].to_string());
            }
            "Factor" => {
                let var = args.get(1).map(|s| s.as_str()).unwrap_or("x");
                resolved_expr = symbolic::factor(&expr_arg, var)
                    .unwrap_or_else(|_| current[range.clone()].to_string());
            }
            "Simplify" => {
                resolved_expr = symbolic::simplify(&expr_arg)
                    .unwrap_or_else(|_| current[range.clone()].to_string());
            }
            "Limit" => {
                let var = args.get(1).map(|s| s.as_str()).unwrap_or("x");
                let at = args.get(2).and_then(|s| s.parse().ok()).unwrap_or(0.0);
                resolved_expr = symbolic::limit(&expr_arg, var, at)
                    .unwrap_or_else(|_| current[range.clone()].to_string());
            }
            _ => {
                resolved_expr = current[range.clone()].to_string();
            }
        }

        if resolved_expr == current[range.clone()] {
            break;
        }
        current.replace_range(range, &format!("({})", resolved_expr));
    }
    current
}

pub fn parse_cas_command(text: &str) -> Option<CasCmd> {
    let text = text.trim();
    if let Some(open) = text.find('[') {
        let close = text.rfind(']')?;
        let command = text[..open].trim().to_string();
        let inside = &text[open + 1..close];
        let args: Vec<String> = split_args(inside)
            .into_iter()
            .map(|s| s.trim().to_string())
            .collect();
        if command.is_empty() {
            return None;
        }
        let normalized = match command.to_lowercase().as_str() {
            "derivative" | "derivada" | "deriv" | "diff" => "Derivative",
            "integral" | "integrar" | "int" => "Integral",
            "solve" | "nsolve" | "resolver" => "Solve",
            "limit" | "limite" | "lim" => "Limit",
            "factor" | "factorizar" => "Factor",
            "expand" | "expandir" => "Expand",
            "simplify" | "simplificar" => "Simplify",
            "tangentat" | "tangenteen" => "TangentAt",
            "normalat" | "normalen" => "NormalAt",
            "arclength" | "longitudarco" => "ArcLength",
            "curvatureat" | "curvaturaen" => "CurvatureAt",
            "volumeofrevolution" | "volumenrevolucion" => "VolumeOfRevolution",
            "surfaceofrevolution" | "superficierevolucion" => "SurfaceOfRevolution",
            "lorenz" => "Lorenz",
            "rossler" | "rössler" => "Rossler",
            "thomas" | "butterfly" => "Thomas",
            "aizawa" => "Aizawa",
            "chen" => "Chen",
            "halvorsen" => "Halvorsen",
            "dadras" => "Dadras",
            "chua" => "Chua",
            "mandelbrot" => "Mandelbrot",
            "julia" => "Julia",
            "burningship" | "burning_ship" => "BurningShip",
            "hypercube" | "tesseract" => "Hypercube",
            "hypersphere" => "Hypersphere",
            "vectorfield3d" | "vectorfield" => "VectorField3D",
            "histogram" | "histograma" => "Histogram",
            "scatterplot" | "scatter" => "ScatterPlot",
            "boxplot" => "BoxPlot",
            "linearregression" | "regression" | "regresion" => "LinearRegression",
            "mean" | "media" => "Mean",
            "median" | "mediana" => "Median",
            "stddev" | "desviacion" => "StdDev",
            "correlation" | "correlacion" => "Correlation",
            "determinant" | "det" => "Determinant",
            "inverse" | "inversa" => "Inverse",
            "taylor" => "Taylor",
            "complexgrid" | "complex_grid" | "cgrid" => "ComplexGrid",
            "complexmapping"
            | "complex_mapping"
            | "mapeocomplejo"
            | "mapeo_complejo"
            | "transformadacompleja" => "ComplexMapping",
            "complexsymbol" | "complex_symbol" | "simbolocomplejo" => "ComplexSymbol",
            "domaincoloring" | "domain_coloring" | "dcolor" => "DomainColoring",
            "heatmap" | "heat_map" | "hmap" => "HeatMap",
            "polarcurve" | "polar_curve" | "polar" => "PolarCurve",
            "parametriccurve2d" | "parametric_curve_2d" | "param2d" => "ParametricCurve2D",
            "vectorfield2d" | "vector_field_2d" | "vf2d" => "VectorField2D",
            "phaseportrait" | "phase_portrait" | "phase" => "PhasePortrait",
            "contour" | "contourlines" | "contour_lines" => "Contour",
            "function" | "func" => "Function",
            "piecewise" | "pw" => "Piecewise",
            "distance" | "dist" => "Distance",
            "root" | "raices" | "raiz" => "Root",
            "extremum" | "extremos" | "max" | "min" => "Extremum",
            "intersect" | "interseccion" => "Intersect",
            "yintercept" | "interceptoy" | "intercepto_y" => "YIntercept",
            "analyze" | "analizar" | "analisis" => "Analyze",
            "angle" => "Angle",
            "tangent" => "Tangent",
            "coincident" => "Coincident",
            "horizontal" => "Horizontal",
            "vertical" => "Vertical",
            "equallength" | "equal_length" | "eqlength" => "EqualLength",
            "symmetry" => "Symmetry",
            "ellipsebyfoci" | "ellipse_by_foci" => "EllipseByFoci",
            "parabolabyfocusdirectrix" | "parabola_by_focus_directrix" => {
                "ParabolaByFocusDirectrix"
            }
            "hyperbolabyfoci" | "hyperbola_by_foci" => "HyperbolaByFoci",
            "conicbyfivepoints" | "conic_by_five_points" => "ConicByFivePoints",
            "polygonunion" | "polyunion" => "PolygonUnion",
            "polygonintersection" | "polyintersection" => "PolygonIntersection",
            "polygondifference" | "polydifference" => "PolygonDifference",
            "polygonxor" | "polyxor" => "PolygonXor",
            "segment" => "Segment",
            "ray" => "Ray",
            "vector" => "Vector",
            "regularpolygon" | "regular_polygon" => "RegularPolygon",
            _ => {
                if args.is_empty()
                    || command.contains(' ')
                    || command.contains('=')
                    || command.contains('(')
                {
                    return None;
                }
                return Some(CasCmd { command, args });
            }
        };
        Some(CasCmd {
            command: normalized.to_string(),
            args,
        })
    } else {
        let cmd_lower = text.to_lowercase();
        let bare_commands = [
            "lorenz",
            "rossler",
            "thomas",
            "butterfly",
            "aizawa",
            "chen",
            "halvorsen",
            "dadras",
            "chua",
            "mandelbrot",
            "burningship",
            "hypercube",
            "hypersphere",
        ];
        for &cmd in &bare_commands {
            if cmd_lower == cmd {
                let normalized = match cmd {
                    "burningship" => "BurningShip".to_string(),
                    "butterfly" => "Thomas".to_string(),
                    "lorenz" => "Lorenz".to_string(),
                    "rossler" => "Rossler".to_string(),
                    "thomas" => "Thomas".to_string(),
                    "aizawa" => "Aizawa".to_string(),
                    "chen" => "Chen".to_string(),
                    "halvorsen" => "Halvorsen".to_string(),
                    "dadras" => "Dadras".to_string(),
                    "chua" => "Chua".to_string(),
                    "mandelbrot" => "Mandelbrot".to_string(),
                    "hypercube" => "Hypercube".to_string(),
                    "hypersphere" => "Hypersphere".to_string(),
                    _ => {
                        let mut c = cmd.to_string();
                        c[..1].make_ascii_uppercase();
                        c
                    }
                };
                return Some(CasCmd {
                    command: normalized,
                    args: vec![],
                });
            }
        }
        None
    }
}

pub fn split_args(s: &str) -> Vec<String> {
    let mut args = Vec::new();
    let mut depth = 0;
    let mut start = 0;
    for (i, ch) in s.char_indices() {
        match ch {
            '(' | '{' | '[' => depth += 1,
            ')' | '}' | ']' => depth -= 1,
            ',' if depth == 0 => {
                args.push(s[start..i].to_string());
                start = i + 1;
            }
            _ => {}
        }
    }
    args.push(s[start..].to_string());
    args
}

pub fn execute_cas_command(document: &mut Document, cmd: &CasCmd) -> Option<String> {
    match cmd.command.as_str() {
        "Derivative" => {
            let expr = expand_all_cas(cmd.args.first()?, document);
            let var = cmd.args.get(1).map(|s| s.trim()).unwrap_or("x");
            match symbolic::derivative(&expr, var) {
                Ok(d_expr) => {
                    // Also graph the derivative if it contains the variable
                    if d_expr.contains(var) || d_expr.parse::<f64>().is_ok() {
                        let label = next_function_label(document);
                        document.add_object(GeoObject::Function(
                            FunctionObj::new(&d_expr).with_label(&label),
                        ));
                        Some(format!(
                            "d/d{var}({expr}) = {d_expr}  →  Graficado como {label}"
                        ))
                    } else {
                        Some(format!("d/d{var}({expr}) = {d_expr}"))
                    }
                }
                Err(e) => Some(format!("Error calculando derivada: {}", e)),
            }
        }
        "Integral" => {
            let expr = expand_all_cas(cmd.args.first()?, document);
            let mut var = "x".to_string();
            let mut a_str = None;
            let mut b_str = None;

            if cmd.args.len() == 4 {
                var = cmd.args[1].trim().to_string();
                a_str = cmd.args.get(2);
                b_str = cmd.args.get(3);
            } else if cmd.args.len() == 3 {
                a_str = cmd.args.get(1);
                b_str = cmd.args.get(2);
            } else if cmd.args.len() == 2 {
                var = cmd.args[1].trim().to_string();
            }

            // Check if upper limit is a variable (e.g. Integral[expr, t, 0, x])
            // → graph as f(x) = ∫ₐˣ expr dt
            if let (Some(a_s), Some(b_s)) = (a_str, b_str) {
                let b_trim = b_s.trim();
                if b_trim.len() == 1 && b_trim.chars().all(|c| c.is_alphabetic()) {
                    let lower: f64 = a_s.trim().parse().unwrap_or(0.0);
                    let label = next_function_label(document);
                    let obj = FunctionObj::new(&expr)
                        .with_label(&label)
                        .as_integral(&var, lower);
                    document.add_object(GeoObject::Function(obj));
                    return Some(format!(
                        "F({}) = ∫₍{}₎ˣ {} d{} → {}",
                        b_trim, lower, expr, var, label
                    ));
                }
            }

            let label = next_function_label(document);
            document.add_object(GeoObject::Function(
                FunctionObj::new(&expr).with_label(&label),
            ));

            if let (Some(a_s), Some(b_s)) = (a_str, b_str) {
                let a: f64 = a_s.trim().parse().unwrap_or(0.0);
                let b: f64 = b_s.trim().parse().unwrap_or(1.0);

                // Ruta híbrida GPU/CPU: si hay un evaluador GPU registrado,
                // la expresión es compatible y los límites son numéricos,
                // evaluamos en GPU y reducimos en CPU con Simpson compuesto.
                if var == "x" {
                    const HYBRID_SAMPLES: usize = 4096;
                    if let Some(evaluator) = GPU_FUNCTION_EVALUATOR.get() {
                        if let Some(ys) = evaluator.evaluate_function_batch(
                            &expr,
                            a,
                            b,
                            HYBRID_SAMPLES,
                            &document.variables,
                        ) {
                            if ys.len() >= 2 {
                                let dx = (b - a) / (ys.len() - 1) as f64;
                                let approx = grafito_geometry::integral::composite_simpson(&ys, dx);
                                if approx.is_finite() {
                                    return Some(format!(
                                        "≈ {:.6} (híbrido GPU/CPU) → Graficado como {}",
                                        approx, label
                                    ));
                                }
                            }
                        }
                    }
                }

                match symbolic::integrate_definite(&expr, &var, a, b) {
                    Ok(result) => Some(format!("{} → Graficado como {}", result, label)),
                    Err(e) => Some(format!("Error calculando integral: {}", e)),
                }
            } else {
                match symbolic::integrate(&expr, &var) {
                    Ok(result) => Some(format!("{} → Graficado original como {}", result, label)),
                    Err(e) => Some(format!("Error calculando integral: {}", e)),
                }
            }
        }
        "Solve" => {
            let expr_raw = expand_all_cas(cmd.args.first()?, document);
            let mut expr_clean = expr_raw.trim().to_string();
            if let Some((lhs, rhs)) = split_on_standalone_eq(&expr_clean) {
                expr_clean = format!("({}) - ({})", lhs, rhs);
            }
            let var = cmd
                .args
                .get(1)
                .map(|s| s.trim().to_string())
                .unwrap_or_else(|| {
                    // simple variable extraction
                    for c in expr_clean.chars() {
                        if c.is_alphabetic() && c != 'e' && c != 'i' {
                            return c.to_string();
                        }
                    }
                    "x".to_string()
                });
            let var = var.as_str();
            let a: f64 = cmd
                .args
                .get(2)
                .and_then(|s| s.trim().parse().ok())
                .unwrap_or(-20.0);
            let b: f64 = cmd
                .args
                .get(3)
                .and_then(|s| s.trim().parse().ok())
                .unwrap_or(20.0);

            let graph_expr = replace_variable(&expr_clean, var, "x");
            let label = next_function_label(document);
            document.add_object(GeoObject::Function(
                FunctionObj::new(&graph_expr).with_label(&label),
            ));

            let mut complex_roots_found = false;
            let mut strs = Vec::new();

            let preprocessed = expr_clean.replace(" ", "");
            if let Ok(ast) = grafito_geometry::ast::parse_ast(&preprocessed) {
                if let Some(roots) = symbolic::solve_polynomial_complex(&ast, var) {
                    if !roots.is_empty() {
                        complex_roots_found = true;
                        for r in &roots {
                            if r.1.abs() < 1e-9 {
                                strs.push(format!("{var} ≈ {:.6}", r.0));
                                document.add_object(GeoObject::Point(
                                    PointObj::new(Point2::new(r.0, 0.0)).with_label("Raíz"),
                                ));
                            } else {
                                let sign = if r.1 > 0.0 { "+" } else { "-" };
                                strs.push(format!("{var} ≈ {:.6} {} {:.6}i", r.0, sign, r.1.abs()));
                                document.add_object(GeoObject::Point(
                                    PointObj::new(Point2::new(r.0, r.1)).with_label("Raíz"),
                                ));
                            }
                        }
                    }
                }
            }

            if complex_roots_found {
                return Some(format!("{} → Graficado como {}", strs.join(", "), label));
            }

            let expr_c1 = expr_clean.clone();
            let expr_c2 = expr_clean.clone();
            let vars1 = document.variables.clone();
            let vars2 = document.variables.clone();
            let var_name1 = var.to_string();
            let var_name2 = var.to_string();
            let f = move |x: f64| {
                let mut v = vars1.clone();
                v.insert(var_name1.clone(), x);
                eval_function_with_vars(&expr_c1, x, &v).unwrap_or(f64::NAN)
            };
            let mut roots = Vec::new();
            let steps = 4000;
            let step = (b - a) / steps as f64;
            let mut prev = f(a);
            for i in 1..=steps {
                let x = a + i as f64 * step;
                let curr = f(x);
                if curr.abs() < 1e-12 {
                    let duplicate = roots.iter().any(|&r: &f64| (r - x).abs() < 1e-6);
                    if !duplicate {
                        roots.push(x);
                    }
                } else if prev.is_finite() && curr.is_finite() && prev * curr <= 0.0 {
                    let mut left = x - step;
                    let mut right = x;
                    let mut f_left = prev;
                    let mut root = left;
                    for _ in 0..50 {
                        let mid = (left + right) * 0.5;
                        let mut v2 = vars2.clone();
                        v2.insert(var_name2.clone(), mid);
                        let f_mid = eval_function_with_vars(&expr_c2, mid, &v2).unwrap_or(f64::NAN);
                        if f_mid.abs() < 1e-9 {
                            root = mid;
                            break;
                        }
                        if f_mid.is_finite() {
                            if f_left * f_mid < 0.0 {
                                right = mid;
                            } else {
                                left = mid;
                                f_left = f_mid;
                            }
                        } else {
                            break;
                        }
                        root = mid;
                    }
                    let duplicate = roots.iter().any(|&r: &f64| (r - root).abs() < 1e-6);
                    if !duplicate {
                        roots.push(root);
                    }
                }
                prev = curr;
            }
            if roots.is_empty() {
                Some(format!(
                    "Sin raíces para {} en [{a:.1}, {b:.1}] → Graficado como {label}",
                    var
                ))
            } else {
                let mut strs = Vec::new();
                for r in &roots {
                    strs.push(format!("{var} ≈ {:.6}", r));
                    document.add_object(GeoObject::Point(
                        PointObj::new(Point2::new(*r, 0.0)).with_label("Raíz"),
                    ));
                }
                Some(format!("{} → Graficado como {}", strs.join(", "), label))
            }
        }
        "Taylor" => {
            let expr = expand_all_cas(cmd.args.first()?, document);
            let var = cmd.args.get(1).map(|s| s.trim()).unwrap_or("x");
            let center: f64 = cmd
                .args
                .get(2)
                .and_then(|s| s.trim().parse().ok())
                .unwrap_or(0.0);
            let order: usize = cmd
                .args
                .get(3)
                .and_then(|s| s.trim().parse().ok())
                .unwrap_or(5);
            match symbolic::taylor_series(&expr, var, center, order) {
                Ok(result) => {
                    let label = next_function_label(document);
                    document.add_object(GeoObject::Function(
                        FunctionObj::new(&result).with_label(&label),
                    ));
                    Some(format!("{} → Graficado como {}", result, label))
                }
                Err(e) => Some(format!("Error: {}", e)),
            }
        }
        "Limit" => {
            let expr = expand_all_cas(cmd.args.first()?, document);
            let var = cmd.args.get(1).map(|s| s.trim()).unwrap_or("x");
            let at_str = cmd.args.get(2).map(|s| s.trim()).unwrap_or("0");
            let at: f64 = match at_str {
                "inf" | "infty" | "infinity" | "∞" => f64::INFINITY,
                "-inf" | "-infty" | "-infinity" | "-∞" => f64::NEG_INFINITY,
                s => s.parse().unwrap_or(0.0),
            };

            let label = next_function_label(document);
            document.add_object(GeoObject::Function(
                FunctionObj::new(&expr).with_label(&label),
            ));

            match symbolic::limit(&expr, var, at) {
                Ok(result) => {
                    if let Some(val_str) = result.split("=").last() {
                        if let Ok(val) = val_str.trim().parse::<f64>() {
                            if at.is_finite() {
                                document.add_object(GeoObject::Point(
                                    PointObj::new(Point2::new(at, val)).with_label("Límite"),
                                ));
                            }
                        }
                    }
                    Some(format!("{} → Graficado como {}", result, label))
                }
                Err(e) => Some(format!("Error: {}", e)),
            }
        }
        "Factor" => {
            let expr = expand_all_cas(cmd.args.first()?, document);
            match symbolic::factor(&expr, "x") {
                Ok(factors) => Some(format!("{} = {}", expr, factors)),
                Err(e) => Some(format!("Factor error: {}", e)),
            }
        }
        "Expand" => {
            let expr = expand_all_cas(cmd.args.first()?, document);
            match symbolic::expand(&expr) {
                Ok(expanded) => Some(format!("{} = {}", expr, expanded)),
                Err(e) => Some(format!("Expand error: {}", e)),
            }
        }
        "Simplify" => {
            let expr = expand_all_cas(cmd.args.first()?, document);
            match symbolic::simplify(&expr) {
                Ok(simplified) => Some(format!("{} = {}", expr, simplified)),
                Err(e) => Some(format!("Simplify error: {}", e)),
            }
        }
        "TangentAt" => {
            let expr_raw = cmd.args.first()?.trim();
            let x: f64 = cmd.args.get(1)?.trim().parse().ok()?;
            let expr = substitute_document_vars(expr_raw, document);
            match tangent_line_at(&expr, x) {
                Ok((x0, fx, slope)) => {
                    let p1 = Point2::new(x0, fx);
                    let p2 = Point2::new(x0 + 1.0, fx + slope);
                    document
                        .add_object(GeoObject::Line(LineObj::new(p1, p2).with_label("tangente")));
                    Some(format!(
                        "Tangente en x={:.4}: y = {:.4} + {:.4}·(x − {:.4})",
                        x0, fx, slope, x0
                    ))
                }
                Err(e) => Some(format!("Error en TangentAt: {e}")),
            }
        }
        "NormalAt" => {
            let expr_raw = cmd.args.first()?.trim();
            let x: f64 = cmd.args.get(1)?.trim().parse().ok()?;
            let expr = substitute_document_vars(expr_raw, document);
            match normal_line_at(&expr, x) {
                Ok((x0, fx, normal_slope)) => {
                    let p1 = Point2::new(x0, fx);
                    let p2 = if normal_slope.is_infinite() {
                        Point2::new(x0, fx + 1.0)
                    } else {
                        Point2::new(x0 + 1.0, fx + normal_slope)
                    };
                    document.add_object(GeoObject::Line(LineObj::new(p1, p2).with_label("normal")));
                    Some(format!("Normal en x={:.4}", x0))
                }
                Err(e) => Some(format!("Error en NormalAt: {e}")),
            }
        }
        "ArcLength" => {
            let expr_raw = cmd.args.first()?.trim();
            let a: f64 = cmd.args.get(1)?.trim().parse().ok()?;
            let b: f64 = cmd.args.get(2)?.trim().parse().ok()?;
            let expr = substitute_document_vars(expr_raw, document);
            match arc_length(&expr, a, b) {
                Ok(length) => Some(format!(
                    "Longitud de arco de {:.4} a {:.4}: {:.6}",
                    a, b, length
                )),
                Err(e) => Some(format!("Error en ArcLength: {e}")),
            }
        }
        "CurvatureAt" => {
            let expr_raw = cmd.args.first()?.trim();
            let x: f64 = cmd.args.get(1)?.trim().parse().ok()?;
            let expr = substitute_document_vars(expr_raw, document);
            match curvature_at(&expr, x) {
                Ok(kappa) => {
                    let radius = if kappa.is_finite() && kappa.abs() > 1e-15 {
                        1.0 / kappa
                    } else {
                        f64::INFINITY
                    };
                    Some(format!(
                        "Curvatura en x={:.4}: κ = {:.6}, Radio = {:.6}",
                        x, kappa, radius
                    ))
                }
                Err(e) => Some(format!("Error en CurvatureAt: {e}")),
            }
        }
        "VolumeOfRevolution" => {
            let expr_raw = cmd.args.first()?.trim();
            let a: f64 = cmd.args.get(1)?.trim().parse().ok()?;
            let b: f64 = cmd.args.get(2)?.trim().parse().ok()?;
            let expr = substitute_document_vars(expr_raw, document);
            match volume_of_revolution(&expr, a, b) {
                Ok(volume) => Some(format!(
                    "Volumen de revolución de {:.4} a {:.4}: {:.6}",
                    a, b, volume
                )),
                Err(e) => Some(format!("Error en VolumeOfRevolution: {e}")),
            }
        }
        "SurfaceOfRevolution" => {
            let expr_raw = cmd.args.first()?.trim();
            let a: f64 = cmd.args.get(1)?.trim().parse().ok()?;
            let b: f64 = cmd.args.get(2)?.trim().parse().ok()?;
            let expr = substitute_document_vars(expr_raw, document);
            match surface_of_revolution(&expr, a, b) {
                Ok(surface) => Some(format!(
                    "Superficie de revolución de {:.4} a {:.4}: {:.6}",
                    a, b, surface
                )),
                Err(e) => Some(format!("Error en SurfaceOfRevolution: {e}")),
            }
        }
        _ => None,
    }
}

pub fn is_function_lhs(name: &str) -> bool {
    if let Some((id, args)) = name.split_once('(') {
        let id = id.trim();
        let args = args.trim_end_matches(')').trim();
        id.chars().all(|c| c.is_alphabetic() || c.is_ascii_digit())
            && !id.is_empty()
            && !id.starts_with(|c: char| c.is_ascii_digit())
            && args.len() == 1
            && args.chars().all(|c| c.is_alphabetic())
    } else {
        false
    }
}

pub fn contains_var(text: &str, var: char) -> bool {
    let chars: Vec<char> = text.chars().collect();
    for i in 0..chars.len() {
        if chars[i] == var {
            let prev = if i > 0 { chars[i - 1] } else { ' ' };
            let next = if i + 1 < chars.len() {
                chars[i + 1]
            } else {
                ' '
            };
            if !prev.is_alphabetic() && !next.is_alphabetic() {
                return true;
            }
        }
    }
    false
}

pub fn find_object_by_label(document: &Document, label: &str) -> Option<ObjectId> {
    document
        .objects_iter()
        .find(|(_, obj)| obj.label() == label.trim())
        .map(|(id, _)| *id)
}

/// Convierte un `GeoObject` en un [`IntersectionCurve`] cuando el tipo lo
/// admite. Devuelve `None` para tipos no soportados (3D, polígonos, …).
fn object_to_intersection_curve(obj: &GeoObject) -> Option<IntersectionCurve<'_>> {
    match obj {
        GeoObject::Line(l) => Some(IntersectionCurve::Line {
            s: l.start,
            e: l.end,
        }),
        GeoObject::Circle(c) => Some(IntersectionCurve::Circle {
            center: c.center,
            radius: c.radius,
        }),
        GeoObject::Function(f) => Some(IntersectionCurve::Function { expr: &f.expr }),
        _ => None,
    }
}

/// Ejecuta un comando de análisis matemático sobre un objeto etiquetado.
fn run_analysis_command(
    document: &mut Document,
    input_text: &mut String,
    label: &str,
    features: &[AnalysisFeature],
    feature_name: &str,
) -> CommandOutcome {
    let view = *document.view();
    let world_tl = view.screen_to_world(glam::Vec2::new(0.0, 0.0));
    let world_br = view.screen_to_world(glam::Vec2::new(view.screen_size.x, view.screen_size.y));
    let view_bounds = (
        world_tl.x.min(world_br.x),
        world_tl.x.max(world_br.x),
        world_tl.y.min(world_br.y),
        world_tl.y.max(world_br.y),
    );

    let base_label = label
        .split_once('(')
        .map(|(id, _)| id.trim())
        .unwrap_or(label);
    if let Some(id) =
        find_object_by_label(document, label).or_else(|| find_object_by_label(document, base_label))
    {
        if let Some(obj) = document.get_object(id).cloned() {
            let results =
                analyzable::analyze_object(&obj, view_bounds, &document.variables, features);
            if results.is_empty() {
                return CommandOutcome::Message(format!(
                    "{}: no se encontraron características",
                    feature_name
                ));
            }
            for r in &results {
                let (color, size) = match r.feature {
                    AnalysisFeature::Root | AnalysisFeature::XIntercept => {
                        (Color::new(1.0, 0.2, 0.2, 1.0), 8.0)
                    }
                    AnalysisFeature::YIntercept => (Color::new(0.2, 0.5, 1.0, 1.0), 8.0),
                    AnalysisFeature::LocalMaximum => (Color::new(0.2, 0.8, 0.4, 1.0), 7.0),
                    AnalysisFeature::LocalMinimum => (Color::new(0.2, 0.8, 0.9, 1.0), 7.0),
                    AnalysisFeature::Inflection => (Color::new(1.0, 0.6, 0.2, 1.0), 7.0),
                    AnalysisFeature::VerticalAsymptote
                    | AnalysisFeature::HorizontalAsymptote
                    | AnalysisFeature::ObliqueAsymptote => (Color::new(0.8, 0.3, 0.8, 1.0), 6.0),
                    AnalysisFeature::Intersection | AnalysisFeature::Equilibrium => {
                        (Color::new(0.9, 0.4, 0.9, 1.0), 7.0)
                    }
                    AnalysisFeature::Centroid => (Color::new(0.4, 0.9, 0.4, 1.0), 8.0),
                };
                let mut p = PointObj::new(r.point).with_label(&r.label);
                p.color = color;
                p.size = size;
                document.add_object(GeoObject::Point(p));
            }
            input_text.clear();
            return CommandOutcome::Message(format!(
                "{}: {} punto(s) de análisis creados",
                feature_name,
                results.len()
            ));
        }
    }
    CommandOutcome::Error(format!("{}: requiere un objeto válido", feature_name))
}

fn resolve_two_polygons(
    document: &Document,
    label_a: &str,
    label_b: &str,
) -> Result<(geo::Polygon<f64>, geo::Polygon<f64>), String> {
    let id_a = find_object_by_label(document, label_a)
        .ok_or_else(|| format!("Object '{}' not found", label_a))?;
    let id_b = find_object_by_label(document, label_b)
        .ok_or_else(|| format!("Object '{}' not found", label_b))?;

    let obj_a = document
        .get_object(id_a)
        .ok_or_else(|| "Object not found".to_string())?;
    let obj_b = document
        .get_object(id_b)
        .ok_or_else(|| "Object not found".to_string())?;

    match (obj_a, obj_b) {
        (GeoObject::Polygon(a), GeoObject::Polygon(b)) => {
            Ok((polygon_to_geo(&a.vertices), polygon_to_geo(&b.vertices)))
        }
        _ => Err("Both arguments must be polygons".to_string()),
    }
}

fn add_boolean_result(document: &mut Document, mp: &geo::MultiPolygon<f64>, base_label: &str) {
    let polys = grafito_geometry::boolean::multipolygon_to_polygons(mp);
    for (i, verts) in polys.into_iter().enumerate() {
        let label = if i == 0 {
            base_label.to_string()
        } else {
            format!("{}{}", base_label, subscript_label(i))
        };
        let mut poly = PolygonObj::new(verts);
        poly.label = label;
        document.add_object(GeoObject::Polygon(poly));
    }
}

fn subscript_label(n: usize) -> String {
    n.to_string()
        .chars()
        .map(|c| match c {
            '0' => '₀',
            '1' => '₁',
            '2' => '₂',
            '3' => '₃',
            '4' => '₄',
            '5' => '₅',
            '6' => '₆',
            '7' => '₇',
            '8' => '₈',
            '9' => '₉',
            _ => c,
        })
        .collect()
}

pub fn parse_point_str(s: &str) -> Result<(f64, f64), String> {
    let s = s.trim();
    // Quitar solo un par de paréntesis externos, no todos
    let s = if s.starts_with('(') && s.ends_with(')') {
        &s[1..s.len() - 1]
    } else {
        s
    };
    let parts: Vec<&str> = s.split(',').map(|p| p.trim()).collect();
    if parts.len() == 2 {
        Ok((
            parts[0].parse().map_err(|_| "bad x")?,
            parts[1].parse().map_err(|_| "bad y")?,
        ))
    } else {
        Err("expected (x, y)".into())
    }
}

pub fn next_function_label(document: &Document) -> String {
    let used: HashSet<String> = document
        .objects_iter()
        .filter_map(|(_, obj)| {
            if let GeoObject::Function(f) = obj {
                Some(f.label.clone())
            } else {
                None
            }
        })
        .collect();
    for c in 'f'..='z' {
        let label = format!("{}(x)", c);
        if !used.contains(&label) {
            return label;
        }
    }
    format!("f{}(x)", document.object_count())
}

/// Devuelve el siguiente label disponible para una `ImplicitCurve`:
/// `I`, `J`, `K`, ... evitando colisiones con labels ya usados.
///
/// Esto permite que el usuario escriba `ComplexMapping[1/z, I]`
/// después de crear la primera implícita con `x^2 + y^2 = 1`,
/// en vez de tener que recordar el label vacío que se asignaba antes.
pub fn next_implicit_label(document: &Document) -> String {
    let used: HashSet<String> = document
        .objects_iter()
        .filter_map(|(_, obj)| {
            if let GeoObject::ImplicitCurve(ic) = obj {
                Some(ic.label.clone())
            } else {
                None
            }
        })
        .collect();
    for c in 'I'..='Z' {
        let label = c.to_string();
        if !used.contains(&label) {
            return label;
        }
    }
    // Después de I..Z (que es 18 letras mayúsculas), usar un sufijo numérico.
    format!("I{}", document.object_count())
}

pub fn find_extrema<F: Fn(f64) -> f64>(f: &F, a: f64, b: f64, find_max: bool) -> Vec<(f64, f64)> {
    let mut pts = Vec::new();
    let steps = 200;
    let step = (b - a) / steps as f64;
    let mut prev = f(a);
    for i in 1..steps {
        let x = a + i as f64 * step;
        let curr = f(x);
        let next = f(x + step);
        if find_max {
            if curr > prev && curr > next && curr.is_finite() {
                pts.push((x, curr));
            }
        } else {
            if curr < prev && curr < next && curr.is_finite() {
                pts.push((x, curr));
            }
        }
        prev = curr;
    }
    pts
}

pub fn root_10<F: Fn(f64) -> f64>(f: &F) -> Option<(f64, f64)> {
    for x0 in -10..=10 {
        if let Ok(r) = grafito_geometry::cas::newton_root_auto(f, x0 as f64) {
            if (-10.0..=10.0).contains(&r) {
                let fx = f(r);
                if fx.abs() < 0.1 {
                    return Some((r, fx));
                }
            }
        }
    }
    None
}

pub fn parse_preview(input_text: &str) -> Option<GeoObject> {
    let raw_text = input_text.trim().to_string();
    if raw_text.is_empty() {
        return None;
    }
    let text = raw_text
        .replace("x²", "x^2")
        .replace("√", "sqrt")
        .replace("|x|", "abs(x)")
        .replace("π", "pi")
        .replace("τ", "tau")
        .replace("÷", "/")
        .replace("×", "*")
        .replace("≤", "<=")
        .replace("≥", ">=");
    if parse_cas_command(&text).is_some() {
        return None;
    }

    let text_with_implicit = insert_implicit_multiplication(&text);
    let text = text_with_implicit.as_str();

    if let Some((name, rest)) = split_on_standalone_eq(text) {
        let name = name.trim();
        let rest = rest.trim();
        if is_function_lhs(name)
            && (rest.contains('x')
                || rest
                    .chars()
                    .all(|c| c.is_numeric() || "+-*/().^x sincostanlognatqerfabs ".contains(c)))
        {
            return Some(GeoObject::Function(FunctionObj::new(rest).with_label(name)));
        }
        if rest.starts_with('(') && rest.ends_with(')') {
            let inner = &rest[1..rest.len() - 1];
            let parts: Vec<&str> = inner.split(',').map(|s| s.trim()).collect();
            if parts.len() == 2 {
                if let (Ok(x), Ok(y)) = (parts[0].parse::<f64>(), parts[1].parse::<f64>()) {
                    return Some(GeoObject::Point(
                        PointObj::new(Point2::new(x, y)).with_label(name),
                    ));
                }
            }
        }
    } else {
        if text.contains('x') {
            return Some(GeoObject::Function(
                FunctionObj::new(text).with_label("preview"),
            ));
        }
        if text.starts_with('(') && text.ends_with(')') {
            let inner = &text[1..text.len() - 1];
            let parts: Vec<&str> = inner.split(',').map(|s| s.trim()).collect();
            if parts.len() == 2 {
                if let (Ok(x), Ok(y)) = (parts[0].parse::<f64>(), parts[1].parse::<f64>()) {
                    return Some(GeoObject::Point(PointObj::new(Point2::new(x, y))));
                }
            }
        }
    }
    None
}

fn parse_brace_list(s: &str) -> Vec<f64> {
    let s = s.trim().trim_start_matches('{').trim_end_matches('}');
    s.split(',')
        .filter_map(|v| {
            let v = v.trim();
            if v.is_empty() {
                None
            } else {
                v.parse::<f64>().ok()
            }
        })
        .collect()
}

fn parse_matrix_arg(s: &str) -> Option<Matrix> {
    let s = s.trim();
    if !s.starts_with('[') || !s.ends_with(']') {
        return None;
    }
    let inner = &s[1..s.len() - 1];
    let mut rows = Vec::new();
    let mut depth = 0;
    let mut start = 0;
    for (i, ch) in inner.char_indices() {
        match ch {
            '[' => depth += 1,
            ']' => {
                depth -= 1;
                if depth == 0 {
                    let row_str = &inner[start..=i];
                    let row: Vec<f64> = row_str
                        .trim_matches(|c| c == '[' || c == ']')
                        .split(',')
                        .filter_map(|v| v.trim().parse().ok())
                        .collect();
                    rows.push(row);
                    start = i + 1;
                }
            }
            ',' if depth == 0 => {
                start = i + 1;
            }
            _ => {}
        }
    }
    Matrix::from_rows(rows)
}

#[cfg(test)]
mod tests {
    use super::*;
    use grafito_core::{Document, GeoObject, ImplicitCurveObj, RelationOperator};

    #[test]
    fn test_next_implicit_label_assigns_i_first() {
        let doc = Document::new();
        assert_eq!(next_implicit_label(&doc), "I");
    }

    #[test]
    fn test_next_implicit_label_skips_used() {
        let mut doc = Document::new();
        let mut ic = ImplicitCurveObj::new("x^2 + y^2 - 4", "0", RelationOperator::Eq);
        ic.label = "I".to_string();
        doc.add_object(GeoObject::ImplicitCurve(ic));
        assert_eq!(next_implicit_label(&doc), "J");
    }

    #[test]
    fn test_next_implicit_label_ignores_other_types() {
        let mut doc = Document::new();
        // Una Function con label "I" no debe interferir con la numeración de
        // implícitas. Las implícitas siguen su propio namespace.
        let f = grafito_core::FunctionObj::new("x^2");
        doc.add_object(GeoObject::Function(f));
        assert_eq!(next_implicit_label(&doc), "I");
    }

    #[test]
    fn test_implicit_curve_gets_auto_label_via_process_input() {
        // El flujo principal: el usuario escribe `x^2 + y^2 = 1` y la implícita
        // se crea con label "I" (no vacío). Luego puede hacer
        // `ComplexMapping[1/z, I]` y encontrar el target.
        let mut doc = Document::new();
        process_input(&mut doc, &mut "x^2 + y^2 = 1".to_string());
        let label = doc
            .objects_iter()
            .find_map(|(_, o)| {
                if let GeoObject::ImplicitCurve(ic) = o {
                    Some(ic.label.clone())
                } else {
                    None
                }
            })
            .expect("should have created an ImplicitCurve");
        assert_eq!(label, "I");

        // Ahora el ComplexMapping debe poder encontrar el target por label.
        let mut out = "ComplexMapping[1/z, I]".to_string();
        let outcome = process_input(&mut doc, &mut out);
        assert!(
            !matches!(outcome, CommandOutcome::Error(_)),
            "ComplexMapping should find the implicit curve by label 'I'"
        );
        let has_cm = doc
            .objects_iter()
            .any(|(_, o)| matches!(o, GeoObject::ComplexMapping(_)));
        assert!(has_cm, "ComplexMapping object should have been created");
    }

    #[test]
    fn test_polygon_union_command() {
        let mut doc = Document::new();

        // Create two overlapping unit squares via process_input.
        process_input(&mut doc, &mut "RegularPolygon[(0,0), 4, 1]".to_string());
        process_input(&mut doc, &mut "RegularPolygon[(0.5,0), 4, 1]".to_string());

        let polygon_labels: Vec<String> = doc
            .objects_iter()
            .filter(|(_, obj)| matches!(obj, GeoObject::Polygon(_)))
            .map(|(_, obj)| obj.label().to_string())
            .collect();
        assert_eq!(polygon_labels.len(), 2, "two input polygons should exist");

        let mut cmd = format!("PolygonUnion[{}, {}]", polygon_labels[0], polygon_labels[1]);
        process_input(&mut doc, &mut cmd);

        let union_exists = doc.objects_iter().any(|(_, obj)| obj.label() == "U");
        assert!(
            union_exists,
            "union result polygon labeled 'U' should exist"
        );
    }
}
