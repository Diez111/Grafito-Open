use crate::constraints::ConstraintGraph;
use crate::numeric_constraints::{
    AngleEq, CoincidentEq, DistanceEq, EqualLengthEq, HorizontalEq, SymmetryEq, TangentEq,
    VerticalEq,
};
use crate::numeric_solver::{NumericSolver, SolveError, VarIndex};
use crate::{
    EllipseObj, GeoObject, HyperbolaObj, ImplicitCurveSegments, LineKind, ObjectId, ParabolaObj,
    PointObj, RelationOperator,
};
use grafito_geometry::expr::evaluate_cached;
use grafito_geometry::{
    distance_point_to_segment, matrices::solve_linear_system, matrices::Matrix, Color, Point2,
    Point3D, ViewTransform,
};
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet, VecDeque};
use std::hash::{DefaultHasher, Hash, Hasher};

/// Recorrido de un parámetro animado dentro de su intervalo permitido.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
pub enum AnimationMode {
    /// Rebota en ambos extremos, conservando el comportamiento histórico de sliders.
    #[default]
    PingPong,
    /// Vuelve continuamente desde el máximo al mínimo, apropiado para fases angulares.
    Loop,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct VariableMeta {
    pub position: Point2,
    pub min: f64,
    pub max: f64,
    pub step: f64,
    pub visible: bool,
    #[serde(default)]
    pub animating: bool,
    #[serde(default = "default_animation_speed")]
    pub animation_speed: f64,
    #[serde(default)]
    pub animation_mode: AnimationMode,
}

fn default_animation_speed() -> f64 {
    1.0
}

fn to_subscript(n: usize) -> String {
    let s = n.to_string();
    s.chars()
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

const MAX_AUTO_LABEL_NUMBER: usize = crate::validation::MAX_OBJECT_COUNT + 1;

fn canonical_label_counter(counter: usize) -> usize {
    if (1..=MAX_AUTO_LABEL_NUMBER).contains(&counter) {
        counter
    } else {
        1
    }
}

fn deserialize_next_label_numbers<'de, D>(
    deserializer: D,
) -> Result<HashMap<String, usize>, D::Error>
where
    D: serde::Deserializer<'de>,
{
    let counters = HashMap::<String, usize>::deserialize(deserializer)?;
    Ok(counters
        .into_iter()
        .map(|(base, counter)| (base, canonical_label_counter(counter)))
        .collect())
}

/// Identifies a scalar geometric property that can participate in numeric
/// constraint solving.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ObjField {
    PointX,
    PointY,
    CircleRadius,
    LineStartX,
    LineStartY,
    LineEndX,
    LineEndY,
}

/// Type alias for the cached variables list used in expression resolution.
pub type CachedVarsList =
    std::sync::Arc<std::sync::Mutex<Option<(u64, std::sync::Arc<Vec<(String, f64)>>)>>>;

/// A fallible mutation that can be staged as part of a document revision.
pub type DocumentOperation = Box<dyn FnOnce(&mut Document) -> Result<(), String> + Send>;

/// A group of document mutations that either commits as one revision or leaves
/// the document untouched.
#[derive(Default)]
pub struct OperationBatch {
    operations: Vec<DocumentOperation>,
}

impl OperationBatch {
    /// Creates an empty batch.
    pub fn new() -> Self {
        Self::default()
    }

    /// Adds a mutation to be validated before the batch commits.
    pub fn push<F>(&mut self, operation: F)
    where
        F: FnOnce(&mut Document) -> Result<(), String> + Send + 'static,
    {
        self.operations.push(Box::new(operation));
    }

    /// Returns the number of staged mutations.
    pub fn len(&self) -> usize {
        self.operations.len()
    }

    /// Returns whether the batch contains no mutations.
    pub fn is_empty(&self) -> bool {
        self.operations.is_empty()
    }
}

/// Semantically valid document states before and after one committed batch.
#[derive(Debug, Clone)]
pub struct ChangeSet {
    /// State before the operation batch committed.
    pub before: Document,
    /// State after the operation batch committed.
    pub after: Document,
}

impl ChangeSet {
    fn same_semantic_state(left: &Document, right: &Document) -> Result<bool, String> {
        let left = serde_json::to_value(left).map_err(|error| error.to_string())?;
        let right = serde_json::to_value(right).map_err(|error| error.to_string())?;
        Ok(left == right)
    }

    fn restore(
        document: &mut Document,
        expected_current: &Document,
        snapshot: &Document,
    ) -> Result<(), String> {
        if !Self::same_semantic_state(document, expected_current)? {
            return Err("ChangeSet cannot be applied after unrelated document changes".into());
        }
        crate::validation::validate_document(snapshot)?;
        if Self::same_semantic_state(document, snapshot)? {
            return Ok(());
        }
        let next_version = document.version.wrapping_add(1);
        let mut restored = snapshot.clone();
        restored.version = next_version;
        restored.spatial_dirty = true;
        *document = restored;
        Ok(())
    }

    /// Restores the state before the batch as a new document revision.
    pub fn undo(&self, document: &mut Document) -> Result<(), String> {
        Self::restore(document, &self.after, &self.before)
    }

    /// Restores the state after the batch as a new document revision.
    pub fn redo(&self, document: &mut Document) -> Result<(), String> {
        Self::restore(document, &self.before, &self.after)
    }
}

/// Estado histórico de una celda ejecutada en la hoja CAS local.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum CasWorksheetStatus {
    /// El comando se evaluó sin errores.
    Success,
    /// El comando fue rechazado y la celda conserva el diagnóstico local.
    Error,
}

/// Resultado inmutable de una entrada enviada desde la hoja CAS.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CasWorksheetEntry {
    /// Entrada textual enviada al intérprete.
    pub input: String,
    /// Resultado o diagnóstico producido al enviar la entrada.
    pub output: String,
    /// Si el resultado representa éxito o error.
    pub status: CasWorksheetStatus,
}

/// The main document containing all geometric objects.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Document {
    objects: HashMap<ObjectId, GeoObject>,
    view: ViewTransform,
    #[serde(skip)]
    selection: Vec<ObjectId>,
    #[serde(default, deserialize_with = "deserialize_next_label_numbers")]
    next_label_number: HashMap<String, usize>,
    pub variables: HashMap<String, f64>,
    #[serde(default)]
    variable_meta: HashMap<String, VariableMeta>,
    pub spreadsheet: Vec<Vec<String>>,
    /// Celdas CAS locales enviadas explícitamente; no hay borradores en este
    /// modelo para que save/open no puedan perder texto parcialmente editado.
    #[serde(default)]
    cas_worksheet: Vec<CasWorksheetEntry>,
    #[serde(default)]
    spreadsheet_variables: HashSet<String>,
    #[serde(default)]
    spreadsheet_coordinate_points: HashMap<String, ObjectId>,
    #[serde(skip)]
    pub spatial: crate::spatial::SpatialIndex,
    #[serde(skip)]
    pub spatial_dirty: bool,
    #[serde(skip)]
    spatial_variables_hash: u64,
    pub complex_base_symbol: String,
    #[serde(default)]
    pub constraints: ConstraintGraph,
    #[serde(skip)]
    pub render_quality: crate::RenderQuality,
    #[serde(skip)]
    pub last_solution: HashMap<(ObjectId, ObjField), f64>,
    #[serde(skip)]
    pub version: u64,
    #[serde(skip)]
    pub cached_vars_list: CachedVarsList,
}

impl Default for Document {
    fn default() -> Self {
        Self {
            objects: HashMap::new(),
            view: ViewTransform::default(),
            selection: Vec::new(),
            next_label_number: HashMap::new(),
            variables: HashMap::new(),
            variable_meta: HashMap::new(),
            spreadsheet: Vec::new(),
            cas_worksheet: Vec::new(),
            spreadsheet_variables: HashSet::new(),
            spreadsheet_coordinate_points: HashMap::new(),
            spatial: crate::spatial::SpatialIndex::new(),
            spatial_dirty: true,
            spatial_variables_hash: 0,
            complex_base_symbol: "z".to_string(),
            constraints: ConstraintGraph::new(),
            render_quality: crate::RenderQuality::default(),
            last_solution: HashMap::new(),
            version: 0,
            cached_vars_list: std::sync::Arc::new(std::sync::Mutex::new(None)),
        }
    }
}

impl Document {
    fn auto_label(&mut self, base_name: &str, id: ObjectId) -> Result<String, String> {
        let used_labels: HashSet<String> = self
            .objects
            .values()
            .map(|object| object.label().to_owned())
            .collect();
        let start = self
            .next_label_number
            .get(base_name)
            .copied()
            .map(canonical_label_counter)
            .unwrap_or(1);

        for number in (start..=MAX_AUTO_LABEL_NUMBER).chain(1..start) {
            let label = if number == 1 {
                base_name.to_string()
            } else {
                format!("{}{}", base_name, to_subscript(number - 1))
            };
            if used_labels.contains(&label) {
                continue;
            }
            if label.len() > crate::validation::MAX_STRING_LENGTH {
                return Err("Generated object label exceeds maximum length".to_string());
            }

            let next = number
                .checked_add(1)
                .filter(|next| *next <= MAX_AUTO_LABEL_NUMBER)
                .unwrap_or(1);
            self.next_label_number.insert(base_name.to_string(), next);
            return Ok(label);
        }

        // A valid document contains at most MAX_OBJECT_COUNT objects, so this
        // fallback is only reachable for malformed in-memory documents.
        let label = format!("{base_name}_{}", id.0);
        if label.len() > crate::validation::MAX_STRING_LENGTH {
            Err("Generated object label exceeds maximum length".to_string())
        } else {
            Ok(label)
        }
    }

    pub(crate) fn validate_label_counters(&self) -> Result<(), String> {
        for (base, counter) in &self.next_label_number {
            if base.len() > crate::validation::MAX_STRING_LENGTH {
                return Err("Automatic label base exceeds maximum length".to_string());
            }
            if *counter != canonical_label_counter(*counter) {
                return Err("Automatic label counter is outside its valid range".to_string());
            }
        }
        Ok(())
    }

    /// Clones semantic state for a transaction without sharing mutable runtime
    /// caches with the live document. This keeps rejected staged work from
    /// invalidating render caches or expression-variable caches in place.
    pub fn detached_clone_for_staging(&self) -> Self {
        let mut staged = self.clone();
        staged.spatial = crate::spatial::SpatialIndex::new();
        staged.spatial_dirty = true;
        staged.spatial_variables_hash = 0;
        staged.cached_vars_list = Default::default();
        for object in staged.objects.values_mut() {
            object.detach_runtime_caches();
        }
        staged
    }

    /// Stages and validates several mutations before committing one revision.
    pub fn commit(&mut self, batch: OperationBatch) -> Result<ChangeSet, String> {
        let before = self.clone();
        let mut staged = self.detached_clone_for_staging();
        for operation in batch.operations {
            operation(&mut staged)?;
        }
        crate::validation::validate_document(&staged)?;
        if ChangeSet::same_semantic_state(&before, &staged)? {
            return Ok(ChangeSet {
                before: before.clone(),
                after: before,
            });
        }
        staged.version = before.version.wrapping_add(1);
        staged.spatial_dirty = true;
        let after = staged.clone();
        *self = staged;
        Ok(ChangeSet { before, after })
    }

    pub fn bump_version(&mut self) {
        self.version = self.version.wrapping_add(1);
    }
    pub fn new() -> Self {
        Self::default()
    }
    pub fn invalidate_all_caches(&self) {
        for obj in self.objects.values() {
            obj.invalidate_cache();
        }
    }

    pub fn migrate_complex_symbol(&mut self, new_symbol: &str) {
        let old = self.complex_base_symbol.clone();
        if old == new_symbol {
            return;
        }

        self.complex_base_symbol = new_symbol.to_string();
        self.bump_version();

        // Helper: reemplaza `old` por `new_symbol` sólo cuando aparezca como
        // word (boundary chars no alfanum/underscore/subscripts). Trabaja en
        // `char` para no romper con Unicode (subíndices son multibyte).
        let rewrite_expr = |expr: &str| -> String {
            if expr.is_empty() || old.is_empty() {
                return expr.to_string();
            }
            let chars: Vec<char> = expr.chars().collect();
            let old_chars: Vec<char> = old.chars().collect();
            let n = chars.len();
            let m = old_chars.len();
            let mut out = String::with_capacity(expr.len());
            let mut i = 0;
            while i < n {
                if i + m <= n && chars[i..i + m] == old_chars[..] {
                    let prev = if i == 0 { ' ' } else { chars[i - 1] };
                    let next = if i + m >= n { ' ' } else { chars[i + m] };
                    let is_boundary = |c: char| {
                        !c.is_alphanumeric()
                            && c != '_'
                            && !matches!(
                                c,
                                '₀' | '₁' | '₂' | '₃' | '₄' | '₅' | '₆' | '₇' | '₈' | '₉'
                            )
                    };
                    if is_boundary(prev) && is_boundary(next) {
                        out.push_str(new_symbol);
                        i += m;
                        continue;
                    }
                }
                out.push(chars[i]);
                i += 1;
            }
            out
        };

        let mut label_updates: Vec<(ObjectId, String)> = Vec::new();
        let mut expr_updates: Vec<(ObjectId, String)> = Vec::new();
        for (id, obj) in &self.objects {
            // --- Label rename ---
            let label = obj.label().to_string();
            if label.starts_with(&old) {
                let rest = &label[old.len()..];
                let is_subscript = rest.is_empty()
                    || rest.chars().all(|c| {
                        matches!(c, '₀' | '₁' | '₂' | '₃' | '₄' | '₅' | '₆' | '₇' | '₈' | '₉')
                    });
                if is_subscript {
                    label_updates.push((*id, format!("{}{}", new_symbol, rest)));
                }
            }
            // --- Expr token rewrite ---
            let new_expr_opt = match obj {
                GeoObject::ComplexGrid(o) => {
                    let r = rewrite_expr(&o.expr);
                    if r != o.expr {
                        Some(r)
                    } else {
                        None
                    }
                }
                GeoObject::ComplexMapping(o) => {
                    let r = rewrite_expr(&o.expr);
                    if r != o.expr {
                        Some(r)
                    } else {
                        None
                    }
                }
                _ => None,
            };
            if let Some(new_expr) = new_expr_opt {
                expr_updates.push((*id, new_expr));
            }
        }

        for (id, new_label) in label_updates {
            if let Some(obj) = self.objects.get_mut(&id) {
                match obj {
                    GeoObject::ComplexGrid(o) => o.label = new_label,
                    GeoObject::ComplexMapping(o) => o.label = new_label,
                    _ => {}
                }
            }
        }
        for (id, new_expr) in expr_updates {
            if let Some(obj) = self.objects.get_mut(&id) {
                match obj {
                    GeoObject::ComplexGrid(o) => o.expr = new_expr,
                    GeoObject::ComplexMapping(o) => o.expr = new_expr,
                    _ => {}
                }
            }
        }
    }

    /// Inserta un punto libre sólo si su posición y estilo son válidos.
    pub fn try_add_point(&mut self, pos: Point2) -> Result<ObjectId, String> {
        self.try_add_object(GeoObject::Point(PointObj::new(pos)))
    }

    /// Crea un lugar geométrico persistente que registra la posición final del
    /// punto objetivo cada vez que el grafo alcanza un estado válido.
    pub fn try_add_locus(
        &mut self,
        driver: ObjectId,
        target: ObjectId,
    ) -> Result<(ObjectId, usize), String> {
        if driver == target {
            return Err("Locus: los puntos de entrada deben ser distintos".to_string());
        }
        if !matches!(self.get_object(driver), Some(GeoObject::Point(_)))
            || !matches!(self.get_object(target), Some(GeoObject::Point(_)))
        {
            return Err("Locus: las entradas deben ser puntos".to_string());
        }
        let target_position = match self.get_object(target) {
            Some(GeoObject::Point(point)) => point.position,
            _ => return Err("Locus: no se encontró el punto objetivo".to_string()),
        };
        if !target_position.x.is_finite() || !target_position.y.is_finite() {
            return Err("Locus: el punto objetivo debe tener coordenadas finitas".to_string());
        }

        let locus = crate::PencilObj::new(vec![target_position]).with_locus_binding(driver, target);
        self.try_add_constructed_object(GeoObject::Pencil(locus), "Locus", &[driver, target])
    }

    /// Variante de compatibilidad que falla de forma visible ante un punto inválido.
    ///
    /// # Panics
    ///
    /// Panics when [`Self::try_add_point`] rejects the point.
    #[track_caller]
    pub fn add_point(&mut self, pos: Point2) -> ObjectId {
        self.try_add_point(pos)
            .unwrap_or_else(|error| panic!("Document::add_point rejected point: {error}"))
    }

    /// Inserts one free object while enforcing document-wide hard limits and
    /// unambiguous labels. Rejection leaves the document untouched.
    pub fn try_add_object(&mut self, obj: GeoObject) -> Result<ObjectId, String> {
        let id = obj.id();
        if self.objects.len() >= crate::validation::MAX_OBJECT_COUNT {
            return Err(format!(
                "Document reached the maximum of {} objects",
                crate::validation::MAX_OBJECT_COUNT
            ));
        }
        if self.objects.contains_key(&id) {
            return Err(format!("Object {id} already exists"));
        }
        crate::validation::validate_object_candidate(self, &obj)?;
        if !obj.label().is_empty()
            && self
                .objects
                .values()
                .any(|existing| existing.label() == obj.label())
        {
            return Err(format!("Object label '{}' is already in use", obj.label()));
        }

        // Auto-label if empty
        let obj = if obj.label().is_empty() {
            let mut obj = obj;
            let name = obj.name();
            let base_name = match &obj {
                GeoObject::ComplexGrid(_) | GeoObject::ComplexMapping(_) => {
                    self.complex_base_symbol.clone()
                }
                _ => name
                    .chars()
                    .next()
                    .map(|c| c.to_string())
                    .unwrap_or_else(|| "?".to_string()),
            };
            let label = self.auto_label(&base_name, id)?;
            obj.set_label(label);
            crate::validation::validate_object_candidate(self, &obj)?;
            obj
        } else {
            obj
        };
        self.objects.insert(id, obj);
        self.constraints.add_free_object(id);
        self.spatial_dirty = true;
        self.bump_version();
        Ok(id)
    }

    /// Reemplaza un objeto existente sólo cuando el documento completo sigue
    /// siendo válido. El candidato se aplica sobre una copia aislada para que
    /// un rechazo no altere el documento vivo ni sus cachés de ejecución.
    pub fn try_replace_object(
        &mut self,
        id: ObjectId,
        candidate: GeoObject,
    ) -> Result<bool, String> {
        Ok(self
            .try_replace_object_with_previous(id, candidate)?
            .is_some())
    }

    /// Reemplaza un objeto validado y devuelve el documento anterior sólo tras
    /// un commit exitoso. Los editores usan ese estado para el historial sin
    /// clonar el documento ni comparar JSON en rechazos o no-ops.
    pub fn try_replace_object_with_previous(
        &mut self,
        id: ObjectId,
        mut candidate: GeoObject,
    ) -> Result<Option<Self>, String> {
        if candidate.id() != id {
            return Err(format!(
                "Replacement candidate id {} does not match target id {}",
                candidate.id(),
                id
            ));
        }
        let Some(existing) = self.objects.get(&id) else {
            return Ok(None);
        };
        if existing == &candidate {
            return Ok(None);
        }
        if !candidate.label().is_empty()
            && candidate.label() != existing.label()
            && self.objects.iter().any(|(existing_id, object)| {
                *existing_id != id && object.label() == candidate.label()
            })
        {
            return Err(format!(
                "Object label '{}' is already in use",
                candidate.label()
            ));
        }

        let mut staged = self.detached_clone_for_staging();
        let Some(slot) = staged.objects.get_mut(&id) else {
            return Ok(None);
        };
        candidate.detach_runtime_caches();
        *slot = candidate;
        crate::validation::validate_document(&staged)?;
        staged.version = self.version.wrapping_add(1);
        staged.spatial_dirty = true;
        Ok(Some(std::mem::replace(self, staged)))
    }

    /// Compatibility wrapper for callers that cannot return insertion errors.
    ///
    /// # Deprecated
    ///
    /// New code must use [`Self::try_add_object`] so rejection remains a normal,
    /// diagnosable control-flow path.
    ///
    /// # Panics
    ///
    /// Panics when insertion is rejected. Use [`Self::try_add_object`] to
    /// receive the diagnostic and preserve normal control flow.
    #[track_caller]
    pub fn add_object(&mut self, obj: GeoObject) -> ObjectId {
        self.try_add_object(obj)
            .unwrap_or_else(|error| panic!("Document::add_object rejected object: {error}"))
    }

    pub fn add_constructed_object(
        &mut self,
        obj: GeoObject,
        constraint_name: &str,
        inputs: &[ObjectId],
    ) -> (ObjectId, usize) {
        self.add_constructed_object_with_params(obj, constraint_name, inputs, HashMap::new())
    }

    /// Add a constructed object only when its constraint can be registered.
    pub fn try_add_constructed_object(
        &mut self,
        obj: GeoObject,
        constraint_name: &str,
        inputs: &[ObjectId],
    ) -> Result<(ObjectId, usize), String> {
        self.try_add_constructed_object_with_params(obj, constraint_name, inputs, HashMap::new())
    }

    /// Legacy construction API. On rejection it logs the error and returns an
    /// unregistered object ID with `usize::MAX` as the constraint ID.
    pub fn add_constructed_object_with_params(
        &mut self,
        obj: GeoObject,
        constraint_name: &str,
        inputs: &[ObjectId],
        params: HashMap<String, f64>,
    ) -> (ObjectId, usize) {
        self.try_add_constructed_object_with_params(obj, constraint_name, inputs, params)
            .unwrap_or_else(|error| {
                log::warn!("{error}");
                (ObjectId::new(), usize::MAX)
            })
    }

    /// Add a constructed object with constraint parameters only when the graph
    /// has capacity and a representable next identifier.
    pub fn try_add_constructed_object_with_params(
        &mut self,
        obj: GeoObject,
        constraint_name: &str,
        inputs: &[ObjectId],
        params: HashMap<String, f64>,
    ) -> Result<(ObjectId, usize), String> {
        let id = obj.id();
        self.validate_constructive_constraint_parts(constraint_name, inputs, &obj, &params)?;
        self.constraints
            .validate_new_constraint(constraint_name, inputs, &[id], &params)?;

        let mut staged = self.detached_clone_for_staging();
        let id = staged.try_add_object(obj)?;
        let constraint_id = staged.constraints.try_add_constraint(
            constraint_name,
            inputs.to_vec(),
            vec![id],
            params,
        )?;
        staged.apply_constructive_constraints(&[constraint_id])?;
        crate::validation::validate_document(&staged)?;
        staged.version = self.version.wrapping_add(1);
        staged.spatial_dirty = true;
        *self = staged;
        Ok((id, constraint_id))
    }

    pub fn remove_object(&mut self, id: ObjectId) -> Option<GeoObject> {
        self.bump_version();
        let referencing_objects: Vec<ObjectId> = self
            .objects
            .iter()
            .filter_map(|(candidate_id, object)| {
                object
                    .referenced_object_ids()
                    .contains(&id)
                    .then_some(*candidate_id)
            })
            .collect();
        // Eliminar del grafo de restricciones y recolectar los outputs que
        // quedaron huérfanos (constraints eliminadas que producían esos
        // objetos). Esos outputs ya no son impulsados por ninguna
        // restricción, así que también deben eliminarse del documento de
        // forma recursiva: un output huérfano podría ser a su vez input de
        // otra restricción, lo que dispara una nueva cascada.
        let orphaned = self.constraints.remove_object(id);
        self.spatial_dirty = true;
        self.selection.retain(|&s| s != id);
        self.spreadsheet_coordinate_points
            .retain(|_, point_id| *point_id != id);
        let removed = self.objects.remove(&id);
        for out in orphaned {
            // Evitar doble eliminación si el output era el propio `id`.
            if out != id {
                let _ = self.remove_object(out);
            }
        }
        for dependent in referencing_objects {
            if dependent != id {
                let _ = self.remove_object(dependent);
            }
        }
        removed
    }

    /// Compatibility wrapper that moves a free point and propagates every
    /// dependency atomically. Failure is logged and reported as no affected IDs.
    pub fn move_point(&mut self, id: ObjectId, new_pos: Point2) -> Vec<ObjectId> {
        if !self.constraints.is_free(&id)
            || !matches!(self.objects.get(&id), Some(GeoObject::Point(point)) if point.position != new_pos)
        {
            return vec![];
        }
        let mut affected = vec![id];
        let constraint_order = self.constraints.get_update_order(&[id]);
        for cons_id in constraint_order {
            if let Some(cons) = self.constraints.get_constraint(cons_id) {
                affected.extend(cons.outputs.iter().cloned());
            }
        }
        match self.try_move_point_and_re_evaluate(id, new_pos) {
            Ok(true) => affected,
            Ok(false) => Vec::new(),
            Err(error) => {
                log::warn!("{error}");
                Vec::new()
            }
        }
    }

    /// Moves a free point and reevaluates every affected constraint as one
    /// transaction. A failed numeric solve leaves the live source and all
    /// derived geometry exactly as they were before the drag update.
    pub fn try_move_point_and_re_evaluate(
        &mut self,
        id: ObjectId,
        new_pos: Point2,
    ) -> Result<bool, String> {
        if !new_pos.x.is_finite() || !new_pos.y.is_finite() {
            return Err("Point position must be finite".to_string());
        }
        self.try_update_point_and_re_evaluate(id, |point| {
            point.position = new_pos;
            Ok(())
        })
    }

    /// Applies a free-point mutation and all dependent constraints on a
    /// detached document, committing only a fully valid final state.
    pub fn try_update_point_and_re_evaluate<F>(
        &mut self,
        id: ObjectId,
        update: F,
    ) -> Result<bool, String>
    where
        F: FnOnce(&mut PointObj) -> Result<(), String>,
    {
        if self
            .spreadsheet_coordinate_points
            .values()
            .any(|point_id| *point_id == id)
        {
            return Err("spreadsheet coordinate points must be edited in their cell".to_string());
        }
        if !self.constraints.is_free(&id)
            || !matches!(self.objects.get(&id), Some(GeoObject::Point(_)))
        {
            return Ok(false);
        }

        let mut staged = self.detached_clone_for_staging();
        let Some(GeoObject::Point(point)) = staged.objects.get_mut(&id) else {
            return Ok(false);
        };
        update(point)?;

        if ChangeSet::same_semantic_state(self, &staged)? {
            return Ok(false);
        }

        let order = staged.propagation_order(&[id]);
        staged.re_evaluate_constraints_in_place(&order)?;
        crate::validation::validate_document(&staged)?;
        staged.version = self.version.wrapping_add(1);
        staged.spatial_dirty = true;
        *self = staged;
        Ok(true)
    }

    /// Move a free 3D point and return IDs of all affected objects.
    pub fn move_point3d(
        &mut self,
        id: ObjectId,
        new_pos: grafito_geometry::Point3D,
    ) -> Vec<ObjectId> {
        if !self.constraints.is_free(&id) {
            return vec![];
        }
        let mut affected = vec![id];
        if let Some(GeoObject::Point3D(p)) = self.get_object_mut(id) {
            p.position = new_pos;
        }
        let constraint_order = self.constraints.get_update_order(&[id]);
        for cons_id in constraint_order {
            if let Some(cons) = self.constraints.get_constraint(cons_id) {
                affected.extend(cons.outputs.iter().cloned());
            }
        }
        affected
    }

    /// Get the update order for re-evaluating dependent objects when these IDs change.
    pub fn propagation_order(&self, changed: &[ObjectId]) -> Vec<usize> {
        self.constraints.get_update_order(changed)
    }

    pub fn is_free_object(&self, id: &ObjectId) -> bool {
        self.constraints.is_free(id)
    }

    pub fn creator_of(&self, id: &ObjectId) -> Option<&crate::constraints::Constraint> {
        self.constraints.creator_of(id)
    }

    /// Collect all free numeric variables that participate in any numeric
    /// constraint (`Distance`, `Angle`, or `Tangent`).
    pub fn build_solver_variables(&self) -> Vec<(ObjectId, ObjField)> {
        let numeric_ids: Vec<usize> = self
            .constraints
            .iter()
            .filter(|c| Self::is_numeric_constraint_name(&c.name))
            .map(|c| c.id)
            .collect();
        self.build_solver_variables_for_constraints(&numeric_ids)
    }

    fn build_solver_variables_for_constraints(
        &self,
        numeric_ids: &[usize],
    ) -> Vec<(ObjectId, ObjField)> {
        let mut seen = HashSet::new();
        let mut vars = Vec::new();

        for &id in numeric_ids {
            let Some(cons) = self.constraints.get_constraint(id) else {
                continue;
            };
            for &input in &cons.inputs {
                if !self.constraints.is_free(&input) {
                    continue;
                }
                if let Some(obj) = self.get_object(input) {
                    match obj {
                        GeoObject::Point(_) => {
                            if seen.insert((input, ObjField::PointX)) {
                                vars.push((input, ObjField::PointX));
                            }
                            if seen.insert((input, ObjField::PointY)) {
                                vars.push((input, ObjField::PointY));
                            }
                        }
                        GeoObject::Circle(_) if seen.insert((input, ObjField::CircleRadius)) => {
                            vars.push((input, ObjField::CircleRadius));
                        }
                        GeoObject::Line(_) => {
                            if seen.insert((input, ObjField::LineStartX)) {
                                vars.push((input, ObjField::LineStartX));
                            }
                            if seen.insert((input, ObjField::LineStartY)) {
                                vars.push((input, ObjField::LineStartY));
                            }
                            if seen.insert((input, ObjField::LineEndX)) {
                                vars.push((input, ObjField::LineEndX));
                            }
                            if seen.insert((input, ObjField::LineEndY)) {
                                vars.push((input, ObjField::LineEndY));
                            }
                        }
                        _ => {}
                    }
                }
            }
        }

        vars
    }

    fn get_field_value(&self, id: ObjectId, field: ObjField) -> f64 {
        match (self.get_object(id), field) {
            (Some(GeoObject::Point(p)), ObjField::PointX) => p.position.x,
            (Some(GeoObject::Point(p)), ObjField::PointY) => p.position.y,
            (Some(GeoObject::Circle(c)), ObjField::CircleRadius) => c.radius,
            (Some(GeoObject::Line(l)), ObjField::LineStartX) => l.start.x,
            (Some(GeoObject::Line(l)), ObjField::LineStartY) => l.start.y,
            (Some(GeoObject::Line(l)), ObjField::LineEndX) => l.end.x,
            (Some(GeoObject::Line(l)), ObjField::LineEndY) => l.end.y,
            _ => 0.0,
        }
    }

    fn set_field_value(&mut self, id: ObjectId, field: ObjField, value: f64) {
        match (self.get_object_mut(id), field) {
            (Some(GeoObject::Point(p)), ObjField::PointX) => p.position.x = value,
            (Some(GeoObject::Point(p)), ObjField::PointY) => p.position.y = value,
            (Some(GeoObject::Circle(c)), ObjField::CircleRadius) => c.radius = value,
            (Some(GeoObject::Line(l)), ObjField::LineStartX) => l.start.x = value,
            (Some(GeoObject::Line(l)), ObjField::LineStartY) => l.start.y = value,
            (Some(GeoObject::Line(l)), ObjField::LineEndX) => l.end.x = value,
            (Some(GeoObject::Line(l)), ObjField::LineEndY) => l.end.y = value,
            _ => {}
        }
    }

    pub fn point_position(&self, id: ObjectId) -> Option<Point2> {
        match self.get_object(id)? {
            GeoObject::Point(p) => Some(p.position),
            _ => None,
        }
    }

    fn build_numeric_equations(
        &self,
        numeric_ids: &[usize],
        var_index: &HashMap<(ObjectId, ObjField), VarIndex>,
    ) -> Result<Vec<Box<dyn crate::numeric_solver::ConstraintEquation>>, String> {
        let mut equations: Vec<Box<dyn crate::numeric_solver::ConstraintEquation>> = Vec::new();
        for &id in numeric_ids {
            let cons = self
                .constraints
                .get_constraint(id)
                .ok_or_else(|| format!("Numeric constraint {id} is missing"))?;
            self.validate_numeric_constraint_definition(&cons.name, &cons.inputs, &cons.params)?;
            match cons.name.as_str() {
                "Distance" => {
                    let target = cons.params.get("distance").copied().unwrap_or(0.0);
                    let eq = DistanceEq::from_inputs(
                        self,
                        cons.inputs[0],
                        cons.inputs[1],
                        target,
                        var_index,
                    )
                    .ok_or_else(|| "Distance: no se pudo construir la ecuación".to_string())?;
                    equations.push(Box::new(eq));
                }
                "Angle" => {
                    let target = cons.params.get("angle").copied().unwrap_or(0.0);
                    let eq = AngleEq::from_inputs(self, &cons.inputs, target, var_index)
                        .ok_or_else(|| "Angle: no se pudo construir la ecuación".to_string())?;
                    equations.push(Box::new(eq));
                }
                "Tangent" => {
                    let eq =
                        TangentEq::from_inputs(self, cons.inputs[0], cons.inputs[1], var_index)
                            .ok_or_else(|| {
                                "Tangent: no se pudo construir la ecuación".to_string()
                            })?;
                    equations.push(Box::new(eq));
                }
                "Coincident" => {
                    let eq =
                        CoincidentEq::from_inputs(self, cons.inputs[0], cons.inputs[1], var_index)
                            .ok_or_else(|| {
                                "Coincident: no se pudo construir la ecuación".to_string()
                            })?;
                    equations.push(Box::new(eq));
                }
                "Horizontal" => {
                    let eq = HorizontalEq::from_inputs(self, cons.inputs[0], var_index)
                        .ok_or_else(|| {
                            "Horizontal: no se pudo construir la ecuación".to_string()
                        })?;
                    equations.push(Box::new(eq));
                }
                "Vertical" => {
                    let eq = VerticalEq::from_inputs(self, cons.inputs[0], var_index)
                        .ok_or_else(|| "Vertical: no se pudo construir la ecuación".to_string())?;
                    equations.push(Box::new(eq));
                }
                "EqualLength" => {
                    let eq =
                        EqualLengthEq::from_inputs(self, cons.inputs[0], cons.inputs[1], var_index)
                            .ok_or_else(|| {
                                "EqualLength: no se pudo construir la ecuación".to_string()
                            })?;
                    equations.push(Box::new(eq));
                }
                "Symmetry" => {
                    let eq = SymmetryEq::from_inputs(
                        self,
                        cons.inputs[0],
                        cons.inputs[1],
                        cons.inputs[2],
                        var_index,
                    )
                    .ok_or_else(|| "Symmetry: no se pudo construir la ecuación".to_string())?;
                    equations.push(Box::new(eq));
                }
                _ => {}
            }
        }
        Ok(equations)
    }

    fn write_solver_variables(
        &mut self,
        var_map: &[(ObjectId, ObjField)],
        vars: &[f64],
    ) -> Vec<ObjectId> {
        let mut changed = Vec::new();
        for ((id, field), value) in var_map.iter().zip(vars.iter()) {
            let old = self.get_field_value(*id, *field);
            if (old - *value).abs() > 1e-12 {
                self.set_field_value(*id, *field, *value);
                if changed.last() != Some(id) {
                    changed.push(*id);
                }
            }
        }
        changed
    }

    pub(crate) fn is_numeric_constraint_name(name: &str) -> bool {
        matches!(
            name,
            "Distance"
                | "Angle"
                | "Tangent"
                | "Coincident"
                | "Horizontal"
                | "Vertical"
                | "EqualLength"
                | "Symmetry"
        )
    }

    pub(crate) fn validate_numeric_constraint_definition(
        &self,
        name: &str,
        inputs: &[ObjectId],
        params: &HashMap<String, f64>,
    ) -> Result<(), String> {
        let object_is = |id: ObjectId, predicate: fn(&GeoObject) -> bool| {
            self.get_object(id).is_some_and(predicate)
        };
        let finite_param = |key: &str| {
            params
                .get(key)
                .copied()
                .filter(|value| value.is_finite())
                .ok_or_else(|| format!("{name}: el parámetro '{key}' debe ser finito"))
        };

        match name {
            "Distance" => {
                if inputs.len() != 2
                    || !inputs
                        .iter()
                        .all(|id| object_is(*id, |object| matches!(object, GeoObject::Point(_))))
                {
                    return Err("Distance: requiere dos puntos".to_string());
                }
                if finite_param("distance")? < 0.0 {
                    return Err("Distance: la distancia debe ser no negativa".to_string());
                }
            }
            "Angle" => {
                if inputs.len() != 2
                    || !inputs
                        .iter()
                        .all(|id| object_is(*id, |object| matches!(object, GeoObject::Line(_))))
                {
                    return Err("Angle: requiere dos rectas".to_string());
                }
                finite_param("angle")?;
            }
            "Tangent" => {
                if inputs.len() != 2
                    || !((object_is(inputs[0], |object| matches!(object, GeoObject::Circle(_)))
                        && object_is(inputs[1], |object| matches!(object, GeoObject::Line(_))))
                        || (object_is(inputs[0], |object| matches!(object, GeoObject::Line(_)))
                            && object_is(inputs[1], |object| {
                                matches!(object, GeoObject::Circle(_))
                            })))
                {
                    return Err("Tangent: requiere un círculo y una recta".to_string());
                }
            }
            "Coincident" => {
                if inputs.len() != 2
                    || !inputs
                        .iter()
                        .all(|id| object_is(*id, |object| matches!(object, GeoObject::Point(_))))
                {
                    return Err("Coincident: requiere dos puntos".to_string());
                }
            }
            "Horizontal" | "Vertical" => {
                if inputs.len() != 1
                    || !object_is(inputs[0], |object| matches!(object, GeoObject::Line(_)))
                {
                    return Err(format!("{name}: requiere una recta"));
                }
            }
            "EqualLength" => {
                if inputs.len() != 2
                    || !inputs.iter().all(|id| {
                        matches!(
                            self.get_object(*id),
                            Some(GeoObject::Line(line)) if line.kind == LineKind::Segment
                        )
                    })
                {
                    return Err("EqualLength: requiere dos segmentos".to_string());
                }
            }
            "Symmetry" => {
                if inputs.len() != 3
                    || !object_is(inputs[0], |object| matches!(object, GeoObject::Point(_)))
                    || !object_is(inputs[1], |object| matches!(object, GeoObject::Point(_)))
                    || !object_is(inputs[2], |object| matches!(object, GeoObject::Line(_)))
                {
                    return Err("Symmetry: requiere dos puntos y una recta".to_string());
                }
            }
            _ => return Err(format!("{name}: restricción numérica desconocida")),
        }
        Ok(())
    }

    fn validate_constructive_constraint_parts(
        &self,
        name: &str,
        inputs: &[ObjectId],
        output: &GeoObject,
        params: &HashMap<String, f64>,
    ) -> Result<(), String> {
        if inputs.is_empty() {
            return Err(format!("{name}: requiere objetos de entrada"));
        }
        let mut distinct = HashSet::new();
        for id in inputs {
            if self.get_object(*id).is_none() {
                return Err(format!("{name}: no se encontró el objeto de entrada {id}"));
            }
            if !distinct.insert(*id) {
                return Err(format!(
                    "{name}: los objetos de entrada deben ser distintos"
                ));
            }
        }

        let require_arity = |expected: usize| {
            if inputs.len() == expected {
                Ok(())
            } else {
                Err(format!(
                    "{name}: requiere exactamente {expected} objeto(s) de entrada"
                ))
            }
        };
        let input = |index: usize| -> Result<&GeoObject, String> {
            let id = inputs
                .get(index)
                .ok_or_else(|| format!("{name}: falta la entrada {index}"))?;
            self.get_object(*id)
                .ok_or_else(|| format!("{name}: no se encontró la entrada {index}"))
        };
        let point = |index: usize| -> Result<Point2, String> {
            match input(index)? {
                GeoObject::Point(point) => Ok(point.position),
                _ => Err(format!(
                    "{name}: la entrada {} debe ser un punto",
                    index + 1
                )),
            }
        };
        let finite_point = |point: Point2, operation: &str| {
            if point.x.is_finite() && point.y.is_finite() {
                Ok(point)
            } else {
                Err(format!(
                    "{name}: {operation} produjo coordenadas no finitas"
                ))
            }
        };
        let line_direction = |line: &crate::LineObj| -> Result<(f64, f64), String> {
            let dx = line.end.x - line.start.x;
            let dy = line.end.y - line.start.y;
            let length = dx.hypot(dy);
            if dx.is_finite() && dy.is_finite() && length.is_finite() && length > 1e-12 {
                Ok((dx, dy))
            } else {
                Err(format!(
                    "{name}: la recta de entrada no puede ser degenerada"
                ))
            }
        };
        let paired_point = |prefix: &str| -> Result<Option<Point2>, String> {
            let x = params.get(&format!("{prefix}_x")).copied();
            let y = params.get(&format!("{prefix}_y")).copied();
            match (x, y) {
                (Some(x), Some(y)) if x.is_finite() && y.is_finite() => Ok(Some(Point2::new(x, y))),
                (None, None) => Ok(None),
                _ => Err(format!(
                    "{name}: los parámetros {prefix}_x y {prefix}_y deben aparecer juntos"
                )),
            }
        };
        let required_param = |key: &str| {
            params
                .get(key)
                .copied()
                .filter(|value| value.is_finite())
                .ok_or_else(|| format!("{name}: requiere el parámetro finito '{key}'"))
        };

        match name {
            "Midpoint" => {
                require_arity(2)?;
                let a = point(0)?;
                let b = point(1)?;
                if !matches!(output, GeoObject::Point(_)) {
                    return Err("Midpoint: el resultado debe ser un punto".into());
                }
                finite_point(
                    Point2::new(a.x * 0.5 + b.x * 0.5, a.y * 0.5 + b.y * 0.5),
                    "el punto medio",
                )?;
            }
            "Translate" => {
                require_arity(1)?;
                let source = point(0)?;
                if !matches!(output, GeoObject::Point(_)) {
                    return Err("Translate: el resultado debe ser un punto".into());
                }
                let dx = required_param("dx")?;
                let dy = required_param("dy")?;
                finite_point(Point2::new(source.x + dx, source.y + dy), "la traslación")?;
            }
            "Rotate" => {
                if !(1..=2).contains(&inputs.len()) {
                    return Err("Rotate: requiere un punto de origen y un centro opcional".into());
                }
                let source = point(0)?;
                if !matches!(output, GeoObject::Point(_)) {
                    return Err("Rotate: el resultado debe ser un punto".into());
                }
                let angle = required_param("angle")?.to_radians();
                let literal_center = paired_point("center")?;
                let center = if inputs.len() == 2 {
                    if literal_center.is_some() {
                        return Err("Rotate: el centro no puede estar duplicado".into());
                    }
                    point(1)?
                } else {
                    literal_center.unwrap_or_else(|| Point2::new(0.0, 0.0))
                };
                let dx = source.x - center.x;
                let dy = source.y - center.y;
                finite_point(
                    Point2::new(
                        center.x + dx * angle.cos() - dy * angle.sin(),
                        center.y + dx * angle.sin() + dy * angle.cos(),
                    ),
                    "la rotación",
                )?;
            }
            "Dilate" => {
                if !matches!(output, GeoObject::Point(_)) {
                    return Err("Dilate: el resultado debe ser un punto".into());
                }
                required_param("factor")?;
                let literal_source = paired_point("source")?;
                let literal_center = paired_point("center")?;
                let (source, center) = if let Some(source) = literal_source {
                    if inputs.len() != 1 || literal_center.is_some() {
                        return Err(
                            "Dilate: una fuente literal requiere un único centro etiquetado".into(),
                        );
                    }
                    (source, point(0)?)
                } else {
                    if !(1..=2).contains(&inputs.len()) {
                        return Err("Dilate: requiere un punto de origen".into());
                    }
                    let source = point(0)?;
                    let center = if inputs.len() == 2 {
                        if literal_center.is_some() {
                            return Err("Dilate: el centro no puede estar duplicado".into());
                        }
                        point(1)?
                    } else {
                        literal_center.ok_or_else(|| "Dilate: requiere un centro".to_string())?
                    };
                    (source, center)
                };
                let factor = required_param("factor")?;
                finite_point(
                    Point2::new(
                        center.x + (source.x - center.x) * factor,
                        center.y + (source.y - center.y) * factor,
                    ),
                    "la dilatación",
                )?;
            }
            "Perpendicular" => {
                require_arity(2)?;
                let GeoObject::Line(line) = input(0)? else {
                    return Err("Perpendicular: el primer argumento debe ser una recta".into());
                };
                let (dx, dy) = line_direction(line)?;
                let through = point(1)?;
                let GeoObject::Line(output) = output else {
                    return Err("Perpendicular: el resultado debe ser una recta".into());
                };
                if output.kind != LineKind::Line {
                    return Err("Perpendicular: el resultado debe ser una recta infinita".into());
                }
                finite_point(
                    Point2::new(through.x - dy, through.y + dx),
                    "la perpendicular",
                )?;
                finite_point(
                    Point2::new(through.x + dy, through.y - dx),
                    "la perpendicular",
                )?;
            }
            "Parallel" => {
                require_arity(2)?;
                let GeoObject::Line(line) = input(0)? else {
                    return Err("Parallel: el primer argumento debe ser una recta".into());
                };
                let (dx, dy) = line_direction(line)?;
                let through = point(1)?;
                let GeoObject::Line(output) = output else {
                    return Err("Parallel: el resultado debe ser una recta".into());
                };
                if output.kind != LineKind::Line {
                    return Err("Parallel: el resultado debe ser una recta infinita".into());
                }
                finite_point(Point2::new(through.x - dx, through.y - dy), "la paralela")?;
                finite_point(Point2::new(through.x + dx, through.y + dy), "la paralela")?;
            }
            "Intersect" => {
                require_arity(2)?;
                if !matches!(output, GeoObject::Point(_)) {
                    return Err("Intersect: cada resultado debe ser un punto".into());
                }
                if doc_intersect(input(0)?, input(1)?).is_empty() {
                    return Err("Intersect: la intersección requerida no está definida".into());
                }
            }
            "Extrude" => {
                require_arity(1)?;
                let GeoObject::Polygon(polygon) = input(0)? else {
                    return Err("Extrude: la entrada debe ser un polígono".into());
                };
                if polygon.vertices.len() < 3 {
                    return Err("Extrude: el polígono requiere al menos tres vértices".into());
                }
                if !matches!(output, GeoObject::Segment3D(_)) {
                    return Err("Extrude: el resultado debe ser un segmento 3D".into());
                }
                let height = required_param("height")?;
                if height.abs() <= 1e-12 {
                    return Err("Extrude: la altura debe ser distinta de cero".into());
                }
                let edge_index = required_param("edge_index")?;
                if edge_index < 0.0
                    || edge_index.fract() != 0.0
                    || edge_index >= polygon.vertices.len() as f64
                {
                    return Err("Extrude: edge_index está fuera del polígono".into());
                }
                let edge_kind = required_param("edge_kind")?;
                if edge_kind.fract() != 0.0 || !(0.0..=2.0).contains(&edge_kind) {
                    return Err("Extrude: edge_kind debe ser 0, 1 o 2".into());
                }
            }
            "PointOnObject" => {
                require_arity(2)?;
                let probe = point(1)?;
                if !matches!(output, GeoObject::Point(_)) {
                    return Err("PointOnObject: el resultado debe ser un punto".into());
                }
                let projected = match input(0)? {
                    GeoObject::Line(line) => {
                        line_direction(line)?;
                        project_point_to_line(probe, line.start, line.end)
                    }
                    GeoObject::Circle(circle)
                        if circle.radius.is_finite() && circle.radius > 0.0 =>
                    {
                        project_point_to_circle(probe, circle.center, circle.radius)
                    }
                    GeoObject::Polygon(polygon) if polygon.vertices.len() >= 2 => {
                        project_point_to_polygon_edges(probe, &polygon.vertices)
                    }
                    _ => {
                        return Err(
                            "PointOnObject: la primera entrada debe ser una curva proyectable"
                                .into(),
                        )
                    }
                };
                finite_point(projected, "la proyección")?;
            }
            "CircleByCenterRadius" => {
                require_arity(1)?;
                point(0)?;
                if !matches!(output, GeoObject::Circle(_)) {
                    return Err("CircleByCenterRadius: el resultado debe ser un círculo".into());
                }
                if required_param("radius")? <= 0.0 {
                    return Err("CircleByCenterRadius: el radio debe ser positivo".into());
                }
            }
            "CircleByThreePoints" => {
                require_arity(3)?;
                if !matches!(output, GeoObject::Circle(_)) {
                    return Err("CircleByThreePoints: el resultado debe ser un círculo".into());
                }
                let (center, radius) = circle_from_three_points(point(0)?, point(1)?, point(2)?)
                    .ok_or_else(|| {
                        "CircleByThreePoints: los puntos no definen un círculo".to_string()
                    })?;
                if !center.x.is_finite()
                    || !center.y.is_finite()
                    || !radius.is_finite()
                    || radius <= 0.0
                {
                    return Err("CircleByThreePoints: el círculo no es representable".into());
                }
            }
            "EllipseByFoci" => {
                require_arity(3)?;
                if !matches!(output, GeoObject::Ellipse(_)) {
                    return Err("EllipseByFoci: el resultado debe ser una elipse".into());
                }
                let f1 = point(0)?;
                let f2 = point(1)?;
                let on_ellipse = point(2)?;
                let d1 = on_ellipse.distance(&f1);
                let d2 = on_ellipse.distance(&f2);
                let a = d1 * 0.5 + d2 * 0.5;
                let c = f1.distance(&f2) * 0.5;
                let b = ((a - c) * (a + c)).sqrt();
                if !a.is_finite() || !b.is_finite() || a <= c + 1e-12 || b <= 1e-12 {
                    return Err("EllipseByFoci: las entradas no definen una elipse".into());
                }
            }
            "ParabolaByFocusDirectrix" => {
                require_arity(2)?;
                let focus = point(0)?;
                let GeoObject::Line(directrix) = input(1)? else {
                    return Err(
                        "ParabolaByFocusDirectrix: la segunda entrada debe ser una recta".into(),
                    );
                };
                line_direction(directrix)?;
                if !matches!(output, GeoObject::Parabola(_)) {
                    return Err(
                        "ParabolaByFocusDirectrix: el resultado debe ser una parábola".into(),
                    );
                }
                let projection = project_point_to_line(focus, directrix.start, directrix.end);
                let p = focus.distance(&projection) * 0.5;
                if !projection.x.is_finite()
                    || !projection.y.is_finite()
                    || !p.is_finite()
                    || p <= 1e-12
                {
                    return Err(
                        "ParabolaByFocusDirectrix: el foco no define una parábola con la directriz"
                            .into(),
                    );
                }
            }
            "HyperbolaByFoci" => {
                require_arity(3)?;
                if !matches!(output, GeoObject::Hyperbola(_)) {
                    return Err("HyperbolaByFoci: el resultado debe ser una hipérbola".into());
                }
                let f1 = point(0)?;
                let f2 = point(1)?;
                let on_hyperbola = point(2)?;
                let a = (on_hyperbola.distance(&f1) - on_hyperbola.distance(&f2)).abs() * 0.5;
                let c = f1.distance(&f2) * 0.5;
                let b = ((c - a) * (c + a)).sqrt();
                if !a.is_finite() || !b.is_finite() || a <= 1e-12 || a >= c - 1e-12 || b <= 1e-12 {
                    return Err("HyperbolaByFoci: las entradas no definen una hipérbola".into());
                }
            }
            "ConicByFivePoints" => {
                require_arity(5)?;
                if !matches!(output, GeoObject::Ellipse(_) | GeoObject::Hyperbola(_)) {
                    return Err("ConicByFivePoints: el resultado debe ser una cónica".into());
                }
                let points = (0..5).map(point).collect::<Result<Vec<_>, _>>()?;
                if conic_from_five_points(&points).is_none() {
                    return Err("ConicByFivePoints: los puntos no definen una cónica válida".into());
                }
            }
            "Locus" => {
                require_arity(2)?;
                if inputs[0] == inputs[1] {
                    return Err("Locus: los puntos de entrada deben ser distintos".into());
                }
                point(0)?;
                point(1)?;
                let GeoObject::Pencil(locus) = output else {
                    return Err("Locus: el resultado debe ser un trazo persistente".into());
                };
                let Some(binding) = locus.locus_binding() else {
                    return Err("Locus: el trazo debe conservar driver y target".into());
                };
                if binding.driver != inputs[0] || binding.target != inputs[1] {
                    return Err(
                        "Locus: las referencias del trazo no coinciden con sus entradas".into(),
                    );
                }
                if locus.points.is_empty() {
                    return Err("Locus: el trazo requiere una muestra inicial".into());
                }
            }
            _ if Self::is_numeric_constraint_name(name) => {}
            _ => return Err(format!("{name}: construcción desconocida")),
        }
        Ok(())
    }

    pub(crate) fn validate_constructive_constraint_definition(
        &self,
        name: &str,
        inputs: &[ObjectId],
        outputs: &[ObjectId],
        params: &HashMap<String, f64>,
    ) -> Result<(), String> {
        if Self::is_numeric_constraint_name(name) {
            return Ok(());
        }
        if outputs.is_empty() {
            return Err(format!("{name}: requiere al menos un resultado"));
        }
        if name != "Intersect" && outputs.len() != 1 {
            return Err(format!("{name}: requiere exactamente un resultado"));
        }
        for output_id in outputs {
            let output = self
                .get_object(*output_id)
                .ok_or_else(|| format!("{name}: no se encontró el resultado"))?;
            self.validate_constructive_constraint_parts(name, inputs, output, params)?;
        }
        if name == "Intersect" {
            let intersections = doc_intersect(
                self.get_object(inputs[0])
                    .ok_or_else(|| "Intersect: falta la primera entrada".to_string())?,
                self.get_object(inputs[1])
                    .ok_or_else(|| "Intersect: falta la segunda entrada".to_string())?,
            );
            if intersections.len() < outputs.len() {
                return Err("Intersect: no hay suficientes intersecciones definidas".into());
            }
        }
        Ok(())
    }

    fn try_add_numeric_constraint(
        &mut self,
        name: &str,
        inputs: Vec<ObjectId>,
        params: HashMap<String, f64>,
    ) -> Result<usize, String> {
        self.validate_numeric_constraint_definition(name, &inputs, &params)?;
        self.constraints
            .try_add_constraint(name, inputs, vec![], params)
    }

    /// Add a numeric distance constraint between two objects.
    pub fn add_distance_constraint(&mut self, a: ObjectId, b: ObjectId, distance: f64) -> usize {
        self.try_add_distance_constraint(a, b, distance)
            .unwrap_or_else(|error| {
                log::warn!("{error}");
                usize::MAX
            })
    }

    pub fn try_add_distance_constraint(
        &mut self,
        a: ObjectId,
        b: ObjectId,
        distance: f64,
    ) -> Result<usize, String> {
        let mut params = HashMap::new();
        params.insert("distance".to_string(), distance);
        self.try_add_numeric_constraint("Distance", vec![a, b], params)
    }

    /// Add a numeric angle constraint between two objects (lines) or three points.
    pub fn add_angle_constraint(&mut self, a: ObjectId, b: ObjectId, angle_deg: f64) -> usize {
        self.try_add_angle_constraint(a, b, angle_deg)
            .unwrap_or_else(|error| {
                log::warn!("{error}");
                usize::MAX
            })
    }

    pub fn try_add_angle_constraint(
        &mut self,
        a: ObjectId,
        b: ObjectId,
        angle_deg: f64,
    ) -> Result<usize, String> {
        let mut params = HashMap::new();
        params.insert("angle".to_string(), angle_deg);
        self.try_add_numeric_constraint("Angle", vec![a, b], params)
    }

    /// Add a numeric tangent constraint between two objects.
    pub fn add_tangent_constraint(&mut self, a: ObjectId, b: ObjectId) -> usize {
        self.try_add_tangent_constraint(a, b)
            .unwrap_or_else(|error| {
                log::warn!("{error}");
                usize::MAX
            })
    }

    pub fn try_add_tangent_constraint(
        &mut self,
        a: ObjectId,
        b: ObjectId,
    ) -> Result<usize, String> {
        self.try_add_numeric_constraint("Tangent", vec![a, b], HashMap::new())
    }

    /// Add a numeric coincident constraint between two points.
    pub fn add_coincident_constraint(&mut self, a: ObjectId, b: ObjectId) -> usize {
        self.try_add_coincident_constraint(a, b)
            .unwrap_or_else(|error| {
                log::warn!("{error}");
                usize::MAX
            })
    }

    pub fn try_add_coincident_constraint(
        &mut self,
        a: ObjectId,
        b: ObjectId,
    ) -> Result<usize, String> {
        self.try_add_numeric_constraint("Coincident", vec![a, b], HashMap::new())
    }

    /// Add a numeric horizontal constraint to a line.
    pub fn add_horizontal_constraint(&mut self, line: ObjectId) -> usize {
        self.try_add_horizontal_constraint(line)
            .unwrap_or_else(|error| {
                log::warn!("{error}");
                usize::MAX
            })
    }

    pub fn try_add_horizontal_constraint(&mut self, line: ObjectId) -> Result<usize, String> {
        self.try_add_numeric_constraint("Horizontal", vec![line], HashMap::new())
    }

    /// Add a numeric vertical constraint to a line.
    pub fn add_vertical_constraint(&mut self, line: ObjectId) -> usize {
        self.try_add_vertical_constraint(line)
            .unwrap_or_else(|error| {
                log::warn!("{error}");
                usize::MAX
            })
    }

    pub fn try_add_vertical_constraint(&mut self, line: ObjectId) -> Result<usize, String> {
        self.try_add_numeric_constraint("Vertical", vec![line], HashMap::new())
    }

    /// Add a numeric equal-length constraint between two line segments.
    pub fn add_equal_length_constraint(&mut self, line1: ObjectId, line2: ObjectId) -> usize {
        self.try_add_equal_length_constraint(line1, line2)
            .unwrap_or_else(|error| {
                log::warn!("{error}");
                usize::MAX
            })
    }

    pub fn try_add_equal_length_constraint(
        &mut self,
        line1: ObjectId,
        line2: ObjectId,
    ) -> Result<usize, String> {
        self.try_add_numeric_constraint("EqualLength", vec![line1, line2], HashMap::new())
    }

    /// Add a numeric symmetry constraint: `mirror_point` is the mirror of
    /// `point` across `mirror_line`.
    pub fn add_symmetry_constraint(
        &mut self,
        point: ObjectId,
        mirror_point: ObjectId,
        mirror_line: ObjectId,
    ) -> usize {
        self.try_add_symmetry_constraint(point, mirror_point, mirror_line)
            .unwrap_or_else(|error| {
                log::warn!("{error}");
                usize::MAX
            })
    }

    pub fn try_add_symmetry_constraint(
        &mut self,
        point: ObjectId,
        mirror_point: ObjectId,
        mirror_line: ObjectId,
    ) -> Result<usize, String> {
        self.try_add_numeric_constraint(
            "Symmetry",
            vec![point, mirror_point, mirror_line],
            HashMap::new(),
        )
    }

    /// Add a constructive constraint that creates an ellipse from two foci and
    /// a point on the ellipse.
    pub fn add_ellipse_by_foci_constraint(
        &mut self,
        f1: ObjectId,
        f2: ObjectId,
        p: ObjectId,
    ) -> usize {
        let (_, cons_id) = self.add_constructed_object(
            GeoObject::Ellipse(EllipseObj::new(Point2::new(0.0, 0.0), 1.0, 1.0)),
            "EllipseByFoci",
            &[f1, f2, p],
        );
        cons_id
    }

    /// Add a constructive constraint that creates a parabola from a focus point
    /// and a directrix line.
    pub fn add_parabola_by_focus_directrix_constraint(
        &mut self,
        focus: ObjectId,
        directrix: ObjectId,
    ) -> usize {
        let (_, cons_id) = self.add_constructed_object(
            GeoObject::Parabola(ParabolaObj::new(Point2::new(0.0, 0.0), 1.0)),
            "ParabolaByFocusDirectrix",
            &[focus, directrix],
        );
        cons_id
    }

    /// Add a constructive constraint that creates a hyperbola from two foci and
    /// a point on the hyperbola.
    pub fn add_hyperbola_by_foci_constraint(
        &mut self,
        f1: ObjectId,
        f2: ObjectId,
        p: ObjectId,
    ) -> usize {
        let (_, cons_id) = self.add_constructed_object(
            GeoObject::Hyperbola(HyperbolaObj::new(Point2::new(0.0, 0.0), 1.0, 1.0)),
            "HyperbolaByFoci",
            &[f1, f2, p],
        );
        cons_id
    }

    /// Add a constructive constraint that fits a conic through five points.
    pub fn add_conic_by_five_points_constraint(&mut self, points: &[ObjectId]) -> usize {
        self.try_add_conic_by_five_points_constraint(points)
            .unwrap_or_else(|error| {
                log::warn!("{error}");
                usize::MAX
            })
    }

    /// Fits and registers a conic only when all five inputs form a
    /// representable ellipse or hyperbola. Failed fits leave the document
    /// untouched instead of registering a placeholder ellipse.
    pub fn try_add_conic_by_five_points_constraint(
        &mut self,
        points: &[ObjectId],
    ) -> Result<usize, String> {
        if points.len() != 5 {
            return Err("ConicByFivePoints: requiere exactamente cinco puntos".to_string());
        }

        let positions = points
            .iter()
            .map(|id| match self.get_object(*id) {
                Some(GeoObject::Point(point)) => Ok(point.position),
                _ => Err("ConicByFivePoints: requiere cinco puntos válidos".to_string()),
            })
            .collect::<Result<Vec<_>, _>>()?;
        let mut conic = conic_from_five_points(&positions).ok_or_else(|| {
            "ConicByFivePoints: los puntos no definen una cónica válida".to_string()
        })?;
        match &mut conic {
            GeoObject::Ellipse(ellipse) => ellipse.label = "C".to_string(),
            GeoObject::Hyperbola(hyperbola) => hyperbola.label = "C".to_string(),
            _ => return Err("ConicByFivePoints: ajuste no representable".to_string()),
        }
        let (_, constraint_id) =
            self.try_add_constructed_object(conic, "ConicByFivePoints", points)?;
        Ok(constraint_id)
    }

    /// Re-evaluate constraints atomically and return a diagnostic when the
    /// numeric system cannot satisfy the document state.
    pub fn try_re_evaluate_constraints(&mut self, order: &[usize]) -> Result<(), String> {
        if self.constraints.constraint_count() == 0 {
            return Ok(());
        }
        let mut staged = self.detached_clone_for_staging();
        crate::validation::validate_document(&staged)?;
        staged.re_evaluate_constraints_in_place(order)?;
        crate::validation::validate_document(&staged)?;
        *self = staged;
        Ok(())
    }

    /// Legacy fire-and-forget propagation for UI paths that cannot yet return
    /// a diagnostic. Command paths use [`Self::try_re_evaluate_constraints`].
    pub fn re_evaluate_constraints(&mut self, order: &[usize]) {
        if let Err(error) = self.try_re_evaluate_constraints(order) {
            log::warn!("{error}");
        }
    }

    fn re_evaluate_constraints_in_place(&mut self, order: &[usize]) -> Result<(), String> {
        if self.constraints.constraint_count() == 0 {
            return Ok(());
        }
        self.bump_version();
        // Numeric constraints have no outputs, so they never appear in a
        // propagation order rooted at changed objects. Always include them.
        let numeric_ids: Vec<usize> = self
            .constraints
            .iter()
            .filter(|c| Self::is_numeric_constraint_name(&c.name))
            .map(|c| c.id)
            .collect();

        let constructive_ids: Vec<usize> = order
            .iter()
            .cloned()
            .filter(|&id| {
                self.constraints
                    .get_constraint(id)
                    .map(|c| !Self::is_numeric_constraint_name(&c.name))
                    .unwrap_or(false)
            })
            .collect();

        if numeric_ids.is_empty() {
            self.apply_constructive_constraints(&constructive_ids)?;
            self.capture_locus_samples(&constructive_ids);
            return Ok(());
        }

        for id in &numeric_ids {
            let constraint = self
                .constraints
                .get_constraint(*id)
                .ok_or_else(|| format!("Numeric constraint {id} is missing"))?;
            self.validate_numeric_constraint_definition(
                &constraint.name,
                &constraint.inputs,
                &constraint.params,
            )?;
        }

        let var_map = self.build_solver_variables_for_constraints(&numeric_ids);
        let var_index: HashMap<(ObjectId, ObjField), VarIndex> = var_map
            .iter()
            .enumerate()
            .map(|(i, (id, field))| ((*id, *field), i))
            .collect();

        let solver = NumericSolver::default();
        let mut changed: Vec<ObjectId> = Vec::new();
        let bounds: Vec<crate::numeric_solver::Bounds> =
            std::iter::repeat_with(crate::numeric_solver::Bounds::default)
                .take(var_map.len())
                .collect();

        for _ in 0..5 {
            let current_order = if changed.is_empty() {
                constructive_ids.clone()
            } else {
                self.propagation_order(&changed)
                    .into_iter()
                    .filter(|id| !numeric_ids.contains(id))
                    .collect()
            };
            self.apply_constructive_constraints(&current_order)?;

            // Constructed inputs are captured as constants by numeric equations,
            // so they must be rebound after every propagation pass.
            let equations = self.build_numeric_equations(&numeric_ids, &var_index)?;

            let mut vars: Vec<f64> = var_map
                .iter()
                .map(|(id, field)| self.get_field_value(*id, *field))
                .collect();

            // Keep the stored solution in sync with the current document state
            // before solving so that user edits are not reverted by the warm
            // start.
            for (id, field) in &var_map {
                let current = self.get_field_value(*id, *field);
                self.last_solution.insert((*id, *field), current);
            }
            let warm_start: Vec<f64> = var_map
                .iter()
                .map(|(id, field)| {
                    self.last_solution
                        .get(&(*id, *field))
                        .copied()
                        .unwrap_or_else(|| self.get_field_value(*id, *field))
                })
                .collect();

            match solver.solve_with_warm_start_and_bounds(
                &mut vars,
                &equations,
                Some(&warm_start),
                &bounds,
            ) {
                Ok(stats) => {
                    for ((id, field), value) in var_map.iter().zip(vars.iter()) {
                        self.last_solution.insert((*id, *field), *value);
                    }
                    changed = self.write_solver_variables(&var_map, &vars);
                    if stats.final_residual < solver.tol && changed.is_empty() {
                        break;
                    }
                }
                Err(error) => return Err(format!("Numeric constraint solver failed: {error}")),
            }
        }

        let mut capture_order = constructive_ids.clone();
        if !changed.is_empty() {
            let final_order: Vec<usize> = self
                .propagation_order(&changed)
                .into_iter()
                .filter(|id| !numeric_ids.contains(id))
                .collect();
            self.apply_constructive_constraints(&final_order)?;
            capture_order.extend(final_order);
        }

        self.verify_numeric_constraints(&numeric_ids, &var_map, &var_index)?;
        self.capture_locus_samples(&capture_order);
        Ok(())
    }

    /// Captura una sola posición distinta por salida Locus, después de terminar
    /// toda propagación constructiva y numérica. Nunca se invoca dentro de los
    /// pases intermedios del solver.
    fn capture_locus_samples(&mut self, order: &[usize]) {
        let samples: Vec<(ObjectId, Point2)> = order
            .iter()
            .filter_map(|constraint_id| self.constraints.get_constraint(*constraint_id))
            .filter(|constraint| constraint.name == "Locus")
            .filter_map(|constraint| {
                let locus_id = *constraint.outputs.first()?;
                let GeoObject::Pencil(locus) = self.get_object(locus_id)? else {
                    return None;
                };
                let binding = locus.locus_binding()?;
                if binding.driver != *constraint.inputs.first()?
                    || binding.target != *constraint.inputs.get(1)?
                {
                    return None;
                }
                let GeoObject::Point(target) = self.get_object(binding.target)? else {
                    return None;
                };
                (target.position.x.is_finite() && target.position.y.is_finite())
                    .then_some((locus_id, target.position))
            })
            .collect();

        let mut changed = false;
        for (locus_id, target_position) in samples {
            if let Some(GeoObject::Pencil(locus)) = self.objects.get_mut(&locus_id) {
                changed |= locus.capture_locus_sample(target_position);
            }
        }
        if changed {
            self.spatial_dirty = true;
        }
    }

    fn verify_numeric_constraints(
        &self,
        numeric_ids: &[usize],
        var_map: &[(ObjectId, ObjField)],
        var_index: &HashMap<(ObjectId, ObjField), VarIndex>,
    ) -> Result<(), String> {
        let equations = self.build_numeric_equations(numeric_ids, var_index)?;
        let mut vars: Vec<f64> = var_map
            .iter()
            .map(|(id, field)| self.get_field_value(*id, *field))
            .collect();
        let verifier = NumericSolver {
            max_iter: 0,
            ..NumericSolver::default()
        };
        match verifier.solve(&mut vars, &equations) {
            Ok(_) => Ok(()),
            Err(SolveError::MaxIterations { final_residual })
            | Err(SolveError::Unsatisfied { final_residual }) => Err(format!(
                "Numeric constraints remain unsatisfied (residual {final_residual:.3e})"
            )),
            Err(error) => Err(format!("Numeric constraint solver failed: {error}")),
        }
    }

    fn apply_constructive_constraints(&mut self, order: &[usize]) -> Result<(), String> {
        for cons_id in order {
            let cons = self
                .constraints
                .get_constraint(*cons_id)
                .cloned()
                .ok_or_else(|| format!("Constructive constraint {cons_id} is missing"))?;
            self.validate_constructive_constraint_definition(
                &cons.name,
                &cons.inputs,
                &cons.outputs,
                &cons.params,
            )?;
            match cons.name.as_str() {
                "Midpoint" if cons.inputs.len() >= 2 && !cons.outputs.is_empty() => {
                    let a = self.get_object(cons.inputs[0]).cloned();
                    let b = self.get_object(cons.inputs[1]).cloned();
                    if let (Some(GeoObject::Point(a)), Some(GeoObject::Point(b))) = (&a, &b) {
                        if let Some(GeoObject::Point(out)) = self.get_object_mut(cons.outputs[0]) {
                            out.position = grafito_geometry::Point2::new(
                                a.position.x * 0.5 + b.position.x * 0.5,
                                a.position.y * 0.5 + b.position.y * 0.5,
                            );
                        }
                    }
                }
                "Translate" if !cons.inputs.is_empty() && !cons.outputs.is_empty() => {
                    let obj = self.get_object(cons.inputs[0]).cloned();
                    let dx = cons.params.get("dx").copied().unwrap_or(0.0);
                    let dy = cons.params.get("dy").copied().unwrap_or(0.0);
                    if let Some(GeoObject::Point(p)) = &obj {
                        if let Some(GeoObject::Point(out)) = self.get_object_mut(cons.outputs[0]) {
                            out.position =
                                grafito_geometry::Point2::new(p.position.x + dx, p.position.y + dy);
                        }
                    }
                }
                "Rotate" if !cons.inputs.is_empty() && !cons.outputs.is_empty() => {
                    let obj = self.get_object(cons.inputs[0]).cloned();
                    let center = cons
                        .inputs
                        .get(1)
                        .and_then(|id| self.get_object(*id))
                        .and_then(|object| match object {
                            GeoObject::Point(point) => Some(point.position),
                            _ => None,
                        })
                        .or_else(|| {
                            Some(Point2::new(
                                *cons.params.get("center_x")?,
                                *cons.params.get("center_y")?,
                            ))
                        })
                        .unwrap_or_else(|| Point2::new(0.0, 0.0));
                    let angle = cons.params.get("angle").copied().unwrap_or(0.0);
                    let angle_rad = angle.to_radians();
                    if let Some(GeoObject::Point(p)) = &obj {
                        if let Some(GeoObject::Point(out)) = self.get_object_mut(cons.outputs[0]) {
                            let dx = p.position.x - center.x;
                            let dy = p.position.y - center.y;
                            out.position = grafito_geometry::Point2::new(
                                center.x + dx * angle_rad.cos() - dy * angle_rad.sin(),
                                center.y + dx * angle_rad.sin() + dy * angle_rad.cos(),
                            );
                        }
                    }
                }
                "Dilate" if !cons.inputs.is_empty() && !cons.outputs.is_empty() => {
                    let literal_source = cons
                        .params
                        .get("source_x")
                        .zip(cons.params.get("source_y"))
                        .map(|(x, y)| Point2::new(*x, *y));
                    let (source, center_input_index) = if let Some(source) = literal_source {
                        (Some(source), 0)
                    } else {
                        let source = cons
                            .inputs
                            .first()
                            .and_then(|id| self.get_object(*id))
                            .and_then(|object| match object {
                                GeoObject::Point(point) => Some(point.position),
                                _ => None,
                            });
                        (source, 1)
                    };
                    let center = cons
                        .inputs
                        .get(center_input_index)
                        .and_then(|id| self.get_object(*id))
                        .and_then(|object| match object {
                            GeoObject::Point(point) => Some(point.position),
                            _ => None,
                        })
                        .or_else(|| {
                            Some(Point2::new(
                                *cons.params.get("center_x")?,
                                *cons.params.get("center_y")?,
                            ))
                        });
                    let factor = cons.params.get("factor").copied().unwrap_or(1.0);
                    if let (Some(point), Some(center)) = (source, center) {
                        if let Some(GeoObject::Point(out)) = self.get_object_mut(cons.outputs[0]) {
                            out.position = Point2::new(
                                center.x + (point.x - center.x) * factor,
                                center.y + (point.y - center.y) * factor,
                            );
                        }
                    }
                }
                "Intersect" if cons.inputs.len() >= 2 => {
                    let a = self.get_object(cons.inputs[0]).cloned();
                    let b = self.get_object(cons.inputs[1]).cloned();
                    if let (Some(a), Some(b)) = (&a, &b) {
                        let pts = doc_intersect(a, b);
                        for (i, out_id) in cons.outputs.iter().enumerate() {
                            if let Some(GeoObject::Point(out)) = self.get_object_mut(*out_id) {
                                if let Some(pt) = pts.get(i) {
                                    out.position = *pt;
                                }
                            }
                        }
                    }
                }
                "Extrude" if !cons.inputs.is_empty() => {
                    let height = cons.params.get("height").copied().unwrap_or(0.0);
                    if height.abs() < 1e-12 {
                        continue;
                    }
                    if let Some(GeoObject::Polygon(poly)) = self.get_object(cons.inputs[0]) {
                        let verts = poly.vertices.clone();
                        if verts.len() < 3 {
                            continue;
                        }
                        let edge_index = cons
                            .params
                            .get("edge_index")
                            .copied()
                            .filter(|index| {
                                index.is_finite()
                                    && *index >= 0.0
                                    && index.fract() == 0.0
                                    && *index < verts.len() as f64
                            })
                            .map(|index| index as usize)
                            .unwrap_or(0);
                        let edge_kind = cons
                            .params
                            .get("edge_kind")
                            .copied()
                            .filter(|kind| {
                                kind.is_finite()
                                    && *kind >= 0.0
                                    && kind.fract() == 0.0
                                    && *kind <= 2.0
                            })
                            .map(|kind| kind as usize)
                            .unwrap_or(0);
                        let base_y = 0.0;
                        let top_y = height;
                        let v = verts[edge_index];
                        let vn = verts[(edge_index + 1) % verts.len()];
                        let base = Point3D::new(v.x, base_y, v.y);
                        let top = Point3D::new(v.x, top_y, v.y);
                        let next_base = Point3D::new(vn.x, base_y, vn.y);
                        let next_top = Point3D::new(vn.x, top_y, vn.y);
                        let (a, b) = match edge_kind {
                            0 => (base, top),
                            1 => (base, next_base),
                            2 => (top, next_top),
                            _ => unreachable!("Extrude edge kind was validated above"),
                        };
                        for output in cons.outputs {
                            if let Some(GeoObject::Segment3D(segment)) = self.get_object_mut(output)
                            {
                                segment.a = a;
                                segment.b = b;
                            }
                        }
                    }
                }
                "Perpendicular" if cons.inputs.len() >= 2 && !cons.outputs.is_empty() => {
                    let line_obj = self.get_object(cons.inputs[0]).cloned();
                    let point_obj = self.get_object(cons.inputs[1]).cloned();
                    if let (Some(GeoObject::Line(line)), Some(GeoObject::Point(pt))) =
                        (&line_obj, &point_obj)
                    {
                        if let Some(GeoObject::Line(out)) = self.get_object_mut(cons.outputs[0]) {
                            let dx = line.end.x - line.start.x;
                            let dy = line.end.y - line.start.y;
                            let direction_length = dx.hypot(dy);
                            if !dx.is_finite()
                                || !dy.is_finite()
                                || !direction_length.is_finite()
                                || direction_length <= 1e-12
                            {
                                continue;
                            }
                            out.start = Point2::new(pt.position.x - dy, pt.position.y + dx);
                            out.end = Point2::new(pt.position.x + dy, pt.position.y - dx);
                            out.kind = LineKind::Line;
                        }
                    }
                }
                "Parallel" if cons.inputs.len() >= 2 && !cons.outputs.is_empty() => {
                    let line_obj = self.get_object(cons.inputs[0]).cloned();
                    let point_obj = self.get_object(cons.inputs[1]).cloned();
                    if let (Some(GeoObject::Line(line)), Some(GeoObject::Point(pt))) =
                        (&line_obj, &point_obj)
                    {
                        if let Some(GeoObject::Line(out)) = self.get_object_mut(cons.outputs[0]) {
                            let dx = line.end.x - line.start.x;
                            let dy = line.end.y - line.start.y;
                            out.start = Point2::new(pt.position.x - dx, pt.position.y - dy);
                            out.end = Point2::new(pt.position.x + dx, pt.position.y + dy);
                            out.kind = LineKind::Line;
                        }
                    }
                }
                "PointOnObject" if cons.inputs.len() >= 2 && !cons.outputs.is_empty() => {
                    let obj = self.get_object(cons.inputs[0]).cloned();
                    let point = self.get_object(cons.inputs[1]).cloned();
                    if let (Some(obj), Some(GeoObject::Point(pt))) = (&obj, &point) {
                        if let Some(GeoObject::Point(out)) = self.get_object_mut(cons.outputs[0]) {
                            out.position = match obj {
                                GeoObject::Line(l) => {
                                    project_point_to_line(pt.position, l.start, l.end)
                                }
                                GeoObject::Circle(c) => {
                                    project_point_to_circle(pt.position, c.center, c.radius)
                                }
                                GeoObject::Polygon(poly) => {
                                    project_point_to_polygon_edges(pt.position, &poly.vertices)
                                }
                                _ => pt.position,
                            };
                        }
                    }
                }
                "CircleByCenterRadius" if !cons.inputs.is_empty() && !cons.outputs.is_empty() => {
                    let radius = cons.params.get("radius").copied().unwrap_or(1.0);
                    if let Some(GeoObject::Point(center)) = self.get_object(cons.inputs[0]).cloned()
                    {
                        if let Some(GeoObject::Circle(out)) = self.get_object_mut(cons.outputs[0]) {
                            out.center = center.position;
                            out.radius = radius;
                        }
                    }
                }
                "CircleByThreePoints" if cons.inputs.len() >= 3 && !cons.outputs.is_empty() => {
                    let a = self.get_object(cons.inputs[0]).cloned();
                    let b = self.get_object(cons.inputs[1]).cloned();
                    let c = self.get_object(cons.inputs[2]).cloned();
                    if let (
                        Some(GeoObject::Point(pa)),
                        Some(GeoObject::Point(pb)),
                        Some(GeoObject::Point(pc)),
                    ) = (&a, &b, &c)
                    {
                        if let Some((center, radius)) =
                            circle_from_three_points(pa.position, pb.position, pc.position)
                        {
                            if let Some(GeoObject::Circle(out)) =
                                self.get_object_mut(cons.outputs[0])
                            {
                                out.center = center;
                                out.radius = radius;
                            }
                        }
                    }
                }
                "EllipseByFoci" if cons.inputs.len() >= 3 && !cons.outputs.is_empty() => {
                    let f1 = self.get_object(cons.inputs[0]).cloned();
                    let f2 = self.get_object(cons.inputs[1]).cloned();
                    let p = self.get_object(cons.inputs[2]).cloned();
                    if let (
                        Some(GeoObject::Point(f1)),
                        Some(GeoObject::Point(f2)),
                        Some(GeoObject::Point(p)),
                    ) = (&f1, &f2, &p)
                    {
                        if let Some(GeoObject::Ellipse(out)) = self.get_object_mut(cons.outputs[0])
                        {
                            let d1 = p.position.distance(&f1.position);
                            let d2 = p.position.distance(&f2.position);
                            let a = (d1 + d2) * 0.5;
                            let c = f1.position.distance(&f2.position) * 0.5;
                            let b = (a * a - c * c).max(0.0).sqrt();
                            out.center = Point2::new(
                                (f1.position.x + f2.position.x) * 0.5,
                                (f1.position.y + f2.position.y) * 0.5,
                            );
                            out.rx = a;
                            out.ry = b;
                            out.angle = (f2.position.y - f1.position.y)
                                .atan2(f2.position.x - f1.position.x);
                        }
                    }
                }
                "ParabolaByFocusDirectrix"
                    if cons.inputs.len() >= 2 && !cons.outputs.is_empty() =>
                {
                    let focus = self.get_object(cons.inputs[0]).cloned();
                    let directrix = self.get_object(cons.inputs[1]).cloned();
                    if let (Some(GeoObject::Point(f)), Some(GeoObject::Line(d))) =
                        (&focus, &directrix)
                    {
                        if let Some(GeoObject::Parabola(out)) = self.get_object_mut(cons.outputs[0])
                        {
                            let proj = project_point_to_line(f.position, d.start, d.end);
                            out.vertex = Point2::new(
                                (f.position.x + proj.x) * 0.5,
                                (f.position.y + proj.y) * 0.5,
                            );
                            out.p = f.position.distance(&proj) * 0.5;
                            let dx = d.end.x - d.start.x;
                            let dy = d.end.y - d.start.y;
                            // Axis direction points from the directrix toward the focus.
                            let axis_dx = f.position.x - proj.x;
                            let axis_dy = f.position.y - proj.y;
                            if dx.abs() < 1e-12 {
                                // Directrix is vertical => parabola opens horizontally.
                                out.vertical = false;
                                out.angle = if axis_dx >= 0.0 {
                                    -std::f64::consts::FRAC_PI_2
                                } else {
                                    std::f64::consts::FRAC_PI_2
                                };
                            } else if dy.abs() < 1e-12 {
                                // Directrix is horizontal => parabola opens vertically.
                                out.vertical = true;
                                out.angle = if axis_dy >= 0.0 {
                                    0.0
                                } else {
                                    std::f64::consts::PI
                                };
                            } else {
                                // General directrix: the local parabola (t, t^2/(4p))
                                // opens toward +y, so rotate it so that its axis aligns
                                // with the focus-directrix axis.
                                let axis_angle = axis_dy.atan2(axis_dx);
                                out.vertical = false;
                                out.angle = axis_angle - std::f64::consts::FRAC_PI_2;
                            }
                        }
                    }
                }
                "HyperbolaByFoci" if cons.inputs.len() >= 3 && !cons.outputs.is_empty() => {
                    let f1 = self.get_object(cons.inputs[0]).cloned();
                    let f2 = self.get_object(cons.inputs[1]).cloned();
                    let p = self.get_object(cons.inputs[2]).cloned();
                    if let (
                        Some(GeoObject::Point(f1)),
                        Some(GeoObject::Point(f2)),
                        Some(GeoObject::Point(p)),
                    ) = (&f1, &f2, &p)
                    {
                        if let Some(GeoObject::Hyperbola(out)) =
                            self.get_object_mut(cons.outputs[0])
                        {
                            let d1 = p.position.distance(&f1.position);
                            let d2 = p.position.distance(&f2.position);
                            let a = (d1 - d2).abs() * 0.5;
                            let c = f1.position.distance(&f2.position) * 0.5;
                            let b = (c * c - a * a).max(0.0).sqrt();
                            out.center = Point2::new(
                                (f1.position.x + f2.position.x) * 0.5,
                                (f1.position.y + f2.position.y) * 0.5,
                            );
                            out.a = a;
                            out.b = b;
                            let axis_angle = (f2.position.y - f1.position.y)
                                .atan2(f2.position.x - f1.position.x);
                            out.angle = axis_angle;
                            // The renderer rotates the horizontal local transverse axis by
                            // `angle`; changing `horizontal` here would rotate it twice.
                            out.horizontal = true;
                        }
                    }
                }
                "ConicByFivePoints" if cons.inputs.len() >= 5 && !cons.outputs.is_empty() => {
                    let mut pts = Vec::with_capacity(5);
                    for &id in &cons.inputs[..5] {
                        if let Some(GeoObject::Point(p)) = self.get_object(id) {
                            pts.push(p.position);
                        }
                    }
                    if pts.len() == 5 {
                        if let Some(obj) = conic_from_five_points(&pts) {
                            let out_id = cons.outputs[0];
                            if let Some(existing) = self.objects.get(&out_id) {
                                let (label, color, visible, width) = match existing {
                                    GeoObject::Ellipse(o) => {
                                        (o.label.clone(), o.color, o.visible, o.width)
                                    }
                                    GeoObject::Hyperbola(o) => {
                                        (o.label.clone(), o.color, o.visible, o.width)
                                    }
                                    _ => (String::new(), Color::BLACK, true, 2.0),
                                };
                                let new_obj = match obj {
                                    GeoObject::Ellipse(mut o) => {
                                        o.id = out_id;
                                        o.label = label;
                                        o.color = color;
                                        o.visible = visible;
                                        o.width = width;
                                        GeoObject::Ellipse(o)
                                    }
                                    GeoObject::Hyperbola(mut o) => {
                                        o.id = out_id;
                                        o.label = label;
                                        o.color = color;
                                        o.visible = visible;
                                        o.width = width;
                                        GeoObject::Hyperbola(o)
                                    }
                                    _ => obj,
                                };
                                self.objects.insert(out_id, new_obj);
                            }
                        }
                    }
                }
                "Locus" => {
                    // La captura ocurre una sola vez tras la estabilización del
                    // documento en `capture_locus_samples`, nunca por pase.
                }
                _ => {}
            }
        }
        self.spatial_dirty = true;
        Ok(())
    }

    pub fn get_object(&self, id: ObjectId) -> Option<&GeoObject> {
        self.objects.get(&id)
    }

    pub fn get_object_mut(&mut self, id: ObjectId) -> Option<&mut GeoObject> {
        self.bump_version();
        self.spatial_dirty = true;
        self.objects.get_mut(&id)
    }

    /// Devuelve una **copia** de los segmentos cacheados de un
    /// `ImplicitCurveObj`. Si el id no corresponde a una implícita, o si el
    /// cache aún no está poblado, devuelve un vector vacío (es seguro y
    /// barato; el render simplemente no dibujará ese `ComplexMapping`).
    pub fn implicit_curve_segments(&self, id: ObjectId) -> ImplicitCurveSegments {
        if let Some(GeoObject::ImplicitCurve(ic)) = self.objects.get(&id) {
            ic.cached_segments
                .read()
                .unwrap_or_else(|p| {
                    log::warn!("cache lock envenenado; recuperando estado parcial");
                    p.into_inner()
                })
                .clone()
        } else {
            Vec::new()
        }
    }

    pub fn objects(&self) -> &HashMap<ObjectId, GeoObject> {
        &self.objects
    }

    pub fn objects_iter(&self) -> impl Iterator<Item = (&ObjectId, &GeoObject)> {
        self.objects.iter()
    }

    /// Returns every exact label match in stable object-ID order. Legacy files
    /// may contain duplicates even though new insertions reject them.
    pub fn object_ids_by_label(&self, label: &str) -> Vec<ObjectId> {
        let label = label.trim();
        let mut matches: Vec<_> = self
            .objects
            .iter()
            .filter_map(|(id, object)| (object.label() == label).then_some(*id))
            .collect();
        matches.sort_unstable();
        matches
    }

    /// Resolves a label only when it identifies at most one object.
    pub fn try_find_object_by_label(&self, label: &str) -> Result<Option<ObjectId>, String> {
        let matches = self.object_ids_by_label(label);
        match matches.as_slice() {
            [] => Ok(None),
            [id] => Ok(Some(*id)),
            _ => Err(format!(
                "Object label '{}' is ambiguous across {} objects",
                label.trim(),
                matches.len()
            )),
        }
    }

    pub fn objects_iter_mut(&mut self) -> impl Iterator<Item = (&ObjectId, &mut GeoObject)> {
        self.spatial_dirty = true;
        self.objects.iter_mut()
    }

    pub fn view(&self) -> &ViewTransform {
        &self.view
    }

    pub fn view_mut(&mut self) -> &mut ViewTransform {
        self.spatial_dirty = true;
        &mut self.view
    }

    /// Actualiza la proyección de pantalla sólo si el canvas cambió realmente.
    /// Devuelve si se invalidó el índice espacial asociado a la vista.
    pub fn set_screen_size(&mut self, width: f32, height: f32) -> bool {
        if self.view.screen_size.x == width && self.view.screen_size.y == height {
            return false;
        }
        self.view.screen_size.x = width;
        self.view.screen_size.y = height;
        self.spatial_dirty = true;
        true
    }

    pub fn set_view(&mut self, view: ViewTransform) {
        self.view = view;
        self.spatial_dirty = true;
    }

    pub fn selection(&self) -> &[ObjectId] {
        &self.selection
    }

    pub fn select(&mut self, id: ObjectId) {
        if !self.selection.contains(&id) {
            self.selection.push(id);
        }
    }

    pub fn deselect(&mut self, id: ObjectId) {
        self.selection.retain(|&s| s != id);
    }

    pub fn clear_selection(&mut self) {
        self.selection.clear();
    }

    pub fn is_selected(&self, id: ObjectId) -> bool {
        self.selection.contains(&id)
    }

    /// Find object near a screen point (in world coordinates).
    pub fn pick_object(&mut self, world: Point2, tolerance: f64) -> Option<ObjectId> {
        let variables_hash = self.spatial_variables_hash();
        if variables_hash != self.spatial_variables_hash {
            self.spatial_dirty = true;
            *self.cached_vars_list.lock().unwrap_or_else(|poisoned| {
                log::warn!("cache lock envenenado; recuperando estado parcial");
                poisoned.into_inner()
            }) = None;
        }
        if self.spatial_dirty {
            self.rebuild_spatial_index();
        }
        let mut candidates = self.spatial.candidates(world.x, world.y, tolerance);
        if candidates.is_empty() {
            candidates.extend(self.objects.keys().copied());
        }

        let mut hits: Vec<(ObjectId, f64)> = Vec::new();
        for id in candidates {
            if let Some(obj) = self.objects.get(&id) {
                if !obj.is_visible() {
                    continue;
                }
                if self.check_hit(obj, world, tolerance) {
                    let dist = match obj {
                        GeoObject::Point(p) => self.resolved_point_position(p).distance(&world),
                        GeoObject::Line(l) => {
                            let start = Point2::new(
                                self.resolve_expr(&l.start_x_expr, l.start.x),
                                self.resolve_expr(&l.start_y_expr, l.start.y),
                            );
                            let end = Point2::new(
                                self.resolve_expr(&l.end_x_expr, l.end.x),
                                self.resolve_expr(&l.end_y_expr, l.end.y),
                            );
                            match l.kind {
                                LineKind::Segment => distance_point_to_segment(world, start, end),
                                LineKind::Ray => {
                                    grafito_geometry::distance_point_to_ray(world, start, end)
                                }
                                LineKind::Line => {
                                    grafito_geometry::distance_point_to_line(world, start, end)
                                }
                            }
                        }
                        GeoObject::Circle(c) => {
                            (c.center.distance(&world) - self.resolved_circle_radius(c)).abs()
                        }
                        _ => tolerance,
                    };
                    hits.push((id, dist));
                }
            }
        }
        hits.sort_by(|a, b| {
            a.1.partial_cmp(&b.1)
                .unwrap_or(std::cmp::Ordering::Equal)
                .then_with(|| a.0.cmp(&b.0))
        });
        hits.first().map(|(id, _)| *id)
    }

    fn spatial_variables_hash(&self) -> u64 {
        let mut variables: Vec<_> = self.variables.iter().collect();
        variables.sort_unstable_by(|left, right| left.0.cmp(right.0));
        let mut hasher = DefaultHasher::new();
        for (name, value) in variables {
            name.hash(&mut hasher);
            value.to_bits().hash(&mut hasher);
        }
        hasher.finish()
    }

    fn resolved_point_position(&self, point: &PointObj) -> Point2 {
        Point2::new(
            self.resolve_expr(&point.x_expr, point.position.x),
            self.resolve_expr(&point.y_expr, point.position.y),
        )
    }

    fn resolved_circle_radius(&self, circle: &crate::CircleObj) -> f64 {
        self.resolve_expr(&circle.radius_expr, circle.radius)
    }

    fn check_hit(&self, obj: &GeoObject, world: Point2, tolerance: f64) -> bool {
        match obj {
            GeoObject::Point(p) => {
                self.resolved_point_position(p).distance(&world)
                    <= tolerance.max(p.size as f64 / self.view.scale.abs())
            }
            GeoObject::Line(l) => {
                let start = Point2::new(
                    self.resolve_expr(&l.start_x_expr, l.start.x),
                    self.resolve_expr(&l.start_y_expr, l.start.y),
                );
                let end = Point2::new(
                    self.resolve_expr(&l.end_x_expr, l.end.x),
                    self.resolve_expr(&l.end_y_expr, l.end.y),
                );
                let tolerance = tolerance.max(l.width as f64 / (2.0 * self.view.scale.abs()));
                match l.kind {
                    LineKind::Segment => {
                        grafito_geometry::distance_point_to_segment(world, start, end) <= tolerance
                    }
                    LineKind::Ray => {
                        grafito_geometry::distance_point_to_ray(world, start, end) <= tolerance
                    }
                    LineKind::Line => {
                        grafito_geometry::distance_point_to_line(world, start, end) <= tolerance
                    }
                }
            }
            GeoObject::Circle(c) => {
                (c.center.distance(&world) - self.resolved_circle_radius(c)).abs() <= tolerance
            }
            GeoObject::Polygon(poly) if poly.vertices.len() >= 3 => {
                let resolved: Vec<Point2> = poly
                    .vertices
                    .iter()
                    .enumerate()
                    .map(|(i, v)| {
                        let x = self.resolve_expr(poly.x_exprs.get(i).unwrap_or(&None), v.x);
                        let y = self.resolve_expr(poly.y_exprs.get(i).unwrap_or(&None), v.y);
                        Point2::new(x, y)
                    })
                    .collect();
                distance_point_to_polygon(world, &resolved) <= tolerance
            }
            GeoObject::Function(f) => {
                let x_min = self.resolve_expr(
                    &f.domain_min_expr,
                    f.domain_min.unwrap_or(f64::NEG_INFINITY),
                );
                let x_max =
                    self.resolve_expr(&f.domain_max_expr, f.domain_max.unwrap_or(f64::INFINITY));
                if world.x < x_min - tolerance || world.x > x_max + tolerance {
                    return false;
                }
                if let (Ok(y0), Ok(y1), Ok(y2)) = (
                    grafito_geometry::expr::evaluate(&f.expr, &[("x".to_string(), world.x)]),
                    grafito_geometry::expr::evaluate(
                        &f.expr,
                        &[("x".to_string(), world.x - tolerance * 2.0)],
                    ),
                    grafito_geometry::expr::evaluate(
                        &f.expr,
                        &[("x".to_string(), world.x + tolerance * 2.0)],
                    ),
                ) {
                    // Check horizontal distance (if within y range of nearby x)
                    let min_y = y1.min(y2).min(y0) - tolerance * 2.0;
                    let max_y = y1.max(y2).max(y0) + tolerance * 2.0;
                    if world.y >= min_y && world.y <= max_y {
                        return true;
                    }
                    // Fallback to strict vertical distance if curve is very flat
                    (world.y - y0).abs() <= tolerance * 2.0
                } else {
                    false
                }
            }
            GeoObject::Ellipse(el) => {
                // Check if point is near the ellipse boundary
                let dx = world.x - el.center.x;
                let dy = world.y - el.center.y;
                let cos_a = el.angle.cos();
                let sin_a = el.angle.sin();
                let rx = dx * cos_a + dy * sin_a;
                let ry = -dx * sin_a + dy * cos_a;
                let ellipse_eq = (rx / el.rx).powi(2) + (ry / el.ry).powi(2);
                (ellipse_eq - 1.0).abs() <= tolerance / el.rx.min(el.ry)
            }
            GeoObject::Parabola(pb) => {
                if !pb.p.is_finite() || pb.p.abs() < 1e-12 {
                    return false;
                }
                // Transform the test point into the parabola's local coordinate system.
                let dx = world.x - pb.vertex.x;
                let dy = world.y - pb.vertex.y;
                let cos_a = pb.angle.cos();
                let sin_a = pb.angle.sin();
                let lx = dx * cos_a + dy * sin_a;
                let ly = -dx * sin_a + dy * cos_a;
                let curve_y = lx * lx / (4.0 * pb.p);
                // Approximate geometric distance by dividing the vertical residual by the
                // derivative magnitude sqrt(1 + (x/(2p))^2).
                let residual = (ly - curve_y).abs();
                let denom = (1.0 + (lx / (2.0 * pb.p)).powi(2)).sqrt();
                residual / denom.max(1.0) <= tolerance
            }
            GeoObject::Hyperbola(hb) => {
                // Transform the test point into the hyperbola's local coordinate system.
                let dx = world.x - hb.center.x;
                let dy = world.y - hb.center.y;
                let cos_a = hb.angle.cos();
                let sin_a = hb.angle.sin();
                let lx = dx * cos_a + dy * sin_a;
                let ly = -dx * sin_a + dy * cos_a;
                let a = hb.a.max(1e-12);
                let b = hb.b.max(1e-12);
                let hyperbola_eq = if hb.horizontal {
                    (lx / a).powi(2) - (ly / b).powi(2)
                } else {
                    (ly / a).powi(2) - (lx / b).powi(2)
                };
                (hyperbola_eq - 1.0).abs() <= tolerance / a.min(b)
            }
            GeoObject::Text(txt) => {
                // Simple bounding box check
                let width = txt.content.len() as f64 * txt.font_size as f64 * 0.6;
                let height = txt.font_size as f64;
                world.x >= txt.position.x
                    && world.x <= txt.position.x + width
                    && world.y >= txt.position.y - height
                    && world.y <= txt.position.y
            }
            GeoObject::ParametricCurve2D(pc) => {
                // Sample the curve and check distance to segments
                let t_min = self.resolve_expr(&pc.t_min_expr, pc.t_min);
                let t_max = self.resolve_expr(&pc.t_max_expr, pc.t_max);
                let steps = 100;
                let dt = (t_max - t_min) / steps as f64;
                let mut prev_point: Option<Point2> = None;
                for i in 0..=steps {
                    let t = t_min + i as f64 * dt;
                    if let (Ok(x), Ok(y)) = (
                        grafito_geometry::expr::evaluate(&pc.expr_x, &[("t".to_string(), t)]),
                        grafito_geometry::expr::evaluate(&pc.expr_y, &[("t".to_string(), t)]),
                    ) {
                        if x.is_finite() && y.is_finite() {
                            let curr_point = Point2::new(x, y);
                            if let Some(prev) = prev_point {
                                if distance_point_to_segment(world, prev, curr_point) <= tolerance {
                                    return true;
                                }
                            }
                            prev_point = Some(curr_point);
                        }
                    }
                }
                false
            }
            GeoObject::PolarCurve(pol) => {
                // Sample the curve and check distance to segments
                let t_min = self.resolve_expr(&pol.t_min_expr, pol.t_min);
                let t_max = self.resolve_expr(&pol.t_max_expr, pol.t_max);
                let steps = 100;
                let dt = (t_max - t_min) / steps as f64;
                let mut prev_point: Option<Point2> = None;
                for i in 0..=steps {
                    let t = t_min + i as f64 * dt;
                    if let Ok(r) =
                        grafito_geometry::expr::evaluate(&pol.expr_r, &[("t".to_string(), t)])
                    {
                        if r.is_finite() {
                            let x = r * t.cos();
                            let y = r * t.sin();
                            let curr_point = Point2::new(x, y);
                            if let Some(prev) = prev_point {
                                if distance_point_to_segment(world, prev, curr_point) <= tolerance {
                                    return true;
                                }
                            }
                            prev_point = Some(curr_point);
                        }
                    }
                }
                false
            }
            GeoObject::ImplicitCurve(ic) => {
                // Evaluate both sides and check if close to the relation
                if let (Ok(lhs), Ok(rhs)) = (
                    grafito_geometry::expr::evaluate(
                        &ic.expr_lhs,
                        &[("x".to_string(), world.x), ("y".to_string(), world.y)],
                    ),
                    grafito_geometry::expr::evaluate(
                        &ic.expr_rhs,
                        &[("x".to_string(), world.x), ("y".to_string(), world.y)],
                    ),
                ) {
                    let diff = (lhs - rhs).abs();
                    match ic.operator {
                        RelationOperator::Eq => diff <= tolerance,
                        RelationOperator::Less => lhs < rhs + tolerance,
                        RelationOperator::Greater => lhs > rhs - tolerance,
                        RelationOperator::LessEq => lhs <= rhs + tolerance,
                        RelationOperator::GreaterEq => lhs >= rhs - tolerance,
                    }
                } else {
                    false
                }
            }
            GeoObject::ScatterPlot(sp) => {
                // Check distance to any point
                for (x, y) in sp.xs.iter().zip(sp.ys.iter()) {
                    if Point2::new(*x, *y).distance(&world) <= tolerance {
                        return true;
                    }
                }
                false
            }
            GeoObject::RegressionLine(rl) => {
                // Check distance to the line y = slope * x + intercept
                let expected_y = rl.slope * world.x + rl.intercept;
                (world.y - expected_y).abs() <= tolerance
            }
            GeoObject::Histogram(h) => {
                // Check if point is inside any bar
                let bins = grafito_geometry::statistics::histogram(&h.data, h.bins);
                let max_count = bins.iter().map(|(_, _, c)| *c).fold(0.0f64, f64::max);
                if max_count <= 0.0 {
                    return false;
                }
                let y_scale = (h.y_max - h.y_min) / max_count;
                for (left, right, count) in &bins {
                    let bar_height = h.y_min + count * y_scale;
                    if world.x >= *left
                        && world.x <= *right
                        && world.y >= h.y_min
                        && world.y <= bar_height
                    {
                        return true;
                    }
                }
                false
            }
            GeoObject::BoxPlot(bp) => {
                // Check if point is inside the box
                if let Some((_, q1, _, q3, _, _)) =
                    grafito_geometry::statistics::boxplot_stats(&bp.data)
                {
                    let half_w = bp.width_box * 0.5;
                    world.x >= bp.position - half_w
                        && world.x <= bp.position + half_w
                        && world.y >= q1
                        && world.y <= q3
                } else {
                    false
                }
            }
            GeoObject::Fractal2D(fr) => {
                // Bounding box check
                world.x >= fr.x_min
                    && world.x <= fr.x_max
                    && world.y >= fr.y_min
                    && world.y <= fr.y_max
            }
            GeoObject::Pencil(pencil) => {
                // Pencil: comprobamos si el punto está cerca de algún
                // segmento de la polilínea. La tolerancia se escala por
                // el grosor del trazo para PencilObj gruesos.
                let eff_tol = tolerance.max(pencil.width as f64 / self.view.scale * 0.5);
                if pencil.points.len() < 2 {
                    return pencil.is_dynamic_locus()
                        && pencil
                            .points
                            .first()
                            .is_some_and(|point| world.distance(point) <= eff_tol);
                }
                for w in pencil.points.windows(2) {
                    let d = grafito_geometry::distance_point_to_segment(world, w[0], w[1]);
                    if d <= eff_tol {
                        return true;
                    }
                }
                false
            }
            // 3D objects and complex objects - use bounding box or return false
            GeoObject::VectorField2D(_)
            | GeoObject::PhasePortrait(_)
            | GeoObject::Transformed(_)
            | GeoObject::ComplexGrid(_)
            | GeoObject::ComplexMapping(_)
            | GeoObject::ComplexIntegral(_) => false,
            _ => false, // 3D objects require projection, skip for now
        }
    }

    pub fn rebuild_spatial_index(&mut self) {
        let mut items = Vec::new();
        let mut unbounded = Vec::new();
        for (id, obj) in &self.objects {
            if !obj.is_visible() {
                continue;
            }
            let (min_x, min_y, max_x, max_y) = match obj {
                GeoObject::Point(p) => {
                    let position = self.resolved_point_position(p);
                    let padding = (p.size as f64 / self.view.scale.abs()).max(0.1);
                    (
                        position.x - padding,
                        position.y - padding,
                        position.x + padding,
                        position.y + padding,
                    )
                }
                GeoObject::Line(l) => {
                    if l.kind != LineKind::Segment {
                        unbounded.push(*id);
                        continue;
                    }
                    let start = Point2::new(
                        self.resolve_expr(&l.start_x_expr, l.start.x),
                        self.resolve_expr(&l.start_y_expr, l.start.y),
                    );
                    let end = Point2::new(
                        self.resolve_expr(&l.end_x_expr, l.end.x),
                        self.resolve_expr(&l.end_y_expr, l.end.y),
                    );
                    let padding = l.width as f64 / (2.0 * self.view.scale.abs());
                    (
                        start.x.min(end.x) - padding,
                        start.y.min(end.y) - padding,
                        start.x.max(end.x) + padding,
                        start.y.max(end.y) + padding,
                    )
                }
                GeoObject::Circle(c) => {
                    let radius = self.resolved_circle_radius(c);
                    (
                        c.center.x - radius,
                        c.center.y - radius,
                        c.center.x + radius,
                        c.center.y + radius,
                    )
                }
                GeoObject::Polygon(poly) => {
                    let mut min_x = f64::MAX;
                    let mut min_y = f64::MAX;
                    let mut max_x = f64::MIN;
                    let mut max_y = f64::MIN;
                    for (i, v) in poly.vertices.iter().enumerate() {
                        let x = self.resolve_expr(poly.x_exprs.get(i).unwrap_or(&None), v.x);
                        let y = self.resolve_expr(poly.y_exprs.get(i).unwrap_or(&None), v.y);
                        min_x = min_x.min(x);
                        min_y = min_y.min(y);
                        max_x = max_x.max(x);
                        max_y = max_y.max(y);
                    }
                    if poly.vertices.is_empty() {
                        continue;
                    }
                    (min_x, min_y, max_x, max_y)
                }
                GeoObject::Function(f) => {
                    let x_min =
                        self.resolve_expr(&f.domain_min_expr, f.domain_min.unwrap_or(-10.0));
                    let x_max = self.resolve_expr(&f.domain_max_expr, f.domain_max.unwrap_or(10.0));
                    // Sample function to estimate y bounds
                    let mut y_min = f64::MAX;
                    let mut y_max = f64::MIN;
                    let steps = 50;
                    let dx = (x_max - x_min) / steps as f64;
                    for i in 0..=steps {
                        let x = x_min + i as f64 * dx;
                        if let Ok(y) =
                            grafito_geometry::expr::evaluate(&f.expr, &[("x".to_string(), x)])
                        {
                            if y.is_finite() {
                                y_min = y_min.min(y);
                                y_max = y_max.max(y);
                            }
                        }
                    }
                    if y_min > y_max {
                        continue;
                    }
                    (x_min, y_min, x_max, y_max)
                }
                GeoObject::Ellipse(el) => {
                    let max_r = el.rx.max(el.ry);
                    (
                        el.center.x - max_r,
                        el.center.y - max_r,
                        el.center.x + max_r,
                        el.center.y + max_r,
                    )
                }
                GeoObject::Parabola(pb) => {
                    let range = 10.0;
                    if !pb.p.is_finite() || pb.p.abs() < 1e-12 {
                        continue;
                    }
                    let cos_a = pb.angle.cos();
                    let sin_a = pb.angle.sin();
                    let mut min_x = f64::INFINITY;
                    let mut min_y = f64::INFINITY;
                    let mut max_x = f64::NEG_INFINITY;
                    let mut max_y = f64::NEG_INFINITY;
                    for index in 0..=32 {
                        let t = -range + 2.0 * range * index as f64 / 32.0;
                        let local_y = t * t / (4.0 * pb.p);
                        let x = pb.vertex.x + t * cos_a - local_y * sin_a;
                        let y = pb.vertex.y + t * sin_a + local_y * cos_a;
                        if x.is_finite() && y.is_finite() {
                            min_x = min_x.min(x);
                            min_y = min_y.min(y);
                            max_x = max_x.max(x);
                            max_y = max_y.max(y);
                        }
                    }
                    if !min_x.is_finite()
                        || !min_y.is_finite()
                        || !max_x.is_finite()
                        || !max_y.is_finite()
                    {
                        continue;
                    }
                    (min_x, min_y, max_x, max_y)
                }
                GeoObject::Hyperbola(hb) => {
                    let range = hb.a.max(hb.b) * 3.0;
                    (
                        hb.center.x - range,
                        hb.center.y - range,
                        hb.center.x + range,
                        hb.center.y + range,
                    )
                }
                GeoObject::Text(txt) => {
                    let width = txt.content.len() as f64 * txt.font_size as f64 * 0.6;
                    let height = txt.font_size as f64;
                    (
                        txt.position.x,
                        txt.position.y - height,
                        txt.position.x + width,
                        txt.position.y,
                    )
                }
                GeoObject::ParametricCurve2D(pc) => {
                    // Sample curve to compute bounding box
                    let t_min = self.resolve_expr(&pc.t_min_expr, pc.t_min);
                    let t_max = self.resolve_expr(&pc.t_max_expr, pc.t_max);
                    let mut min_x = f64::MAX;
                    let mut min_y = f64::MAX;
                    let mut max_x = f64::MIN;
                    let mut max_y = f64::MIN;
                    let steps = 100;
                    let dt = (t_max - t_min) / steps as f64;
                    for i in 0..=steps {
                        let t = t_min + i as f64 * dt;
                        if let (Ok(x), Ok(y)) = (
                            grafito_geometry::expr::evaluate(&pc.expr_x, &[("t".to_string(), t)]),
                            grafito_geometry::expr::evaluate(&pc.expr_y, &[("t".to_string(), t)]),
                        ) {
                            if x.is_finite() && y.is_finite() {
                                min_x = min_x.min(x);
                                min_y = min_y.min(y);
                                max_x = max_x.max(x);
                                max_y = max_y.max(y);
                            }
                        }
                    }
                    if min_x > max_x {
                        continue;
                    }
                    (min_x, min_y, max_x, max_y)
                }
                GeoObject::PolarCurve(pol) => {
                    // Sample curve to compute bounding box
                    let t_min = self.resolve_expr(&pol.t_min_expr, pol.t_min);
                    let t_max = self.resolve_expr(&pol.t_max_expr, pol.t_max);
                    let mut min_x = f64::MAX;
                    let mut min_y = f64::MAX;
                    let mut max_x = f64::MIN;
                    let mut max_y = f64::MIN;
                    let steps = 100;
                    let dt = (t_max - t_min) / steps as f64;
                    for i in 0..=steps {
                        let t = t_min + i as f64 * dt;
                        if let Ok(r) =
                            grafito_geometry::expr::evaluate(&pol.expr_r, &[("t".to_string(), t)])
                        {
                            if r.is_finite() {
                                let x = r * t.cos();
                                let y = r * t.sin();
                                min_x = min_x.min(x);
                                min_y = min_y.min(y);
                                max_x = max_x.max(x);
                                max_y = max_y.max(y);
                            }
                        }
                    }
                    if min_x > max_x {
                        continue;
                    }
                    (min_x, min_y, max_x, max_y)
                }
                GeoObject::ImplicitCurve(_ic) => {
                    // Use view bounds as approximation
                    let view = &self.view;
                    let x_min = -10.0 / view.scale;
                    let x_max = 10.0 / view.scale;
                    let y_min = -10.0 / view.scale;
                    let y_max = 10.0 / view.scale;
                    (x_min, y_min, x_max, y_max)
                }
                GeoObject::ScatterPlot(sp) => {
                    if sp.xs.is_empty() || sp.ys.is_empty() {
                        continue;
                    }
                    let mut min_x = f64::MAX;
                    let mut min_y = f64::MAX;
                    let mut max_x = f64::MIN;
                    let mut max_y = f64::MIN;
                    for (x, y) in sp.xs.iter().zip(sp.ys.iter()) {
                        min_x = min_x.min(*x);
                        min_y = min_y.min(*y);
                        max_x = max_x.max(*x);
                        max_y = max_y.max(*y);
                    }
                    (min_x, min_y, max_x, max_y)
                }
                GeoObject::RegressionLine(rl) => {
                    if rl.xs.is_empty() {
                        continue;
                    }
                    let x_min = rl.xs.iter().cloned().fold(f64::INFINITY, f64::min);
                    let x_max = rl.xs.iter().cloned().fold(f64::NEG_INFINITY, f64::max);
                    let y1 = rl.slope * x_min + rl.intercept;
                    let y2 = rl.slope * x_max + rl.intercept;
                    let y_min = y1.min(y2);
                    let y_max = y1.max(y2);
                    (x_min, y_min, x_max, y_max)
                }
                GeoObject::Histogram(h) => {
                    if h.data.is_empty() {
                        continue;
                    }
                    let x_min = h.data.iter().cloned().fold(f64::INFINITY, f64::min);
                    let x_max = h.data.iter().cloned().fold(f64::NEG_INFINITY, f64::max);
                    (x_min, 0.0, x_max, h.data.len() as f64)
                }
                GeoObject::BoxPlot(bp) => {
                    if bp.data.is_empty() {
                        continue;
                    }
                    let y_min = bp.data.iter().cloned().fold(f64::INFINITY, f64::min);
                    let y_max = bp.data.iter().cloned().fold(f64::NEG_INFINITY, f64::max);
                    let half_w = bp.width_box * 0.5;
                    (bp.position - half_w, y_min, bp.position + half_w, y_max)
                }
                GeoObject::Fractal2D(fr) => (fr.x_min, fr.y_min, fr.x_max, fr.y_max),
                GeoObject::VectorField2D(_vf) => {
                    // Use view bounds as approximation
                    let view = &self.view;
                    let x_min = -10.0 / view.scale;
                    let x_max = 10.0 / view.scale;
                    let y_min = -10.0 / view.scale;
                    let y_max = 10.0 / view.scale;
                    (x_min, y_min, x_max, y_max)
                }
                GeoObject::ComplexGrid(cg) => (cg.x_min, cg.y_min, cg.x_max, cg.y_max),
                GeoObject::ComplexMapping(_) => {
                    // ComplexMapping doesn't have its own bounds, skip
                    continue;
                }
                // Las tablas son fuentes de análisis, no geometría seleccionable.
                GeoObject::DataTable(_) => continue,
                GeoObject::PhasePortrait(pp) => (pp.x_min, pp.y_min, pp.x_max, pp.y_max),
                GeoObject::Transformed(_) => (0.0, 0.0, 0.0, 0.0),
                GeoObject::ComplexIntegral(_) => (0.0, 0.0, 0.0, 0.0),
                GeoObject::Pencil(p) => {
                    if p.points.is_empty() {
                        continue;
                    }
                    let mut min_x = f64::INFINITY;
                    let mut min_y = f64::INFINITY;
                    let mut max_x = f64::NEG_INFINITY;
                    let mut max_y = f64::NEG_INFINITY;
                    for pt in &p.points {
                        if pt.x < min_x {
                            min_x = pt.x;
                        }
                        if pt.x > max_x {
                            max_x = pt.x;
                        }
                        if pt.y < min_y {
                            min_y = pt.y;
                        }
                        if pt.y > max_y {
                            max_y = pt.y;
                        }
                    }
                    (min_x, min_y, max_x, max_y)
                }
                // 3D objects are not indexed in 2D spatial index
                GeoObject::Point3D(_)
                | GeoObject::Segment3D(_)
                | GeoObject::Plane3D(_)
                | GeoObject::Line3D(_)
                | GeoObject::Sphere3D(_)
                | GeoObject::Cube3D(_)
                | GeoObject::Tetrahedron3D(_)
                | GeoObject::Pyramid3D(_)
                | GeoObject::Cone3D(_)
                | GeoObject::Cylinder3D(_)
                | GeoObject::Torus3D(_)
                | GeoObject::MoebiusStrip(_)
                | GeoObject::Surface3D(_)
                | GeoObject::ParametricCurve3D(_)
                | GeoObject::Attractor3D(_)
                | GeoObject::RegularPolychoron4D(_)
                | GeoObject::RegularPolytopeND(_)
                | GeoObject::HyperSurface4D(_)
                | GeoObject::VectorField3D(_) => {
                    continue;
                }
            };
            items.push((*id, min_x, min_y, max_x, max_y));
        }
        self.spatial.rebuild_with_unbounded(items, unbounded);
        self.spatial_dirty = false;
        self.spatial_variables_hash = self.spatial_variables_hash();
    }

    pub fn clear(&mut self) {
        self.bump_version();
        self.objects.clear();
        self.selection.clear();
        self.next_label_number.clear();
        self.variables.clear();
        self.variable_meta.clear();
        self.spreadsheet.clear();
        self.cas_worksheet.clear();
        self.spreadsheet_variables.clear();
        self.spreadsheet_coordinate_points.clear();
        self.spatial = crate::spatial::SpatialIndex::new();
        self.spatial_dirty = true;
        self.constraints = ConstraintGraph::new();
        self.last_solution.clear();
    }

    pub fn resolve_expr(&self, expr: &Option<String>, fallback: f64) -> f64 {
        match expr {
            Some(e) => {
                let vars = {
                    let mut cache = self.cached_vars_list.lock().unwrap_or_else(|p| {
                        log::warn!("cache lock envenenado; recuperando estado parcial");
                        p.into_inner()
                    });
                    if let Some((ver, cached)) = &*cache {
                        if *ver == self.version {
                            cached.clone()
                        } else {
                            let new_vars = std::sync::Arc::new(
                                self.variables
                                    .iter()
                                    .map(|(k, v)| (k.clone(), *v))
                                    .collect::<Vec<_>>(),
                            );
                            *cache = Some((self.version, new_vars.clone()));
                            new_vars
                        }
                    } else {
                        let new_vars = std::sync::Arc::new(
                            self.variables
                                .iter()
                                .map(|(k, v)| (k.clone(), *v))
                                .collect::<Vec<_>>(),
                        );
                        *cache = Some((self.version, new_vars.clone()));
                        new_vars
                    }
                };
                match evaluate_cached(e, &vars) {
                    Ok(v) if v.is_finite() => v,
                    _ => fallback,
                }
            }
            None => fallback,
        }
    }

    fn recompute_bound_parameters_with_changes(&mut self) -> Vec<ObjectId> {
        let vars: Vec<(String, f64)> = self
            .variables
            .iter()
            .map(|(k, v)| (k.clone(), *v))
            .collect();
        let mut changed = Vec::new();
        for obj in self.objects.values_mut() {
            match obj {
                GeoObject::Point(p) => {
                    let previous = p.position;
                    if let Some(expr) = &p.x_expr {
                        if let Ok(x) = evaluate_cached(expr, &vars) {
                            p.position.x = x;
                        }
                    }
                    if let Some(expr) = &p.y_expr {
                        if let Ok(y) = evaluate_cached(expr, &vars) {
                            p.position.y = y;
                        }
                    }
                    if p.position != previous {
                        changed.push(p.id);
                    }
                }
                GeoObject::Circle(c) => {
                    let previous = c.radius;
                    if let Some(expr) = &c.radius_expr {
                        if let Ok(r) = evaluate_cached(expr, &vars) {
                            c.radius = r;
                        }
                    }
                    if c.radius != previous {
                        changed.push(c.id);
                    }
                }
                GeoObject::Function(f) => {
                    f.invalidate_cache();
                }
                _ => {}
            }
        }
        // Bound expressions can move indexed geometry without going through a
        // mutable object accessor, so invalidate the spatial index explicitly.
        self.spatial_dirty = true;
        changed
    }

    pub fn recompute_bound_parameters(&mut self) {
        if let Err(error) = self.try_recompute_bound_parameters() {
            log::warn!("{error}");
        }
    }

    fn try_recompute_bound_parameters(&mut self) -> Result<bool, String> {
        let mut staged = self.detached_clone_for_staging();
        let changed = staged.recompute_bound_parameters_with_changes();
        if changed.is_empty() {
            return Ok(false);
        }
        staged.propagate_changed_roots(&changed)?;
        crate::validation::validate_document(&staged)?;
        staged.version = self.version.wrapping_add(1);
        staged.spatial_dirty = true;
        *self = staged;
        Ok(true)
    }

    fn propagate_changed_roots(&mut self, changed: &[ObjectId]) -> Result<(), String> {
        if changed.is_empty() {
            return Ok(());
        }
        let order = self.propagation_order(changed);
        self.re_evaluate_constraints_in_place(&order)
    }

    fn commit_variable_mutation<F>(&mut self, mutate: F) -> Result<(), String>
    where
        F: FnOnce(&mut Self),
    {
        let mut staged = self.detached_clone_for_staging();
        mutate(&mut staged);
        staged.recompute_spreadsheet_variables()?;
        crate::validation::validate_document(&staged)?;
        staged.version = self.version.wrapping_add(1);
        staged.spatial_dirty = true;
        *self = staged;
        Ok(())
    }

    /// Actualiza una variable y, si mueve geometría ligada, propaga y captura
    /// el estado final de manera atómica.
    pub fn try_set_variable(&mut self, name: String, value: f64) -> Result<(), String> {
        if !value.is_finite() {
            return Err("Variable value must be finite".to_string());
        }
        if self.is_spreadsheet_owned_variable(&name) {
            return Err("Spreadsheet-owned variables must be edited in their cell".to_string());
        }
        self.commit_variable_mutation(move |document| {
            document.variables.insert(name, value);
        })
    }

    /// Reemplaza los metadatos de una variable existente sólo cuando el
    /// documento completo sigue siendo válido. El candidato se valida sobre
    /// una copia aislada, por lo que los rechazos no alteran el estado vivo ni
    /// sus cachés de ejecución.
    pub fn try_replace_variable_meta_with_previous(
        &mut self,
        name: &str,
        candidate: VariableMeta,
    ) -> Result<Option<Self>, String> {
        if self.is_spreadsheet_owned_variable(name) {
            return Err("Spreadsheet-owned variables must be edited in their cell".to_string());
        }
        if !self.variables.contains_key(name) {
            return Ok(None);
        }
        if self.variable_meta.get(name) == Some(&candidate) {
            return Ok(None);
        }

        let mut staged = self.detached_clone_for_staging();
        staged.variable_meta.insert(name.to_string(), candidate);
        crate::validation::validate_document(&staged)?;
        staged.version = self.version.wrapping_add(1);
        staged.spatial_dirty = true;
        Ok(Some(std::mem::replace(self, staged)))
    }

    pub fn set_variable(&mut self, name: String, value: f64) {
        if let Err(error) = self.try_set_variable(name, value) {
            log::warn!("Variable update rejected: {error}");
        }
    }

    /// Configura una variable escalar para animarse de forma local y determinista.
    ///
    /// Si la variable todavía no existe, comienza en cero cuando el intervalo lo
    /// contiene; de lo contrario comienza en el extremo mínimo. La operación se
    /// valida antes de modificar el documento para que un rango inválido no deje
    /// metadatos huérfanos.
    pub fn configure_variable_animation(
        &mut self,
        name: &str,
        min: f64,
        max: f64,
        speed: f64,
        mode: AnimationMode,
    ) -> Result<(), String> {
        let name = name.trim();
        if name.is_empty() {
            return Err("Animation variable name must not be empty".into());
        }
        if self.is_spreadsheet_owned_variable(name) {
            return Err("Spreadsheet-owned variables must be edited in their cell".into());
        }
        if !min.is_finite() || !max.is_finite() || !speed.is_finite() {
            return Err("Animation bounds and speed must be finite".into());
        }
        if min >= max {
            return Err("Animation minimum must be smaller than its maximum".into());
        }

        let span = max - min;
        let fallback_step = (span / 100.0).max(f64::MIN_POSITIVE);
        let current = self
            .variables
            .get(name)
            .copied()
            .filter(|value| value.is_finite())
            .unwrap_or_else(|| if (min..=max).contains(&0.0) { 0.0 } else { min })
            .clamp(min, max);
        let previous = self.variable_meta.get(name).cloned();
        let meta = VariableMeta {
            position: previous
                .as_ref()
                .map(|meta| meta.position)
                .unwrap_or_else(|| Point2::new(0.0, 0.0)),
            min,
            max,
            step: previous
                .as_ref()
                .map(|meta| meta.step)
                .filter(|step| step.is_finite() && *step > 0.0)
                .unwrap_or(fallback_step),
            visible: match previous.as_ref() {
                Some(meta) => meta.visible,
                None => true,
            },
            animating: true,
            animation_speed: speed,
            animation_mode: mode,
        };

        self.commit_variable_mutation(move |document| {
            document.variables.insert(name.to_string(), current);
            document.variable_meta.insert(name.to_string(), meta);
        })
    }

    /// Avanza todas las variables animadas un intervalo de tiempo acotado por el llamador.
    ///
    /// La actualización agrupa todas las variables en una sola revisión e
    /// invalidación de caché, evitando que una escena con varios parámetros se
    /// recalcule una vez por variable y por frame.
    pub fn advance_variable_animations(&mut self, delta_seconds: f64) -> bool {
        if !delta_seconds.is_finite() || delta_seconds <= 0.0 {
            return false;
        }

        let mut changes = Vec::new();
        for (name, meta) in &self.variable_meta {
            if self.is_spreadsheet_owned_variable(name)
                || !meta.animating
                || meta.animation_speed == 0.0
                || !meta.animation_speed.is_finite()
                || !meta.min.is_finite()
                || !meta.max.is_finite()
                || meta.min >= meta.max
            {
                continue;
            }
            let Some(current) = self
                .variables
                .get(name)
                .copied()
                .filter(|value| value.is_finite())
            else {
                continue;
            };
            let travel = meta.animation_speed * delta_seconds;
            if !travel.is_finite() {
                continue;
            }

            let span = meta.max - meta.min;
            let raw = (current.clamp(meta.min, meta.max) - meta.min) + travel;
            let (next_value, next_speed) = match meta.animation_mode {
                AnimationMode::Loop => (meta.min + raw.rem_euclid(span), meta.animation_speed),
                AnimationMode::PingPong => {
                    // Conserva la semántica histórica de los sliders: el frame
                    // que alcanza un límite se fija allí y cambia de dirección.
                    if raw >= span {
                        (meta.max, -meta.animation_speed.abs())
                    } else if raw <= 0.0 {
                        (meta.min, meta.animation_speed.abs())
                    } else {
                        (meta.min + raw, meta.animation_speed)
                    }
                }
            };
            if next_value != current || next_speed != meta.animation_speed {
                changes.push((name.clone(), next_value, next_speed));
            }
        }

        if changes.is_empty() {
            return false;
        }
        let mut staged = self.detached_clone_for_staging();
        for (name, value, speed) in changes {
            staged.variables.insert(name.clone(), value);
            if let Some(meta) = staged.variable_meta.get_mut(&name) {
                meta.animation_speed = speed;
            }
        }
        if let Err(error) = staged.recompute_spreadsheet_variables() {
            log::warn!("Animation update rejected: {error}");
            return false;
        }
        if let Err(error) = crate::validation::validate_document(&staged) {
            log::warn!("Animation update rejected: {error}");
            return false;
        }
        staged.version = self.version.wrapping_add(1);
        staged.spatial_dirty = true;
        *self = staged;
        true
    }

    pub fn remove_variable(&mut self, name: &str) {
        let name = name.to_string();
        if let Err(error) = self.commit_variable_mutation(move |document| {
            document.variables.remove(&name);
            document.variable_meta.remove(&name);
        }) {
            log::warn!("Variable removal rejected: {error}");
        }
    }

    pub fn get_variable(&self, name: &str) -> Option<f64> {
        self.variables.get(name).copied()
    }

    pub fn variables(&self) -> &HashMap<String, f64> {
        &self.variables
    }

    /// Indica si el valor de una variable es derivado de una celda de spreadsheet.
    pub fn is_spreadsheet_owned_variable(&self, name: &str) -> bool {
        let Some((row, column)) = Self::spreadsheet_coordinate_cell_indices(name) else {
            return false;
        };
        self.spreadsheet
            .get(row)
            .and_then(|cells| cells.get(column))
            .is_some_and(|value| !value.trim().is_empty())
    }

    /// Devuelve los metadatos inmutables de una variable, si fueron configurados.
    pub fn variable_meta(&self, name: &str) -> Option<&VariableMeta> {
        self.variable_meta.get(name)
    }

    pub(crate) fn variable_metadata(&self) -> &HashMap<String, VariableMeta> {
        &self.variable_meta
    }

    /// Máximo de celdas CAS retenidas en un documento local.
    pub const MAX_CAS_WORKSHEET_CELLS: usize = 200;
    /// Máximo de bytes para la entrada de una celda CAS persistida.
    pub const MAX_CAS_WORKSHEET_INPUT_BYTES: usize = 4_096;
    /// Máximo de bytes para el resultado de una celda CAS persistida.
    pub const MAX_CAS_WORKSHEET_OUTPUT_BYTES: usize = 8_192;
    /// Máximo agregado de bytes de entrada y resultado en toda la hoja CAS.
    pub const MAX_CAS_WORKSHEET_BYTES: usize = 256_000;

    /// Devuelve las celdas CAS locales en orden de envío.
    pub fn cas_worksheet(&self) -> &[CasWorksheetEntry] {
        &self.cas_worksheet
    }

    /// Verifica si una nueva entrada podría reservar una celda CAS antes de
    /// ejecutar trabajo simbólico potencialmente costoso.
    pub fn validate_cas_worksheet_input(&self, input: &str) -> Result<(), String> {
        self.validate_cas_worksheet_input_text(input)?;
        if self.cas_worksheet.len() >= Self::MAX_CAS_WORKSHEET_CELLS {
            return Err(format!(
                "CAS worksheet contains the maximum {} cells",
                Self::MAX_CAS_WORKSHEET_CELLS
            ));
        }

        let minimum_cell_bytes = input
            .len()
            .checked_add(1)
            .ok_or_else(|| "CAS worksheet cell size overflow".to_string())?;
        if self
            .cas_worksheet_bytes()?
            .checked_add(minimum_cell_bytes)
            .is_none_or(|total| total > Self::MAX_CAS_WORKSHEET_BYTES)
        {
            return Err(format!(
                "CAS worksheet exceeds the {} byte limit",
                Self::MAX_CAS_WORKSHEET_BYTES
            ));
        }
        Ok(())
    }

    /// Añade una celda CAS histórica después de aplicar límites de memoria.
    pub fn try_append_cas_worksheet_cell(
        &mut self,
        input: String,
        output: String,
        status: CasWorksheetStatus,
    ) -> Result<(), String> {
        self.validate_cas_worksheet_input(&input)?;
        self.validate_cas_worksheet_output(&output)?;

        let entry_bytes = input
            .len()
            .checked_add(output.len())
            .ok_or_else(|| "CAS worksheet cell size overflow".to_string())?;
        let total_bytes = self.cas_worksheet_bytes()?;
        if total_bytes
            .checked_add(entry_bytes)
            .is_none_or(|total| total > Self::MAX_CAS_WORKSHEET_BYTES)
        {
            return Err(format!(
                "CAS worksheet exceeds the {} byte limit",
                Self::MAX_CAS_WORKSHEET_BYTES
            ));
        }

        self.cas_worksheet.push(CasWorksheetEntry {
            input,
            output,
            status,
        });
        if let Err(error) = crate::persistence::serialize_document(self) {
            self.cas_worksheet.pop();
            return Err(error.to_string());
        }
        self.bump_version();
        Ok(())
    }

    /// Elimina todas las celdas CAS y devuelve si había contenido persistido.
    pub fn clear_cas_worksheet(&mut self) -> bool {
        if self.cas_worksheet.is_empty() {
            return false;
        }
        self.cas_worksheet.clear();
        self.bump_version();
        true
    }

    pub(crate) fn validate_cas_worksheet(&self) -> Result<(), String> {
        if self.cas_worksheet.len() > Self::MAX_CAS_WORKSHEET_CELLS {
            return Err(format!(
                "CAS worksheet contains {} cells, maximum is {}",
                self.cas_worksheet.len(),
                Self::MAX_CAS_WORKSHEET_CELLS
            ));
        }

        for entry in &self.cas_worksheet {
            self.validate_cas_worksheet_entry(&entry.input, &entry.output)?;
        }
        if self.cas_worksheet_bytes()? > Self::MAX_CAS_WORKSHEET_BYTES {
            return Err(format!(
                "CAS worksheet exceeds the {} byte limit",
                Self::MAX_CAS_WORKSHEET_BYTES
            ));
        }
        Ok(())
    }

    fn validate_cas_worksheet_entry(&self, input: &str, output: &str) -> Result<(), String> {
        self.validate_cas_worksheet_input_text(input)?;
        self.validate_cas_worksheet_output(output)
    }

    fn validate_cas_worksheet_input_text(&self, input: &str) -> Result<(), String> {
        if input.trim().is_empty() {
            return Err("CAS worksheet input cannot be empty".to_string());
        }
        if input.len() > Self::MAX_CAS_WORKSHEET_INPUT_BYTES {
            return Err(format!(
                "CAS worksheet input exceeds the {} byte limit",
                Self::MAX_CAS_WORKSHEET_INPUT_BYTES
            ));
        }
        Ok(())
    }

    fn validate_cas_worksheet_output(&self, output: &str) -> Result<(), String> {
        if output.is_empty() {
            return Err("CAS worksheet output cannot be empty".to_string());
        }
        if output.len() > Self::MAX_CAS_WORKSHEET_OUTPUT_BYTES {
            return Err(format!(
                "CAS worksheet output exceeds the {} byte limit",
                Self::MAX_CAS_WORKSHEET_OUTPUT_BYTES
            ));
        }
        Ok(())
    }

    fn cas_worksheet_bytes(&self) -> Result<usize, String> {
        self.cas_worksheet
            .iter()
            .try_fold(0usize, |total, entry| {
                total
                    .checked_add(entry.input.len())
                    .and_then(|total| total.checked_add(entry.output.len()))
            })
            .ok_or_else(|| "CAS worksheet size overflow".to_string())
    }

    pub fn get_spreadsheet_cell(&self, row: usize, col: usize) -> String {
        if row < self.spreadsheet.len() && col < self.spreadsheet[row].len() {
            self.spreadsheet[row][col].clone()
        } else {
            String::new()
        }
    }

    // A fully populated sheet must fit the 200,000-element raw JSON structural
    // gate, the 1,000,000-node gate, and the 10 MiB serialized document cap.
    pub const MAX_SPREADSHEET_ROWS: usize = 400;
    pub const MAX_SPREADSHEET_COLS: usize = 400;
    pub const MAX_SPREADSHEET_RECOMPUTE_CELLS: usize = 10_000;

    fn spreadsheet_cell_label(row: usize, col: usize) -> String {
        let mut column = col;
        let mut letters = String::new();
        loop {
            letters.push(char::from(b'A' + (column % 26) as u8));
            if column < 26 {
                break;
            }
            column = column / 26 - 1;
        }
        format!("{}{}", letters.chars().rev().collect::<String>(), row + 1)
    }

    fn spreadsheet_expression_references(expression: &str) -> HashSet<String> {
        let bytes = expression.as_bytes();
        let mut references = HashSet::new();
        let mut index = 0;

        while index < bytes.len() {
            let starts_identifier = bytes[index].is_ascii_uppercase()
                && (index == 0
                    || (!bytes[index - 1].is_ascii_alphanumeric() && bytes[index - 1] != b'_'));
            if !starts_identifier {
                index += 1;
                continue;
            }

            let start = index;
            while index < bytes.len() && bytes[index].is_ascii_uppercase() {
                index += 1;
            }
            let letters_end = index;
            while index < bytes.len() && bytes[index].is_ascii_digit() {
                index += 1;
            }
            if letters_end == index
                || (index < bytes.len()
                    && (bytes[index].is_ascii_alphanumeric() || bytes[index] == b'_'))
            {
                continue;
            }

            let candidate = &expression[start..index];
            if Self::spreadsheet_coordinate_cell_indices(candidate).is_some() {
                references.insert(candidate.to_string());
            }
        }

        references
    }

    fn spreadsheet_coordinate_cell_indices(cell: &str) -> Option<(usize, usize)> {
        let letter_count = cell.bytes().take_while(u8::is_ascii_uppercase).count();
        if letter_count == 0 || letter_count == cell.len() {
            return None;
        }
        let (letters, row_text) = cell.split_at(letter_count);
        if row_text.starts_with('0') {
            return None;
        }
        let row = row_text.parse::<usize>().ok()?.checked_sub(1)?;
        if row >= Self::MAX_SPREADSHEET_ROWS {
            return None;
        }
        let mut column = 0usize;
        for letter in letters.bytes() {
            column = column
                .checked_mul(26)?
                .checked_add((letter - b'A' + 1) as usize)?;
        }
        let column = column.checked_sub(1)?;
        if column >= Self::MAX_SPREADSHEET_COLS {
            return None;
        }
        let mut canonical_column = column;
        let mut canonical_letters = String::new();
        loop {
            canonical_letters.push(char::from(b'A' + (canonical_column % 26) as u8));
            if canonical_column < 26 {
                break;
            }
            canonical_column = canonical_column / 26 - 1;
        }
        let canonical = format!(
            "{}{}",
            canonical_letters.chars().rev().collect::<String>(),
            row + 1
        );
        (canonical == cell).then_some((row, column))
    }

    fn is_valid_spreadsheet_coordinate_owner(&self, cell: &str, point_id: ObjectId) -> bool {
        Self::spreadsheet_coordinate_cell_indices(cell).is_some()
            && self.constraints.is_free(&point_id)
            && matches!(
                self.objects.get(&point_id),
                Some(GeoObject::Point(point)) if point.label == cell
            )
    }

    /// Returns the exact point generated for a coordinate cell, dropping stale
    /// ownership left by malformed or legacy persisted data.
    pub fn spreadsheet_coordinate_point(&mut self, cell: &str) -> Option<ObjectId> {
        let point_id = self.spreadsheet_coordinate_points.get(cell).copied()?;
        if self.is_valid_spreadsheet_coordinate_owner(cell, point_id) {
            Some(point_id)
        } else {
            self.spreadsheet_coordinate_points.remove(cell);
            None
        }
    }

    /// Records that `point_id` was generated by the spreadsheet coordinate cell.
    pub fn set_spreadsheet_coordinate_point(&mut self, cell: String, point_id: ObjectId) {
        if self.is_valid_spreadsheet_coordinate_owner(&cell, point_id) {
            self.spreadsheet_coordinate_points
                .retain(|existing_cell, existing_id| {
                    existing_cell == &cell || *existing_id != point_id
                });
            self.spreadsheet_coordinate_points.insert(cell, point_id);
        }
    }

    /// Drops coordinate-cell ownership entries whose points are absent or are
    /// no longer points. This keeps old or manually edited files safe to load.
    pub fn prune_spreadsheet_coordinate_points(&mut self) {
        let mut mappings: Vec<(String, ObjectId)> = self
            .spreadsheet_coordinate_points
            .iter()
            .map(|(cell, point_id)| (cell.clone(), *point_id))
            .collect();
        mappings.sort_unstable_by(|left, right| left.0.cmp(&right.0));
        let mut owned_points = HashSet::new();
        self.spreadsheet_coordinate_points.clear();
        for (cell, point_id) in mappings {
            if self.is_valid_spreadsheet_coordinate_owner(&cell, point_id)
                && owned_points.insert(point_id)
            {
                self.spreadsheet_coordinate_points.insert(cell, point_id);
            }
        }
    }

    /// Reconciles persisted coordinate-owned points from their present cell
    /// sources. Source-less legacy owners remain intact because no cell source
    /// exists to authoritatively replace them.
    pub(crate) fn reconcile_spreadsheet_coordinate_points_from_sources(
        &mut self,
    ) -> Result<(), String> {
        self.prune_spreadsheet_coordinate_points();
        let cells: Vec<String> = self.spreadsheet_coordinate_points.keys().cloned().collect();
        let mut changed_points = Vec::new();
        let mut changed_point_ids = HashSet::new();

        for cell in cells {
            let Some((row, column)) = Self::spreadsheet_coordinate_cell_indices(&cell) else {
                continue;
            };
            let Some(value) = self
                .spreadsheet
                .get(row)
                .and_then(|cells| cells.get(column))
                .filter(|value| !value.trim().is_empty())
                .cloned()
            else {
                continue;
            };
            self.reconcile_spreadsheet_coordinate_point_in_place(
                &cell,
                &value,
                &mut changed_points,
                &mut changed_point_ids,
            )?;
        }

        self.propagate_changed_roots(&changed_points)
    }

    /// Validate persisted spreadsheet ownership before it is allowed to claim a
    /// point as a coordinate-cell generated object.
    pub(crate) fn validate_spreadsheet_coordinate_points(&self) -> Result<(), String> {
        let mut owned_points = HashSet::new();
        for (cell, point_id) in &self.spreadsheet_coordinate_points {
            let Some((row, column)) = Self::spreadsheet_coordinate_cell_indices(cell) else {
                return Err(format!(
                    "Spreadsheet coordinate owner key '{cell}' is invalid"
                ));
            };
            let Some(GeoObject::Point(point)) = self.objects.get(point_id) else {
                return Err(format!(
                    "Spreadsheet coordinate owner '{cell}' is not a point"
                ));
            };
            if point.label != *cell {
                return Err(format!(
                    "Spreadsheet coordinate owner '{cell}' does not match point label '{}',",
                    point.label
                ));
            }
            if !self.constraints.is_free(point_id) {
                return Err(format!(
                    "Spreadsheet coordinate owner '{cell}' must reference a free point"
                ));
            }
            if let Some(source) = self
                .spreadsheet
                .get(row)
                .and_then(|cells| cells.get(column))
                .filter(|source| !source.trim().is_empty())
            {
                let Some(position) = Self::parse_spreadsheet_coordinate(source) else {
                    return Err(format!(
                        "Spreadsheet coordinate owner '{cell}' has no coordinate cell source"
                    ));
                };
                if point.position != position {
                    return Err(format!(
                        "Spreadsheet coordinate owner '{cell}' does not match its cell source"
                    ));
                }
            }
            if !owned_points.insert(*point_id) {
                return Err(format!(
                    "Spreadsheet point {point_id} has multiple owner cells"
                ));
            }
        }
        Ok(())
    }

    pub fn set_spreadsheet_cell(
        &mut self,
        row: usize,
        col: usize,
        value: String,
    ) -> Result<(), String> {
        let clears_coordinate_owner = value.trim().is_empty();
        let edits = [(row, col, value)];
        Self::validate_spreadsheet_cell_edit_batch(&edits)?;
        let label = Self::spreadsheet_cell_label(row, col);
        self.set_spreadsheet_cell_sources_in_place(&edits);
        if clears_coordinate_owner {
            if let Some(point_id) = self.spreadsheet_coordinate_point(&label) {
                let _ = self.remove_object(point_id);
            }
        }
        Ok(())
    }

    /// Returns whether an editor may commit a cell value immediately. Partial
    /// coordinate text remains local while an existing coordinate cell owns a
    /// generated point, matching the spreadsheet's interactive edit contract.
    fn spreadsheet_cell_edit_is_committable(
        &mut self,
        row: usize,
        column: usize,
        value: &str,
    ) -> bool {
        let label = Self::spreadsheet_cell_label(row, column);
        let owns_coordinate_point = self.spreadsheet_coordinate_point(&label).is_some();
        !owns_coordinate_point
            || !value.trim_start().starts_with('(')
            || Self::parse_spreadsheet_coordinate(value).is_some()
    }

    /// Stages sorted spreadsheet source changes on one detached document.
    /// Coordinate-owned points, spreadsheet variables, metadata, and document
    /// validation are reconciled only after every source reflects its final
    /// value, so rejected batches leave `self` entirely unchanged.
    pub fn stage_spreadsheet_cell_edits(
        &self,
        edits: &[(usize, usize, String)],
    ) -> Result<Self, String> {
        Self::validate_spreadsheet_cell_edit_batch(edits)?;
        if edits.is_empty() {
            return Ok(self.detached_clone_for_staging());
        }

        let mut staged = self.detached_clone_for_staging();
        for (row, column, value) in edits {
            if !staged.spreadsheet_cell_edit_is_committable(*row, *column, value) {
                let label = Self::spreadsheet_cell_label(*row, *column);
                return Err(format!(
                    "Celda {label}: el borrador está incompleto o no es válido"
                ));
            }
        }

        staged.set_spreadsheet_cell_sources_in_place(edits);
        let mut changed_points = Vec::new();
        let mut changed_point_ids = HashSet::new();
        for (row, column, value) in edits {
            let label = Self::spreadsheet_cell_label(*row, *column);
            staged.reconcile_spreadsheet_coordinate_point_in_place(
                &label,
                value,
                &mut changed_points,
                &mut changed_point_ids,
            )?;
        }
        for point_id in staged.recompute_spreadsheet_variables_with_bound_changes()? {
            if changed_point_ids.insert(point_id) {
                changed_points.push(point_id);
            }
        }
        staged.propagate_changed_roots(&changed_points)?;
        crate::validation::validate_document(&staged)?;
        staged.version = self.version.wrapping_add(1);
        staged.spatial_dirty = true;
        Ok(staged)
    }

    fn validate_spreadsheet_cell_edit_batch(
        edits: &[(usize, usize, String)],
    ) -> Result<(), String> {
        let maximum_cells = Self::MAX_SPREADSHEET_ROWS * Self::MAX_SPREADSHEET_COLS;
        if edits.len() > maximum_cells {
            return Err(format!(
                "Spreadsheet batch exceeds the maximum of {maximum_cells} cells"
            ));
        }

        let mut previous = None;
        for (row, column, _) in edits {
            if *row >= Self::MAX_SPREADSHEET_ROWS {
                return Err(format!(
                    "row {} exceeds maximum {}",
                    row,
                    Self::MAX_SPREADSHEET_ROWS
                ));
            }
            if *column >= Self::MAX_SPREADSHEET_COLS {
                return Err(format!(
                    "col {} exceeds maximum {}",
                    column,
                    Self::MAX_SPREADSHEET_COLS
                ));
            }
            if previous.is_some_and(|previous| previous >= (*row, *column)) {
                return Err("Spreadsheet batch cells must be strictly sorted and unique".into());
            }
            previous = Some((*row, *column));
        }
        Ok(())
    }

    fn set_spreadsheet_cell_sources_in_place(&mut self, edits: &[(usize, usize, String)]) {
        let max_row = edits
            .last()
            .map(|(row, _, _)| *row)
            .expect("non-empty spreadsheet edit batch");
        if self.spreadsheet.len() <= max_row {
            self.spreadsheet.resize_with(max_row + 1, Vec::new);
        }
        for (row, column, value) in edits {
            let cells = &mut self.spreadsheet[*row];
            if cells.len() <= *column {
                cells.resize(*column + 1, String::new());
            }
            cells[*column] = value.clone();
        }
        self.bump_version();
    }

    fn parse_spreadsheet_coordinate(value: &str) -> Option<Point2> {
        let value = value.trim();
        let value = value
            .strip_prefix('(')
            .and_then(|value| value.strip_suffix(')'))
            .unwrap_or(value);
        let mut coordinates = value.split(',').map(str::trim);
        let x = coordinates.next()?.parse::<f64>().ok()?;
        let y = coordinates.next()?.parse::<f64>().ok()?;
        if coordinates.next().is_some() || !x.is_finite() || !y.is_finite() {
            return None;
        }
        Some(Point2::new(x, y))
    }

    fn reconcile_spreadsheet_coordinate_point_in_place(
        &mut self,
        label: &str,
        cell_value: &str,
        changed_points: &mut Vec<ObjectId>,
        changed_point_ids: &mut HashSet<ObjectId>,
    ) -> Result<(), String> {
        if let Some(position) = Self::parse_spreadsheet_coordinate(cell_value) {
            if let Some(id) = self.spreadsheet_coordinate_point(label) {
                if self.constraints.is_free(&id) {
                    let Some(GeoObject::Point(point)) = self.objects.get_mut(&id) else {
                        return Ok(());
                    };
                    if point.position != position {
                        point.position = position;
                        self.spatial_dirty = true;
                        if changed_point_ids.insert(id) {
                            changed_points.push(id);
                        }
                    }
                }
            } else {
                let id = self
                    .try_add_object(GeoObject::Point(PointObj::new(position).with_label(label)))?;
                self.set_spreadsheet_coordinate_point(label.to_string(), id);
            }
        } else if let Some(id) = self.spreadsheet_coordinate_point(label) {
            let _ = self.remove_object(id);
        }
        Ok(())
    }

    pub fn eval_spreadsheet_cell(&self, row: usize, col: usize) -> Option<f64> {
        if row >= self.spreadsheet.len() || col >= self.spreadsheet[row].len() {
            return None;
        }
        let expr = &self.spreadsheet[row][col];
        if expr.is_empty() {
            return None;
        }
        grafito_geometry::expr::evaluate(
            expr,
            &self
                .variables
                .iter()
                .map(|(k, v)| (k.clone(), *v))
                .collect::<Vec<_>>(),
        )
        .ok()
    }

    /// Recomputes the scalar variables owned by spreadsheet cells. A
    /// topological traversal resolves every acyclic dependency once, while
    /// cycles and invalid formulas remain unresolved without retaining stale
    /// values.
    pub fn recompute_spreadsheet_variables(&mut self) -> Result<(), String> {
        let changed = self.recompute_spreadsheet_variables_with_bound_changes()?;
        self.propagate_changed_roots(&changed)
    }

    fn recompute_spreadsheet_variables_with_bound_changes(
        &mut self,
    ) -> Result<Vec<ObjectId>, String> {
        let mut cells = Vec::new();
        for (row, values) in self.spreadsheet.iter().enumerate() {
            for (col, value) in values.iter().enumerate() {
                if value.trim().is_empty() {
                    continue;
                }
                if cells.len() == Self::MAX_SPREADSHEET_RECOMPUTE_CELLS {
                    for name in self.spreadsheet_variables.drain() {
                        self.variables.remove(&name);
                    }
                    self.variable_meta
                        .retain(|name, _| self.variables.contains_key(name));
                    let _ = self.recompute_bound_parameters_with_changes();
                    self.bump_version();
                    return Err(format!(
                        "Spreadsheet exceeds the {} cell recomputation limit",
                        Self::MAX_SPREADSHEET_RECOMPUTE_CELLS
                    ));
                }
                cells.push((Self::spreadsheet_cell_label(row, col), value.clone()));
            }
        }

        let cell_indices: HashMap<String, usize> = cells
            .iter()
            .enumerate()
            .map(|(index, (label, _))| (label.clone(), index))
            .collect();
        for name in self.spreadsheet_variables.drain() {
            self.variables.remove(&name);
        }
        for (label, _) in &cells {
            self.variables.remove(label);
        }

        let mut dependency_counts = vec![0usize; cells.len()];
        let mut dependents = vec![Vec::new(); cells.len()];
        for (index, (_, expression)) in cells.iter().enumerate() {
            for dependency in Self::spreadsheet_expression_references(expression) {
                if let Some(&dependency_index) = cell_indices.get(&dependency) {
                    dependency_counts[index] += 1;
                    dependents[dependency_index].push(index);
                }
            }
        }

        let mut variables: Vec<(String, f64)> = self
            .variables
            .iter()
            .map(|(name, value)| (name.clone(), *value))
            .collect();
        let mut variable_indices: HashMap<String, usize> = variables
            .iter()
            .enumerate()
            .map(|(index, (name, _))| (name.clone(), index))
            .collect();
        let mut ready: VecDeque<usize> = dependency_counts
            .iter()
            .enumerate()
            .filter_map(|(index, count)| (*count == 0).then_some(index))
            .collect();
        let mut resolved = HashSet::new();
        while let Some(index) = ready.pop_front() {
            let (label, expression) = &cells[index];
            let Ok(value) = grafito_geometry::expr::evaluate(expression, &variables) else {
                continue;
            };
            if !value.is_finite() {
                continue;
            }

            self.variables.insert(label.clone(), value);
            if let Some(&variable_index) = variable_indices.get(label) {
                variables[variable_index].1 = value;
            } else {
                let variable_index = variables.len();
                variables.push((label.clone(), value));
                variable_indices.insert(label.clone(), variable_index);
            }
            resolved.insert(label.clone());
            for &dependent in &dependents[index] {
                dependency_counts[dependent] -= 1;
                if dependency_counts[dependent] == 0 {
                    ready.push_back(dependent);
                }
            }
        }

        self.spreadsheet_variables = resolved;
        self.variable_meta
            .retain(|name, _| self.variables.contains_key(name));
        let changed = self.recompute_bound_parameters_with_changes();
        self.bump_version();
        Ok(changed)
    }

    pub fn spreadsheet_dim(&self) -> (usize, usize) {
        // Count only rows/cols that have actual non-empty content
        let mut max_row = 0_usize;
        let mut max_col = 0_usize;
        for (r, row) in self.spreadsheet.iter().enumerate() {
            for (c, cell) in row.iter().enumerate() {
                if !cell.is_empty() {
                    max_row = max_row.max(r + 1);
                    max_col = max_col.max(c + 1);
                }
            }
        }
        // At least 3×3, plus 1 extra for expansion without rendering cells
        // that the bounded setter must reject.
        (
            max_row
                .max(3)
                .saturating_add(1)
                .min(Self::MAX_SPREADSHEET_ROWS),
            max_col
                .max(3)
                .saturating_add(1)
                .min(Self::MAX_SPREADSHEET_COLS),
        )
    }

    pub fn object_count(&self) -> usize {
        self.objects.len()
    }
}

fn distance_point_to_polygon(p: Point2, vertices: &[Point2]) -> f64 {
    if vertices.len() < 2 {
        return f64::INFINITY;
    }
    let mut min_dist = f64::INFINITY;
    for i in 0..vertices.len() {
        let a = vertices[i];
        let b = vertices[(i + 1) % vertices.len()];
        let d = distance_point_to_segment(p, a, b);
        if d < min_dist {
            min_dist = d;
        }
    }
    min_dist
}

fn project_point_to_line(p: Point2, a: Point2, b: Point2) -> Point2 {
    let dx = b.x - a.x;
    let dy = b.y - a.y;
    let len2 = dx * dx + dy * dy;
    if len2 < 1e-12 {
        return a;
    }
    let t = ((p.x - a.x) * dx + (p.y - a.y) * dy) / len2;
    Point2::new(a.x + t * dx, a.y + t * dy)
}

fn project_point_to_segment(p: Point2, a: Point2, b: Point2) -> Point2 {
    let dx = b.x - a.x;
    let dy = b.y - a.y;
    let len2 = dx * dx + dy * dy;
    if len2 < 1e-12 {
        return a;
    }
    let t = ((p.x - a.x) * dx + (p.y - a.y) * dy) / len2;
    let t = t.clamp(0.0, 1.0);
    Point2::new(a.x + t * dx, a.y + t * dy)
}

fn project_point_to_circle(p: Point2, center: Point2, radius: f64) -> Point2 {
    let dx = p.x - center.x;
    let dy = p.y - center.y;
    let d = (dx * dx + dy * dy).sqrt();
    if d < 1e-12 {
        return Point2::new(center.x + radius, center.y);
    }
    Point2::new(center.x + radius * dx / d, center.y + radius * dy / d)
}

fn project_point_to_polygon_edges(p: Point2, vertices: &[Point2]) -> Point2 {
    if vertices.len() < 2 {
        return p;
    }
    let mut best = vertices[0];
    let mut best_dist = f64::INFINITY;
    for i in 0..vertices.len() {
        let a = vertices[i];
        let b = vertices[(i + 1) % vertices.len()];
        let proj = project_point_to_segment(p, a, b);
        let d = proj.distance(&p);
        if d < best_dist {
            best_dist = d;
            best = proj;
        }
    }
    best
}

fn circle_from_three_points(a: Point2, b: Point2, c: Point2) -> Option<(Point2, f64)> {
    let d = 2.0 * (a.x * (b.y - c.y) + b.x * (c.y - a.y) + c.x * (a.y - b.y));
    if d.abs() < 1e-12 {
        return None;
    }
    let a2 = a.x * a.x + a.y * a.y;
    let b2 = b.x * b.x + b.y * b.y;
    let c2 = c.x * c.x + c.y * c.y;
    let ux = (a2 * (b.y - c.y) + b2 * (c.y - a.y) + c2 * (a.y - b.y)) / d;
    let uy = (a2 * (c.x - b.x) + b2 * (a.x - c.x) + c2 * (b.x - a.x)) / d;
    let center = Point2::new(ux, uy);
    let radius = center.distance(&a);
    Some((center, radius))
}

fn conic_from_five_points(points: &[Point2]) -> Option<GeoObject> {
    // Build the 5x6 homogeneous system for the conic coefficients
    // [A, B, C, D, E, F] such that A*x^2 + B*x*y + C*y^2 + D*x + E*y + F = 0.
    let mut rows = Vec::with_capacity(5);
    for p in points {
        rows.push(vec![p.x * p.x, p.x * p.y, p.y * p.y, p.x, p.y, 1.0]);
    }
    let m = Matrix::from_rows(rows)?;

    // Try fixing each coefficient to 1 and solving the resulting 5x5 system.
    let mut coeffs: Option<[f64; 6]> = None;
    for fixed in 0..6 {
        let mut a_rows = Vec::with_capacity(5);
        let mut b_rows = Vec::with_capacity(5);
        for r in 0..5 {
            let mut a_row = Vec::with_capacity(5);
            let b_val = -m.get(r, fixed);
            for c in 0..6 {
                if c == fixed {
                    continue;
                }
                a_row.push(m.get(r, c));
            }
            a_rows.push(a_row);
            b_rows.push(vec![b_val]);
        }
        let a_mat = Matrix::from_rows(a_rows)?;
        let b_mat = Matrix::from_rows(b_rows)?;
        if let Some(sol) = solve_linear_system(&a_mat, &b_mat) {
            let mut coeffs_local = [0.0; 6];
            let mut idx = 0;
            for (i, coeff) in coeffs_local.iter_mut().enumerate() {
                if i == fixed {
                    *coeff = 1.0;
                } else {
                    *coeff = sol.get(idx, 0);
                    idx += 1;
                }
            }
            // Verify the solution fits the points.
            let max_residual = points
                .iter()
                .zip(m.data.chunks(6))
                .map(|(_p, row)| {
                    let v = row
                        .iter()
                        .zip(coeffs_local.iter())
                        .map(|(a, b)| a * b)
                        .sum::<f64>();
                    v.abs()
                })
                .fold(0.0f64, f64::max);
            if max_residual < 1e-6 {
                coeffs = Some(coeffs_local);
                break;
            }
        }
    }

    let [a, b, c, d, e, f] = coeffs?;

    // Discriminant: B^2 - 4AC.
    let discriminant = b * b - 4.0 * a * c;

    // Center of the conic (valid for ellipse/hyperbola).
    let q = Matrix::from_rows(vec![vec![a, b * 0.5], vec![b * 0.5, c]])?;
    let q_inv = q.inverse()?;
    let center = Point2::new(
        -0.5 * (q_inv.get(0, 0) * d + q_inv.get(0, 1) * e),
        -0.5 * (q_inv.get(1, 0) * d + q_inv.get(1, 1) * e),
    );

    // Evaluate the constant term at the center.
    let f_prime = a * center.x * center.x
        + b * center.x * center.y
        + c * center.y * center.y
        + d * center.x
        + e * center.y
        + f;

    // Eigen-decomposition of Q.
    let trace = a + c;
    let diff = a - c;
    let gap = (diff * diff + b * b).sqrt();
    let lambda1 = 0.5 * (trace + gap);
    let lambda2 = 0.5 * (trace - gap);

    let angle = if gap < 1e-12 {
        // A circular ellipse has an isotropic quadratic form. Its orientation
        // is arbitrary, so choose the canonical zero angle instead of
        // rejecting the otherwise valid fit for lack of an eigenvector.
        0.0
    } else {
        // Eigenvector for lambda1.
        let mut ev_x = lambda1 - c;
        let mut ev_y = b * 0.5;
        let mut ev_norm = (ev_x * ev_x + ev_y * ev_y).sqrt();
        if ev_norm < 1e-12 {
            ev_x = b * 0.5;
            ev_y = lambda1 - a;
            ev_norm = (ev_x * ev_x + ev_y * ev_y).sqrt();
        }
        if ev_norm < 1e-12 {
            return None;
        }
        ev_y.atan2(ev_x)
    };

    if discriminant < -1e-12 {
        // Ellipse.
        let denom1 = -f_prime / lambda1;
        let denom2 = -f_prime / lambda2;
        if denom1 > 1e-12 && denom2 > 1e-12 {
            return Some(GeoObject::Ellipse(EllipseObj {
                id: ObjectId::new(),
                label: String::new(),
                center,
                rx: denom1.sqrt(),
                ry: denom2.sqrt(),
                angle,
                color: Color::BLACK,
                visible: true,
                width: 2.0,
                fill_color: Some(Color::new(0.2, 0.5, 0.9, 0.15)),
            }));
        }
    } else if discriminant > 1e-12 {
        // Hyperbola.
        let denom1 = -f_prime / lambda1;
        let denom2 = -f_prime / lambda2;
        if denom1 * denom2 < -1e-12 {
            // One denominator positive, one negative.
            let (a_axis, b_axis, transverse_is_lambda1) = if denom1 > 0.0 {
                (denom1.sqrt(), (-denom2).sqrt(), true)
            } else {
                (denom2.sqrt(), (-denom1).sqrt(), false)
            };
            // Angle of the transverse axis.
            let transverse_angle = if transverse_is_lambda1 {
                angle
            } else {
                angle + std::f64::consts::FRAC_PI_2
            };
            return Some(GeoObject::Hyperbola(HyperbolaObj {
                id: ObjectId::new(),
                label: String::new(),
                center,
                a: a_axis,
                b: b_axis,
                horizontal: true,
                angle: transverse_angle,
                color: Color::RED,
                visible: true,
                width: 2.0,
            }));
        }
    }

    None
}

fn doc_intersect(obj_a: &GeoObject, obj_b: &GeoObject) -> Vec<Point2> {
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
                _ => vec![],
            }
        }
        (GeoObject::Line(l), GeoObject::Function(f))
        | (GeoObject::Function(f), GeoObject::Line(l)) => {
            let dx = l.end.x - l.start.x;
            if dx.abs() < 1e-12 {
                return grafito_geometry::expr::eval_function(&f.expr, l.start.x)
                    .ok()
                    .filter(|y| y.is_finite())
                    .map(|y| Point2::new(l.start.x, y))
                    .filter(|point| l.kind_contains_t(l.param_at_point(*point)))
                    .into_iter()
                    .collect();
            }

            let slope = (l.end.y - l.start.y) / dx;
            let intercept = l.start.y - slope * l.start.x;
            let x_min = f.domain_min.unwrap_or(-10.0);
            let x_max = f.domain_max.unwrap_or(10.0);
            intersections::function_line(&f.expr, slope, intercept, x_min, x_max)
                .into_iter()
                .filter(|p| l.kind_contains_t(l.param_at_point(*p)))
                .collect()
        }
        _ => vec![],
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::CircleObj;
    use crate::FunctionObj;
    use crate::{LineKind, LineObj};

    #[test]
    fn new_document_is_empty() {
        let doc = Document::new();
        assert_eq!(doc.object_count(), 0);
        assert!(doc.objects_iter().next().is_none());
        assert_eq!(doc.constraints.constraint_count(), 0);
    }

    #[test]
    fn screen_size_updates_mark_spatial_state_only_when_dimensions_change() {
        let mut document = Document::new();
        document.spatial_dirty = false;
        let initial_size = document.view().screen_size;

        assert!(!document.set_screen_size(initial_size.x, initial_size.y));
        assert!(!document.spatial_dirty);

        assert!(document.set_screen_size(initial_size.x + 1.0, initial_size.y));
        assert!(document.spatial_dirty);

        document.spatial_dirty = false;
        assert!(!document
            .set_screen_size(document.view().screen_size.x, document.view().screen_size.y,));
        assert!(!document.spatial_dirty);
    }

    #[test]
    fn add_point_stores_object() {
        let mut doc = Document::new();
        let id = doc.add_point(Point2::new(1.5, -2.0));
        assert_eq!(doc.object_count(), 1);
        assert!(doc.get_object(id).is_some());
        let pos = doc.point_position(id).unwrap();
        assert!((pos.x - 1.5).abs() < 1e-12);
        assert!((pos.y + 2.0).abs() < 1e-12);
        // A user-created point is registered as free.
        assert!(doc.constraints.is_free(&id));
    }

    #[test]
    fn spreadsheet_dimensions_do_not_offer_cells_beyond_the_persisted_limit() {
        let mut doc = Document::new();
        doc.set_spreadsheet_cell(
            Document::MAX_SPREADSHEET_ROWS - 1,
            Document::MAX_SPREADSHEET_COLS - 1,
            "1".to_string(),
        )
        .expect("boundary cell is valid");

        assert_eq!(
            doc.spreadsheet_dim(),
            (
                Document::MAX_SPREADSHEET_ROWS,
                Document::MAX_SPREADSHEET_COLS
            )
        );
    }

    #[test]
    fn constructed_intersection_evaluates_vertical_lines_at_their_x_coordinate() {
        let line = GeoObject::Line(LineObj::new_with_kind(
            Point2::new(2.0, 1.0),
            Point2::new(2.0, 3.0),
            LineKind::Segment,
        ));
        let function = GeoObject::Function(FunctionObj::new("x".to_string()));

        assert_eq!(doc_intersect(&line, &function), vec![Point2::new(2.0, 2.0)]);
    }

    #[test]
    fn remove_object_by_id_drops_it() {
        let mut doc = Document::new();
        let id = doc.add_point(Point2::new(0.0, 0.0));
        assert_eq!(doc.object_count(), 1);
        let removed = doc.remove_object(id);
        assert!(removed.is_some());
        assert!(doc.get_object(id).is_none());
        assert_eq!(doc.object_count(), 0);
        // Removing a non-existent id returns None and is a no-op.
        assert!(doc.remove_object(id).is_none());
    }

    #[test]
    fn set_variable_recomputes_bound_point() {
        let mut doc = Document::new();
        // Create a point whose coordinates are bound to document variables.
        let mut p = PointObj::new(Point2::new(0.0, 0.0));
        p.x_expr = Some("a".to_string());
        p.y_expr = Some("b * 2".to_string());
        let id = doc.add_object(GeoObject::Point(p));

        // Before setting variables, the fallback (0.0) is used.
        let pos0 = doc.point_position(id).unwrap();
        assert!((pos0.x).abs() < 1e-9 && (pos0.y).abs() < 1e-9);

        doc.set_variable("a".to_string(), 3.0);
        doc.set_variable("b".to_string(), 5.0);

        let pos = doc.point_position(id).unwrap();
        assert!((pos.x - 3.0).abs() < 1e-9, "x should be a=3, got {}", pos.x);
        assert!(
            (pos.y - 10.0).abs() < 1e-9,
            "y should be b*2=10, got {}",
            pos.y
        );
        assert_eq!(doc.get_variable("a"), Some(3.0));
    }

    #[test]
    fn json_roundtrip_preserves_objects_and_variables() {
        let mut doc = Document::new();
        let a = doc.add_point(Point2::new(0.0, 0.0));
        let b = doc.add_point(Point2::new(4.0, 0.0));
        let circle = doc.add_object(GeoObject::Circle(CircleObj::new(
            Point2::new(2.0, 1.0),
            1.5,
        )));
        doc.set_variable("r".to_string(), 1.5);
        let (_mid, _) = doc.add_constructed_object(
            GeoObject::Point(PointObj::new(Point2::new(0.0, 0.0)).with_label("M")),
            "Midpoint",
            &[a, b],
        );
        let n_before = doc.object_count();
        let c_before = doc.constraints.constraint_count();

        let json = serde_json::to_string(&doc).expect("serialize");
        let doc2: Document = serde_json::from_str(&json).expect("deserialize");

        assert_eq!(doc2.object_count(), n_before);
        assert_eq!(doc2.constraints.constraint_count(), c_before);
        assert!(doc2.get_object(a).is_some());
        assert!(doc2.get_object(b).is_some());
        assert!(doc2.get_object(circle).is_some());
        if let GeoObject::Circle(c) = doc2.get_object(circle).unwrap() {
            assert!((c.radius - 1.5).abs() < 1e-9);
        } else {
            panic!("expected circle after roundtrip");
        }
        // Variables survive the round-trip.
        assert_eq!(doc2.get_variable("r"), Some(1.5));
    }

    #[test]
    fn old_function_json_without_integral_fields_deserializes() {
        let mut doc = Document::new();
        doc.add_object(GeoObject::Function(FunctionObj::new("x^2")));
        let mut value = serde_json::to_value(&doc).expect("serialize value");

        fn strip_integral_fields(value: &mut serde_json::Value) {
            match value {
                serde_json::Value::Object(map) => {
                    map.remove("is_integral");
                    map.remove("integral_var");
                    map.remove("integral_lower");
                    for child in map.values_mut() {
                        strip_integral_fields(child);
                    }
                }
                serde_json::Value::Array(items) => {
                    for child in items {
                        strip_integral_fields(child);
                    }
                }
                _ => {}
            }
        }

        strip_integral_fields(&mut value);
        let loaded: Document = serde_json::from_value(value).expect("old document should load");
        let function = loaded.objects_iter().find_map(|(_, obj)| match obj {
            GeoObject::Function(f) => Some(f),
            _ => None,
        });
        let function = function.expect("function should exist");
        assert!(!function.is_integral);
        assert_eq!(function.integral_var, "x");
        assert_eq!(function.integral_lower, 0.0);
    }

    #[test]
    fn re_evaluate_constraints_midpoint() {
        let mut doc = Document::new();
        let a = doc.add_point(Point2::new(0.0, 0.0));
        let b = doc.add_point(Point2::new(4.0, 0.0));
        let (mid, _) = doc.add_constructed_object(
            GeoObject::Point(PointObj::new(Point2::new(0.0, 0.0)).with_label("M")),
            "Midpoint",
            &[a, b],
        );

        // Initial evaluation: midpoint of (0,0) and (4,0) is (2,0).
        let order = doc.propagation_order(&[a, b]);
        doc.re_evaluate_constraints(&order);
        if let GeoObject::Point(m) = doc.get_object(mid).unwrap() {
            assert!((m.position.x - 2.0).abs() < 1e-9);
            assert!(m.position.y.abs() < 1e-9);
        } else {
            panic!("expected midpoint point");
        }

        // Move a free point and re-evaluate; midpoint must follow.
        doc.move_point(a, Point2::new(2.0, 0.0));
        let order = doc.propagation_order(&[a]);
        doc.re_evaluate_constraints(&order);
        if let GeoObject::Point(m) = doc.get_object(mid).unwrap() {
            assert!((m.position.x - 3.0).abs() < 1e-9, "midpoint x after move");
            assert!(m.position.y.abs() < 1e-9);
        } else {
            panic!("expected midpoint point after move");
        }
    }
}
