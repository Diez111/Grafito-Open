//! Document validation to mitigate untrusted save-file DoS.

use crate::{pencil::MAX_PENCIL_POINTS, Document, GeoObject};
use grafito_geometry::{
    Color, Point2, Point3D, RegularPolytopeProjectionError, RegularPolytopeProjectionPlan,
    MAX_REGULAR_POLYTOPE_DIMENSION, MIN_REGULAR_POLYTOPE_DIMENSION,
};
use serde_json::Value;

pub const MAX_DOCUMENT_SIZE_BYTES: usize = 10_000_000;
pub const MAX_JSON_DEPTH: usize = 64;
/// Maximum number of array/object separators accepted before JSON materialization.
pub const MAX_JSON_STRUCTURAL_ELEMENTS: usize = MAX_ARRAY_LENGTH;
pub const MAX_STRING_LENGTH: usize = 10_000;
pub const MAX_ARRAY_LENGTH: usize = 200_000;
pub const MAX_OBJECT_COUNT: usize = 5_000;
pub const MAX_EXPR_LENGTH: usize = 2_000;
pub const MAX_DENSITY: usize = 500;
pub const MAX_FRACTAL_RESOLUTION: usize = 1_000;
pub const MAX_FRACTAL_ITER: u32 = grafito_geometry::fractals::MAX_FRACTAL_ITER;
pub const MAX_ATTRACTOR_STEPS: usize = 500_000;
pub const MAX_SURFACE_MESH_RES: usize = 200;
pub const MAX_HYPERSURFACE_RES: usize = 100;
pub const MAX_HISTOGRAM_BINS: usize = grafito_geometry::statistics::MAX_HISTOGRAM_BINS;
/// Máximo de filas para una tabla local persistente y sus ajustes enlazados.
pub const MAX_DATA_TABLE_ROWS: usize = grafito_geometry::statistics::MAX_FIT_DATA_POINTS;
/// Maximum number of vertices accepted for one polygon.
pub const MAX_POLYGON_VERTICES: usize = 8_192;
/// Maximum nesting accepted for `GeoObject::Transformed` wrappers.
pub const MAX_TRANSFORM_DEPTH: usize = 64;
/// Maximum number of contour levels on one implicit curve.
pub const MAX_CONTOUR_LEVELS: usize = 16;
/// Maximum marching-squares cell visits across all contour levels of one curve.
pub const MAX_CONTOUR_WORK_UNITS: usize = 8 * 1024 * 1024;
const MAX_IMPLICIT_GRID_CELLS: usize = 1024 * 1024;
const MAX_OBJECT_PARAMETERS: usize = 64;
/// Épsilon geométrico para pruebas de degeneración (longitud, altura, dirección).
pub const GEOM_EPS: f64 = 1e-12;

/// Wrapper fail-closed que garantiza que el `Document` interno pasó `validate_document`.
///
/// Uso: `ValidatedDocument::try_new(doc)?` antes de persistir o de exponer un
/// snapshot al render. Migrar `HashMap` → `BTreeMap` en `Document` es la
/// siguiente fase para determinismo total; por ahora el wrapper evita que un
/// documento a medio mutar escape de `detached_clone` → `commit`.
#[derive(Debug, Clone)]
pub struct ValidatedDocument(pub crate::Document);

impl ValidatedDocument {
    pub fn try_new(doc: crate::Document) -> Result<Self, String> {
        validate_document(&doc)?;
        Ok(Self(doc))
    }
    pub fn inner(&self) -> &crate::Document {
        &self.0
    }
    pub fn into_inner(self) -> crate::Document {
        self.0
    }
}

/// Validate the raw JSON before deserializing into a `Document`.
fn validate_text_nesting(json: &str) -> Result<(), String> {
    let mut depth: usize = 0;
    let mut max_depth: usize = 0;
    let mut structural_elements: usize = 0;
    let mut in_string = false;
    let mut escape = false;
    for c in json.chars() {
        if in_string {
            if escape {
                escape = false;
            } else if c == '\\' {
                escape = true;
            } else if c == '"' {
                in_string = false;
            }
            continue;
        }
        match c {
            '"' => in_string = true,
            '[' | '{' => {
                structural_elements += 1;
                if structural_elements > MAX_JSON_STRUCTURAL_ELEMENTS {
                    return Err("Document JSON contains too many structural elements".to_string());
                }
                depth = depth.saturating_add(1);
                max_depth = max_depth.max(depth);
                if max_depth > MAX_JSON_DEPTH {
                    return Err("Document JSON is too deeply nested".to_string());
                }
            }
            ']' | '}' => {
                depth = depth.saturating_sub(1);
            }
            ',' => {
                structural_elements += 1;
                if structural_elements > MAX_JSON_STRUCTURAL_ELEMENTS {
                    return Err("Document JSON contains too many structural elements".to_string());
                }
            }
            _ => {}
        }
    }
    Ok(())
}

pub fn validate_document_json(json: &str) -> Result<(), String> {
    parse_document_json(json).map(|_| ())
}

/// Parse document JSON only after applying text and structural resource limits.
pub fn parse_document_json(json: &str) -> Result<Value, String> {
    if json.len() > MAX_DOCUMENT_SIZE_BYTES {
        return Err(format!(
            "Document size {} exceeds maximum {}",
            json.len(),
            MAX_DOCUMENT_SIZE_BYTES
        ));
    }
    validate_text_nesting(json)?;
    let value: Value = serde_json::from_str(json).map_err(|e| e.to_string())?;
    validate_value(&value, 0, &mut 0)?;
    validate_spreadsheet_json(&value)?;
    validate_cas_worksheet_json(&value)?;
    Ok(value)
}

/// Rechaza hojas sobredimensionadas antes de que serde asigne sus `Vec`s
/// anidados en un `Document`. Soporta envelopes actuales y documentos crudos
/// heredados.
fn validate_spreadsheet_json(value: &Value) -> Result<(), String> {
    let document = value
        .as_object()
        .and_then(|object| object.get("document"))
        .unwrap_or(value);
    let Some(spreadsheet) = document
        .as_object()
        .and_then(|object| object.get("spreadsheet"))
    else {
        return Ok(());
    };
    let rows = spreadsheet
        .as_array()
        .ok_or_else(|| "Spreadsheet must be an array".to_string())?;
    if rows.len() > Document::MAX_SPREADSHEET_ROWS {
        return Err("Spreadsheet contains too many rows".to_string());
    }
    for row in rows {
        let cells = row
            .as_array()
            .ok_or_else(|| "Spreadsheet row must be an array".to_string())?;
        if cells.len() > Document::MAX_SPREADSHEET_COLS {
            return Err("Spreadsheet contains too many columns".to_string());
        }
    }
    Ok(())
}

/// Rechaza hojas CAS con demasiadas celdas antes de deserializar el documento.
/// Igual que la hoja de cálculo, funciona con envelopes y documentos crudos.
fn validate_cas_worksheet_json(value: &Value) -> Result<(), String> {
    let document = value
        .as_object()
        .and_then(|object| object.get("document"))
        .unwrap_or(value);
    let Some(worksheet) = document
        .as_object()
        .and_then(|object| object.get("cas_worksheet"))
    else {
        return Ok(());
    };
    let cells = worksheet
        .as_array()
        .ok_or_else(|| "CAS worksheet must be an array".to_string())?;
    if cells.len() > Document::MAX_CAS_WORKSHEET_CELLS {
        return Err("CAS worksheet contains too many cells".to_string());
    }
    Ok(())
}

fn validate_value(value: &Value, depth: usize, nodes: &mut usize) -> Result<(), String> {
    if depth > MAX_JSON_DEPTH {
        return Err("Document JSON is too deeply nested".to_string());
    }
    *nodes += 1;
    if *nodes > 1_000_000 {
        return Err("Document JSON contains too many nodes".to_string());
    }

    match value {
        Value::Null | Value::Bool(_) | Value::Number(_) => Ok(()),
        Value::String(s) => {
            if s.len() > MAX_STRING_LENGTH {
                return Err(format!(
                    "String length {} exceeds maximum {}",
                    s.len(),
                    MAX_STRING_LENGTH
                ));
            }
            Ok(())
        }
        Value::Array(arr) => {
            if arr.len() > MAX_ARRAY_LENGTH {
                return Err(format!(
                    "Array length {} exceeds maximum {}",
                    arr.len(),
                    MAX_ARRAY_LENGTH
                ));
            }
            for v in arr {
                validate_value(v, depth + 1, nodes)?;
            }
            Ok(())
        }
        Value::Object(map) => {
            if map.len() > MAX_ARRAY_LENGTH {
                return Err(format!(
                    "Object field count {} exceeds maximum {}",
                    map.len(),
                    MAX_ARRAY_LENGTH
                ));
            }
            for (k, v) in map {
                if k.len() > MAX_STRING_LENGTH {
                    return Err(format!(
                        "Object key length {} exceeds maximum {}",
                        k.len(),
                        MAX_STRING_LENGTH
                    ));
                }
                validate_value(v, depth + 1, nodes)?;
            }
            Ok(())
        }
    }
}

/// Validate a deserialized document, capping expensive object parameters.
pub fn validate_document(doc: &Document) -> Result<(), String> {
    let count = doc.object_count();
    if count > MAX_OBJECT_COUNT {
        return Err(format!(
            "Document contains {} objects, maximum is {}",
            count, MAX_OBJECT_COUNT
        ));
    }

    // Limit the total number of constraints to bound the cost of cycle
    // detection / topological sort in `get_update_order`.
    if doc.constraints.constraint_count() > crate::constraints::MAX_CONSTRAINTS {
        return Err(format!(
            "Document contains {} constraints, maximum is {}",
            doc.constraints.constraint_count(),
            crate::constraints::MAX_CONSTRAINTS
        ));
    }

    let view = doc.view();
    validate_point2(view.offset, "Document.view.offset")?;
    validate_finite(view.scale, "Document.view.scale")?;
    validate_finite_f32(view.screen_size.x, "Document.view.screen_size.x")?;
    validate_finite_f32(view.screen_size.y, "Document.view.screen_size.y")?;

    if doc.variables.len() > MAX_ARRAY_LENGTH {
        return Err("Document contains too many variables".to_string());
    }
    for (name, value) in &doc.variables {
        validate_string(name, "Variable name")?;
        validate_finite(*value, &format!("Variable {name}"))?;
    }
    let variable_metadata = doc.variable_metadata();
    if variable_metadata.len() > MAX_ARRAY_LENGTH {
        return Err("Document contains too many variable metadata entries".to_string());
    }
    for (name, meta) in variable_metadata {
        validate_string(name, "Variable metadata name")?;
        if !doc.variables.contains_key(name) {
            return Err(format!(
                "Variable metadata '{name}' does not have a corresponding variable"
            ));
        }
        validate_point2(meta.position, &format!("VariableMeta {name}.position"))?;
        validate_finite(meta.min, &format!("VariableMeta {name}.min"))?;
        validate_finite(meta.max, &format!("VariableMeta {name}.max"))?;
        validate_finite(meta.step, &format!("VariableMeta {name}.step"))?;
        validate_finite(
            meta.animation_speed,
            &format!("VariableMeta {name}.animation_speed"),
        )?;
        if meta.min >= meta.max {
            return Err(format!("VariableMeta {name}.min must be smaller than max"));
        }
        if meta.step <= 0.0 {
            return Err(format!("VariableMeta {name}.step must be positive"));
        }
    }
    if doc.spreadsheet.len() > Document::MAX_SPREADSHEET_ROWS {
        return Err("Spreadsheet contains too many rows".to_string());
    }
    let mut active_spreadsheet_cells = 0usize;
    for row in &doc.spreadsheet {
        if row.len() > Document::MAX_SPREADSHEET_COLS {
            return Err("Spreadsheet contains too many columns".to_string());
        }
        for cell in row {
            validate_string(cell, "Spreadsheet cell")?;
            if !cell.trim().is_empty() {
                active_spreadsheet_cells += 1;
                if active_spreadsheet_cells > Document::MAX_SPREADSHEET_RECOMPUTE_CELLS {
                    return Err(format!(
                        "Spreadsheet exceeds the {} cell recomputation limit",
                        Document::MAX_SPREADSHEET_RECOMPUTE_CELLS
                    ));
                }
            }
        }
    }
    doc.validate_cas_worksheet()?;
    doc.validate_spreadsheet_coordinate_points()?;
    validate_string(&doc.complex_base_symbol, "Complex base symbol")?;
    doc.validate_label_counters()?;

    for (id, obj) in doc.objects_iter() {
        validate_object_candidate(doc, obj)?;
        if *id != obj.id() {
            return Err(format!(
                "Object map key {} does not match embedded object id {}",
                id,
                obj.id()
            ));
        }
    }

    // Check canonical topology before algorithm-specific semantics so a cycle
    // or duplicate creator cannot be masked by an unrelated algorithm name.
    doc.constraints
        .validate_semantics(doc.objects_iter().map(|(id, _)| *id))?;

    for constraint in doc.constraints.iter() {
        for id in &constraint.inputs {
            if doc.get_object(*id).is_none() {
                return Err(format!(
                    "Constraint {} references missing input object {}",
                    constraint.id, id
                ));
            }
        }
        for id in &constraint.outputs {
            if doc.get_object(*id).is_none() {
                return Err(format!(
                    "Constraint {} references missing output object {}",
                    constraint.id, id
                ));
            }
        }
        if Document::is_numeric_constraint_name(&constraint.name) {
            doc.validate_numeric_constraint_definition(
                &constraint.name,
                &constraint.inputs,
                &constraint.params,
            )?;
        }
        doc.validate_constructive_constraint_definition(
            &constraint.name,
            &constraint.inputs,
            &constraint.outputs,
            &constraint.params,
        )?;
    }

    // Whiteboard persistente — cota: acotada a 500 elementos / 8192 puntos por trazo ya validado en whiteboard lib.
    if doc.whiteboard.len() > 500 {
        return Err("Whiteboard contiene demasiados elementos (máx 500)".to_string());
    }
    for element in doc.whiteboard.elements() {
        match element {
            grafito_whiteboard::WhiteboardElement::Stroke { points, width, .. } => {
                if points.len() > crate::pencil::MAX_PENCIL_POINTS {
                    return Err("Whiteboard Stroke excede MAX_PENCIL_POINTS".to_string());
                }
                validate_positive_f32(*width as f32, "Whiteboard Stroke.width")?;
                for (idx, (x, y)) in points.iter().enumerate() {
                    validate_finite(*x, &format!("Whiteboard Stroke.points[{idx}].x"))?;
                    validate_finite(*y, &format!("Whiteboard Stroke.points[{idx}].y"))?;
                }
            }
            grafito_whiteboard::WhiteboardElement::Text { text, size, .. } => {
                validate_string(text, "Whiteboard Text.text")?;
                validate_positive_f32(*size as f32, "Whiteboard Text.size")?;
            }
            _ => {}
        }
    }

    for (id, object) in doc.objects_iter() {
        let GeoObject::Pencil(locus) = object else {
            continue;
        };
        let Some(binding) = locus.locus_binding() else {
            continue;
        };
        let matches = doc
            .constraints
            .iter()
            .filter(|constraint| {
                constraint.name == "Locus"
                    && constraint.inputs == vec![binding.driver, binding.target]
                    && constraint.outputs == vec![*id]
            })
            .count();
        if matches != 1 {
            return Err(format!(
                "Locus {} must have exactly one matching Locus constraint",
                id
            ));
        }
    }

    for id in doc.constraints.free_objects_iter() {
        if doc.get_object(*id).is_none() {
            return Err(format!(
                "Free object reference {} is missing from document",
                id
            ));
        }
    }

    Ok(())
}

/// Valida la semántica propia de un objeto candidato y sus referencias.
///
/// No comprueba capacidad, colisiones de identificador ni unicidad de etiqueta;
/// esas políticas pertenecen a [`Document::try_add_object`].
pub fn validate_object_candidate(doc: &Document, obj: &GeoObject) -> Result<(), String> {
    validate_geo_object(doc, obj, 0)
}

fn validate_geo_object(doc: &Document, obj: &GeoObject, depth: usize) -> Result<(), String> {
    for target in obj.referenced_object_ids() {
        if target == obj.id() || doc.get_object(target).is_none() {
            return Err(format!("{} target {} is missing", obj.name(), target));
        }
    }

    if let GeoObject::Transformed(o) = obj {
        if depth >= MAX_TRANSFORM_DEPTH {
            return Err(format!(
                "Transformed object nesting exceeds maximum {}",
                MAX_TRANSFORM_DEPTH
            ));
        }
        validate_expr(&o.complex_expr)?;
        if let Some(compiled) = &o.compiled_expr {
            validate_expr(compiled)?;
        }
        return validate_geo_object(doc, &o.inner, depth + 1);
    }

    let label = obj.label();
    validate_string(label, "Object label")?;
    validate_color(obj.color(), &format!("{}.color", obj.name()))?;

    match obj {
        GeoObject::Point(o) => {
            validate_point2(o.position, "Point.position")?;
            validate_optional_expr(&o.x_expr, "Point.x_expr")?;
            validate_optional_expr(&o.y_expr, "Point.y_expr")?;
            validate_positive_f32(o.size, "Point.size")?;
        }
        GeoObject::Line(o) => {
            validate_point2(o.start, "Line.start")?;
            validate_point2(o.end, "Line.end")?;
            validate_optional_expr(&o.start_x_expr, "Line.start_x_expr")?;
            validate_optional_expr(&o.start_y_expr, "Line.start_y_expr")?;
            validate_optional_expr(&o.end_x_expr, "Line.end_x_expr")?;
            validate_optional_expr(&o.end_y_expr, "Line.end_y_expr")?;
            validate_positive_f32(o.width, "Line.width")?;
            if o.kind != crate::LineKind::Segment {
                validate_nonzero_direction_2d(o.start, o.end, "Line")?;
            }
        }
        GeoObject::Circle(o) => {
            validate_point2(o.center, "Circle.center")?;
            validate_positive(o.radius, "Circle.radius")?;
            validate_optional_expr(&o.radius_expr, "Circle.radius_expr")?;
            validate_positive_f32(o.width, "Circle.width")?;
            validate_optional_color(o.fill_color, "Circle.fill_color")?;
        }
        GeoObject::Polygon(o) => {
            if o.vertices.len() > MAX_POLYGON_VERTICES {
                return Err(format!(
                    "Polygon vertices {} exceeds maximum {}",
                    o.vertices.len(),
                    MAX_POLYGON_VERTICES
                ));
            }
            if o.x_exprs.len() > MAX_POLYGON_VERTICES || o.y_exprs.len() > MAX_POLYGON_VERTICES {
                return Err("Polygon expression count exceeds vertex maximum".to_string());
            }
            for (index, point) in o.vertices.iter().copied().enumerate() {
                validate_point2(point, &format!("Polygon.vertices[{index}]"))?;
            }
            for (index, expr) in o.x_exprs.iter().enumerate() {
                validate_optional_expr(expr, &format!("Polygon.x_exprs[{index}]"))?;
            }
            for (index, expr) in o.y_exprs.iter().enumerate() {
                validate_optional_expr(expr, &format!("Polygon.y_exprs[{index}]"))?;
            }
            validate_positive_f32(o.width, "Polygon.width")?;
            validate_optional_color(o.fill_color, "Polygon.fill_color")?;
            // Validación de colinealidad/degeneración vía shoelace.
            if o.vertices.len() >= 3 {
                let n = o.vertices.len();
                let mut area2 = 0.0;
                let mut perim = 0.0;
                for i in 0..n {
                    let p1 = o.vertices[i];
                    let p2 = o.vertices[(i + 1) % n];
                    area2 += p1.x * p2.y - p2.x * p1.y;
                    perim += (p2.x - p1.x).hypot(p2.y - p1.y);
                }
                if !area2.is_finite() || !perim.is_finite() {
                    return Err("Polygon vertices must be finite".to_string());
                }
                if area2.abs() <= GEOM_EPS * perim * perim {
                    return Err("Polygon is degenerate or colinear".to_string());
                }
                // Chequeo adicional: todos los cross de triples consecutivos < GEOM_EPS
                let mut all_small = true;
                for i in 0..n {
                    let a = o.vertices[i];
                    let b = o.vertices[(i + 1) % n];
                    let c = o.vertices[(i + 2) % n];
                    let cross = (b.x - a.x) * (c.y - a.y) - (b.y - a.y) * (c.x - a.x);
                    if cross.abs() >= GEOM_EPS {
                        all_small = false;
                        break;
                    }
                }
                if all_small {
                    return Err("Polygon is degenerate or colinear".to_string());
                }
            }
        }
        GeoObject::Pencil(o) => {
            if o.points.len() > MAX_PENCIL_POINTS {
                return Err(format!(
                    "Pencil points {} exceeds maximum {}",
                    o.points.len(),
                    MAX_PENCIL_POINTS
                ));
            }
            for (index, point) in o.points.iter().copied().enumerate() {
                validate_point2(point, &format!("Pencil.points[{index}]"))?;
            }
            validate_positive_f32(o.width, "Pencil.width")?;
        }
        GeoObject::Function(o) => {
            validate_expr(&o.expr)?;
            validate_optional_finite(o.domain_min, "Function.domain_min")?;
            validate_optional_finite(o.domain_max, "Function.domain_max")?;
            if let (Some(min), Some(max)) = (o.domain_min, o.domain_max) {
                validate_ordered_bounds(min, max, "Function.domain_min", "Function.domain_max")?;
            }
            validate_optional_expr(&o.domain_min_expr, "Function.domain_min_expr")?;
            validate_optional_expr(&o.domain_max_expr, "Function.domain_max_expr")?;
            validate_optional_color(o.fill_color, "Function.fill_color")?;
            validate_positive_f32(o.width, "Function.width")?;
            validate_string(&o.integral_var, "Function.integral_var")?;
            validate_finite(o.integral_lower, "Function.integral_lower")?;
            if let Some(fit) = &o.fit {
                validate_fit_metadata(doc, fit)?;
            }
        }
        GeoObject::Text(o) => {
            validate_string(&o.content, "Text.content")?;
            validate_point2(o.position, "Text.position")?;
            validate_positive_f32(o.font_size, "Text.font_size")?;
        }
        GeoObject::Ellipse(o) => {
            validate_point2(o.center, "Ellipse.center")?;
            validate_positive(o.rx, "Ellipse.rx")?;
            validate_positive(o.ry, "Ellipse.ry")?;
            validate_finite(o.angle, "Ellipse.angle")?;
            validate_positive_f32(o.width, "Ellipse.width")?;
            validate_optional_color(o.fill_color, "Ellipse.fill_color")?;
        }
        GeoObject::Parabola(o) => {
            validate_point2(o.vertex, "Parabola.vertex")?;
            validate_nonzero(o.p, "Parabola.p")?;
            validate_finite(o.angle, "Parabola.angle")?;
            validate_positive_f32(o.width, "Parabola.width")?;
        }
        GeoObject::Hyperbola(o) => {
            validate_point2(o.center, "Hyperbola.center")?;
            validate_positive(o.a, "Hyperbola.a")?;
            validate_positive(o.b, "Hyperbola.b")?;
            validate_finite(o.angle, "Hyperbola.angle")?;
            validate_positive_f32(o.width, "Hyperbola.width")?;
        }
        GeoObject::Point3D(o) => {
            validate_point3(o.position, "Point3D.position")?;
            validate_positive_f32(o.size, "Point3D.size")?;
        }
        GeoObject::Segment3D(o) => {
            validate_point3(o.a, "Segment3D.a")?;
            validate_point3(o.b, "Segment3D.b")?;
            validate_positive_f32(o.width, "Segment3D.width")?;
        }
        GeoObject::Plane3D(o) => {
            validate_finite(o.a, "Plane3D.a")?;
            validate_finite(o.b, "Plane3D.b")?;
            validate_finite(o.c, "Plane3D.c")?;
            validate_finite(o.d, "Plane3D.d")?;
            validate_optional_expr(&o.a_expr, "Plane3D.a_expr")?;
            validate_optional_expr(&o.b_expr, "Plane3D.b_expr")?;
            validate_optional_expr(&o.c_expr, "Plane3D.c_expr")?;
            validate_optional_expr(&o.d_expr, "Plane3D.d_expr")?;
            let normal_length = o.a.hypot(o.b).hypot(o.c);
            if !normal_length.is_finite() || normal_length <= 0.0 {
                return Err("Plane3D normal must be nonzero".to_string());
            }
            validate_unit_interval_f32(o.opacity, "Plane3D.opacity")?;
        }
        GeoObject::Line3D(o) => {
            validate_point3(o.point, "Line3D.point")?;
            validate_point3(o.direction, "Line3D.direction")?;
            validate_optional_expr(&o.px_expr, "Line3D.px_expr")?;
            validate_optional_expr(&o.py_expr, "Line3D.py_expr")?;
            validate_optional_expr(&o.pz_expr, "Line3D.pz_expr")?;
            validate_optional_expr(&o.dx_expr, "Line3D.dx_expr")?;
            validate_optional_expr(&o.dy_expr, "Line3D.dy_expr")?;
            validate_optional_expr(&o.dz_expr, "Line3D.dz_expr")?;
            let direction_length = o.direction.x.hypot(o.direction.y).hypot(o.direction.z);
            if !direction_length.is_finite() || direction_length <= 0.0 {
                return Err("Line3D.direction must be nonzero".to_string());
            }
            validate_positive_f32(o.width, "Line3D.width")?;
        }
        GeoObject::Sphere3D(o) => {
            validate_point3(o.center, "Sphere3D.center")?;
            validate_positive(o.radius, "Sphere3D.radius")?;
            validate_positive_f32(o.width, "Sphere3D.width")?;
            validate_optional_color(o.fill_color, "Sphere3D.fill_color")?;
        }
        GeoObject::Cube3D(o) => {
            validate_point3(o.center, "Cube3D.center")?;
            validate_positive(o.size, "Cube3D.size")?;
            validate_positive_f32(o.width, "Cube3D.width")?;
            validate_optional_color(o.fill_color, "Cube3D.fill_color")?;
        }
        GeoObject::Tetrahedron3D(o) => {
            validate_point3(o.center, "Tetrahedron3D.center")?;
            validate_positive(o.edge_length, "Tetrahedron3D.edge_length")?;
            if !grafito_geometry::Tetrahedron3D::new(o.center, o.edge_length).is_renderable() {
                return Err(
                    "Tetrahedron3D vertices exceed the maximum renderable coordinate".to_string(),
                );
            }
            validate_positive_f32(o.width, "Tetrahedron3D.width")?;
            validate_optional_color(o.fill_color, "Tetrahedron3D.fill_color")?;
        }
        GeoObject::Pyramid3D(o) => {
            validate_point3(o.base_center, "Pyramid3D.base_center")?;
            validate_point3(o.apex, "Pyramid3D.apex")?;
            validate_positive(o.base_size, "Pyramid3D.base_size")?;
            validate_nonzero_direction_3d(o.base_center, o.apex, "Pyramid3D.axis")?;
            validate_positive_f32(o.width, "Pyramid3D.width")?;
            validate_optional_color(o.fill_color, "Pyramid3D.fill_color")?;
        }
        GeoObject::Cone3D(o) => {
            validate_point3(o.base_center, "Cone3D.base_center")?;
            validate_point3(o.apex, "Cone3D.apex")?;
            validate_positive(o.radius, "Cone3D.radius")?;
            validate_nonzero_direction_3d(o.base_center, o.apex, "Cone3D.axis")?;
            validate_positive_f32(o.width, "Cone3D.width")?;
            validate_optional_color(o.fill_color, "Cone3D.fill_color")?;
        }
        GeoObject::Cylinder3D(o) => {
            validate_point3(o.base_center, "Cylinder3D.base_center")?;
            validate_point3(o.top_center, "Cylinder3D.top_center")?;
            validate_positive(o.radius, "Cylinder3D.radius")?;
            validate_nonzero_direction_3d(o.base_center, o.top_center, "Cylinder3D.axis")?;
            validate_positive_f32(o.width, "Cylinder3D.width")?;
            validate_optional_color(o.fill_color, "Cylinder3D.fill_color")?;
        }
        GeoObject::Torus3D(o) => {
            validate_point3(o.center, "Torus3D.center")?;
            validate_positive(o.r_major, "Torus3D.r_major")?;
            validate_positive(o.r_minor, "Torus3D.r_minor")?;
            validate_positive_f32(o.width, "Torus3D.width")?;
        }
        GeoObject::MoebiusStrip(o) => {
            validate_point3(o.center, "MoebiusStrip.center")?;
            validate_positive(o.radius, "MoebiusStrip.radius")?;
            validate_positive(o.width_r, "MoebiusStrip.width_r")?;
            validate_positive_f32(o.width, "MoebiusStrip.width")?;
        }
        GeoObject::ParametricCurve2D(o) => {
            validate_expr(&o.expr_x)?;
            validate_expr(&o.expr_y)?;
            validate_ordered_bounds(
                o.t_min,
                o.t_max,
                "ParametricCurve2D.t_min",
                "ParametricCurve2D.t_max",
            )?;
            validate_optional_expr(&o.t_min_expr, "ParametricCurve2D.t_min_expr")?;
            validate_optional_expr(&o.t_max_expr, "ParametricCurve2D.t_max_expr")?;
            validate_positive_f32(o.width, "ParametricCurve2D.width")?;
        }
        GeoObject::ParametricCurve3D(o) => {
            validate_expr(&o.expr_x)?;
            validate_expr(&o.expr_y)?;
            validate_expr(&o.expr_z)?;
            validate_string(&o.parameter, "ParametricCurve3D.parameter")?;
            validate_ordered_bounds(
                o.t_min,
                o.t_max,
                "ParametricCurve3D.t_min",
                "ParametricCurve3D.t_max",
            )?;
            validate_optional_expr(&o.t_min_expr, "ParametricCurve3D.t_min_expr")?;
            validate_optional_expr(&o.t_max_expr, "ParametricCurve3D.t_max_expr")?;
            validate_positive_f32(o.width, "ParametricCurve3D.width")?;
        }
        GeoObject::PolarCurve(o) => {
            validate_expr(&o.expr_r)?;
            validate_ordered_bounds(o.t_min, o.t_max, "PolarCurve.t_min", "PolarCurve.t_max")?;
            validate_optional_expr(&o.t_min_expr, "PolarCurve.t_min_expr")?;
            validate_optional_expr(&o.t_max_expr, "PolarCurve.t_max_expr")?;
            validate_positive_f32(o.width, "PolarCurve.width")?;
            validate_optional_color(o.fill_color, "PolarCurve.fill_color")?;
        }
        GeoObject::Surface3D(o) => {
            validate_expr(&o.expr)?;
            validate_expr(&o.expr_x)?;
            validate_expr(&o.expr_y)?;
            validate_expr(&o.expr_z)?;
            for (value, field) in [
                (o.x_min, "Surface3D.x_min"),
                (o.x_max, "Surface3D.x_max"),
                (o.y_min, "Surface3D.y_min"),
                (o.y_max, "Surface3D.y_max"),
                (o.u_min, "Surface3D.u_min"),
                (o.u_max, "Surface3D.u_max"),
                (o.v_min, "Surface3D.v_min"),
                (o.v_max, "Surface3D.v_max"),
            ] {
                validate_finite(value, field)?;
            }
            if o.is_parametric {
                validate_ordered_bounds(o.u_min, o.u_max, "Surface3D.u_min", "Surface3D.u_max")?;
                validate_ordered_bounds(o.v_min, o.v_max, "Surface3D.v_min", "Surface3D.v_max")?;
            } else {
                validate_ordered_bounds(o.x_min, o.x_max, "Surface3D.x_min", "Surface3D.x_max")?;
                validate_ordered_bounds(o.y_min, o.y_max, "Surface3D.y_min", "Surface3D.y_max")?;
            }
            validate_optional_expr(&o.x_min_expr, "Surface3D.x_min_expr")?;
            validate_optional_expr(&o.x_max_expr, "Surface3D.x_max_expr")?;
            validate_optional_expr(&o.y_min_expr, "Surface3D.y_min_expr")?;
            validate_optional_expr(&o.y_max_expr, "Surface3D.y_max_expr")?;
            validate_positive_f32(o.width, "Surface3D.width")?;
            if o.mesh_res == 0 || o.mesh_res > MAX_SURFACE_MESH_RES {
                return Err(format!(
                    "Surface3D mesh_res {} must be between 1 and {}",
                    o.mesh_res, MAX_SURFACE_MESH_RES
                ));
            }
        }
        GeoObject::VectorField2D(o) => {
            validate_expr(&o.expr_u)?;
            validate_expr(&o.expr_v)?;
            if o.density == 0 || o.density > MAX_DENSITY {
                return Err(format!(
                    "VectorField2D density {} must be between 1 and {}",
                    o.density, MAX_DENSITY
                ));
            }
        }
        GeoObject::VectorField3D(o) => {
            validate_expr(&o.expr_u)?;
            validate_expr(&o.expr_v)?;
            validate_expr(&o.expr_w)?;
            for (value, field) in [
                (o.x_min, "VectorField3D.x_min"),
                (o.x_max, "VectorField3D.x_max"),
                (o.y_min, "VectorField3D.y_min"),
                (o.y_max, "VectorField3D.y_max"),
                (o.z_min, "VectorField3D.z_min"),
                (o.z_max, "VectorField3D.z_max"),
            ] {
                validate_finite(value, field)?;
            }
            if o.density == 0 || o.density > MAX_DENSITY {
                return Err(format!(
                    "VectorField3D density {} must be between 1 and {}",
                    o.density, MAX_DENSITY
                ));
            }
        }
        GeoObject::ComplexGrid(o) => {
            validate_expr(&o.expr)?;
            for (value, field) in [
                (o.x_min, "ComplexGrid.x_min"),
                (o.x_max, "ComplexGrid.x_max"),
                (o.y_min, "ComplexGrid.y_min"),
                (o.y_max, "ComplexGrid.y_max"),
            ] {
                validate_finite(value, field)?;
            }
            validate_ordered_bounds(o.x_min, o.x_max, "ComplexGrid.x_min", "ComplexGrid.x_max")?;
            validate_ordered_bounds(o.y_min, o.y_max, "ComplexGrid.y_min", "ComplexGrid.y_max")?;
            if o.density == 0 || o.density > MAX_DENSITY {
                return Err(format!(
                    "ComplexGrid density {} must be between 1 and {}",
                    o.density, MAX_DENSITY
                ));
            }
        }
        GeoObject::ComplexMapping(o) => {
            validate_expr(&o.expr)?;
            validate_finite_f32(o.homotopy_speed, "ComplexMapping.homotopy_speed")?;
        }
        GeoObject::ComplexIntegral(o) => {
            validate_expr(&o.expr)?;
        }
        GeoObject::ImplicitCurve(o) => {
            validate_expr(&o.expr_lhs)?;
            validate_expr(&o.expr_rhs)?;
            validate_positive_f32(o.width, "ImplicitCurve.width")?;
            validate_optional_color(o.fill_color, "ImplicitCurve.fill_color")?;
            validate_contours(o)?;
        }
        GeoObject::Attractor3D(o) => {
            validate_string(&o.attractor_type, "Attractor3D.attractor_type")?;
            validate_parameter_slice(&o.params, "Attractor3D.params")?;
            for (value, field) in [
                (o.x0, "Attractor3D.x0"),
                (o.y0, "Attractor3D.y0"),
                (o.z0, "Attractor3D.z0"),
                (o.dt, "Attractor3D.dt"),
            ] {
                validate_finite(value, field)?;
            }
            validate_positive_f32(o.width, "Attractor3D.width")?;
            if o.steps > MAX_ATTRACTOR_STEPS {
                return Err(format!(
                    "Attractor3D steps {} exceeds maximum {}",
                    o.steps, MAX_ATTRACTOR_STEPS
                ));
            }
        }
        GeoObject::Fractal2D(o) => {
            validate_string(&o.fractal_type, "Fractal2D.fractal_type")?;
            validate_parameter_slice(&o.params, "Fractal2D.params")?;
            for (value, field) in [
                (o.x_min, "Fractal2D.x_min"),
                (o.x_max, "Fractal2D.x_max"),
                (o.y_min, "Fractal2D.y_min"),
                (o.y_max, "Fractal2D.y_max"),
            ] {
                validate_finite(value, field)?;
            }
            if o.resolution == 0 || o.resolution > MAX_FRACTAL_RESOLUTION {
                return Err(format!(
                    "Fractal2D resolution {} must be between 1 and {}",
                    o.resolution, MAX_FRACTAL_RESOLUTION
                ));
            }
            if o.max_iter > MAX_FRACTAL_ITER {
                return Err(format!(
                    "Fractal2D max_iter {} exceeds maximum {}",
                    o.max_iter, MAX_FRACTAL_ITER
                ));
            }
            grafito_geometry::fractals::validate_fractal_budget(
                o.resolution,
                o.resolution,
                o.max_iter,
            )
            .map_err(|error| format!("Fractal2D {error}"))?;
        }
        GeoObject::RegularPolychoron4D(o) => {
            validate_positive(o.scale, "RegularPolychoron4D.scale")?;
            validate_regular_polytope_projection_bound(
                "RegularPolychoron4D.scale",
                o.kind.projection_plan(o.scale),
            )?;
            for (index, angle) in o.rotation_angles.iter().copied().enumerate() {
                validate_finite(
                    angle,
                    &format!("RegularPolychoron4D.rotation_angles[{index}]"),
                )?;
            }
            validate_positive_f32(o.width, "RegularPolychoron4D.width")?;
            validate_optional_color(o.fill_color, "RegularPolychoron4D.fill_color")?;
        }
        GeoObject::RegularPolytopeND(o) => {
            let Some(expected_rotation_count) =
                crate::RegularPolytopeNDObj::expected_rotation_angle_count(o.dimension)
            else {
                return Err(format!(
                    "RegularPolytopeND.dimension {} must be between {} and {}",
                    o.dimension, MIN_REGULAR_POLYTOPE_DIMENSION, MAX_REGULAR_POLYTOPE_DIMENSION
                ));
            };
            validate_positive(o.scale, "RegularPolytopeND.scale")?;
            validate_regular_polytope_projection_bound(
                "RegularPolytopeND.scale",
                o.family.projection_plan(o.dimension, o.scale),
            )?;
            if o.rotation_angles.len() != expected_rotation_count {
                return Err(format!(
                    "RegularPolytopeND.rotation_angles must contain {} angles for dimension {}",
                    expected_rotation_count, o.dimension
                ));
            }
            for (index, angle) in o.rotation_angles.iter().copied().enumerate() {
                validate_finite(
                    angle,
                    &format!("RegularPolytopeND.rotation_angles[{index}]"),
                )?;
            }
            validate_positive_f32(o.width, "RegularPolytopeND.width")?;
            validate_optional_color(o.fill_color, "RegularPolytopeND.fill_color")?;
        }
        GeoObject::HyperSurface4D(o) => {
            validate_string(&o.surface_type, "HyperSurface4D.surface_type")?;
            validate_parameter_slice(&o.params, "HyperSurface4D.params")?;
            validate_parameter_slice(&o.rotation_angles, "HyperSurface4D.rotation_angles")?;
            validate_positive_f32(o.width, "HyperSurface4D.width")?;
            if o.resolution == 0 || o.resolution > MAX_HYPERSURFACE_RES {
                return Err(format!(
                    "HyperSurface4D resolution {} must be between 1 and {}",
                    o.resolution, MAX_HYPERSURFACE_RES
                ));
            }
        }
        GeoObject::PhasePortrait(o) => {
            validate_expr(&o.expr_dx)?;
            validate_expr(&o.expr_dy)?;
            for (value, field) in [
                (o.x_min, "PhasePortrait.x_min"),
                (o.x_max, "PhasePortrait.x_max"),
                (o.y_min, "PhasePortrait.y_min"),
                (o.y_max, "PhasePortrait.y_max"),
            ] {
                validate_finite(value, field)?;
            }
            if o.density == 0 || o.density > MAX_DENSITY {
                return Err(format!(
                    "PhasePortrait density {} must be between 1 and {}",
                    o.density, MAX_DENSITY
                ));
            }
        }
        GeoObject::Histogram(o) if o.bins == 0 || o.bins > MAX_HISTOGRAM_BINS => {
            return Err(format!(
                "Histogram bins {} must be between 1 and {}",
                o.bins, MAX_HISTOGRAM_BINS
            ));
        }
        GeoObject::Histogram(o) => {
            validate_finite_slice(&o.data, "Histogram data")?;
            validate_plot_bounds(o.x_min, o.x_max, o.y_min, o.y_max, "Histogram")?;
            validate_positive_f32(o.width, "Histogram.width")?;
            validate_optional_color(o.fill_color, "Histogram.fill_color")?;
        }
        GeoObject::ScatterPlot(o) => {
            validate_finite_slice(&o.xs, "ScatterPlot.xs")?;
            validate_finite_slice(&o.ys, "ScatterPlot.ys")?;
            if o.xs.len() != o.ys.len() {
                return Err("ScatterPlot xs and ys must have the same length".to_string());
            }
            if let Some(source) = o.source_data {
                let Some(GeoObject::DataTable(table)) = doc.get_object(source) else {
                    return Err("ScatterPlot source_data must reference a DataTable".to_string());
                };
                if o.xs != table.xs || o.ys != table.ys {
                    return Err("ScatterPlot linked data must match its DataTable".to_string());
                }
            }
            validate_plot_bounds(o.x_min, o.x_max, o.y_min, o.y_max, "ScatterPlot")?;
            validate_positive_f32(o.point_size, "ScatterPlot.point_size")?;
        }
        GeoObject::BoxPlot(o) => {
            if o.data.iter().any(|value| !value.is_finite()) {
                return Err("BoxPlot data contains non-finite values".to_string());
            }
            validate_finite_slice(&o.data, "BoxPlot data")?;
            validate_finite(o.position, "BoxPlot.position")?;
            validate_positive(o.width_box, "BoxPlot.width_box")?;
            validate_plot_bounds(o.x_min, o.x_max, o.y_min, o.y_max, "BoxPlot")?;
            validate_positive_f32(o.width, "BoxPlot.width")?;
            validate_optional_color(o.fill_color, "BoxPlot.fill_color")?;
        }
        GeoObject::RegressionLine(o) => {
            validate_finite_slice(&o.xs, "RegressionLine.xs")?;
            validate_finite_slice(&o.ys, "RegressionLine.ys")?;
            validate_finite(o.slope, "RegressionLine.slope")?;
            validate_finite(o.intercept, "RegressionLine.intercept")?;
            validate_finite(o.r_squared, "RegressionLine.r_squared")?;
            validate_string(&o.regression_type, "RegressionLine.regression_type")?;
            validate_plot_bounds(o.x_min, o.x_max, o.y_min, o.y_max, "RegressionLine")?;
            validate_positive_f32(o.width, "RegressionLine.width")?;
        }
        GeoObject::DataTable(o) => {
            validate_string(&o.x_name, "DataTable.x_name")?;
            validate_string(&o.y_name, "DataTable.y_name")?;
            if o.xs.len() != o.ys.len() {
                return Err("DataTable xs and ys must have the same length".to_string());
            }
            if o.xs.len() < 2 {
                return Err("DataTable requires at least two rows".to_string());
            }
            if o.xs.len() > MAX_DATA_TABLE_ROWS {
                return Err(format!(
                    "DataTable rows {} exceeds maximum {}",
                    o.xs.len(),
                    MAX_DATA_TABLE_ROWS
                ));
            }
            validate_finite_slice(&o.xs, "DataTable.xs")?;
            validate_finite_slice(&o.ys, "DataTable.ys")?;
        }
        GeoObject::Transformed(_) => {
            return Err("Transformed object was not validated recursively".to_string());
        }
    }
    Ok(())
}

fn validate_finite(value: f64, field: &str) -> Result<(), String> {
    if value.is_finite() {
        Ok(())
    } else {
        Err(format!("{field} must be finite"))
    }
}

fn validate_positive(value: f64, field: &str) -> Result<(), String> {
    validate_finite(value, field)?;
    if value > 0.0 {
        Ok(())
    } else {
        Err(format!("{field} must be positive"))
    }
}

/// Valida la cota de proyeccion desde geometria sin conocer implementaciones de render.
fn validate_regular_polytope_projection_bound(
    field: &str,
    plan: Result<RegularPolytopeProjectionPlan, RegularPolytopeProjectionError>,
) -> Result<(), String> {
    let plan = plan.map_err(|error| format!("{field} projection plan is invalid: {error}"))?;
    plan.ensure_within_coordinate_limit(grafito_geometry::MAX_WORLD_COORDINATE)
        .map_err(|error| {
            format!("{field} projection bound exceeds maximum renderable coordinate: {error}")
        })
}

fn validate_nonzero(value: f64, field: &str) -> Result<(), String> {
    validate_finite(value, field)?;
    if value != 0.0 {
        Ok(())
    } else {
        Err(format!("{field} must be nonzero"))
    }
}

fn validate_finite_f32(value: f32, field: &str) -> Result<(), String> {
    if value.is_finite() {
        Ok(())
    } else {
        Err(format!("{field} must be finite"))
    }
}

fn validate_positive_f32(value: f32, field: &str) -> Result<(), String> {
    validate_finite_f32(value, field)?;
    if value > 0.0 {
        Ok(())
    } else {
        Err(format!("{field} must be positive"))
    }
}

fn validate_unit_interval_f32(value: f32, field: &str) -> Result<(), String> {
    validate_finite_f32(value, field)?;
    if (0.0..=1.0).contains(&value) {
        Ok(())
    } else {
        Err(format!("{field} must be between 0 and 1"))
    }
}

fn validate_ordered_bounds(
    min: f64,
    max: f64,
    min_field: &str,
    max_field: &str,
) -> Result<(), String> {
    validate_finite(min, min_field)?;
    validate_finite(max, max_field)?;
    if min < max {
        Ok(())
    } else {
        Err(format!("{min_field} must be less than {max_field}"))
    }
}

fn validate_point2(point: Point2, field: &str) -> Result<(), String> {
    validate_finite(point.x, &format!("{field}.x"))?;
    validate_finite(point.y, &format!("{field}.y"))
}

fn validate_point3(point: Point3D, field: &str) -> Result<(), String> {
    validate_finite(point.x, &format!("{field}.x"))?;
    validate_finite(point.y, &format!("{field}.y"))?;
    validate_finite(point.z, &format!("{field}.z"))
}

fn validate_nonzero_direction_2d(start: Point2, end: Point2, field: &str) -> Result<(), String> {
    let length = (end.x - start.x).hypot(end.y - start.y);
    if length.is_finite() && length > 0.0 {
        Ok(())
    } else {
        Err(format!("{field} direction must be finite and nonzero"))
    }
}

fn validate_nonzero_direction_3d(start: Point3D, end: Point3D, field: &str) -> Result<(), String> {
    let length = (end.x - start.x)
        .hypot(end.y - start.y)
        .hypot(end.z - start.z);
    if length.is_finite() && length > 0.0 {
        Ok(())
    } else {
        Err(format!("{field} must be finite and nonzero"))
    }
}

fn validate_color(color: Color, field: &str) -> Result<(), String> {
    validate_finite_f32(color.r, &format!("{field}.r"))?;
    validate_finite_f32(color.g, &format!("{field}.g"))?;
    validate_finite_f32(color.b, &format!("{field}.b"))?;
    validate_finite_f32(color.a, &format!("{field}.a"))
}

fn validate_optional_color(color: Option<Color>, field: &str) -> Result<(), String> {
    color.map_or(Ok(()), |color| validate_color(color, field))
}

fn validate_optional_finite(value: Option<f64>, field: &str) -> Result<(), String> {
    value.map_or(Ok(()), |value| validate_finite(value, field))
}

fn validate_optional_expr(expr: &Option<String>, field: &str) -> Result<(), String> {
    expr.as_deref().map_or(Ok(()), |expr| {
        validate_expr(expr).map_err(|error| format!("{field}: {error}"))
    })
}

fn validate_string(value: &str, field: &str) -> Result<(), String> {
    if value.len() > MAX_STRING_LENGTH {
        return Err(format!(
            "{field} length {} exceeds maximum {}",
            value.len(),
            MAX_STRING_LENGTH
        ));
    }
    if value.contains('\0') {
        return Err(format!("{field} must not contain NUL"));
    }
    if value.contains('\u{FEFF}') {
        return Err(format!("{field} must not contain BOM"));
    }
    Ok(())
}

fn validate_finite_slice(values: &[f64], field: &str) -> Result<(), String> {
    if values.len() > MAX_ARRAY_LENGTH {
        return Err(format!("{field} length exceeds maximum {MAX_ARRAY_LENGTH}"));
    }
    for (index, value) in values.iter().copied().enumerate() {
        validate_finite(value, &format!("{field}[{index}]"))?;
    }
    Ok(())
}

fn validate_fit_metadata(doc: &Document, fit: &crate::FitMetadata) -> Result<(), String> {
    let Some(GeoObject::DataTable(table)) = doc.get_object(fit.source) else {
        return Err("Function fit source must reference a DataTable".to_string());
    };
    let Some(expected_coefficients) = fit.kind.coefficient_count() else {
        return Err("Function fit model has an invalid parameter count".to_string());
    };
    if fit.coefficients.len() != expected_coefficients {
        return Err(format!(
            "Function fit expected {expected_coefficients} coefficients but found {}",
            fit.coefficients.len()
        ));
    }
    if fit.diagnostics.residuals.len() != table.xs.len() {
        return Err("Function fit residuals must match the source DataTable rows".to_string());
    }
    validate_parameter_slice(&fit.coefficients, "Function.fit.coefficients")?;
    validate_finite(fit.x_offset, "Function.fit.x_offset")?;
    validate_positive(fit.x_scale, "Function.fit.x_scale")?;
    validate_finite_slice(&fit.diagnostics.residuals, "Function.fit.residuals")?;
    validate_finite(fit.diagnostics.rmse, "Function.fit.rmse")?;
    if fit.diagnostics.rmse < 0.0 {
        return Err("Function.fit.rmse must be nonnegative".to_string());
    }
    validate_finite(fit.diagnostics.r_squared, "Function.fit.r_squared")
}

fn validate_parameter_slice(values: &[f64], field: &str) -> Result<(), String> {
    if values.len() > MAX_OBJECT_PARAMETERS {
        return Err(format!(
            "{field} length exceeds maximum {MAX_OBJECT_PARAMETERS}"
        ));
    }
    for (index, value) in values.iter().copied().enumerate() {
        validate_finite(value, &format!("{field}[{index}]"))?;
    }
    Ok(())
}

fn validate_plot_bounds(
    x_min: f64,
    x_max: f64,
    y_min: f64,
    y_max: f64,
    object: &str,
) -> Result<(), String> {
    validate_finite(x_min, &format!("{object}.x_min"))?;
    validate_finite(x_max, &format!("{object}.x_max"))?;
    validate_finite(y_min, &format!("{object}.y_min"))?;
    validate_finite(y_max, &format!("{object}.y_max"))
}

fn validate_contours(curve: &crate::ImplicitCurveObj) -> Result<(), String> {
    let Some(levels) = &curve.contour_levels else {
        return validate_contour_colors(curve.contour_colors.as_deref());
    };
    if levels.len() > MAX_CONTOUR_LEVELS {
        return Err(format!(
            "ImplicitCurve contour level count {} exceeds maximum {}",
            levels.len(),
            MAX_CONTOUR_LEVELS
        ));
    }
    let work = levels
        .len()
        .checked_mul(MAX_IMPLICIT_GRID_CELLS)
        .ok_or_else(|| "ImplicitCurve contour work budget overflowed".to_string())?;
    if work > MAX_CONTOUR_WORK_UNITS {
        return Err(format!(
            "ImplicitCurve contour work budget {} exceeds maximum {}",
            work, MAX_CONTOUR_WORK_UNITS
        ));
    }
    for (index, level) in levels.iter().copied().enumerate() {
        validate_finite(level, &format!("ImplicitCurve contour level {index}"))?;
        if levels[..index].contains(&level) {
            return Err("ImplicitCurve contains duplicate contour levels".to_string());
        }
    }
    validate_contour_colors(curve.contour_colors.as_deref())
}

fn validate_contour_colors(colors: Option<&[Color]>) -> Result<(), String> {
    let Some(colors) = colors else {
        return Ok(());
    };
    if colors.len() > MAX_CONTOUR_LEVELS {
        return Err(format!(
            "ImplicitCurve contour color count {} exceeds maximum {}",
            colors.len(),
            MAX_CONTOUR_LEVELS
        ));
    }
    for (index, color) in colors.iter().copied().enumerate() {
        validate_color(color, &format!("ImplicitCurve.contour_colors[{index}]"))?;
    }
    Ok(())
}

fn validate_expr(expr: &str) -> Result<(), String> {
    if expr.len() > MAX_EXPR_LENGTH {
        return Err(format!(
            "Expression length {} exceeds maximum {}",
            expr.len(),
            MAX_EXPR_LENGTH
        ));
    }
    Ok(())
}
