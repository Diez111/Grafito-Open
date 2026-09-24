//! Document validation to mitigate untrusted save-file DoS.

use crate::error::CoreError;
use crate::{pencil::MAX_PENCIL_POINTS, Document, GeoObject};
use grafito_geometry::{
    Color, Point2, Point3D, RegularPolytopeProjectionError, RegularPolytopeProjectionPlan,
    MAX_REGULAR_POLYTOPE_DIMENSION, MIN_REGULAR_POLYTOPE_DIMENSION,
};
use serde_json::Value;

// Validación Jacobiana para Transformed usa aritmética compleja y ValidatedMatrix.
// No se toca UI; solo validación fail-closed en core.

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
/// Máximo de ítems de una lista persistible (P1).
pub const MAX_LIST_LENGTH: usize = 10_000;
/// Profundidad máxima de anidamiento de listas (P1).
pub const MAX_LIST_DEPTH: usize = 8;
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
/// Máximo de entradas en `Document::object_scripts` (una por etiqueta a lo
/// sumo; nunca puede superar la cantidad de objetos del documento).
pub const MAX_OBJECT_SCRIPTS: usize = MAX_OBJECT_COUNT;
/// Bytes máximos de un guion individual (`on_click`/`on_update`/`on_load`).
/// 4 KiB cubre con holgura todo lo almacenable por la vía validada
/// (`check_script_allowlist` en `grafito-command` limita el guion total a
/// `MAX_EXPR_LENGTH` = 2000 caracteres).
pub const MAX_SCRIPT_BYTES: usize = 4_096;
/// Pasos máximos de un guion almacenado (paridad con `MAX_GGT_STEPS` de
/// custom tools en `grafito-command`; la expansión en ejecución tiene su
/// propio presupuesto aparte).
pub const MAX_SCRIPT_STEPS: usize = 100;
/// Bytes totales de todos los guiones del documento (defensa ante
/// `object_scripts` gigante que igual pasaría el tope de 10 MiB de JSON).
pub const MAX_OBJECT_SCRIPTS_TOTAL_BYTES: usize = 1_048_576;

/// Comandos canónicos permitidos dentro de guiones almacenados.
///
/// Espejo local de `GGBSCRIPT_ALLOWLIST`
/// (`crates/grafito-command/src/ggbscript.rs`): core no puede depender de
/// `grafito-command` (dependencia circular: command depende de core), así
/// que la lista vive acá como fuente de validación en reposo y allá como
/// fuente de validación al guardar/ejecutar. Si se agrega un comando al
/// subset, hay que actualizar ambas y el test pinneado de allá
/// (`auto_script_allowlist_is_benign_and_pinned`).
pub const SCRIPT_ALLOWLIST: &[&str] = &[
    "SetValue",
    "Show",
    "Hide",
    "ZoomIn",
    "ZoomOut",
    "PlayPause",
    "If",
    "Repeat",
    "Button",
    "Checkbox",
    "InputBox",
    "TextField",
    "DefineTool",
    "LoadTool",
];

/// Wrapper fail-closed que garantiza que el `Document` interno pasó `validate_document`.
///
/// Uso: `ValidatedDocument::try_new(doc)?` antes de persistir o de exponer un
/// snapshot al render. Los mapas de `Document` (`objects`, `variables`,
/// `variable_meta`, `live_sequences`, `variables_assumptions`,
/// `next_label_number`, `spreadsheet_coordinate_points`) ya son `BTreeMap`
/// (determinismo total); el wrapper evita que un documento a medio mutar
/// escape de `detached_clone` → `commit`.
///
/// # Cableado — fail-closed
///
/// Sitios que usan el wrapper vía `try_new_typed` (no hay 0 call-sites):
/// `persistence::serialize_document` (`persistence.rs`) valida antes de
/// serializar. `Document::commit` y `ChangeSet::restore` validan con
/// `validate_document` directo sobre el staged/snapshot; migrarlos a
/// `try_new_typed` queda como mejora sin cambio de comportamiento
/// (ambas vías corren la misma `validate_document`).
///
/// ```ignore
/// // 1) Document::commit (document.rs ~384):
/// // antes: crate::validation::validate_document(&staged)?;
/// // después:
/// let _validated = ValidatedDocument::try_new_typed(staged.clone())?;
/// // o: ValidatedDocument::try_new(staged.clone()).map_err(CoreError::Validation)?;
///
/// // 2) ChangeSet::restore (document.rs ~184):
/// // antes: crate::validation::validate_document(snapshot)?;
/// // después:
/// let _validated = ValidatedDocument::try_new_typed(snapshot.clone())?;
/// // El snapshot ya validado se usa para el restore; evita que un snapshot
/// // corrupto entre al documento vivo.
///
/// // 3) persistence::serialize_document (persistence.rs ~54):
/// // antes: validate_document(document).map_err(SemanticValidation)?;
/// // después:
/// let validated = ValidatedDocument::try_new_typed(document.clone())?;
/// let envelope = DocumentEnvelope { document: validated.into_inner(), .. };
/// // Alternativa sin clonar el documento si ya se valida por referencia:
/// // validate_document_typed(document)?;
///
/// // Helper listo para persistencia sin tocar document.rs:
/// // let json = validate_and_serialize(document)?;
/// // let json = validate_and_serialize_envelope(document, version)?;
/// ```
///
/// Si `document.rs` es owned por otro agente, no se edita directamente:
/// en su lugar `validate_and_serialize` y `try_new_typed` quedan expuestos
/// para que `persistence.rs` los use y document.rs pueda migrar después
/// sin romper el lifecycle `Empty -> Loading -> Validating -> Ready -> Mutating -> Persisting`.
#[derive(Debug, Clone)]
pub struct ValidatedDocument(pub crate::Document);

impl ValidatedDocument {
    /// Compat: retorna `String` para call-sites y tests legacy.
    /// Nuevos call-sites deben preferir [`Self::try_new_typed`] que retorna [`CoreError`].
    pub fn try_new(doc: crate::Document) -> Result<Self, String> {
        validate_document(&doc)?;
        Ok(Self(doc))
    }

    /// Variante tipada fail-closed: `String` → `CoreError::Validation`.
    /// No rompe tests existentes (pueden usar `.unwrap()` igual) y permite `?`
    /// propagar `CoreError` sin parsear texto.
    pub fn try_new_typed(doc: crate::Document) -> Result<Self, CoreError> {
        validate_document_typed(&doc)?;
        Ok(Self(doc))
    }

    /// Valida por referencia sin consumir el documento (útil para `&Document`).
    pub fn validate_ref(doc: &crate::Document) -> Result<(), CoreError> {
        validate_document_typed(doc)
    }

    pub fn inner(&self) -> &crate::Document {
        &self.0
    }
    pub fn into_inner(self) -> crate::Document {
        self.0
    }
}

/// Helper fail-closed para `persistence.rs` sin necesidad de tocar `document.rs`.
///
/// Valida el documento vía [`ValidatedDocument::try_new_typed`] y serializa el
/// `Document` interno a JSON (no envelope). Para envelope versionado usar
/// [`validate_and_serialize_envelope`]. Retorna `CoreError::Validation` si el
/// documento no pasa `validate_document`, o `CoreError::Persistence` si la
/// serialización falla o excede `MAX_DOCUMENT_SIZE_BYTES`.
///
/// # Uso en persistence.rs
/// ```ignore
/// use crate::validation::validate_and_serialize;
///
/// pub fn serialize_document(doc: &Document) -> Result<String, DocumentPersistenceError> {
///     let json = validate_and_serialize(doc).map_err(|e| match e {
///         CoreError::Validation(msg) => DocumentPersistenceError::SemanticValidation(msg),
///         other => DocumentPersistenceError::SemanticValidation(other.to_string()),
///     })?;
///     // envolver en DocumentEnvelope si se necesita
///     Ok(json)
/// }
/// ```
pub fn validate_and_serialize(doc: &crate::Document) -> Result<String, CoreError> {
    let validated = ValidatedDocument::try_new_typed(doc.clone())?;
    let json = serde_json::to_string(&validated.into_inner())
        .map_err(|error| CoreError::Persistence(error.to_string()))?;
    if json.len() > MAX_DOCUMENT_SIZE_BYTES {
        return Err(CoreError::Validation(format!(
            "Document size {} exceeds maximum {}",
            json.len(),
            MAX_DOCUMENT_SIZE_BYTES
        )));
    }
    Ok(json)
}

/// Variante que serializa directamente el envelope versionado. Evita duplicar
/// lógica de `persistence::serialize_document` cuando el caller solo necesita
/// un JSON listo para `write_atomic`.
pub fn validate_and_serialize_envelope(
    doc: &crate::Document,
    schema_version: u32,
    producer_version: String,
) -> Result<String, CoreError> {
    let validated = ValidatedDocument::try_new_typed(doc.clone())?;
    let envelope = crate::persistence::DocumentEnvelope {
        schema_version,
        producer_version,
        document: validated.into_inner(),
    };
    let json = serde_json::to_string_pretty(&envelope)
        .map_err(|error| CoreError::Persistence(error.to_string()))?;
    if json.len() > MAX_DOCUMENT_SIZE_BYTES {
        return Err(CoreError::Validation(format!(
            "Document size {} exceeds maximum {}",
            json.len(),
            MAX_DOCUMENT_SIZE_BYTES
        )));
    }
    Ok(json)
}

/// Validador tipado: `String` → `CoreError::Validation`. Mantiene compat con
/// call-sites que aún esperan `String` via `validate_document`, pero nuevos
/// call-sites deben usar este para `?` con `CoreError`.
pub fn validate_document_typed(doc: &Document) -> Result<(), CoreError> {
    validate_document(doc).map_err(CoreError::Validation)
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
    validate_positive(view.scale, "Document.view.scale")?;
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

    // Capas Q2: fail-closed ante JSON editado a mano (el único escritor
    // válido es `Document::set_layer`, que ya acota a 0..=255).
    for (id, layer) in doc.layers() {
        if *layer > crate::symbolic::exchange::MAX_LAYERS {
            return Err(format!(
                "Layer {layer} for object {id} exceeds maximum {}",
                crate::symbolic::exchange::MAX_LAYERS
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

    // Libro de pizarra (Ola 4): la carga acota en deserialización, pero un
    // documento construido en memoria también debe respetar hojas,
    // elementos y — sobre todo — una cota TOTAL de bytes de trazo (el
    // espejo `doc.whiteboard` no se cuenta: es la hoja actual duplicada).
    if doc.whiteboard_pages.len() > crate::document::MAX_WHITEBOARD_PAGES {
        return Err(format!(
            "Whiteboard supera {} hojas",
            crate::document::MAX_WHITEBOARD_PAGES
        ));
    }
    let mut whiteboard_bytes = 0usize;
    for page in &doc.whiteboard_pages {
        if page.doc.len() > crate::document::MAX_WHITEBOARD_ELEMENTS_PER_PAGE {
            return Err(format!(
                "Whiteboard supera {} elementos por hoja",
                crate::document::MAX_WHITEBOARD_ELEMENTS_PER_PAGE
            ));
        }
        for element in page.doc.elements() {
            whiteboard_bytes = whiteboard_bytes.saturating_add(match element {
                grafito_whiteboard::WhiteboardElement::Stroke { points, .. } => {
                    points.len().saturating_mul(16)
                }
                grafito_whiteboard::WhiteboardElement::Text { text, .. } => text.len(),
                _ => 64,
            });
        }
    }
    if whiteboard_bytes > crate::document::MAX_WHITEBOARD_TOTAL_BYTES {
        return Err(format!(
            "Whiteboard supera {} MiB de trazo total",
            crate::document::MAX_WHITEBOARD_TOTAL_BYTES / (1024 * 1024)
        ));
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

    // Secuencias vivas — cotas y referencias.
    if doc.live_sequences.len() > crate::document::MAX_LIVE_SEQUENCES {
        return Err(format!(
            "Document contains {} live sequences, maximum is {}",
            doc.live_sequences.len(),
            crate::document::MAX_LIVE_SEQUENCES
        ));
    }
    for (id, binding) in &doc.live_sequences {
        if doc.get_object(*id).is_none() {
            return Err(format!("LiveSequence {id} references missing object"));
        }
        if !matches!(doc.get_object(*id), Some(crate::GeoObject::DataTable(_))) {
            return Err(format!("LiveSequence {id} must reference a DataTable"));
        }
        validate_string(&binding.expr, "LiveSequence.expr")?;
        validate_string(&binding.var, "LiveSequence.var")?;
        validate_string(&binding.start_expr, "LiveSequence.start_expr")?;
        validate_string(&binding.end_expr, "LiveSequence.end_expr")?;
        if binding.expr.len() > MAX_EXPR_LENGTH
            || binding.start_expr.len() > MAX_EXPR_LENGTH
            || binding.end_expr.len() > MAX_EXPR_LENGTH
        {
            return Err("LiveSequence expression exceeds maximum length".to_string());
        }
    }

    validate_object_scripts(doc)?;

    Ok(())
}

/// Valida los guiones almacenados (`object_scripts` + `on_load_script`).
///
/// Fail-closed: un guion fuera del allowlist (o que exceda cotas/charset)
/// rechaza el documento entero, así `ValidatedDocument::try_new` tampoco lo
/// acepta. Espeja el chequeo al guardar de `grafito-command`
/// (`check_script_allowlist`): la ejecución re-valida de todos modos
/// (defensa en profundidad), pero un archivo editado a mano nunca debe
/// llegar a la piel con script arbitrario.
pub fn validate_object_scripts(doc: &Document) -> Result<(), String> {
    if doc.object_scripts.len() > MAX_OBJECT_SCRIPTS {
        return Err(format!(
            "Document contains {} object scripts, maximum is {}",
            doc.object_scripts.len(),
            MAX_OBJECT_SCRIPTS
        ));
    }
    let mut total_bytes = 0usize;
    for (label, scripts) in &doc.object_scripts {
        validate_string(label, "ObjectScripts label")?;
        for (kind, script) in [
            ("on_click", &scripts.on_click),
            ("on_update", &scripts.on_update),
        ] {
            if let Some(script) = script {
                total_bytes = total_bytes.saturating_add(script.len());
                validate_script(script, &format!("ObjectScripts[{label}].{kind}"))?;
            }
        }
    }
    if let Some(script) = &doc.on_load_script {
        total_bytes = total_bytes.saturating_add(script.len());
        validate_script(script, "Document.on_load_script")?;
    }
    if total_bytes > MAX_OBJECT_SCRIPTS_TOTAL_BYTES {
        return Err(format!(
            "Object scripts exceed {MAX_OBJECT_SCRIPTS_TOTAL_BYTES} total bytes ({total_bytes})"
        ));
    }
    Ok(())
}

/// Valida un guion contra cotas, charset y allowlist de comandos.
///
/// Espeja `check_script_allowlist` (`grafito-command/src/ggbscript.rs`) sin
/// depender de ese crate: longitud total, cantidad de pasos, balance de
/// delimitadores (con `"` protegiendo `;`) y cabeza de cada paso contra
/// [`SCRIPT_ALLOWLIST`]. Sin allowlist que lo avale → `Err`.
pub fn validate_script(script: &str, what: &str) -> Result<(), String> {
    if script.len() > MAX_SCRIPT_BYTES {
        return Err(format!(
            "{what} excede {MAX_SCRIPT_BYTES} bytes (tiene {})",
            script.len()
        ));
    }
    for ch in script.chars() {
        if ch == '\0' {
            return Err(format!("{what} no debe contener NUL"));
        }
        if ch == '\u{FEFF}' {
            return Err(format!("{what} no debe contener BOM"));
        }
        if ch.is_control() && !matches!(ch, '\t' | '\n' | '\r') {
            return Err(format!("{what} no debe contener caracteres de control"));
        }
    }
    let steps = split_script_steps(script).map_err(|e| format!("{what}: {e}"))?;
    if steps.is_empty() {
        return Err(format!("{what} no contiene pasos"));
    }
    if steps.len() > MAX_SCRIPT_STEPS {
        return Err(format!(
            "{what} excede {MAX_SCRIPT_STEPS} pasos (tiene {})",
            steps.len()
        ));
    }
    for step in &steps {
        if step.len() > MAX_EXPR_LENGTH {
            return Err(format!(
                "{what}: el paso excede {MAX_EXPR_LENGTH} caracteres"
            ));
        }
        let head = script_step_head(step).ok_or_else(|| {
            format!("{what}: paso no es un comando válido del subset GGBScript: '{step}'")
        })?;
        if !SCRIPT_ALLOWLIST
            .iter()
            .any(|name| name.eq_ignore_ascii_case(head))
        {
            return Err(format!(
                "{what}: paso '{step}' usa '{head}', fuera del subset GGBScript (permitidos: {})",
                SCRIPT_ALLOWLIST.join(", ")
            ));
        }
    }
    Ok(())
}

/// Parte un guion en pasos por `;` de nivel superior (espejo de
/// `split_script_commands` en `grafito-command`: `"` protege `;` y se exige
/// balance de `()[]{{}}`).
fn split_script_steps(script: &str) -> Result<Vec<String>, String> {
    let mut steps = Vec::new();
    let mut delimiters = Vec::new();
    let mut start = 0;
    let mut in_string = false;
    for (index, ch) in script.char_indices() {
        if ch == '"' {
            in_string = !in_string;
            continue;
        }
        if in_string {
            continue;
        }
        match ch {
            '(' | '[' | '{' => delimiters.push(ch),
            ')' | ']' | '}' => {
                let expected = match ch {
                    ')' => '(',
                    ']' => '[',
                    '}' => '{',
                    _ => return Err("guion con delimitadores desbalanceados".into()),
                };
                if delimiters.pop() != Some(expected) {
                    return Err("guion con delimitadores desbalanceados".into());
                }
            }
            ';' if delimiters.is_empty() => {
                let step = script[start..index].trim();
                if !step.is_empty() {
                    steps.push(step.to_string());
                }
                start = index + ch.len_utf8();
            }
            _ => {}
        }
    }
    if in_string {
        return Err("guion con comilla sin cerrar".into());
    }
    if !delimiters.is_empty() {
        return Err("guion con delimitadores desbalanceados".into());
    }
    let tail = script[start..].trim();
    if !tail.is_empty() {
        steps.push(tail.to_string());
    }
    Ok(steps)
}

/// Cabeza de un paso (`SetValue` en `SetValue[x, 1]`): corrida inicial
/// alfabética seguida de `[` o `(`. `None` si no hay forma de comando.
fn script_step_head(step: &str) -> Option<&str> {
    let step = step.trim();
    let end = step
        .find(|c: char| !c.is_ascii_alphabetic())
        .unwrap_or(step.len());
    if end == 0 {
        return None;
    }
    let rest = step[end..].trim_start();
    if rest.starts_with('[') || rest.starts_with('(') {
        Some(&step[..end])
    } else {
        None
    }
}

/// Valida la semántica propia de un objeto candidato y sus referencias.
///
/// No comprueba capacidad, colisiones de identificador ni unicidad de etiqueta;
/// esas políticas pertenecen a [`Document::try_add_object`].
pub fn validate_object_candidate(doc: &Document, obj: &GeoObject) -> Result<(), String> {
    validate_geo_object(doc, obj, 0)
}

/// Variante tipada de `validate_object_candidate` para Transformed.
pub fn validate_object_candidate_typed(doc: &Document, obj: &GeoObject) -> Result<(), CoreError> {
    validate_geo_object_typed(doc, obj, 0)
}

fn validate_geo_object_typed(
    doc: &Document,
    obj: &GeoObject,
    depth: usize,
) -> Result<(), CoreError> {
    for target in obj.referenced_object_ids() {
        if target == obj.id() || doc.get_object(target).is_none() {
            return Err(CoreError::Validation(format!(
                "{} target {} is missing",
                obj.name(),
                target
            )));
        }
    }

    if let GeoObject::Transformed(o) = obj {
        validate_transformed_depth_typed(depth)?;
        validate_expr_typed(&o.complex_expr).map_err(|e| match e {
            CoreError::Validation(msg) => CoreError::InvalidExpression {
                expression: o.complex_expr.clone(),
                reason: msg,
            },
            other => other,
        })?;
        if let Some(compiled) = &o.compiled_expr {
            validate_expr_typed(compiled).map_err(|e| match e {
                CoreError::Validation(msg) => CoreError::InvalidExpression {
                    expression: compiled.clone(),
                    reason: msg,
                },
                other => other,
            })?;
        }
        validate_transformed_jacobian_typed(&o.complex_expr)?;
        return validate_geo_object_typed(doc, &o.inner, depth + 1);
    }

    // Para objetos no-Transformed, delegar al match legacy completo
    // y convertir el error String en CoreError::Validation para tipado.
    validate_geo_object_legacy_match(doc, obj).map_err(CoreError::Validation)
}

fn validate_geo_object(doc: &Document, obj: &GeoObject, depth: usize) -> Result<(), String> {
    validate_geo_object_typed(doc, obj, depth).map_err(|e| e.to_string())
}

fn validate_geo_object_legacy_match(doc: &Document, obj: &GeoObject) -> Result<(), String> {
    for target in obj.referenced_object_ids() {
        if target == obj.id() || doc.get_object(target).is_none() {
            return Err(format!("{} target {} is missing", obj.name(), target));
        }
    }
    // Transformed ya handled en la capa tipada; si llega aquí siendo Transformed es bug.
    if let GeoObject::Transformed(_) = obj {
        return Ok(());
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
            // Validación de colinealidad/degeneración vía área de regiones.
            //
            // FIX: el shoelace CON SIGNO (área algebraica neta) rechazaba
            // toda curva auto-intersecada —Lissajous, lazos, lemniscatas—
            // porque las regiones se cancelan y el área neta da ~0. El signo
            // sirve para la orientación, NO para detectar degeneración: acá
            // se suman las áreas ABSOLUTAS de los triángulos abanicados desde
            // el primer vértice (invariante ante traslación y ante
            // auto-intersección; da 0 sii todos los puntos son colineales).
            if o.vertices.len() >= 3 {
                let n = o.vertices.len();
                let p0 = o.vertices[0];
                let mut area2_abs = 0.0;
                let mut perim = 0.0;
                for i in 0..n {
                    let p1 = o.vertices[i];
                    let p2 = o.vertices[(i + 1) % n];
                    perim += (p2.x - p1.x).hypot(p2.y - p1.y);
                }
                for i in 1..n.saturating_sub(1) {
                    let p1 = o.vertices[i];
                    let p2 = o.vertices[i + 1];
                    let cross = (p1.x - p0.x) * (p2.y - p0.y) - (p2.x - p0.x) * (p1.y - p0.y);
                    area2_abs += cross.abs();
                }
                if !area2_abs.is_finite() || !perim.is_finite() {
                    return Err("Polygon vertices must be finite".to_string());
                }
                if area2_abs <= GEOM_EPS * perim * perim {
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
        GeoObject::Polyline(o) => {
            // Cadena abierta: al menos 2 puntos, sin chequeo de colinealidad
            // (una recta quebrada recta es válida) ni de cierre.
            if o.points.len() < 2 {
                return Err("Polyline requires at least 2 points".to_string());
            }
            if o.points.len() > MAX_POLYGON_VERTICES {
                return Err(format!(
                    "Polyline points {} exceeds maximum {}",
                    o.points.len(),
                    MAX_POLYGON_VERTICES
                ));
            }
            for (index, point) in o.points.iter().copied().enumerate() {
                validate_point2(point, &format!("Polyline.points[{index}]"))?;
            }
            validate_positive_f32(o.width, "Polyline.width")?;
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
            validate_finite_f32(o.rotation, "Text.rotation")?;
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
        GeoObject::Arc(o) => {
            validate_point2(o.center, "Arc.center")?;
            validate_positive(o.radius, "Arc.radius")?;
            validate_finite(o.start_angle, "Arc.start_angle")?;
            validate_finite(o.end_angle, "Arc.end_angle")?;
            validate_positive_f32(o.width, "Arc.width")?;
        }
        GeoObject::Sector(o) => {
            validate_point2(o.center, "Sector.center")?;
            validate_positive(o.radius, "Sector.radius")?;
            validate_finite(o.start_angle, "Sector.start_angle")?;
            validate_finite(o.end_angle, "Sector.end_angle")?;
            validate_positive_f32(o.width, "Sector.width")?;
            validate_optional_color(o.fill_color, "Sector.fill_color")?;
        }
        GeoObject::BezierCurve(o) => {
            if o.control_points.len() < 2 {
                return Err("BezierCurve requiere al menos 2 puntos de control".to_string());
            }
            if o.control_points.len() > MAX_POLYGON_VERTICES {
                return Err(format!(
                    "BezierCurve puntos {} excede máximo {}",
                    o.control_points.len(),
                    MAX_POLYGON_VERTICES
                ));
            }
            for (idx, pt) in o.control_points.iter().enumerate() {
                validate_point2(*pt, &format!("BezierCurve.control_points[{idx}]"))?;
            }
            validate_positive_f32(o.width, "BezierCurve.width")?;
        }
        GeoObject::Spline(o) => {
            if o.points.len() < 2 {
                return Err("Spline requiere al menos 2 puntos".to_string());
            }
            if o.points.len() > MAX_POLYGON_VERTICES {
                return Err(format!(
                    "Spline puntos {} excede máximo {}",
                    o.points.len(),
                    MAX_POLYGON_VERTICES
                ));
            }
            for (idx, pt) in o.points.iter().enumerate() {
                validate_point2(*pt, &format!("Spline.points[{idx}]"))?;
            }
            validate_positive_f32(o.width, "Spline.width")?;
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
        GeoObject::Platonic3D(o) => {
            validate_point3(o.center, "Platonic3D.center")?;
            validate_positive(o.edge_length, "Platonic3D.edge_length")?;
            if grafito_geometry::platonic_mesh(o.kind.to_geometry_solid(), o.edge_length).is_err() {
                return Err(
                    "Platonic3D malla no construible con esa arista (debe ser finita y positiva)"
                        .to_string(),
                );
            }
            validate_positive_f32(o.width, "Platonic3D.width")?;
            validate_optional_color(o.fill_color, "Platonic3D.fill_color")?;
        }
        GeoObject::InfiniteCone3D(o) => {
            validate_point3(o.apex, "InfiniteCone3D.apex")?;
            validate_point3(o.direction, "InfiniteCone3D.direction")?;
            let dx = o.direction.x - o.apex.x;
            let dy = o.direction.y - o.apex.y;
            let dz = o.direction.z - o.apex.z;
            if !dx.is_finite() || !dy.is_finite() || !dz.is_finite() {
                return Err("InfiniteCone3D.direction no finita".to_string());
            }
            if dx.hypot(dy).hypot(dz) <= 1e-12 {
                return Err("InfiniteCone3D requiere dirección no nula".to_string());
            }
            if !o.half_angle_rad.is_finite()
                || o.half_angle_rad <= 0.0
                || o.half_angle_rad >= std::f64::consts::FRAC_PI_2
            {
                return Err("InfiniteCone3D.half_angle debe estar en (0, π/2)".to_string());
            }
            validate_positive_f32(o.width, "InfiniteCone3D.width")?;
        }
        GeoObject::InfiniteCylinder3D(o) => {
            validate_point3(o.base_point, "InfiniteCylinder3D.base_point")?;
            validate_point3(o.direction, "InfiniteCylinder3D.direction")?;
            let dx = o.direction.x - o.base_point.x;
            let dy = o.direction.y - o.base_point.y;
            let dz = o.direction.z - o.base_point.z;
            if !dx.is_finite() || !dy.is_finite() || !dz.is_finite() {
                return Err("InfiniteCylinder3D.direction no finita".to_string());
            }
            if dx.hypot(dy).hypot(dz) <= 1e-12 {
                return Err("InfiniteCylinder3D requiere dirección no nula".to_string());
            }
            validate_positive(o.radius, "InfiniteCylinder3D.radius")?;
            validate_positive_f32(o.width, "InfiniteCylinder3D.width")?;
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
        GeoObject::Prism3D(o) => {
            if o.base_vertices.len() < 3 {
                return Err("Prism3D requiere al menos 3 vértices base".to_string());
            }
            if o.base_vertices.len() > MAX_POLYGON_VERTICES {
                return Err(format!(
                    "Prism3D vertices {} excede máximo {}",
                    o.base_vertices.len(),
                    MAX_POLYGON_VERTICES
                ));
            }
            for (idx, pt) in o.base_vertices.iter().enumerate() {
                validate_point3(*pt, &format!("Prism3D.base_vertices[{idx}]"))?;
            }
            validate_point3(o.direction, "Prism3D.direction")?;
            let len = o.direction.x.hypot(o.direction.y).hypot(o.direction.z);
            if !len.is_finite() || len <= GEOM_EPS {
                return Err("Prism3D.direction debe ser un vector no nulo y finito".to_string());
            }
            validate_positive_f32(o.width, "Prism3D.width")?;
            validate_optional_color(o.fill_color, "Prism3D.fill_color")?;
            // Valida que los vértices no excedan la cota global de coordenadas.
            for (idx, pt) in o.base_vertices.iter().enumerate() {
                if pt.x.abs() > grafito_geometry::MAX_WORLD_COORDINATE
                    || pt.y.abs() > grafito_geometry::MAX_WORLD_COORDINATE
                    || pt.z.abs() > grafito_geometry::MAX_WORLD_COORDINATE
                {
                    return Err(format!(
                        "Prism3D.base_vertices[{idx}] excede la cota renderizable"
                    ));
                }
            }
        }
        GeoObject::Quadric3D(o) => {
            for (value, field) in [
                (o.a, "Quadric3D.a"),
                (o.b, "Quadric3D.b"),
                (o.c, "Quadric3D.c"),
                (o.d, "Quadric3D.d"),
                (o.e, "Quadric3D.e"),
                (o.f, "Quadric3D.f"),
                (o.g, "Quadric3D.g"),
                (o.h, "Quadric3D.h"),
                (o.i, "Quadric3D.i"),
                (o.j, "Quadric3D.j"),
            ] {
                validate_finite(value, field)?;
            }
            // Al menos un coeficiente cuadrático debe ser no nulo para ser cuádrica no degenerada.
            let quad_norm = o.a.abs() + o.b.abs() + o.c.abs() + o.d.abs() + o.e.abs() + o.f.abs();
            if quad_norm <= GEOM_EPS {
                return Err(
                    "Quadric3D: al menos un coeficiente cuadrático (a,b,c,d,e,f) debe ser no nulo"
                        .to_string(),
                );
            }
            validate_positive_f32(o.width, "Quadric3D.width")?;
        }
        GeoObject::ImplicitSurface3D(o) => {
            validate_expr(&o.expr)?;
            validate_ordered_bounds(
                o.x_min,
                o.x_max,
                "ImplicitSurface3D.x_min",
                "ImplicitSurface3D.x_max",
            )?;
            validate_ordered_bounds(
                o.y_min,
                o.y_max,
                "ImplicitSurface3D.y_min",
                "ImplicitSurface3D.y_max",
            )?;
            validate_ordered_bounds(
                o.z_min,
                o.z_max,
                "ImplicitSurface3D.z_min",
                "ImplicitSurface3D.z_max",
            )?;
            for (value, field) in [
                (o.x_min, "ImplicitSurface3D.x_min"),
                (o.x_max, "ImplicitSurface3D.x_max"),
                (o.y_min, "ImplicitSurface3D.y_min"),
                (o.y_max, "ImplicitSurface3D.y_max"),
                (o.z_min, "ImplicitSurface3D.z_min"),
                (o.z_max, "ImplicitSurface3D.z_max"),
            ] {
                validate_finite(value, field)?;
                if value.abs() > grafito_geometry::MAX_WORLD_COORDINATE {
                    return Err(format!("{field} excede la cota renderizable"));
                }
            }
            if o.cells < 1 || o.cells > grafito_geometry::GB_MAX_MARCHING_CELLS_PER_AXIS {
                return Err(format!(
                    "ImplicitSurface3D cells {} must be between 1 and {}",
                    o.cells,
                    grafito_geometry::GB_MAX_MARCHING_CELLS_PER_AXIS
                ));
            }
            validate_positive_f32(o.width, "ImplicitSurface3D.width")?;
            validate_optional_color(o.fill_color, "ImplicitSurface3D.fill_color")?;
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
        GeoObject::BarChart(o) => {
            validate_finite_slice(&o.data, "BarChart data")?;
            validate_plot_bounds(o.x_min, o.x_max, o.y_min, o.y_max, "BarChart")?;
            validate_positive_f32(o.width, "BarChart.width")?;
            validate_optional_color(o.fill_color, "BarChart.fill_color")?;
        }
        GeoObject::PieChart(o) => {
            validate_finite_slice(&o.data, "PieChart data")?;
            if o.data.iter().any(|value| *value < 0.0) {
                return Err("PieChart data must be non-negative".to_string());
            }
            let total: f64 = o.data.iter().sum();
            if !total.is_finite() || total <= 0.0 {
                return Err("PieChart data must have a positive total".to_string());
            }
            validate_finite(o.center.x, "PieChart.center.x")?;
            validate_finite(o.center.y, "PieChart.center.y")?;
            validate_finite(o.radius, "PieChart.radius")?;
            if o.radius <= 0.0 {
                return Err("PieChart radius must be positive".to_string());
            }
            validate_positive_f32(o.width, "PieChart.width")?;
            validate_optional_color(o.fill_color, "PieChart.fill_color")?;
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
        GeoObject::List(o) => {
            validate_list_items(&o.items, "List.items", 0)?;
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

/// Valida ítems de lista con cotas P1 (longitud, profundidad, finitud,
/// textos acotados). Recursiva acotada por `MAX_LIST_DEPTH`.
fn validate_list_items(items: &[crate::ListItem], field: &str, depth: usize) -> Result<(), String> {
    use crate::ListItem;
    if items.len() > MAX_LIST_LENGTH {
        return Err(format!("{field} length exceeds maximum {MAX_LIST_LENGTH}"));
    }
    if depth > MAX_LIST_DEPTH {
        return Err(format!("{field} depth exceeds maximum {MAX_LIST_DEPTH}"));
    }
    for (index, item) in items.iter().enumerate() {
        match item {
            ListItem::Scalar(value) => {
                validate_finite(*value, &format!("{field}[{index}]"))?;
            }
            ListItem::Text(text) => {
                validate_string(text, &format!("{field}[{index}]"))?;
            }
            ListItem::List(inner) => {
                validate_list_items(inner, &format!("{field}[{index}]"), depth + 1)?;
            }
        }
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

fn validate_expr_typed(expr: &str) -> Result<(), CoreError> {
    validate_expr(expr).map_err(CoreError::Validation)
}

/// Valida depth de anidamiento Transformed con error tipado.
pub fn validate_transformed_depth_typed(depth: usize) -> Result<(), CoreError> {
    if depth >= MAX_TRANSFORM_DEPTH {
        return Err(CoreError::TransformDepthExceeded {
            depth,
            maximum: MAX_TRANSFORM_DEPTH,
        });
    }
    Ok(())
}

/// Variante tipada de `validate_transformed_jacobian`.
/// Retorna `CoreError::TransformJacobianSingular` o `CoreError::InvalidExpression`.
pub fn validate_transformed_jacobian_typed(expr: &str) -> Result<(), CoreError> {
    match validate_transformed_jacobian(expr) {
        Ok(()) => Ok(()),
        Err(msg) if msg.contains("Transformed Jacobian singular") => {
            Err(CoreError::TransformJacobianSingular {
                expr: expr.to_string(),
                reason: msg,
            })
        }
        Err(msg) if msg.contains("complex_expr") => Err(CoreError::InvalidExpression {
            expression: expr.to_string(),
            reason: msg,
        }),
        Err(msg) => Err(CoreError::Validation(msg)),
    }
}

/// Valida que el Jacobiano de `complex_expr` no sea singular en muestreo.
///
/// Compila la expresión compleja y evalúa `det(J)` numéricamente en 4 puntos
/// alejados del origen. Si `|det| < 1e-12` o no finito en algún punto,
/// retorna `Err("Transformed Jacobian singular")`. Usa `ValidatedMatrix`
/// como wrapper fail-closed para la matriz Jacobiana 2×2.
pub(crate) fn validate_transformed_jacobian(expr: &str) -> Result<(), String> {
    const SINGULAR_MSG: &str = "Transformed Jacobian singular";
    // Compila la expresión compleja; si falla, propagamos error de sintaxis.
    let ast = grafito_complex::complex_expr::parse(expr)
        .map_err(|reason| format!("complex_expr inválida: {reason}"))?;

    // Puntos de muestreo deterministas, alejados del origen para no penalizar
    // ceros aislados (p. ej. `z^2` en 0) pero detectando colapsos globales.
    let samples = [
        num_complex::Complex64::new(1.0, 0.0),
        num_complex::Complex64::new(0.0, 1.0),
        num_complex::Complex64::new(1.0, 1.0),
        num_complex::Complex64::new(0.7, -0.3),
    ];
    const H: f64 = 1e-6;
    let mut any_success = false;

    for z0 in samples {
        // Evalúa f(z) en vecindad para derivadas centradas.
        let eval = |z: num_complex::Complex64| -> Option<num_complex::Complex64> {
            let mut env = std::collections::HashMap::new();
            env.insert("z".to_string(), z);
            // `ast.eval` ya maneja constantes pi/e/i; variables del documento no se
            // consideran en validación estática (se asume vacío).
            match ast.eval(&env) {
                Ok(value) if value.re.is_finite() && value.im.is_finite() => Some(value),
                _ => None,
            }
        };

        let f_px = eval(z0 + num_complex::Complex64::new(H, 0.0));
        let f_mx = eval(z0 - num_complex::Complex64::new(H, 0.0));
        let f_py = eval(z0 + num_complex::Complex64::new(0.0, H));
        let f_my = eval(z0 - num_complex::Complex64::new(0.0, H));

        let (Some(f_px), Some(f_mx), Some(f_py), Some(f_my)) = (f_px, f_mx, f_py, f_my) else {
            // Si no se puede evaluar en este punto (polo cercano), probamos siguiente.
            continue;
        };

        let df_dx = (f_px - f_mx) * (0.5 / H);
        let df_dy = (f_py - f_my) * (0.5 / H);

        let u_x = df_dx.re;
        let v_x = df_dx.im;
        let u_y = df_dy.re;
        let v_y = df_dy.im;

        if !u_x.is_finite() || !v_x.is_finite() || !u_y.is_finite() || !v_y.is_finite() {
            return Err(SINGULAR_MSG.to_string());
        }

        let det = u_x * v_y - u_y * v_x;
        if !det.is_finite() || det.abs() <= GEOM_EPS {
            return Err(SINGULAR_MSG.to_string());
        }

        // Validación adicional vía ValidatedMatrix (fail-closed).
        let matrix =
            grafito_geometry::matrices::Matrix::from_rows(vec![vec![u_x, u_y], vec![v_x, v_y]])
                .ok_or_else(|| SINGULAR_MSG.to_string())?;
        if grafito_geometry::matrices::ValidatedMatrix::try_new(matrix).is_err() {
            return Err(SINGULAR_MSG.to_string());
        }

        any_success = true;
    }

    if !any_success {
        // Ningún punto evaluable -> no se puede garantizar invertibilidad.
        return Err(SINGULAR_MSG.to_string());
    }

    Ok(())
}

#[cfg(test)]
mod tests_transformed_jacobian {
    use super::*;

    #[test]
    fn jacobian_rejects_constant_and_collapsed_expressions() {
        // Constante colapsa el objeto: det=0 en todo punto.
        assert!(validate_transformed_jacobian("0").is_err());
        assert!(validate_transformed_jacobian("1").is_err());
        assert!(validate_transformed_jacobian("z*0").is_err());
        // Expresión no holomorfa que colapsa dimensión (v=0).
        assert!(validate_transformed_jacobian("z*conj(z)").is_err());
        let err = validate_transformed_jacobian("0").unwrap_err();
        assert!(
            err.contains("Transformed Jacobian singular"),
            "mensaje esperado 'Transformed Jacobian singular', obtuvo {err}"
        );
    }

    #[test]
    fn jacobian_accepts_regular_transforms() {
        // Transformaciones regulares con det !=0 en muestreo.
        assert!(validate_transformed_jacobian("z").is_ok());
        assert!(validate_transformed_jacobian("z+2").is_ok());
        assert!(validate_transformed_jacobian("2*z").is_ok());
        assert!(validate_transformed_jacobian("z^2").is_ok()); // singular solo en 0, muestreo lo evita
        assert!(validate_transformed_jacobian("exp(z)").is_ok());
    }

    #[test]
    fn validated_matrix_rejects_singular_jacobian() {
        // Wrapper ValidatedMatrix debe rechazar matriz Jacobiana singular 2x2.
        let singular =
            grafito_geometry::matrices::Matrix::from_rows(vec![vec![0.0, 0.0], vec![0.0, 0.0]])
                .unwrap();
        assert!(
            grafito_geometry::matrices::ValidatedMatrix::try_new(singular).is_err(),
            "matriz nula debe ser singular"
        );
        let regular =
            grafito_geometry::matrices::Matrix::from_rows(vec![vec![1.0, 0.0], vec![0.0, 1.0]])
                .unwrap();
        assert!(
            grafito_geometry::matrices::ValidatedMatrix::try_new(regular).is_ok(),
            "identidad no debe ser singular"
        );
    }

    #[test]
    fn jacobian_typed_returns_core_error() {
        let err = validate_transformed_jacobian_typed("0").unwrap_err();
        match err {
            crate::CoreError::TransformJacobianSingular { expr, reason } => {
                assert_eq!(expr, "0");
                assert!(reason.contains("Transformed Jacobian singular"));
            }
            other => panic!("esperaba TransformJacobianSingular, obtuvo {other:?}"),
        }
        assert!(validate_transformed_jacobian_typed("z").is_ok());
        assert!(validate_transformed_jacobian_typed("exp(z)").is_ok());
    }

    #[test]
    fn depth_typed_rejects_64_and_accepts_63() {
        assert!(validate_transformed_depth_typed(63).is_ok());
        assert!(validate_transformed_depth_typed(0).is_ok());
        let err = validate_transformed_depth_typed(64).unwrap_err();
        match err {
            crate::CoreError::TransformDepthExceeded { depth, maximum } => {
                assert_eq!(depth, 64);
                assert_eq!(maximum, crate::validation::MAX_TRANSFORM_DEPTH);
                assert_eq!(maximum, 64);
            }
            other => panic!("esperaba TransformDepthExceeded, obtuvo {other:?}"),
        }
        let err = validate_transformed_depth_typed(100).unwrap_err();
        assert!(matches!(
            err,
            crate::CoreError::TransformDepthExceeded { .. }
        ));
    }

    #[test]
    fn object_candidate_typed_maps_jacobian_and_depth() {
        // jacobian singular a través de validate_object_candidate_typed
        let doc = crate::Document::new();
        let inner = crate::GeoObject::Point(crate::PointObj::new(grafito_geometry::Point2::new(
            0.0, 0.0,
        )));
        let bad = crate::GeoObject::Transformed(crate::TransformedObj {
            inner: Box::new(inner.clone()),
            complex_expr: "0".to_string(),
            compiled_expr: None,
        });
        let err = validate_object_candidate_typed(&doc, &bad).unwrap_err();
        assert!(
            matches!(err, crate::CoreError::TransformJacobianSingular { .. }),
            "esperaba jacobian singular, obtuvo {err:?}"
        );
        // depth 64 anidado debe fallar con TransformDepthExceeded
        let mut deep = inner;
        for _ in 0..64 {
            deep = crate::GeoObject::Transformed(crate::TransformedObj {
                inner: Box::new(deep),
                complex_expr: "z".to_string(),
                compiled_expr: None,
            });
        }
        // 64 niveles es el límite: depth 64 debe rechazar el siguiente
        let deepest = crate::GeoObject::Transformed(crate::TransformedObj {
            inner: Box::new(deep),
            complex_expr: "z".to_string(),
            compiled_expr: None,
        });
        let err = validate_object_candidate_typed(&doc, &deepest).unwrap_err();
        assert!(
            matches!(err, crate::CoreError::TransformDepthExceeded { .. }),
            "esperaba depth exceeded, obtuvo {err:?}"
        );
        // 63 niveles debe ser ok
        let mut ok_deep = crate::GeoObject::Point(crate::PointObj::new(
            grafito_geometry::Point2::new(0.0, 0.0),
        ));
        for _ in 0..63 {
            ok_deep = crate::GeoObject::Transformed(crate::TransformedObj {
                inner: Box::new(ok_deep),
                complex_expr: "z".to_string(),
                compiled_expr: None,
            });
        }
        assert!(validate_object_candidate_typed(&doc, &ok_deep).is_ok());
    }
}

#[cfg(test)]
mod tests_text_rotation {
    use super::*;

    #[test]
    fn rotacion_finita_pasa_y_no_finita_falla() {
        let doc = Document::new();
        let mut ok = crate::TextObj::new("hola", grafito_geometry::Point2::new(0.0, 0.0));
        ok.rotation = 1.0;
        assert!(validate_object_candidate_typed(&doc, &crate::GeoObject::Text(ok)).is_ok());
        let mut bad = crate::TextObj::new("hola", grafito_geometry::Point2::new(0.0, 0.0));
        bad.rotation = f32::NAN;
        assert!(validate_object_candidate_typed(&doc, &crate::GeoObject::Text(bad)).is_err());
    }
}

#[cfg(test)]
mod tests_budgets_ola4 {
    use super::*;

    /// Ola 4 P0: `MAX_EXPR_LENGTH` 2000 en la frontera exacta.
    ///
    /// Puro y determinista: 2000 pasa, 2001 falla con mensaje que cita el tope.
    #[test]
    fn expr_length_2000_passes_2001_fails() {
        assert_eq!(MAX_EXPR_LENGTH, 2_000);
        assert!(validate_expr(&"x".repeat(2_000)).is_ok());
        let err = validate_expr(&"x".repeat(2_001)).unwrap_err();
        assert!(
            err.contains("2000"),
            "el mensaje debe citar el tope, fue: {err}"
        );
    }

    /// Ola 4 P0: presupuesto 10 MiB con documento real.
    ///
    /// Determinista y sin sleeps: un `Document` real pequeño pasa la guarda
    /// (`parse_document_json` Ok sobre su JSON) y un payload de
    /// `MAX_DOCUMENT_SIZE_BYTES + 1` se rechaza con el mensaje de tamaño
    /// antes de deserializar (la misma comparación que aplica persistencia).
    #[test]
    fn document_size_budget_10mb_with_real_document() {
        assert_eq!(MAX_DOCUMENT_SIZE_BYTES, 10_000_000);
        let doc = Document::new();
        let json = validate_and_serialize(&doc).expect("documento vacío válido");
        assert!(
            json.len() < MAX_DOCUMENT_SIZE_BYTES,
            "documento real pequeño bajo el tope, len={}",
            json.len()
        );
        assert!(parse_document_json(&json).is_ok());
        let oversized = "x".repeat(MAX_DOCUMENT_SIZE_BYTES + 1);
        let err = parse_document_json(&oversized).unwrap_err();
        assert!(
            err.contains("exceeds maximum"),
            "la guarda de tamaño debe rechazar, fue: {err}"
        );
    }

    /// Ola 4: el libro de pizarra respeta hojas, elementos y bytes totales.
    ///
    /// Determinista: 33 hojas se rechazan; una hoja con trazo gigante que
    /// supera los 32 MiB totales también (sin esta guarda, el teórico era
    /// ~1 GiB clonado por snapshot de undo).
    #[test]
    fn whiteboard_book_caps_reject_oversized() {
        use crate::document::{
            WhiteboardPageData, MAX_WHITEBOARD_ELEMENTS_PER_PAGE, MAX_WHITEBOARD_PAGES,
            MAX_WHITEBOARD_TOTAL_BYTES,
        };
        use grafito_whiteboard::{WhiteboardDoc, WhiteboardElement};
        assert_eq!(MAX_WHITEBOARD_PAGES, 32);
        assert_eq!(MAX_WHITEBOARD_TOTAL_BYTES, 32 * 1024 * 1024);
        // Vacío pasa.
        assert!(validate_document(&Document::new()).is_ok());
        // 33 hojas se rechazan.
        let mut doc = Document::new();
        doc.whiteboard_pages = (0..MAX_WHITEBOARD_PAGES + 1)
            .map(|i| WhiteboardPageData {
                title: format!("H{i}"),
                doc: WhiteboardDoc::new(),
                pan: (0.0, 0.0),
                zoom: 1.0,
            })
            .collect();
        let err = validate_document(&doc).unwrap_err();
        assert!(err.contains("hojas"), "fue: {err}");
        // Trazo gigante: supera los 32 MiB totales.
        let mut doc = Document::new();
        let mut big = WhiteboardDoc::new();
        let puntos_por_trazo = crate::pencil::MAX_PENCIL_POINTS;
        let trazos = MAX_WHITEBOARD_TOTAL_BYTES / (puntos_por_trazo * 16) + 2;
        for _ in 0..trazos.min(MAX_WHITEBOARD_ELEMENTS_PER_PAGE + 1) {
            big.add(WhiteboardElement::Stroke {
                points: vec![(1.0, 2.0); puntos_por_trazo],
                color: (0, 0, 0),
                width: 1.0,
            });
        }
        doc.whiteboard_pages = vec![WhiteboardPageData {
            title: "grande".to_string(),
            doc: big,
            pan: (0.0, 0.0),
            zoom: 1.0,
        }];
        // Con 258 trazos de 128 KiB (< 501 elementos) cae por el tope
        // TOTAL de 32 MiB, no por elementos/hoja.
        let err = validate_document(&doc).unwrap_err();
        assert!(err.contains("MiB"), "debe citar el tope total, fue: {err}");
    }
}

#[cfg(test)]
mod tests_object_scripts {
    use super::*;
    use crate::document::ObjectScripts;
    use std::collections::BTreeMap;

    fn doc_con_on_load(script: &str) -> Document {
        let mut doc = Document::new();
        doc.on_load_script = Some(script.to_string());
        doc
    }

    /// FIX 1 red-first: un `OnLoad` hostil (fuera del allowlist) debe hacer
    /// fallar `validate_document`.
    #[test]
    fn on_load_hostil_falla_validate_document() {
        let doc = doc_con_on_load("Delete[A]");
        let err = validate_document(&doc).unwrap_err();
        assert!(
            err.contains("fuera del subset"),
            "debe citar el allowlist, fue: {err}"
        );
    }

    /// El wrapper fail-closed hereda el rechazo (corre `validate_document`).
    #[test]
    fn on_load_hostil_falla_validated_document() {
        let doc = doc_con_on_load("Script[Delete[A]]");
        assert!(ValidatedDocument::try_new(doc).is_err());
        let doc = doc_con_on_load("Delete[A]");
        assert!(ValidatedDocument::try_new_typed(doc).is_err());
    }

    /// `object_scripts` hostil también se rechaza (OnClick y OnUpdate).
    #[test]
    fn object_scripts_hostil_falla() {
        let mut doc = Document::new();
        doc.object_scripts.insert(
            "A".to_string(),
            ObjectScripts {
                on_click: Some("Delete[A]".to_string()),
                on_update: None,
            },
        );
        assert!(validate_document(&doc).is_err());
        let mut doc = Document::new();
        doc.object_scripts.insert(
            "A".to_string(),
            ObjectScripts {
                on_click: None,
                on_update: Some("RunCommand[Delete[A]]".to_string()),
            },
        );
        assert!(validate_document(&doc).is_err());
    }

    /// Guiones benignos del subset pasan (paridad con la vía al guardar).
    #[test]
    fn guiones_benignos_pasan() {
        let mut doc = Document::new();
        doc.object_scripts.insert(
            "A".to_string(),
            ObjectScripts {
                on_click: Some("SetValue[x, 1]; Show[A]".to_string()),
                on_update: Some("If[x > 0, \"Hide[B]\"]".to_string()),
            },
        );
        doc.on_load_script = Some("ZoomIn[2]".to_string());
        assert!(validate_document(&doc).is_ok());
    }

    /// Cotas: guion vacío, gigante, con NUL y exceso de entradas se rechazan.
    #[test]
    fn cotas_de_guiones() {
        assert!(validate_document(&doc_con_on_load("")).is_err());
        assert!(validate_document(&doc_con_on_load("   ")).is_err());
        let gigante = format!("Show[{}]", "A".repeat(MAX_SCRIPT_BYTES));
        assert!(validate_document(&doc_con_on_load(&gigante)).is_err());
        assert!(validate_document(&doc_con_on_load("Show[A\0]")).is_err());
        assert!(validate_document(&doc_con_on_load("Show[A")).is_err());
        let mut doc = Document::new();
        doc.object_scripts = (0..MAX_OBJECT_SCRIPTS + 1)
            .map(|i| {
                (
                    format!("L{i}"),
                    ObjectScripts {
                        on_click: Some("Show[A]".to_string()),
                        on_update: None,
                    },
                )
            })
            .collect::<BTreeMap<_, _>>();
        let err = validate_document(&doc).unwrap_err();
        assert!(err.contains("object scripts"), "fue: {err}");
    }

    /// El espejo local no diverge del subset benigno pinneado por test en
    /// `grafito-command` (`auto_script_allowlist_is_benign_and_pinned`).
    #[test]
    fn allowlist_local_pinneada() {
        for cmd in [
            "SetValue",
            "Show",
            "Hide",
            "ZoomIn",
            "ZoomOut",
            "PlayPause",
            "If",
            "Repeat",
            "Button",
            "Checkbox",
            "InputBox",
            "TextField",
            "DefineTool",
            "LoadTool",
        ] {
            assert!(
                SCRIPT_ALLOWLIST.contains(&cmd),
                "falta {cmd} en el espejo local"
            );
        }
        assert_eq!(SCRIPT_ALLOWLIST.len(), 14);
        // Destructivos jamás entran.
        for cmd in ["Delete", "Script", "RunCommand", "Rename", "Clear"] {
            assert!(
                !SCRIPT_ALLOWLIST.iter().any(|n| n.eq_ignore_ascii_case(cmd)),
                "{cmd} no debe estar en el allowlist"
            );
        }
    }
}

#[cfg(test)]
mod tests_polygon_lazos_autointersectados {
    //! Regresión del bug Lissajous: el shoelace CON SIGNO daba área algebraica
    //! neta 0 en toda curva auto-intersecada (las regiones se cancelan) y la
    //! validación la rechazaba como "degenerate or colinear" en cualquier
    //! parametrización. Estos lazos DEBEN validar; lo colineal DEBE seguir
    //! rechazándose.

    use super::*;
    use crate::object::PolygonObj;
    use grafito_geometry::Point2;

    fn valida(pts: Vec<Point2>) -> Result<(), String> {
        let doc = Document::new();
        validate_object_candidate(&doc, &GeoObject::Polygon(PolygonObj::new(pts)))
    }

    /// Muestrea una curva paramétrica cerrada en `n` pasos (paridad con
    /// `grafito_geometry::special_curves::lissajous`: `t = 2πi/n`).
    fn curva<F>(n: usize, f: F) -> Vec<Point2>
    where
        F: Fn(f64) -> (f64, f64),
    {
        (0..n)
            .map(|i| {
                let t = std::f64::consts::TAU * (i as f64) / (n as f64);
                let (x, y) = f(t);
                Point2::new(x, y)
            })
            .collect()
    }

    /// Lissajous(3,2) con δ=π/2 y 400 puntos: el caso exacto del comando
    /// `Lissajous[a, b, freq_x, freq_y, delta]` (`commands.rs:12407-12445`).
    #[test]
    fn lissajous_3_2_pasa_pese_a_auto_intersecarse() {
        let pts = curva(400, |t| {
            (
                (3.0 * t + std::f64::consts::FRAC_PI_2).sin(),
                (2.0 * t).sin(),
            )
        });
        assert!(
            valida(pts).is_ok(),
            "Lissajous(3,2) es una curva válida: su área neta con signo da 0, \
             pero NO está degenerada"
        );
    }

    /// Lazo 1: lemniscata de Bernoulli (dos lóbulos con cruce en el origen).
    #[test]
    fn lemniscata_de_bernoulli_pasa() {
        let pts = curva(400, |t| {
            let d = 1.0 + t.sin().powi(2);
            (t.cos() / d, t.sin() * t.cos() / d)
        });
        assert!(
            valida(pts).is_ok(),
            "lemniscata: lazo auto-intersecado válido"
        );
    }

    /// Lazo 2: rosa de 3 pétalos (`r = sen(3θ)`, se cruza en el origen).
    #[test]
    fn rosa_de_3_petalos_pasa() {
        let pts = curva(400, |t| {
            let r = (3.0 * t).sin();
            (r * t.cos(), r * t.sin())
        });
        assert!(valida(pts).is_ok(), "rosa de 3 pétalos: lazo válido");
    }

    /// Lazo 3: lazo de trébol (proyección del nudo trefoil, 3 lazos).
    #[test]
    fn lazo_de_trebol_pasa() {
        let pts = curva(400, |t| {
            (
                t.sin() + 2.0 * (2.0 * t).sin(),
                t.cos() - 2.0 * (2.0 * t).cos(),
            )
        });
        assert!(valida(pts).is_ok(), "lazo de trébol: lazo válido");
    }

    /// Lo genuinamente degenerado SIGUE rechazándose: puntos colineales.
    #[test]
    fn colineales_siguen_rechazandose() {
        let diagonal: Vec<Point2> = (0..8).map(|i| Point2::new(i as f64, i as f64)).collect();
        let err = valida(diagonal).expect_err("colineal es degenerado");
        assert!(
            err.contains("degenerate or colinear"),
            "debe citar degeneración, fue: {err}"
        );
        // Colineales en orden no monótono (zigzag sobre la recta y=x con
        // cruces de signo alternado): el área neta Y la absoluta dan 0.
        let zigzag: Vec<Point2> = [0.0, 3.0, 1.0, 4.0, 2.0, 5.0, 6.0]
            .iter()
            .map(|i| Point2::new(*i, *i))
            .collect();
        let err = valida(zigzag).expect_err("zigzag colineal con área 0 es degenerado");
        assert!(err.contains("degenerate or colinear"), "fue: {err}");
    }

    /// Un triángulo simple sigue pasando (sanity del chequeo).
    #[test]
    fn triangulo_simple_pasa() {
        let pts = vec![
            Point2::new(0.0, 0.0),
            Point2::new(1.0, 0.0),
            Point2::new(0.0, 1.0),
        ];
        assert!(valida(pts).is_ok());
    }
}
