use geo::BooleanOps;
use grafito_core::{
    analyzable::{self, default_analysis_features},
    implicit_curve::validate_contour_levels,
    Attractor3DObj, BoxPlotObj, CasWorksheetStatus, CircleObj, ComplexGridObj, ComplexIntegralObj,
    ComplexMappingObj, Cone3DObj, Cube3DObj, Cylinder3DObj, DataTableObj, Document, EllipseObj,
    FitMetadata, Fractal2DObj, FunctionObj, GeoObject, HistogramObj, HyperSurface4DObj,
    HyperbolaObj, ImplicitCurveObj, Line3DObj, LineKind, LineObj, MoebiusStripObj, ObjectId,
    ParabolaObj, ParametricCurve2DObj, ParametricCurve3DObj, PencilObj, PhasePortraitObj,
    Plane3DObj, Point3DObj, PointObj, PolarCurveObj, PolygonObj, RegressionLineObj,
    RegularPolychoron4DObj, RegularPolytopeNDObj, RelationOperator, ScatterPlotObj, Segment3DObj,
    Sphere3DObj, Surface3DObj, Tetrahedron3DObj, Torus3DObj, VectorField2DObj, VectorField3DObj,
};
use grafito_geometry::analysis::{
    analyze_intersection, arc_length, curvature_at, normal_line_at, surface_of_revolution,
    tangent_line_at, volume_of_revolution, AnalysisFeature, IntersectionCurve,
};
use grafito_geometry::boolean::polygon_to_geo;
use grafito_geometry::expr::{evaluate, prepare_function_ast};
use grafito_geometry::matrices::{
    cholesky, condition_number, eigenvalues, eigenvectors, lu_decomposition, null_space,
    qr_decomposition, rank, solve_linear_system, svd, Matrix,
};
use grafito_geometry::statistics;
use grafito_geometry::symbolic;
use grafito_geometry::{
    intersect_planes, line_line_relation, plane_through_lines, project_line_onto_plane,
    Line3D as GeomLine3D, LineLineRelation, LineProjectionOnPlane, Plane3D as GeomPlane3D,
    PlanePlaneIntersection, PlaneThroughLines,
};
use grafito_geometry::{Color, Point2, Point3D, RegularPolychoron, RegularPolytopeFamily};
use std::collections::{HashMap, HashSet};
use std::sync::OnceLock;

/// Reemplaza una variable por otra solo en límites de palabra (identificadores completos).
/// Evita corromper nombres de funciones: `replace_variable("exp(e)", "e", "x")` → `"exp(x)"`, no `"xxp(x)"`.
fn replace_variable(expr: &str, from: &str, to: &str) -> String {
    if from.is_empty() {
        return expr.to_string();
    }
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

const MAX_COMMAND_INPUT_BYTES: usize = 65_536;
const MAX_COMMAND_NESTING: usize = 32;
const MAX_COMMAND_ARGS: usize = 64;
const MAX_SCRIPT_COMMANDS: usize = 100;
const MAX_SCRIPT_DEPTH: usize = 5;
const MAX_DISCRETE_COUNT: u32 = 10_000;
const MAX_TAYLOR_ORDER: usize = 64;
const REGULAR_POLYCHORON_4D_ROTATION_ANGLE_COUNT: usize = 6;
/// A fixed-step trajectory includes its initial point, so this is one less
/// than the persisted open-polyline limit.
const MAX_ODE_PLOT_STEPS: usize = grafito_core::pencil::MAX_PENCIL_POINTS.saturating_sub(1);

fn validate_ode_plot_steps(command: &str, steps: usize) -> Result<(), CommandOutcome> {
    if steps > MAX_ODE_PLOT_STEPS {
        return Err(CommandOutcome::Error(format!(
            "{command}: steps={steps} excede el máximo para una trayectoria ({MAX_ODE_PLOT_STEPS})"
        )));
    }
    Ok(())
}

/// Selects an endpoint-preserving uniform subset for adaptive trajectories.
/// The returned count never exceeds the core open-polyline point limit.
fn bounded_ode_plot_indices(point_count: usize) -> Vec<usize> {
    let retained = point_count.min(grafito_core::pencil::MAX_PENCIL_POINTS);
    if retained == 0 {
        return Vec::new();
    }
    if retained == 1 {
        return vec![0];
    }

    (0..retained)
        .map(|index| index * (point_count - 1) / (retained - 1))
        .collect()
}

#[derive(Default)]
struct ScriptBudget {
    depth: usize,
    executed_commands: usize,
}

fn validate_command_input(input: &str) -> Result<(), String> {
    if input.len() > MAX_COMMAND_INPUT_BYTES {
        return Err(format!(
            "Command input exceeds maximum {MAX_COMMAND_INPUT_BYTES} bytes"
        ));
    }

    let mut delimiters = Vec::with_capacity(MAX_COMMAND_NESTING);
    let mut found_outer_arguments = false;
    let mut outer_argument_count = 1;

    for ch in input.chars() {
        match ch {
            '(' | '[' | '{' => {
                if delimiters.len() >= MAX_COMMAND_NESTING {
                    return Err(format!(
                        "Command input nesting depth exceeds maximum {MAX_COMMAND_NESTING}"
                    ));
                }
                if ch == '[' && delimiters.is_empty() {
                    found_outer_arguments = true;
                }
                delimiters.push(ch);
            }
            ')' | ']' | '}' => {
                let expected_open = match ch {
                    ')' => '(',
                    ']' => '[',
                    '}' => '{',
                    _ => {
                        return Err("Command input contains unbalanced delimiters".into());
                    }
                };
                if delimiters.pop() != Some(expected_open) {
                    return Err("Command input contains unbalanced delimiters".into());
                }
            }
            ',' if found_outer_arguments && delimiters.len() == 1 => {
                outer_argument_count += 1;
                if outer_argument_count > MAX_COMMAND_ARGS {
                    return Err(format!(
                        "Command input exceeds maximum {MAX_COMMAND_ARGS} arguments"
                    ));
                }
            }
            _ => {}
        }
    }

    if !delimiters.is_empty() {
        return Err("Command input contains unbalanced delimiters".into());
    }

    Ok(())
}

fn split_script_commands(script: &str) -> Result<Vec<String>, String> {
    let mut commands = Vec::new();
    let mut delimiters = Vec::new();
    let mut start = 0;

    for (index, ch) in script.char_indices() {
        match ch {
            '(' | '[' | '{' => delimiters.push(ch),
            ')' | ']' | '}' => {
                let expected_open = match ch {
                    ')' => '(',
                    ']' => '[',
                    '}' => '{',
                    _ => {
                        return Err("Script contains unbalanced delimiters".into());
                    }
                };
                if delimiters.pop() != Some(expected_open) {
                    return Err("Script contains unbalanced delimiters".into());
                }
            }
            ';' if delimiters.is_empty() => {
                let command = script[start..index].trim();
                if !command.is_empty() {
                    commands.push(command.to_string());
                }
                start = index + ch.len_utf8();
            }
            _ => {}
        }
    }

    if !delimiters.is_empty() {
        return Err("Script contains unbalanced delimiters".into());
    }

    let command = script[start..].trim();
    if !command.is_empty() {
        commands.push(command.to_string());
    }
    Ok(commands)
}

pub fn insert_implicit_multiplication(text: &str) -> String {
    let mut res = String::new();
    let chars: Vec<char> = text.chars().collect();
    for i in 0..chars.len() {
        res.push(chars[i]);
        if i + 1 < chars.len() {
            let c1 = chars[i];
            let c2 = chars[i + 1];
            let exponent_start = i + 2;
            let scientific_exponent = matches!(c2, 'e' | 'E')
                && (chars
                    .get(exponent_start)
                    .is_some_and(|next| next.is_ascii_digit())
                    || (chars
                        .get(exponent_start)
                        .is_some_and(|next| matches!(next, '+' | '-'))
                        && chars
                            .get(exponent_start + 1)
                            .is_some_and(|next| next.is_ascii_digit())));
            if c1.is_ascii_digit() && c2.is_ascii_alphabetic() && !scientific_exponent {
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

fn is_math_identifier(value: &str) -> bool {
    let mut chars = value.chars();
    chars
        .next()
        .is_some_and(|first| first.is_alphabetic() || first == '_')
        && chars.all(|ch| ch.is_alphanumeric() || ch == '_')
}

fn require_finite(value: Result<f64, String>) -> Result<f64, String> {
    let value = value?;
    if value.is_finite() {
        Ok(value)
    } else {
        Err(format!("No es finito: {value}"))
    }
}

macro_rules! command_result {
    ($result:expr) => {
        match $result {
            Ok(value) => value,
            Err(error) => return error,
        }
    };
}

fn prepare_command_object(document: &Document, mut object: GeoObject) -> GeoObject {
    let label = object.label().to_string();
    if !label.is_empty() {
        object.set_label(unique_object_label(document, &label));
    }
    object
}

fn try_insert_command_object(
    document: &mut Document,
    object: GeoObject,
) -> Result<ObjectId, String> {
    let object = prepare_command_object(document, object);
    document.try_add_object(object)
}

fn try_insert_command_construction(
    document: &mut Document,
    object: GeoObject,
    constraint_name: &str,
    inputs: &[ObjectId],
) -> Result<(ObjectId, usize), String> {
    let object = prepare_command_object(document, object);
    document.try_add_constructed_object(object, constraint_name, inputs)
}

macro_rules! insert_command_object {
    ($document:expr, $object:expr) => {{
        match try_insert_command_object($document, $object) {
            Ok(id) => id,
            Err(error) => return CommandOutcome::Error(error),
        }
    }};
}

macro_rules! insert_typed_command_object {
    ($document:expr, $object:expr) => {{
        match try_insert_command_object($document, $object) {
            Ok(id) => id,
            Err(error) => return Some(Err(error)),
        }
    }};
}

macro_rules! insert_command_construction {
    ($document:expr, $object:expr, $constraint_name:expr, $inputs:expr) => {{
        match try_insert_command_construction($document, $object, $constraint_name, $inputs) {
            Ok(inserted) => inserted,
            Err(error) => return CommandOutcome::Error(error),
        }
    }};
}

fn parse_finite_command_arg(
    command: &str,
    field: &str,
    value: &str,
    variables: &HashMap<String, f64>,
) -> Result<f64, CommandOutcome> {
    require_finite(parse_numeric_arg(value, variables))
        .map_err(|_| CommandOutcome::Error(format!("{command}: {field} debe ser un número finito")))
}

fn parse_optional_finite_command_arg(
    command: &str,
    field: &str,
    args: &[String],
    index: usize,
    default: f64,
    variables: &HashMap<String, f64>,
) -> Result<f64, CommandOutcome> {
    args.get(index).map_or(Ok(default), |value| {
        parse_finite_command_arg(command, field, value, variables)
    })
}

fn parse_positive_regular_polytope_scale(
    command: &str,
    argument: Option<&String>,
    variables: &HashMap<String, f64>,
) -> Result<f64, CommandOutcome> {
    let scale = match argument {
        Some(argument) => require_finite(parse_numeric_arg(argument, variables)),
        None => Ok(1.0),
    }
    .map_err(|_| CommandOutcome::Error(format!("{command}: scale debe ser finito y positivo")))?;
    if scale <= 0.0 {
        return Err(CommandOutcome::Error(format!(
            "{command}: scale debe ser finito y positivo"
        )));
    }
    Ok(scale)
}

fn parse_exact_regular_polytope_rotations(
    command: &str,
    argument: Option<&String>,
    expected_count: usize,
    variables: &HashMap<String, f64>,
) -> Result<Vec<f64>, CommandOutcome> {
    let Some(argument) = argument else {
        return Ok(vec![0.0; expected_count]);
    };
    let rotations = parse_brace_list(argument, variables).map_err(|error| {
        CommandOutcome::Error(format!("{command}: rotaciones invalidas: {error}"))
    })?;
    if rotations.len() != expected_count {
        return Err(CommandOutcome::Error(format!(
            "{command}: se requieren exactamente {expected_count} angulos de rotacion"
        )));
    }
    Ok(rotations)
}

fn parse_regular_polychoron_4d_command_args(
    command: &str,
    args: &[String],
    variables: &HashMap<String, f64>,
) -> Result<(f64, [f64; REGULAR_POLYCHORON_4D_ROTATION_ANGLE_COUNT]), CommandOutcome> {
    let scale = parse_positive_regular_polytope_scale(command, args.first(), variables)?;
    let rotations = parse_exact_regular_polytope_rotations(
        command,
        args.get(1),
        REGULAR_POLYCHORON_4D_ROTATION_ANGLE_COUNT,
        variables,
    )?;
    let rotation_angles: [f64; REGULAR_POLYCHORON_4D_ROTATION_ANGLE_COUNT] = rotations
        .try_into()
        .unwrap_or([0.0; REGULAR_POLYCHORON_4D_ROTATION_ANGLE_COUNT]);
    Ok((scale, rotation_angles))
}

fn parse_regular_polytope_nd_command_args(
    command: &str,
    args: &[String],
    variables: &HashMap<String, f64>,
) -> Result<(usize, f64, Vec<f64>), CommandOutcome> {
    let dimension = args
        .first()
        .and_then(|argument| argument.trim().parse::<usize>().ok())
        .filter(|dimension| {
            (grafito_geometry::MIN_REGULAR_POLYTOPE_DIMENSION
                ..=grafito_geometry::MAX_REGULAR_POLYTOPE_DIMENSION)
                .contains(dimension)
        })
        .ok_or_else(|| {
            CommandOutcome::Error(format!(
                "{command}: n debe ser un entero entre {} y {}",
                grafito_geometry::MIN_REGULAR_POLYTOPE_DIMENSION,
                grafito_geometry::MAX_REGULAR_POLYTOPE_DIMENSION
            ))
        })?;
    let scale = parse_positive_regular_polytope_scale(command, args.get(1), variables)?;
    let Some(expected_rotation_count) =
        RegularPolytopeNDObj::expected_rotation_angle_count(dimension)
    else {
        return Err(CommandOutcome::Error(format!(
            "{command}: dimensión no soportada para rotaciones"
        )));
    };
    let rotation_angles = parse_exact_regular_polytope_rotations(
        command,
        args.get(2),
        expected_rotation_count,
        variables,
    )?;
    Ok((dimension, scale, rotation_angles))
}

fn parse_i32_command_arg(command: &str, field: &str, value: &str) -> Result<i32, CommandOutcome> {
    value
        .trim()
        .parse::<i32>()
        .map_err(|_| CommandOutcome::Error(format!("{command}: {field} debe ser un entero válido")))
}

fn require_finite_outputs(command: &str, outputs: &[f64]) -> Result<(), CommandOutcome> {
    if outputs.iter().all(|value| value.is_finite()) {
        Ok(())
    } else {
        Err(CommandOutcome::Error(format!(
            "{command}: el resultado no es finito o no está soportado"
        )))
    }
}

fn parse_data_command_arg(
    command: &str,
    value: &str,
    variables: &HashMap<String, f64>,
) -> Result<Vec<f64>, CommandOutcome> {
    parse_brace_list(value, variables)
        .map_err(|error| CommandOutcome::Error(format!("{command}: {error}")))
}

fn data_table_for_fit(
    document: &Document,
    command: &str,
    label: &str,
) -> Result<(ObjectId, Vec<f64>, Vec<f64>), CommandOutcome> {
    let label = label.trim().trim_matches('"');
    let id = document
        .try_find_object_by_label(label)
        .map_err(|error| CommandOutcome::Error(format!("{command}: {error}")))?
        .ok_or_else(|| {
            CommandOutcome::Error(format!("{command}: tabla '{label}' no encontrada"))
        })?;
    let Some(GeoObject::DataTable(table)) = document.get_object(id) else {
        return Err(CommandOutcome::Error(format!(
            "{command}: '{label}' debe ser una tabla de datos local"
        )));
    };
    Ok((id, table.xs.clone(), table.ys.clone()))
}

fn fit_function_from_table(
    command: &str,
    source: ObjectId,
    xs: &[f64],
    ys: &[f64],
    kind: statistics::FitKind,
) -> Result<(FunctionObj, String), CommandOutcome> {
    let fit = statistics::fit_xy(kind, xs, ys)
        .map_err(|error| CommandOutcome::Error(format!("{command}: {error}")))?;
    let expression = fit.expression();
    if expression.is_empty() {
        return Err(CommandOutcome::Error(format!(
            "{command}: no se pudo representar el modelo ajustado"
        )));
    }
    let x_min = xs.iter().copied().fold(f64::INFINITY, f64::min);
    let x_max = xs.iter().copied().fold(f64::NEG_INFINITY, f64::max);
    if !x_min.is_finite() || !x_max.is_finite() || x_min >= x_max {
        return Err(CommandOutcome::Error(format!(
            "{command}: la tabla requiere variación finita de x"
        )));
    }

    let diagnostics = fit.diagnostics.clone();
    let mut function = FunctionObj::new(expression).with_label(command);
    function.color = Color::RED;
    function.width = 2.5;
    function.domain_min = Some(x_min);
    function.domain_max = Some(x_max);
    function = function.with_fit(FitMetadata::from_result(source, fit));
    Ok((
        function,
        format!(
            "Ajuste {}: RMSE={:.6}, R²={:.6}",
            kind.display_name(),
            diagnostics.rmse,
            diagnostics.r_squared
        ),
    ))
}

fn require_finite_curve_points(command: &str, points: &[Point2]) -> Result<(), CommandOutcome> {
    if points.len() < 3 {
        return Err(CommandOutcome::Error(format!(
            "{command}: no se pudieron generar suficientes puntos"
        )));
    }
    if points
        .iter()
        .all(|point| point.x.is_finite() && point.y.is_finite())
    {
        Ok(())
    } else {
        Err(CommandOutcome::Error(format!(
            "{command}: la curva produjo coordenadas no finitas"
        )))
    }
}

fn require_ordered_domain(
    command: &str,
    min_name: &str,
    max_name: &str,
    min: f64,
    max: f64,
) -> Result<(), CommandOutcome> {
    if min < max {
        Ok(())
    } else {
        Err(CommandOutcome::Error(format!(
            "{command}: se requiere {min_name} < {max_name} con ambos valores finitos"
        )))
    }
}

fn parse_rect_bounds(
    command: &str,
    args: &[String],
    variables: &HashMap<String, f64>,
    defaults: (f64, f64, f64, f64),
) -> Result<(f64, f64, f64, f64), CommandOutcome> {
    let x_min = args.get(1).map_or(Ok(defaults.0), |value| {
        parse_finite_command_arg(command, "x_min", value, variables)
    })?;
    let x_max = args.get(2).map_or(Ok(defaults.1), |value| {
        parse_finite_command_arg(command, "x_max", value, variables)
    })?;
    let y_min = args.get(3).map_or(Ok(defaults.2), |value| {
        parse_finite_command_arg(command, "y_min", value, variables)
    })?;
    let y_max = args.get(4).map_or(Ok(defaults.3), |value| {
        parse_finite_command_arg(command, "y_max", value, variables)
    })?;

    if x_min < x_max && y_min < y_max {
        Ok((x_min, x_max, y_min, y_max))
    } else {
        Err(CommandOutcome::Error(format!(
            "{command}: se requiere x_min < x_max e y_min < y_max con límites finitos"
        )))
    }
}

fn parse_parametric_surface_components(value: &str) -> Option<[String; 3]> {
    let inner = value.trim().strip_prefix('(')?.strip_suffix(')')?;
    let components = split_args(inner);
    if components.len() != 3
        || components
            .iter()
            .any(|component| component.trim().is_empty())
    {
        return None;
    }
    components.try_into().ok()
}

fn normalize_parametric_surface_components(
    mut components: [String; 3],
) -> Result<[String; 3], CommandOutcome> {
    let uses_uv = components.iter().any(|component| {
        replace_variable(component, "u", "__grafito_surface_u__") != *component
            || replace_variable(component, "v", "__grafito_surface_v__") != *component
    });
    let uses_xy = components.iter().any(|component| {
        replace_variable(component, "x", "__grafito_surface_x__") != *component
            || replace_variable(component, "y", "__grafito_surface_y__") != *component
    });
    if uses_uv && uses_xy {
        return Err(CommandOutcome::Error(
            "Surface3D: usá parámetros u,v o x,y, pero no ambos en la misma superficie".into(),
        ));
    }
    if uses_xy {
        for component in &mut components {
            *component = replace_variable(component, "x", "u");
            *component = replace_variable(component, "y", "v");
        }
    }
    Ok(components)
}

fn validate_parametric_surface_expression(
    component: &str,
    name: &str,
    variables: &HashMap<String, f64>,
) -> Result<(), CommandOutcome> {
    prepare_function_ast(component.trim(), variables, &["u", "v"]).map_err(|error| {
        CommandOutcome::Error(format!(
            "Surface3D: expresión {name} inválida para los parámetros de la superficie: {error}"
        ))
    })?;
    Ok(())
}

fn validate_curve_3d_expression(
    component: &str,
    name: &str,
    parameter: &str,
    variables: &HashMap<String, f64>,
) -> Result<(), CommandOutcome> {
    prepare_function_ast(component.trim(), variables, &[parameter]).map_err(|error| {
        CommandOutcome::Error(format!(
            "Curve3D: expresión {name} inválida para el parámetro {parameter}: {error}"
        ))
    })?;
    Ok(())
}

fn parse_discrete_count(command: &str, name: &str, value: &str) -> Result<u32, CommandOutcome> {
    let count = value.trim().parse::<u32>().map_err(|_| {
        CommandOutcome::Error(format!(
            "{command}: {name} must be an integer between 0 and {MAX_DISCRETE_COUNT}"
        ))
    })?;
    if count > MAX_DISCRETE_COUNT {
        return Err(CommandOutcome::Error(format!(
            "{command}: {name} {count} exceeds maximum {MAX_DISCRETE_COUNT}"
        )));
    }
    Ok(count)
}

fn parse_taylor_order(value: Option<&str>) -> Result<usize, CommandOutcome> {
    let Some(value) = value else {
        return Ok(5);
    };
    let order = value.trim().parse::<usize>().map_err(|_| {
        CommandOutcome::Error(format!(
            "Taylor: order must be an integer between 0 and {MAX_TAYLOR_ORDER}"
        ))
    })?;
    if order > MAX_TAYLOR_ORDER {
        return Err(CommandOutcome::Error(format!(
            "Taylor: order {order} exceeds maximum {MAX_TAYLOR_ORDER}"
        )));
    }
    Ok(order)
}

fn parse_fractal_max_iter(command: &str, value: Option<&str>) -> Result<u32, CommandOutcome> {
    let Some(value) = value.filter(|value| !value.trim().is_empty()) else {
        return Ok(256);
    };
    let max_iter = value.trim().parse::<u32>().map_err(|_| {
        CommandOutcome::Error(format!(
            "{command}: max_iter debe ser un entero no negativo"
        ))
    })?;
    if max_iter > grafito_geometry::fractals::MAX_FRACTAL_ITER {
        return Err(CommandOutcome::Error(format!(
            "{command}: max_iter {max_iter} exceeds maximum {}",
            grafito_geometry::fractals::MAX_FRACTAL_ITER
        )));
    }
    Ok(max_iter)
}

fn validate_fractal_command_budget(
    command: &str,
    fractal: &Fractal2DObj,
) -> Result<(), CommandOutcome> {
    grafito_geometry::fractals::validate_fractal_budget(
        fractal.resolution,
        fractal.resolution,
        fractal.max_iter,
    )
    .map_err(|error| CommandOutcome::Error(format!("{command}: {error}")))
}

/// Parse attractor parameters, supporting key=value syntax.
fn parse_attractor_params(
    command: &str,
    args: &[String],
    variables: &std::collections::HashMap<String, f64>,
    defaults: &[f64],
) -> Result<Vec<f64>, CommandOutcome> {
    if args.len() > defaults.len() {
        return Err(CommandOutcome::Error(format!(
            "{command}: se esperaban como máximo {} parámetros",
            defaults.len()
        )));
    }
    let mut parameters = defaults.to_vec();
    for (index, argument) in args.iter().enumerate() {
        let rhs = argument.split('=').next_back().unwrap_or(argument).trim();
        parameters[index] = require_finite(parse_numeric_arg(rhs, variables)).map_err(|error| {
            CommandOutcome::Error(format!(
                "{command}: parámetro {} inválido: {error}",
                index + 1
            ))
        })?;
    }
    Ok(parameters)
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

struct NaturalIntegralDefinition {
    label: String,
    output_var: String,
    integration_var: String,
    expression: String,
}

/// Reconoce la notación manuscrita/teclado `f(x): ∫e−x2dx`.
///
/// Una integral indefinida no fija una constante de integración. Para que el
/// resultado sea graficable y determinista, se interpreta como la acumulada
/// `f(x) = ∫₀ˣ integrando dvariable`.
fn parse_natural_integral_definition(
    text: &str,
) -> Option<Result<NaturalIntegralDefinition, String>> {
    let integral_start = text.find('∫')?;
    let prefix = text[..integral_start].trim();
    let integrand_and_differential = text[integral_start + '∫'.len_utf8()..].trim();

    let function_lhs = prefix
        .strip_suffix(':')
        .or_else(|| prefix.strip_suffix('='))
        .map(str::trim)
        .filter(|lhs| is_function_lhs(lhs));
    let Some(function_lhs) = function_lhs else {
        return Some(Err(
            "Integral: usa la forma f(x): ∫integrando dx".to_string()
        ));
    };
    let Some((label, output_var)) = function_lhs.split_once('(') else {
        return Some(Err(
            "Integral: la función debe tener la forma f(x)".to_string()
        ));
    };
    let output_var = output_var.trim_end_matches(')').trim();
    if label.trim().is_empty()
        || output_var.chars().count() != 1
        || !output_var
            .chars()
            .all(|character| character.is_ascii_alphabetic())
    {
        return Some(Err(
            "Integral: la función debe tener una variable de una letra".to_string(),
        ));
    }

    let compact = integrand_and_differential
        .chars()
        .filter(|character| !character.is_whitespace())
        .collect::<String>();
    let Some(differential_start) = compact.rfind('d') else {
        return Some(Err(
            "Integral: falta el diferencial final, por ejemplo dx".to_string()
        ));
    };
    let integration_var = compact[differential_start + 1..].trim();
    if integration_var.chars().count() != 1
        || !integration_var
            .chars()
            .all(|character| character.is_ascii_alphabetic())
    {
        return Some(Err(
            "Integral: el diferencial debe ser una variable de una letra, por ejemplo dx"
                .to_string(),
        ));
    }

    let integrand = compact[..differential_start].trim_end_matches('*').trim();
    if integrand.is_empty() {
        return Some(Err("Integral: falta el integrando".to_string()));
    }

    let integration_var = integration_var.to_string();
    let expression = normalize_natural_integrand(integrand, &integration_var);
    Some(Ok(NaturalIntegralDefinition {
        label: label.trim().to_string(),
        output_var: output_var.to_string(),
        integration_var,
        expression,
    }))
}

fn normalize_natural_integrand(integrand: &str, variable: &str) -> String {
    let compact_square = integrand.replace(&format!("{variable}2"), &format!("{variable}^2"));
    if let Some(exponent) = compact_square.strip_prefix("e-") {
        return format!("exp(-{exponent})");
    }
    if let Some(exponent) = compact_square.strip_prefix("e^") {
        return format!("exp({exponent})");
    }
    compact_square
}

fn validate_command_arity(command: &CasCmd) -> Result<(), String> {
    if let Some(spec) = crate::command_registry::resolve(&command.command) {
        if spec.accepts_argument_count(command.args.len()) {
            return Ok(());
        }
        let signatures = spec
            .signatures
            .iter()
            .map(|signature| signature.syntax)
            .collect::<Vec<_>>()
            .join(" o ");
        return Err(format!(
            "{}: cantidad de argumentos inválida; usa {}",
            spec.canonical, signatures
        ));
    }

    let bounds = match command.command.as_str() {
        "Gamma" | "LnGamma" | "Erf" | "Erfc" | "Digamma" | "Cardioid" => (1, 1),
        "Beta"
        | "BesselJ"
        | "BesselY"
        | "BesselI"
        | "TTest"
        | "TTest2"
        | "ChiSqTest"
        | "Epicycloid"
        | "Hypocycloid"
        | "SetValue"
        | "CircleByCenterRadius" => (2, 2),
        "Uniform" | "GammaDist" | "BetaDist" | "Cauchy" | "Pareto" | "Laplace" | "NegBinomial"
        | "CIProportion" => (2, 3),
        "Rayleigh" | "CIMean" => (1, 2),
        "ZTest" | "Rose" | "ArchimedeanSpiral" | "LogarithmicSpiral" => (3, 3),
        "Cofactor" | "LaplaceExpansion" => (3, 3),
        "Lissajous" => (5, 5),
        "Quadrants" => (0, 4),
        "ODE" => (4, 7),
        "ODESystem" => (5, 9),
        "Script" => (1, 1),
        _ => return Ok(()),
    };
    if (bounds.0..=bounds.1).contains(&command.args.len()) {
        Ok(())
    } else if command.command == "Script" {
        Err("Script expects exactly one argument".into())
    } else {
        Err(format!(
            "{}: cantidad de argumentos inválida (esperados {}..={}, recibidos {})",
            command.command,
            bounds.0,
            bounds.1,
            command.args.len()
        ))
    }
}

fn validate_command_label_ambiguity(document: &Document, command: &CasCmd) -> Result<(), String> {
    fn validate_candidate(
        document: &Document,
        command: &str,
        candidate: &str,
    ) -> Result<(), String> {
        let label = clean_label(candidate);
        if label.is_empty() {
            return Ok(());
        }
        document
            .try_find_object_by_label(label)
            .map_err(|error| format!("{command}: {error}"))?;
        if let Some((base_label, _)) = label.split_once('(') {
            let base_label = base_label.trim();
            if !base_label.is_empty() && base_label != label {
                document
                    .try_find_object_by_label(base_label)
                    .map_err(|error| format!("{command}: {error}"))?;
            }
        }
        Ok(())
    }

    let indices: &[usize] = match command.command.as_str() {
        "Root"
        | "Extremum"
        | "Inflection"
        | "YIntercept"
        | "XIntercept"
        | "Centroid"
        | "Analyze"
        | "Area"
        | "Circumference"
        | "Center"
        | "Horizontal"
        | "Vertical"
        | "Translate"
        | "Length"
        | "Slope"
        | "SetValue"
        | "Extrude"
        | "Erase"
        | "CircleByCenterRadius"
        | "Derivative"
        | "Integral"
        | "Solve"
        | "Taylor"
        | "Factor"
        | "Expand"
        | "Simplify"
        | "Limit"
        | "TangentAt"
        | "NormalAt"
        | "ArcLength"
        | "CurvatureAt"
        | "VolumeOfRevolution"
        | "SurfaceOfRevolution"
        | "FitLinear"
        | "FitExp"
        | "FitLog"
        | "FitPow"
        | "FitSin"
        | "FitPoly" => &[0],
        "Distance"
        | "Intersect"
        | "Angle"
        | "Tangent"
        | "Coincident"
        | "EqualLength"
        | "ParabolaByFocusDirectrix"
        | "PolygonUnion"
        | "PolygonIntersection"
        | "PolygonDifference"
        | "PolygonXor"
        | "Midpoint"
        | "Perpendicular"
        | "Parallel"
        | "PointOnObject"
        | "EquidistantFrom"
        | "Projection3D"
        | "PlaneThroughLines"
        | "PlaneThroughLinePoint"
        | "LineRelation3D"
        | "Locus" => &[0, 1],
        "Symmetry" | "EllipseByFoci" | "HyperbolaByFoci" | "CircleByThreePoints" | "Reflect" => {
            &[0, 1, 2]
        }
        "ConicByFivePoints" => &[0, 1, 2, 3, 4],
        "Dilate" => &[0, 2],
        "Rotate" if command.args.len() == 3 => &[0, 1],
        "Rotate" => &[0],
        "Line3D" if command.args.len() == 2 => &[0, 1],
        "Plane3D" if command.args.len() == 3 => &[0, 1, 2],
        "Intersection3D" if command.args.len() == 3 => &[0, 1, 2],
        "Intersection3D" => &[0, 1],
        "ComplexMapping" | "ComplexIntegral" | "Gauss" => &[1],
        _ => &[],
    };
    for index in indices {
        if let Some(argument) = command.args.get(*index) {
            validate_candidate(document, &command.command, argument)?;
        }
    }
    if command.command == "Solve3DGeometry" {
        if let Some(equation) = command.args.first() {
            if let Some((label_a, label_b)) =
                parse_distance_equality_labels(equation.trim().trim_matches('"'))
            {
                validate_candidate(document, &command.command, label_a)?;
                validate_candidate(document, &command.command, label_b)?;
            }
        }
    }
    for argument in &command.args {
        if let Some(nested) = parse_cas_command(argument) {
            validate_command_label_ambiguity(document, &nested)?;
        }
    }
    Ok(())
}

fn auto_define_variables(text: &str, document: &mut Document) -> HashSet<String> {
    let mut current_word = String::new();
    let mut words = Vec::new();
    let mut created = HashSet::new();
    for c in text.chars() {
        if c.is_alphabetic() {
            current_word.push(c);
        } else if !current_word.is_empty() {
            words.push(current_word.clone());
            current_word.clear();
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
        document.set_variable(word.clone(), 1.0);
        created.insert(word);
    }

    created
}

/// Ejecuta un comando como una transacción de compatibilidad.
///
/// El intérprete actual muta el documento durante parseo y ejecución. Mientras
/// la capa de `OperationBatch` no exista, usar un documento staged evita que
/// errores de sintaxis o validación dejen efectos parciales persistidos.
pub fn process_input(document: &mut Document, input_text: &mut String) -> CommandOutcome {
    if let Err(message) = validate_command_input(input_text.trim()) {
        return CommandOutcome::Error(message);
    }
    let mut staged = document.detached_clone_for_staging();
    let outcome = process_input_in_place(&mut staged, input_text);
    if !matches!(outcome, CommandOutcome::Error(_)) {
        if let Err(error) = grafito_core::validation::validate_document(&staged) {
            return CommandOutcome::Error(format!("Document validation failed: {error}"));
        }
        let changed = match (
            serde_json::to_value(&*document),
            serde_json::to_value(&staged),
        ) {
            (Ok(before), Ok(after)) => before != after,
            (Err(error), _) | (_, Err(error)) => {
                return CommandOutcome::Error(format!(
                    "Document transaction could not compare semantic state: {error}"
                ));
            }
        };
        if changed {
            staged.version = document.version.wrapping_add(1);
            staged.spatial_dirty = true;
            *document = staged;
        }
    }
    outcome
}

/// Ejecuta una celda CAS local y conserva su resultado histórico en la misma
/// transacción que cualquier geometría creada por el comando.
///
/// Los errores también producen una celda, pero se descartan todos los efectos
/// parciales del intérprete antes de persistir el diagnóstico.
pub fn process_cas_worksheet_cell(document: &mut Document, input: &str) -> CommandOutcome {
    let input = input.trim();
    if input.is_empty() {
        return CommandOutcome::Ok;
    }
    if let Err(error) = document.validate_cas_worksheet_input(input) {
        return CommandOutcome::Error(error);
    }

    let mut parser_input = input.to_string();
    let mut evaluated = document.detached_clone_for_staging();
    let evaluated_outcome = process_input_in_place(&mut evaluated, &mut parser_input);
    let (mut staged, outcome, output, status) = match evaluated_outcome {
        CommandOutcome::Ok => (
            evaluated,
            CommandOutcome::Ok,
            "Comando completado".to_string(),
            CasWorksheetStatus::Success,
        ),
        CommandOutcome::Message(message)
            if message.len() <= Document::MAX_CAS_WORKSHEET_OUTPUT_BYTES =>
        {
            (
                evaluated,
                CommandOutcome::Message(message.clone()),
                message,
                CasWorksheetStatus::Success,
            )
        }
        CommandOutcome::Error(message)
            if message.len() <= Document::MAX_CAS_WORKSHEET_OUTPUT_BYTES =>
        {
            (
                document.detached_clone_for_staging(),
                CommandOutcome::Error(message.clone()),
                message,
                CasWorksheetStatus::Error,
            )
        }
        CommandOutcome::Message(_) | CommandOutcome::Error(_) => {
            let message = format!(
                "CAS worksheet output exceeds the {} byte limit",
                Document::MAX_CAS_WORKSHEET_OUTPUT_BYTES
            );
            (
                document.detached_clone_for_staging(),
                CommandOutcome::Error(message.clone()),
                message,
                CasWorksheetStatus::Error,
            )
        }
    };

    if let Err(error) = staged.try_append_cas_worksheet_cell(input.to_string(), output, status) {
        return CommandOutcome::Error(error);
    }

    staged.version = document.version.wrapping_add(1);
    staged.spatial_dirty = true;
    *document = staged;
    outcome
}

fn process_input_in_place(document: &mut Document, input_text: &mut String) -> CommandOutcome {
    let mut script_budget = ScriptBudget::default();
    process_input_in_place_with_budget(document, input_text, &mut script_budget)
}

fn process_input_in_place_with_budget(
    document: &mut Document,
    input_text: &mut String,
    script_budget: &mut ScriptBudget,
) -> CommandOutcome {
    let raw_text = input_text.trim().to_string();
    if raw_text.is_empty() {
        return CommandOutcome::Ok;
    }
    if let Err(message) = validate_command_input(&raw_text) {
        return CommandOutcome::Error(message);
    }

    // ── Batch: pegar múltiples comandos en líneas separadas (ej: Segment + 4 Points) ──
    // Grafito históricamente solo acepta un comando por submit; al pegar 5 líneas
    // `parse_cas_command` ve texto tras el primer `]` y devuelve error
    // "no se permite texto después del corchete final". Detectar batch y ejecutar secuencialmente.
    if raw_text.contains('\n') {
        let lines: Vec<String> = raw_text
            .lines()
            .map(|l| l.trim().to_string())
            .filter(|l| !l.is_empty() && !l.eq_ignore_ascii_case("grafito"))
            .collect();
        if lines.len() > 1 && lines.len() <= MAX_SCRIPT_COMMANDS {
            let is_batch = lines.iter().all(|l| {
                let t = l.trim();
                // Cada línea debe parecer comando bracketed, asignación o expresión con paréntesis
                looks_like_bracketed_command(t)
                    || t.contains('=')
                    || t.starts_with('(')
                    || parse_cas_command(t).is_some()
                    || t.eq_ignore_ascii_case("eraseall")
            });
            if is_batch {
                let mut last_outcome = CommandOutcome::Ok;
                let mut any_error = None;
                for line in &lines {
                    if script_budget.executed_commands >= MAX_SCRIPT_COMMANDS {
                        return CommandOutcome::Error(format!(
                            "Batch excede el límite de {MAX_SCRIPT_COMMANDS} comandos"
                        ));
                    }
                    script_budget.executed_commands += 1;
                    let mut line_buf = line.clone();
                    let outcome =
                        process_input_in_place_with_budget(document, &mut line_buf, script_budget);
                    match &outcome {
                        CommandOutcome::Error(msg) => {
                            // Reportar línea fallida con contexto, pero seguir para diagnóstico completo
                            any_error = Some(format!("{line}: {msg}"));
                            last_outcome = outcome;
                            break;
                        }
                        _ => last_outcome = outcome,
                    }
                }
                if let Some(err) = any_error {
                    return CommandOutcome::Error(err);
                }
                input_text.clear();
                return last_outcome;
            }
        }
    }

    // Sanitize mathematical unicode symbols from the virtual keyboard.
    // Do not rewrite global `X`/`Y`: command names and object labels are case-sensitive.
    let text = raw_text
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
        .replace("−", "-")
        .replace("≤", "<=")
        .replace("≥", ">=")
        // **Cubos y potencias mayores**: x³, x⁴, etc. (sufijos Unicode U+00B3, U+2074, etc.)
        .replace("x³", "x^3")
        .replace("y³", "y^3")
        .replace("z³", "z^3");

    if let Err(message) = validate_command_input(&text) {
        return CommandOutcome::Error(message);
    }

    if let Some(definition) = parse_natural_integral_definition(&text) {
        let definition = match definition {
            Ok(definition) => definition,
            Err(error) => return CommandOutcome::Error(error),
        };
        if let Err(error) = prepare_function_ast(
            &definition.expression,
            &document.variables,
            &[definition.integration_var.as_str()],
        ) {
            return CommandOutcome::Error(format!(
                "Integral: no se pudo interpretar el integrando '{}': {error}",
                definition.expression
            ));
        }
        match document.try_find_object_by_label(&definition.label) {
            Ok(Some(id)) => {
                document.remove_object(id);
            }
            Ok(None) => {}
            Err(error) => return CommandOutcome::Error(format!("Integral: {error}")),
        }
        let object = FunctionObj::new(&definition.expression)
            .with_label(&definition.label)
            .as_integral(&definition.integration_var, 0.0);
        insert_command_object!(document, GeoObject::Function(object));
        input_text.clear();
        return CommandOutcome::Message(format!(
            "{}({}) = ∫₀ˣ {} d{} → graficada",
            definition.label,
            definition.output_var,
            definition.expression,
            definition.integration_var
        ));
    }

    let parsed_cas_command = parse_cas_command(&text);
    if parsed_cas_command.is_none() && looks_like_bracketed_command(&text) {
        return CommandOutcome::Error(
            "Sintaxis de comando inválida: no se permite texto después del corchete final".into(),
        );
    }

    if let Some(command) = parsed_cas_command.as_ref() {
        if let Err(error) = validate_command_arity(command) {
            return CommandOutcome::Error(error);
        }
        if let Err(error) = validate_command_label_ambiguity(document, command) {
            return CommandOutcome::Error(error);
        }
    } else {
        auto_define_variables(&text, document);
    }

    let mut result: CommandOutcome = CommandOutcome::Ok;

    if let Some(mut cmd) = parsed_cas_command {
        // A Script body contains nested command identifiers such as Segment3D.
        // Each nested command normalizes its own arguments after splitting.
        if cmd.command != "Script" {
            cmd.args = cmd
                .args
                .iter()
                .map(|a| insert_implicit_multiplication(a))
                .collect();
        }
        match cmd.command.as_str() {
            "Point" if cmd.args.len() == 1 => {
                let point = match parse_finite_point_arg(&cmd.args[0], &document.variables) {
                    Ok(point) => point,
                    Err(error) => return CommandOutcome::Error(format!("Point: {error}")),
                };
                insert_command_object!(document, GeoObject::Point(PointObj::new(point)));
                input_text.clear();
                return CommandOutcome::Ok;
            }
            "Circle" if cmd.args.len() == 2 => {
                let center = match parse_finite_point_arg(&cmd.args[0], &document.variables) {
                    Ok(point) => point,
                    Err(error) => return CommandOutcome::Error(format!("Circle: {error}")),
                };
                let radius =
                    match require_finite(parse_numeric_arg(&cmd.args[1], &document.variables)) {
                        Ok(radius) if radius > 0.0 => radius,
                        _ => {
                            return CommandOutcome::Error(
                                "Circle: el radio debe ser finito y positivo".into(),
                            )
                        }
                    };
                insert_command_object!(document, GeoObject::Circle(CircleObj::new(center, radius)));
                input_text.clear();
                return CommandOutcome::Ok;
            }
            "Polygon" if cmd.args.len() >= 3 => {
                let mut vertices = Vec::with_capacity(cmd.args.len());
                for argument in &cmd.args {
                    match parse_finite_point_arg(argument, &document.variables) {
                        Ok(point) => vertices.push(point),
                        Err(error) => {
                            return CommandOutcome::Error(format!("Polygon: {error}"));
                        }
                    }
                }
                insert_command_object!(document, GeoObject::Polygon(PolygonObj::new(vertices)));
                input_text.clear();
                return CommandOutcome::Ok;
            }
            "Ellipse" if cmd.args.len() == 3 => {
                let center =
                    command_result!(parse_finite_point_arg(&cmd.args[0], &document.variables,)
                        .map_err(|error| CommandOutcome::Error(format!("Ellipse: {error}"))));
                let rx = command_result!(parse_finite_command_arg(
                    "Ellipse",
                    "rx",
                    &cmd.args[1],
                    &document.variables,
                ));
                let ry = command_result!(parse_finite_command_arg(
                    "Ellipse",
                    "ry",
                    &cmd.args[2],
                    &document.variables,
                ));
                if rx <= 0.0 || ry <= 0.0 {
                    return CommandOutcome::Error(
                        "Ellipse: los semiejes deben ser finitos y positivos".into(),
                    );
                }
                insert_command_object!(
                    document,
                    GeoObject::Ellipse(EllipseObj::new(center, rx, ry))
                );
                input_text.clear();
                return CommandOutcome::Ok;
            }
            "RegularPolygon" if cmd.args.len() == 3 => {
                let center =
                    command_result!(parse_finite_point_arg(&cmd.args[0], &document.variables,)
                        .map_err(|error| CommandOutcome::Error(format!(
                            "RegularPolygon: {error}"
                        ))));
                let n = match cmd.args[1].trim().parse::<usize>() {
                    Ok(n) if (3..=64).contains(&n) => n,
                    _ => {
                        return CommandOutcome::Error(
                            "RegularPolygon: n debe ser un entero entre 3 y 64".into(),
                        )
                    }
                };
                let radius = command_result!(parse_finite_command_arg(
                    "RegularPolygon",
                    "radio",
                    &cmd.args[2],
                    &document.variables,
                ));
                if radius <= 0.0 {
                    return CommandOutcome::Error(
                        "RegularPolygon: el radio debe ser finito y positivo".into(),
                    );
                }
                let verts: Vec<Point2> = (0..n)
                    .map(|i| {
                        let angle = i as f64 / n as f64 * std::f64::consts::TAU;
                        Point2::new(
                            center.x + radius * angle.cos(),
                            center.y + radius * angle.sin(),
                        )
                    })
                    .collect();
                insert_command_object!(document, GeoObject::Polygon(PolygonObj::new(verts)));
                input_text.clear();
                return CommandOutcome::Ok;
            }
            "Distance" if matches!(cmd.args.len(), 2 | 3) => {
                if let (Some(a), Some(b)) = (
                    find_object_by_label(document, cmd.args[0].trim()),
                    find_object_by_label(document, cmd.args[1].trim()),
                ) {
                    let target = match cmd.args.get(2) {
                        Some(value) => command_result!(parse_finite_command_arg(
                            "Distance",
                            "valor",
                            value,
                            &document.variables,
                        )),
                        None => {
                            if let (Some(p1), Some(p2)) =
                                (document.point_position(a), document.point_position(b))
                            {
                                p1.distance(&p2)
                            } else {
                                0.0
                            }
                        }
                    };
                    if let Err(error) = document.try_add_distance_constraint(a, b, target) {
                        return CommandOutcome::Error(error);
                    }
                    if let Err(error) = document.try_re_evaluate_constraints(&[]) {
                        return CommandOutcome::Error(error);
                    }
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
                                insert_command_object!(document, GeoObject::Point(pt));
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
                                insert_command_object!(
                                    document,
                                    GeoObject::Point(PointObj::new(r))
                                );
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
                            insert_command_object!(document, GeoObject::Polygon(fill_poly));
                            let txt = grafito_core::TextObj::new(
                                format!("A = {:.3}", area),
                                Point2::new(cx, cy),
                            );
                            insert_command_object!(document, GeoObject::Text(txt));
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
                            insert_command_object!(
                                document,
                                GeoObject::Point(PointObj::new(c).with_label(&new_label),)
                            );
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
                    insert_command_object!(document, GeoObject::Polygon(sector_poly));
                    let txt = grafito_core::TextObj::new(
                        format!(
                            "Sector: r={:.2}  θ={:.1}°  A={:.3}  L={:.3}",
                            r, deg, area, arc_len
                        ),
                        Point2::new(cx, cy),
                    );
                    insert_command_object!(document, GeoObject::Text(txt));
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
                    let arc_len = r * (t2 - t1).abs();
                    let mut arc = ParametricCurve2DObj::new(
                        &format!("({cx}) + ({r}) * cos(t)"),
                        &format!("({cy}) + ({r}) * sin(t)"),
                        t1,
                        t2,
                    )
                    .with_label("Arc");
                    arc.color = grafito_geometry::Color::new(0.9, 0.5, 0.2, 1.0);
                    arc.width = 2.0;
                    insert_command_object!(document, GeoObject::ParametricCurve2D(arc));
                    let txt = grafito_core::TextObj::new(
                        format!("Arco: r={:.2}  L={:.3}", r, arc_len),
                        Point2::new(cx, cy),
                    );
                    insert_command_object!(document, GeoObject::Text(txt));
                    input_text.clear();
                    return CommandOutcome::Message(format!(
                        "Arco creado: r={:.2}, {:.1}° → {:.1}°",
                        r, deg1, deg2
                    ));
                }
                return CommandOutcome::Error("Arc: argumentos inválidos".into());
            }
            "Angle" if matches!(cmd.args.len(), 2 | 3) => {
                if let (Some(a), Some(b)) = (
                    find_object_by_label(document, cmd.args[0].trim()),
                    find_object_by_label(document, cmd.args[1].trim()),
                ) {
                    let target = match cmd.args.get(2) {
                        Some(value) => command_result!(parse_finite_command_arg(
                            "Angle",
                            "grados",
                            value,
                            &document.variables,
                        )),
                        None => 0.0,
                    };
                    if let Err(error) = document.try_add_angle_constraint(a, b, target) {
                        return CommandOutcome::Error(error);
                    }
                    if let Err(error) = document.try_re_evaluate_constraints(&[]) {
                        return CommandOutcome::Error(error);
                    }
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
                    if let Err(error) = document.try_add_tangent_constraint(a, b) {
                        return CommandOutcome::Error(error);
                    }
                    if let Err(error) = document.try_re_evaluate_constraints(&[]) {
                        return CommandOutcome::Error(error);
                    }
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
                    if let Err(error) = document.try_add_coincident_constraint(a, b) {
                        return CommandOutcome::Error(error);
                    }
                    if let Err(error) = document.try_re_evaluate_constraints(&[]) {
                        return CommandOutcome::Error(error);
                    }
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
                    if let Err(error) = document.try_add_horizontal_constraint(id) {
                        return CommandOutcome::Error(error);
                    }
                    if let Err(error) = document.try_re_evaluate_constraints(&[]) {
                        return CommandOutcome::Error(error);
                    }
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
                    if let Err(error) = document.try_add_vertical_constraint(id) {
                        return CommandOutcome::Error(error);
                    }
                    if let Err(error) = document.try_re_evaluate_constraints(&[]) {
                        return CommandOutcome::Error(error);
                    }
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
                    if let Err(error) = document.try_add_equal_length_constraint(a, b) {
                        return CommandOutcome::Error(error);
                    }
                    if let Err(error) = document.try_re_evaluate_constraints(&[]) {
                        return CommandOutcome::Error(error);
                    }
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
                    if let Err(error) = document.try_add_symmetry_constraint(*p, *q, *line) {
                        return CommandOutcome::Error(error);
                    }
                    if let Err(error) = document.try_re_evaluate_constraints(&[]) {
                        return CommandOutcome::Error(error);
                    }
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
                    insert_command_construction!(
                        document,
                        GeoObject::Ellipse(EllipseObj::new(Point2::new(0.0, 0.0), 1.0, 1.0)),
                        "EllipseByFoci",
                        &[*f1, *f2, *p]
                    );
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
                    insert_command_construction!(
                        document,
                        GeoObject::Parabola(ParabolaObj::new(Point2::new(0.0, 0.0), 1.0)),
                        "ParabolaByFocusDirectrix",
                        &[focus, directrix]
                    );
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
                    insert_command_construction!(
                        document,
                        GeoObject::Hyperbola(HyperbolaObj::new(Point2::new(0.0, 0.0), 1.0, 1.0,)),
                        "HyperbolaByFoci",
                        &[*f1, *f2, *p]
                    );
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
                    let collision = match document.try_find_object_by_label("C") {
                        Ok(collision) => collision,
                        Err(error) => {
                            return CommandOutcome::Error(format!("ConicByFivePoints: {error}"))
                        }
                    };
                    let mut staged = document.detached_clone_for_staging();
                    if let Some(collision_id) = collision {
                        let temporary_label = unique_object_label(&staged, "ConicInputC");
                        let Some(object) = staged.get_object_mut(collision_id) else {
                            return CommandOutcome::Error(
                                "ConicByFivePoints: objeto C inválido".into(),
                            );
                        };
                        object.set_label(temporary_label);
                    }
                    let constraint_id = match staged.try_add_conic_by_five_points_constraint(&ids) {
                        Ok(constraint_id) => constraint_id,
                        Err(error) => return CommandOutcome::Error(error),
                    };
                    if let Some(collision_id) = collision {
                        let output_id = staged
                            .constraints
                            .get_constraint(constraint_id)
                            .and_then(|constraint| constraint.outputs.first())
                            .copied();
                        let Some(output_id) = output_id else {
                            return CommandOutcome::Error(
                                "ConicByFivePoints: no se creó una salida".into(),
                            );
                        };
                        let output_label = unique_object_label(&staged, "Conic");
                        let Some(output) = staged.get_object_mut(output_id) else {
                            return CommandOutcome::Error(
                                "ConicByFivePoints: salida inválida".into(),
                            );
                        };
                        output.set_label(output_label);
                        let Some(original) = staged.get_object_mut(collision_id) else {
                            return CommandOutcome::Error(
                                "ConicByFivePoints: objeto C inválido".into(),
                            );
                        };
                        original.set_label("C".to_string());
                    }
                    let order = staged.propagation_order(&ids);
                    if let Err(error) = staged.try_re_evaluate_constraints(&order) {
                        return CommandOutcome::Error(error);
                    }
                    *document = staged;
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
                        if let Err(error) = add_boolean_result(document, &a.union(&b), "U") {
                            result = CommandOutcome::Error(error);
                        }
                    }
                    Err(msg) => result = CommandOutcome::Error(msg),
                }
                input_text.clear();
                return result;
            }
            "PolygonIntersection" if cmd.args.len() == 2 => {
                match resolve_two_polygons(document, &cmd.args[0], &cmd.args[1]) {
                    Ok((a, b)) => {
                        if let Err(error) = add_boolean_result(document, &a.intersection(&b), "I") {
                            result = CommandOutcome::Error(error);
                        }
                    }
                    Err(msg) => result = CommandOutcome::Error(msg),
                }
                input_text.clear();
                return result;
            }
            "PolygonDifference" if cmd.args.len() == 2 => {
                match resolve_two_polygons(document, &cmd.args[0], &cmd.args[1]) {
                    Ok((a, b)) => {
                        if let Err(error) = add_boolean_result(document, &a.difference(&b), "D") {
                            result = CommandOutcome::Error(error);
                        }
                    }
                    Err(msg) => result = CommandOutcome::Error(msg),
                }
                input_text.clear();
                return result;
            }
            "PolygonXor" if cmd.args.len() == 2 => {
                match resolve_two_polygons(document, &cmd.args[0], &cmd.args[1]) {
                    Ok((a, b)) => {
                        if let Err(error) = add_boolean_result(document, &a.xor(&b), "X") {
                            result = CommandOutcome::Error(error);
                        }
                    }
                    Err(msg) => result = CommandOutcome::Error(msg),
                }
                input_text.clear();
                return result;
            }
            "Translate" if cmd.args.len() == 2 => {
                let Some(id) = find_object_by_label(document, cmd.args[0].trim()) else {
                    return CommandOutcome::Error(format!(
                        "Translate: no se encontró el punto '{}'",
                        cmd.args[0]
                    ));
                };
                let Some(GeoObject::Point(point)) = document.get_object(id).cloned() else {
                    return CommandOutcome::Error("Translate solo admite puntos".into());
                };
                let displacement =
                    command_result!(parse_finite_point_arg(&cmd.args[1], &document.variables,)
                        .map_err(|error| CommandOutcome::Error(format!("Translate: {error}"))));
                let position = Point2::new(
                    point.position.x + displacement.x,
                    point.position.y + displacement.y,
                );
                if !position.x.is_finite() || !position.y.is_finite() {
                    return CommandOutcome::Error(
                        "Translate produjo coordenadas no finitas".into(),
                    );
                }
                let base_label = if point.label.is_empty() {
                    "T'".to_string()
                } else {
                    format!("{}'", point.label)
                };
                let label = unique_object_label(document, &base_label);
                let params = HashMap::from([
                    ("dx".to_string(), displacement.x),
                    ("dy".to_string(), displacement.y),
                ]);
                if let Err(error) = document.try_add_constructed_object_with_params(
                    GeoObject::Point(PointObj::new(position).with_label(label)),
                    "Translate",
                    &[id],
                    params,
                ) {
                    return CommandOutcome::Error(format!("Translate: {error}"));
                }
                input_text.clear();
                return CommandOutcome::Ok;
            }
            "Rotate" if cmd.args.len() == 2 || cmd.args.len() == 3 => {
                let Some(id) = find_object_by_label(document, cmd.args[0].trim()) else {
                    return CommandOutcome::Error(format!(
                        "Rotate: no se encontró el objeto '{}'",
                        cmd.args[0]
                    ));
                };
                let Some(GeoObject::Point(point)) = document.get_object(id).cloned() else {
                    return CommandOutcome::Error("Rotate solo admite puntos".into());
                };
                let (center, center_id, angle_arg) = if cmd.args.len() == 3 {
                    let (center, center_id) = match resolve_point_arg(document, &cmd.args[1]) {
                        Ok(center) => center,
                        Err(error) => {
                            return CommandOutcome::Error(format!("Rotate: {error}"));
                        }
                    };
                    (center, center_id, &cmd.args[2])
                } else {
                    (Point2::new(0.0, 0.0), None, &cmd.args[1])
                };
                let angle = match require_finite(parse_numeric_arg(angle_arg, &document.variables))
                {
                    Ok(angle) => angle,
                    Err(error) => return CommandOutcome::Error(format!("Rotate: {error}")),
                };
                let angle_rad = angle.to_radians();
                let dx = point.position.x - center.x;
                let dy = point.position.y - center.y;
                let position = Point2::new(
                    center.x + dx * angle_rad.cos() - dy * angle_rad.sin(),
                    center.y + dx * angle_rad.sin() + dy * angle_rad.cos(),
                );
                if !position.x.is_finite() || !position.y.is_finite() {
                    return CommandOutcome::Error("Rotate produjo coordenadas no finitas".into());
                }
                let mut params = HashMap::from([
                    ("angle".to_string(), angle),
                    ("center_x".to_string(), center.x),
                    ("center_y".to_string(), center.y),
                ]);
                let mut inputs = vec![id];
                if let Some(center_id) = center_id {
                    inputs.push(center_id);
                    params.remove("center_x");
                    params.remove("center_y");
                }
                let base_label = if point.label.is_empty() {
                    "R'".to_string()
                } else {
                    format!("{}'", point.label)
                };
                let label = unique_object_label(document, &base_label);
                if let Err(error) = document.try_add_constructed_object_with_params(
                    GeoObject::Point(PointObj::new(position).with_label(label)),
                    "Rotate",
                    &inputs,
                    params,
                ) {
                    return CommandOutcome::Error(format!("Rotate: {error}"));
                }
                input_text.clear();
                return CommandOutcome::Ok;
            }
            "Surface3D" if cmd.args.len() == 5 => {
                let Some(components) = parse_parametric_surface_components(&cmd.args[0]) else {
                    let expr = cmd.args[0].trim();
                    if expr.starts_with('(') {
                        return CommandOutcome::Error(
                            "Surface3D: la forma paramétrica requiere (x(u,v), y(u,v), z(u,v))"
                                .into(),
                        );
                    }
                    let (x_min, x_max, y_min, y_max) = command_result!(parse_rect_bounds(
                        "Surface3D",
                        &cmd.args,
                        &document.variables,
                        (0.0, 0.0, 0.0, 0.0),
                    ));
                    insert_command_object!(
                        document,
                        GeoObject::Surface3D(Surface3DObj::new(
                            expr,
                            (x_min, x_max),
                            (y_min, y_max),
                        ))
                    );
                    input_text.clear();
                    return CommandOutcome::Ok;
                };
                let [expr_x, expr_y, expr_z] =
                    command_result!(normalize_parametric_surface_components(components));
                let umin = command_result!(parse_finite_command_arg(
                    "Surface3D",
                    "u_min",
                    &cmd.args[1],
                    &document.variables,
                ));
                let umax = command_result!(parse_finite_command_arg(
                    "Surface3D",
                    "u_max",
                    &cmd.args[2],
                    &document.variables,
                ));
                let vmin = command_result!(parse_finite_command_arg(
                    "Surface3D",
                    "v_min",
                    &cmd.args[3],
                    &document.variables,
                ));
                let vmax = command_result!(parse_finite_command_arg(
                    "Surface3D",
                    "v_max",
                    &cmd.args[4],
                    &document.variables,
                ));
                if umin >= umax || vmin >= vmax {
                    return CommandOutcome::Error(
                        "Surface3D: se requiere u_min < u_max y v_min < v_max con límites finitos"
                            .into(),
                    );
                }
                command_result!(validate_parametric_surface_expression(
                    &expr_x,
                    "x",
                    &document.variables,
                ));
                command_result!(validate_parametric_surface_expression(
                    &expr_y,
                    "y",
                    &document.variables,
                ));
                command_result!(validate_parametric_surface_expression(
                    &expr_z,
                    "z",
                    &document.variables,
                ));
                insert_command_object!(
                    document,
                    GeoObject::Surface3D(Surface3DObj::new_parametric(
                        expr_x,
                        expr_y,
                        expr_z,
                        (umin, umax),
                        (vmin, vmax),
                    ))
                );
                input_text.clear();
                return CommandOutcome::Ok;
            }
            "Surface3D" if cmd.args.len() == 7 => {
                // Parametric form: Surface3D[x(u,v), y(u,v), z(u,v), umin, umax, vmin, vmax]
                let [expr_x, expr_y, expr_z] =
                    command_result!(normalize_parametric_surface_components([
                        cmd.args[0].trim().to_owned(),
                        cmd.args[1].trim().to_owned(),
                        cmd.args[2].trim().to_owned(),
                    ]));
                let umin = command_result!(parse_finite_command_arg(
                    "Surface3D",
                    "u_min",
                    &cmd.args[3],
                    &document.variables,
                ));
                let umax = command_result!(parse_finite_command_arg(
                    "Surface3D",
                    "u_max",
                    &cmd.args[4],
                    &document.variables,
                ));
                let vmin = command_result!(parse_finite_command_arg(
                    "Surface3D",
                    "v_min",
                    &cmd.args[5],
                    &document.variables,
                ));
                let vmax = command_result!(parse_finite_command_arg(
                    "Surface3D",
                    "v_max",
                    &cmd.args[6],
                    &document.variables,
                ));
                if umin >= umax || vmin >= vmax {
                    return CommandOutcome::Error(
                        "Surface3D: se requiere u_min < u_max y v_min < v_max con límites finitos"
                            .into(),
                    );
                }
                command_result!(validate_parametric_surface_expression(
                    &expr_x,
                    "x",
                    &document.variables,
                ));
                command_result!(validate_parametric_surface_expression(
                    &expr_y,
                    "y",
                    &document.variables,
                ));
                command_result!(validate_parametric_surface_expression(
                    &expr_z,
                    "z",
                    &document.variables,
                ));
                insert_command_object!(
                    document,
                    GeoObject::Surface3D(Surface3DObj::new_parametric(
                        expr_x,
                        expr_y,
                        expr_z,
                        (umin, umax),
                        (vmin, vmax),
                    ))
                );
                input_text.clear();
                return CommandOutcome::Ok;
            }
            "Point3D" if cmd.args.len() == 3 => {
                let x = command_result!(parse_finite_command_arg(
                    "Point3D",
                    "x",
                    &cmd.args[0],
                    &document.variables,
                ));
                let y = command_result!(parse_finite_command_arg(
                    "Point3D",
                    "y",
                    &cmd.args[1],
                    &document.variables,
                ));
                let z = command_result!(parse_finite_command_arg(
                    "Point3D",
                    "z",
                    &cmd.args[2],
                    &document.variables,
                ));
                let obj = GeoObject::Point3D(Point3DObj::new(Point3D::new(x, y, z)));
                insert_command_object!(document, obj);
                input_text.clear();
                return CommandOutcome::Ok;
            }
            "Segment3D" if cmd.args.len() == 6 => {
                let coordinates = cmd
                    .args
                    .iter()
                    .enumerate()
                    .map(|(index, value)| {
                        parse_finite_command_arg(
                            "Segment3D",
                            ["x1", "y1", "z1", "x2", "y2", "z2"][index],
                            value,
                            &document.variables,
                        )
                    })
                    .collect::<Result<Vec<_>, _>>();
                let coordinates = command_result!(coordinates);
                let obj = GeoObject::Segment3D(Segment3DObj::new(
                    Point3D::new(coordinates[0], coordinates[1], coordinates[2]),
                    Point3D::new(coordinates[3], coordinates[4], coordinates[5]),
                ));
                insert_command_object!(document, obj);
                input_text.clear();
                return CommandOutcome::Ok;
            }
            "Sphere" if cmd.args.len() == 4 => {
                let coordinates = cmd
                    .args
                    .iter()
                    .enumerate()
                    .map(|(index, value)| {
                        parse_finite_command_arg(
                            "Sphere",
                            ["x", "y", "z", "radius"][index],
                            value,
                            &document.variables,
                        )
                    })
                    .collect::<Result<Vec<_>, _>>();
                let coordinates = command_result!(coordinates);
                if coordinates[3] <= 0.0 {
                    return CommandOutcome::Error("Sphere: el radio debe ser positivo.".into());
                }
                let obj = GeoObject::Sphere3D(Sphere3DObj::new(
                    Point3D::new(coordinates[0], coordinates[1], coordinates[2]),
                    coordinates[3],
                ));
                insert_command_object!(document, obj);
                input_text.clear();
                return CommandOutcome::Ok;
            }
            "Cube" if cmd.args.len() == 4 => {
                let coordinates = cmd
                    .args
                    .iter()
                    .enumerate()
                    .map(|(index, value)| {
                        parse_finite_command_arg(
                            "Cube",
                            ["x", "y", "z", "size"][index],
                            value,
                            &document.variables,
                        )
                    })
                    .collect::<Result<Vec<_>, _>>();
                let coordinates = command_result!(coordinates);
                if coordinates[3] <= 0.0 {
                    return CommandOutcome::Error("Cube: el tamaño debe ser positivo.".into());
                }
                let obj = GeoObject::Cube3D(Cube3DObj::new(
                    Point3D::new(coordinates[0], coordinates[1], coordinates[2]),
                    coordinates[3],
                ));
                insert_command_object!(document, obj);
                input_text.clear();
                return CommandOutcome::Ok;
            }
            "Tetrahedron" if cmd.args.len() == 4 => {
                let coordinates = cmd
                    .args
                    .iter()
                    .enumerate()
                    .map(|(index, value)| {
                        parse_finite_command_arg(
                            "Tetrahedron",
                            ["x", "y", "z", "edge"][index],
                            value,
                            &document.variables,
                        )
                    })
                    .collect::<Result<Vec<_>, _>>();
                let coordinates = command_result!(coordinates);
                if coordinates[3] <= 0.0 {
                    return CommandOutcome::Error(
                        "Tetrahedron: la arista debe ser positiva.".into(),
                    );
                }
                let center = Point3D::new(coordinates[0], coordinates[1], coordinates[2]);
                if !grafito_geometry::Tetrahedron3D::new(center, coordinates[3]).is_renderable() {
                    return CommandOutcome::Error(
                        "Tetrahedron: los vértices exceden el límite de coordenadas renderizables"
                            .into(),
                    );
                }
                let obj = GeoObject::Tetrahedron3D(Tetrahedron3DObj::new(center, coordinates[3]));
                insert_command_object!(document, obj);
                input_text.clear();
                return CommandOutcome::Ok;
            }
            "Cylinder" if cmd.args.len() == 5 => {
                let values = cmd
                    .args
                    .iter()
                    .enumerate()
                    .map(|(index, value)| {
                        parse_finite_command_arg(
                            "Cylinder",
                            ["x", "y", "z", "radius", "height"][index],
                            value,
                            &document.variables,
                        )
                    })
                    .collect::<Result<Vec<_>, _>>();
                let values = command_result!(values);
                let (x, y, z, r, h) = (values[0], values[1], values[2], values[3], values[4]);
                if r <= 0.0 || h <= 0.0 {
                    return CommandOutcome::Error(
                        "Cylinder: el radio y la altura deben ser positivos.".into(),
                    );
                }
                let obj = GeoObject::Cylinder3D(Cylinder3DObj::new(
                    Point3D::new(x, y, z),
                    Point3D::new(x, y + h, z),
                    r,
                ));
                insert_command_object!(document, obj);
                input_text.clear();
                return CommandOutcome::Ok;
            }
            "Cone" if cmd.args.len() == 5 => {
                let values = cmd
                    .args
                    .iter()
                    .enumerate()
                    .map(|(index, value)| {
                        parse_finite_command_arg(
                            "Cone",
                            ["x", "y", "z", "radius", "height"][index],
                            value,
                            &document.variables,
                        )
                    })
                    .collect::<Result<Vec<_>, _>>();
                let values = command_result!(values);
                let (x, y, z, r, h) = (values[0], values[1], values[2], values[3], values[4]);
                if r <= 0.0 || h <= 0.0 {
                    return CommandOutcome::Error(
                        "Cone: el radio y la altura deben ser positivos.".into(),
                    );
                }
                let obj = GeoObject::Cone3D(Cone3DObj::new(
                    Point3D::new(x, y, z),
                    Point3D::new(x, y + h, z),
                    r,
                ));
                insert_command_object!(document, obj);
                input_text.clear();
                return CommandOutcome::Ok;
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
                    insert_command_object!(document, obj);
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
                    insert_command_object!(document, obj);
                    input_text.clear();
                    return CommandOutcome::Ok;
                }
            }
            "Plane3D" if cmd.args.len() == 4 => {
                // Plane3D[a, b, c, d]  →  ax + by + cz + d = 0
                if let (Ok(a), Ok(b), Ok(c), Ok(d)) = (
                    parse_numeric_arg(&cmd.args[0], &document.variables),
                    parse_numeric_arg(&cmd.args[1], &document.variables),
                    parse_numeric_arg(&cmd.args[2], &document.variables),
                    parse_numeric_arg(&cmd.args[3], &document.variables),
                ) {
                    if a * a + b * b + c * c < 1e-18 {
                        return CommandOutcome::Error(
                            "Plane3D: el vector normal no puede ser cero".into(),
                        );
                    }
                    let obj = GeoObject::Plane3D(Plane3DObj::from_equation(a, b, c, d));
                    insert_command_object!(document, obj);
                    input_text.clear();
                    return CommandOutcome::Ok;
                }
            }
            "Plane3D" if cmd.args.len() == 3 => {
                // Plane3D[label1, label2, label3]  →  plano por 3 puntos
                let pts = parse_three_point_labels(document, &cmd.args);
                if let Some((p1, p2, p3)) = pts {
                    let Some(plane) = Plane3DObj::from_three_points(p1, p2, p3) else {
                        return CommandOutcome::Error(
                            "Plane3D: los tres puntos son colineales o repetidos".into(),
                        );
                    };
                    let obj = GeoObject::Plane3D(plane);
                    insert_command_object!(document, obj);
                    input_text.clear();
                    return CommandOutcome::Ok;
                }
            }
            "Line3D" if cmd.args.len() == 6 => {
                // Line3D[x0, y0, z0, dx, dy, dz]  →  punto + dirección
                if let (Ok(x0), Ok(y0), Ok(z0), Ok(dx), Ok(dy), Ok(dz)) = (
                    parse_numeric_arg(&cmd.args[0], &document.variables),
                    parse_numeric_arg(&cmd.args[1], &document.variables),
                    parse_numeric_arg(&cmd.args[2], &document.variables),
                    parse_numeric_arg(&cmd.args[3], &document.variables),
                    parse_numeric_arg(&cmd.args[4], &document.variables),
                    parse_numeric_arg(&cmd.args[5], &document.variables),
                ) {
                    if dx * dx + dy * dy + dz * dz < 1e-18 {
                        return CommandOutcome::Error(
                            "Line3D: el vector dirección no puede ser cero".into(),
                        );
                    }
                    let obj = GeoObject::Line3D(Line3DObj::from_point_and_direction(
                        Point3D::new(x0, y0, z0),
                        Point3D::new(dx, dy, dz),
                    ));
                    insert_command_object!(document, obj);
                    input_text.clear();
                    return CommandOutcome::Ok;
                }
            }
            "Line3D" if cmd.args.len() == 2 => {
                // Line3D[label1, label2]  →  recta por 2 puntos
                if let (Some(id1), Some(id2)) = (
                    find_object_by_label(document, &cmd.args[0]),
                    find_object_by_label(document, &cmd.args[1]),
                ) {
                    if let (Some(GeoObject::Point3D(p1)), Some(GeoObject::Point3D(p2))) =
                        (document.get_object(id1), document.get_object(id2))
                    {
                        if p1.position.distance(&p2.position) < 1e-9 {
                            return CommandOutcome::Error(
                                "Line3D: los dos puntos deben ser distintos".into(),
                            );
                        }
                        let obj =
                            GeoObject::Line3D(Line3DObj::from_two_points(p1.position, p2.position));
                        insert_command_object!(document, obj);
                        input_text.clear();
                        return CommandOutcome::Ok;
                    }
                }
                return CommandOutcome::Error(
                    "Line3D: se requieren dos puntos 3D con etiquetas válidas".into(),
                );
            }
            "EquidistantFrom" if cmd.args.len() >= 3 => {
                let label_a = cmd.args[0].trim();
                let label_b = cmd.args[1].trim();
                let axis = match Axis3D::parse(&cmd.args[2]) {
                    Some(axis) => axis,
                    None => {
                        return CommandOutcome::Error(
                            "EquidistantFrom: usa \"x-axis\", \"y-axis\" o \"z-axis\"".into(),
                        );
                    }
                };
                let Some(id_a) = find_object_by_label(document, label_a) else {
                    return CommandOutcome::Error(format!(
                        "EquidistantFrom: no existe el objeto '{}'",
                        label_a
                    ));
                };
                let Some(id_b) = find_object_by_label(document, label_b) else {
                    return CommandOutcome::Error(format!(
                        "EquidistantFrom: no existe el objeto '{}'",
                        label_b
                    ));
                };
                let Some(obj_a) = document.get_object(id_a).cloned() else {
                    return CommandOutcome::Error("EquidistantFrom: objeto inválido".into());
                };
                let Some(obj_b) = document.get_object(id_b).cloned() else {
                    return CommandOutcome::Error("EquidistantFrom: objeto inválido".into());
                };
                return add_equidistant_solutions(document, &obj_a, &obj_b, axis);
            }
            "Solve3DGeometry" if cmd.args.len() >= 3 => {
                let equation = cmd.args[0].trim().trim_matches('"');
                let var = cmd.args[1].trim();
                let constraint = cmd.args[2].trim().trim_matches('"');
                let Some((label_a, label_b)) = parse_distance_equality_labels(equation) else {
                    return CommandOutcome::Error(
                        "Solve3DGeometry: usa una ecuación tipo dist(P,A)=dist(P,B)".into(),
                    );
                };
                let Some(axis) = Axis3D::parse_point_constraint(constraint, var) else {
                    return CommandOutcome::Error(
                        "Solve3DGeometry: usa una restricción tipo P=(0,y,0)".into(),
                    );
                };
                let Some(id_a) = find_object_by_label(document, label_a) else {
                    return CommandOutcome::Error(format!(
                        "Solve3DGeometry: no existe el objeto '{}'",
                        label_a
                    ));
                };
                let Some(id_b) = find_object_by_label(document, label_b) else {
                    return CommandOutcome::Error(format!(
                        "Solve3DGeometry: no existe el objeto '{}'",
                        label_b
                    ));
                };
                let Some(obj_a) = document.get_object(id_a).cloned() else {
                    return CommandOutcome::Error("Solve3DGeometry: objeto inválido".into());
                };
                let Some(obj_b) = document.get_object(id_b).cloned() else {
                    return CommandOutcome::Error("Solve3DGeometry: objeto inválido".into());
                };
                return add_equidistant_solutions(document, &obj_a, &obj_b, axis);
            }
            "Intersection3D" if cmd.args.len() == 2 => {
                return run_intersection_3d(document, &cmd.args[0], &cmd.args[1]);
            }
            "Intersection3D" if cmd.args.len() == 3 => {
                return run_three_plane_intersection(
                    document,
                    &cmd.args[0],
                    &cmd.args[1],
                    &cmd.args[2],
                );
            }
            "Projection3D" if cmd.args.len() == 2 => {
                return run_projection_3d(document, &cmd.args[0], &cmd.args[1]);
            }
            "PlaneThroughLines" if cmd.args.len() == 2 => {
                return run_plane_through_lines(document, &cmd.args[0], &cmd.args[1]);
            }
            "PlaneThroughLinePoint" if cmd.args.len() == 2 => {
                return run_plane_through_line_point(document, &cmd.args[0], &cmd.args[1]);
            }
            "LineRelation3D" if cmd.args.len() == 2 => {
                return run_line_relation_3d(document, &cmd.args[0], &cmd.args[1]);
            }
            "SolveLine3DParameters" if cmd.args.len() >= 4 => {
                return run_solve_line_3d_parameters(&cmd.args, document);
            }
            "Point3D" | "Segment3D" | "Plane3D" | "Line3D" | "Sphere" | "Cube" | "Tetrahedron"
            | "Cylinder" | "Cone" | "Torus" | "Moebius" | "Surface3D" => {
                return CommandOutcome::Error("Argumentos inválidos para comando 3D".into());
            }
            "Tangent" => {
                let center =
                    command_result!(parse_finite_point_arg(&cmd.args[0], &document.variables,)
                        .map_err(|error| CommandOutcome::Error(format!("Tangent: {error}"))));
                let radius = command_result!(parse_finite_command_arg(
                    "Tangent",
                    "radio",
                    &cmd.args[1],
                    &document.variables,
                ));
                if radius <= 0.0 {
                    return CommandOutcome::Error(
                        "Tangent: el radio debe ser finito y positivo".into(),
                    );
                }
                let external =
                    command_result!(parse_finite_point_arg(&cmd.args[2], &document.variables,)
                        .map_err(|error| CommandOutcome::Error(format!("Tangent: {error}"))));
                let dx = external.x - center.x;
                let dy = external.y - center.y;
                let distance = dx.hypot(dy);
                if distance <= radius {
                    input_text.clear();
                    return CommandOutcome::Message(
                        "Tangent: el punto está dentro del círculo, no hay tangentes".into(),
                    );
                }
                let along = radius * radius / distance;
                let offset = (radius * radius - along * along).sqrt();
                let midpoint = Point2::new(
                    center.x + along * dx / distance,
                    center.y + along * dy / distance,
                );
                let tangent_a = Point2::new(
                    midpoint.x - offset * dy / distance,
                    midpoint.y + offset * dx / distance,
                );
                let tangent_b = Point2::new(
                    midpoint.x + offset * dy / distance,
                    midpoint.y - offset * dx / distance,
                );
                insert_command_object!(
                    document,
                    GeoObject::Line(
                        LineObj::new_with_kind(external, tangent_a, LineKind::Line)
                            .with_label("T1")
                    )
                );
                insert_command_object!(
                    document,
                    GeoObject::Line(
                        LineObj::new_with_kind(external, tangent_b, LineKind::Line)
                            .with_label("T2")
                    )
                );
                input_text.clear();
                return CommandOutcome::Ok;
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
                    insert_command_object!(
                        document,
                        GeoObject::Line(
                            LineObj::new_with_kind(p1, p2, LineKind::Line).with_label("B"),
                        )
                    );
                } else {
                    return CommandOutcome::Error(
                        "PerpendicularBisector: se requieren dos puntos finitos".into(),
                    );
                }
                input_text.clear();
                return CommandOutcome::Ok;
            }
            "AngleBisector" if cmd.args.len() == 3 => {
                let ((x1, y1), (xv, yv), (x2, y2)) = match (
                    parse_point_str(&cmd.args[0]),
                    parse_point_str(&cmd.args[1]),
                    parse_point_str(&cmd.args[2]),
                ) {
                    (Ok(first), Ok(vertex), Ok(second))
                        if first.0.is_finite()
                            && first.1.is_finite()
                            && vertex.0.is_finite()
                            && vertex.1.is_finite()
                            && second.0.is_finite()
                            && second.1.is_finite() =>
                    {
                        (first, vertex, second)
                    }
                    _ => {
                        return CommandOutcome::Error(
                            "AngleBisector: se requieren tres puntos finitos".into(),
                        )
                    }
                };
                let d1 = ((xv - x1).powi(2) + (yv - y1).powi(2)).sqrt();
                let d2 = ((xv - x2).powi(2) + (yv - y2).powi(2)).sqrt();
                if d1 <= 1e-12 || d2 <= 1e-12 {
                    return CommandOutcome::Error(
                        "AngleBisector: el vértice debe ser distinto de los dos puntos que definen el ángulo."
                            .into(),
                    );
                }
                let b_len = (((x1 - xv) / d1 + (x2 - xv) / d2).powi(2)
                    + ((y1 - yv) / d1 + (y2 - yv) / d2).powi(2))
                .sqrt();
                if b_len <= 1e-12 {
                    return CommandOutcome::Error(
                        "AngleBisector: los rayos del ángulo no pueden ser opuestos.".into(),
                    );
                }
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
                            insert_command_object!(
                                document,
                                GeoObject::Line(
                                    LineObj::new_with_kind(Point2::new(xv, yv), p, LineKind::Ray)
                                        .with_label("Ab"),
                                )
                            );
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
                        insert_command_construction!(
                            document,
                            GeoObject::Point(PointObj::new(Point2::new(mx, my)).with_label("M")),
                            "Midpoint",
                            &[id_a, id_b]
                        );
                    } else {
                        return CommandOutcome::Error(
                            "Midpoint: ambos objetos deben ser puntos".into(),
                        );
                    }
                } else if let (Ok(first), Ok(second)) = (
                    parse_finite_point_arg(&cmd.args[0], &document.variables),
                    parse_finite_point_arg(&cmd.args[1], &document.variables),
                ) {
                    let obj = GeoObject::Point(
                        PointObj::new(Point2::new(
                            (first.x + second.x) * 0.5,
                            (first.y + second.y) * 0.5,
                        ))
                        .with_label("M"),
                    );
                    insert_command_object!(document, obj);
                } else {
                    return CommandOutcome::Error(
                        "Midpoint: se requieren dos puntos o dos etiquetas de puntos válidas"
                            .into(),
                    );
                }
                input_text.clear();
                return CommandOutcome::Ok;
            }
            "Perpendicular" if cmd.args.len() == 2 => {
                let Some(first_id) = find_object_by_label(document, cmd.args[0].trim()) else {
                    return CommandOutcome::Error(format!(
                        "Perpendicular: no se encontró '{}'",
                        cmd.args[0]
                    ));
                };
                let Some(second_id) = find_object_by_label(document, cmd.args[1].trim()) else {
                    return CommandOutcome::Error(format!(
                        "Perpendicular: no se encontró '{}'",
                        cmd.args[1]
                    ));
                };
                let (line_id, point_id, line, point) = match (
                    document.get_object(first_id).cloned(),
                    document.get_object(second_id).cloned(),
                ) {
                    (Some(GeoObject::Point(point)), Some(GeoObject::Line(line))) => {
                        (second_id, first_id, line, point)
                    }
                    (Some(GeoObject::Line(line)), Some(GeoObject::Point(point))) => {
                        (first_id, second_id, line, point)
                    }
                    _ => {
                        return CommandOutcome::Error(
                            "Perpendicular requiere un punto y una recta".into(),
                        )
                    }
                };
                let dx = line.end.x - line.start.x;
                let dy = line.end.y - line.start.y;
                let direction_length = dx.hypot(dy);
                if !dx.is_finite()
                    || !dy.is_finite()
                    || !direction_length.is_finite()
                    || direction_length <= 1e-12
                {
                    return CommandOutcome::Error(
                        "Perpendicular requiere una recta no degenerada".into(),
                    );
                }
                let output = GeoObject::Line(
                    LineObj::new_with_kind(
                        Point2::new(point.position.x - dy, point.position.y + dx),
                        Point2::new(point.position.x + dy, point.position.y - dx),
                        LineKind::Line,
                    )
                    .with_label(unique_object_label(document, "perpendicular")),
                );
                if let Err(error) = document.try_add_constructed_object(
                    output,
                    "Perpendicular",
                    &[line_id, point_id],
                ) {
                    return CommandOutcome::Error(format!("Perpendicular: {error}"));
                }
                input_text.clear();
                return CommandOutcome::Ok;
            }
            "Parallel" if cmd.args.len() == 2 => {
                let Some(point_id) = find_object_by_label(document, cmd.args[0].trim()) else {
                    return CommandOutcome::Error(format!(
                        "Parallel: no se encontró el punto '{}'",
                        cmd.args[0]
                    ));
                };
                let Some(line_id) = find_object_by_label(document, cmd.args[1].trim()) else {
                    return CommandOutcome::Error(format!(
                        "Parallel: no se encontró la recta '{}'",
                        cmd.args[1]
                    ));
                };
                let (point, line) = match (
                    document.get_object(point_id).cloned(),
                    document.get_object(line_id).cloned(),
                ) {
                    (Some(GeoObject::Point(point)), Some(GeoObject::Line(line))) => (point, line),
                    _ => {
                        return CommandOutcome::Error(
                            "Parallel requiere un punto seguido de una recta".into(),
                        )
                    }
                };
                let dx = line.end.x - line.start.x;
                let dy = line.end.y - line.start.y;
                let length = dx.hypot(dy);
                if !length.is_finite() || length <= 1e-12 {
                    return CommandOutcome::Error(
                        "Parallel requiere una recta finita no degenerada".into(),
                    );
                }
                let start = Point2::new(point.position.x - dx, point.position.y - dy);
                let end = Point2::new(point.position.x + dx, point.position.y + dy);
                if !start.x.is_finite()
                    || !start.y.is_finite()
                    || !end.x.is_finite()
                    || !end.y.is_finite()
                {
                    return CommandOutcome::Error("Parallel produjo coordenadas no finitas".into());
                }
                let output = GeoObject::Line(
                    LineObj::new_with_kind(start, end, LineKind::Line)
                        .with_label(unique_object_label(document, "parallel")),
                );
                if let Err(error) =
                    document.try_add_constructed_object(output, "Parallel", &[line_id, point_id])
                {
                    return CommandOutcome::Error(format!("Parallel: {error}"));
                }
                input_text.clear();
                return CommandOutcome::Ok;
            }
            "PointOnObject" if cmd.args.len() == 2 => {
                let Some(object_id) = find_object_by_label(document, cmd.args[0].trim()) else {
                    return CommandOutcome::Error(format!(
                        "PointOnObject: no se encontró el objeto '{}'",
                        cmd.args[0]
                    ));
                };
                let Some(point_id) = find_object_by_label(document, cmd.args[1].trim()) else {
                    return CommandOutcome::Error(format!(
                        "PointOnObject: no se encontró el punto '{}'",
                        cmd.args[1]
                    ));
                };
                let Some(GeoObject::Point(point)) = document.get_object(point_id) else {
                    return CommandOutcome::Error(
                        "PointOnObject: el segundo argumento debe ser un punto".into(),
                    );
                };
                insert_command_construction!(
                    document,
                    GeoObject::Point(PointObj::new(point.position).with_label("P_on")),
                    "PointOnObject",
                    &[object_id, point_id]
                );
                input_text.clear();
                return CommandOutcome::Ok;
            }
            "CircleByCenterRadius" if cmd.args.len() == 2 => {
                let Some(center_id) = find_object_by_label(document, cmd.args[0].trim()) else {
                    return CommandOutcome::Error(format!(
                        "CircleByCenterRadius: no se encontró el punto '{}'",
                        cmd.args[0]
                    ));
                };
                if !matches!(document.get_object(center_id), Some(GeoObject::Point(_))) {
                    return CommandOutcome::Error(
                        "CircleByCenterRadius: el centro debe ser un punto".into(),
                    );
                }
                let radius = command_result!(parse_finite_command_arg(
                    "CircleByCenterRadius",
                    "radio",
                    &cmd.args[1],
                    &document.variables,
                ));
                if radius <= 0.0 {
                    return CommandOutcome::Error(
                        "CircleByCenterRadius: el radio debe ser positivo".into(),
                    );
                }
                let params = HashMap::from([("radius".to_string(), radius)]);
                if let Err(error) = document.try_add_constructed_object_with_params(
                    GeoObject::Circle(
                        CircleObj::new(Point2::new(0.0, 0.0), radius)
                            .with_label(unique_object_label(document, "C")),
                    ),
                    "CircleByCenterRadius",
                    &[center_id],
                    params,
                ) {
                    return CommandOutcome::Error(format!("CircleByCenterRadius: {error}"));
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
                let [Some(id1), Some(id2), Some(id3)] = ids.as_slice() else {
                    return CommandOutcome::Error(
                        "CircleByThreePoints: no se encontraron los tres puntos".into(),
                    );
                };
                if !matches!(
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
                    return CommandOutcome::Error(
                        "CircleByThreePoints: los tres objetos deben ser puntos".into(),
                    );
                }
                insert_command_construction!(
                    document,
                    GeoObject::Circle(CircleObj::new(Point2::new(0.0, 0.0), 1.0).with_label("C")),
                    "CircleByThreePoints",
                    &[*id1, *id2, *id3]
                );
                input_text.clear();
                return CommandOutcome::Ok;
            }
            "PointExpr" if cmd.args.len() == 2 => {
                let x_expr = cmd.args[0].trim().to_string();
                let y_expr = cmd.args[1].trim().to_string();
                let mut point = PointObj::new(Point2::new(0.0, 0.0));
                point.x_expr = Some(x_expr);
                point.y_expr = Some(y_expr);
                insert_command_object!(document, GeoObject::Point(point));
                document.recompute_bound_parameters();
                input_text.clear();
                return CommandOutcome::Ok;
            }
            "CircleExpr" if cmd.args.len() == 2 => {
                let center_arg = cmd.args[0].trim();
                let radius_expr = cmd.args[1].trim().to_string();
                let radius = match parse_numeric_arg(&radius_expr, &document.variables) {
                    Ok(radius) if radius.is_finite() && radius > 0.0 => radius,
                    Ok(_) => {
                        return CommandOutcome::Error(
                            "CircleExpr: el radio debe ser finito y mayor que cero".into(),
                        )
                    }
                    Err(error) => {
                        return CommandOutcome::Error(format!(
                            "CircleExpr: radio inválido: {error}"
                        ))
                    }
                };
                let center = match resolve_point_arg(document, center_arg) {
                    Ok((point, _)) => point,
                    Err(error) => {
                        return CommandOutcome::Error(format!("CircleExpr: {error}"));
                    }
                };
                let mut circle = CircleObj::new(center, radius);
                circle.radius_expr = Some(radius_expr);
                insert_command_object!(document, GeoObject::Circle(circle));
                document.recompute_bound_parameters();
                input_text.clear();
                return CommandOutcome::Ok;
            }
            "Vector" if cmd.args.len() == 2 => {
                let start =
                    command_result!(parse_finite_point_arg(&cmd.args[0], &document.variables,)
                        .map_err(|error| CommandOutcome::Error(format!("Vector: {error}"))));
                let end =
                    command_result!(parse_finite_point_arg(&cmd.args[1], &document.variables,)
                        .map_err(|error| CommandOutcome::Error(format!("Vector: {error}"))));
                insert_command_object!(
                    document,
                    GeoObject::Line(
                        LineObj::new_with_kind(start, end, LineKind::Segment).with_label("v")
                    )
                );
                input_text.clear();
                return CommandOutcome::Ok;
            }
            "Ray" if cmd.args.len() == 2 => {
                let start =
                    command_result!(parse_finite_point_arg(&cmd.args[0], &document.variables,)
                        .map_err(|error| CommandOutcome::Error(format!("Ray: {error}"))));
                let end =
                    command_result!(parse_finite_point_arg(&cmd.args[1], &document.variables,)
                        .map_err(|error| CommandOutcome::Error(format!("Ray: {error}"))));
                insert_command_object!(
                    document,
                    GeoObject::Line(
                        LineObj::new_with_kind(start, end, LineKind::Ray).with_label("r")
                    )
                );
                input_text.clear();
                return CommandOutcome::Ok;
            }
            "Line" if cmd.args.len() == 2 => {
                let start =
                    command_result!(parse_finite_point_arg(&cmd.args[0], &document.variables,)
                        .map_err(|error| CommandOutcome::Error(format!("Line: {error}"))));
                let end =
                    command_result!(parse_finite_point_arg(&cmd.args[1], &document.variables,)
                        .map_err(|error| CommandOutcome::Error(format!("Line: {error}"))));
                insert_command_object!(
                    document,
                    GeoObject::Line(
                        LineObj::new_with_kind(start, end, LineKind::Line).with_label("l")
                    )
                );
                input_text.clear();
                return CommandOutcome::Ok;
            }
            "Segment" if cmd.args.len() == 2 => {
                let start =
                    command_result!(parse_finite_point_arg(&cmd.args[0], &document.variables,)
                        .map_err(|error| CommandOutcome::Error(format!("Segment: {error}"))));
                let end =
                    command_result!(parse_finite_point_arg(&cmd.args[1], &document.variables,)
                        .map_err(|error| CommandOutcome::Error(format!("Segment: {error}"))));
                insert_command_object!(
                    document,
                    GeoObject::Line(
                        LineObj::new_with_kind(start, end, LineKind::Segment).with_label("s")
                    )
                );
                input_text.clear();
                return CommandOutcome::Ok;
            }
            "Parabola" if cmd.args.len() >= 2 => {
                let (vx, vy) = match parse_point_str(&cmd.args[0]) {
                    Ok(point) if point.0.is_finite() && point.1.is_finite() => point,
                    _ => {
                        return CommandOutcome::Error(
                            "Parabola: el vértice debe ser un punto finito.".into(),
                        )
                    }
                };
                let p =
                    match require_finite(parse_numeric_arg(&cmd.args[1], &document.variables)) {
                        Ok(value) if value.abs() > 1e-12 => value,
                        _ => return CommandOutcome::Error(
                            "Parabola: el parámetro p debe ser un número finito distinto de cero"
                                .into(),
                        ),
                    };
                insert_command_object!(
                    document,
                    GeoObject::Parabola(ParabolaObj::new(Point2::new(vx, vy), p,))
                );
                input_text.clear();
                return CommandOutcome::Ok;
            }
            "Hyperbola" if cmd.args.len() >= 3 => {
                let center =
                    command_result!(parse_finite_point_arg(&cmd.args[0], &document.variables,)
                        .map_err(|error| CommandOutcome::Error(format!("Hyperbola: {error}"))));
                let a = command_result!(parse_finite_command_arg(
                    "Hyperbola",
                    "a",
                    &cmd.args[1],
                    &document.variables,
                ));
                let b = command_result!(parse_finite_command_arg(
                    "Hyperbola",
                    "b",
                    &cmd.args[2],
                    &document.variables,
                ));
                if a <= 0.0 || b <= 0.0 {
                    return CommandOutcome::Error(
                        "Hyperbola: los semiejes deben ser finitos y positivos".into(),
                    );
                }
                insert_command_object!(
                    document,
                    GeoObject::Hyperbola(HyperbolaObj::new(center, a, b))
                );
                input_text.clear();
                return CommandOutcome::Ok;
            }
            "Dilate" if cmd.args.len() == 3 => {
                let (point, point_id, label) = if let Some(id) =
                    find_object_by_label(document, cmd.args[0].trim())
                {
                    let Some(GeoObject::Point(point)) = document.get_object(id).cloned() else {
                        return CommandOutcome::Error("Dilate solo admite puntos".into());
                    };
                    let base_label = if point.label.is_empty() {
                        "D'".to_string()
                    } else {
                        format!("{}'", point.label)
                    };
                    let label = unique_object_label(document, &base_label);
                    (point.position, Some(id), label)
                } else {
                    let point = match parse_finite_point_arg(&cmd.args[0], &document.variables) {
                        Ok(point) => point,
                        Err(error) => {
                            return CommandOutcome::Error(format!("Dilate: {error}"));
                        }
                    };
                    (point, None, unique_object_label(document, "D'"))
                };
                let factor =
                    match require_finite(parse_numeric_arg(&cmd.args[1], &document.variables)) {
                        Ok(factor) => factor,
                        Err(error) => return CommandOutcome::Error(format!("Dilate: {error}")),
                    };
                let (center, center_id) = match resolve_point_arg(document, &cmd.args[2]) {
                    Ok(center) => center,
                    Err(error) => return CommandOutcome::Error(format!("Dilate: {error}")),
                };
                let position = Point2::new(
                    center.x + (point.x - center.x) * factor,
                    center.y + (point.y - center.y) * factor,
                );
                if !position.x.is_finite() || !position.y.is_finite() {
                    return CommandOutcome::Error("Dilate produjo coordenadas no finitas".into());
                }
                let output = GeoObject::Point(PointObj::new(position).with_label(label));
                if let Some(point_id) = point_id {
                    let mut params = HashMap::from([
                        ("factor".to_string(), factor),
                        ("center_x".to_string(), center.x),
                        ("center_y".to_string(), center.y),
                    ]);
                    let mut inputs = vec![point_id];
                    if let Some(center_id) = center_id {
                        inputs.push(center_id);
                        params.remove("center_x");
                        params.remove("center_y");
                    }
                    if let Err(error) = document
                        .try_add_constructed_object_with_params(output, "Dilate", &inputs, params)
                    {
                        return CommandOutcome::Error(format!("Dilate: {error}"));
                    }
                } else if let Some(center_id) = center_id {
                    let params = HashMap::from([
                        ("factor".to_string(), factor),
                        ("source_x".to_string(), point.x),
                        ("source_y".to_string(), point.y),
                    ]);
                    if let Err(error) = document.try_add_constructed_object_with_params(
                        output,
                        "Dilate",
                        &[center_id],
                        params,
                    ) {
                        return CommandOutcome::Error(format!("Dilate: {error}"));
                    }
                } else {
                    insert_command_object!(document, output);
                }
                input_text.clear();
                return CommandOutcome::Ok;
            }
            "Reflect" if cmd.args.len() == 3 => {
                let (axis_start, _) = match resolve_point_arg(document, &cmd.args[1]) {
                    Ok(point) => point,
                    Err(error) => return CommandOutcome::Error(format!("Reflect: {error}")),
                };
                let (axis_end, _) = match resolve_point_arg(document, &cmd.args[2]) {
                    Ok(point) => point,
                    Err(error) => return CommandOutcome::Error(format!("Reflect: {error}")),
                };
                let dx = axis_end.x - axis_start.x;
                let dy = axis_end.y - axis_start.y;
                let length = dx.hypot(dy);
                if !length.is_finite() || length <= 1e-12 {
                    return CommandOutcome::Error(
                        "Reflect: el eje requiere dos puntos distintos.".into(),
                    );
                }
                let ux = dx / length;
                let uy = dy / length;
                let mirror_point = |point: Point2| -> Result<Point2, String> {
                    let projection = (point.x - axis_start.x) * ux + (point.y - axis_start.y) * uy;
                    let closest = Point2::new(
                        axis_start.x + projection * ux,
                        axis_start.y + projection * uy,
                    );
                    let reflected =
                        Point2::new(2.0 * closest.x - point.x, 2.0 * closest.y - point.y);
                    if reflected.x.is_finite() && reflected.y.is_finite() {
                        Ok(reflected)
                    } else {
                        Err("la reflexión produjo coordenadas no finitas".into())
                    }
                };

                let reflected = if let Some(id) = find_object_by_label(document, &cmd.args[0]) {
                    let Some(object) = document.get_object(id).cloned() else {
                        return CommandOutcome::Error(format!(
                            "Reflect: no se encontró el objeto '{}'",
                            cmd.args[0]
                        ));
                    };
                    let base_label = if object.label().is_empty() {
                        "R'".to_string()
                    } else {
                        format!("{}'", object.label())
                    };
                    let label = unique_object_label(document, &base_label);
                    match object {
                        GeoObject::Point(point) => GeoObject::Point(
                            PointObj::new(command_result!(mirror_point(point.position).map_err(
                                |error| CommandOutcome::Error(format!("Reflect: {error}")),
                            )))
                            .with_label(label),
                        ),
                        GeoObject::Line(line) => GeoObject::Line(
                            LineObj::new(
                                command_result!(mirror_point(line.start).map_err(|error| {
                                    CommandOutcome::Error(format!("Reflect: {error}"))
                                })),
                                command_result!(mirror_point(line.end).map_err(|error| {
                                    CommandOutcome::Error(format!("Reflect: {error}"))
                                })),
                            )
                            .with_label(label),
                        ),
                        GeoObject::Circle(circle) => GeoObject::Circle(
                            CircleObj::new(
                                command_result!(mirror_point(circle.center).map_err(|error| {
                                    CommandOutcome::Error(format!("Reflect: {error}"))
                                })),
                                circle.radius,
                            )
                            .with_label(label),
                        ),
                        GeoObject::Polygon(polygon) => {
                            let vertices = command_result!(polygon
                                .vertices
                                .iter()
                                .copied()
                                .map(mirror_point)
                                .collect::<Result<Vec<_>, _>>()
                                .map_err(|error| {
                                    CommandOutcome::Error(format!("Reflect: {error}"))
                                }));
                            let mut reflected = PolygonObj::new(vertices);
                            reflected.label = label;
                            GeoObject::Polygon(reflected)
                        }
                        _ => {
                            return CommandOutcome::Error(
                                "Reflect: el tipo de objeto no está soportado".into(),
                            )
                        }
                    }
                } else {
                    let point = match parse_finite_point_arg(&cmd.args[0], &document.variables) {
                        Ok(point) => point,
                        Err(error) => {
                            return CommandOutcome::Error(format!("Reflect: {error}"));
                        }
                    };
                    GeoObject::Point(
                        PointObj::new(command_result!(mirror_point(point).map_err(|error| {
                            CommandOutcome::Error(format!("Reflect: {error}"))
                        })))
                        .with_label(unique_object_label(document, "R'")),
                    )
                };
                insert_command_object!(document, reflected);
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
            "Locus" => {
                let driver_label = clean_label(&cmd.args[0]);
                let target_label = clean_label(&cmd.args[1]);
                let driver = match document.try_find_object_by_label(driver_label) {
                    Ok(Some(id)) => id,
                    Ok(None) => {
                        return CommandOutcome::Error(
                            "Locus: objeto driver no encontrado".to_string(),
                        )
                    }
                    Err(error) => return CommandOutcome::Error(format!("Locus: {error}")),
                };
                let target = match document.try_find_object_by_label(target_label) {
                    Ok(Some(id)) => id,
                    Ok(None) => {
                        return CommandOutcome::Error(
                            "Locus: objeto target no encontrado".to_string(),
                        )
                    }
                    Err(error) => return CommandOutcome::Error(format!("Locus: {error}")),
                };
                let (locus, _) = match document.try_add_locus(driver, target) {
                    Ok(result) => result,
                    Err(error) => return CommandOutcome::Error(error),
                };
                let label = document
                    .get_object(locus)
                    .map(|object| object.label().to_string())
                    .unwrap_or_else(|| "Locus".to_string());
                input_text.clear();
                return CommandOutcome::Message(format!(
                    "Locus {label}: {} sigue a {}",
                    target_label, driver_label
                ));
            }
            "SampledGraph" if cmd.args.len() == 2 => {
                let expr = cmd.args[0].trim();
                let range = match parse_finite_command_arg(
                    "SampledGraph",
                    "el rango",
                    &cmd.args[1],
                    &document.variables,
                ) {
                    Ok(value) if value > 0.0 => value,
                    _ => {
                        return CommandOutcome::Error(
                            "SampledGraph: el rango debe ser un número finito mayor que cero."
                                .into(),
                        )
                    }
                };
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
                if vertices.len() < 2 {
                    return CommandOutcome::Error(
                        "SampledGraph: la expresión debe producir al menos dos puntos finitos en el rango indicado."
                            .into(),
                    );
                }
                let mut poly = PolygonObj::new(vertices);
                poly.label = "sampled_graph".to_string();
                insert_command_object!(document, GeoObject::Polygon(poly));
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
                let mu = match require_finite(parse_numeric_arg(&cmd.args[0], &document.variables))
                {
                    Ok(value) => value,
                    Err(error) => {
                        return CommandOutcome::Error(format!("Normal: mu inválido: {error}"))
                    }
                };
                let sigma =
                    match require_finite(parse_numeric_arg(&cmd.args[1], &document.variables)) {
                        Ok(value) if value > 0.0 => value,
                        Ok(_) => {
                            return CommandOutcome::Error(
                                "Normal: sigma debe ser finito y mayor que cero".into(),
                            )
                        }
                        Err(error) => {
                            return CommandOutcome::Error(format!(
                                "Normal: sigma inválido: {error}"
                            ))
                        }
                    };
                let expr = format!("exp(-(x-{})^2/(2*{}^2))/({}*sqrt(2*pi))", mu, sigma, sigma);
                insert_command_object!(
                    document,
                    GeoObject::Function(
                        FunctionObj::new(expr).with_label(format!("N({},{})", mu, sigma)),
                    )
                );
                result = CommandOutcome::Message(format!("Normal N({},{}) added", mu, sigma));
                input_text.clear();
                return result;
            }
            "Binomial" if cmd.args.len() == 3 => {
                let n = match parse_discrete_count("Binomial", "n", &cmd.args[0]) {
                    Ok(value) => value as usize,
                    Err(error) => return error,
                };
                let p = command_result!(parse_finite_command_arg(
                    "Binomial",
                    "p",
                    &cmd.args[1],
                    &document.variables,
                ));
                if !(0.0..=1.0).contains(&p) {
                    return CommandOutcome::Error(
                        "Binomial: p debe estar en el intervalo [0, 1]".into(),
                    );
                }
                let k = match parse_discrete_count("Binomial", "k", &cmd.args[2]) {
                    Ok(value) => value as usize,
                    Err(error) => return error,
                };
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
                let prob = if k > n {
                    0.0
                } else {
                    comb(n, k) * p.powi(k as i32) * (1.0 - p).powi((n - k) as i32)
                };
                command_result!(require_finite_outputs("Binomial", &[prob]));
                result = CommandOutcome::Message(format!(
                    "P(X={}) = {:.6} (Binom({},{}))",
                    k, prob, n, p
                ));
                input_text.clear();
                return result;
            }
            "Poisson" if cmd.args.len() == 2 => {
                let lambda = command_result!(parse_finite_command_arg(
                    "Poisson",
                    "lambda",
                    &cmd.args[0],
                    &document.variables,
                ));
                if lambda < 0.0 {
                    return CommandOutcome::Error("Poisson: lambda no puede ser negativo".into());
                }
                let k = match parse_discrete_count("Poisson", "k", &cmd.args[1]) {
                    Ok(value) => value as usize,
                    Err(error) => return error,
                };
                let mut prob = (-lambda).exp();
                for i in 1..=k {
                    prob *= lambda / i as f64;
                }
                command_result!(require_finite_outputs("Poisson", &[prob]));
                result = CommandOutcome::Message(format!(
                    "P(X={}) = {:.6} (Poisson({}))",
                    k, prob, lambda
                ));
                input_text.clear();
                return result;
            }
            "Curve3D" if matches!(cmd.args.len(), 3 | 4) => {
                let inner = cmd.args[0]
                    .trim()
                    .trim_start_matches('(')
                    .trim_end_matches(')');
                let parts = split_args(inner);
                if parts.len() != 3 || parts.iter().any(|part| part.trim().is_empty()) {
                    input_text.clear();
                    return CommandOutcome::Error(
                        "Curve3D: se requieren tres expresiones (x(t), y(t), z(t))".into(),
                    );
                }

                let parameter = if cmd.args.len() == 4 {
                    clean_symbol_arg(&cmd.args[1])
                } else {
                    "t".to_string()
                };
                if !is_valid_parameter_name(&parameter) {
                    return CommandOutcome::Error(
                        "Curve3D: el parámetro debe ser un identificador válido".into(),
                    );
                }
                for (name, component) in ["x", "y", "z"].iter().zip(parts.iter()) {
                    if let Err(error) = validate_curve_3d_expression(
                        component,
                        name,
                        &parameter,
                        &document.variables,
                    ) {
                        return error;
                    }
                }

                let (t_min_idx, t_max_idx) = if cmd.args.len() == 4 { (2, 3) } else { (1, 2) };
                let t_min = match parse_finite_command_arg(
                    "Curve3D",
                    "t_min",
                    &cmd.args[t_min_idx],
                    &document.variables,
                ) {
                    Ok(value) => value,
                    Err(error) => return error,
                };
                let t_max = match parse_finite_command_arg(
                    "Curve3D",
                    "t_max",
                    &cmd.args[t_max_idx],
                    &document.variables,
                ) {
                    Ok(value) => value,
                    Err(error) => return error,
                };
                if let Err(error) =
                    require_ordered_domain("Curve3D", "t_min", "t_max", t_min, t_max)
                {
                    return error;
                }
                let obj = GeoObject::ParametricCurve3D(
                    ParametricCurve3DObj::new(
                        parts[0].trim(),
                        parts[1].trim(),
                        parts[2].trim(),
                        t_min,
                        t_max,
                    )
                    .with_parameter(parameter),
                );
                insert_command_object!(document, obj);
                input_text.clear();
                return CommandOutcome::Ok;
            }
            "SetValue" if cmd.args.len() == 2 => {
                let name = cmd.args[0].trim();
                if let Some(id) = find_object_by_label(document, name) {
                    if let Ok(val) = parse_numeric_arg(&cmd.args[1], &document.variables) {
                        if let Err(error) = document.try_set_variable(name.to_string(), val) {
                            return CommandOutcome::Error(format!("SetValue: {error}"));
                        }
                    } else if let Ok(position) =
                        parse_finite_point_arg(&cmd.args[1], &document.variables)
                    {
                        if !document.constraints.is_free(&id)
                            || !matches!(document.get_object(id), Some(GeoObject::Point(_)))
                        {
                            return CommandOutcome::Error(format!(
                                "SetValue: '{}' debe ser un punto libre.",
                                name
                            ));
                        }
                        match document.try_update_point_and_re_evaluate(id, |point| {
                            point.position = position;
                            Ok(())
                        }) {
                            Ok(_) => {}
                            Err(error) => {
                                return CommandOutcome::Error(format!("SetValue: {error}"))
                            }
                        }
                    } else {
                        return CommandOutcome::Error(
                            "SetValue: se requiere un número finito o un punto (x, y).".into(),
                        );
                    }
                } else if let Ok(value) =
                    require_finite(parse_numeric_arg(&cmd.args[1], &document.variables))
                {
                    let existed = document.variables.contains_key(name);
                    if let Err(error) = document.try_set_variable(name.to_string(), value) {
                        return CommandOutcome::Error(format!("SetValue: {error}"));
                    }
                    input_text.clear();
                    if existed {
                        return CommandOutcome::Ok;
                    }
                    return CommandOutcome::Message(format!(
                        "SetValue: se creó la variable '{}' con valor {}.",
                        name, value
                    ));
                } else {
                    return CommandOutcome::Error(
                        "SetValue: el valor debe ser un número finito para una variable nueva."
                            .into(),
                    );
                }
                input_text.clear();
                return CommandOutcome::Ok;
            }
            "Animate" => {
                let outcome = run_animate_command(&cmd.args, document);
                if !matches!(&outcome, CommandOutcome::Error(_)) {
                    input_text.clear();
                }
                return outcome;
            }
            "GenerateAnimation" => {
                let outcome = run_generate_animation_command(&cmd.args, document);
                if !matches!(&outcome, CommandOutcome::Error(_)) {
                    input_text.clear();
                }
                return outcome;
            }
            "Extrude" if cmd.args.len() == 2 => {
                let height = command_result!(parse_finite_command_arg(
                    "Extrude",
                    "altura",
                    &cmd.args[1],
                    &document.variables,
                ));
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
                    let Some(segment_count) = verts.len().checked_mul(3) else {
                        return CommandOutcome::Error(
                            "Extrude: el número de segmentos excede el máximo permitido".into(),
                        );
                    };
                    let object_count = document.object_count().checked_add(segment_count);
                    if object_count
                        .map(|count| count > grafito_core::validation::MAX_OBJECT_COUNT)
                        .unwrap_or(true)
                    {
                        return CommandOutcome::Error(format!(
                            "Extrude: requiere {segment_count} segmentos, pero excede el máximo de objetos"
                        ));
                    }
                    let constraint_count = document
                        .constraints
                        .constraint_count()
                        .checked_add(segment_count);
                    if constraint_count
                        .map(|count| count > grafito_core::constraints::MAX_CONSTRAINTS)
                        .unwrap_or(true)
                    {
                        return CommandOutcome::Error(format!(
                            "Extrude: requiere {segment_count} restricciones, pero excede el máximo permitido"
                        ));
                    }

                    let mut staged = document.detached_clone_for_staging();
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
                            for (edge_kind, segment) in [
                                (0, Segment3DObj::new(b, t)),
                                (1, Segment3DObj::new(b, bn)),
                                (2, Segment3DObj::new(t, tn)),
                            ] {
                                let mut params = HashMap::new();
                                params.insert("height".to_string(), height);
                                params.insert("edge_index".to_string(), i as f64);
                                params.insert("edge_kind".to_string(), edge_kind as f64);
                                let segment = segment.with_label(unique_object_label(&staged, "E"));
                                if let Err(error) = staged.try_add_constructed_object_with_params(
                                    GeoObject::Segment3D(segment),
                                    "Extrude",
                                    &[poly_id],
                                    params,
                                ) {
                                    return CommandOutcome::Error(format!("Extrude: {error}"));
                                }
                            }
                        }
                    }
                    *document = staged;
                } else {
                    result = CommandOutcome::Error(
                        "Extrude only supports Polygons with 3+ vertices".into(),
                    );
                }
                input_text.clear();
                return result;
            }
            "Script" => {
                if cmd.args.len() != 1 {
                    input_text.clear();
                    return CommandOutcome::Error("Script requires exactly one argument".into());
                }
                if script_budget.depth >= MAX_SCRIPT_DEPTH {
                    input_text.clear();
                    return CommandOutcome::Error(format!(
                        "Script depth exceeds maximum {MAX_SCRIPT_DEPTH}"
                    ));
                }

                let commands = match split_script_commands(&cmd.args[0]) {
                    Ok(commands) => commands,
                    Err(message) => {
                        input_text.clear();
                        return CommandOutcome::Error(message);
                    }
                };
                if script_budget.executed_commands + commands.len() > MAX_SCRIPT_COMMANDS {
                    input_text.clear();
                    return CommandOutcome::Error(format!(
                        "Script exceeds maximum {MAX_SCRIPT_COMMANDS} commands"
                    ));
                }

                script_budget.depth += 1;
                for command in commands {
                    script_budget.executed_commands += 1;
                    let mut nested_input = command;
                    match process_input_in_place_with_budget(
                        document,
                        &mut nested_input,
                        script_budget,
                    ) {
                        CommandOutcome::Message(_) | CommandOutcome::Ok => {}
                        CommandOutcome::Error(message) => {
                            script_budget.depth -= 1;
                            input_text.clear();
                            return CommandOutcome::Error(message);
                        }
                    }
                }
                script_budget.depth -= 1;
                input_text.clear();
                return CommandOutcome::Message("Script executed".into());
            }
            "Lorenz" => {
                let params = command_result!(parse_attractor_params(
                    "Lorenz",
                    &cmd.args,
                    &document.variables,
                    &[10.0, 28.0, 8.0 / 3.0],
                ));
                let obj = GeoObject::Attractor3D(Attractor3DObj::new("lorenz", params));
                insert_command_object!(document, obj);
                input_text.clear();
                return CommandOutcome::Message("Lorenz attractor created".into());
            }
            "Rossler" => {
                let params = command_result!(parse_attractor_params(
                    "Rossler",
                    &cmd.args,
                    &document.variables,
                    &[0.2, 0.2, 5.7],
                ));
                let obj = GeoObject::Attractor3D(Attractor3DObj::new("rossler", params));
                insert_command_object!(document, obj);
                input_text.clear();
                return CommandOutcome::Message("Rössler attractor created".into());
            }
            "Thomas" | "Butterfly" => {
                let params = command_result!(parse_attractor_params(
                    "Thomas",
                    &cmd.args,
                    &document.variables,
                    &[0.208186],
                ));
                let obj = GeoObject::Attractor3D(Attractor3DObj::new("thomas", params));
                insert_command_object!(document, obj);
                input_text.clear();
                return CommandOutcome::Message("Thomas butterfly attractor created".into());
            }
            "Aizawa" => {
                let params = command_result!(parse_attractor_params(
                    "Aizawa",
                    &cmd.args,
                    &document.variables,
                    &[0.95, 0.7, 0.6, 3.5, 0.25, 0.1],
                ));
                let obj = GeoObject::Attractor3D(Attractor3DObj::new("aizawa", params));
                insert_command_object!(document, obj);
                input_text.clear();
                return CommandOutcome::Message("Aizawa attractor created".into());
            }
            "Chen" => {
                let params = command_result!(parse_attractor_params(
                    "Chen",
                    &cmd.args,
                    &document.variables,
                    &[35.0, 3.0, 28.0],
                ));
                let obj = GeoObject::Attractor3D(Attractor3DObj::new("chen", params));
                insert_command_object!(document, obj);
                input_text.clear();
                return CommandOutcome::Message("Chen attractor created".into());
            }
            "Halvorsen" => {
                let params = command_result!(parse_attractor_params(
                    "Halvorsen",
                    &cmd.args,
                    &document.variables,
                    &[1.4, 0.0, 0.0, 0.0],
                ));
                let obj = GeoObject::Attractor3D(Attractor3DObj::new("halvorsen", params));
                insert_command_object!(document, obj);
                input_text.clear();
                return CommandOutcome::Message("Halvorsen attractor created".into());
            }
            "Dadras" => {
                let params = command_result!(parse_attractor_params(
                    "Dadras",
                    &cmd.args,
                    &document.variables,
                    &[3.0, 2.7, 1.7, 2.0, 9.0],
                ));
                let obj = GeoObject::Attractor3D(Attractor3DObj::new("dadras", params));
                insert_command_object!(document, obj);
                input_text.clear();
                return CommandOutcome::Message("Dadras attractor created".into());
            }
            "Chua" => {
                let params = command_result!(parse_attractor_params(
                    "Chua",
                    &cmd.args,
                    &document.variables,
                    &[15.6, 28.0, -1.143, -0.714],
                ));
                let obj = GeoObject::Attractor3D(Attractor3DObj::new("chua", params));
                insert_command_object!(document, obj);
                input_text.clear();
                return CommandOutcome::Message("Chua attractor created".into());
            }
            "Mandelbrot" => {
                let max_iter = match parse_fractal_max_iter(
                    "Mandelbrot",
                    cmd.args.first().map(String::as_str),
                ) {
                    Ok(value) => value,
                    Err(error) => return error,
                };
                let obj = GeoObject::Fractal2D(Fractal2DObj::mandelbrot().with_max_iter(max_iter));
                if let GeoObject::Fractal2D(fractal) = &obj {
                    if let Err(error) = validate_fractal_command_budget("Mandelbrot", fractal) {
                        return error;
                    }
                }
                insert_command_object!(document, obj);
                input_text.clear();
                return CommandOutcome::Message("Mandelbrot fractal created".into());
            }
            "Julia" if matches!(cmd.args.len(), 2 | 3) => {
                let cr = command_result!(parse_finite_command_arg(
                    "Julia",
                    "cr",
                    &cmd.args[0],
                    &document.variables,
                ));
                let ci = command_result!(parse_finite_command_arg(
                    "Julia",
                    "ci",
                    &cmd.args[1],
                    &document.variables,
                ));
                let max_iter =
                    match parse_fractal_max_iter("Julia", cmd.args.get(2).map(String::as_str)) {
                        Ok(value) => value,
                        Err(error) => return error,
                    };
                let obj = GeoObject::Fractal2D(Fractal2DObj::julia(cr, ci).with_max_iter(max_iter));
                if let GeoObject::Fractal2D(fractal) = &obj {
                    if let Err(error) = validate_fractal_command_budget("Julia", fractal) {
                        return error;
                    }
                }
                insert_command_object!(document, obj);
                input_text.clear();
                return CommandOutcome::Message(format!("Julia set c={cr}+{ci}i created"));
            }
            "BurningShip" => {
                let obj = GeoObject::Fractal2D(Fractal2DObj::burning_ship());
                insert_command_object!(document, obj);
                input_text.clear();
                return CommandOutcome::Message("Burning Ship fractal created".into());
            }
            "Pentachoron4D" | "Tesseract4D" | "SixteenCell4D" | "TwentyFourCell4D"
            | "OneTwentyCell4D" | "SixHundredCell4D" => {
                let kind = match cmd.command.as_str() {
                    "Pentachoron4D" => RegularPolychoron::Pentachoron,
                    "Tesseract4D" => RegularPolychoron::Tesseract,
                    "SixteenCell4D" => RegularPolychoron::SixteenCell,
                    "TwentyFourCell4D" => RegularPolychoron::TwentyFourCell,
                    "OneTwentyCell4D" => RegularPolychoron::OneTwentyCell,
                    "SixHundredCell4D" => RegularPolychoron::SixHundredCell,
                    _ => {
                        return CommandOutcome::Error(format!(
                            "Polychoron desconocido: {}",
                            cmd.command
                        ))
                    }
                };
                let (scale, rotation_angles) =
                    command_result!(parse_regular_polychoron_4d_command_args(
                        &cmd.command,
                        &cmd.args,
                        &document.variables,
                    ));
                let mut polychoron = RegularPolychoron4DObj::new(kind);
                polychoron.scale = scale;
                polychoron.rotation_angles = rotation_angles;
                insert_command_object!(document, GeoObject::RegularPolychoron4D(polychoron));
                input_text.clear();
                return CommandOutcome::Ok;
            }
            "SimplexND" | "HypercubeND" | "CrossPolytopeND" => {
                let family = match cmd.command.as_str() {
                    "SimplexND" => RegularPolytopeFamily::Simplex,
                    "HypercubeND" => RegularPolytopeFamily::Hypercube,
                    "CrossPolytopeND" => RegularPolytopeFamily::CrossPolytope,
                    _ => {
                        return CommandOutcome::Error(format!(
                            "Familia politopo desconocida: {}",
                            cmd.command
                        ))
                    }
                };
                let (dimension, scale, rotation_angles) =
                    command_result!(parse_regular_polytope_nd_command_args(
                        &cmd.command,
                        &cmd.args,
                        &document.variables,
                    ));
                let mut polytope = RegularPolytopeNDObj::new(family, dimension);
                polytope.scale = scale;
                polytope.rotation_angles = rotation_angles;
                insert_command_object!(document, GeoObject::RegularPolytopeND(polytope));
                input_text.clear();
                return CommandOutcome::Ok;
            }
            "Hypercube" => {
                let angles = command_result!(parse_attractor_params(
                    "Hypercube",
                    &cmd.args,
                    &document.variables,
                    &[0.3, 0.5, 0.7],
                ));
                let obj =
                    GeoObject::HyperSurface4D(HyperSurface4DObj::hypercube().with_rotation(angles));
                insert_command_object!(document, obj);
                input_text.clear();
                return CommandOutcome::Message("Hipercubo 4D creado (escala=3.0). Botón derecho para orbitar, scroll para zoom.".into());
            }
            "Hypersphere" => {
                let angles = vec![0.3, 0.5, 0.7];
                let obj = GeoObject::HyperSurface4D(
                    HyperSurface4DObj::hypersphere().with_rotation(angles),
                );
                insert_command_object!(document, obj);
                input_text.clear();
                return CommandOutcome::Message("Hiperesfera 4D creada (escala=3.0). Botón derecho para orbitar, scroll para zoom.".into());
            }
            "VectorField3D" if cmd.args.len() >= 3 => {
                let obj = GeoObject::VectorField3D(VectorField3DObj::new(
                    cmd.args[0].trim(),
                    cmd.args[1].trim(),
                    cmd.args[2].trim(),
                ));
                insert_command_object!(document, obj);
                input_text.clear();
                return CommandOutcome::Message("3D Vector Field created".into());
            }
            "Histogram" if !cmd.args.is_empty() => {
                let data = command_result!(parse_data_command_arg(
                    "Histogram",
                    &cmd.args[0],
                    &document.variables,
                ));
                let bins = match cmd.args.get(1) {
                    Some(value) => match value.trim().parse::<usize>() {
                        Ok(value)
                            if (1..=grafito_geometry::statistics::MAX_HISTOGRAM_BINS)
                                .contains(&value) =>
                        {
                            value
                        }
                        Ok(value) => {
                            return CommandOutcome::Error(format!(
                                "Histogram: bins {value} exceeds maximum {}",
                                grafito_geometry::statistics::MAX_HISTOGRAM_BINS
                            ));
                        }
                        Err(_) => {
                            return CommandOutcome::Error(
                                "Histogram: bins debe ser un entero positivo".into(),
                            );
                        }
                    },
                    None => 10,
                };
                if !data.is_empty() {
                    let obj = GeoObject::Histogram(HistogramObj::new(data, bins));
                    insert_command_object!(document, obj);
                    input_text.clear();
                    return CommandOutcome::Message("Histogram created".into());
                }
            }
            "Image" => {
                return CommandOutcome::Error(
                    "Image no está disponible: Grafito aún no tiene un modelo persistente de imagen en el documento."
                        .into(),
                );
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
            "ScatterPlot" if cmd.args.len() >= 2 => {
                let xs = command_result!(parse_data_command_arg(
                    "ScatterPlot",
                    &cmd.args[0],
                    &document.variables,
                ));
                let ys = command_result!(parse_data_command_arg(
                    "ScatterPlot",
                    &cmd.args[1],
                    &document.variables,
                ));
                if xs.iter().chain(ys.iter()).any(|value| !value.is_finite()) {
                    return CommandOutcome::Error(
                        "ScatterPlot: las muestras deben ser finitas".into(),
                    );
                }
                if !xs.is_empty() && xs.len() == ys.len() {
                    let obj = GeoObject::ScatterPlot(ScatterPlotObj::new(xs, ys));
                    insert_command_object!(document, obj);
                    input_text.clear();
                    return CommandOutcome::Message("Scatter plot created".into());
                }
            }
            "DataTable" if cmd.args.len() == 2 => {
                let xs = command_result!(parse_data_command_arg(
                    "DataTable",
                    &cmd.args[0],
                    &document.variables,
                ));
                let ys = command_result!(parse_data_command_arg(
                    "DataTable",
                    &cmd.args[1],
                    &document.variables,
                ));
                if xs.len() != ys.len() {
                    return CommandOutcome::Error(
                        "DataTable: las columnas x e y deben tener la misma longitud".into(),
                    );
                }
                if xs.len() < 2 {
                    return CommandOutcome::Error(
                        "DataTable: se necesitan al menos dos filas finitas".into(),
                    );
                }
                let table = DataTableObj::new("x", "y", xs.clone(), ys.clone());
                let table_id = table.id;
                insert_command_object!(document, GeoObject::DataTable(table));
                insert_command_object!(
                    document,
                    GeoObject::ScatterPlot(ScatterPlotObj::new(xs, ys).linked_to(table_id))
                );
                let label = document
                    .get_object(table_id)
                    .map(|object| object.label().to_string())
                    .unwrap_or_else(|| "tabla".to_string());
                input_text.clear();
                return CommandOutcome::Message(format!(
                    "Tabla local '{label}' creada con gráfico de dispersión enlazado"
                ));
            }
            "FitLinear" if cmd.args.len() == 1 => {
                let (source, xs, ys) =
                    command_result!(data_table_for_fit(document, "FitLinear", &cmd.args[0],));
                let (function, message) = command_result!(fit_function_from_table(
                    "FitLinear",
                    source,
                    &xs,
                    &ys,
                    statistics::FitKind::Linear,
                ));
                insert_command_object!(document, GeoObject::Function(function));
                input_text.clear();
                return CommandOutcome::Message(message);
            }
            "FitPoly" if cmd.args.len() == 2 => {
                let degree = match cmd.args[1].trim().parse::<usize>() {
                    Ok(degree) => degree,
                    Err(_) => {
                        return CommandOutcome::Error(
                            "FitPoly: el grado debe ser un entero positivo".into(),
                        );
                    }
                };
                let (source, xs, ys) =
                    command_result!(data_table_for_fit(document, "FitPoly", &cmd.args[0],));
                let (function, message) = command_result!(fit_function_from_table(
                    "FitPoly",
                    source,
                    &xs,
                    &ys,
                    statistics::FitKind::Polynomial { degree },
                ));
                insert_command_object!(document, GeoObject::Function(function));
                input_text.clear();
                return CommandOutcome::Message(message);
            }
            "FitExp" if cmd.args.len() == 1 => {
                let (source, xs, ys) =
                    command_result!(data_table_for_fit(document, "FitExp", &cmd.args[0],));
                let (function, message) = command_result!(fit_function_from_table(
                    "FitExp",
                    source,
                    &xs,
                    &ys,
                    statistics::FitKind::Exponential,
                ));
                insert_command_object!(document, GeoObject::Function(function));
                input_text.clear();
                return CommandOutcome::Message(message);
            }
            "FitLog" if cmd.args.len() == 1 => {
                let (source, xs, ys) =
                    command_result!(data_table_for_fit(document, "FitLog", &cmd.args[0],));
                let (function, message) = command_result!(fit_function_from_table(
                    "FitLog",
                    source,
                    &xs,
                    &ys,
                    statistics::FitKind::Logarithmic,
                ));
                insert_command_object!(document, GeoObject::Function(function));
                input_text.clear();
                return CommandOutcome::Message(message);
            }
            "FitPow" if cmd.args.len() == 1 => {
                let (source, xs, ys) =
                    command_result!(data_table_for_fit(document, "FitPow", &cmd.args[0],));
                let (function, message) = command_result!(fit_function_from_table(
                    "FitPow",
                    source,
                    &xs,
                    &ys,
                    statistics::FitKind::Power,
                ));
                insert_command_object!(document, GeoObject::Function(function));
                input_text.clear();
                return CommandOutcome::Message(message);
            }
            "FitSin" if cmd.args.len() == 1 => {
                let (source, xs, ys) =
                    command_result!(data_table_for_fit(document, "FitSin", &cmd.args[0],));
                let (function, message) = command_result!(fit_function_from_table(
                    "FitSin",
                    source,
                    &xs,
                    &ys,
                    statistics::FitKind::Sinusoidal,
                ));
                insert_command_object!(document, GeoObject::Function(function));
                input_text.clear();
                return CommandOutcome::Message(message);
            }
            "BoxPlot" if !cmd.args.is_empty() => {
                let data = command_result!(parse_data_command_arg(
                    "BoxPlot",
                    &cmd.args[0],
                    &document.variables,
                ));
                if data.iter().any(|value| !value.is_finite()) {
                    return CommandOutcome::Error("BoxPlot: las muestras deben ser finitas".into());
                }
                if !data.is_empty() {
                    let obj = GeoObject::BoxPlot(BoxPlotObj::new(data));
                    insert_command_object!(document, obj);
                    input_text.clear();
                    return CommandOutcome::Message("Box plot created".into());
                }
            }
            "LinearRegression" if cmd.args.len() >= 2 => {
                let xs = command_result!(parse_data_command_arg(
                    "LinearRegression",
                    &cmd.args[0],
                    &document.variables,
                ));
                let ys = command_result!(parse_data_command_arg(
                    "LinearRegression",
                    &cmd.args[1],
                    &document.variables,
                ));
                if xs.iter().chain(ys.iter()).any(|value| !value.is_finite()) {
                    return CommandOutcome::Error(
                        "LinearRegression: las muestras deben ser finitas".into(),
                    );
                }
                if !xs.is_empty() && xs.len() == ys.len() {
                    if let Some((slope, intercept, r2)) = statistics::linear_regression(&xs, &ys) {
                        command_result!(require_finite_outputs(
                            "LinearRegression",
                            &[slope, intercept, r2],
                        ));
                        let obj = GeoObject::RegressionLine(RegressionLineObj::linear(
                            xs, ys, slope, intercept, r2,
                        ));
                        insert_command_object!(document, obj);
                        input_text.clear();
                        return CommandOutcome::Message(format!(
                            "y = {:.4}x + {:.4}, R²={:.4}",
                            slope, intercept, r2
                        ));
                    }
                }
            }
            "Mean" if !cmd.args.is_empty() => {
                let data = command_result!(parse_data_command_arg(
                    "Mean",
                    &cmd.args[0],
                    &document.variables,
                ));
                if let Some(m) = statistics::mean(&data) {
                    command_result!(require_finite_outputs("Mean", &[m]));
                    input_text.clear();
                    return CommandOutcome::Message(format!("Mean = {:.6}", m));
                }
            }
            "Median" if !cmd.args.is_empty() => {
                let data = command_result!(parse_data_command_arg(
                    "Median",
                    &cmd.args[0],
                    &document.variables,
                ));
                if let Some(m) = statistics::median(&data) {
                    command_result!(require_finite_outputs("Median", &[m]));
                    input_text.clear();
                    return CommandOutcome::Message(format!("Median = {:.6}", m));
                }
            }
            "StdDev" if !cmd.args.is_empty() => {
                let data = command_result!(parse_data_command_arg(
                    "StdDev",
                    &cmd.args[0],
                    &document.variables,
                ));
                if let Some(s) = statistics::std_dev(&data) {
                    command_result!(require_finite_outputs("StdDev", &[s]));
                    input_text.clear();
                    return CommandOutcome::Message(format!("StdDev = {:.6}", s));
                }
            }
            "Correlation" if cmd.args.len() >= 2 => {
                let xs = command_result!(parse_data_command_arg(
                    "Correlation",
                    &cmd.args[0],
                    &document.variables,
                ));
                let ys = command_result!(parse_data_command_arg(
                    "Correlation",
                    &cmd.args[1],
                    &document.variables,
                ));
                if let Some(r) = statistics::pearson_correlation(&xs, &ys) {
                    command_result!(require_finite_outputs("Correlation", &[r]));
                    input_text.clear();
                    return CommandOutcome::Message(format!("r = {:.6}", r));
                }
            }
            "Determinant" if !cmd.args.is_empty() => {
                let matrix = match parse_matrix_arg_strict(&cmd.args[0], &document.variables) {
                    Ok(matrix) => matrix,
                    Err(error) => {
                        return CommandOutcome::Error(format!("Determinant: {error}"));
                    }
                };
                let Some(determinant) = matrix.determinant().filter(|value| value.is_finite())
                else {
                    return CommandOutcome::Error(
                        "Determinant: la matriz no produjo un determinante finito".into(),
                    );
                };
                input_text.clear();
                return CommandOutcome::Message(format!("det = {:.6}", determinant));
            }
            "Inverse" if !cmd.args.is_empty() => {
                let matrix = match parse_matrix_arg_strict(&cmd.args[0], &document.variables) {
                    Ok(matrix) => matrix,
                    Err(error) => return CommandOutcome::Error(format!("Inverse: {error}")),
                };
                let Some(inverse) = matrix.inverse() else {
                    return CommandOutcome::Error("Inverse: la matriz no es invertible".into());
                };
                input_text.clear();
                return CommandOutcome::Message(format!("Inverse:\n{}", inverse));
            }
            "Transpose" if !cmd.args.is_empty() => {
                let matrix = match parse_matrix_arg_strict(&cmd.args[0], &document.variables) {
                    Ok(m) => m,
                    Err(e) => return CommandOutcome::Error(format!("Transpose: {e}")),
                };
                input_text.clear();
                return CommandOutcome::Message(format!("Transpose:\n{}", matrix.transpose()));
            }
            "Trace" if !cmd.args.is_empty() => {
                let matrix = match parse_matrix_arg_strict(&cmd.args[0], &document.variables) {
                    Ok(m) => m,
                    Err(e) => return CommandOutcome::Error(format!("Trace: {e}")),
                };
                let Some(trace) = matrix.trace() else {
                    return CommandOutcome::Error("Trace: la matriz debe ser cuadrada".into());
                };
                input_text.clear();
                return CommandOutcome::Message(format!("trace = {}", fmt_scalar(trace)));
            }
            "Rank" if !cmd.args.is_empty() => {
                let matrix = match parse_matrix_arg_strict(&cmd.args[0], &document.variables) {
                    Ok(m) => m,
                    Err(e) => return CommandOutcome::Error(format!("Rank: {e}")),
                };
                let Some(r) = rank(&matrix) else {
                    return CommandOutcome::Error("Rank: matriz inválida".into());
                };
                input_text.clear();
                return CommandOutcome::Message(format!("rank = {r}"));
            }
            "NullSpace" if !cmd.args.is_empty() => {
                let matrix = match parse_matrix_arg_strict(&cmd.args[0], &document.variables) {
                    Ok(m) => m,
                    Err(e) => return CommandOutcome::Error(format!("NullSpace: {e}")),
                };
                let Some(ns) = null_space(&matrix) else {
                    return CommandOutcome::Error("NullSpace: matriz inválida".into());
                };
                input_text.clear();
                return CommandOutcome::Message(format!(
                    "NullSpace dimension = {}\nbasis = {}",
                    ns.len(),
                    fmt_vector_basis(&ns)
                ));
            }
            "LinearSolve" if cmd.args.len() >= 2 => {
                let a = match parse_matrix_arg_strict(&cmd.args[0], &document.variables) {
                    Ok(m) => m,
                    Err(e) => return CommandOutcome::Error(format!("LinearSolve: {e}")),
                };
                let b = match parse_vector_or_matrix_arg(&cmd.args[1], &document.variables) {
                    Ok(m) => m,
                    Err(e) => return CommandOutcome::Error(format!("LinearSolve: {e}")),
                };
                return solve_linear_command(&a, &b);
            }
            "Eigenvalues" if !cmd.args.is_empty() => {
                let matrix = match parse_matrix_arg_strict(&cmd.args[0], &document.variables) {
                    Ok(m) => m,
                    Err(e) => return CommandOutcome::Error(format!("Eigenvalues: {e}")),
                };
                let Some(values) = eigenvalues(&matrix) else {
                    return CommandOutcome::Error(
                        "Eigenvalues: la matriz debe ser cuadrada".into(),
                    );
                };
                input_text.clear();
                return CommandOutcome::Message(format!(
                    "Eigenvalues: {}",
                    values
                        .iter()
                        .map(|(re, im)| fmt_complex_pair(*re, *im))
                        .collect::<Vec<_>>()
                        .join(", ")
                ));
            }
            "Eigenvectors" if !cmd.args.is_empty() => {
                let matrix = match parse_matrix_arg_strict(&cmd.args[0], &document.variables) {
                    Ok(m) => m,
                    Err(e) => return CommandOutcome::Error(format!("Eigenvectors: {e}")),
                };
                let Some(vectors) = eigenvectors(&matrix) else {
                    return CommandOutcome::Error(
                        "Eigenvectors: la matriz debe ser cuadrada".into(),
                    );
                };
                let lines = vectors
                    .iter()
                    .map(|(v, re, im)| {
                        format!(
                            "lambda = {}, v = {}",
                            fmt_complex_pair(*re, *im),
                            fmt_vector(v)
                        )
                    })
                    .collect::<Vec<_>>()
                    .join("\n");
                input_text.clear();
                return CommandOutcome::Message(format!("Eigenvectors:\n{lines}"));
            }
            "LU" if !cmd.args.is_empty() => {
                let matrix = match parse_matrix_arg_strict(&cmd.args[0], &document.variables) {
                    Ok(m) => m,
                    Err(e) => return CommandOutcome::Error(format!("LU: {e}")),
                };
                let Some((l, u)) = lu_decomposition(&matrix) else {
                    return CommandOutcome::Error("LU: la matriz debe ser cuadrada".into());
                };
                input_text.clear();
                return CommandOutcome::Message(format!("L:\n{}U:\n{}", l, u));
            }
            "QR" if !cmd.args.is_empty() => {
                let matrix = match parse_matrix_arg_strict(&cmd.args[0], &document.variables) {
                    Ok(m) => m,
                    Err(e) => return CommandOutcome::Error(format!("QR: {e}")),
                };
                let Some((q, r)) = qr_decomposition(&matrix) else {
                    return CommandOutcome::Error("QR: matriz inválida".into());
                };
                input_text.clear();
                return CommandOutcome::Message(format!("Q:\n{}R:\n{}", q, r));
            }
            "Cholesky" if !cmd.args.is_empty() => {
                let matrix = match parse_matrix_arg_strict(&cmd.args[0], &document.variables) {
                    Ok(m) => m,
                    Err(e) => return CommandOutcome::Error(format!("Cholesky: {e}")),
                };
                let Some(l) = cholesky(&matrix) else {
                    return CommandOutcome::Error(
                        "Cholesky: matriz no simétrica definida positiva".into(),
                    );
                };
                input_text.clear();
                return CommandOutcome::Message(format!("Cholesky L:\n{}", l));
            }
            "SVD" if !cmd.args.is_empty() => {
                let matrix = match parse_matrix_arg_strict(&cmd.args[0], &document.variables) {
                    Ok(m) => m,
                    Err(e) => return CommandOutcome::Error(format!("SVD: {e}")),
                };
                let Some((u, sigma, v_t)) = svd(&matrix) else {
                    return CommandOutcome::Error("SVD: matriz inválida".into());
                };
                input_text.clear();
                return CommandOutcome::Message(format!(
                    "SVD:\nU:\n{}Sigma = {}\nV^T:\n{}",
                    u,
                    fmt_vector(&sigma),
                    v_t
                ));
            }
            "ConditionNumber" if !cmd.args.is_empty() => {
                let matrix = match parse_matrix_arg_strict(&cmd.args[0], &document.variables) {
                    Ok(m) => m,
                    Err(e) => return CommandOutcome::Error(format!("ConditionNumber: {e}")),
                };
                let Some(cond) = condition_number(&matrix) else {
                    return CommandOutcome::Error("ConditionNumber: matriz inválida".into());
                };
                input_text.clear();
                return CommandOutcome::Message(format!("condition_number = {:.10}", cond));
            }
            "P2Dependence" if !cmd.args.is_empty() => {
                return run_p2_dependence(&cmd.args, document);
            }
            "P2Basis" if !cmd.args.is_empty() => {
                return run_p2_basis(&cmd.args, document);
            }
            "P2Equations" if !cmd.args.is_empty() => {
                return run_p2_equations(&cmd.args, document);
            }
            "SubspaceDimension" if !cmd.args.is_empty() => {
                return run_subspace_dimension(&cmd.args[0], document);
            }
            "SubspaceBasis" if !cmd.args.is_empty() => {
                return run_subspace_basis(&cmd.args[0], document);
            }
            "SubspaceSum" if cmd.args.len() >= 2 => {
                return run_subspace_sum(&cmd.args[0], &cmd.args[1], document);
            }
            "SubspaceIntersection" if cmd.args.len() >= 2 => {
                return run_subspace_intersection(&cmd.args[0], &cmd.args[1], document);
            }
            "OrthogonalComplement" if !cmd.args.is_empty() => {
                return run_orthogonal_complement(&cmd.args[0], document);
            }
            "MatrixParamSolve" if cmd.args.len() >= 2 => {
                return run_matrix_param_solve(&cmd.args[0], &cmd.args[1], document);
            }
            "GaussJordan" if !cmd.args.is_empty() => {
                return run_gauss_jordan_command(&cmd.args, document);
            }
            "GaussJordanSolve" if cmd.args.len() >= 2 => {
                return run_gauss_jordan_solve_command(&cmd.args, document);
            }
            "Cramer" if cmd.args.len() >= 2 => {
                return run_cramer_command(&cmd.args, document);
            }
            "Cofactor" if cmd.args.len() >= 3 => {
                return run_cofactor_command(&cmd.args, document);
            }
            "Adjugate" if !cmd.args.is_empty() => {
                return run_adjugate_command(&cmd.args, document);
            }
            "LaplaceExpansion" if cmd.args.len() >= 3 => {
                return run_laplace_expansion_command(&cmd.args, document);
            }
            "ChangeOfBasis" if cmd.args.len() >= 3 => {
                return run_change_of_basis_command(&cmd.args, document);
            }
            "LinearTransformationMatrix" if cmd.args.len() >= 2 => {
                return run_linear_transformation_matrix_command(&cmd.args, document);
            }
            "Diagonalization" if !cmd.args.is_empty() => {
                return run_diagonalization_command(&cmd.args, document);
            }
            "Gradient" if !cmd.args.is_empty() => {
                return run_gradient_command(&cmd.args, document);
            }
            "JacobianMatrix" if !cmd.args.is_empty() => {
                return run_jacobian_matrix_command(&cmd.args, document);
            }
            "Hessian" if !cmd.args.is_empty() => {
                return run_hessian_command(&cmd.args, document);
            }
            "CriticalPoints" if cmd.args.len() >= 6 => {
                return run_critical_points_command(&cmd.args, document);
            }
            "LagrangeMultipliers" if cmd.args.len() >= 7 => {
                return run_lagrange_multipliers_command(&cmd.args, document);
            }
            "DirectionalDerivative" if cmd.args.len() >= 4 => {
                return run_directional_derivative_command(&cmd.args, document);
            }
            "TangentPlane" if cmd.args.len() >= 2 => {
                return run_tangent_plane_command(&cmd.args, document);
            }
            "Divergence" if !cmd.args.is_empty() => {
                return run_divergence_command(&cmd.args, document);
            }
            "Curl" if !cmd.args.is_empty() => {
                return run_curl_command(&cmd.args, document);
            }
            "DoubleIntegral" if cmd.args.len() >= 7 => {
                return run_double_integral_command(&cmd.args, document, false);
            }
            "SurfaceArea" if cmd.args.len() >= 7 => {
                return run_double_integral_command(&cmd.args, document, true);
            }
            "LineIntegralScalar" if cmd.args.len() >= 6 => {
                return run_line_integral_scalar_command(&cmd.args, document);
            }
            "LineIntegralVector" if cmd.args.len() >= 6 => {
                return run_line_integral_vector_command(&cmd.args, document);
            }
            "TripleIntegral" if cmd.args.len() >= 10 => {
                return run_triple_integral_command(&cmd.args, document);
            }
            "SurfaceIntegralScalar" if cmd.args.len() >= 8 => {
                return run_surface_integral_scalar_command(&cmd.args, document);
            }
            "Flux" if cmd.args.len() >= 8 => {
                return run_flux_command(&cmd.args, document);
            }
            "IsConservative" if !cmd.args.is_empty() => {
                return run_is_conservative_command(&cmd.args, document);
            }
            "PotentialFunction" if !cmd.args.is_empty() => {
                return run_potential_function_command(&cmd.args, document);
            }
            "GreenTheorem" if cmd.args.len() >= 7 => {
                return run_green_theorem_command(&cmd.args, document);
            }
            "StokesTheorem" if cmd.args.len() >= 8 => {
                return run_stokes_theorem_command(&cmd.args, document);
            }
            "GaussOstrogradski" if cmd.args.len() >= 10 => {
                return run_gauss_ostrogradski_command(&cmd.args, document);
            }
            "ChangeOfVariables" if cmd.args.len() >= 3 => {
                return run_change_of_variables_command(&cmd.args, document);
            }
            "RiemannSum" if cmd.args.len() >= 5 => {
                return run_riemann_sum_command(&cmd.args, document);
            }
            "ImproperIntegral" if cmd.args.len() >= 4 => {
                return run_improper_integral_command(&cmd.args, document);
            }
            "BolzanoCheck" if cmd.args.len() >= 4 => {
                return run_bolzano_check_command(&cmd.args, document);
            }
            "RolleCheck" if cmd.args.len() >= 4 => {
                return run_rolle_check_command(&cmd.args, document);
            }
            "MeanValueCheck" if cmd.args.len() >= 4 => {
                return run_mean_value_check_command(&cmd.args, document);
            }
            "CauchyMeanValueCheck" if cmd.args.len() >= 5 => {
                return run_cauchy_mean_value_check_command(&cmd.args, document);
            }
            "LHopital" if cmd.args.len() >= 4 => {
                return run_lhopital_command(&cmd.args, document);
            }
            "AlternatingSeriesTest" if !cmd.args.is_empty() => {
                return run_alternating_series_test_command(&cmd.args, document);
            }
            "IntegralTest" if cmd.args.len() >= 3 => {
                return run_integral_test_command(&cmd.args, document);
            }
            "AbsoluteConvergence" if !cmd.args.is_empty() => {
                return run_absolute_convergence_command(&cmd.args, document);
            }
            "SequenceLimit" if !cmd.args.is_empty() => {
                return run_sequence_limit_command(&cmd.args, document);
            }
            "SeriesSum" if cmd.args.len() >= 4 => {
                return run_series_sum_command(&cmd.args, document);
            }
            "RatioTest" if !cmd.args.is_empty() => {
                return run_series_ratio_test_command(&cmd.args, document);
            }
            "RootTest" if !cmd.args.is_empty() => {
                return run_series_root_test_command(&cmd.args, document);
            }
            "Taylor" if cmd.args.len() >= 2 => {
                let expr = cmd.args[0].trim();
                let var = cmd.args.get(1).map(|s| s.trim()).unwrap_or("x");
                let center = command_result!(parse_optional_finite_command_arg(
                    "Taylor",
                    "centro",
                    &cmd.args,
                    2,
                    0.0,
                    &document.variables,
                ));
                let order = match parse_taylor_order(cmd.args.get(3).map(String::as_str)) {
                    Ok(value) => value,
                    Err(error) => return error,
                };
                match symbolic::taylor_series(expr, var, center, order) {
                    Ok(series) => {
                        let label = next_function_label(document);
                        let obj = GeoObject::Function(FunctionObj::new(&series).with_label(&label));
                        insert_command_object!(document, obj);
                        input_text.clear();
                        return CommandOutcome::Message(format!("Taylor: {} → {}", series, label));
                    }
                    Err(error) => return CommandOutcome::Error(format!("Taylor: {error}")),
                }
            }
            "Cardioid" if cmd.args.len() == 1 => {
                let a = command_result!(parse_finite_command_arg(
                    "Cardioid",
                    "a",
                    &cmd.args[0],
                    &document.variables,
                ));
                let steps = 200;
                let points = grafito_geometry::special_curves::cardioid(a, steps);
                command_result!(require_finite_curve_points("Cardioid", &points));
                let mut poly = PolygonObj::new(points);
                poly.label = "Cardioid".to_string();
                insert_command_object!(document, GeoObject::Polygon(poly));
                input_text.clear();
                return CommandOutcome::Message(format!("Cardioid(a={}) created", a));
            }
            "Rose" if cmd.args.len() == 3 => {
                let a = command_result!(parse_finite_command_arg(
                    "Rose",
                    "a",
                    &cmd.args[0],
                    &document.variables,
                ));
                let n = command_result!(parse_i32_command_arg("Rose", "n", &cmd.args[1]));
                let d = command_result!(parse_i32_command_arg("Rose", "d", &cmd.args[2]));
                let steps = 400;
                let points = match grafito_geometry::special_curves::try_rose(a, n, d, steps) {
                    Ok(points) => points,
                    Err(grafito_geometry::special_curves::RoseError::ZeroFrequencyDenominator) => {
                        return CommandOutcome::Error(
                            "Rose: el denominador no puede ser cero".into(),
                        );
                    }
                    Err(grafito_geometry::special_curves::RoseError::StepLimitExceeded {
                        maximum,
                        ..
                    }) => {
                        return CommandOutcome::Error(format!(
                            "Rose: el número de muestras excede el máximo permitido ({maximum})"
                        ));
                    }
                    Err(grafito_geometry::special_curves::RoseError::AllocationFailed) => {
                        return CommandOutcome::Error(
                            "Rose: no se pudo reservar memoria para las muestras".into(),
                        );
                    }
                };
                command_result!(require_finite_curve_points("Rose", &points));
                let mut poly = PolygonObj::new(points);
                poly.label = format!("Rose({}/{})", n, d);
                insert_command_object!(document, GeoObject::Polygon(poly));
                input_text.clear();
                return CommandOutcome::Message(format!("Rose(a={}, n={}, d={}) created", a, n, d));
            }
            "ArchimedeanSpiral" if cmd.args.len() == 3 => {
                let a = command_result!(parse_finite_command_arg(
                    "ArchimedeanSpiral",
                    "a",
                    &cmd.args[0],
                    &document.variables,
                ));
                let b = command_result!(parse_finite_command_arg(
                    "ArchimedeanSpiral",
                    "b",
                    &cmd.args[1],
                    &document.variables,
                ));
                let max_theta = command_result!(parse_finite_command_arg(
                    "ArchimedeanSpiral",
                    "theta_max",
                    &cmd.args[2],
                    &document.variables,
                ));
                let steps = 300;
                let points =
                    grafito_geometry::special_curves::archimedean_spiral(a, b, max_theta, steps);
                command_result!(require_finite_curve_points("ArchimedeanSpiral", &points));
                let mut poly = PolygonObj::new(points);
                poly.label = "Spiral".to_string();
                insert_command_object!(document, GeoObject::Polygon(poly));
                input_text.clear();
                return CommandOutcome::Message(format!(
                    "Archimedean Spiral(a={}, b={}, θ={}) created",
                    a, b, max_theta
                ));
            }
            "LogarithmicSpiral" if cmd.args.len() == 3 => {
                let a = command_result!(parse_finite_command_arg(
                    "LogarithmicSpiral",
                    "a",
                    &cmd.args[0],
                    &document.variables,
                ));
                let b = command_result!(parse_finite_command_arg(
                    "LogarithmicSpiral",
                    "b",
                    &cmd.args[1],
                    &document.variables,
                ));
                let max_theta = command_result!(parse_finite_command_arg(
                    "LogarithmicSpiral",
                    "theta_max",
                    &cmd.args[2],
                    &document.variables,
                ));
                let steps = 300;
                let points =
                    grafito_geometry::special_curves::logarithmic_spiral(a, b, max_theta, steps);
                command_result!(require_finite_curve_points("LogarithmicSpiral", &points));
                let mut poly = PolygonObj::new(points);
                poly.label = "LogSpiral".to_string();
                insert_command_object!(document, GeoObject::Polygon(poly));
                input_text.clear();
                return CommandOutcome::Message(format!(
                    "Logarithmic Spiral(a={}, b={}, θ={}) created",
                    a, b, max_theta
                ));
            }
            "Lissajous" if cmd.args.len() == 5 => {
                let a = command_result!(parse_finite_command_arg(
                    "Lissajous",
                    "a",
                    &cmd.args[0],
                    &document.variables,
                ));
                let b = command_result!(parse_finite_command_arg(
                    "Lissajous",
                    "b",
                    &cmd.args[1],
                    &document.variables,
                ));
                let freq_x = command_result!(parse_finite_command_arg(
                    "Lissajous",
                    "freq_x",
                    &cmd.args[2],
                    &document.variables,
                ));
                let freq_y = command_result!(parse_finite_command_arg(
                    "Lissajous",
                    "freq_y",
                    &cmd.args[3],
                    &document.variables,
                ));
                let delta = command_result!(parse_finite_command_arg(
                    "Lissajous",
                    "delta",
                    &cmd.args[4],
                    &document.variables,
                ));
                let steps = 400;
                let points =
                    grafito_geometry::special_curves::lissajous(a, b, freq_x, freq_y, delta, steps);
                command_result!(require_finite_curve_points("Lissajous", &points));
                let mut poly = PolygonObj::new(points);
                poly.label = "Lissajous".to_string();
                insert_command_object!(document, GeoObject::Polygon(poly));
                input_text.clear();
                return CommandOutcome::Message(format!(
                    "Lissajous(a={}, b={}, fx={}, fy={}, δ={}) created",
                    a, b, freq_x, freq_y, delta
                ));
            }
            "Epicycloid" if cmd.args.len() == 2 => {
                let r = command_result!(parse_finite_command_arg(
                    "Epicycloid",
                    "r",
                    &cmd.args[0],
                    &document.variables,
                ));
                let k = command_result!(parse_finite_command_arg(
                    "Epicycloid",
                    "k",
                    &cmd.args[1],
                    &document.variables,
                ));
                let steps = 400;
                let points = grafito_geometry::special_curves::epicycloid(r, k, steps);
                command_result!(require_finite_curve_points("Epicycloid", &points));
                let mut poly = PolygonObj::new(points);
                poly.label = "Epicycloid".to_string();
                insert_command_object!(document, GeoObject::Polygon(poly));
                input_text.clear();
                return CommandOutcome::Message(format!("Epicycloid(r={}, k={}) created", r, k));
            }
            "Hypocycloid" if cmd.args.len() == 2 => {
                let r = command_result!(parse_finite_command_arg(
                    "Hypocycloid",
                    "r",
                    &cmd.args[0],
                    &document.variables,
                ));
                let k = command_result!(parse_finite_command_arg(
                    "Hypocycloid",
                    "k",
                    &cmd.args[1],
                    &document.variables,
                ));
                let steps = 400;
                let points = grafito_geometry::special_curves::hypocycloid(r, k, steps);
                command_result!(require_finite_curve_points("Hypocycloid", &points));
                let mut poly = PolygonObj::new(points);
                poly.label = "Hypocycloid".to_string();
                insert_command_object!(document, GeoObject::Polygon(poly));
                input_text.clear();
                return CommandOutcome::Message(format!("Hypocycloid(r={}, k={}) created", r, k));
            }
            "ODE" if (4..=7).contains(&cmd.args.len()) => {
                let expr = cmd.args[0].trim();
                let t0 = command_result!(parse_finite_command_arg(
                    "ODE",
                    "t0",
                    &cmd.args[1],
                    &document.variables,
                ));
                let y0 = command_result!(parse_finite_command_arg(
                    "ODE",
                    "y0",
                    &cmd.args[2],
                    &document.variables,
                ));
                let t_end = command_result!(parse_finite_command_arg(
                    "ODE",
                    "t_end",
                    &cmd.args[3],
                    &document.variables,
                ));
                let steps = match cmd.args.get(4) {
                    Some(value) => match value.trim().parse::<usize>() {
                        Ok(value) if value > 0 => value,
                        _ => {
                            return CommandOutcome::Error(
                                "ODE: steps debe ser un entero positivo".into(),
                            )
                        }
                    },
                    None => 200,
                };
                if let Err(outcome) = validate_ode_plot_steps("ODE", steps) {
                    return outcome;
                }
                let method = cmd
                    .args
                    .get(5)
                    .map(|s| s.trim().to_lowercase())
                    .unwrap_or("rk4".to_string());
                if !matches!(
                    method.as_str(),
                    "euler"
                        | "rk4"
                        | "rk45"
                        | "rkf45"
                        | "fehlberg"
                        | "backward"
                        | "backwardeuler"
                        | "backward_euler"
                        | "implicit"
                ) {
                    return CommandOutcome::Error(format!("ODE: método desconocido '{method}'"));
                }
                if !matches!(method.as_str(), "rk45" | "rkf45" | "fehlberg") && t0 != t_end {
                    let step = (t_end - t0) / steps as f64;
                    if !step.is_finite() || step == 0.0 || t0 + step == t0 {
                        return CommandOutcome::Error(
                            "ODE: el paso fijo no puede avanzar el tiempo solicitado".into(),
                        );
                    }
                }
                let tolerance = command_result!(parse_optional_finite_command_arg(
                    "ODE",
                    "tolerancia",
                    &cmd.args,
                    6,
                    1e-6,
                    &document.variables,
                ));
                if tolerance <= 0.0 {
                    return CommandOutcome::Error("ODE: la tolerancia debe ser positiva".into());
                }

                let eval_error = std::cell::Cell::new(false);
                let f = |t: f64, y: f64| -> f64 {
                    let mut vars = document.variables.clone();
                    vars.insert("t".to_string(), t);
                    vars.insert("y".to_string(), y);
                    match evaluate(
                        expr,
                        &vars
                            .iter()
                            .map(|(k, v)| (k.clone(), *v))
                            .collect::<Vec<_>>(),
                    ) {
                        Ok(v) if v.is_finite() => v,
                        _ => {
                            eval_error.set(true);
                            f64::NAN
                        }
                    }
                };

                let _ = f(t0, y0);
                if eval_error.get() {
                    return CommandOutcome::Error(format!(
                        "ODE: no se pudo evaluar la derivada '{}' en la condición inicial",
                        expr
                    ));
                }

                let solution = match method.as_str() {
                    "euler" => grafito_geometry::ode::euler(f, t0, y0, t_end, steps),
                    "rk45" | "rkf45" | "fehlberg" => {
                        match grafito_geometry::ode::try_runge_kutta_45(f, t0, y0, t_end, tolerance)
                        {
                            Ok(solution) => solution,
                            Err(error) => {
                                return CommandOutcome::Error(format!(
                                    "ODE: RKF45 no completó la integración: {error}"
                                ));
                            }
                        }
                    }
                    "backward" | "backwardeuler" | "backward_euler" | "implicit" => {
                        let jac_expr =
                            symbolic::derivative(expr, "y").unwrap_or_else(|_| "0".into());
                        let jac = |t: f64, y: f64| -> f64 {
                            let mut vars = document.variables.clone();
                            vars.insert("t".to_string(), t);
                            vars.insert("y".to_string(), y);
                            match evaluate(
                                &jac_expr,
                                &vars
                                    .iter()
                                    .map(|(k, v)| (k.clone(), *v))
                                    .collect::<Vec<_>>(),
                            ) {
                                Ok(v) if v.is_finite() => v,
                                _ => {
                                    eval_error.set(true);
                                    f64::NAN
                                }
                            }
                        };
                        grafito_geometry::ode::backward_euler(f, jac, t0, y0, t_end, steps)
                    }
                    _ => grafito_geometry::ode::runge_kutta_4(f, t0, y0, t_end, steps),
                };

                if eval_error.get() || solution.iter().any(|(_, y)| !y.is_finite()) {
                    return CommandOutcome::Error(format!(
                        "ODE: la expresión '{}' produjo valores no finitos durante la integración",
                        expr
                    ));
                }
                if t0 != t_end && solution.windows(2).any(|points| points[0].0 == points[1].0) {
                    return CommandOutcome::Error(
                        "ODE: la integración dejó de avanzar antes del tiempo final".into(),
                    );
                }
                let endpoint_tolerance = 8.0 * f64::EPSILON * t_end.abs().max(1.0);
                if !solution
                    .last()
                    .is_some_and(|(t, _)| (t - t_end).abs() <= endpoint_tolerance)
                {
                    return CommandOutcome::Error(
                        "ODE: la integración no alcanzó el tiempo final solicitado".into(),
                    );
                }

                let plot_indices = bounded_ode_plot_indices(solution.len());
                let points: Vec<Point2> = plot_indices
                    .into_iter()
                    .map(|index| {
                        let (t, y) = solution[index];
                        Point2::new(t, y)
                    })
                    .collect();
                let plotted_point_count = points.len();
                if points.len() >= 2 {
                    let pencil = PencilObj::new(points).with_label(format!("ODE({})", method));
                    insert_command_object!(document, GeoObject::Pencil(pencil));
                }
                input_text.clear();
                let detail = if plotted_point_count < solution.len() {
                    format!(
                        "{} points, decimated to {} trajectory points",
                        solution.len(),
                        plotted_point_count
                    )
                } else {
                    format!("{} points", plotted_point_count)
                };
                return CommandOutcome::Message(format!(
                    "ODE solved with {} method ({detail})",
                    method
                ));
            }
            "ODESystem" if (5..=9).contains(&cmd.args.len()) => {
                let expr1 = cmd.args[0].trim();
                let expr2 = cmd.args[1].trim();
                let t0 = command_result!(parse_finite_command_arg(
                    "ODESystem",
                    "t0",
                    &cmd.args[2],
                    &document.variables,
                ));
                let y0_1 = command_result!(parse_finite_command_arg(
                    "ODESystem",
                    "y0_1",
                    &cmd.args[3],
                    &document.variables,
                ));
                let y0_2 = command_result!(parse_finite_command_arg(
                    "ODESystem",
                    "y0_2",
                    &cmd.args[4],
                    &document.variables,
                ));
                let t_end = command_result!(parse_optional_finite_command_arg(
                    "ODESystem",
                    "t_end",
                    &cmd.args,
                    5,
                    10.0,
                    &document.variables,
                ));
                let steps = match cmd.args.get(6) {
                    Some(value) => match value.trim().parse::<usize>() {
                        Ok(value) if value > 0 => value,
                        _ => {
                            return CommandOutcome::Error(
                                "ODESystem: steps debe ser un entero positivo".into(),
                            )
                        }
                    },
                    None => 200,
                };
                if let Err(outcome) = validate_ode_plot_steps("ODESystem", steps) {
                    return outcome;
                }
                let method = cmd
                    .args
                    .get(7)
                    .map(|s| s.trim().to_lowercase())
                    .unwrap_or("rk4".to_string());
                if !matches!(
                    method.as_str(),
                    "euler" | "rk4" | "rk45" | "rkf45" | "fehlberg"
                ) {
                    return CommandOutcome::Error(format!(
                        "ODESystem: método desconocido '{method}'"
                    ));
                }
                let tolerance = command_result!(parse_optional_finite_command_arg(
                    "ODESystem",
                    "tolerancia",
                    &cmd.args,
                    8,
                    1e-6,
                    &document.variables,
                ));
                if tolerance <= 0.0 {
                    return CommandOutcome::Error(
                        "ODESystem: la tolerancia debe ser positiva".into(),
                    );
                }

                let eval_error = std::cell::Cell::new(false);
                let f = |t: f64, state: &[f64]| -> Vec<f64> {
                    let mut vars = document.variables.clone();
                    vars.insert("t".to_string(), t);
                    vars.insert("x".to_string(), state[0]);
                    vars.insert("y".to_string(), state[1]);
                    let dy1 = match evaluate(
                        expr1,
                        &vars
                            .iter()
                            .map(|(k, v)| (k.clone(), *v))
                            .collect::<Vec<_>>(),
                    ) {
                        Ok(v) if v.is_finite() => v,
                        _ => {
                            eval_error.set(true);
                            f64::NAN
                        }
                    };
                    let dy2 = match evaluate(
                        expr2,
                        &vars
                            .iter()
                            .map(|(k, v)| (k.clone(), *v))
                            .collect::<Vec<_>>(),
                    ) {
                        Ok(v) if v.is_finite() => v,
                        _ => {
                            eval_error.set(true);
                            f64::NAN
                        }
                    };
                    vec![dy1, dy2]
                };

                let initial = vec![y0_1, y0_2];
                let _ = f(t0, &initial);
                if eval_error.get() {
                    return CommandOutcome::Error(format!(
                        "ODESystem: no se pudieron evaluar las derivadas '{}' y '{}' en la condición inicial",
                        expr1, expr2
                    ));
                }
                let solution = match method.as_str() {
                    "euler" => grafito_geometry::ode::euler_system(f, t0, initial, t_end, steps),
                    "rk45" | "rkf45" | "fehlberg" => {
                        match grafito_geometry::ode::try_runge_kutta_45_system(
                            f, t0, &initial, t_end, tolerance,
                        ) {
                            Ok(solution) => solution,
                            Err(error) => {
                                return CommandOutcome::Error(format!(
                                    "ODESystem: RKF45 no completó la integración: {error}"
                                ));
                            }
                        }
                    }
                    _ => grafito_geometry::ode::runge_kutta_4_system(f, t0, initial, t_end, steps),
                };

                if eval_error.get()
                    || solution
                        .iter()
                        .any(|(_, state)| state.iter().any(|v| !v.is_finite()))
                {
                    return CommandOutcome::Error(
                        "ODESystem: las expresiones produjeron valores no finitos durante la integración"
                            .to_string(),
                    );
                }
                let endpoint_tolerance = 8.0 * f64::EPSILON * t_end.abs().max(1.0);
                if !solution
                    .last()
                    .is_some_and(|(t, _)| (t - t_end).abs() <= endpoint_tolerance)
                {
                    return CommandOutcome::Error(
                        "ODESystem: la integración no alcanzó el tiempo final solicitado".into(),
                    );
                }

                // Plot y1 vs y2 (phase portrait)
                let plot_indices = bounded_ode_plot_indices(solution.len());
                let points: Vec<Point2> = plot_indices
                    .into_iter()
                    .map(|index| {
                        let (_, state) = &solution[index];
                        Point2::new(state[0], state[1])
                    })
                    .collect();
                let plotted_point_count = points.len();

                if points.len() >= 2 {
                    let pencil = PencilObj::new(points).with_label(format!("Phase({})", method));
                    insert_command_object!(document, GeoObject::Pencil(pencil));
                }
                input_text.clear();
                let detail = if plotted_point_count < solution.len() {
                    format!(
                        "{} points, decimated to {} trajectory points",
                        solution.len(),
                        plotted_point_count
                    )
                } else {
                    format!("{} points", plotted_point_count)
                };
                return CommandOutcome::Message(format!(
                    "ODE system solved with {} method ({detail})",
                    method
                ));
            }
            "Gamma" if cmd.args.len() == 1 => {
                let x = command_result!(parse_finite_command_arg(
                    "Gamma",
                    "x",
                    &cmd.args[0],
                    &document.variables,
                ));
                let value = grafito_geometry::special_functions::gamma(x);
                command_result!(require_finite_outputs("Gamma", &[value]));
                input_text.clear();
                return CommandOutcome::Message(format!("Γ({}) = {:.6}", x, value));
            }
            "LnGamma" if cmd.args.len() == 1 => {
                let x = command_result!(parse_finite_command_arg(
                    "LnGamma",
                    "x",
                    &cmd.args[0],
                    &document.variables,
                ));
                let value = grafito_geometry::special_functions::ln_gamma(x);
                command_result!(require_finite_outputs("LnGamma", &[value]));
                input_text.clear();
                return CommandOutcome::Message(format!("ln(Γ({})) = {:.6}", x, value));
            }
            "Beta" if cmd.args.len() == 2 => {
                let a = command_result!(parse_finite_command_arg(
                    "Beta",
                    "a",
                    &cmd.args[0],
                    &document.variables,
                ));
                let b = command_result!(parse_finite_command_arg(
                    "Beta",
                    "b",
                    &cmd.args[1],
                    &document.variables,
                ));
                let value = grafito_geometry::special_functions::beta(a, b);
                command_result!(require_finite_outputs("Beta", &[value]));
                input_text.clear();
                return CommandOutcome::Message(format!("B({}, {}) = {:.6}", a, b, value));
            }
            "BesselJ" if cmd.args.len() == 2 => {
                let n = command_result!(parse_i32_command_arg("BesselJ", "orden", &cmd.args[0]));
                if !grafito_geometry::special_functions::bessel_j_order_is_supported(n) {
                    return CommandOutcome::Error(format!(
                        "BesselJ: el orden debe estar entre -{} y {}",
                        grafito_geometry::special_functions::MAX_BESSEL_J_ORDER,
                        grafito_geometry::special_functions::MAX_BESSEL_J_ORDER,
                    ));
                }
                let x = command_result!(parse_finite_command_arg(
                    "BesselJ",
                    "x",
                    &cmd.args[1],
                    &document.variables,
                ));
                let value = grafito_geometry::special_functions::bessel_j(n, x);
                command_result!(require_finite_outputs("BesselJ", &[value]));
                input_text.clear();
                return CommandOutcome::Message(format!("J_{}({}) = {:.6}", n, x, value));
            }
            "BesselY" if cmd.args.len() == 2 => {
                let n = command_result!(parse_i32_command_arg("BesselY", "orden", &cmd.args[0]));
                if !grafito_geometry::special_functions::bessel_y_order_is_supported(n) {
                    return CommandOutcome::Error(format!(
                        "BesselY: el orden debe estar entre -{} y {}",
                        grafito_geometry::special_functions::MAX_BESSEL_Y_ORDER,
                        grafito_geometry::special_functions::MAX_BESSEL_Y_ORDER,
                    ));
                }
                let x = command_result!(parse_finite_command_arg(
                    "BesselY",
                    "x",
                    &cmd.args[1],
                    &document.variables,
                ));
                let value = grafito_geometry::special_functions::bessel_y(n, x);
                command_result!(require_finite_outputs("BesselY", &[value]));
                input_text.clear();
                return CommandOutcome::Message(format!("Y_{}({}) = {:.6}", n, x, value));
            }
            "BesselI" if cmd.args.len() == 2 => {
                let n = command_result!(parse_i32_command_arg("BesselI", "orden", &cmd.args[0]));
                if !grafito_geometry::special_functions::bessel_i_order_is_supported(n) {
                    return CommandOutcome::Error(format!(
                        "BesselI: el orden debe estar entre -{} y {}",
                        grafito_geometry::special_functions::MAX_BESSEL_I_ORDER,
                        grafito_geometry::special_functions::MAX_BESSEL_I_ORDER,
                    ));
                }
                let x = command_result!(parse_finite_command_arg(
                    "BesselI",
                    "x",
                    &cmd.args[1],
                    &document.variables,
                ));
                let value = grafito_geometry::special_functions::bessel_i(n, x);
                command_result!(require_finite_outputs("BesselI", &[value]));
                input_text.clear();
                return CommandOutcome::Message(format!("I_{}({}) = {:.6}", n, x, value));
            }
            "Erf" if cmd.args.len() == 1 => {
                let x = command_result!(parse_finite_command_arg(
                    "Erf",
                    "x",
                    &cmd.args[0],
                    &document.variables,
                ));
                let value = grafito_geometry::special_functions::erf(x);
                command_result!(require_finite_outputs("Erf", &[value]));
                input_text.clear();
                return CommandOutcome::Message(format!("erf({}) = {:.6}", x, value));
            }
            "Erfc" if cmd.args.len() == 1 => {
                let x = command_result!(parse_finite_command_arg(
                    "Erfc",
                    "x",
                    &cmd.args[0],
                    &document.variables,
                ));
                let value = grafito_geometry::special_functions::erfc(x);
                command_result!(require_finite_outputs("Erfc", &[value]));
                input_text.clear();
                return CommandOutcome::Message(format!("erfc({}) = {:.6}", x, value));
            }
            "Digamma" if cmd.args.len() == 1 => {
                let x = command_result!(parse_finite_command_arg(
                    "Digamma",
                    "x",
                    &cmd.args[0],
                    &document.variables,
                ));
                let value = grafito_geometry::special_functions::digamma(x);
                command_result!(require_finite_outputs("Digamma", &[value]));
                input_text.clear();
                return CommandOutcome::Message(format!("ψ({}) = {:.6}", x, value));
            }
            "Uniform" if matches!(cmd.args.len(), 2 | 3) => {
                let a = command_result!(parse_finite_command_arg(
                    "Uniform",
                    "a",
                    &cmd.args[0],
                    &document.variables,
                ));
                let b = command_result!(parse_finite_command_arg(
                    "Uniform",
                    "b",
                    &cmd.args[1],
                    &document.variables,
                ));
                let x = command_result!(parse_optional_finite_command_arg(
                    "Uniform",
                    "x",
                    &cmd.args,
                    2,
                    0.5,
                    &document.variables,
                ));
                if a >= b {
                    return CommandOutcome::Error("Uniform: se requiere a < b".into());
                }
                let pdf = grafito_geometry::statistics::uniform_pdf(x, a, b);
                let cdf = grafito_geometry::statistics::uniform_cdf(x, a, b);
                command_result!(require_finite_outputs("Uniform", &[pdf, cdf]));
                input_text.clear();
                return CommandOutcome::Message(format!(
                    "U({},{}): PDF({}) = {:.6}, CDF({}) = {:.6}",
                    a, b, x, pdf, x, cdf
                ));
            }
            "GammaDist" if matches!(cmd.args.len(), 2 | 3) => {
                let alpha = command_result!(parse_finite_command_arg(
                    "GammaDist",
                    "alpha",
                    &cmd.args[0],
                    &document.variables,
                ));
                let beta = command_result!(parse_finite_command_arg(
                    "GammaDist",
                    "beta",
                    &cmd.args[1],
                    &document.variables,
                ));
                let x = command_result!(parse_optional_finite_command_arg(
                    "GammaDist",
                    "x",
                    &cmd.args,
                    2,
                    1.0,
                    &document.variables,
                ));
                if alpha <= 0.0 || beta <= 0.0 {
                    return CommandOutcome::Error(
                        "GammaDist: alpha y beta deben ser positivos".into(),
                    );
                }
                let pdf = grafito_geometry::statistics::gamma_pdf(x, alpha, beta);
                command_result!(require_finite_outputs("GammaDist", &[pdf]));
                input_text.clear();
                return CommandOutcome::Message(format!(
                    "Gamma({},{}): PDF({}) = {:.6}",
                    alpha, beta, x, pdf
                ));
            }
            "BetaDist" if matches!(cmd.args.len(), 2 | 3) => {
                let alpha = command_result!(parse_finite_command_arg(
                    "BetaDist",
                    "alpha",
                    &cmd.args[0],
                    &document.variables,
                ));
                let beta = command_result!(parse_finite_command_arg(
                    "BetaDist",
                    "beta",
                    &cmd.args[1],
                    &document.variables,
                ));
                let x = command_result!(parse_optional_finite_command_arg(
                    "BetaDist",
                    "x",
                    &cmd.args,
                    2,
                    0.5,
                    &document.variables,
                ));
                if alpha <= 0.0 || beta <= 0.0 {
                    return CommandOutcome::Error(
                        "BetaDist: alpha y beta deben ser positivos".into(),
                    );
                }
                let pdf = grafito_geometry::statistics::beta_pdf(x, alpha, beta);
                command_result!(require_finite_outputs("BetaDist", &[pdf]));
                input_text.clear();
                return CommandOutcome::Message(format!(
                    "Beta({},{}): PDF({}) = {:.6}",
                    alpha, beta, x, pdf
                ));
            }
            "Cauchy" if matches!(cmd.args.len(), 2 | 3) => {
                let x0 = command_result!(parse_finite_command_arg(
                    "Cauchy",
                    "x0",
                    &cmd.args[0],
                    &document.variables,
                ));
                let gamma = command_result!(parse_finite_command_arg(
                    "Cauchy",
                    "gamma",
                    &cmd.args[1],
                    &document.variables,
                ));
                let x = command_result!(parse_optional_finite_command_arg(
                    "Cauchy",
                    "x",
                    &cmd.args,
                    2,
                    0.0,
                    &document.variables,
                ));
                if gamma <= 0.0 {
                    return CommandOutcome::Error("Cauchy: gamma debe ser positivo".into());
                }
                let pdf = grafito_geometry::statistics::cauchy_pdf(x, x0, gamma);
                let cdf = grafito_geometry::statistics::cauchy_cdf(x, x0, gamma);
                command_result!(require_finite_outputs("Cauchy", &[pdf, cdf]));
                input_text.clear();
                return CommandOutcome::Message(format!(
                    "Cauchy({},{}): PDF({}) = {:.6}, CDF({}) = {:.6}",
                    x0, gamma, x, pdf, x, cdf
                ));
            }
            "Pareto" if matches!(cmd.args.len(), 2 | 3) => {
                let xm = command_result!(parse_finite_command_arg(
                    "Pareto",
                    "xm",
                    &cmd.args[0],
                    &document.variables,
                ));
                let alpha = command_result!(parse_finite_command_arg(
                    "Pareto",
                    "alpha",
                    &cmd.args[1],
                    &document.variables,
                ));
                let x = command_result!(parse_optional_finite_command_arg(
                    "Pareto",
                    "x",
                    &cmd.args,
                    2,
                    2.0,
                    &document.variables,
                ));
                if xm <= 0.0 || alpha <= 0.0 {
                    return CommandOutcome::Error("Pareto: xm y alpha deben ser positivos".into());
                }
                let pdf = grafito_geometry::statistics::pareto_pdf(x, xm, alpha);
                let cdf = grafito_geometry::statistics::pareto_cdf(x, xm, alpha);
                command_result!(require_finite_outputs("Pareto", &[pdf, cdf]));
                input_text.clear();
                return CommandOutcome::Message(format!(
                    "Pareto({},{}): PDF({}) = {:.6}, CDF({}) = {:.6}",
                    xm, alpha, x, pdf, x, cdf
                ));
            }
            "Rayleigh" if matches!(cmd.args.len(), 1 | 2) => {
                let sigma = command_result!(parse_finite_command_arg(
                    "Rayleigh",
                    "sigma",
                    &cmd.args[0],
                    &document.variables,
                ));
                let x = command_result!(parse_optional_finite_command_arg(
                    "Rayleigh",
                    "x",
                    &cmd.args,
                    1,
                    1.0,
                    &document.variables,
                ));
                if sigma <= 0.0 {
                    return CommandOutcome::Error("Rayleigh: sigma debe ser positivo".into());
                }
                let pdf = grafito_geometry::statistics::rayleigh_pdf(x, sigma);
                let cdf = grafito_geometry::statistics::rayleigh_cdf(x, sigma);
                command_result!(require_finite_outputs("Rayleigh", &[pdf, cdf]));
                input_text.clear();
                return CommandOutcome::Message(format!(
                    "Rayleigh({}): PDF({}) = {:.6}, CDF({}) = {:.6}",
                    sigma, x, pdf, x, cdf
                ));
            }
            "Laplace" if matches!(cmd.args.len(), 2 | 3) => {
                let mu = command_result!(parse_finite_command_arg(
                    "Laplace",
                    "mu",
                    &cmd.args[0],
                    &document.variables,
                ));
                let b = command_result!(parse_finite_command_arg(
                    "Laplace",
                    "b",
                    &cmd.args[1],
                    &document.variables,
                ));
                let x = command_result!(parse_optional_finite_command_arg(
                    "Laplace",
                    "x",
                    &cmd.args,
                    2,
                    0.0,
                    &document.variables,
                ));
                if b <= 0.0 {
                    return CommandOutcome::Error("Laplace: b debe ser positivo".into());
                }
                let pdf = grafito_geometry::statistics::laplace_pdf(x, mu, b);
                let cdf = grafito_geometry::statistics::laplace_cdf(x, mu, b);
                command_result!(require_finite_outputs("Laplace", &[pdf, cdf]));
                input_text.clear();
                return CommandOutcome::Message(format!(
                    "Laplace({},{}): PDF({}) = {:.6}, CDF({}) = {:.6}",
                    mu, b, x, pdf, x, cdf
                ));
            }
            "NegBinomial" if matches!(cmd.args.len(), 2 | 3) => {
                let r = match parse_discrete_count("NegBinomial", "r", &cmd.args[0]) {
                    Ok(value) => value,
                    Err(error) => return error,
                };
                if r == 0 {
                    return CommandOutcome::Error("NegBinomial: r debe ser positivo".into());
                }
                let p = command_result!(parse_finite_command_arg(
                    "NegBinomial",
                    "p",
                    &cmd.args[1],
                    &document.variables,
                ));
                if !(0.0..=1.0).contains(&p) || p == 0.0 {
                    return CommandOutcome::Error(
                        "NegBinomial: p debe estar en el intervalo (0, 1]".into(),
                    );
                }
                let k = match cmd.args.get(2) {
                    Some(value) => match parse_discrete_count("NegBinomial", "k", value) {
                        Ok(value) => value,
                        Err(error) => return error,
                    },
                    None => 0,
                };
                let pmf = grafito_geometry::statistics::negative_binomial_pmf(r, p, k);
                let cdf = grafito_geometry::statistics::negative_binomial_cdf(r, p, k);
                command_result!(require_finite_outputs("NegBinomial", &[pmf, cdf]));
                input_text.clear();
                return CommandOutcome::Message(format!(
                    "NegBin({},{}): PMF({}) = {:.6}, CDF({}) = {:.6}",
                    r, p, k, pmf, k, cdf
                ));
            }
            "TTest" if cmd.args.len() == 2 => {
                let data = command_result!(parse_data_command_arg(
                    "TTest",
                    &cmd.args[0],
                    &document.variables,
                ));
                let mu0 = command_result!(parse_finite_command_arg(
                    "TTest",
                    "mu0",
                    &cmd.args[1],
                    &document.variables,
                ));
                if let Some((t_stat, p_value)) =
                    grafito_geometry::statistics::t_test_one_sample(&data, mu0)
                {
                    command_result!(require_finite_outputs("TTest", &[t_stat, p_value]));
                    input_text.clear();
                    return CommandOutcome::Message(format!(
                        "t-test: t = {:.4}, p = {:.6}",
                        t_stat, p_value
                    ));
                }
            }
            "TTest2" if cmd.args.len() == 2 => {
                let data1 = command_result!(parse_data_command_arg(
                    "TTest2",
                    &cmd.args[0],
                    &document.variables,
                ));
                let data2 = command_result!(parse_data_command_arg(
                    "TTest2",
                    &cmd.args[1],
                    &document.variables,
                ));
                if let Some((t_stat, p_value)) =
                    grafito_geometry::statistics::t_test_two_sample(&data1, &data2)
                {
                    command_result!(require_finite_outputs("TTest2", &[t_stat, p_value]));
                    input_text.clear();
                    return CommandOutcome::Message(format!(
                        "t-test (2 samples): t = {:.4}, p = {:.6}",
                        t_stat, p_value
                    ));
                }
            }
            "ZTest" if cmd.args.len() == 3 => {
                let data = command_result!(parse_data_command_arg(
                    "ZTest",
                    &cmd.args[0],
                    &document.variables,
                ));
                let mu0 = command_result!(parse_finite_command_arg(
                    "ZTest",
                    "mu0",
                    &cmd.args[1],
                    &document.variables,
                ));
                let sigma = command_result!(parse_finite_command_arg(
                    "ZTest",
                    "sigma",
                    &cmd.args[2],
                    &document.variables,
                ));
                if sigma <= 0.0 {
                    return CommandOutcome::Error("ZTest: sigma debe ser positivo".into());
                }
                if let Some((z_stat, p_value)) =
                    grafito_geometry::statistics::z_test_one_sample(&data, mu0, sigma)
                {
                    command_result!(require_finite_outputs("ZTest", &[z_stat, p_value]));
                    input_text.clear();
                    return CommandOutcome::Message(format!(
                        "z-test: z = {:.4}, p = {:.6}",
                        z_stat, p_value
                    ));
                }
            }
            "ChiSqTest" if cmd.args.len() == 2 => {
                let observed = command_result!(parse_data_command_arg(
                    "ChiSqTest",
                    &cmd.args[0],
                    &document.variables,
                ));
                let expected = command_result!(parse_data_command_arg(
                    "ChiSqTest",
                    &cmd.args[1],
                    &document.variables,
                ));
                if let Some((chi2, p_value)) =
                    grafito_geometry::statistics::chi_squared_test(&observed, &expected)
                {
                    command_result!(require_finite_outputs("ChiSqTest", &[chi2, p_value]));
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
                    groups.push(command_result!(parse_data_command_arg(
                        "ANOVA",
                        arg,
                        &document.variables,
                    )));
                }
                let group_refs: Vec<&[f64]> = groups.iter().map(|g| g.as_slice()).collect();
                if let Some((f_stat, p_value)) =
                    grafito_geometry::statistics::anova_one_way(&group_refs)
                {
                    command_result!(require_finite_outputs("ANOVA", &[f_stat, p_value]));
                    input_text.clear();
                    return CommandOutcome::Message(format!(
                        "ANOVA: F = {:.4}, p = {:.6}",
                        f_stat, p_value
                    ));
                }
            }
            "CIMean" if matches!(cmd.args.len(), 1 | 2) => {
                let data = command_result!(parse_data_command_arg(
                    "CIMean",
                    &cmd.args[0],
                    &document.variables,
                ));
                let confidence = command_result!(parse_optional_finite_command_arg(
                    "CIMean",
                    "confianza",
                    &cmd.args,
                    1,
                    0.95,
                    &document.variables,
                ));
                if !(0.0..1.0).contains(&confidence) {
                    return CommandOutcome::Error(
                        "CIMean: la confianza debe estar en (0, 1)".into(),
                    );
                }
                if let Some((lower, mean, upper)) =
                    grafito_geometry::statistics::confidence_interval_mean(&data, confidence)
                {
                    command_result!(require_finite_outputs("CIMean", &[lower, mean, upper]));
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
            "CIProportion" if matches!(cmd.args.len(), 2 | 3) => {
                let successes = match cmd.args[0].trim().parse::<u32>() {
                    Ok(value) => value,
                    Err(_) => {
                        return CommandOutcome::Error(
                            "CIProportion: éxitos debe ser un entero no negativo".into(),
                        )
                    }
                };
                let n = match cmd.args[1].trim().parse::<u32>() {
                    Ok(value) if value > 0 => value,
                    _ => {
                        return CommandOutcome::Error(
                            "CIProportion: n debe ser un entero positivo".into(),
                        )
                    }
                };
                if successes > n {
                    return CommandOutcome::Error("CIProportion: éxitos no puede superar n".into());
                }
                let confidence = command_result!(parse_optional_finite_command_arg(
                    "CIProportion",
                    "confianza",
                    &cmd.args,
                    2,
                    0.95,
                    &document.variables,
                ));
                if !(0.0..1.0).contains(&confidence) {
                    return CommandOutcome::Error(
                        "CIProportion: la confianza debe estar en (0, 1)".into(),
                    );
                }
                if let Some((lower, p_hat, upper)) =
                    grafito_geometry::statistics::confidence_interval_proportion(
                        successes, n, confidence,
                    )
                {
                    command_result!(require_finite_outputs(
                        "CIProportion",
                        &[lower, p_hat, upper],
                    ));
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
                let (x_min, x_max, y_min, y_max) = command_result!(parse_rect_bounds(
                    "ComplexGrid",
                    &cmd.args,
                    &document.variables,
                    (-5.0, 5.0, -5.0, 5.0),
                ));
                let density = match cmd.args.get(5) {
                    Some(value) => match value.trim().parse::<usize>() {
                        Ok(value) if (2..=128).contains(&value) => value,
                        _ => {
                            return CommandOutcome::Error(
                                "ComplexGrid: la densidad debe estar entre 2 y 128".into(),
                            )
                        }
                    },
                    None => 10,
                };
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
                if grafito_complex::complex_expr::parse(expr).is_err() {
                    return CommandOutcome::Error(
                        "ComplexGrid: la expresión compleja no es válida".into(),
                    );
                }
                cg.expr = expr.to_string();
                insert_command_object!(document, GeoObject::ComplexGrid(cg));
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
                let resolved = if resolved.is_none() && base_label == "I" {
                    Some(insert_command_object!(
                        document,
                        GeoObject::ImplicitCurve(
                            ImplicitCurveObj::new("x^2 + y^2", "1", RelationOperator::Less)
                                .with_label("I")
                        )
                    ))
                } else {
                    resolved
                };
                match resolved {
                    Some(id) => {
                        let Some(target) = document.get_object(id) else {
                            return CommandOutcome::Error(format!(
                                "ComplexMapping: objeto '{target_label}' no encontrado"
                            ));
                        };
                        if !complex_mapping_target_is_supported(target) {
                            return CommandOutcome::Error(format!(
                                "ComplexMapping: '{}' no tiene una representación compleja 2D mapeable",
                                target_label
                            ));
                        }
                        let cm = ComplexMappingObj::new_with_symbol(
                            expr,
                            id,
                            document.complex_base_symbol.as_str(),
                        );
                        insert_command_object!(document, GeoObject::ComplexMapping(cm));
                        input_text.clear();
                        return CommandOutcome::Message(format!(
                            "ComplexMapping: {expr} sobre {target_label}"
                        ));
                    }
                    None => {
                        return CommandOutcome::Error(format!(
                            "ComplexMapping: objeto '{target_label}' no encontrado"
                        ));
                    }
                }
            }
            "ComplexIntegral" | "Gauss" if cmd.args.len() == 2 => {
                let expr = cmd.args[0].trim();
                let target_label = cmd.args[1].trim();

                let is_gauss = cmd.command.eq_ignore_ascii_case("Gauss");

                if let Some(target_id) = find_object_by_label(document, target_label) {
                    let integral = ComplexIntegralObj::new(expr, target_id, is_gauss);
                    insert_command_object!(document, GeoObject::ComplexIntegral(integral));
                    input_text.clear();
                    if is_gauss {
                        return CommandOutcome::Message(format!(
                            "Gauss (Residuos): {} sobre {}",
                            expr, target_label
                        ));
                    } else {
                        return CommandOutcome::Message(format!(
                            "Integral de Contorno: {} sobre {}",
                            expr, target_label
                        ));
                    }
                } else {
                    return CommandOutcome::Error(format!(
                        "Integral: objeto '{}' no encontrado",
                        target_label
                    ));
                }
            }
            "DomainColoring" if !cmd.args.is_empty() => {
                let (x_min, x_max, y_min, y_max) = command_result!(parse_rect_bounds(
                    "DomainColoring",
                    &cmd.args,
                    &document.variables,
                    (-5.0, 5.0, -5.0, 5.0),
                ));
                let res = match cmd.args.get(5) {
                    Some(value) => match value.trim().parse::<usize>() {
                        Ok(value) if (16..=300).contains(&value) => value,
                        _ => {
                            return CommandOutcome::Error(
                                "DomainColoring: la resolución debe estar entre 16 y 300".into(),
                            )
                        }
                    },
                    None => 200,
                };
                let expr = cmd.args[0].trim();
                let expr = expr.strip_prefix("f(z)=").unwrap_or(expr);
                let expr = expr.strip_prefix("w=").unwrap_or(expr);
                if grafito_complex::complex_expr::parse(expr).is_err() {
                    return CommandOutcome::Error(
                        "DomainColoring: la expresión compleja no es válida".into(),
                    );
                }
                let cg = ComplexGridObj::new(expr, x_min, x_max, y_min, y_max).as_domain_coloring();
                let mut cg2 = cg;
                cg2.density = res;
                insert_command_object!(document, GeoObject::ComplexGrid(cg2));
                input_text.clear();
                return CommandOutcome::Message(format!(
                    "Domain coloring ({}x{}) created",
                    res, res
                ));
            }
            "HeatMap" if !cmd.args.is_empty() => {
                let (x_min, x_max, y_min, y_max) = command_result!(parse_rect_bounds(
                    "HeatMap",
                    &cmd.args,
                    &document.variables,
                    (-5.0, 5.0, -5.0, 5.0),
                ));
                let res = match cmd.args.get(5) {
                    Some(value) => match value.trim().parse::<usize>() {
                        Ok(value) if (16..=300).contains(&value) => value,
                        _ => {
                            return CommandOutcome::Error(
                                "HeatMap: la resolución debe estar entre 16 y 300".into(),
                            )
                        }
                    },
                    None => 150,
                };
                let expr = cmd.args[0].trim();
                let expr = expr.strip_prefix("f(x,y)=").unwrap_or(expr);
                let expr = expr.strip_prefix("z=").unwrap_or(expr);
                let cg = ComplexGridObj::new(expr, x_min, x_max, y_min, y_max).as_heat_map();
                let mut cg2 = cg;
                cg2.density = res;
                insert_command_object!(document, GeoObject::ComplexGrid(cg2));
                input_text.clear();
                return CommandOutcome::Message(format!("Heat map ({}x{}) created", res, res));
            }
            "ComplexSymbol" if !cmd.args.is_empty() => {
                let new_sym = cmd.args[0].trim();
                if new_sym.is_empty() {
                    return CommandOutcome::Error("Símbolo vacío".into());
                }
                document.migrate_complex_symbol(new_sym);
                input_text.clear();
                return CommandOutcome::Message(format!("Símbolo base cambiado a '{}'", new_sym));
            }
            "ComplexSurface" if !cmd.args.is_empty() => {
                let (x_min, x_max, y_min, y_max) = command_result!(parse_rect_bounds(
                    "ComplexSurface",
                    &cmd.args,
                    &document.variables,
                    (-3.0, 3.0, -3.0, 3.0),
                ));
                let mesh_res = match cmd.args.get(5) {
                    Some(value) => match value.trim().parse::<usize>() {
                        Ok(value) if (16..=100).contains(&value) => value,
                        _ => {
                            return CommandOutcome::Error(
                                "ComplexSurface: la resolución debe estar entre 16 y 100".into(),
                            )
                        }
                    },
                    None => 40,
                };
                let expr = cmd.args[0].trim();
                let expr = expr.strip_prefix("f(z)=").unwrap_or(expr);
                let expr = expr.strip_prefix("w=").unwrap_or(expr);
                if grafito_complex::complex_expr::parse(expr).is_err() {
                    return CommandOutcome::Error(
                        "ComplexSurface: la expresión compleja no es válida".into(),
                    );
                }
                let mut surf = grafito_core::object::Surface3DObj::new_complex(
                    expr,
                    (x_min, x_max),
                    (y_min, y_max),
                );
                surf.mesh_res = mesh_res;
                surf.color = grafito_geometry::Color::new(0.4, 0.6, 1.0, 1.0);
                insert_command_object!(document, GeoObject::Surface3D(surf));
                input_text.clear();
                return CommandOutcome::Message(format!(
                    "ComplexSurface |{}| [{}..{}]×[{}..{}], res={}",
                    expr, x_min, x_max, y_min, y_max, mesh_res
                ));
            }
            "Quadrants" => {
                let x_min = command_result!(parse_optional_finite_command_arg(
                    "Quadrants",
                    "x_min",
                    &cmd.args,
                    0,
                    -5.0,
                    &document.variables,
                ));
                let x_max = command_result!(parse_optional_finite_command_arg(
                    "Quadrants",
                    "x_max",
                    &cmd.args,
                    1,
                    5.0,
                    &document.variables,
                ));
                let y_min = command_result!(parse_optional_finite_command_arg(
                    "Quadrants",
                    "y_min",
                    &cmd.args,
                    2,
                    -5.0,
                    &document.variables,
                ));
                let y_max = command_result!(parse_optional_finite_command_arg(
                    "Quadrants",
                    "y_max",
                    &cmd.args,
                    3,
                    5.0,
                    &document.variables,
                ));
                if x_min >= x_max || y_min >= y_max {
                    return CommandOutcome::Error(
                        "Quadrants: se requiere x_min < x_max e y_min < y_max".into(),
                    );
                }
                let mut cg = ComplexGridObj::new("z", x_min, x_max, y_min, y_max);
                cg.render_mode = 4;
                cg.density = 20;
                insert_command_object!(document, GeoObject::ComplexGrid(cg));
                input_text.clear();
                return CommandOutcome::Message("Cuadrantes del plano complejo creados".into());
            }
            "PolarCurve" if cmd.args.len() >= 3 => {
                let expr = cmd.args[0].trim();
                let t_min = command_result!(parse_finite_command_arg(
                    "PolarCurve",
                    "t_min",
                    &cmd.args[1],
                    &document.variables,
                ));
                let t_max = command_result!(parse_finite_command_arg(
                    "PolarCurve",
                    "t_max",
                    &cmd.args[2],
                    &document.variables,
                ));
                command_result!(require_ordered_domain(
                    "PolarCurve",
                    "t_min",
                    "t_max",
                    t_min,
                    t_max,
                ));
                let obj = GeoObject::PolarCurve(PolarCurveObj::new(expr, t_min, t_max));
                insert_command_object!(document, obj);
                input_text.clear();
                return CommandOutcome::Message(format!(
                    "Polar curve r = {} [{}..{}]",
                    expr, t_min, t_max
                ));
            }
            "ParametricCurve2D" if cmd.args.len() >= 4 => {
                let expr_x = cmd.args[0].trim();
                let expr_y = cmd.args[1].trim();
                let t_min = command_result!(parse_finite_command_arg(
                    "ParametricCurve2D",
                    "t_min",
                    &cmd.args[2],
                    &document.variables,
                ));
                let t_max = command_result!(parse_finite_command_arg(
                    "ParametricCurve2D",
                    "t_max",
                    &cmd.args[3],
                    &document.variables,
                ));
                command_result!(require_ordered_domain(
                    "ParametricCurve2D",
                    "t_min",
                    "t_max",
                    t_min,
                    t_max,
                ));
                let obj = GeoObject::ParametricCurve2D(ParametricCurve2DObj::new(
                    expr_x, expr_y, t_min, t_max,
                ));
                insert_command_object!(document, obj);
                input_text.clear();
                return CommandOutcome::Message("Parametric curve created".into());
            }
            "Function" if !cmd.args.is_empty() => {
                let expr = cmd.args[0].trim();
                if expr.is_empty() {
                    input_text.clear();
                    return CommandOutcome::Error("Function: se requiere una expresión".into());
                }
                if let Ok(ast) = prepare_function_ast(expr, &HashMap::new(), &["x"]) {
                    if let Err(error) = ast.validate_static_bessel_orders() {
                        return CommandOutcome::Error(format!("Function: {error}"));
                    }
                }
                let label = next_function_label(document);
                insert_command_object!(
                    document,
                    GeoObject::Function(FunctionObj::new(expr).with_label(&label),)
                );
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
                insert_command_object!(
                    document,
                    GeoObject::Function(FunctionObj::new(&expr).with_label(&label),)
                );
                input_text.clear();
                return CommandOutcome::Message(format!("Piecewise function → {}", label));
            }
            "VectorField2D" if cmd.args.len() >= 2 => {
                let obj = GeoObject::VectorField2D(VectorField2DObj::new(
                    cmd.args[0].trim(),
                    cmd.args[1].trim(),
                ));
                insert_command_object!(document, obj);
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
                insert_command_object!(document, GeoObject::PhasePortrait(pp));
                input_text.clear();
                return CommandOutcome::Message("Phase portrait created".into());
            }
            "Contour" if cmd.args.len() >= 6 => {
                let expr = cmd.args[0].trim();
                let _bounds = command_result!(parse_rect_bounds(
                    "Contour",
                    &cmd.args,
                    &document.variables,
                    (-5.0, 5.0, -5.0, 5.0),
                ));
                let mut levels = Vec::with_capacity(cmd.args.len() - 5);
                for level in &cmd.args[5..] {
                    let value = command_result!(require_finite(parse_numeric_arg(
                        level,
                        &document.variables,
                    ))
                    .map_err(|_| {
                        CommandOutcome::Error(
                            "Contour: cada nivel debe ser un número finito.".into(),
                        )
                    }));
                    levels.push(value);
                }
                command_result!(validate_contour_levels(&levels)
                    .map_err(|error| { CommandOutcome::Error(format!("Contour: {error}")) }));
                // Split LHS/RHS using relation-aware splitting
                let (lhs, rhs, op) = split_relation(expr);
                let mut obj = ImplicitCurveObj::new(lhs, rhs, op);
                obj.label = next_implicit_label(document);
                obj.contour_levels = Some(levels);
                insert_command_object!(document, GeoObject::ImplicitCurve(obj));
                input_text.clear();
                return CommandOutcome::Message("Contour curves created".into());
            }
            "ImplicitCurve" if !cmd.args.is_empty() => {
                let (lhs, rhs, op) = match cmd.args.as_slice() {
                    [expression] => split_relation(expression.trim()),
                    [lhs, rhs, relation] => {
                        let operator = match relation.trim() {
                            "=" | "==" => RelationOperator::Eq,
                            "<" => RelationOperator::Less,
                            "<=" => RelationOperator::LessEq,
                            ">" => RelationOperator::Greater,
                            ">=" => RelationOperator::GreaterEq,
                            _ => {
                                return CommandOutcome::Error(
                                    "ImplicitCurve: relación inválida; usa =, <, <=, > o >=".into(),
                                )
                            }
                        };
                        (lhs.trim(), rhs.trim(), operator)
                    }
                    _ => {
                        return CommandOutcome::Error(
                            "ImplicitCurve: usa una ecuación o lhs, rhs y relación".into(),
                        )
                    }
                };
                let mut obj = ImplicitCurveObj::new(lhs, rhs, op);
                obj.label = next_implicit_label(document);
                insert_command_object!(document, GeoObject::ImplicitCurve(obj));
                input_text.clear();
                return CommandOutcome::Message("Implicit curve created".into());
            }
            _ => {}
        }
        result = match execute_cas_command_typed(document, &cmd) {
            Some(Ok(message)) => CommandOutcome::Message(message),
            Some(Err(message)) => CommandOutcome::Error(message),
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
                if let Err(error) = document.try_set_variable(name.to_string(), val) {
                    return CommandOutcome::Error(format!("Assignment: {error}"));
                }
                input_text.clear();
                return CommandOutcome::Ok;
            }
        }
        if is_function_lhs(name) {
            let label = name
                .split_once('(')
                .map(|(id, _)| id.trim())
                .unwrap_or(name);
            match document.try_find_object_by_label(label) {
                Ok(Some(id)) => {
                    document.remove_object(id);
                }
                Ok(None) => {}
                Err(error) => return CommandOutcome::Error(format!("Function: {error}")),
            }
            let final_expr = expand_all_cas(rest, document);
            let obj = GeoObject::Function(FunctionObj::new(&final_expr).with_label(label));
            insert_command_object!(document, obj);
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
                    insert_command_object!(document, obj);
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
                    insert_command_object!(document, obj);
                    input_text.clear();
                    return CommandOutcome::Ok;
                }
            }
        }

        if name == "y" {
            let label = next_function_label(document);
            let obj = GeoObject::Function(FunctionObj::new(rest).with_label(&label));
            insert_command_object!(document, obj);
            input_text.clear();
            return CommandOutcome::Ok;
        }

        // Polar curve: r = f(theta) or r(theta) = f(theta)
        if name == "r" || name == "r(θ)" || name == "r(t)" || name == "r(theta)" {
            let t_min = 0.0;
            let t_max = 2.0 * std::f64::consts::PI;
            let obj = GeoObject::PolarCurve(PolarCurveObj::new(rest, t_min, t_max));
            insert_command_object!(document, obj);
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
                    insert_command_object!(document, obj);
                    input_text.clear();
                    return CommandOutcome::Ok;
                }
            }
        }

        if rest == "y" {
            let label = next_function_label(document);
            let obj = GeoObject::Function(FunctionObj::new(name).with_label(&label));
            insert_command_object!(document, obj);
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
                    if let Err(error) = validate_contour_levels(&levels) {
                        return CommandOutcome::Error(format!("Contour: {error}"));
                    }
                    let mut obj = ImplicitCurveObj::new(name, "0", RelationOperator::Eq);
                    obj.label = next_implicit_label(document);
                    obj.contour_levels = Some(levels);
                    insert_command_object!(document, GeoObject::ImplicitCurve(obj));
                    input_text.clear();
                    return CommandOutcome::Ok;
                }
            }
        }

        let mut obj = ImplicitCurveObj::new(name, rest, RelationOperator::Eq);
        obj.label = next_implicit_label(document);
        insert_command_object!(document, GeoObject::ImplicitCurve(obj));
        input_text.clear();
        return CommandOutcome::Ok;
    } else if let Some((lhs, rhs)) = text.split_once("<=") {
        let mut obj = ImplicitCurveObj::new(lhs.trim(), rhs.trim(), RelationOperator::LessEq);
        obj.label = next_implicit_label(document);
        insert_command_object!(document, GeoObject::ImplicitCurve(obj));
        input_text.clear();
        return CommandOutcome::Ok;
    } else if let Some((lhs, rhs)) = text.split_once(">=") {
        let mut obj = ImplicitCurveObj::new(lhs.trim(), rhs.trim(), RelationOperator::GreaterEq);
        obj.label = next_implicit_label(document);
        insert_command_object!(document, GeoObject::ImplicitCurve(obj));
        input_text.clear();
        return CommandOutcome::Ok;
    } else if let Some((lhs, rhs)) = text.split_once('<') {
        let mut obj = ImplicitCurveObj::new(lhs.trim(), rhs.trim(), RelationOperator::Less);
        obj.label = next_implicit_label(document);
        insert_command_object!(document, GeoObject::ImplicitCurve(obj));
        input_text.clear();
        return CommandOutcome::Ok;
    } else if let Some((lhs, rhs)) = text.split_once('>') {
        let mut obj = ImplicitCurveObj::new(lhs.trim(), rhs.trim(), RelationOperator::Greater);
        obj.label = next_implicit_label(document);
        insert_command_object!(document, GeoObject::ImplicitCurve(obj));
        input_text.clear();
        return CommandOutcome::Ok;
    } else {
        let vars_vec: Vec<(String, f64)> = document
            .variables
            .iter()
            .map(|(k, v)| (k.clone(), *v))
            .collect();
        let z_char = document.complex_base_symbol.chars().next().unwrap_or('z');
        if contains_var(text, 'x') {
            let label = next_function_label(document);
            let obj = GeoObject::Function(FunctionObj::new(text).with_label(label));
            insert_command_object!(document, obj);
            input_text.clear();
            return CommandOutcome::Ok;
        } else if contains_var(text, z_char)
            || text.contains("deriv_z")
            || text.contains("deriv_z_conj")
        {
            let cg = ComplexGridObj::new(text, -2.0, 2.0, -2.0, 2.0).as_domain_coloring();
            insert_command_object!(document, GeoObject::ComplexGrid(cg));
            input_text.clear();
            return CommandOutcome::Ok;
        } else if let Ok(val) = evaluate(text, &vars_vec) {
            let mut name = String::new();
            for c in b'a'..=b'z' {
                let letter = (c as char).to_string();
                if !document.variables.contains_key(&letter)
                    && document.object_ids_by_label(&letter).is_empty()
                {
                    name = letter;
                    break;
                }
            }
            if !name.is_empty() {
                if let Err(error) = document.try_set_variable(name.clone(), val) {
                    return CommandOutcome::Error(format!("Variable: {error}"));
                }
                if let Err(error) = document.try_replace_variable_meta_with_previous(
                    &name,
                    grafito_core::VariableMeta {
                        position: grafito_geometry::Point2::new(0.0, 0.0),
                        min: -5.0,
                        max: 5.0,
                        step: 0.1,
                        visible: true,
                        animating: false,
                        animation_speed: 1.0,
                        animation_mode: grafito_core::AnimationMode::PingPong,
                    },
                ) {
                    return CommandOutcome::Error(format!("Variable: {error}"));
                }
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
                    insert_command_object!(document, obj);
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
                    insert_command_object!(document, obj);
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

fn complex_mapping_target_is_supported(target: &GeoObject) -> bool {
    matches!(
        target,
        GeoObject::Point(_)
            | GeoObject::Line(_)
            | GeoObject::Circle(_)
            | GeoObject::Polygon(_)
            | GeoObject::Pencil(_)
            | GeoObject::Function(_)
            | GeoObject::ImplicitCurve(_)
            | GeoObject::ParametricCurve2D(_)
            | GeoObject::PolarCurve(_)
            | GeoObject::Ellipse(_)
            | GeoObject::Parabola(_)
            | GeoObject::Hyperbola(_)
            | GeoObject::RegressionLine(_)
            | GeoObject::VectorField2D(_)
    )
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
                        if let (Ok(a_val), Ok(b_val)) = (
                            require_finite(parse_numeric_arg(a, &document.variables)),
                            require_finite(parse_numeric_arg(b, &document.variables)),
                        ) {
                            resolved_expr =
                                symbolic::integrate_definite(&expr_arg, var, a_val, b_val)
                                    .unwrap_or_else(|_| current[range.clone()].to_string());
                        } else {
                            resolved_expr = current[range.clone()].to_string();
                        }
                    }
                } else {
                    resolved_expr = symbolic::integrate(&expr_arg, var)
                        .unwrap_or_else(|_| current[range.clone()].to_string());
                }
            }
            "Taylor" => {
                if let (Some(var), Some(center), Some(order)) =
                    (args.get(1), args.get(2), args.get(3))
                {
                    match (
                        is_math_identifier(var),
                        require_finite(parse_numeric_arg(center, &document.variables)),
                        parse_taylor_order(Some(order)),
                    ) {
                        (true, Ok(center), Ok(order)) => {
                            resolved_expr = symbolic::taylor_series(&expr_arg, var, center, order)
                                .unwrap_or_else(|_| current[range.clone()].to_string());
                        }
                        _ => resolved_expr = current[range.clone()].to_string(),
                    }
                } else {
                    resolved_expr = current[range.clone()].to_string();
                }
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
                resolved_expr = match (args.get(1), args.get(2)) {
                    (Some(var), Some(at)) if is_math_identifier(var) => {
                        match require_finite(parse_numeric_arg(at, &document.variables)) {
                            Ok(at) => match symbolic::limit_typed(&expr_arg, var, at) {
                                grafito_geometry::outcome::MathResult::Exact(value)
                                    if value.is_finite() =>
                                {
                                    value.to_string()
                                }
                                grafito_geometry::outcome::MathResult::Approximate {
                                    value,
                                    error_estimate,
                                } if value.is_finite() && error_estimate.is_finite() => {
                                    value.to_string()
                                }
                                _ => current[range.clone()].to_string(),
                            },
                            Err(_) => current[range.clone()].to_string(),
                        }
                    }
                    _ => current[range.clone()].to_string(),
                };
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
        let mut depth = 0usize;
        let mut close = None;
        for (offset, ch) in text[open..].char_indices() {
            match ch {
                '[' => depth += 1,
                ']' => {
                    depth = depth.checked_sub(1)?;
                    if depth == 0 {
                        close = Some(open + offset);
                        break;
                    }
                }
                _ => {}
            }
        }
        let close = close?;
        if !text[close + 1..].trim().is_empty() {
            return None;
        }
        let command = text[..open].trim().to_string();
        let inside = &text[open + 1..close];
        let args: Vec<String> = if inside.trim().is_empty() {
            Vec::new()
        } else {
            split_args(inside)
                .into_iter()
                .map(|s| s.trim().to_string())
                .collect()
        };
        if command.is_empty() {
            return None;
        }
        let normalized = if let Some(normalized) = crate::command_registry::canonicalize(&command) {
            normalized
        } else {
            match command.to_lowercase().as_str() {
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
                "transpose" | "transpuesta" => "Transpose",
                "trace" | "traza" => "Trace",
                "rank" | "rango" | "matrixrank" => "Rank",
                "nullspace" | "null_space" | "kernel" | "nucleo" | "núcleo" => "NullSpace",
                "linearsolve" | "linsolve" | "solvesystem" | "sistema" | "resolver_sistema" => {
                    "LinearSolve"
                }
                "eigenvalues" | "autovalores" => "Eigenvalues",
                "eigenvectors" | "autovectores" => "Eigenvectors",
                "lu" | "ludecomposition" | "lu_decomposition" => "LU",
                "qr" | "qrdecomposition" | "qr_decomposition" => "QR",
                "cholesky" => "Cholesky",
                "svd" | "singularvalues" | "valores_singulares" => "SVD",
                "conditionnumber" | "condition_number" | "condicion" => "ConditionNumber",
                "gaussjordan" | "gauss_jordan" | "jordan" | "reduccionescalonada" => "GaussJordan",
                "gaussjordansolve" | "gauss_jordan_solve" | "resolvergaussjordan" => {
                    "GaussJordanSolve"
                }
                "cramer" | "reglacramer" | "regla_cramer" => "Cramer",
                "cofactor" | "cofactorial" => "Cofactor",
                "adjugate" | "adjunta" => "Adjugate",
                "laplaceexpansion" | "laplace_expansion" | "desarrollolaplace" => {
                    "LaplaceExpansion"
                }
                "changeofbasis" | "change_of_basis" | "cambiobase" | "cambio_base" => {
                    "ChangeOfBasis"
                }
                "lineartransformationmatrix"
                | "linear_transformation_matrix"
                | "matriztransformacion" => "LinearTransformationMatrix",
                "diagonalization" | "diagonalizacion" | "diagonalización" => "Diagonalization",
                "gradient" | "gradiente" | "grad" => "Gradient",
                "jacobianmatrix" | "jacobian" | "jacobiana" | "matrizjacobiana" => "JacobianMatrix",
                "hessian" | "hessiana" => "Hessian",
                "criticalpoints" | "critical_points" | "puntoscriticos" | "puntos_críticos" => {
                    "CriticalPoints"
                }
                "lagrangemultipliers" | "lagrange_multipliers" | "multiplicadoreslagrange" => {
                    "LagrangeMultipliers"
                }
                "directionalderivative" | "directional_derivative" | "derivadadireccional" => {
                    "DirectionalDerivative"
                }
                "tangentplane" | "tangent_plane" | "planotangente" => "TangentPlane",
                "divergence" | "divergencia" => "Divergence",
                "curl" | "rotor" | "rotacional" => "Curl",
                "doubleintegral" | "double_integral" | "integraldoble" => "DoubleIntegral",
                "surfacearea" | "surface_area" | "areasuperficie" => "SurfaceArea",
                "lineintegralscalar" | "line_integral_scalar" | "integrallinealescalar" => {
                    "LineIntegralScalar"
                }
                "lineintegralvector" | "line_integral_vector" | "integrallinealvectorial" => {
                    "LineIntegralVector"
                }
                "tripleintegral" | "triple_integral" | "integraltriple" => "TripleIntegral",
                "surfaceintegralscalar" | "surface_integral_scalar" | "integralsuperficie" => {
                    "SurfaceIntegralScalar"
                }
                "flux" | "flujo" => "Flux",
                "isconservative" | "is_conservative" | "campoconservativo" | "conservativo" => {
                    "IsConservative"
                }
                "potentialfunction" | "potential_function" | "funcionpotencial" | "potencial" => {
                    "PotentialFunction"
                }
                "greentheorem" | "green_theorem" | "teoremagreen" => "GreenTheorem",
                "stokestheorem" | "stokes_theorem" | "teoremastokes" => "StokesTheorem",
                "gaussostrogradski" | "gauss_ostrogradski" | "divergencetheorem" => {
                    "GaussOstrogradski"
                }
                "changeofvariables" | "change_of_variables" | "cambiovariables" => {
                    "ChangeOfVariables"
                }
                "riemannsum" | "riemann_sum" | "sumariemann" => "RiemannSum",
                "improperintegral" | "improper_integral" | "integralimpropia" => "ImproperIntegral",
                "bolzanocheck" | "bolzano" | "teoremabolzano" => "BolzanoCheck",
                "rollecheck" | "rolle" | "teoremarolle" => "RolleCheck",
                "meanvaluecheck" | "mean_value_check" | "lagrangecheck" | "teoremalagrange" => {
                    "MeanValueCheck"
                }
                "cauchymeanvaluecheck" | "cauchy_mean_value_check" | "teoremacauchy" => {
                    "CauchyMeanValueCheck"
                }
                "lhopital" | "l'hopital" | "hopital" => "LHopital",
                "alternatingseriestest" | "alternating_series_test" | "criterioalternada" => {
                    "AlternatingSeriesTest"
                }
                "integraltest" | "integral_test" | "criteriointegral" => "IntegralTest",
                "absoluteconvergence" | "absolute_convergence" | "convergenciaabsoluta" => {
                    "AbsoluteConvergence"
                }
                "sequencelimit" | "sequence_limit" | "limitesucesion" | "limite_sucesion" => {
                    "SequenceLimit"
                }
                "seriessum" | "series_sum" | "sumaserie" | "suma_serie" => "SeriesSum",
                "ratiotest" | "ratio_test" | "cociente" | "criteriocociente" => "RatioTest",
                "roottest" | "root_test" | "criterioraiz" => "RootTest",
                "taylor" => "Taylor",
                "ode" | "edo" => "ODE",
                "odesystem" | "ode_system" | "sistemaedo" | "sistema_edo" => "ODESystem",
                "complexgrid" | "complex_grid" | "cgrid" => "ComplexGrid",
                "complexsurface" | "complex_surface" | "csurface" => "ComplexSurface",
                "quadrants" | "cuadrantes" => "Quadrants",
                "complexmapping"
                | "complex_mapping"
                | "mapeocomplejo"
                | "mapeo_complejo"
                | "transformadacompleja" => "ComplexMapping",
                "integralcompleja" | "contourintegral" | "complexintegral" => "ComplexIntegral",
                "gauss" | "residuos" | "residue" => "Gauss",
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
                "xintercept" | "interceptox" | "intercepto_x" => "XIntercept",
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
                "plane3d" | "plane" | "plano" | "plano3d" => "Plane3D",
                "line3d" | "line3" | "recta3d" | "recta" => "Line3D",
                "equidistantfrom" | "equidistant" | "equidistante" => "EquidistantFrom",
                "solve3dgeometry" | "solve3d" | "resolver3d" => "Solve3DGeometry",
                "intersection3d" | "intersect3d" | "interseccion3d" | "intersección3d" => {
                    "Intersection3D"
                }
                "projection3d" | "project3d" | "proyeccion3d" | "proyección3d" => "Projection3D",
                "planethroughlines" | "planebylines" | "planoporrectas" | "plano_por_rectas" => {
                    "PlaneThroughLines"
                }
                "planethroughlinepoint" | "planoporrectapunto" => "PlaneThroughLinePoint",
                "linerelation3d" | "relacionrectas3d" | "relaciónrectas3d" => "LineRelation3D",
                "solveline3dparameters" | "resolverparametrosrecta3d" | "parametrosrecta3d" => {
                    "SolveLine3DParameters"
                }
                "matrixparamsolve" | "solveparammatrix" | "matrizparametrica" => "MatrixParamSolve",
                "p2dependence" | "p2dep" | "dependenciap2" => "P2Dependence",
                "p2basis" | "basep2" => "P2Basis",
                "p2equations" | "ecuacionesp2" => "P2Equations",
                "subspacedimension" | "subspacedim" | "dimsubspace" | "dimensionsubespacio" => {
                    "SubspaceDimension"
                }
                "subspacebasis" | "basissubspace" | "basesubespacio" => "SubspaceBasis",
                "subspacesum" | "sumsubspaces" | "sumasubespacios" => "SubspaceSum",
                "subspaceintersection" | "intersectionsubspaces" | "interseccionsubespacios" => {
                    "SubspaceIntersection"
                }
                "orthogonalcomplement" | "orthogonal" | "complementoortogonal" | "ortogonal" => {
                    "OrthogonalComplement"
                }
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

fn looks_like_bracketed_command(text: &str) -> bool {
    let Some(open) = text.find('[') else {
        return false;
    };
    let command = text[..open].trim();
    !command.is_empty()
        && command
            .chars()
            .all(|ch| ch.is_alphanumeric() || ch == '_' || ch == '\'')
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

fn execute_cas_command_typed(
    document: &mut Document,
    cmd: &CasCmd,
) -> Option<Result<String, String>> {
    let numeric_variables = document.variables.clone();
    let finite_arg = |index: usize, name: &str| {
        let value = cmd
            .args
            .get(index)
            .ok_or_else(|| format!("falta el argumento {name}"))?;
        require_finite(parse_numeric_arg(value, &numeric_variables))
            .map_err(|error| format!("argumento {name} inválido: {error}"))
    };
    match cmd.command.as_str() {
        "Derivative" => {
            let expr = expand_all_cas(cmd.args.first()?, document);
            let var = cmd.args.get(1).map(|s| s.trim()).unwrap_or("x");
            match symbolic::derivative(&expr, var) {
                Ok(d_expr) => {
                    // Also graph the derivative if it contains the variable
                    if d_expr.contains(var) || d_expr.parse::<f64>().is_ok() {
                        let label = next_function_label(document);
                        insert_typed_command_object!(
                            document,
                            GeoObject::Function(FunctionObj::new(&d_expr).with_label(&label),)
                        );
                        Some(Ok(format!(
                            "d/d{var}({expr}) = {d_expr}  →  Graficado como {label}"
                        )))
                    } else {
                        Some(Ok(format!("d/d{var}({expr}) = {d_expr}")))
                    }
                }
                Err(e) => Some(Err(format!("Error calculando derivada: {}", e))),
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
            if !is_math_identifier(&var) {
                return Some(Err("Error: Integral requiere una variable válida".into()));
            }

            // Check if upper limit is a variable (e.g. Integral[expr, t, 0, x])
            // → graph as f(x) = ∫ₐˣ expr dt
            if let (Some(a_s), Some(b_s)) = (a_str, b_str) {
                let b_trim = b_s.trim();
                if b_trim.len() == 1 && b_trim.chars().all(|c| c.is_alphabetic()) {
                    let lower = match require_finite(parse_numeric_arg(a_s, &document.variables)) {
                        Ok(value) => value,
                        Err(error) => {
                            return Some(Err(format!(
                                "Error en límite inferior de Integral: {error}"
                            )))
                        }
                    };
                    let label = next_function_label(document);
                    let obj = FunctionObj::new(&expr)
                        .with_label(&label)
                        .as_integral(&var, lower);
                    insert_typed_command_object!(document, GeoObject::Function(obj));
                    return Some(Ok(format!(
                        "F({}) = ∫₍{}₎ˣ {} d{} → {}",
                        b_trim, lower, expr, var, label
                    )));
                }
            }

            let label = next_function_label(document);
            insert_typed_command_object!(
                document,
                GeoObject::Function(FunctionObj::new(&expr).with_label(&label),)
            );

            if let (Some(a_s), Some(b_s)) = (a_str, b_str) {
                let a = match require_finite(parse_numeric_arg(a_s, &document.variables)) {
                    Ok(value) => value,
                    Err(error) => {
                        return Some(Err(format!(
                            "Error en límite inferior de Integral: {error}"
                        )))
                    }
                };
                let b = match require_finite(parse_numeric_arg(b_s, &document.variables)) {
                    Ok(value) => value,
                    Err(error) => {
                        return Some(Err(format!(
                            "Error en límite superior de Integral: {error}"
                        )))
                    }
                };

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
                                    return Some(Ok(format!(
                                        "≈ {:.6} (híbrido GPU/CPU) → Graficado como {}",
                                        approx, label
                                    )));
                                }
                            }
                        }
                    }
                }

                match symbolic::integrate_definite(&expr, &var, a, b) {
                    Ok(result) => Some(Ok(format!("{} → Graficado como {}", result, label))),
                    Err(e) => Some(Err(format!("Error calculando integral: {}", e))),
                }
            } else {
                match symbolic::integrate(&expr, &var) {
                    Ok(result) => Some(Ok(format!(
                        "{} → Graficado original como {}",
                        result, label
                    ))),
                    Err(e) => Some(Err(format!("Error calculando integral: {}", e))),
                }
            }
        }
        "Solve" => {
            if cmd.args.is_empty() || cmd.args.len() > 4 {
                return Some(Err(
                    "Error: Solve requiere Solve[expresión, variable?, mínimo?, máximo?]".into(),
                ));
            }
            let expr_raw = expand_all_cas(cmd.args.first()?, document);
            let mut expr_clean = expr_raw.trim().to_string();
            if expr_clean.is_empty() {
                return Some(Err("Error: Solve requiere una expresión no vacía".into()));
            }
            if let Some((lhs, rhs)) = split_on_standalone_eq(&expr_clean) {
                expr_clean = format!("({}) - ({})", lhs, rhs);
            }
            let preprocessed = grafito_geometry::expr::preprocess_expr(&expr_clean);
            let ast = match grafito_geometry::ast::parse_ast(&preprocessed) {
                Ok(ast) => ast,
                Err(error) => {
                    return Some(Err(format!("Error en expresión de Solve: {error}")));
                }
            };
            let var = match cmd.args.get(1) {
                Some(arg) if arg.trim().is_empty() => {
                    return Some(Err("Error: Solve requiere una variable no vacía".into()));
                }
                Some(arg) => arg.trim().to_string(),
                None => {
                    let mut variables = HashSet::new();
                    ast.get_variables(&mut variables);
                    variables.retain(|name| !document.variables.contains_key(name));
                    if variables.len() > 1 {
                        return Some(Err(
                            "Error: Solve requiere indicar la variable cuando hay varias incógnitas"
                                .into(),
                        ));
                    }
                    variables
                        .into_iter()
                        .next()
                        .unwrap_or_else(|| "x".to_string())
                }
            };
            let var = var.as_str();
            if !is_math_identifier(var) {
                return Some(Err(format!(
                    "Error: la variable de Solve no es un identificador válido: '{var}'"
                )));
            }
            let mut unresolved = HashSet::new();
            ast.get_variables(&mut unresolved);
            unresolved.remove(var);
            unresolved.retain(|name| !document.variables.contains_key(name));
            if !unresolved.is_empty() {
                let mut unresolved: Vec<_> = unresolved.into_iter().collect();
                unresolved.sort();
                return Some(Err(format!(
                    "Error: Solve contiene símbolos no definidos: {}",
                    unresolved.join(", ")
                )));
            }
            let parse_bound = |index: usize, default: f64, name: &str| {
                cmd.args.get(index).map_or(Ok(default), |argument| {
                    require_finite(parse_numeric_arg(argument, &document.variables))
                        .map_err(|error| format!("Error en límite {name} de Solve: {error}"))
                })
            };
            let a = match parse_bound(2, -20.0, "inferior") {
                Ok(value) => value,
                Err(error) => return Some(Err(error)),
            };
            let b = match parse_bound(3, 20.0, "superior") {
                Ok(value) => value,
                Err(error) => return Some(Err(error)),
            };
            if a >= b {
                return Some(Err(format!(
                    "Error: el límite inferior de Solve ({a}) debe ser menor que el superior ({b})"
                )));
            }

            let resolved_ast = ast.substitute_vars(&document.variables, &[var]);
            let all_values_are_solutions = symbolic::is_identically_zero(&resolved_ast);

            let graph_expr = replace_variable(&expr_clean, var, "x");
            let label = next_function_label(document);
            insert_typed_command_object!(
                document,
                GeoObject::Function(FunctionObj::new(&graph_expr).with_label(&label),)
            );
            if all_values_are_solutions {
                return Some(Ok(format!(
                    "La ecuación se cumple para todos los valores de {var} → Graficado como {label}"
                )));
            }

            let mut complex_roots_found = false;
            let mut strs = Vec::new();

            if let Some(mut roots) = symbolic::solve_polynomial_complex(&resolved_ast, var) {
                roots.sort_by(|left, right| {
                    left.0
                        .total_cmp(&right.0)
                        .then_with(|| left.1.total_cmp(&right.1))
                });
                roots.dedup_by(|left, right| left.0 == right.0 && left.1 == right.1);
                if !roots.is_empty() {
                    for r in &roots {
                        if !(a..=b).contains(&r.0) {
                            continue;
                        }
                        complex_roots_found = true;
                        if r.1 == 0.0 {
                            strs.push(format!("{var} ≈ {:.6}", r.0));
                            let root_label = unique_object_label(document, "Raíz");
                            insert_typed_command_object!(
                                document,
                                GeoObject::Point(
                                    PointObj::new(Point2::new(r.0, 0.0)).with_label(root_label),
                                )
                            );
                        } else {
                            let sign = if r.1 > 0.0 { "+" } else { "-" };
                            strs.push(format!("{var} ≈ {:.6} {} {:.6}i", r.0, sign, r.1.abs()));
                            let root_label = unique_object_label(document, "Raíz");
                            insert_typed_command_object!(
                                document,
                                GeoObject::Point(
                                    PointObj::new(Point2::new(r.0, r.1)).with_label(root_label),
                                )
                            );
                        }
                    }
                }
            }

            if complex_roots_found {
                return Some(Ok(format!(
                    "{} → Graficado como {}",
                    strs.join(", "),
                    label
                )));
            }

            let roots = symbolic::find_real_roots_numeric(&resolved_ast, var, a, b);
            if roots.is_empty() {
                Some(Ok(format!(
                    "Sin raíces para {} en [{a:.1}, {b:.1}] → Graficado como {label}",
                    var
                )))
            } else {
                let mut strs = Vec::new();
                for r in &roots {
                    strs.push(format!("{var} ≈ {:.6}", r));
                    let root_label = unique_object_label(document, "Raíz");
                    insert_typed_command_object!(
                        document,
                        GeoObject::Point(
                            PointObj::new(Point2::new(*r, 0.0)).with_label(root_label),
                        )
                    );
                }
                Some(Ok(format!(
                    "{} → Graficado como {}",
                    strs.join(", "),
                    label
                )))
            }
        }
        "Taylor" => {
            let expr = expand_all_cas(cmd.args.first()?, document);
            let var = cmd.args.get(1).map(|s| s.trim()).unwrap_or("x");
            if !is_math_identifier(var) {
                return Some(Err("Error: Taylor requiere una variable válida".into()));
            }
            let center = match cmd.args.get(2) {
                Some(value) => {
                    match require_finite(parse_numeric_arg(value, &document.variables)) {
                        Ok(value) => value,
                        Err(error) => {
                            return Some(Err(format!("Error en centro de Taylor: {error}")))
                        }
                    }
                }
                None => 0.0,
            };
            let order = match parse_taylor_order(cmd.args.get(3).map(String::as_str)) {
                Ok(value) => value,
                Err(CommandOutcome::Error(message)) => {
                    return Some(Err(format!("Error: {message}")))
                }
                Err(_) => return Some(Err("Error: Taylor order is invalid".into())),
            };
            match symbolic::taylor_series(&expr, var, center, order) {
                Ok(result) => {
                    let label = next_function_label(document);
                    insert_typed_command_object!(
                        document,
                        GeoObject::Function(FunctionObj::new(&result).with_label(&label),)
                    );
                    Some(Ok(format!("{} → Graficado como {}", result, label)))
                }
                Err(e) => Some(Err(format!("Error: {}", e))),
            }
        }
        "Limit" => {
            if cmd.args.len() != 3 {
                return Some(Err(
                    "Error: Limit requiere Limit[expr, variable, punto]".into()
                ));
            }
            let expr = expand_all_cas(cmd.args.first()?, document);
            if expr.trim().is_empty() {
                return Some(Err("Error: Limit requiere una expresión no vacía".into()));
            }
            let var = cmd.args[1].trim();
            if !is_math_identifier(var) {
                return Some(Err("Error: Limit requiere una variable válida".into()));
            }
            let at = match require_finite(parse_numeric_arg(&cmd.args[2], &document.variables)) {
                Ok(value) => value,
                Err(error) => return Some(Err(format!("Error en punto de Limit: {error}"))),
            };

            match symbolic::limit_typed(&expr, var, at) {
                grafito_geometry::outcome::MathResult::Exact(value) if value.is_finite() => {
                    Some(Ok(format!("lim({var}→{at}) {expr} = {value:.8}")))
                }
                grafito_geometry::outcome::MathResult::Approximate {
                    value,
                    error_estimate,
                } if value.is_finite() && error_estimate.is_finite() => Some(Ok(format!(
                    "lim({var}→{at}) {expr} ≈ {value:.8} (error estimado {error_estimate:.3e})"
                ))),
                _ => Some(Err(
                    "Error: Limit no produjo un valor bilateral finito y confiable".into(),
                )),
            }
        }
        "Factor" => {
            let expr = expand_all_cas(cmd.args.first()?, document);
            match symbolic::factor(&expr, "x") {
                Ok(factors) => Some(Ok(format!("{} = {}", expr, factors))),
                Err(e) => Some(Err(format!("Factor error: {}", e))),
            }
        }
        "Expand" => {
            let expr = expand_all_cas(cmd.args.first()?, document);
            match symbolic::expand(&expr) {
                Ok(expanded) => Some(Ok(format!("{} = {}", expr, expanded))),
                Err(e) => Some(Err(format!("Expand error: {}", e))),
            }
        }
        "Simplify" => {
            let expr = expand_all_cas(cmd.args.first()?, document);
            match symbolic::simplify(&expr) {
                Ok(simplified) => Some(Ok(format!("{} = {}", expr, simplified))),
                Err(e) => Some(Err(format!("Simplify error: {}", e))),
            }
        }
        "TangentAt" => {
            let expr_raw = cmd.args.first()?.trim();
            let x = match finite_arg(1, "x") {
                Ok(value) => value,
                Err(error) => return Some(Err(format!("TangentAt: {error}"))),
            };
            let expr = substitute_document_vars(expr_raw, document);
            match tangent_line_at(&expr, x) {
                Ok((x0, fx, slope)) => {
                    let p1 = Point2::new(x0, fx);
                    let p2 = Point2::new(x0 + 1.0, fx + slope);
                    insert_typed_command_object!(
                        document,
                        GeoObject::Line(LineObj::new(p1, p2).with_label("tangente"))
                    );
                    Some(Ok(format!(
                        "Tangente en x={:.4}: y = {:.4} + {:.4}·(x − {:.4})",
                        x0, fx, slope, x0
                    )))
                }
                Err(e) => Some(Err(format!("Error en TangentAt: {e}"))),
            }
        }
        "NormalAt" => {
            let expr_raw = cmd.args.first()?.trim();
            let x = match finite_arg(1, "x") {
                Ok(value) => value,
                Err(error) => return Some(Err(format!("NormalAt: {error}"))),
            };
            let expr = substitute_document_vars(expr_raw, document);
            match normal_line_at(&expr, x) {
                Ok((x0, fx, normal_slope)) => {
                    let p1 = Point2::new(x0, fx);
                    let p2 = if normal_slope.is_infinite() {
                        Point2::new(x0, fx + 1.0)
                    } else {
                        Point2::new(x0 + 1.0, fx + normal_slope)
                    };
                    insert_typed_command_object!(
                        document,
                        GeoObject::Line(LineObj::new(p1, p2).with_label("normal"))
                    );
                    Some(Ok(format!("Normal en x={:.4}", x0)))
                }
                Err(e) => Some(Err(format!("Error en NormalAt: {e}"))),
            }
        }
        "ArcLength" => {
            let expr_raw = cmd.args.first()?.trim();
            let a = match finite_arg(1, "a") {
                Ok(value) => value,
                Err(error) => return Some(Err(format!("ArcLength: {error}"))),
            };
            let b = match finite_arg(2, "b") {
                Ok(value) => value,
                Err(error) => return Some(Err(format!("ArcLength: {error}"))),
            };
            let expr = substitute_document_vars(expr_raw, document);
            match arc_length(&expr, a, b) {
                Ok(length) => Some(Ok(format!(
                    "Longitud de arco de {:.4} a {:.4}: {:.6}",
                    a, b, length
                ))),
                Err(e) => Some(Err(format!("Error en ArcLength: {e}"))),
            }
        }
        "CurvatureAt" => {
            let expr_raw = cmd.args.first()?.trim();
            let x = match finite_arg(1, "x") {
                Ok(value) => value,
                Err(error) => return Some(Err(format!("CurvatureAt: {error}"))),
            };
            let expr = substitute_document_vars(expr_raw, document);
            match curvature_at(&expr, x) {
                Ok(kappa) => {
                    let radius = if kappa.is_finite() && kappa.abs() > 1e-15 {
                        1.0 / kappa
                    } else {
                        f64::INFINITY
                    };
                    Some(Ok(format!(
                        "Curvatura en x={:.4}: κ = {:.6}, Radio = {:.6}",
                        x, kappa, radius
                    )))
                }
                Err(e) => Some(Err(format!("Error en CurvatureAt: {e}"))),
            }
        }
        "VolumeOfRevolution" => {
            let expr_raw = cmd.args.first()?.trim();
            let a = match finite_arg(1, "a") {
                Ok(value) => value,
                Err(error) => return Some(Err(format!("VolumeOfRevolution: {error}"))),
            };
            let b = match finite_arg(2, "b") {
                Ok(value) => value,
                Err(error) => return Some(Err(format!("VolumeOfRevolution: {error}"))),
            };
            let expr = substitute_document_vars(expr_raw, document);
            match volume_of_revolution(&expr, a, b) {
                Ok(volume) => Some(Ok(format!(
                    "Volumen de revolución de {:.4} a {:.4}: {:.6}",
                    a, b, volume
                ))),
                Err(e) => Some(Err(format!("Error en VolumeOfRevolution: {e}"))),
            }
        }
        "SurfaceOfRevolution" => {
            let expr_raw = cmd.args.first()?.trim();
            let a = match finite_arg(1, "a") {
                Ok(value) => value,
                Err(error) => return Some(Err(format!("SurfaceOfRevolution: {error}"))),
            };
            let b = match finite_arg(2, "b") {
                Ok(value) => value,
                Err(error) => return Some(Err(format!("SurfaceOfRevolution: {error}"))),
            };
            let expr = substitute_document_vars(expr_raw, document);
            match surface_of_revolution(&expr, a, b) {
                Ok(surface) => Some(Ok(format!(
                    "Superficie de revolución de {:.4} a {:.4}: {:.6}",
                    a, b, surface
                ))),
                Err(e) => Some(Err(format!("Error en SurfaceOfRevolution: {e}"))),
            }
        }
        _ => None,
    }
}

pub fn execute_cas_command(document: &mut Document, cmd: &CasCmd) -> Option<String> {
    execute_cas_command_typed(document, cmd)
        .map(|outcome| outcome.unwrap_or_else(|message| message))
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
    document.try_find_object_by_label(label).ok().flatten()
}

/// Busca tres `Point3D` por etiqueta y devuelve sus posiciones.
fn parse_three_point_labels(
    document: &Document,
    args: &[String],
) -> Option<(Point3D, Point3D, Point3D)> {
    let id1 = find_object_by_label(document, &args[0])?;
    let id2 = find_object_by_label(document, &args[1])?;
    let id3 = find_object_by_label(document, &args[2])?;
    let p1 = document.get_object(id1)?;
    let p2 = document.get_object(id2)?;
    let p3 = document.get_object(id3)?;
    match (p1, p2, p3) {
        (GeoObject::Point3D(a), GeoObject::Point3D(b), GeoObject::Point3D(c)) => {
            Some((a.position, b.position, c.position))
        }
        _ => None,
    }
}

fn clean_label(label: &str) -> &str {
    label.trim().trim_matches('"').trim_matches('\'')
}

fn object_by_label_cloned(document: &Document, label: &str) -> Result<GeoObject, String> {
    let label = clean_label(label);
    let Some(id) = find_object_by_label(document, label) else {
        return Err(format!("no existe el objeto '{}'", label));
    };
    document
        .get_object(id)
        .cloned()
        .ok_or_else(|| format!("objeto '{}' inválido", label))
}

fn as_geom_plane(obj: &GeoObject) -> Option<GeomPlane3D> {
    match obj {
        GeoObject::Plane3D(p) => Some(GeomPlane3D::from_equation(p.a, p.b, p.c, p.d)),
        _ => None,
    }
}

fn as_geom_line(obj: &GeoObject) -> Option<GeomLine3D> {
    match obj {
        GeoObject::Line3D(l) => Some(GeomLine3D::from_point_and_direction(l.point, l.direction)),
        _ => None,
    }
}

fn run_intersection_3d(document: &mut Document, a_label: &str, b_label: &str) -> CommandOutcome {
    let a = match object_by_label_cloned(document, a_label) {
        Ok(obj) => obj,
        Err(e) => return CommandOutcome::Error(format!("Intersection3D: {e}")),
    };
    let b = match object_by_label_cloned(document, b_label) {
        Ok(obj) => obj,
        Err(e) => return CommandOutcome::Error(format!("Intersection3D: {e}")),
    };
    let eps = 1e-9;

    match (&a, &b) {
        (GeoObject::Plane3D(_), GeoObject::Plane3D(_)) => {
            let Some(p1) = as_geom_plane(&a) else {
                return CommandOutcome::Error("Intersection3D: plano degenerado".into());
            };
            let Some(p2) = as_geom_plane(&b) else {
                return CommandOutcome::Error("Intersection3D: plano degenerado".into());
            };
            match intersect_planes(p1, p2, eps) {
                PlanePlaneIntersection::Line(line) => {
                    let id = insert_command_object!(
                        document,
                        GeoObject::Line3D(Line3DObj::from_point_and_direction(
                            line.point,
                            line.direction
                        ),)
                    );
                    let label = document
                        .get_object(id)
                        .map(|o| o.label().to_string())
                        .unwrap_or_default();
                    CommandOutcome::Message(format!(
                        "Intersection3D: recta {} + t{} → {}",
                        fmt_point3(line.point),
                        fmt_point3(line.direction),
                        label
                    ))
                }
                PlanePlaneIntersection::ParallelDistinct => {
                    CommandOutcome::Message("Intersection3D: planos paralelos distintos".into())
                }
                PlanePlaneIntersection::Coincident => {
                    CommandOutcome::Message("Intersection3D: planos coincidentes".into())
                }
                PlanePlaneIntersection::Degenerate => {
                    CommandOutcome::Error("Intersection3D: plano degenerado".into())
                }
            }
        }
        (GeoObject::Line3D(_), GeoObject::Plane3D(_))
        | (GeoObject::Plane3D(_), GeoObject::Line3D(_)) => {
            let Some(line) = as_geom_line(&a).or_else(|| as_geom_line(&b)) else {
                return CommandOutcome::Error("Intersection3D: recta degenerada".into());
            };
            let Some(plane) = as_geom_plane(&a).or_else(|| as_geom_plane(&b)) else {
                return CommandOutcome::Error("Intersection3D: plano degenerado".into());
            };
            intersect_line_plane_command(document, line, plane, eps)
        }
        (GeoObject::Line3D(_), GeoObject::Line3D(_)) => {
            let Some(l1) = as_geom_line(&a) else {
                return CommandOutcome::Error("Intersection3D: recta degenerada".into());
            };
            let Some(l2) = as_geom_line(&b) else {
                return CommandOutcome::Error("Intersection3D: recta degenerada".into());
            };
            match line_line_relation(l1, l2, eps) {
                LineLineRelation::Intersecting(p) => {
                    add_point3d_message(document, p, "Intersection3D")
                }
                LineLineRelation::ParallelDistinct => {
                    CommandOutcome::Message("Intersection3D: rectas paralelas distintas".into())
                }
                LineLineRelation::Coincident => {
                    CommandOutcome::Message("Intersection3D: rectas coincidentes".into())
                }
                LineLineRelation::Skew { distance, .. } => CommandOutcome::Message(format!(
                    "Intersection3D: rectas alabeadas; distancia mínima ≈ {:.10}",
                    distance
                )),
                LineLineRelation::Degenerate => {
                    CommandOutcome::Error("Intersection3D: recta degenerada".into())
                }
            }
        }
        _ => CommandOutcome::Error(
            "Intersection3D: soporta Plano-Plano, Recta-Plano o Recta-Recta".into(),
        ),
    }
}

fn run_three_plane_intersection(
    document: &mut Document,
    a_label: &str,
    b_label: &str,
    c_label: &str,
) -> CommandOutcome {
    let labels = [a_label, b_label, c_label];
    let mut planes = Vec::new();
    for label in labels {
        let obj = match object_by_label_cloned(document, label) {
            Ok(obj) => obj,
            Err(e) => return CommandOutcome::Error(format!("Intersection3D: {e}")),
        };
        let Some(plane) = as_geom_plane(&obj) else {
            return CommandOutcome::Error("Intersection3D: los 3 objetos deben ser Plane3D".into());
        };
        planes.push(plane);
    }
    let Some(a) = Matrix::from_rows(vec![
        vec![planes[0].a, planes[0].b, planes[0].c],
        vec![planes[1].a, planes[1].b, planes[1].c],
        vec![planes[2].a, planes[2].b, planes[2].c],
    ]) else {
        return CommandOutcome::Error("Intersection3D: plano degenerado".into());
    };
    let Some(b) = Matrix::from_rows(vec![
        vec![-planes[0].d],
        vec![-planes[1].d],
        vec![-planes[2].d],
    ]) else {
        return CommandOutcome::Error("Intersection3D: plano degenerado".into());
    };
    if let Some(sol) = solve_linear_system(&a, &b) {
        add_point3d_message(
            document,
            Point3D::new(sol.get(0, 0), sol.get(1, 0), sol.get(2, 0)),
            "Intersection3D",
        )
    } else {
        CommandOutcome::Message("Intersection3D: los 3 planos no tienen intersección única".into())
    }
}

fn intersect_line_plane_command(
    document: &mut Document,
    line: GeomLine3D,
    plane: GeomPlane3D,
    eps: f64,
) -> CommandOutcome {
    let n = (plane.a, plane.b, plane.c);
    let d = (line.direction.x, line.direction.y, line.direction.z);
    let denom = n.0 * d.0 + n.1 * d.1 + n.2 * d.2;
    let at_point =
        plane.a * line.point.x + plane.b * line.point.y + plane.c * line.point.z + plane.d;
    if denom.abs() <= eps {
        if at_point.abs() <= eps {
            CommandOutcome::Message("Intersection3D: la recta está contenida en el plano".into())
        } else {
            CommandOutcome::Message("Intersection3D: recta paralela al plano".into())
        }
    } else {
        let t = -at_point / denom;
        let p = Point3D::new(
            line.point.x + t * line.direction.x,
            line.point.y + t * line.direction.y,
            line.point.z + t * line.direction.z,
        );
        add_point3d_message(document, p, "Intersection3D")
    }
}

fn run_projection_3d(
    document: &mut Document,
    source_label: &str,
    target_label: &str,
) -> CommandOutcome {
    let source = match object_by_label_cloned(document, source_label) {
        Ok(obj) => obj,
        Err(e) => return CommandOutcome::Error(format!("Projection3D: {e}")),
    };
    let target = match object_by_label_cloned(document, target_label) {
        Ok(obj) => obj,
        Err(e) => return CommandOutcome::Error(format!("Projection3D: {e}")),
    };
    let eps = 1e-9;
    match (&source, &target) {
        (GeoObject::Point3D(p), GeoObject::Plane3D(_)) => {
            let Some(plane) = as_geom_plane(&target) else {
                return CommandOutcome::Error("Projection3D: plano degenerado".into());
            };
            add_point3d_message(document, plane.project_point(p.position), "Projection3D")
        }
        (GeoObject::Point3D(p), GeoObject::Line3D(_)) => {
            let Some(line) = as_geom_line(&target) else {
                return CommandOutcome::Error("Projection3D: recta degenerada".into());
            };
            add_point3d_message(document, line.closest_point_to(p.position), "Projection3D")
        }
        (GeoObject::Line3D(_), GeoObject::Plane3D(_)) => {
            let Some(line) = as_geom_line(&source) else {
                return CommandOutcome::Error("Projection3D: recta degenerada".into());
            };
            let Some(plane) = as_geom_plane(&target) else {
                return CommandOutcome::Error("Projection3D: plano degenerado".into());
            };
            match project_line_onto_plane(line, plane, eps) {
                LineProjectionOnPlane::Line(l) => add_line3d_message(document, l, "Projection3D"),
                LineProjectionOnPlane::Point(p) => add_point3d_message(document, p, "Projection3D"),
                LineProjectionOnPlane::DegenerateLine => {
                    CommandOutcome::Error("Projection3D: recta degenerada".into())
                }
                LineProjectionOnPlane::DegeneratePlane => {
                    CommandOutcome::Error("Projection3D: plano degenerado".into())
                }
            }
        }
        _ => CommandOutcome::Error(
            "Projection3D: soporta Punto→Plano, Punto→Recta y Recta→Plano".into(),
        ),
    }
}

fn run_plane_through_lines(
    document: &mut Document,
    a_label: &str,
    b_label: &str,
) -> CommandOutcome {
    let a = match object_by_label_cloned(document, a_label) {
        Ok(obj) => obj,
        Err(e) => return CommandOutcome::Error(format!("PlaneThroughLines: {e}")),
    };
    let b = match object_by_label_cloned(document, b_label) {
        Ok(obj) => obj,
        Err(e) => return CommandOutcome::Error(format!("PlaneThroughLines: {e}")),
    };
    let (Some(l1), Some(l2)) = (as_geom_line(&a), as_geom_line(&b)) else {
        return CommandOutcome::Error("PlaneThroughLines: ambos objetos deben ser Line3D".into());
    };
    match plane_through_lines(l1, l2, 1e-9) {
        PlaneThroughLines::Plane(p) => add_plane3d_message(document, p, "PlaneThroughLines"),
        PlaneThroughLines::Skew => {
            CommandOutcome::Error("PlaneThroughLines: las rectas son alabeadas".into())
        }
        PlaneThroughLines::CoincidentLines => CommandOutcome::Message(
            "PlaneThroughLines: rectas coincidentes; existen infinitos planos".into(),
        ),
        PlaneThroughLines::DegenerateLine => {
            CommandOutcome::Error("PlaneThroughLines: recta degenerada".into())
        }
    }
}

fn run_plane_through_line_point(
    document: &mut Document,
    line_label: &str,
    point_label: &str,
) -> CommandOutcome {
    let line_obj = match object_by_label_cloned(document, line_label) {
        Ok(obj) => obj,
        Err(e) => return CommandOutcome::Error(format!("PlaneThroughLinePoint: {e}")),
    };
    let point_obj = match object_by_label_cloned(document, point_label) {
        Ok(obj) => obj,
        Err(e) => return CommandOutcome::Error(format!("PlaneThroughLinePoint: {e}")),
    };
    let Some(line) = as_geom_line(&line_obj) else {
        return CommandOutcome::Error(
            "PlaneThroughLinePoint: el primer objeto debe ser Line3D".into(),
        );
    };
    let GeoObject::Point3D(point) = point_obj else {
        return CommandOutcome::Error(
            "PlaneThroughLinePoint: el segundo objeto debe ser Point3D".into(),
        );
    };
    let d = (line.direction.x, line.direction.y, line.direction.z);
    let w = (
        point.position.x - line.point.x,
        point.position.y - line.point.y,
        point.position.z - line.point.z,
    );
    let n = (
        d.1 * w.2 - d.2 * w.1,
        d.2 * w.0 - d.0 * w.2,
        d.0 * w.1 - d.1 * w.0,
    );
    let n_len = (n.0 * n.0 + n.1 * n.1 + n.2 * n.2).sqrt();
    if n_len < 1e-9 {
        return CommandOutcome::Message(
            "PlaneThroughLinePoint: el punto pertenece a la recta; existen infinitos planos".into(),
        );
    }
    add_plane3d_message(
        document,
        GeomPlane3D::from_equation(
            n.0,
            n.1,
            n.2,
            -(n.0 * line.point.x + n.1 * line.point.y + n.2 * line.point.z),
        ),
        "PlaneThroughLinePoint",
    )
}

fn run_line_relation_3d(document: &Document, a_label: &str, b_label: &str) -> CommandOutcome {
    let a = match object_by_label_cloned(document, a_label) {
        Ok(obj) => obj,
        Err(e) => return CommandOutcome::Error(format!("LineRelation3D: {e}")),
    };
    let b = match object_by_label_cloned(document, b_label) {
        Ok(obj) => obj,
        Err(e) => return CommandOutcome::Error(format!("LineRelation3D: {e}")),
    };
    let (Some(l1), Some(l2)) = (as_geom_line(&a), as_geom_line(&b)) else {
        return CommandOutcome::Error("LineRelation3D: ambos objetos deben ser Line3D".into());
    };
    match line_line_relation(l1, l2, 1e-9) {
        LineLineRelation::Intersecting(p) => {
            CommandOutcome::Message(format!("LineRelation3D: se cortan en {}", fmt_point3(p)))
        }
        LineLineRelation::ParallelDistinct => {
            CommandOutcome::Message("LineRelation3D: paralelas distintas".into())
        }
        LineLineRelation::Coincident => {
            CommandOutcome::Message("LineRelation3D: coincidentes".into())
        }
        LineLineRelation::Skew {
            closest_on_first,
            closest_on_second,
            distance,
        } => CommandOutcome::Message(format!(
            "LineRelation3D: alabeadas; distancia mínima ≈ {:.10}; puntos más cercanos {} y {}",
            distance,
            fmt_point3(closest_on_first),
            fmt_point3(closest_on_second)
        )),
        LineLineRelation::Degenerate => {
            CommandOutcome::Error("LineRelation3D: recta degenerada".into())
        }
    }
}

fn add_point3d_message(document: &mut Document, p: Point3D, command: &str) -> CommandOutcome {
    let id = insert_command_object!(document, GeoObject::Point3D(Point3DObj::new(p)));
    let label = document
        .get_object(id)
        .map(|o| o.label().to_string())
        .unwrap_or_default();
    CommandOutcome::Message(format!("{command}: punto {} → {label}", fmt_point3(p)))
}

fn add_line3d_message(document: &mut Document, line: GeomLine3D, command: &str) -> CommandOutcome {
    let id = insert_command_object!(
        document,
        GeoObject::Line3D(Line3DObj::from_point_and_direction(
            line.point,
            line.direction,
        ))
    );
    let label = document
        .get_object(id)
        .map(|o| o.label().to_string())
        .unwrap_or_default();
    CommandOutcome::Message(format!(
        "{command}: recta {} + t{} → {label}",
        fmt_point3(line.point),
        fmt_point3(line.direction)
    ))
}

fn add_plane3d_message(
    document: &mut Document,
    plane: GeomPlane3D,
    command: &str,
) -> CommandOutcome {
    let id = insert_command_object!(
        document,
        GeoObject::Plane3D(Plane3DObj::from_equation(
            plane.a, plane.b, plane.c, plane.d,
        ))
    );
    let label = document
        .get_object(id)
        .map(|o| o.label().to_string())
        .unwrap_or_default();
    CommandOutcome::Message(format!(
        "{command}: {:.10}x + {:.10}y + {:.10}z + {:.10} = 0 → {label}",
        plane.a, plane.b, plane.c, plane.d
    ))
}

fn fmt_point3(p: Point3D) -> String {
    format!("({:.10}, {:.10}, {:.10})", p.x, p.y, p.z)
}

#[derive(Debug, Clone, Copy)]
enum Axis3D {
    X,
    Y,
    Z,
}

impl Axis3D {
    fn parse(text: &str) -> Option<Self> {
        let t = text
            .trim()
            .trim_matches('"')
            .trim_matches('\'')
            .to_lowercase()
            .replace('_', "-")
            .replace(' ', "");
        match t.as_str() {
            "x" | "x-axis" | "axis-x" | "ejex" | "eje-x" => Some(Self::X),
            "y" | "y-axis" | "axis-y" | "ejey" | "eje-y" => Some(Self::Y),
            "z" | "z-axis" | "axis-z" | "ejez" | "eje-z" => Some(Self::Z),
            _ => None,
        }
    }

    fn parse_point_constraint(text: &str, var: &str) -> Option<Self> {
        let normalized = text.replace(' ', "");
        let rhs = normalized
            .strip_prefix("P=")
            .or_else(|| normalized.strip_prefix("p="))?;
        let inner = rhs.strip_prefix('(')?.strip_suffix(')')?;
        let parts: Vec<_> = inner.split(',').collect();
        if parts.len() != 3 {
            return None;
        }
        let is_zero = |s: &str| matches!(s, "0" | "0.0" | "+0" | "-0");
        if parts[0] == var && is_zero(parts[1]) && is_zero(parts[2]) {
            Some(Self::X)
        } else if is_zero(parts[0]) && parts[1] == var && is_zero(parts[2]) {
            Some(Self::Y)
        } else if is_zero(parts[0]) && is_zero(parts[1]) && parts[2] == var {
            Some(Self::Z)
        } else {
            None
        }
    }

    fn variable_name(self) -> &'static str {
        match self {
            Self::X => "x",
            Self::Y => "y",
            Self::Z => "z",
        }
    }

    fn point(self, t: f64) -> Point3D {
        match self {
            Self::X => Point3D::new(t, 0.0, 0.0),
            Self::Y => Point3D::new(0.0, t, 0.0),
            Self::Z => Point3D::new(0.0, 0.0, t),
        }
    }
}

fn parse_distance_equality_labels(equation: &str) -> Option<(&str, &str)> {
    let (lhs, rhs) = equation.split_once('=')?;
    Some((parse_dist_label(lhs.trim())?, parse_dist_label(rhs.trim())?))
}

fn parse_dist_label(term: &str) -> Option<&str> {
    let term = term.trim();
    let inner = term.strip_prefix("dist(")?.strip_suffix(')')?;
    let mut parts = inner.split(',').map(str::trim);
    let p = parts.next()?;
    let label = parts.next()?;
    if parts.next().is_some() || !p.eq_ignore_ascii_case("p") || label.is_empty() {
        return None;
    }
    Some(label)
}

fn add_equidistant_solutions(
    document: &mut Document,
    obj_a: &GeoObject,
    obj_b: &GeoObject,
    axis: Axis3D,
) -> CommandOutcome {
    if squared_distance_to_3d_object(axis.point(0.0), obj_a).is_none()
        || squared_distance_to_3d_object(axis.point(0.0), obj_b).is_none()
    {
        return CommandOutcome::Error(
            "EquidistantFrom: solo soporta Point3D, Plane3D y Line3D".into(),
        );
    }

    let f = |t: f64| -> Option<f64> {
        let p = axis.point(t);
        Some(squared_distance_to_3d_object(p, obj_a)? - squared_distance_to_3d_object(p, obj_b)?)
    };
    let roots = find_real_roots_scan(f, -100.0, 100.0, 8000, 1e-10);
    if roots.is_empty() {
        return CommandOutcome::Message(
            "EquidistantFrom: no se encontraron soluciones reales en [-100, 100]".into(),
        );
    }

    let mut parts = Vec::new();
    for root in &roots {
        let p = axis.point(*root);
        insert_command_object!(
            document,
            GeoObject::Point3D(Point3DObj::new(p).with_label("Sol3D"),)
        );
        parts.push(format!("{} ≈ {:.10}", axis.variable_name(), root));
    }
    CommandOutcome::Message(format!("Soluciones equidistantes: {}", parts.join(", ")))
}

fn squared_distance_to_3d_object(p: Point3D, obj: &GeoObject) -> Option<f64> {
    match obj {
        GeoObject::Point3D(point) => {
            let d = p.distance(&point.position);
            Some(d * d)
        }
        GeoObject::Plane3D(plane) => {
            let numerator = plane.a * p.x + plane.b * p.y + plane.c * p.z + plane.d;
            let denom = plane.a * plane.a + plane.b * plane.b + plane.c * plane.c;
            if denom < 1e-15 {
                None
            } else {
                Some(numerator * numerator / denom)
            }
        }
        GeoObject::Line3D(line) => {
            let q = line.point;
            let d = line.direction;
            let pq = (p.x - q.x, p.y - q.y, p.z - q.z);
            let dir_len_sq = d.x * d.x + d.y * d.y + d.z * d.z;
            if dir_len_sq < 1e-15 {
                let dist = p.distance(&q);
                return Some(dist * dist);
            }
            let pq_len_sq = pq.0 * pq.0 + pq.1 * pq.1 + pq.2 * pq.2;
            let dot = pq.0 * d.x + pq.1 * d.y + pq.2 * d.z;
            Some(pq_len_sq - dot * dot / dir_len_sq)
        }
        _ => None,
    }
}

fn find_real_roots_scan<F>(f: F, lo: f64, hi: f64, steps: usize, tol: f64) -> Vec<f64>
where
    F: Fn(f64) -> Option<f64>,
{
    let mut roots = Vec::new();
    let step = (hi - lo) / steps as f64;
    let mut prev_x = lo;
    let mut prev_y = match f(prev_x) {
        Some(v) if v.is_finite() => v,
        _ => f64::NAN,
    };

    for i in 1..=steps {
        let x = lo + i as f64 * step;
        let y = match f(x) {
            Some(v) if v.is_finite() => v,
            _ => {
                prev_x = x;
                prev_y = f64::NAN;
                continue;
            }
        };

        if y.abs() < tol {
            push_unique_root(&mut roots, x);
        } else if prev_y.is_finite() && prev_y * y < 0.0 {
            let root = bisect_root(&f, prev_x, x, tol);
            push_unique_root(&mut roots, root);
        }
        prev_x = x;
        prev_y = y;
    }
    roots.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
    roots
}

fn bisect_root<F>(f: &F, mut lo: f64, mut hi: f64, tol: f64) -> f64
where
    F: Fn(f64) -> Option<f64>,
{
    let mut flo = f(lo).unwrap_or(f64::NAN);
    for _ in 0..100 {
        let mid = 0.5 * (lo + hi);
        let fmid = f(mid).unwrap_or(f64::NAN);
        if !fmid.is_finite() || fmid.abs() < tol || (hi - lo).abs() < tol {
            return mid;
        }
        if flo.is_finite() && flo * fmid <= 0.0 {
            hi = mid;
        } else {
            lo = mid;
            flo = fmid;
        }
    }
    0.5 * (lo + hi)
}

fn push_unique_root(roots: &mut Vec<f64>, root: f64) {
    if roots.iter().all(|r| (r - root).abs() > 1e-5) {
        roots.push(root);
    }
}

/// Convierte un `GeoObject` en un [`IntersectionCurve`] cuando el tipo lo
/// admite. Devuelve `None` para tipos no soportados (3D, polígonos, …).
fn object_to_intersection_curve(obj: &GeoObject) -> Option<IntersectionCurve<'_>> {
    match obj {
        GeoObject::Line(l) => Some(IntersectionCurve::Line {
            s: l.start,
            e: l.end,
            kind: l.kind,
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
fn run_animate_command(args: &[String], document: &mut Document) -> CommandOutcome {
    let (name, min, max, speed, mode) = match args {
        [] => (
            "phase".to_string(),
            0.0,
            std::f64::consts::TAU,
            1.0,
            grafito_core::AnimationMode::Loop,
        ),
        [name] => {
            let name = clean_symbol_arg(name);
            if !is_valid_parameter_name(&name) {
                return CommandOutcome::Error(
                    "Animate: la variable debe ser un identificador válido".into(),
                );
            }
            let existing = document.variable_meta(&name);
            let min = existing
                .map(|meta| meta.min)
                .filter(|value| value.is_finite())
                .unwrap_or(-5.0);
            let max = existing
                .map(|meta| meta.max)
                .filter(|value| value.is_finite())
                .unwrap_or(5.0);
            let speed = existing
                .map(|meta| meta.animation_speed)
                .filter(|value| value.is_finite() && *value != 0.0)
                .unwrap_or(1.0);
            let mode = existing.map_or_else(
                || {
                    if name == "phase" {
                        grafito_core::AnimationMode::Loop
                    } else {
                        grafito_core::AnimationMode::PingPong
                    }
                },
                |meta| meta.animation_mode,
            );
            (name, min, max, speed, mode)
        }
        [name, min, max, speed] => {
            let name = clean_symbol_arg(name);
            if !is_valid_parameter_name(&name) {
                return CommandOutcome::Error(
                    "Animate: la variable debe ser un identificador válido".into(),
                );
            }
            let min = command_result!(parse_finite_command_arg(
                "Animate",
                "mínimo",
                min,
                &document.variables,
            ));
            let max = command_result!(parse_finite_command_arg(
                "Animate",
                "máximo",
                max,
                &document.variables,
            ));
            let speed = command_result!(parse_finite_command_arg(
                "Animate",
                "velocidad",
                speed,
                &document.variables,
            ));
            let mode = if name == "phase" {
                grafito_core::AnimationMode::Loop
            } else {
                grafito_core::AnimationMode::PingPong
            };
            (name, min, max, speed, mode)
        }
        _ => {
            return CommandOutcome::Error(
                "Animate: usa Animate[], Animate[variable] o Animate[variable, mínimo, máximo, velocidad]"
                    .into(),
            )
        }
    };

    if speed == 0.0 {
        return CommandOutcome::Error("Animate: la velocidad no puede ser cero".into());
    }
    match document.configure_variable_animation(&name, min, max, speed, mode) {
        Ok(()) => CommandOutcome::Message(format!(
            "Animate: '{}' se anima localmente entre {} y {}.",
            name,
            fmt_scalar(min),
            fmt_scalar(max)
        )),
        Err(error) => CommandOutcome::Error(format!("Animate: {error}")),
    }
}

fn run_generate_animation_command(args: &[String], _document: &mut Document) -> CommandOutcome {
    let template = args
        .first()
        .map(|s| s.trim())
        .filter(|s| !s.is_empty())
        .unwrap_or("derivative-slope");
    let concept = args
        .get(1)
        .map(|s| s.trim())
        .filter(|s| !s.is_empty())
        .unwrap_or("animación didáctica");
    CommandOutcome::Message(format!(
        "GenerateAnimation: plantilla '{template}' para '{concept}' — la animación se genera en segundo plano."
    ))
}

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
                insert_command_object!(document, GeoObject::Point(p));
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

fn add_boolean_result(
    document: &mut Document,
    mp: &geo::MultiPolygon<f64>,
    base_label: &str,
) -> Result<(), String> {
    let polys = grafito_geometry::boolean::multipolygon_to_polygons(mp);
    for (i, verts) in polys.into_iter().enumerate() {
        let label = if i == 0 {
            base_label.to_string()
        } else {
            format!("{}{}", base_label, subscript_label(i))
        };
        let mut poly = PolygonObj::new(verts);
        poly.label = label;
        try_insert_command_object(document, GeoObject::Polygon(poly))?;
    }
    Ok(())
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

fn unique_object_label(document: &Document, base: &str) -> String {
    let candidate = bounded_label_candidate(base, "");
    if document.object_ids_by_label(&candidate).is_empty() {
        return candidate;
    }
    for index in 1..=document.object_count().saturating_add(1) {
        let candidate = bounded_label_candidate(base, &subscript_label(index));
        if document.object_ids_by_label(&candidate).is_empty() {
            return candidate;
        }
    }
    bounded_label_candidate(base, &format!("_{}", document.object_count()))
}

fn bounded_label_candidate(base: &str, suffix: &str) -> String {
    let max_base_len = grafito_core::validation::MAX_STRING_LENGTH.saturating_sub(suffix.len());
    let mut end = base.len().min(max_base_len);
    while !base.is_char_boundary(end) {
        end -= 1;
    }
    format!("{}{suffix}", &base[..end])
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

fn parse_finite_point_arg(
    argument: &str,
    variables: &HashMap<String, f64>,
) -> Result<Point2, String> {
    let argument = argument.trim();
    let inner = argument
        .strip_prefix('(')
        .and_then(|value| value.strip_suffix(')'))
        .ok_or_else(|| "se esperaba un punto (x, y)".to_string())?;
    let components = split_args(inner);
    if components.len() != 2 {
        return Err("se esperaba un punto (x, y)".into());
    }
    let values: Vec<(String, f64)> = variables
        .iter()
        .map(|(name, value)| (name.clone(), *value))
        .collect();
    let x = evaluate(components[0].trim(), &values)
        .map_err(|error| format!("coordenada x inválida: {error}"))?;
    let y = evaluate(components[1].trim(), &values)
        .map_err(|error| format!("coordenada y inválida: {error}"))?;
    if !x.is_finite() || !y.is_finite() {
        return Err("las coordenadas deben ser finitas".into());
    }
    Ok(Point2::new(x, y))
}

fn resolve_point_arg(
    document: &Document,
    argument: &str,
) -> Result<(Point2, Option<ObjectId>), String> {
    if let Some(id) = find_object_by_label(document, argument.trim()) {
        return match document.get_object(id) {
            Some(GeoObject::Point(point)) => Ok((point.position, Some(id))),
            Some(_) => Err(format!("'{}' no es un punto", argument.trim())),
            None => Err(format!("no se encontró '{}'", argument.trim())),
        };
    }
    parse_finite_point_arg(argument, &document.variables).map(|point| (point, None))
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
        let is_extremum = if find_max {
            curr > prev && curr > next
        } else {
            curr < prev && curr < next
        };
        if is_extremum && curr.is_finite() {
            pts.push((x, curr));
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

fn parse_brace_list(s: &str, variables: &HashMap<String, f64>) -> Result<Vec<f64>, String> {
    let s = s.trim();
    let inner = s
        .strip_prefix('{')
        .and_then(|value| value.strip_suffix('}'))
        .ok_or_else(|| "se esperaba una lista con sintaxis {a, b, c}".to_string())?;
    if inner.trim().is_empty() {
        return Ok(Vec::new());
    }
    split_args(inner)
        .into_iter()
        .map(|value| {
            let value = value.trim();
            if value.is_empty() {
                return Err("la lista contiene un valor vacío".into());
            }
            require_finite(parse_numeric_arg(value, variables))
                .map_err(|error| format!("valor inválido '{value}': {error}"))
        })
        .collect()
}

fn parse_matrix_arg_strict(s: &str, variables: &HashMap<String, f64>) -> Result<Matrix, String> {
    let s = s.trim();
    if !s.starts_with('[') || !s.ends_with(']') {
        return Err("se esperaba matriz con sintaxis [[...],[...]]".into());
    }
    let inner = &s[1..s.len() - 1];
    let row_tokens = split_args(inner);
    if row_tokens.is_empty() {
        return Err("matriz vacía".into());
    }
    let mut rows = Vec::with_capacity(row_tokens.len());
    for row_token in row_tokens {
        let row_token = row_token.trim();
        if !row_token.starts_with('[') || !row_token.ends_with(']') {
            return Err(format!("fila inválida '{}': usa [a,b,c]", row_token));
        }
        let row_inner = &row_token[1..row_token.len() - 1];
        let entries = split_args(row_inner);
        if entries.is_empty() {
            return Err("fila vacía".into());
        }
        let mut row = Vec::with_capacity(entries.len());
        for entry in entries {
            let value = parse_numeric_arg(entry.trim(), variables)
                .map_err(|_| format!("entrada numérica inválida '{}'", entry.trim()))?;
            if !value.is_finite() {
                return Err(format!("entrada no finita '{}'", entry.trim()));
            }
            row.push(value);
        }
        rows.push(row);
    }
    Matrix::from_rows(rows).ok_or_else(|| "filas con longitudes incompatibles".into())
}

fn parse_vector_or_matrix_arg(s: &str, variables: &HashMap<String, f64>) -> Result<Matrix, String> {
    let s = s.trim();
    if !s.starts_with('[') || !s.ends_with(']') {
        return Err("se esperaba vector [a,b,c] o matriz [[...]]".into());
    }
    let inner = &s[1..s.len() - 1];
    if inner.trim_start().starts_with('[') {
        parse_matrix_arg_strict(s, variables)
    } else {
        let entries = split_args(inner);
        if entries.is_empty() {
            return Err("vector vacío".into());
        }
        let mut rows = Vec::with_capacity(entries.len());
        for entry in entries {
            let value = parse_numeric_arg(entry.trim(), variables)
                .map_err(|_| format!("entrada numérica inválida '{}'", entry.trim()))?;
            if !value.is_finite() {
                return Err(format!("entrada no finita '{}'", entry.trim()));
            }
            rows.push(vec![value]);
        }
        Matrix::from_rows(rows).ok_or_else(|| "vector inválido".into())
    }
}

fn parse_expression_vector_arg(s: &str) -> Result<Vec<String>, String> {
    let s = s.trim();
    if !s.starts_with('[') || !s.ends_with(']') {
        return Err("se esperaba vector [a,b,c]".into());
    }
    let inner = &s[1..s.len() - 1];
    let entries = split_args(inner);
    if entries.is_empty() {
        return Err("vector vacío".into());
    }
    Ok(entries
        .into_iter()
        .map(|entry| entry.trim().to_string())
        .collect())
}

fn parse_expression_matrix_arg(s: &str) -> Result<Vec<Vec<String>>, String> {
    let s = s.trim();
    if !s.starts_with('[') || !s.ends_with(']') {
        return Err("se esperaba matriz con sintaxis [[...],[...]]".into());
    }
    let inner = &s[1..s.len() - 1];
    let row_tokens = split_args(inner);
    if row_tokens.is_empty() {
        return Err("matriz vacía".into());
    }
    let mut rows: Vec<Vec<String>> = Vec::with_capacity(row_tokens.len());
    for row_token in row_tokens {
        let row_token = row_token.trim();
        if !row_token.starts_with('[') || !row_token.ends_with(']') {
            return Err(format!("fila inválida '{}': usa [a,b,c]", row_token));
        }
        let row_inner = &row_token[1..row_token.len() - 1];
        let entries = split_args(row_inner);
        if entries.is_empty() {
            return Err("fila vacía".into());
        }
        rows.push(
            entries
                .into_iter()
                .map(|entry| entry.trim().to_string())
                .collect(),
        );
    }
    if rows.iter().any(|row| row.len() != rows[0].len()) {
        return Err("filas con longitudes incompatibles".into());
    }
    Ok(rows)
}

fn evaluate_expression_vector(
    entries: &[String],
    variables: &HashMap<String, f64>,
) -> Result<Vec<f64>, String> {
    entries
        .iter()
        .map(|entry| parse_numeric_arg(entry, variables))
        .collect()
}

fn evaluate_expression_matrix(
    rows: &[Vec<String>],
    variables: &HashMap<String, f64>,
) -> Result<Matrix, String> {
    let mut numeric_rows = Vec::with_capacity(rows.len());
    for row in rows {
        let mut numeric_row = Vec::with_capacity(row.len());
        for entry in row {
            numeric_row.push(parse_numeric_arg(entry, variables)?);
        }
        numeric_rows.push(numeric_row);
    }
    Matrix::from_rows(numeric_rows).ok_or_else(|| "matriz inválida".into())
}

fn parse_numeric_vector_arg(s: &str, variables: &HashMap<String, f64>) -> Result<Vec<f64>, String> {
    let entries = parse_expression_vector_arg(s)?;
    let values = evaluate_expression_vector(&entries, variables)?;
    if values.iter().any(|v| !v.is_finite()) {
        return Err("vector con entradas no finitas".into());
    }
    Ok(values)
}

fn eval_multivar_expr(
    expr: &str,
    base_vars: &HashMap<String, f64>,
    assignments: &[(&str, f64)],
) -> Result<f64, String> {
    let mut vars = base_vars.clone();
    for (name, value) in assignments {
        vars.insert((*name).to_string(), *value);
    }
    let bindings = vars.into_iter().collect::<Vec<_>>();
    evaluate(expr, &bindings).and_then(|value| {
        if value.is_finite() {
            Ok(value)
        } else {
            Err(format!("evaluación no finita para '{expr}'"))
        }
    })
}

fn default_multivar_names(n: usize) -> Vec<String> {
    ["x", "y", "z"]
        .iter()
        .take(n)
        .map(|s| (*s).to_string())
        .collect()
}

fn parse_vars_arg(args: &[String], index: usize, fallback_n: usize) -> Result<Vec<String>, String> {
    if let Some(raw) = args.get(index) {
        parse_expression_vector_arg(raw)
            .map(|vars| vars.into_iter().map(|v| clean_symbol_arg(&v)).collect())
    } else {
        Ok(default_multivar_names(fallback_n))
    }
}

fn symbolic_partial(expr: &str, var: &str) -> Result<String, String> {
    symbolic::derivative(expr, var)
}

fn simplified_difference(a: &str, b: &str) -> String {
    let raw = format!("({a}) - ({b})");
    symbolic::simplify(&raw).unwrap_or(raw)
}

fn run_gradient_command(args: &[String], document: &Document) -> CommandOutcome {
    let expr = expand_all_cas(&args[0], document);
    let vars = match parse_vars_arg(args, 1, 2) {
        Ok(v) => v,
        Err(e) => return CommandOutcome::Error(format!("Gradient: {e}")),
    };
    if vars.is_empty() {
        return CommandOutcome::Error("Gradient: se requiere al menos una variable".into());
    }
    let mut parts = Vec::with_capacity(vars.len());
    for var in &vars {
        match symbolic_partial(&expr, var) {
            Ok(d) => parts.push(d),
            Err(e) => return CommandOutcome::Error(format!("Gradient: {e}")),
        }
    }
    CommandOutcome::Message(format!(
        "Gradient = [{}]",
        parts
            .iter()
            .map(|s| s.as_str())
            .collect::<Vec<_>>()
            .join(", ")
    ))
}

fn run_directional_derivative_command(args: &[String], document: &Document) -> CommandOutcome {
    let expr = expand_all_cas(&args[0], document);
    let vars = match parse_vars_arg(args, 1, 2) {
        Ok(v) => v,
        Err(e) => return CommandOutcome::Error(format!("DirectionalDerivative: {e}")),
    };
    let point = match parse_numeric_vector_arg(&args[2], &document.variables) {
        Ok(v) => v,
        Err(e) => return CommandOutcome::Error(format!("DirectionalDerivative: {e}")),
    };
    let direction = match parse_numeric_vector_arg(&args[3], &document.variables) {
        Ok(v) => v,
        Err(e) => return CommandOutcome::Error(format!("DirectionalDerivative: {e}")),
    };
    if vars.len() != point.len() || vars.len() != direction.len() {
        return CommandOutcome::Error(
            "DirectionalDerivative: variables, punto y dirección deben tener la misma dimensión"
                .into(),
        );
    }
    let norm = direction.iter().map(|v| v * v).sum::<f64>().sqrt();
    if norm <= 1e-12 {
        return CommandOutcome::Error("DirectionalDerivative: dirección nula".into());
    }
    let assignments = vars
        .iter()
        .zip(point.iter())
        .map(|(name, value)| (name.as_str(), *value))
        .collect::<Vec<_>>();
    let mut gradient_values = Vec::with_capacity(vars.len());
    for var in &vars {
        let partial = match symbolic_partial(&expr, var) {
            Ok(d) => d,
            Err(e) => return CommandOutcome::Error(format!("DirectionalDerivative: {e}")),
        };
        let value = match eval_multivar_expr(&partial, &document.variables, &assignments) {
            Ok(v) => v,
            Err(e) => return CommandOutcome::Error(format!("DirectionalDerivative: {e}")),
        };
        gradient_values.push(value);
    }
    let unit = direction.iter().map(|v| *v / norm).collect::<Vec<_>>();
    let value = gradient_values
        .iter()
        .zip(unit.iter())
        .map(|(g, u)| g * u)
        .sum::<f64>();
    CommandOutcome::Message(format!(
        "DirectionalDerivative = {} ; grad({}) = {}",
        fmt_scalar(value),
        fmt_vector(&point),
        fmt_vector(&gradient_values)
    ))
}

fn run_tangent_plane_command(args: &[String], document: &Document) -> CommandOutcome {
    let expr = expand_all_cas(&args[0], document);
    let point = match parse_numeric_vector_arg(&args[1], &document.variables) {
        Ok(v) => v,
        Err(e) => return CommandOutcome::Error(format!("TangentPlane: {e}")),
    };
    if point.len() != 2 {
        return CommandOutcome::Error("TangentPlane: el punto debe ser [x0,y0]".into());
    }
    let vars = match parse_vars_arg(args, 2, 2) {
        Ok(v) => v,
        Err(e) => return CommandOutcome::Error(format!("TangentPlane: {e}")),
    };
    if vars.len() != 2 {
        return CommandOutcome::Error("TangentPlane: se requieren dos variables".into());
    }
    let assignments = vec![(vars[0].as_str(), point[0]), (vars[1].as_str(), point[1])];
    let z0 = match eval_multivar_expr(&expr, &document.variables, &assignments) {
        Ok(v) => v,
        Err(e) => return CommandOutcome::Error(format!("TangentPlane: {e}")),
    };
    let fx_expr = match symbolic_partial(&expr, &vars[0]) {
        Ok(d) => d,
        Err(e) => return CommandOutcome::Error(format!("TangentPlane: {e}")),
    };
    let fy_expr = match symbolic_partial(&expr, &vars[1]) {
        Ok(d) => d,
        Err(e) => return CommandOutcome::Error(format!("TangentPlane: {e}")),
    };
    let fx = match eval_multivar_expr(&fx_expr, &document.variables, &assignments) {
        Ok(v) => v,
        Err(e) => return CommandOutcome::Error(format!("TangentPlane: {e}")),
    };
    let fy = match eval_multivar_expr(&fy_expr, &document.variables, &assignments) {
        Ok(v) => v,
        Err(e) => return CommandOutcome::Error(format!("TangentPlane: {e}")),
    };
    CommandOutcome::Message(format!(
        "TangentPlane: z = {} + {}*({}-{}) + {}*({}-{})",
        fmt_scalar(z0),
        fmt_scalar(fx),
        vars[0],
        fmt_scalar(point[0]),
        fmt_scalar(fy),
        vars[1],
        fmt_scalar(point[1])
    ))
}

fn run_divergence_command(args: &[String], document: &Document) -> CommandOutcome {
    let fields = match parse_expression_vector_arg(&args[0]) {
        Ok(v) => v
            .into_iter()
            .map(|e| expand_all_cas(&e, document))
            .collect::<Vec<_>>(),
        Err(e) => return CommandOutcome::Error(format!("Divergence: {e}")),
    };
    let vars = match parse_vars_arg(args, 1, fields.len()) {
        Ok(v) => v,
        Err(e) => return CommandOutcome::Error(format!("Divergence: {e}")),
    };
    if fields.len() != vars.len() || fields.is_empty() {
        return CommandOutcome::Error(
            "Divergence: campo y variables deben tener la misma dimensión".into(),
        );
    }
    let mut terms = Vec::with_capacity(fields.len());
    for (field, var) in fields.iter().zip(vars.iter()) {
        match symbolic_partial(field, var) {
            Ok(d) => terms.push(d),
            Err(e) => return CommandOutcome::Error(format!("Divergence: {e}")),
        }
    }
    let raw = terms.join(" + ");
    let simplified = symbolic::simplify(&raw).unwrap_or(raw);
    CommandOutcome::Message(format!("Divergence = {simplified}"))
}

fn run_curl_command(args: &[String], document: &Document) -> CommandOutcome {
    let fields = match parse_expression_vector_arg(&args[0]) {
        Ok(v) => v
            .into_iter()
            .map(|e| expand_all_cas(&e, document))
            .collect::<Vec<_>>(),
        Err(e) => return CommandOutcome::Error(format!("Curl: {e}")),
    };
    let vars = match parse_vars_arg(args, 1, fields.len()) {
        Ok(v) => v,
        Err(e) => return CommandOutcome::Error(format!("Curl: {e}")),
    };
    if fields.len() == 2 && vars.len() == 2 {
        let dq_dx = match symbolic_partial(&fields[1], &vars[0]) {
            Ok(d) => d,
            Err(e) => return CommandOutcome::Error(format!("Curl: {e}")),
        };
        let dp_dy = match symbolic_partial(&fields[0], &vars[1]) {
            Ok(d) => d,
            Err(e) => return CommandOutcome::Error(format!("Curl: {e}")),
        };
        return CommandOutcome::Message(format!(
            "Curl = {}",
            simplified_difference(&dq_dx, &dp_dy)
        ));
    }
    if fields.len() == 3 && vars.len() == 3 {
        let dr_dy = match symbolic_partial(&fields[2], &vars[1]) {
            Ok(d) => d,
            Err(e) => return CommandOutcome::Error(format!("Curl: {e}")),
        };
        let dq_dz = match symbolic_partial(&fields[1], &vars[2]) {
            Ok(d) => d,
            Err(e) => return CommandOutcome::Error(format!("Curl: {e}")),
        };
        let dp_dz = match symbolic_partial(&fields[0], &vars[2]) {
            Ok(d) => d,
            Err(e) => return CommandOutcome::Error(format!("Curl: {e}")),
        };
        let dr_dx = match symbolic_partial(&fields[2], &vars[0]) {
            Ok(d) => d,
            Err(e) => return CommandOutcome::Error(format!("Curl: {e}")),
        };
        let dq_dx = match symbolic_partial(&fields[1], &vars[0]) {
            Ok(d) => d,
            Err(e) => return CommandOutcome::Error(format!("Curl: {e}")),
        };
        let dp_dy = match symbolic_partial(&fields[0], &vars[1]) {
            Ok(d) => d,
            Err(e) => return CommandOutcome::Error(format!("Curl: {e}")),
        };
        return CommandOutcome::Message(format!(
            "Curl = [{}]",
            [
                simplified_difference(&dr_dy, &dq_dz),
                simplified_difference(&dp_dz, &dr_dx),
                simplified_difference(&dq_dx, &dp_dy),
            ]
            .join(", ")
        ));
    }
    CommandOutcome::Error("Curl: use campo 2D [P,Q] o 3D [P,Q,R]".into())
}

fn run_double_integral_command(
    args: &[String],
    document: &Document,
    surface_area: bool,
) -> CommandOutcome {
    let expr = expand_all_cas(&args[0], document);
    let x_var = clean_symbol_arg(&args[1]);
    let a = match require_finite(parse_numeric_arg(&args[2], &document.variables)) {
        Ok(v) => v,
        Err(e) => return CommandOutcome::Error(format!("DoubleIntegral: {e}")),
    };
    let b = match require_finite(parse_numeric_arg(&args[3], &document.variables)) {
        Ok(v) => v,
        Err(e) => return CommandOutcome::Error(format!("DoubleIntegral: {e}")),
    };
    let y_var = clean_symbol_arg(&args[4]);
    let y_min_expr = args[5].trim();
    let y_max_expr = args[6].trim();
    let n = match parse_quadrature_n(args.get(7), 80, 2, 400) {
        Ok(value) => value,
        Err(error) => return CommandOutcome::Error(format!("DoubleIntegral: {error}")),
    };
    if (b - a).abs() < 1e-12 {
        return CommandOutcome::Error("DoubleIntegral: intervalo exterior degenerado".into());
    }

    let fx_expr;
    let fy_expr;
    if surface_area {
        fx_expr = match symbolic_partial(&expr, &x_var) {
            Ok(d) => d,
            Err(e) => return CommandOutcome::Error(format!("SurfaceArea: {e}")),
        };
        fy_expr = match symbolic_partial(&expr, &y_var) {
            Ok(d) => d,
            Err(e) => return CommandOutcome::Error(format!("SurfaceArea: {e}")),
        };
    } else {
        fx_expr = String::new();
        fy_expr = String::new();
    }

    let dx = (b - a) / n as f64;
    let mut total = 0.0;
    for i in 0..n {
        let x = a + (i as f64 + 0.5) * dx;
        let x_assignment = [(x_var.as_str(), x)];
        let y0 = match eval_multivar_expr(y_min_expr, &document.variables, &x_assignment) {
            Ok(v) => v,
            Err(e) => return CommandOutcome::Error(format!("DoubleIntegral: {e}")),
        };
        let y1 = match eval_multivar_expr(y_max_expr, &document.variables, &x_assignment) {
            Ok(v) => v,
            Err(e) => return CommandOutcome::Error(format!("DoubleIntegral: {e}")),
        };
        let dy = (y1 - y0) / n as f64;
        for j in 0..n {
            let y = y0 + (j as f64 + 0.5) * dy;
            let assignments = [(x_var.as_str(), x), (y_var.as_str(), y)];
            let value = if surface_area {
                let fx = match eval_multivar_expr(&fx_expr, &document.variables, &assignments) {
                    Ok(v) => v,
                    Err(e) => return CommandOutcome::Error(format!("SurfaceArea: {e}")),
                };
                let fy = match eval_multivar_expr(&fy_expr, &document.variables, &assignments) {
                    Ok(v) => v,
                    Err(e) => return CommandOutcome::Error(format!("SurfaceArea: {e}")),
                };
                (1.0 + fx * fx + fy * fy).sqrt()
            } else {
                match eval_multivar_expr(&expr, &document.variables, &assignments) {
                    Ok(v) => v,
                    Err(e) => return CommandOutcome::Error(format!("DoubleIntegral: {e}")),
                }
            };
            total += value * dx * dy;
        }
    }
    if !total.is_finite() {
        return CommandOutcome::Error(if surface_area {
            "SurfaceArea: el resultado no es finito".into()
        } else {
            "DoubleIntegral: el resultado no es finito".into()
        });
    }
    if surface_area {
        CommandOutcome::Message(format!("SurfaceArea ≈ {}", fmt_scalar(total.abs())))
    } else {
        CommandOutcome::Message(format!("DoubleIntegral ≈ {}", fmt_scalar(total)))
    }
}

fn run_jacobian_matrix_command(args: &[String], document: &Document) -> CommandOutcome {
    let fields = match parse_expression_vector_arg(&args[0]) {
        Ok(v) => v
            .into_iter()
            .map(|e| expand_all_cas(&e, document))
            .collect::<Vec<_>>(),
        Err(e) => return CommandOutcome::Error(format!("JacobianMatrix: {e}")),
    };
    let vars = match parse_vars_arg(args, 1, fields.len().max(1)) {
        Ok(v) => v,
        Err(e) => return CommandOutcome::Error(format!("JacobianMatrix: {e}")),
    };
    let mut rows = Vec::with_capacity(fields.len());
    for field in &fields {
        let mut row = Vec::with_capacity(vars.len());
        for var in &vars {
            match symbolic_partial(field, var) {
                Ok(d) => row.push(d),
                Err(e) => return CommandOutcome::Error(format!("JacobianMatrix: {e}")),
            }
        }
        rows.push(row);
    }
    CommandOutcome::Message(format!("JacobianMatrix = {}", fmt_symbolic_matrix(&rows)))
}

fn run_hessian_command(args: &[String], document: &Document) -> CommandOutcome {
    let expr = expand_all_cas(&args[0], document);
    let vars = match parse_vars_arg(args, 1, 2) {
        Ok(v) => v,
        Err(e) => return CommandOutcome::Error(format!("Hessian: {e}")),
    };
    if vars.is_empty() {
        return CommandOutcome::Error("Hessian: se requiere al menos una variable".into());
    }
    let mut rows = Vec::with_capacity(vars.len());
    for row_var in &vars {
        let first = match symbolic_partial(&expr, row_var) {
            Ok(d) => d,
            Err(e) => return CommandOutcome::Error(format!("Hessian: {e}")),
        };
        let mut row = Vec::with_capacity(vars.len());
        for col_var in &vars {
            match symbolic_partial(&first, col_var) {
                Ok(d) => row.push(d),
                Err(e) => return CommandOutcome::Error(format!("Hessian: {e}")),
            }
        }
        rows.push(row);
    }
    CommandOutcome::Message(format!("Hessian = {}", fmt_symbolic_matrix(&rows)))
}

fn run_critical_points_command(args: &[String], document: &Document) -> CommandOutcome {
    let expr = expand_all_cas(&args[0], document);
    let vars = match parse_vars_arg(args, 1, 2) {
        Ok(v) => v,
        Err(e) => return CommandOutcome::Error(format!("CriticalPoints: {e}")),
    };
    if vars.len() != 2 {
        return CommandOutcome::Error("CriticalPoints: se requieren dos variables".into());
    }
    let xmin = match require_finite(parse_numeric_arg(&args[2], &document.variables)) {
        Ok(v) => v,
        Err(e) => return CommandOutcome::Error(format!("CriticalPoints: {e}")),
    };
    let xmax = match require_finite(parse_numeric_arg(&args[3], &document.variables)) {
        Ok(v) => v,
        Err(e) => return CommandOutcome::Error(format!("CriticalPoints: {e}")),
    };
    let ymin = match require_finite(parse_numeric_arg(&args[4], &document.variables)) {
        Ok(v) => v,
        Err(e) => return CommandOutcome::Error(format!("CriticalPoints: {e}")),
    };
    let ymax = match require_finite(parse_numeric_arg(&args[5], &document.variables)) {
        Ok(v) => v,
        Err(e) => return CommandOutcome::Error(format!("CriticalPoints: {e}")),
    };
    let n = match parse_quadrature_n(args.get(6), 25, 3, 80) {
        Ok(value) => value,
        Err(error) => return CommandOutcome::Error(format!("CriticalPoints: {error}")),
    };
    let fx = match symbolic_partial(&expr, &vars[0]) {
        Ok(d) => d,
        Err(e) => return CommandOutcome::Error(format!("CriticalPoints: {e}")),
    };
    let fy = match symbolic_partial(&expr, &vars[1]) {
        Ok(d) => d,
        Err(e) => return CommandOutcome::Error(format!("CriticalPoints: {e}")),
    };
    let fxx = symbolic_partial(&fx, &vars[0]).unwrap_or_else(|_| "0".into());
    let fxy = symbolic_partial(&fx, &vars[1]).unwrap_or_else(|_| "0".into());
    let fyy = symbolic_partial(&fy, &vars[1]).unwrap_or_else(|_| "0".into());

    let mut roots: Vec<(f64, f64)> = Vec::new();
    for i in 0..=n {
        let x = xmin + (xmax - xmin) * i as f64 / n as f64;
        for j in 0..=n {
            let y = ymin + (ymax - ymin) * j as f64 / n as f64;
            if let Some((rx, ry)) = newton2_for_system(&fx, &fy, &vars, [x, y], document) {
                if rx >= xmin - 1e-6
                    && rx <= xmax + 1e-6
                    && ry >= ymin - 1e-6
                    && ry <= ymax + 1e-6
                    && !roots.iter().any(|(px, py)| (px - rx).hypot(py - ry) < 1e-5)
                {
                    roots.push((rx, ry));
                }
            }
        }
    }
    if roots.is_empty() {
        return CommandOutcome::Message("CriticalPoints: none found".into());
    }
    let lines = roots
        .iter()
        .map(|(x, y)| {
            let class = classify_hessian_point(&fxx, &fxy, &fyy, &vars, [*x, *y], document);
            format!("{}: {}", fmt_vector(&[*x, *y]), class)
        })
        .collect::<Vec<_>>()
        .join("; ");
    CommandOutcome::Message(format!("CriticalPoints = {lines}"))
}

fn run_lagrange_multipliers_command(args: &[String], document: &Document) -> CommandOutcome {
    let f = expand_all_cas(&args[0], document);
    let g = expand_all_cas(&args[1], document);
    let vars = match parse_vars_arg(args, 2, 2) {
        Ok(v) => v,
        Err(e) => return CommandOutcome::Error(format!("LagrangeMultipliers: {e}")),
    };
    if vars.len() != 2 {
        return CommandOutcome::Error("LagrangeMultipliers: se requieren dos variables".into());
    }
    let parse_bound = |index: usize, name: &str| {
        require_finite(parse_numeric_arg(&args[index], &document.variables)).map_err(|error| {
            CommandOutcome::Error(format!("LagrangeMultipliers: {name} inválido: {error}"))
        })
    };
    let xmin = command_result!(parse_bound(3, "xmin"));
    let xmax = command_result!(parse_bound(4, "xmax"));
    let ymin = command_result!(parse_bound(5, "ymin"));
    let ymax = command_result!(parse_bound(6, "ymax"));
    let n = match parse_quadrature_n(args.get(7), 21, 5, 60) {
        Ok(value) => value,
        Err(error) => return CommandOutcome::Error(format!("LagrangeMultipliers: {error}")),
    };
    let fx = symbolic_partial(&f, &vars[0]).unwrap_or_else(|_| "0".into());
    let fy = symbolic_partial(&f, &vars[1]).unwrap_or_else(|_| "0".into());
    let gx = symbolic_partial(&g, &vars[0]).unwrap_or_else(|_| "0".into());
    let gy = symbolic_partial(&g, &vars[1]).unwrap_or_else(|_| "0".into());
    let mut sols: Vec<(f64, f64, f64, f64)> = Vec::new();
    let system = Lagrange2System {
        fx: &fx,
        fy: &fy,
        gx: &gx,
        gy: &gy,
        constraint: &g,
        vars: &vars,
        document,
    };
    for i in 0..=n {
        let x = xmin + (xmax - xmin) * i as f64 / n as f64;
        for j in 0..=n {
            let y = ymin + (ymax - ymin) * j as f64 / n as f64;
            if let Some((rx, ry, lambda)) = newton_lagrange2(&system, [x, y, 0.0]) {
                if rx >= xmin - 1e-5
                    && rx <= xmax + 1e-5
                    && ry >= ymin - 1e-5
                    && ry <= ymax + 1e-5
                    && !sols
                        .iter()
                        .any(|(px, py, _, _)| (px - rx).hypot(py - ry) < 1e-5)
                {
                    let value = eval_at_vars(&f, &vars, &[rx, ry], document).unwrap_or(f64::NAN);
                    sols.push((rx, ry, lambda, value));
                }
            }
        }
    }
    if sols.is_empty() {
        return CommandOutcome::Message("LagrangeMultipliers: no candidates found".into());
    }
    let lines = sols
        .iter()
        .map(|(x, y, l, value)| {
            format!(
                "point={}, lambda={}, f={}",
                fmt_vector(&[*x, *y]),
                fmt_scalar(*l),
                fmt_scalar(*value)
            )
        })
        .collect::<Vec<_>>()
        .join("; ");
    CommandOutcome::Message(format!("LagrangeMultipliers: {lines}"))
}

fn run_line_integral_scalar_command(args: &[String], document: &Document) -> CommandOutcome {
    let expr = expand_all_cas(&args[0], document);
    let curve = match parse_expression_vector_arg(&args[1]) {
        Ok(v) if (2..=3).contains(&v.len()) => v,
        Ok(_) => {
            return CommandOutcome::Error("LineIntegralScalar: curva 2D o 3D requerida".into())
        }
        Err(e) => return CommandOutcome::Error(format!("LineIntegralScalar: {e}")),
    };
    let t_var = clean_symbol_arg(&args[2]);
    let a = command_result!(parse_finite_command_arg(
        "LineIntegralScalar",
        "a",
        &args[3],
        &document.variables,
    ));
    let b = command_result!(parse_finite_command_arg(
        "LineIntegralScalar",
        "b",
        &args[4],
        &document.variables,
    ));
    let n = match parse_quadrature_n(args.get(5), 200, 2, 2000) {
        Ok(value) => value,
        Err(error) => return CommandOutcome::Error(format!("LineIntegralScalar: {error}")),
    };
    let derivs = curve
        .iter()
        .map(|c| symbolic_partial(c, &t_var).unwrap_or_else(|_| "0".into()))
        .collect::<Vec<_>>();
    let field_vars = default_multivar_names(curve.len());
    let dt = (b - a) / n as f64;
    let mut total = 0.0;
    for i in 0..n {
        let t = a + (i as f64 + 0.5) * dt;
        let coords = eval_param_values(&curve, &t_var, t, document)
            .map_err(|e| CommandOutcome::Error(format!("LineIntegralScalar: {e}")));
        let coords = match coords {
            Ok(v) => v,
            Err(out) => return out,
        };
        let dcoords = eval_param_values(&derivs, &t_var, t, document)
            .map_err(|e| CommandOutcome::Error(format!("LineIntegralScalar: {e}")));
        let dcoords = match dcoords {
            Ok(v) => v,
            Err(out) => return out,
        };
        let value = match eval_at_vars(&expr, &field_vars, &coords, document) {
            Ok(v) => v,
            Err(e) => return CommandOutcome::Error(format!("LineIntegralScalar: {e}")),
        };
        total += value * norm(&dcoords) * dt.abs();
    }
    if total.is_finite() {
        CommandOutcome::Message(format!("LineIntegralScalar ≈ {}", fmt_scalar(total)))
    } else {
        CommandOutcome::Error("LineIntegralScalar: el resultado no es finito".into())
    }
}

fn run_line_integral_vector_command(args: &[String], document: &Document) -> CommandOutcome {
    let fields = match parse_expression_vector_arg(&args[0]) {
        Ok(v) if (2..=3).contains(&v.len()) => v
            .into_iter()
            .map(|e| expand_all_cas(&e, document))
            .collect::<Vec<_>>(),
        Ok(_) => {
            return CommandOutcome::Error("LineIntegralVector: campo 2D o 3D requerido".into())
        }
        Err(e) => return CommandOutcome::Error(format!("LineIntegralVector: {e}")),
    };
    let curve = match parse_expression_vector_arg(&args[1]) {
        Ok(v) if v.len() == fields.len() => v,
        Ok(_) => {
            return CommandOutcome::Error(
                "LineIntegralVector: campo y curva deben tener la misma dimension".into(),
            )
        }
        Err(e) => return CommandOutcome::Error(format!("LineIntegralVector: {e}")),
    };
    let t_var = clean_symbol_arg(&args[2]);
    let a = command_result!(parse_finite_command_arg(
        "LineIntegralVector",
        "a",
        &args[3],
        &document.variables,
    ));
    let b = command_result!(parse_finite_command_arg(
        "LineIntegralVector",
        "b",
        &args[4],
        &document.variables,
    ));
    let n = match parse_quadrature_n(args.get(5), 200, 2, 2000) {
        Ok(value) => value,
        Err(error) => return CommandOutcome::Error(format!("LineIntegralVector: {error}")),
    };
    let derivs = curve
        .iter()
        .map(|c| symbolic_partial(c, &t_var).unwrap_or_else(|_| "0".into()))
        .collect::<Vec<_>>();
    let field_vars = default_multivar_names(fields.len());
    let dt = (b - a) / n as f64;
    let mut total = 0.0;
    for i in 0..n {
        let t = a + (i as f64 + 0.5) * dt;
        let coords = match eval_param_values(&curve, &t_var, t, document) {
            Ok(v) => v,
            Err(e) => return CommandOutcome::Error(format!("LineIntegralVector: {e}")),
        };
        let dcoords = match eval_param_values(&derivs, &t_var, t, document) {
            Ok(v) => v,
            Err(e) => return CommandOutcome::Error(format!("LineIntegralVector: {e}")),
        };
        let fvals = match eval_fields_at(&fields, &field_vars, &coords, document) {
            Ok(v) => v,
            Err(e) => return CommandOutcome::Error(format!("LineIntegralVector: {e}")),
        };
        total += dot(&fvals, &dcoords) * dt;
    }
    if total.is_finite() {
        CommandOutcome::Message(format!("LineIntegralVector ≈ {}", fmt_scalar(total)))
    } else {
        CommandOutcome::Error("LineIntegralVector: el resultado no es finito".into())
    }
}

fn run_triple_integral_command(args: &[String], document: &Document) -> CommandOutcome {
    let expr = expand_all_cas(&args[0], document);
    let x_var = clean_symbol_arg(&args[1]);
    let a = match require_finite(parse_numeric_arg(&args[2], &document.variables)) {
        Ok(value) => value,
        Err(error) => {
            return CommandOutcome::Error(format!(
                "TripleIntegral: límite inferior de x inválido: {error}"
            ))
        }
    };
    let b = match require_finite(parse_numeric_arg(&args[3], &document.variables)) {
        Ok(value) => value,
        Err(error) => {
            return CommandOutcome::Error(format!(
                "TripleIntegral: límite superior de x inválido: {error}"
            ))
        }
    };
    let y_var = clean_symbol_arg(&args[4]);
    let y0_expr = args[5].trim();
    let y1_expr = args[6].trim();
    let z_var = clean_symbol_arg(&args[7]);
    let z0_expr = args[8].trim();
    let z1_expr = args[9].trim();
    let n = match parse_quadrature_n(args.get(10), 40, 2, 160) {
        Ok(value) => value,
        Err(error) => return CommandOutcome::Error(format!("TripleIntegral: {error}")),
    };
    let dx = (b - a) / n as f64;
    let mut total = 0.0;
    for i in 0..n {
        let x = a + (i as f64 + 0.5) * dx;
        let x_assign = [(x_var.as_str(), x)];
        let y0 = match require_finite(eval_multivar_expr(y0_expr, &document.variables, &x_assign)) {
            Ok(v) => v,
            Err(e) => return CommandOutcome::Error(format!("TripleIntegral: {e}")),
        };
        let y1 = match require_finite(eval_multivar_expr(y1_expr, &document.variables, &x_assign)) {
            Ok(v) => v,
            Err(e) => return CommandOutcome::Error(format!("TripleIntegral: {e}")),
        };
        let dy = (y1 - y0) / n as f64;
        for j in 0..n {
            let y = y0 + (j as f64 + 0.5) * dy;
            let xy_assign = [(x_var.as_str(), x), (y_var.as_str(), y)];
            let z0 = match require_finite(eval_multivar_expr(
                z0_expr,
                &document.variables,
                &xy_assign,
            )) {
                Ok(v) => v,
                Err(e) => return CommandOutcome::Error(format!("TripleIntegral: {e}")),
            };
            let z1 = match require_finite(eval_multivar_expr(
                z1_expr,
                &document.variables,
                &xy_assign,
            )) {
                Ok(v) => v,
                Err(e) => return CommandOutcome::Error(format!("TripleIntegral: {e}")),
            };
            let dz = (z1 - z0) / n as f64;
            for k in 0..n {
                let z = z0 + (k as f64 + 0.5) * dz;
                let assignments = [
                    (x_var.as_str(), x),
                    (y_var.as_str(), y),
                    (z_var.as_str(), z),
                ];
                let value = match eval_multivar_expr(&expr, &document.variables, &assignments) {
                    Ok(v) => v,
                    Err(e) => return CommandOutcome::Error(format!("TripleIntegral: {e}")),
                };
                total += value * dx * dy * dz;
            }
        }
    }
    if total.is_finite() {
        CommandOutcome::Message(format!("TripleIntegral ≈ {}", fmt_scalar(total)))
    } else {
        CommandOutcome::Error("TripleIntegral: el resultado no es finito".into())
    }
}

fn run_surface_integral_scalar_command(args: &[String], document: &Document) -> CommandOutcome {
    let scalar = expand_all_cas(&args[0], document);
    let params = match parse_surface_param_args(args, document, "SurfaceIntegralScalar") {
        Ok(p) => p,
        Err(out) => return out,
    };
    let value = match integrate_parametric_surface(&params, document, |coords, normal| {
        eval_at_vars(&scalar, &default_multivar_names(3), coords, document)
            .map(|v| v * norm(normal))
    }) {
        Ok(v) => v,
        Err(e) => return CommandOutcome::Error(format!("SurfaceIntegralScalar: {e}")),
    };
    CommandOutcome::Message(format!("SurfaceIntegralScalar ≈ {}", fmt_scalar(value)))
}

fn run_flux_command(args: &[String], document: &Document) -> CommandOutcome {
    let fields = match parse_expression_vector_arg(&args[0]) {
        Ok(v) if v.len() == 3 => v
            .into_iter()
            .map(|e| expand_all_cas(&e, document))
            .collect::<Vec<_>>(),
        Ok(_) => return CommandOutcome::Error("Flux: campo 3D requerido".into()),
        Err(e) => return CommandOutcome::Error(format!("Flux: {e}")),
    };
    let params = match parse_surface_param_args(args, document, "Flux") {
        Ok(p) => p,
        Err(out) => return out,
    };
    let value = match flux_over_parametric_surface(&fields, &params, document) {
        Ok(v) => v,
        Err(e) => return CommandOutcome::Error(format!("Flux: {e}")),
    };
    CommandOutcome::Message(format!("Flux ≈ {}", fmt_scalar(value)))
}

fn run_is_conservative_command(args: &[String], document: &Document) -> CommandOutcome {
    let fields = match parse_expression_vector_arg(&args[0]) {
        Ok(v) => v
            .into_iter()
            .map(|e| expand_all_cas(&e, document))
            .collect::<Vec<_>>(),
        Err(e) => return CommandOutcome::Error(format!("IsConservative: {e}")),
    };
    let vars = match parse_vars_arg(args, 1, fields.len()) {
        Ok(v) => v,
        Err(e) => return CommandOutcome::Error(format!("IsConservative: {e}")),
    };
    let conservative = if fields.len() == 2 && vars.len() == 2 {
        conservative_pair_equal(&fields[0], &fields[1], &vars[0], &vars[1])
    } else if fields.len() == 3 && vars.len() == 3 {
        partial_derivatives_equal(&fields[2], &vars[1], &fields[1], &vars[2])
            && partial_derivatives_equal(&fields[0], &vars[2], &fields[2], &vars[0])
            && partial_derivatives_equal(&fields[1], &vars[0], &fields[0], &vars[1])
    } else {
        return CommandOutcome::Error("IsConservative: use campo 2D o 3D".into());
    };
    CommandOutcome::Message(format!("IsConservative = {conservative}"))
}

fn run_potential_function_command(args: &[String], document: &Document) -> CommandOutcome {
    let fields = match parse_expression_vector_arg(&args[0]) {
        Ok(v) if v.len() == 2 => v
            .into_iter()
            .map(|e| expand_all_cas(&e, document))
            .collect::<Vec<_>>(),
        Ok(_) => return CommandOutcome::Error("PotentialFunction: solo campo 2D [P,Q]".into()),
        Err(e) => return CommandOutcome::Error(format!("PotentialFunction: {e}")),
    };
    let vars = match parse_vars_arg(args, 1, 2) {
        Ok(v) if v.len() == 2 => v,
        Ok(_) => {
            return CommandOutcome::Error("PotentialFunction: se requieren dos variables".into())
        }
        Err(e) => return CommandOutcome::Error(format!("PotentialFunction: {e}")),
    };
    if !conservative_pair_equal(&fields[0], &fields[1], &vars[0], &vars[1]) {
        return CommandOutcome::Error("PotentialFunction: el campo no parece conservativo".into());
    }
    let phi_x = integrate_for_potential(&fields[0], &vars[0]);
    let d_phi_y = symbolic_partial(&phi_x, &vars[1]).unwrap_or_else(|_| "0".into());
    let correction_derivative = simplified_difference(&fields[1], &d_phi_y);
    let correction = integrate_for_potential(&correction_derivative, &vars[1]);
    let raw = format!("({phi_x}) + ({correction})");
    let potential = symbolic::simplify(&raw).unwrap_or(raw);
    CommandOutcome::Message(format!("PotentialFunction = {potential} + C"))
}

fn run_green_theorem_command(args: &[String], document: &Document) -> CommandOutcome {
    let fields = match parse_expression_vector_arg(&args[0]) {
        Ok(v) if v.len() == 2 => v,
        Ok(_) => return CommandOutcome::Error("GreenTheorem: campo 2D [P,Q] requerido".into()),
        Err(e) => return CommandOutcome::Error(format!("GreenTheorem: {e}")),
    };
    let x_var = clean_symbol_arg(&args[1]);
    let y_var = clean_symbol_arg(&args[4]);
    let dq_dx = symbolic_partial(&fields[1], &x_var).unwrap_or_else(|_| "0".into());
    let dp_dy = symbolic_partial(&fields[0], &y_var).unwrap_or_else(|_| "0".into());
    let integrand = simplified_difference(&dq_dx, &dp_dy);
    let mut di_args = args[1..].to_vec();
    di_args.insert(0, integrand.clone());
    match run_double_integral_command(&di_args, document, false) {
        CommandOutcome::Message(m) => CommandOutcome::Message(format!(
            "GreenTheorem: integrand = {integrand}; {}",
            m.replace("DoubleIntegral", "area integral")
        )),
        other => other,
    }
}

fn run_stokes_theorem_command(args: &[String], document: &Document) -> CommandOutcome {
    let fields = match parse_expression_vector_arg(&args[0]) {
        Ok(v) if v.len() == 3 => v,
        Ok(_) => return CommandOutcome::Error("StokesTheorem: campo 3D requerido".into()),
        Err(e) => return CommandOutcome::Error(format!("StokesTheorem: {e}")),
    };
    let vars = default_multivar_names(3);
    let curl = match curl_components_3d(&fields, &vars) {
        Ok(v) => v,
        Err(e) => return CommandOutcome::Error(format!("StokesTheorem: {e}")),
    };
    let params = match parse_surface_param_args(args, document, "StokesTheorem") {
        Ok(p) => p,
        Err(out) => return out,
    };
    let value = match flux_over_parametric_surface(&curl, &params, document) {
        Ok(v) => v,
        Err(e) => return CommandOutcome::Error(format!("StokesTheorem: {e}")),
    };
    CommandOutcome::Message(format!(
        "StokesTheorem: curl = {}; surface integral ≈ {}",
        fmt_symbolic_vector(&curl),
        fmt_scalar(value)
    ))
}

fn run_gauss_ostrogradski_command(args: &[String], document: &Document) -> CommandOutcome {
    let fields = match parse_expression_vector_arg(&args[0]) {
        Ok(v) if v.len() == 3 => v,
        Ok(_) => return CommandOutcome::Error("GaussOstrogradski: campo 3D requerido".into()),
        Err(e) => return CommandOutcome::Error(format!("GaussOstrogradski: {e}")),
    };
    let vars = [
        clean_symbol_arg(&args[1]),
        clean_symbol_arg(&args[4]),
        clean_symbol_arg(&args[7]),
    ];
    let mut terms = Vec::new();
    for (field, var) in fields.iter().zip(vars.iter()) {
        terms.push(symbolic_partial(field, var).unwrap_or_else(|_| "0".into()));
    }
    let integrand = symbolic::simplify(&terms.join(" + ")).unwrap_or_else(|_| terms.join(" + "));
    let mut ti_args = args[1..].to_vec();
    ti_args.insert(0, integrand.clone());
    match run_triple_integral_command(&ti_args, document) {
        CommandOutcome::Message(m) => CommandOutcome::Message(format!(
            "GaussOstrogradski: divergence = {integrand}; {}",
            m.replace("TripleIntegral", "volume integral")
        )),
        other => other,
    }
}

fn run_change_of_variables_command(args: &[String], _document: &Document) -> CommandOutcome {
    let mapping = match parse_expression_vector_arg(&args[1]) {
        Ok(v) if v.len() == 2 => v,
        Ok(_) => return CommandOutcome::Error("ChangeOfVariables: mapeo 2D requerido".into()),
        Err(e) => return CommandOutcome::Error(format!("ChangeOfVariables: {e}")),
    };
    let vars = match parse_vars_arg(args, 2, 2) {
        Ok(v) if v.len() == 2 => v,
        Ok(_) => {
            return CommandOutcome::Error("ChangeOfVariables: se requieren variables [u,v]".into())
        }
        Err(e) => return CommandOutcome::Error(format!("ChangeOfVariables: {e}")),
    };
    let xu = symbolic_partial(&mapping[0], &vars[0]).unwrap_or_else(|_| "0".into());
    let xv = symbolic_partial(&mapping[0], &vars[1]).unwrap_or_else(|_| "0".into());
    let yu = symbolic_partial(&mapping[1], &vars[0]).unwrap_or_else(|_| "0".into());
    let yv = symbolic_partial(&mapping[1], &vars[1]).unwrap_or_else(|_| "0".into());
    let det_raw = format!("({xu})*({yv}) - ({xv})*({yu})");
    let det = symbolic::simplify(&det_raw).unwrap_or(det_raw);
    CommandOutcome::Message(format!("ChangeOfVariables: Jacobian determinant = {det}"))
}

fn run_riemann_sum_command(args: &[String], document: &Document) -> CommandOutcome {
    let expr = expand_all_cas(&args[0], document);
    let var = clean_symbol_arg(&args[1]);
    let a = command_result!(parse_finite_command_arg(
        "RiemannSum",
        "a",
        &args[2],
        &document.variables,
    ));
    let b = command_result!(parse_finite_command_arg(
        "RiemannSum",
        "b",
        &args[3],
        &document.variables,
    ));
    let n = match parse_quadrature_n(args.get(4), 100, 1, 1_000_000) {
        Ok(value) => value,
        Err(error) => return CommandOutcome::Error(format!("RiemannSum: {error}")),
    };
    let method = args
        .get(5)
        .map(|s| s.trim().to_lowercase())
        .unwrap_or("midpoint".into());
    if !matches!(
        method.as_str(),
        "left" | "izquierda" | "right" | "derecha" | "midpoint" | "medio"
    ) {
        return CommandOutcome::Error(format!("RiemannSum: método desconocido '{method}'"));
    }
    let dx = (b - a) / n as f64;
    let mut total = 0.0;
    for i in 0..n {
        let x = match method.as_str() {
            "left" | "izquierda" => a + i as f64 * dx,
            "right" | "derecha" => a + (i as f64 + 1.0) * dx,
            "midpoint" | "medio" => a + (i as f64 + 0.5) * dx,
            _ => {
                // validated above; keep midpoint as safe default
                a + (i as f64 + 0.5) * dx
            }
        };
        let value = match eval_multivar_expr(&expr, &document.variables, &[(var.as_str(), x)]) {
            Ok(v) => v,
            Err(e) => return CommandOutcome::Error(format!("RiemannSum: {e}")),
        };
        total += value * dx;
    }
    if total.is_finite() {
        CommandOutcome::Message(format!(
            "RiemannSum({method}, n={n}) ≈ {}",
            fmt_scalar(total)
        ))
    } else {
        CommandOutcome::Error("RiemannSum: el resultado no es finito".into())
    }
}

fn run_improper_integral_command(args: &[String], document: &Document) -> CommandOutcome {
    let expr = expand_all_cas(&args[0], document);
    let var = clean_symbol_arg(&args[1]);
    let a = command_result!(parse_finite_command_arg(
        "ImproperIntegral",
        "a",
        &args[2],
        &document.variables,
    ));
    let b = command_result!(parse_finite_command_arg(
        "ImproperIntegral",
        "b",
        &args[3],
        &document.variables,
    ));
    match symbolic::integrate_definite(&expr, &var, a, b) {
        Ok(value) => CommandOutcome::Message(format!("ImproperIntegral: {value}")),
        Err(error) => CommandOutcome::Error(format!("ImproperIntegral: {error}")),
    }
}

fn run_bolzano_check_command(args: &[String], document: &Document) -> CommandOutcome {
    let expr = expand_all_cas(&args[0], document);
    let var = clean_symbol_arg(&args[1]);
    let a = command_result!(parse_finite_command_arg(
        "BolzanoCheck",
        "a",
        &args[2],
        &document.variables,
    ));
    let b = command_result!(parse_finite_command_arg(
        "BolzanoCheck",
        "b",
        &args[3],
        &document.variables,
    ));
    let fa =
        eval_multivar_expr(&expr, &document.variables, &[(var.as_str(), a)]).unwrap_or(f64::NAN);
    let fb =
        eval_multivar_expr(&expr, &document.variables, &[(var.as_str(), b)]).unwrap_or(f64::NAN);
    let continuous = interval_samples_are_finite(&expr, &var, a, b, document);
    if fa.is_finite() && fb.is_finite() && fa * fb <= 0.0 && !continuous {
        return CommandOutcome::Message(format!(
            "BolzanoCheck = inconclusive; f(a)={}, f(b)={}: la función no es finita en todo el intervalo",
            fmt_scalar(fa),
            fmt_scalar(fb)
        ));
    }
    let exists = fa.is_finite() && fb.is_finite() && continuous && fa * fb <= 0.0;
    let root = if exists {
        bisection_root(&expr, &var, a, b, document)
            .map(|r| format!("; c ≈ {}", fmt_scalar(r)))
            .unwrap_or_default()
    } else {
        String::new()
    };
    CommandOutcome::Message(format!(
        "BolzanoCheck = {exists}; f(a)={}, f(b)={}{}",
        fmt_scalar(fa),
        fmt_scalar(fb),
        root
    ))
}

fn run_rolle_check_command(args: &[String], document: &Document) -> CommandOutcome {
    let expr = expand_all_cas(&args[0], document);
    let var = clean_symbol_arg(&args[1]);
    let a = command_result!(parse_finite_command_arg(
        "RolleCheck",
        "a",
        &args[2],
        &document.variables,
    ));
    let b = command_result!(parse_finite_command_arg(
        "RolleCheck",
        "b",
        &args[3],
        &document.variables,
    ));
    let fa =
        eval_multivar_expr(&expr, &document.variables, &[(var.as_str(), a)]).unwrap_or(f64::NAN);
    let fb =
        eval_multivar_expr(&expr, &document.variables, &[(var.as_str(), b)]).unwrap_or(f64::NAN);
    if !interval_samples_are_finite(&expr, &var, a, b, document) {
        return CommandOutcome::Message(format!(
            "RolleCheck = inconclusive; f(a)={}, f(b)={}: la función no es finita en todo el intervalo",
            fmt_scalar(fa),
            fmt_scalar(fb)
        ));
    }
    let deriv = symbolic_partial(&expr, &var).unwrap_or_else(|_| "0".into());
    let candidates = find_roots_in_interval(&deriv, &var, a, b, document, 120);
    CommandOutcome::Message(format!(
        "RolleCheck = {}; f(a)={}, f(b)={}; c = {}",
        (fa - fb).abs() < 1e-6 && !candidates.is_empty(),
        fmt_scalar(fa),
        fmt_scalar(fb),
        fmt_vector(&candidates)
    ))
}

fn run_mean_value_check_command(args: &[String], document: &Document) -> CommandOutcome {
    let expr = expand_all_cas(&args[0], document);
    let var = clean_symbol_arg(&args[1]);
    let a = command_result!(parse_finite_command_arg(
        "MeanValueCheck",
        "a",
        &args[2],
        &document.variables,
    ));
    let b = command_result!(parse_finite_command_arg(
        "MeanValueCheck",
        "b",
        &args[3],
        &document.variables,
    ));
    if a == b {
        return CommandOutcome::Error("MeanValueCheck: se requiere a != b".into());
    }
    let fa =
        eval_multivar_expr(&expr, &document.variables, &[(var.as_str(), a)]).unwrap_or(f64::NAN);
    let fb =
        eval_multivar_expr(&expr, &document.variables, &[(var.as_str(), b)]).unwrap_or(f64::NAN);
    let slope = (fb - fa) / (b - a);
    let deriv = symbolic_partial(&expr, &var).unwrap_or_else(|_| "0".into());
    let equation = format!("({deriv}) - ({slope})");
    let candidates = find_roots_in_interval(&equation, &var, a, b, document, 160);
    CommandOutcome::Message(format!(
        "MeanValueCheck: slope = {}; c = {}",
        fmt_scalar(slope),
        fmt_vector(&candidates)
    ))
}

fn run_cauchy_mean_value_check_command(args: &[String], document: &Document) -> CommandOutcome {
    let f = expand_all_cas(&args[0], document);
    let g = expand_all_cas(&args[1], document);
    let var = clean_symbol_arg(&args[2]);
    let a = command_result!(parse_finite_command_arg(
        "CauchyMeanValueCheck",
        "a",
        &args[3],
        &document.variables,
    ));
    let b = command_result!(parse_finite_command_arg(
        "CauchyMeanValueCheck",
        "b",
        &args[4],
        &document.variables,
    ));
    let fa = eval_multivar_expr(&f, &document.variables, &[(var.as_str(), a)]).unwrap_or(f64::NAN);
    let fb = eval_multivar_expr(&f, &document.variables, &[(var.as_str(), b)]).unwrap_or(f64::NAN);
    let ga = eval_multivar_expr(&g, &document.variables, &[(var.as_str(), a)]).unwrap_or(f64::NAN);
    let gb = eval_multivar_expr(&g, &document.variables, &[(var.as_str(), b)]).unwrap_or(f64::NAN);
    let fp = symbolic_partial(&f, &var).unwrap_or_else(|_| "0".into());
    let gp = symbolic_partial(&g, &var).unwrap_or_else(|_| "0".into());
    let equation = format!("(({fb}-{fa})*({gp})) - (({gb}-{ga})*({fp}))");
    let candidates = find_roots_in_interval(&equation, &var, a, b, document, 160);
    CommandOutcome::Message(format!(
        "CauchyMeanValueCheck: c = {}",
        fmt_vector(&candidates)
    ))
}

fn run_lhopital_command(args: &[String], document: &Document) -> CommandOutcome {
    let mut num = expand_all_cas(&args[0], document);
    let mut den = expand_all_cas(&args[1], document);
    let var = clean_symbol_arg(&args[2]);
    let at = command_result!(parse_finite_command_arg(
        "LHopital",
        "punto",
        &args[3],
        &document.variables,
    ));
    let max_steps = match parse_quadrature_n(args.get(4), 5, 1, 10) {
        Ok(value) => value,
        Err(error) => return CommandOutcome::Error(format!("LHopital: {error}")),
    };
    let numerator_at = eval_multivar_expr(&num, &document.variables, &[(var.as_str(), at)]);
    let denominator_at = eval_multivar_expr(&den, &document.variables, &[(var.as_str(), at)]);
    let indeterminate = match (numerator_at, denominator_at) {
        (Ok(n), Ok(d)) => {
            (n.abs() <= 1e-12 && d.abs() <= 1e-12) || (!n.is_finite() && !d.is_finite())
        }
        _ => false,
    };
    if !indeterminate {
        return CommandOutcome::Error(
            "LHopital: sólo aplica a las formas indeterminadas 0/0 o infinito/infinito".into(),
        );
    }
    for step in 0..=max_steps {
        if let Some(v) = symmetric_quotient_limit(&num, &den, &var, at, document) {
            return CommandOutcome::Message(format!(
                "LHopital: steps = {step}; limit ≈ {}",
                fmt_scalar(v)
            ));
        }
        num = symbolic_partial(&num, &var).unwrap_or_else(|_| "0".into());
        den = symbolic_partial(&den, &var).unwrap_or_else(|_| "0".into());
    }
    CommandOutcome::Message("LHopital: inconclusive".into())
}

fn run_alternating_series_test_command(args: &[String], document: &Document) -> CommandOutcome {
    let expr = expand_all_cas(&args[0], document);
    let var = args
        .get(1)
        .map(|s| clean_symbol_arg(s))
        .unwrap_or_else(|| "n".into());
    let samples = [50.0, 100.0, 200.0, 400.0];
    let mags = samples
        .iter()
        .filter_map(|n| eval_sequence_term(&expr, &var, *n, &document.variables).ok())
        .map(|v| v.abs())
        .collect::<Vec<_>>();
    let decreasing = mags.windows(2).all(|w| w[1] <= w[0] + 1e-9);
    let tends_zero = mags.last().copied().unwrap_or(f64::INFINITY) < 0.05;
    CommandOutcome::Message(format!(
        "AlternatingSeriesTest: decreasing = {decreasing}; tends_to_zero = {tends_zero}"
    ))
}

fn run_integral_test_command(args: &[String], document: &Document) -> CommandOutcome {
    let expr = expand_all_cas(&args[0], document);
    let var = clean_symbol_arg(&args[1]);
    let start = command_result!(parse_finite_command_arg(
        "IntegralTest",
        "inicio",
        &args[2],
        &document.variables,
    ));
    let finite = integrate_1d_midpoint(&expr, &var, start, start + 1000.0, 4000, document)
        .unwrap_or(f64::NAN);
    let tail = integrate_1d_midpoint(&expr, &var, start + 1000.0, start + 2000.0, 2000, document)
        .unwrap_or(f64::NAN);
    let status = if finite.is_finite() && tail.is_finite() && tail.abs() < 1e-3 {
        "likely converges"
    } else {
        "inconclusive"
    };
    CommandOutcome::Message(format!(
        "IntegralTest: partial integral ≈ {}; tail ≈ {}; {status}",
        fmt_scalar(finite),
        fmt_scalar(tail)
    ))
}

fn run_absolute_convergence_command(args: &[String], document: &Document) -> CommandOutcome {
    let expr = format!("abs({})", expand_all_cas(&args[0], document));
    let var = args
        .get(1)
        .map(|s| clean_symbol_arg(s))
        .unwrap_or_else(|| "n".into());
    let ratio_args = vec![expr, var];
    match run_series_ratio_test_command(&ratio_args, document) {
        CommandOutcome::Message(m) => CommandOutcome::Message(format!("AbsoluteConvergence: {m}")),
        other => other,
    }
}

fn eval_sequence_term(
    expr: &str,
    var: &str,
    n: f64,
    variables: &HashMap<String, f64>,
) -> Result<f64, String> {
    eval_multivar_expr(expr, variables, &[(var, n)])
}

fn convergence_label(limit: f64) -> &'static str {
    if limit < 1.0 - 1e-3 {
        "converges"
    } else if limit > 1.0 + 1e-3 {
        "diverges"
    } else {
        "inconclusive"
    }
}

fn run_sequence_limit_command(args: &[String], document: &Document) -> CommandOutcome {
    let expr = expand_all_cas(&args[0], document);
    let var = args
        .get(1)
        .map(|s| clean_symbol_arg(s))
        .unwrap_or_else(|| "n".to_string());
    let samples = [100.0, 300.0, 1000.0, 3000.0, 10000.0];
    let mut values = Vec::with_capacity(samples.len());
    for n in samples {
        match eval_sequence_term(&expr, &var, n, &document.variables) {
            Ok(v) => values.push(v),
            Err(e) => return CommandOutcome::Error(format!("SequenceLimit: {e}")),
        }
    }
    let estimate = *values.last().unwrap_or(&f64::NAN);
    let drift = values
        .windows(2)
        .last()
        .map(|w| (w[1] - w[0]).abs())
        .unwrap_or(f64::NAN);
    let status = if drift.is_finite() && drift < 1e-4 {
        "stable"
    } else {
        "heuristic"
    };
    CommandOutcome::Message(format!(
        "SequenceLimit ≈ {} ({status}, drift {})",
        fmt_scalar(estimate),
        fmt_scalar(drift)
    ))
}

fn run_series_sum_command(args: &[String], document: &Document) -> CommandOutcome {
    let expr = expand_all_cas(&args[0], document);
    let var = clean_symbol_arg(&args[1]);
    let start = match parse_numeric_arg(&args[2], &document.variables) {
        Ok(v) if v.is_finite() && v >= i64::MIN as f64 && v < i64::MAX as f64 => v.round() as i64,
        Ok(_) => {
            return CommandOutcome::Error(
                "SeriesSum: los límites deben ser enteros finitos representables".into(),
            )
        }
        Err(e) => return CommandOutcome::Error(format!("SeriesSum: {e}")),
    };
    let end = match parse_numeric_arg(&args[3], &document.variables) {
        Ok(v) if v.is_finite() && v >= i64::MIN as f64 && v < i64::MAX as f64 => v.round() as i64,
        Ok(_) => {
            return CommandOutcome::Error(
                "SeriesSum: los límites deben ser enteros finitos representables".into(),
            )
        }
        Err(e) => return CommandOutcome::Error(format!("SeriesSum: {e}")),
    };
    match start.abs_diff(end).checked_add(1) {
        Some(count) if count <= 100_000 => {}
        _ => return CommandOutcome::Error("SeriesSum: demasiados términos".into()),
    }
    let step = if end >= start { 1 } else { -1 };
    let mut total = 0.0;
    let mut n = start;
    loop {
        match eval_sequence_term(&expr, &var, n as f64, &document.variables) {
            Ok(v) => total += v,
            Err(e) => return CommandOutcome::Error(format!("SeriesSum: {e}")),
        }
        if n == end {
            break;
        }
        n += step;
    }
    CommandOutcome::Message(format!(
        "SeriesSum[{}={}..{}] = {}",
        var,
        start,
        end,
        fmt_scalar(total)
    ))
}

fn run_series_ratio_test_command(args: &[String], document: &Document) -> CommandOutcome {
    let expr = expand_all_cas(&args[0], document);
    let var = args
        .get(1)
        .map(|s| clean_symbol_arg(s))
        .unwrap_or_else(|| "n".to_string());
    let samples = [20.0, 40.0, 80.0, 120.0];
    let mut ratios = Vec::with_capacity(samples.len());
    for n in samples {
        let a_n = match eval_sequence_term(&expr, &var, n, &document.variables) {
            Ok(v) => v,
            Err(e) => return CommandOutcome::Error(format!("RatioTest: {e}")),
        };
        let a_next = match eval_sequence_term(&expr, &var, n + 1.0, &document.variables) {
            Ok(v) => v,
            Err(e) => return CommandOutcome::Error(format!("RatioTest: {e}")),
        };
        if a_n.abs() > 1e-300 {
            ratios.push((a_next / a_n).abs());
        }
    }
    let Some(limit) = ratios.last().copied() else {
        return CommandOutcome::Error("RatioTest: no se pudo estimar el cociente".into());
    };
    CommandOutcome::Message(format!(
        "RatioTest L ≈ {} -> {}",
        fmt_scalar(limit),
        convergence_label(limit)
    ))
}

fn run_series_root_test_command(args: &[String], document: &Document) -> CommandOutcome {
    let expr = expand_all_cas(&args[0], document);
    let var = args
        .get(1)
        .map(|s| clean_symbol_arg(s))
        .unwrap_or_else(|| "n".to_string());
    let samples = [20.0, 40.0, 80.0, 120.0];
    let mut roots = Vec::with_capacity(samples.len());
    for n in samples {
        let a_n = match eval_sequence_term(&expr, &var, n, &document.variables) {
            Ok(v) => v,
            Err(e) => return CommandOutcome::Error(format!("RootTest: {e}")),
        };
        roots.push(a_n.abs().powf(1.0 / n));
    }
    let Some(limit) = roots.last().copied() else {
        return CommandOutcome::Error("RootTest: no se pudo estimar la raíz".into());
    };
    CommandOutcome::Message(format!(
        "RootTest L ≈ {} -> {}",
        fmt_scalar(limit),
        convergence_label(limit)
    ))
}

fn clean_symbol_arg(s: &str) -> String {
    s.trim()
        .trim_matches('"')
        .trim_matches('\'')
        .trim()
        .to_string()
}

fn is_valid_parameter_name(name: &str) -> bool {
    let mut chars = name.chars();
    let Some(first) = chars.next() else {
        return false;
    };
    (first.is_ascii_alphabetic() || first == '_')
        && chars.all(|c| c.is_ascii_alphanumeric() || c == '_')
}

fn linearized_parameter_system<F>(
    params: &[String],
    base_vars: &HashMap<String, f64>,
    eval_equations: F,
) -> Result<(Option<Matrix>, Option<Matrix>), String>
where
    F: Fn(&HashMap<String, f64>) -> Result<Vec<f64>, String>,
{
    let mut vars = base_vars.clone();
    for param in params {
        vars.insert(param.clone(), 0.0);
    }
    let constants = eval_equations(&vars)?;
    let mut columns = Vec::with_capacity(params.len());
    for param in params {
        let mut vars = base_vars.clone();
        for p in params {
            vars.insert(p.clone(), 0.0);
        }
        vars.insert(param.clone(), 1.0);
        let values = eval_equations(&vars)?;
        if values.len() != constants.len() {
            return Err("cantidad inconsistente de ecuaciones".into());
        }
        columns.push(values);
    }

    let mut rows = Vec::new();
    let mut rhs = Vec::new();
    for i in 0..constants.len() {
        let coeffs = columns
            .iter()
            .map(|col| col[i] - constants[i])
            .collect::<Vec<_>>();
        let has_coeff = coeffs.iter().any(|x| x.abs() > 1e-10);
        if has_coeff || constants[i].abs() > 1e-10 {
            rows.push(coeffs);
            rhs.push(vec![-constants[i]]);
        }
    }
    if rows.is_empty() {
        return Ok((None, None));
    }
    let a = Matrix::from_rows(rows).ok_or_else(|| "sistema paramétrico inválido".to_string())?;
    let b = Matrix::from_rows(rhs).ok_or_else(|| "sistema paramétrico inválido".to_string())?;
    Ok((Some(a), Some(b)))
}

fn format_parameter_solution(params: &[String], a: &Matrix, b: &Matrix) -> String {
    let aug = augment_matrix(a, b);
    let (rref, pivots) = rref_with_pivots_partitioned(&aug, params.len(), 1e-10);
    if has_inconsistent_augmented_row(&rref, params.len(), 1e-10) {
        return format!(
            "No parameter solution: rank(A)={}, rank([A|b])={}",
            coefficient_pivot_count(&pivots, params.len()),
            pivots.len()
        );
    }
    let rank_a = coefficient_pivot_count(&pivots, params.len());
    let x0 = particular_solution_from_augmented_rref(&rref, &pivots, params.len());
    if rank_a == params.len() {
        let assignments = params
            .iter()
            .zip(x0.iter())
            .map(|(name, value)| format!("{} = {}", name, fmt_scalar(*value)))
            .collect::<Vec<_>>()
            .join(", ");
        return format!("Unique parameter solution: {assignments}");
    }
    let basis = null_space_from_rref(&rref, &pivots, params.len());
    format!(
        "Infinite parameter solutions for [{}]:\nx0 = {}\nbasis = {}",
        params.join(", "),
        fmt_vector(&x0),
        fmt_vector_basis(&basis)
    )
}

fn solve_linear_command(a: &Matrix, b: &Matrix) -> CommandOutcome {
    if b.rows != a.rows {
        return CommandOutcome::Error(format!(
            "LinearSolve: dimensiones incompatibles A={}x{}, b={}x{}",
            a.rows, a.cols, b.rows, b.cols
        ));
    }
    if b.cols != 1 {
        if a.rows == a.cols {
            if let Some(x) = solve_linear_system(a, b) {
                return CommandOutcome::Message(format!("Unique solution:\n{}", x));
            }
        }
        return CommandOutcome::Error("LinearSolve: RHS matricial no resoluble".into());
    }

    let aug = augment_matrix(a, b);
    let (rref, pivots) = rref_with_pivots_partitioned(&aug, a.cols, 1e-10);
    if has_inconsistent_augmented_row(&rref, a.cols, 1e-10) {
        return CommandOutcome::Message(format!(
            "No solution: rank(A)={}, rank([A|b])={}",
            coefficient_pivot_count(&pivots, a.cols),
            pivots.len()
        ));
    }
    let rank_a = coefficient_pivot_count(&pivots, a.cols);
    if rank_a == a.cols {
        let x = particular_solution_from_augmented_rref(&rref, &pivots, a.cols);
        return CommandOutcome::Message(format!("Unique solution: x = {}", fmt_vector(&x)));
    }

    let x0 = particular_solution_from_augmented_rref(&rref, &pivots, a.cols);
    let basis = null_space_from_rref(&rref, &pivots, a.cols);
    CommandOutcome::Message(format!(
        "Infinite solutions:\nx0 = {}\nbasis Ker(A) = {}",
        fmt_vector(&x0),
        fmt_vector_basis(&basis)
    ))
}

#[allow(clippy::unwrap_used)]
fn augment_matrix(a: &Matrix, b: &Matrix) -> Matrix {
    let mut rows = Vec::with_capacity(a.rows);
    for r in 0..a.rows {
        let mut row = Vec::with_capacity(a.cols + b.cols);
        for c in 0..a.cols {
            row.push(a.get(r, c));
        }
        for c in 0..b.cols {
            row.push(b.get(r, c));
        }
        rows.push(row);
    }
    Matrix::from_rows(rows).unwrap()
}

fn rref_with_pivots(m: &Matrix, eps: f64) -> (Matrix, Vec<usize>) {
    rref_with_pivots_partitioned(m, m.cols, eps)
}

fn rref_with_pivots_partitioned(
    m: &Matrix,
    coefficient_cols: usize,
    relative_eps: f64,
) -> (Matrix, Vec<usize>) {
    let mut rref = m.clone();
    let mut pivots = Vec::new();
    let mut row = 0;
    let coefficient_cols = coefficient_cols.min(rref.cols);
    let mut coefficient_scale = 0.0_f64;
    let mut rhs_scale = 0.0_f64;
    for r in 0..rref.rows {
        for c in 0..rref.cols {
            if c < coefficient_cols {
                coefficient_scale = coefficient_scale.max(rref.get(r, c).abs());
            } else {
                rhs_scale = rhs_scale.max(rref.get(r, c).abs());
            }
        }
    }
    for col in 0..rref.cols {
        let mut pivot = row;
        let mut max_abs = 0.0;
        for r in row..rref.rows {
            let v = rref.get(r, col).abs();
            if v > max_abs {
                max_abs = v;
                pivot = r;
            }
        }
        let scale = if col < coefficient_cols {
            coefficient_scale
        } else {
            rhs_scale
        };
        if max_abs <= relative_eps * scale {
            continue;
        }
        if pivot != row {
            for c in 0..rref.cols {
                let tmp = rref.get(row, c);
                rref.set(row, c, rref.get(pivot, c));
                rref.set(pivot, c, tmp);
            }
        }
        let pv = rref.get(row, col);
        for c in col..rref.cols {
            rref.set(row, c, rref.get(row, c) / pv);
        }
        for r in 0..rref.rows {
            if r == row {
                continue;
            }
            let factor = rref.get(r, col);
            if factor != 0.0 {
                for c in col..rref.cols {
                    rref.set(r, c, rref.get(r, c) - factor * rref.get(row, c));
                }
            }
        }
        pivots.push(col);
        row += 1;
        if row == rref.rows {
            break;
        }
    }
    (rref, pivots)
}

fn coefficient_pivot_count(pivots: &[usize], vars: usize) -> usize {
    pivots.iter().filter(|&&pivot| pivot < vars).count()
}

fn null_space_from_rref(rref: &Matrix, pivots: &[usize], vars: usize) -> Vec<Vec<f64>> {
    let mut is_pivot = vec![false; vars];
    for &pivot in pivots {
        if pivot < vars {
            is_pivot[pivot] = true;
        }
    }
    let mut basis = Vec::new();
    for free_col in 0..vars {
        if is_pivot[free_col] {
            continue;
        }
        let mut vector = vec![0.0; vars];
        vector[free_col] = 1.0;
        for (row, &pivot_col) in pivots.iter().enumerate() {
            if pivot_col < vars {
                vector[pivot_col] = -rref.get(row, free_col);
            }
        }
        basis.push(vector);
    }
    basis
}

fn particular_solution_from_augmented_rref(
    rref: &Matrix,
    pivots: &[usize],
    vars: usize,
) -> Vec<f64> {
    let mut x = vec![0.0; vars];
    for (row, &pivot_col) in pivots.iter().enumerate() {
        if pivot_col < vars {
            x[pivot_col] = rref.get(row, vars);
        }
    }
    x
}

fn fmt_column_matrix_as_vector(m: &Matrix) -> String {
    let values = (0..m.rows).map(|r| m.get(r, 0)).collect::<Vec<_>>();
    fmt_vector(&values)
}

fn fmt_vector(v: &[f64]) -> String {
    format!(
        "[{}]",
        v.iter()
            .map(|x| fmt_scalar(*x))
            .collect::<Vec<_>>()
            .join(", ")
    )
}

fn fmt_vector_basis(basis: &[Vec<f64>]) -> String {
    if basis.is_empty() {
        "[]".to_string()
    } else {
        format!(
            "[{}]",
            basis
                .iter()
                .map(|v| fmt_vector(v))
                .collect::<Vec<_>>()
                .join(", ")
        )
    }
}

fn fmt_matrix_rows(rows: &[Vec<f64>]) -> String {
    fmt_vector_basis(rows)
}

fn fmt_scalar(x: f64) -> String {
    if x.abs() < 5e-11 {
        "0".to_string()
    } else if (x - x.round()).abs() < 5e-10 {
        format!("{:.0}", x.round())
    } else {
        format!("{:.6}", x)
    }
}

fn fmt_complex_pair(re: f64, im: f64) -> String {
    if im.abs() < 1e-10 {
        fmt_scalar(re)
    } else if im >= 0.0 {
        format!("{} + {}i", fmt_scalar(re), fmt_scalar(im))
    } else {
        format!("{} - {}i", fmt_scalar(re), fmt_scalar(-im))
    }
}

fn run_p2_dependence(args: &[String], document: &Document) -> CommandOutcome {
    let var = args.get(1).map(|s| s.trim()).unwrap_or("x");
    let polys = match parse_p2_list(&args[0], var, document) {
        Ok(p) => p,
        Err(e) => return CommandOutcome::Error(format!("P2Dependence: {e}")),
    };
    let matrix = coefficient_columns_matrix(&polys);
    let r = rank(&matrix).unwrap_or(0);
    if r == polys.len() {
        CommandOutcome::Message(format!("Independent in P2; dimension = {r}"))
    } else {
        let relation = null_space(&matrix).unwrap_or_default().into_iter().next();
        let relation_text = relation
            .as_deref()
            .map(fmt_polynomial_relation)
            .unwrap_or_else(|| "relación no única".to_string());
        CommandOutcome::Message(format!(
            "Dependent in P2; rank = {r}; relation: {relation_text}"
        ))
    }
}

fn run_p2_basis(args: &[String], document: &Document) -> CommandOutcome {
    let var = args.get(1).map(|s| s.trim()).unwrap_or("x");
    let polys = match parse_p2_list(&args[0], var, document) {
        Ok(p) => p,
        Err(e) => return CommandOutcome::Error(format!("P2Basis: {e}")),
    };
    let vectors = polys.iter().map(|p| p.coeffs.clone()).collect::<Vec<_>>();
    let indices = independent_row_indices(&vectors);
    let labels = indices
        .iter()
        .map(|&i| format!("p{}={}", i + 1, polys[i].expr))
        .collect::<Vec<_>>();
    let is_basis_p2 = indices.len() == 3;
    CommandOutcome::Message(format!(
        "Basis of span has dimension {}: {{{}}}. {}",
        indices.len(),
        labels.join(", "),
        if is_basis_p2 {
            "It is a basis of P2."
        } else {
            "Not a basis of P2."
        }
    ))
}

fn run_p2_equations(args: &[String], document: &Document) -> CommandOutcome {
    let var = args.get(1).map(|s| s.trim()).unwrap_or("x");
    let polys = match parse_p2_list(&args[0], var, document) {
        Ok(p) => p,
        Err(e) => return CommandOutcome::Error(format!("P2Equations: {e}")),
    };
    let rows = polys.iter().map(|p| p.coeffs.clone()).collect::<Vec<_>>();
    let Some(matrix) = Matrix::from_rows(rows) else {
        return CommandOutcome::Error("P2Equations: lista vacía".into());
    };
    let dim = rank(&matrix).unwrap_or(0);
    let equations = null_space(&matrix).unwrap_or_default();
    if equations.is_empty() {
        CommandOutcome::Message(format!(
            "P2 span dimension = {dim}\nEquations: none; span is P2"
        ))
    } else {
        let eqs = equations
            .iter()
            .map(|n| fmt_p2_equation(n))
            .collect::<Vec<_>>()
            .join("; ");
        CommandOutcome::Message(format!("P2 span dimension = {dim}\nEquations: {eqs}"))
    }
}

#[derive(Clone)]
struct P2Polynomial {
    expr: String,
    coeffs: Vec<f64>, // [x^2, x, 1]
}

fn parse_p2_list(text: &str, var: &str, document: &Document) -> Result<Vec<P2Polynomial>, String> {
    let text = text.trim();
    if !text.starts_with('{') || !text.ends_with('}') {
        return Err("usa una lista {p1,p2,...}".into());
    }
    let inner = &text[1..text.len() - 1];
    let exprs = split_args(inner);
    if exprs.is_empty() {
        return Err("lista vacía".into());
    }
    exprs
        .into_iter()
        .map(|expr| {
            let coeffs = p2_coefficients(expr.trim(), var, document)?;
            Ok(P2Polynomial {
                expr: expr.trim().to_string(),
                coeffs,
            })
        })
        .collect()
}

fn p2_coefficients(expr: &str, var: &str, document: &Document) -> Result<Vec<f64>, String> {
    let eval = |x: f64| -> Result<f64, String> {
        let mut vars = document
            .variables
            .iter()
            .map(|(k, v)| (k.clone(), *v))
            .collect::<Vec<_>>();
        vars.push((var.to_string(), x));
        evaluate(expr, &vars).map_err(|e| e.to_string())
    };
    let y0 = eval(0.0)?;
    let y1 = eval(1.0)?;
    let ym1 = eval(-1.0)?;
    let c = y0;
    let b = (y1 - ym1) * 0.5;
    let a = (y1 + ym1 - 2.0 * c) * 0.5;
    for x in [2.0, 3.0] {
        let expected = a * x * x + b * x + c;
        let got = eval(x)?;
        if (got - expected).abs() > 1e-7 * got.abs().max(expected.abs()).max(1.0) {
            return Err(format!(
                "'{}' no es un polinomio de grado <= 2 en {}",
                expr, var
            ));
        }
    }
    Ok(vec![a, b, c])
}

#[allow(clippy::unwrap_used)]
fn coefficient_columns_matrix(polys: &[P2Polynomial]) -> Matrix {
    let mut rows = vec![Vec::new(), Vec::new(), Vec::new()];
    for p in polys {
        for (i, coeff) in p.coeffs.iter().enumerate() {
            rows[i].push(*coeff);
        }
    }
    Matrix::from_rows(rows).unwrap()
}

fn fmt_polynomial_relation(coeffs: &[f64]) -> String {
    let mut terms = Vec::new();
    for (i, c) in coeffs.iter().enumerate() {
        if c.abs() > 1e-8 {
            terms.push(format!("{}*p{}", fmt_scalar(*c), i + 1));
        }
    }
    if terms.is_empty() {
        "0 = 0".into()
    } else {
        format!("{} = 0", terms.join(" + ").replace("+ -", "- "))
    }
}

fn fmt_p2_equation(n: &[f64]) -> String {
    let names = ["a", "b", "c"];
    let mut terms = Vec::new();
    for (coeff, name) in n.iter().zip(names) {
        if coeff.abs() > 1e-8 {
            terms.push(format!("{}{}", fmt_scalar(*coeff), name));
        }
    }
    if terms.is_empty() {
        "0 = 0".into()
    } else {
        format!("{} = 0", terms.join(" + ").replace("+ -", "- "))
    }
}

fn run_subspace_dimension(text: &str, document: &Document) -> CommandOutcome {
    let matrix = match parse_matrix_arg_strict(text, &document.variables) {
        Ok(m) => m,
        Err(e) => return CommandOutcome::Error(format!("SubspaceDimension: {e}")),
    };
    let dim = rank(&matrix).unwrap_or(0);
    CommandOutcome::Message(format!(
        "dimension = {dim}\nambient dimension = {}",
        matrix.cols
    ))
}

fn run_subspace_basis(text: &str, document: &Document) -> CommandOutcome {
    let matrix = match parse_matrix_arg_strict(text, &document.variables) {
        Ok(m) => m,
        Err(e) => return CommandOutcome::Error(format!("SubspaceBasis: {e}")),
    };
    let rows = matrix_rows(&matrix);
    let basis = independent_rows(&rows);
    CommandOutcome::Message(format!(
        "basis = {}\ndimension = {}",
        fmt_matrix_rows(&basis),
        basis.len()
    ))
}

fn run_subspace_sum(u_text: &str, v_text: &str, document: &Document) -> CommandOutcome {
    let u = match parse_matrix_arg_strict(u_text, &document.variables) {
        Ok(m) => m,
        Err(e) => return CommandOutcome::Error(format!("SubspaceSum: {e}")),
    };
    let v = match parse_matrix_arg_strict(v_text, &document.variables) {
        Ok(m) => m,
        Err(e) => return CommandOutcome::Error(format!("SubspaceSum: {e}")),
    };
    if u.cols != v.cols {
        return CommandOutcome::Error("SubspaceSum: dimensiones ambientales distintas".into());
    }
    let mut rows = matrix_rows(&u);
    rows.extend(matrix_rows(&v));
    let basis = independent_rows(&rows);
    CommandOutcome::Message(format!(
        "dim(U) = {}\ndim(V) = {}\ndim(U + V) = {}\nbasis(U + V) = {}",
        rank(&u).unwrap_or(0),
        rank(&v).unwrap_or(0),
        basis.len(),
        fmt_matrix_rows(&basis)
    ))
}

fn run_subspace_intersection(u_text: &str, v_text: &str, document: &Document) -> CommandOutcome {
    let u = match parse_matrix_arg_strict(u_text, &document.variables) {
        Ok(m) => m,
        Err(e) => return CommandOutcome::Error(format!("SubspaceIntersection: {e}")),
    };
    let v = match parse_matrix_arg_strict(v_text, &document.variables) {
        Ok(m) => m,
        Err(e) => return CommandOutcome::Error(format!("SubspaceIntersection: {e}")),
    };
    if u.cols != v.cols {
        return CommandOutcome::Error(
            "SubspaceIntersection: dimensiones ambientales distintas".into(),
        );
    }
    let basis = subspace_intersection_basis(&u, &v);
    CommandOutcome::Message(format!(
        "dim(U ∩ V) = {}\nbasis(U ∩ V) = {}",
        basis.len(),
        fmt_matrix_rows(&basis)
    ))
}

fn run_orthogonal_complement(text: &str, document: &Document) -> CommandOutcome {
    let matrix = match parse_matrix_arg_strict(text, &document.variables) {
        Ok(m) => m,
        Err(e) => return CommandOutcome::Error(format!("OrthogonalComplement: {e}")),
    };
    let basis = null_space(&matrix).unwrap_or_default();
    CommandOutcome::Message(format!(
        "dim(U⊥) = {}\nbasis(U⊥) = {}",
        basis.len(),
        fmt_vector_basis(&basis)
    ))
}

fn run_solve_line_3d_parameters(args: &[String], document: &Document) -> CommandOutcome {
    let direction_exprs = match parse_expression_vector_arg(&args[0]) {
        Ok(v) if v.len() == 3 => v,
        Ok(_) => {
            return CommandOutcome::Error(
                "SolveLine3DParameters: el director debe tener 3 coordenadas".into(),
            )
        }
        Err(e) => return CommandOutcome::Error(format!("SolveLine3DParameters: {e}")),
    };
    let relation = clean_symbol_arg(&args[1]).to_lowercase();
    let target_exprs = match parse_expression_vector_arg(&args[2]) {
        Ok(v) if v.len() == 3 => v,
        Ok(_) => {
            return CommandOutcome::Error(
                "SolveLine3DParameters: el vector objetivo debe tener 3 coordenadas".into(),
            )
        }
        Err(e) => return CommandOutcome::Error(format!("SolveLine3DParameters: {e}")),
    };
    let params = args[3..]
        .iter()
        .map(|arg| clean_symbol_arg(arg))
        .collect::<Vec<_>>();
    if params.is_empty() || params.iter().any(|p| !is_valid_parameter_name(p)) {
        return CommandOutcome::Error(
            "SolveLine3DParameters: indica parámetros válidos, por ejemplo h, k".into(),
        );
    }

    let equations = |vars: &HashMap<String, f64>| -> Result<Vec<f64>, String> {
        let d = evaluate_expression_vector(&direction_exprs, vars)?;
        let v = evaluate_expression_vector(&target_exprs, vars)?;
        match relation.as_str() {
            "perpendicular" | "orthogonal" | "ortogonal" => {
                Ok(vec![d[0] * v[0] + d[1] * v[1] + d[2] * v[2]])
            }
            "parallel" | "paralelo" | "paralela" => Ok(vec![
                d[1] * v[2] - d[2] * v[1],
                d[2] * v[0] - d[0] * v[2],
                d[0] * v[1] - d[1] * v[0],
            ]),
            _ => Err("relación soportada: perpendicular u parallel".into()),
        }
    };

    match linearized_parameter_system(&params, &document.variables, equations) {
        Ok((Some(a), Some(b))) => {
            CommandOutcome::Message(format_parameter_solution(&params, &a, &b))
        }
        Ok((None, None)) => CommandOutcome::Message(format!(
            "All parameter values satisfy {} for [{}]",
            relation,
            params.join(", ")
        )),
        Ok(_) => CommandOutcome::Error("SolveLine3DParameters: sistema interno inválido".into()),
        Err(e) => CommandOutcome::Error(format!("SolveLine3DParameters: {e}")),
    }
}

fn run_matrix_param_solve(
    matrix_text: &str,
    param_text: &str,
    document: &Document,
) -> CommandOutcome {
    let rows = match parse_expression_matrix_arg(matrix_text) {
        Ok(rows) => rows,
        Err(e) => return CommandOutcome::Error(format!("MatrixParamSolve: {e}")),
    };
    if rows.len() != rows[0].len() {
        return CommandOutcome::Error("MatrixParamSolve: la matriz debe ser cuadrada".into());
    }
    const MAX_SYMBOLIC_DETERMINANT_ORDER: usize = 8;
    if rows.len() > MAX_SYMBOLIC_DETERMINANT_ORDER {
        return CommandOutcome::Error(format!(
            "MatrixParamSolve: orden {} exceeds maximum {} para expansión simbólica",
            rows.len(),
            MAX_SYMBOLIC_DETERMINANT_ORDER
        ));
    }
    let param = clean_symbol_arg(param_text);
    if !is_valid_parameter_name(&param) {
        return CommandOutcome::Error("MatrixParamSolve: parámetro inválido".into());
    }
    let mut det_expr = determinant_expression(&rows);
    for (name, value) in &document.variables {
        if name != &param && value.is_finite() {
            det_expr = replace_variable(&det_expr, name, &format!("({value})"));
        }
    }

    let mut vars_zero = document.variables.clone();
    vars_zero.insert(param.clone(), 0.0);
    let roots = find_real_roots_scan(
        |x| {
            let mut vars = document.variables.clone();
            vars.insert(param.clone(), x);
            let matrix = evaluate_expression_matrix(&rows, &vars).ok()?;
            matrix.determinant()
        },
        -100.0,
        100.0,
        8000,
        1e-10,
    );
    let symbolic_roots = symbolic::solve(&det_expr, &param)
        .unwrap_or_else(|e| format!("no se pudo resolver simbólicamente: {e}"));
    let roots_text = if roots.is_empty() {
        "[]".to_string()
    } else {
        roots
            .iter()
            .map(|r| fmt_scalar(*r))
            .collect::<Vec<_>>()
            .join(", ")
    };
    let rank_generic = evaluate_expression_matrix(&rows, &vars_zero)
        .ok()
        .and_then(|m| rank(&m))
        .map(|r| r.to_string())
        .unwrap_or_else(|| "n/d".into());
    CommandOutcome::Message(format!(
        "det(A) = {det_expr}\nSingular parameter values ({param}) in [-100,100]: [{roots_text}]\nSymbolic solve: {symbolic_roots}\nrank at {param}=0: {rank_generic}"
    ))
}

fn determinant_expression(rows: &[Vec<String>]) -> String {
    let n = rows.len();
    if n == 1 {
        return rows[0][0].clone();
    }
    let mut terms = Vec::new();
    for col in 0..n {
        let mut minor = Vec::with_capacity(n - 1);
        for source_row in rows.iter().skip(1) {
            let mut row = Vec::with_capacity(n - 1);
            for (c, entry) in source_row.iter().enumerate() {
                if c != col {
                    row.push(entry.clone());
                }
            }
            minor.push(row);
        }
        let term = format!("({})*({})", rows[0][col], determinant_expression(&minor));
        if col % 2 == 0 {
            terms.push(term);
        } else {
            terms.push(format!("-({term})"));
        }
    }
    terms.join("+").replace("+-", "-")
}

fn matrix_rows(m: &Matrix) -> Vec<Vec<f64>> {
    (0..m.rows)
        .map(|r| (0..m.cols).map(|c| m.get(r, c)).collect())
        .collect()
}

#[allow(clippy::unwrap_used)]
fn independent_row_indices(rows: &[Vec<f64>]) -> Vec<usize> {
    let mut selected = Vec::new();
    let mut current_rank = 0;
    let mut current_rows: Vec<Vec<f64>> = Vec::new();
    for (idx, row) in rows.iter().enumerate() {
        current_rows.push(row.clone());
        let candidate = Matrix::from_rows(current_rows.clone()).unwrap();
        let r = rank(&candidate).unwrap_or(0);
        if r > current_rank {
            current_rank = r;
            selected.push(idx);
        } else {
            current_rows.pop();
        }
    }
    selected
}

fn independent_rows(rows: &[Vec<f64>]) -> Vec<Vec<f64>> {
    let indices = independent_row_indices(rows);
    indices.into_iter().map(|i| rows[i].clone()).collect()
}

fn subspace_intersection_basis(u: &Matrix, v: &Matrix) -> Vec<Vec<f64>> {
    let n = u.cols;
    let k = u.rows;
    let m = v.rows;
    let mut rows = Vec::with_capacity(n);
    for coord in 0..n {
        let mut row = Vec::with_capacity(k + m);
        for i in 0..k {
            row.push(u.get(i, coord));
        }
        for j in 0..m {
            row.push(-v.get(j, coord));
        }
        rows.push(row);
    }
    let Some(system) = Matrix::from_rows(rows) else {
        return Vec::new();
    };
    let relations = null_space(&system).unwrap_or_default();
    let u_rows = matrix_rows(u);
    let mut candidates = Vec::new();
    for rel in relations {
        let mut vec = vec![0.0; n];
        for i in 0..k {
            for (coord, value) in vec.iter_mut().enumerate() {
                *value += rel[i] * u_rows[i][coord];
            }
        }
        if vec.iter().any(|x| x.abs() > 1e-8) {
            candidates.push(vec);
        }
    }
    independent_rows(&candidates)
}

#[derive(Clone)]
struct SurfaceParamSpec {
    coords: Vec<String>,
    vars: Vec<String>,
    u0: f64,
    u1: f64,
    v0: f64,
    v1: f64,
    n: usize,
}

struct Lagrange2System<'a> {
    fx: &'a str,
    fy: &'a str,
    gx: &'a str,
    gy: &'a str,
    constraint: &'a str,
    vars: &'a [String],
    document: &'a Document,
}

fn run_gauss_jordan_command(args: &[String], document: &Document) -> CommandOutcome {
    let matrix = match parse_matrix_arg_strict(&args[0], &document.variables) {
        Ok(m) => m,
        Err(e) => return CommandOutcome::Error(format!("GaussJordan: {e}")),
    };
    let (rref, pivots) = rref_with_pivots(&matrix, 1e-10);
    CommandOutcome::Message(format!("GaussJordan RREF:\n{}pivots = {:?}", rref, pivots))
}

fn run_gauss_jordan_solve_command(args: &[String], document: &Document) -> CommandOutcome {
    let a = match parse_matrix_arg_strict(&args[0], &document.variables) {
        Ok(m) => m,
        Err(e) => return CommandOutcome::Error(format!("GaussJordanSolve: {e}")),
    };
    let b = match parse_vector_or_matrix_arg(&args[1], &document.variables) {
        Ok(m) => m,
        Err(e) => return CommandOutcome::Error(format!("GaussJordanSolve: {e}")),
    };
    if b.cols != 1 || b.rows != a.rows {
        return CommandOutcome::Error("GaussJordanSolve: dimensiones incompatibles".into());
    }
    let aug = augment_matrix(&a, &b);
    let (rref, pivots) = rref_with_pivots_partitioned(&aug, a.cols, 1e-10);
    if has_inconsistent_augmented_row(&rref, a.cols, 1e-10) {
        return CommandOutcome::Message(format!("GaussJordanSolve: no solution\nRREF:\n{rref}"));
    }
    if coefficient_pivot_count(&pivots, a.cols) < a.cols {
        let solution = particular_solution_from_augmented_rref(&rref, &pivots, a.cols);
        let basis = null_space_from_rref(&rref, &pivots, a.cols);
        return CommandOutcome::Message(format!(
            "GaussJordanSolve: Infinite solutions\nx0 = {}\nbasis Ker(A) = {}\nRREF:\n{}",
            fmt_vector(&solution),
            fmt_vector_basis(&basis),
            rref
        ));
    }
    let solution = particular_solution_from_augmented_rref(&rref, &pivots, a.cols);
    CommandOutcome::Message(format!(
        "GaussJordanSolve: x = {}\nRREF:\n{}",
        fmt_vector(&solution),
        rref
    ))
}

fn run_cramer_command(args: &[String], document: &Document) -> CommandOutcome {
    let a = match parse_matrix_arg_strict(&args[0], &document.variables) {
        Ok(m) => m,
        Err(e) => return CommandOutcome::Error(format!("Cramer: {e}")),
    };
    let b = match parse_vector_or_matrix_arg(&args[1], &document.variables) {
        Ok(m) => m,
        Err(e) => return CommandOutcome::Error(format!("Cramer: {e}")),
    };
    if a.rows != a.cols || b.cols != 1 || b.rows != a.rows {
        return CommandOutcome::Error("Cramer: A debe ser cuadrada y b vector columna".into());
    }
    let Some(det_a) = a.determinant() else {
        return CommandOutcome::Error("Cramer: determinante no disponible".into());
    };
    if !det_a.is_finite() || det_a == 0.0 {
        return CommandOutcome::Error("Cramer: det(A)=0".into());
    }
    let mut solution = Vec::with_capacity(a.cols);
    let mut details = Vec::with_capacity(a.cols);
    for col in 0..a.cols {
        let replaced = replace_matrix_column(&a, col, &b);
        let det_i = replaced.determinant().unwrap_or(f64::NAN);
        let value = det_i / det_a;
        if !value.is_finite() {
            return CommandOutcome::Error(
                "Cramer: el cociente de determinantes no es finito".into(),
            );
        }
        solution.push(value);
        details.push(format!("det A{}={}", col + 1, fmt_scalar(det_i)));
    }
    CommandOutcome::Message(format!(
        "Cramer: det(A)={}; x = {}; {}",
        fmt_scalar(det_a),
        fmt_vector(&solution),
        details.join(", ")
    ))
}

fn run_cofactor_command(args: &[String], document: &Document) -> CommandOutcome {
    let matrix = match parse_matrix_arg_strict(&args[0], &document.variables) {
        Ok(m) => m,
        Err(e) => return CommandOutcome::Error(format!("Cofactor: {e}")),
    };
    let row = match args[1].trim().parse::<usize>() {
        Ok(value) if value > 0 => value - 1,
        _ => return CommandOutcome::Error("Cofactor: fila debe ser un entero positivo".into()),
    };
    let col = match args[2].trim().parse::<usize>() {
        Ok(value) if value > 0 => value - 1,
        _ => return CommandOutcome::Error("Cofactor: columna debe ser un entero positivo".into()),
    };
    let Some(value) = cofactor_value(&matrix, row, col) else {
        return CommandOutcome::Error("Cofactor: indices o matriz invalidos".into());
    };
    CommandOutcome::Message(format!(
        "Cofactor C_{}_{} = {}",
        row + 1,
        col + 1,
        fmt_scalar(value)
    ))
}

fn run_adjugate_command(args: &[String], document: &Document) -> CommandOutcome {
    let matrix = match parse_matrix_arg_strict(&args[0], &document.variables) {
        Ok(m) => m,
        Err(e) => return CommandOutcome::Error(format!("Adjugate: {e}")),
    };
    let Some(adj) = adjugate_matrix(&matrix) else {
        return CommandOutcome::Error("Adjugate: la matriz debe ser cuadrada".into());
    };
    CommandOutcome::Message(format!("Adjugate:\n{adj}"))
}

fn run_laplace_expansion_command(args: &[String], document: &Document) -> CommandOutcome {
    let matrix = match parse_matrix_arg_strict(&args[0], &document.variables) {
        Ok(m) => m,
        Err(e) => return CommandOutcome::Error(format!("LaplaceExpansion: {e}")),
    };
    if matrix.rows != matrix.cols {
        return CommandOutcome::Error("LaplaceExpansion: matriz cuadrada requerida".into());
    }
    let by_row = !args[1].trim().eq_ignore_ascii_case("col");
    let index = match args[2].trim().parse::<usize>() {
        Ok(value) if value > 0 => value - 1,
        _ => {
            return CommandOutcome::Error(
                "LaplaceExpansion: índice debe ser un entero positivo".into(),
            )
        }
    };
    if (by_row && index >= matrix.rows) || (!by_row && index >= matrix.cols) {
        return CommandOutcome::Error("LaplaceExpansion: indice fuera de rango".into());
    }
    let mut terms = Vec::new();
    let mut det = 0.0;
    let len = if by_row { matrix.cols } else { matrix.rows };
    for k in 0..len {
        let (r, c) = if by_row { (index, k) } else { (k, index) };
        let value = matrix.get(r, c);
        let cof = cofactor_value(&matrix, r, c).unwrap_or(0.0);
        det += value * cof;
        terms.push(format!("{}*{}", fmt_scalar(value), fmt_scalar(cof)));
    }
    CommandOutcome::Message(format!(
        "LaplaceExpansion: det = {} = {}",
        fmt_scalar(det),
        terms.join(" + ")
    ))
}

fn run_change_of_basis_command(args: &[String], document: &Document) -> CommandOutcome {
    let v = match parse_numeric_vector_arg(&args[0], &document.variables) {
        Ok(v) => v,
        Err(e) => return CommandOutcome::Error(format!("ChangeOfBasis: {e}")),
    };
    let from = match parse_matrix_arg_strict(&args[1], &document.variables) {
        Ok(m) => m,
        Err(e) => return CommandOutcome::Error(format!("ChangeOfBasis: {e}")),
    };
    let to = match parse_matrix_arg_strict(&args[2], &document.variables) {
        Ok(m) => m,
        Err(e) => return CommandOutcome::Error(format!("ChangeOfBasis: {e}")),
    };
    if from.rows != v.len() || from.cols != v.len() || to.rows != v.len() || to.cols != v.len() {
        return CommandOutcome::Error("ChangeOfBasis: dimensiones incompatibles".into());
    }
    let standard = multiply_matrix_vector(&from.transpose(), &v);
    let Some(std_col) = Matrix::from_rows(standard.iter().map(|x| vec![*x]).collect()) else {
        return CommandOutcome::Error("ChangeOfBasis: vector invalido".into());
    };
    let Some(coords) = solve_linear_system(&to.transpose(), &std_col) else {
        return CommandOutcome::Error("ChangeOfBasis: base destino singular".into());
    };
    CommandOutcome::Message(format!(
        "ChangeOfBasis: standard = {}; coordinates = {}",
        fmt_vector(&standard),
        fmt_column_matrix_as_vector(&coords)
    ))
}

fn run_linear_transformation_matrix_command(
    args: &[String],
    document: &Document,
) -> CommandOutcome {
    let basis = match parse_matrix_arg_strict(&args[0], &document.variables) {
        Ok(m) => m,
        Err(e) => return CommandOutcome::Error(format!("LinearTransformationMatrix: {e}")),
    };
    let outputs = match parse_matrix_arg_strict(&args[1], &document.variables) {
        Ok(m) => m,
        Err(e) => return CommandOutcome::Error(format!("LinearTransformationMatrix: {e}")),
    };
    if basis.rows != basis.cols || outputs.rows != basis.rows || outputs.cols != basis.cols {
        return CommandOutcome::Error(
            "LinearTransformationMatrix: dimensiones incompatibles".into(),
        );
    }
    let Some(inv) = basis.transpose().inverse() else {
        return CommandOutcome::Error("LinearTransformationMatrix: base singular".into());
    };
    let Some(matrix) = outputs.transpose().mul(&inv) else {
        return CommandOutcome::Error("LinearTransformationMatrix: no se pudo calcular".into());
    };
    CommandOutcome::Message(format!("LinearTransformationMatrix:\n{matrix}"))
}

fn run_diagonalization_command(args: &[String], document: &Document) -> CommandOutcome {
    let matrix = match parse_matrix_arg_strict(&args[0], &document.variables) {
        Ok(m) => m,
        Err(e) => return CommandOutcome::Error(format!("Diagonalization: {e}")),
    };
    if matrix.rows != matrix.cols {
        return CommandOutcome::Error("Diagonalization: matriz cuadrada requerida".into());
    }
    let Some(vectors) = eigenvectors(&matrix) else {
        return CommandOutcome::Error("Diagonalization: no hay autovectores suficientes".into());
    };
    let real_vectors = vectors
        .iter()
        .filter(|(_, _, im)| im.abs() < 1e-9)
        .collect::<Vec<_>>();
    if real_vectors.len() < matrix.rows {
        return CommandOutcome::Error("Diagonalization: no hay base real completa".into());
    }
    let p_cols = real_vectors
        .iter()
        .take(matrix.rows)
        .map(|(v, _, _)| v.clone())
        .collect::<Vec<_>>();
    let Some(p) = matrix_from_columns(&p_cols) else {
        return CommandOutcome::Error("Diagonalization: no se pudo construir la base real".into());
    };
    let d_rows = (0..matrix.rows)
        .map(|r| {
            (0..matrix.rows)
                .map(|c| if r == c { real_vectors[r].1 } else { 0.0 })
                .collect::<Vec<_>>()
        })
        .collect::<Vec<_>>();
    let Some(d) = Matrix::from_rows(d_rows) else {
        return CommandOutcome::Error(
            "Diagonalization: no se pudo construir la matriz diagonal".into(),
        );
    };
    let Some(p_inverse) = p.inverse() else {
        return CommandOutcome::Error("Diagonalization: no hay base real completa".into());
    };
    let Some(reconstructed) = p.mul(&d).and_then(|pd| pd.mul(&p_inverse)) else {
        return CommandOutcome::Error(
            "Diagonalization: no se pudo verificar la descomposición".into(),
        );
    };
    let scale = matrix
        .data
        .iter()
        .chain(&reconstructed.data)
        .map(|value| value.abs())
        .fold(0.0_f64, f64::max);
    let residual = matrix
        .data
        .iter()
        .zip(&reconstructed.data)
        .map(|(actual, expected)| (actual - expected).abs())
        .fold(0.0_f64, f64::max);
    let tolerance = 1024.0 * f64::EPSILON * matrix.rows as f64 * scale;
    if !scale.is_finite() || !residual.is_finite() || residual > tolerance {
        return CommandOutcome::Error(
            "Diagonalization: no se pudo verificar la descomposición".into(),
        );
    }
    CommandOutcome::Message(format!("Diagonalization: A = P*D*P^-1\nP:\n{}D:\n{}", p, d))
}

fn fmt_symbolic_vector(values: &[String]) -> String {
    format!("[{}]", values.join(", "))
}

fn fmt_symbolic_matrix(rows: &[Vec<String>]) -> String {
    format!(
        "[{}]",
        rows.iter()
            .map(|row| fmt_symbolic_vector(row))
            .collect::<Vec<_>>()
            .join(", ")
    )
}

fn parse_quadrature_n(
    raw: Option<&String>,
    default: usize,
    min: usize,
    max: usize,
) -> Result<usize, String> {
    let Some(raw) = raw else {
        return Ok(default);
    };
    let value = raw
        .trim()
        .parse::<usize>()
        .map_err(|_| format!("n debe ser un entero entre {min} y {max}"))?;
    if (min..=max).contains(&value) {
        Ok(value)
    } else {
        Err(format!("n debe estar entre {min} y {max}"))
    }
}

fn eval_at_vars(
    expr: &str,
    vars: &[String],
    values: &[f64],
    document: &Document,
) -> Result<f64, String> {
    if vars.len() != values.len() {
        return Err("variables y valores con dimensiones distintas".into());
    }
    let assignments = vars
        .iter()
        .zip(values.iter())
        .map(|(var, value)| (var.as_str(), *value))
        .collect::<Vec<_>>();
    eval_multivar_expr(expr, &document.variables, &assignments)
}

fn eval_param_values(
    exprs: &[String],
    var: &str,
    value: f64,
    document: &Document,
) -> Result<Vec<f64>, String> {
    exprs
        .iter()
        .map(|expr| eval_multivar_expr(expr, &document.variables, &[(var, value)]))
        .collect()
}

fn eval_fields_at(
    fields: &[String],
    vars: &[String],
    coords: &[f64],
    document: &Document,
) -> Result<Vec<f64>, String> {
    fields
        .iter()
        .map(|field| eval_at_vars(field, vars, coords, document))
        .collect()
}

fn dot(a: &[f64], b: &[f64]) -> f64 {
    a.iter().zip(b.iter()).map(|(x, y)| x * y).sum()
}

fn norm(v: &[f64]) -> f64 {
    dot(v, v).sqrt()
}

fn cross(a: &[f64], b: &[f64]) -> Vec<f64> {
    vec![
        a[1] * b[2] - a[2] * b[1],
        a[2] * b[0] - a[0] * b[2],
        a[0] * b[1] - a[1] * b[0],
    ]
}

fn parse_surface_param_args(
    args: &[String],
    document: &Document,
    command: &str,
) -> Result<SurfaceParamSpec, CommandOutcome> {
    let coords = match parse_expression_vector_arg(&args[1]) {
        Ok(v) if v.len() == 3 => v,
        Ok(_) => {
            return Err(CommandOutcome::Error(format!(
                "{command}: superficie 3D requerida"
            )))
        }
        Err(e) => return Err(CommandOutcome::Error(format!("{command}: {e}"))),
    };
    let vars = match parse_expression_vector_arg(&args[2]) {
        Ok(v) if v.len() == 2 => v.into_iter().map(|s| clean_symbol_arg(&s)).collect(),
        Ok(_) => {
            return Err(CommandOutcome::Error(format!(
                "{command}: variables [u,v] requeridas"
            )))
        }
        Err(e) => return Err(CommandOutcome::Error(format!("{command}: {e}"))),
    };
    let parse_bound = |index: usize, name: &str| {
        require_finite(parse_numeric_arg(&args[index], &document.variables))
            .map_err(|error| CommandOutcome::Error(format!("{command}: {name} inválido: {error}")))
    };
    let u0 = parse_bound(3, "u0")?;
    let u1 = parse_bound(4, "u1")?;
    let v0 = parse_bound(5, "v0")?;
    let v1 = parse_bound(6, "v1")?;
    let n = parse_quadrature_n(args.get(7), 40, 2, 300)
        .map_err(|error| CommandOutcome::Error(format!("{command}: {error}")))?;
    Ok(SurfaceParamSpec {
        coords,
        vars,
        u0,
        u1,
        v0,
        v1,
        n,
    })
}

fn integrate_parametric_surface<F>(
    params: &SurfaceParamSpec,
    document: &Document,
    integrand: F,
) -> Result<f64, String>
where
    F: Fn(&[f64], &[f64]) -> Result<f64, String>,
{
    let du_exprs = params
        .coords
        .iter()
        .map(|c| symbolic_partial(c, &params.vars[0]).unwrap_or_else(|_| "0".into()))
        .collect::<Vec<_>>();
    let dv_exprs = params
        .coords
        .iter()
        .map(|c| symbolic_partial(c, &params.vars[1]).unwrap_or_else(|_| "0".into()))
        .collect::<Vec<_>>();
    let du = (params.u1 - params.u0) / params.n as f64;
    let dv = (params.v1 - params.v0) / params.n as f64;
    let mut total = 0.0;
    for i in 0..params.n {
        let u = params.u0 + (i as f64 + 0.5) * du;
        for j in 0..params.n {
            let v = params.v0 + (j as f64 + 0.5) * dv;
            let values = [u, v];
            let coords = eval_surface_exprs(&params.coords, &params.vars, &values, document)?;
            let ru = eval_surface_exprs(&du_exprs, &params.vars, &values, document)?;
            let rv = eval_surface_exprs(&dv_exprs, &params.vars, &values, document)?;
            let normal = cross(&ru, &rv);
            total += integrand(&coords, &normal)? * du * dv;
        }
    }
    Ok(total)
}

fn eval_surface_exprs(
    exprs: &[String],
    vars: &[String],
    values: &[f64],
    document: &Document,
) -> Result<Vec<f64>, String> {
    exprs
        .iter()
        .map(|expr| eval_at_vars(expr, vars, values, document))
        .collect()
}

fn flux_over_parametric_surface(
    fields: &[String],
    params: &SurfaceParamSpec,
    document: &Document,
) -> Result<f64, String> {
    integrate_parametric_surface(params, document, |coords, normal| {
        let field_vars = default_multivar_names(3);
        let fvals = eval_fields_at(fields, &field_vars, coords, document)?;
        Ok(dot(&fvals, normal))
    })
}

fn conservative_pair_equal(p: &str, q: &str, x: &str, y: &str) -> bool {
    partial_derivatives_equal(p, y, q, x)
}

fn partial_derivatives_equal(left: &str, left_var: &str, right: &str, right_var: &str) -> bool {
    if !symbolic::is_everywhere_differentiable(left).unwrap_or(false)
        || !symbolic::is_everywhere_differentiable(right).unwrap_or(false)
    {
        return false;
    }
    let Ok(mut left_derivative) = symbolic_partial(left, left_var) else {
        return false;
    };
    let Ok(mut right_derivative) = symbolic_partial(right, right_var) else {
        return false;
    };
    left_derivative = remove_identically_zero_addends(&left_derivative).unwrap_or(left_derivative);
    right_derivative =
        remove_identically_zero_addends(&right_derivative).unwrap_or(right_derivative);
    if symbolic::structurally_equal(&left_derivative, &right_derivative).unwrap_or(false) {
        return true;
    }
    let diff = simplified_difference(&left_derivative, &right_derivative);
    symbolic::simplify(&diff).unwrap_or(diff) == "0"
}

fn remove_identically_zero_addends(expression: &str) -> Result<String, String> {
    use grafito_geometry::ast::Expr;

    fn collect_addends(expression: &Expr, negative: bool, addends: &mut Vec<(bool, String)>) {
        if symbolic::is_identically_zero(expression) {
            return;
        }
        match expression {
            Expr::Add(left, right) => {
                collect_addends(left, negative, addends);
                collect_addends(right, negative, addends);
            }
            Expr::Sub(left, right) => {
                collect_addends(left, negative, addends);
                collect_addends(right, !negative, addends);
            }
            Expr::Neg(value) => collect_addends(value, !negative, addends),
            _ => addends.push((negative, expression.to_expr_string())),
        }
    }

    let expanded = symbolic::expand(expression)?;
    let ast = grafito_geometry::ast::parse_ast(&expanded)?;
    let mut addends = Vec::new();
    collect_addends(&ast, false, &mut addends);
    let normalized = addends
        .into_iter()
        .enumerate()
        .map(|(index, (negative, addend))| match (index, negative) {
            (0, false) => addend,
            (0, true) => format!("-({addend})"),
            (_, false) => format!("+({addend})"),
            (_, true) => format!("-({addend})"),
        })
        .collect::<String>();
    if normalized.is_empty() {
        Ok("0".into())
    } else {
        symbolic::simplify(&normalized)
    }
}

fn integrate_for_potential(expr: &str, var: &str) -> String {
    let compact = expr.replace(' ', "");
    if compact == "0" {
        return "0".into();
    }
    if compact == var {
        return format!("0.5*{var}^2");
    }
    if compact == format!("sin({var})") {
        return format!("-cos({var})");
    }
    if compact == format!("cos({var})") {
        return format!("sin({var})");
    }
    if compact == format!("exp({var})") {
        return format!("exp({var})");
    }
    if let Some(result) = integrate_polynomial_like(&compact, var) {
        return result;
    }
    format!("Integral({compact}, d{var})")
}

fn integrate_polynomial_like(expr: &str, var: &str) -> Option<String> {
    let terms = split_additive_terms(expr);
    if terms.is_empty() {
        return None;
    }
    let mut out = Vec::with_capacity(terms.len());
    for (sign, term) in terms {
        let factors = term
            .split('*')
            .filter(|s| !s.is_empty())
            .collect::<Vec<_>>();
        let mut coef = sign;
        let mut power = 0.0;
        let mut rest = Vec::new();
        for factor in factors {
            if let Ok(v) = factor.parse::<f64>() {
                coef *= v;
            } else if factor == var {
                power += 1.0;
            } else if let Some(exp) = factor.strip_prefix(&format!("{var}^")) {
                power += exp.parse::<f64>().ok()?;
            } else if factor.contains(var) {
                return None;
            } else {
                rest.push(factor.to_string());
            }
        }
        let new_power = power + 1.0;
        if new_power.abs() < 1e-12 {
            return None;
        }
        let new_coef = coef / new_power;
        let mut pieces = Vec::new();
        if (new_coef - 1.0).abs() > 1e-12 || (power == 0.0 && rest.is_empty()) {
            pieces.push(fmt_scalar(new_coef));
        } else if (new_coef + 1.0).abs() < 1e-12 {
            pieces.push("-1".into());
        }
        if (new_power - 1.0).abs() < 1e-12 {
            pieces.push(var.to_string());
        } else {
            pieces.push(format!("{var}^{}", fmt_scalar(new_power)));
        }
        pieces.extend(rest);
        out.push(pieces.join("*"));
    }
    Some(out.join(" + ").replace("+ -", "- "))
}

fn split_additive_terms(expr: &str) -> Vec<(f64, String)> {
    let mut terms = Vec::new();
    let mut depth = 0i32;
    let mut start = 0usize;
    let mut sign = 1.0;
    for (idx, ch) in expr.char_indices() {
        match ch {
            '(' | '[' | '{' => depth += 1,
            ')' | ']' | '}' => depth -= 1,
            '+' | '-' if depth == 0 && idx > start => {
                let term = expr[start..idx].trim();
                if !term.is_empty() {
                    terms.push((sign, term.to_string()));
                }
                sign = if ch == '-' { -1.0 } else { 1.0 };
                start = idx + ch.len_utf8();
            }
            '-' if depth == 0 && idx == start => {
                sign = -1.0;
                start = idx + ch.len_utf8();
            }
            _ => {}
        }
    }
    let term = expr[start..].trim();
    if !term.is_empty() {
        terms.push((sign, term.to_string()));
    }
    terms
}

fn curl_components_3d(fields: &[String], vars: &[String]) -> Result<Vec<String>, String> {
    if fields.len() != 3 || vars.len() != 3 {
        return Err("campo y variables 3D requeridos".into());
    }
    let dr_dy = symbolic_partial(&fields[2], &vars[1])?;
    let dq_dz = symbolic_partial(&fields[1], &vars[2])?;
    let dp_dz = symbolic_partial(&fields[0], &vars[2])?;
    let dr_dx = symbolic_partial(&fields[2], &vars[0])?;
    let dq_dx = symbolic_partial(&fields[1], &vars[0])?;
    let dp_dy = symbolic_partial(&fields[0], &vars[1])?;
    Ok(vec![
        simplified_difference(&dr_dy, &dq_dz),
        simplified_difference(&dp_dz, &dr_dx),
        simplified_difference(&dq_dx, &dp_dy),
    ])
}

fn newton2_for_system(
    f: &str,
    g: &str,
    vars: &[String],
    seed: [f64; 2],
    document: &Document,
) -> Option<(f64, f64)> {
    let fx = symbolic_partial(f, &vars[0]).ok()?;
    let fy = symbolic_partial(f, &vars[1]).ok()?;
    let gx = symbolic_partial(g, &vars[0]).ok()?;
    let gy = symbolic_partial(g, &vars[1]).ok()?;
    let mut x = seed[0];
    let mut y = seed[1];
    for _ in 0..30 {
        let values = [x, y];
        let fv = eval_at_vars(f, vars, &values, document).ok()?;
        let gv = eval_at_vars(g, vars, &values, document).ok()?;
        if fv.hypot(gv) < 1e-9 {
            return Some((x, y));
        }
        let a = eval_at_vars(&fx, vars, &values, document).ok()?;
        let b = eval_at_vars(&fy, vars, &values, document).ok()?;
        let c = eval_at_vars(&gx, vars, &values, document).ok()?;
        let d = eval_at_vars(&gy, vars, &values, document).ok()?;
        let det = a * d - b * c;
        if det.abs() < 1e-12 {
            return None;
        }
        let dx = (-fv * d + b * gv) / det;
        let dy = (c * fv - a * gv) / det;
        x += dx;
        y += dy;
        if !x.is_finite() || !y.is_finite() || dx.hypot(dy) < 1e-10 {
            break;
        }
    }
    let values = [x, y];
    let fv = eval_at_vars(f, vars, &values, document).ok()?;
    let gv = eval_at_vars(g, vars, &values, document).ok()?;
    (fv.hypot(gv) < 1e-6).then_some((x, y))
}

fn newton_lagrange2(system: &Lagrange2System<'_>, seed: [f64; 3]) -> Option<(f64, f64, f64)> {
    let mut x = seed[0];
    let mut y = seed[1];
    let mut lambda = seed[2];
    for _ in 0..40 {
        let values = [x, y];
        let f1 = eval_at_vars(system.fx, system.vars, &values, system.document).ok()?
            - lambda * eval_at_vars(system.gx, system.vars, &values, system.document).ok()?;
        let f2 = eval_at_vars(system.fy, system.vars, &values, system.document).ok()?
            - lambda * eval_at_vars(system.gy, system.vars, &values, system.document).ok()?;
        let f3 = eval_at_vars(system.constraint, system.vars, &values, system.document).ok()?;
        if norm(&[f1, f2, f3]) < 1e-8 {
            return Some((x, y, lambda));
        }
        let h = 1e-5;
        let jac = numeric_jacobian3(
            |xx, yy, ll| {
                let vals = [xx, yy];
                Some([
                    eval_at_vars(system.fx, system.vars, &vals, system.document).ok()?
                        - ll * eval_at_vars(system.gx, system.vars, &vals, system.document).ok()?,
                    eval_at_vars(system.fy, system.vars, &vals, system.document).ok()?
                        - ll * eval_at_vars(system.gy, system.vars, &vals, system.document).ok()?,
                    eval_at_vars(system.constraint, system.vars, &vals, system.document).ok()?,
                ])
            },
            x,
            y,
            lambda,
            h,
        )?;
        let a = Matrix::from_rows(jac.iter().map(|row| row.to_vec()).collect())?;
        let b = Matrix::from_rows(vec![vec![-f1], vec![-f2], vec![-f3]])?;
        let step = solve_linear_system(&a, &b)?;
        x += step.get(0, 0);
        y += step.get(1, 0);
        lambda += step.get(2, 0);
        if !x.is_finite() || !y.is_finite() || !lambda.is_finite() {
            return None;
        }
    }
    None
}

fn numeric_jacobian3<F>(f: F, x: f64, y: f64, l: f64, h: f64) -> Option<[[f64; 3]; 3]>
where
    F: Fn(f64, f64, f64) -> Option<[f64; 3]>,
{
    let px = f(x + h, y, l)?;
    let mx = f(x - h, y, l)?;
    let py = f(x, y + h, l)?;
    let my = f(x, y - h, l)?;
    let pl = f(x, y, l + h)?;
    let ml = f(x, y, l - h)?;
    let mut jac = [[0.0; 3]; 3];
    for row in 0..3 {
        jac[row][0] = (px[row] - mx[row]) / (2.0 * h);
        jac[row][1] = (py[row] - my[row]) / (2.0 * h);
        jac[row][2] = (pl[row] - ml[row]) / (2.0 * h);
    }
    Some(jac)
}

fn classify_hessian_point(
    fxx: &str,
    fxy: &str,
    fyy: &str,
    vars: &[String],
    values: [f64; 2],
    document: &Document,
) -> &'static str {
    let a = eval_at_vars(fxx, vars, &values, document).unwrap_or(f64::NAN);
    let b = eval_at_vars(fxy, vars, &values, document).unwrap_or(f64::NAN);
    let c = eval_at_vars(fyy, vars, &values, document).unwrap_or(f64::NAN);
    let det = a * c - b * b;
    if det > 1e-8 && a > 0.0 {
        "minimum"
    } else if det > 1e-8 && a < 0.0 {
        "maximum"
    } else if det < -1e-8 {
        "saddle"
    } else {
        "inconclusive"
    }
}

fn integrate_1d_midpoint(
    expr: &str,
    var: &str,
    a: f64,
    b: f64,
    n: usize,
    document: &Document,
) -> Result<f64, String> {
    let dx = (b - a) / n as f64;
    let mut total = 0.0;
    for i in 0..n {
        let x = a + (i as f64 + 0.5) * dx;
        total += eval_multivar_expr(expr, &document.variables, &[(var, x)])? * dx;
    }
    Ok(total)
}

fn bisection_root(expr: &str, var: &str, a: f64, b: f64, document: &Document) -> Option<f64> {
    let mut lo = a;
    let mut hi = b;
    let mut flo = eval_multivar_expr(expr, &document.variables, &[(var, lo)]).ok()?;
    let fhi = eval_multivar_expr(expr, &document.variables, &[(var, hi)]).ok()?;
    if flo.abs() < 1e-10 {
        return Some(lo);
    }
    if fhi.abs() < 1e-10 {
        return Some(hi);
    }
    if flo * fhi > 0.0 {
        return None;
    }
    for _ in 0..80 {
        let mid = 0.5 * (lo + hi);
        let fmid = eval_multivar_expr(expr, &document.variables, &[(var, mid)]).ok()?;
        if fmid.abs() < 1e-10 || (hi - lo).abs() < 1e-10 {
            return Some(mid);
        }
        if flo * fmid <= 0.0 {
            hi = mid;
        } else {
            lo = mid;
            flo = fmid;
        }
    }
    Some(0.5 * (lo + hi))
}

fn find_roots_in_interval(
    expr: &str,
    var: &str,
    a: f64,
    b: f64,
    document: &Document,
    samples: usize,
) -> Vec<f64> {
    let mut roots = Vec::new();
    let mut prev_x = a;
    let mut prev_y =
        eval_multivar_expr(expr, &document.variables, &[(var, prev_x)]).unwrap_or(f64::NAN);
    for i in 1..=samples {
        let x = a + (b - a) * i as f64 / samples as f64;
        let y = eval_multivar_expr(expr, &document.variables, &[(var, x)]).unwrap_or(f64::NAN);
        if prev_y.is_finite() && y.is_finite() && prev_y * y <= 0.0 {
            if let Some(root) = bisection_root(expr, var, prev_x, x, document) {
                if !roots.iter().any(|r: &f64| (*r - root).abs() < 1e-5) {
                    roots.push(root);
                }
            }
        }
        prev_x = x;
        prev_y = y;
    }
    roots
}

fn interval_samples_are_finite(expr: &str, var: &str, a: f64, b: f64, document: &Document) -> bool {
    const CONTINUITY_SAMPLES: usize = 128;
    a.is_finite()
        && b.is_finite()
        && (0..=CONTINUITY_SAMPLES).all(|i| {
            let x = a + (b - a) * i as f64 / CONTINUITY_SAMPLES as f64;
            eval_multivar_expr(expr, &document.variables, &[(var, x)]).is_ok_and(f64::is_finite)
        })
}

fn symmetric_quotient_limit(
    num: &str,
    den: &str,
    var: &str,
    at: f64,
    document: &Document,
) -> Option<f64> {
    let hs = [1e-3, 5e-4, 1e-4, 5e-5];
    let mut values = Vec::new();
    for h in hs {
        for x in [at - h, at + h] {
            let n = eval_multivar_expr(num, &document.variables, &[(var, x)]).ok()?;
            let d = eval_multivar_expr(den, &document.variables, &[(var, x)]).ok()?;
            if d.abs() > 1e-12 {
                let q = n / d;
                if q.is_finite() {
                    values.push(q);
                }
            }
        }
    }
    if values.len() < 4 {
        return None;
    }
    let avg = values.iter().sum::<f64>() / values.len() as f64;
    let spread = values.iter().map(|v| (v - avg).abs()).fold(0.0, f64::max);
    (spread < 1e-3).then_some(avg)
}

fn has_inconsistent_augmented_row(rref: &Matrix, vars: usize, eps: f64) -> bool {
    (0..rref.rows)
        .any(|r| (0..vars).all(|c| rref.get(r, c).abs() <= eps) && rref.get(r, vars).abs() > eps)
}

#[allow(clippy::unwrap_used)]
fn replace_matrix_column(a: &Matrix, col: usize, b: &Matrix) -> Matrix {
    Matrix::from_rows(
        (0..a.rows)
            .map(|r| {
                (0..a.cols)
                    .map(|c| if c == col { b.get(r, 0) } else { a.get(r, c) })
                    .collect::<Vec<_>>()
            })
            .collect(),
    )
    .unwrap()
}

fn minor_matrix(m: &Matrix, row: usize, col: usize) -> Option<Matrix> {
    if m.rows != m.cols || row >= m.rows || col >= m.cols {
        return None;
    }
    Matrix::from_rows(
        (0..m.rows)
            .filter(|&r| r != row)
            .map(|r| {
                (0..m.cols)
                    .filter(|&c| c != col)
                    .map(|c| m.get(r, c))
                    .collect::<Vec<_>>()
            })
            .collect(),
    )
}

fn cofactor_value(m: &Matrix, row: usize, col: usize) -> Option<f64> {
    if m.rows == 1 && m.cols == 1 && row == 0 && col == 0 {
        return Some(1.0);
    }
    let minor = minor_matrix(m, row, col)?;
    let det = minor.determinant()?;
    Some(if (row + col).is_multiple_of(2) {
        det
    } else {
        -det
    })
}

fn adjugate_matrix(m: &Matrix) -> Option<Matrix> {
    if m.rows != m.cols {
        return None;
    }
    Matrix::from_rows(
        (0..m.rows)
            .map(|r| {
                (0..m.cols)
                    .map(|c| cofactor_value(m, c, r).unwrap_or(0.0))
                    .collect::<Vec<_>>()
            })
            .collect(),
    )
}

fn multiply_matrix_vector(m: &Matrix, v: &[f64]) -> Vec<f64> {
    (0..m.rows)
        .map(|r| (0..m.cols).map(|c| m.get(r, c) * v[c]).sum())
        .collect()
}

fn matrix_from_columns(cols: &[Vec<f64>]) -> Option<Matrix> {
    if cols.is_empty() {
        return None;
    }
    let rows = cols[0].len();
    Matrix::from_rows(
        (0..rows)
            .map(|r| cols.iter().map(|col| col[r]).collect::<Vec<_>>())
            .collect(),
    )
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
    fn ode_plot_decimation_preserves_both_endpoints_within_the_polygon_limit() {
        let point_count = grafito_core::validation::MAX_POLYGON_VERTICES + 37;
        let indices = bounded_ode_plot_indices(point_count);

        assert_eq!(
            indices.len(),
            grafito_core::validation::MAX_POLYGON_VERTICES
        );
        assert_eq!(indices.first(), Some(&0));
        assert_eq!(indices.last(), Some(&(point_count - 1)));
        assert!(indices.windows(2).all(|pair| pair[0] < pair[1]));
    }

    #[test]
    fn conservative_polynomial_partials_match_symbolically() {
        let cases = [
            ("x^2*y", "y", "x^2*z", "z"),
            ("2*x*y*z", "z", "x^2*y", "x"),
            ("x^2*z", "x", "2*x*y*z", "y"),
        ];

        for (left, left_var, right, right_var) in cases {
            let left_derivative = symbolic_partial(left, left_var).unwrap();
            let right_derivative = symbolic_partial(right, right_var).unwrap();
            assert!(
                partial_derivatives_equal(left, left_var, right, right_var),
                "d/d{left_var}({left}) = {left_derivative}, d/d{right_var}({right}) = {right_derivative}"
            );
        }
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

    #[test]
    fn test_complex_expression_creates_complex_grid() {
        let mut doc = Document::new();
        let outcome = process_input(&mut doc, &mut "deriv_z_conj(z^2)".to_string());
        assert!(matches!(outcome, CommandOutcome::Ok));
        let has_grid = doc
            .objects_iter()
            .any(|(_, o)| matches!(o, GeoObject::ComplexGrid(_)));
        assert!(
            has_grid,
            "Should automatically create a ComplexGrid for complex expressions"
        );
    }

    #[test]
    fn test_batch_multiline_segment_and_points() {
        let mut doc = Document::new();
        let input = "GRAFITO\n\nSegment[(0, 0), (1, 0)]\nPoint[(0, 0)]\nPoint[(0.3333, 0)]\nPoint[(0.6667, 0)]\nPoint[(1, 0)]";
        let mut buf = input.to_string();
        let outcome = process_input(&mut doc, &mut buf);
        assert!(
            !matches!(outcome, CommandOutcome::Error(_)),
            "batch should succeed, got {outcome:?}"
        );
        let points = doc
            .objects_iter()
            .filter(|(_, o)| matches!(o, GeoObject::Point(_)))
            .count();
        let segments = doc
            .objects_iter()
            .filter(|(_, o)| matches!(o, GeoObject::Line(_)))
            .count();
        assert_eq!(points, 4, "should have 4 points, have {}", points);
        assert_eq!(segments, 1, "should have 1 segment, have {segments}");
    }
}
