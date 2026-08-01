//! Vista previa y aplicación atómica de las operaciones allowlisted del asistente.

use crate::assistant_context::document_context;
use grafito_assistant_types::{
    AssistantOperation, AssistantPlanReceipt, AssistantPlanReceiptDelta,
    AssistantPlanReceiptDigestAlgorithm, AssistantPlanReceiptState, PlanBasis, ProposedPlan,
    ASSISTANT_PLAN_RECEIPT_POLICY_VERSION, ASSISTANT_PLAN_RECEIPT_SCHEMA_VERSION,
};
use grafito_core::{
    deserialize_document, serialize_document, ChangeSet, Document, FunctionObj, GeoObject,
    OperationBatch, CURRENT_DOCUMENT_SCHEMA_VERSION,
};
use grafito_geometry::expr::evaluate;
use sha2::{Digest, Sha256};
use std::collections::{BTreeMap, BTreeSet};

const MAX_GRAPH_EXPRESSION_BYTES: usize = 4_096;
const MAX_GRAPH_DOMAIN_ABS: f64 = 10_000.0;

/// Vista previa textual de una propuesta sin mutar el documento.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PlanPreview {
    /// Diferencias legibles que se aplicarán si la base sigue vigente.
    pub changes: Vec<String>,
}

/// Resultado de una aplicación atómica de propuesta.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PlanApplyResult {
    /// Diferencias que se validaron y aplicaron.
    pub changes: Vec<String>,
    /// Nueva revisión del documento.
    pub revision: u64,
    /// Nueva huella del contexto mínimo del documento.
    pub digest: String,
    /// Evidencia hash-only del staging aplicado.
    pub receipt: AssistantPlanReceipt,
}

/// Resultado no mutante de volver a verificar un receipt local.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PlanReplayResult {
    /// Diferencias que se volverían a aplicar tras la verificación.
    pub changes: Vec<String>,
    /// Revisión staged verificada por el receipt.
    pub staged_revision: u64,
}

/// Plan allowlisted que ya superó staging local y espera una aplicación explícita.
///
/// El `ChangeSet` y el documento staged nunca se serializan en el receipt.
pub struct StagedPlan {
    preview: PlanPreview,
    basis: PlanBasis,
    change_set: ChangeSet,
    receipt: AssistantPlanReceipt,
}

impl StagedPlan {
    /// Vista previa derivada del staging exitoso.
    pub fn preview(&self) -> &PlanPreview {
        &self.preview
    }

    /// Receipt hash-only que un cliente puede guardar localmente de forma explícita.
    pub fn receipt(&self) -> &AssistantPlanReceipt {
        &self.receipt
    }
}

/// Valida una propuesta contra su revisión/huella y describe su efecto sin mutar.
pub fn preview_plan(document: &Document, plan: &ProposedPlan) -> Result<PlanPreview, String> {
    Ok(stage_plan(document, plan)?.preview)
}

/// Revalida y aplica la propuesta dentro de un `OperationBatch` atómico.
///
/// Nunca llama a `process_input`; las únicas mutaciones posibles son
/// `SetVariable` y `CreateGraph` después de validación independiente.
pub fn apply_plan(document: &mut Document, plan: &ProposedPlan) -> Result<PlanApplyResult, String> {
    let staged = stage_plan(document, plan)?;
    apply_staged_plan(document, staged)
}

/// Stagea un plan completo sobre una copia aislada del documento.
///
/// Las operaciones se validan y ejecutan secuencialmente: una variable creada
/// por una operación puede ser usada por una gráfica posterior de la misma propuesta.
pub fn stage_plan(document: &Document, plan: &ProposedPlan) -> Result<StagedPlan, String> {
    stage_plan_with_basis(document, plan, true)
}

fn stage_plan_for_replay(document: &Document, plan: &ProposedPlan) -> Result<StagedPlan, String> {
    stage_plan_with_basis(document, plan, false)
}

fn stage_plan_with_basis(
    document: &Document,
    plan: &ProposedPlan,
    require_current_basis: bool,
) -> Result<StagedPlan, String> {
    if require_current_basis {
        validate_basis(document, &plan.basis)?;
    }
    validate_plan_structure(plan)?;
    validate_spreadsheet_variable_ownership(&persistence_canonical_document(document)?, plan)?;
    let mut staged_document = document.detached_clone_for_staging();
    let change_set = staged_document.commit(plan_batch(plan))?;
    if change_set.before.version == change_set.after.version {
        return Err("assistant plan has no semantic delta to stage".into());
    }

    let preview = PlanPreview {
        changes: plan.operations.iter().map(describe_operation).collect(),
    };
    let receipt = build_receipt(plan, &change_set)?;
    Ok(StagedPlan {
        preview,
        basis: plan.basis.clone(),
        change_set,
        receipt,
    })
}

/// Aplica exactamente el estado de un staging previo tras comprobar que su base sigue vigente.
pub fn apply_staged_plan(
    document: &mut Document,
    staged: StagedPlan,
) -> Result<PlanApplyResult, String> {
    let StagedPlan {
        preview,
        basis,
        change_set,
        receipt,
    } = staged;
    validate_basis(document, &basis)?;
    change_set.redo(document)?;
    let context = document_context(document);
    Ok(PlanApplyResult {
        changes: preview.changes,
        revision: context.revision,
        digest: context.digest,
        receipt,
    })
}

/// Reejecuta el staging sobre una copia y compara base, delta y evidencia sin mutar el documento.
pub fn replay_plan(
    document: &Document,
    plan: &ProposedPlan,
    receipt: &AssistantPlanReceipt,
) -> Result<PlanReplayResult, String> {
    receipt.validate()?;
    if evidence_commitment(receipt)? != receipt.evidence_commitment {
        return Err("assistant receipt evidence does not match its contents".into());
    }
    let persisted = persistence_canonical_document(document)?;
    if !same_persisted_receipt_state(
        &receipt_state(&persisted, document.version, &BTreeMap::new())?,
        &receipt.base,
    ) {
        return Err("assistant receipt base does not match the current document".into());
    }
    if plan_commitment(plan)? != receipt.plan_commitment {
        return Err("assistant receipt plan commitment does not match".into());
    }
    let staged = stage_plan_for_replay(document, plan)?;
    let expected = staged.receipt();
    if receipt.delta != expected.delta {
        return Err("assistant receipt delta does not match the staged result".into());
    }
    if !same_persisted_receipt_state(&receipt.staged, &expected.staged) {
        return Err("assistant receipt staged state does not match".into());
    }
    let staged_revision = expected.staged.revision;
    Ok(PlanReplayResult {
        changes: staged.preview.changes,
        staged_revision,
    })
}

fn validate_plan_structure(plan: &ProposedPlan) -> Result<(), String> {
    if plan.schema_version != grafito_assistant_types::ASSISTANT_SCHEMA_VERSION {
        return Err("assistant plan schema version is unsupported".into());
    }
    plan.validate()
}

fn validate_spreadsheet_variable_ownership(
    document: &Document,
    plan: &ProposedPlan,
) -> Result<(), String> {
    for operation in &plan.operations {
        if let AssistantOperation::SetVariable { name, .. } = operation {
            if document.is_spreadsheet_owned_variable(name) {
                return Err("assistant plan cannot overwrite a spreadsheet-owned variable".into());
            }
        }
    }
    Ok(())
}

fn plan_batch(plan: &ProposedPlan) -> OperationBatch {
    let mut batch = OperationBatch::new();
    for operation in plan.operations.clone() {
        batch.push(move |staged| {
            validate_operation(staged, &operation)?;
            apply_operation(staged, operation)
        });
    }
    batch
}

fn validate_basis(document: &Document, basis: &PlanBasis) -> Result<(), String> {
    let current = document_context(document);
    if basis.revision != current.revision || basis.digest != current.digest {
        return Err("assistant plan is stale; review a fresh preview before applying".into());
    }
    Ok(())
}

fn validate_operation(document: &Document, operation: &AssistantOperation) -> Result<(), String> {
    match operation {
        AssistantOperation::SetVariable { name, value } => {
            if !is_variable_name(name) {
                return Err("assistant variable name is not allowed".into());
            }
            if !value.is_finite() {
                return Err("assistant variable value must be finite".into());
            }
            Ok(())
        }
        AssistantOperation::CreateGraph {
            expression,
            variable,
            domain_min,
            domain_max,
        } => {
            if variable != "x" {
                return Err("assistant graph variable must be x".into());
            }
            if expression.trim().is_empty() || expression.len() > MAX_GRAPH_EXPRESSION_BYTES {
                return Err("assistant graph expression is outside the allowed size".into());
            }
            if contains_forbidden_graph_token(expression) {
                return Err("assistant graph expression contains a forbidden command token".into());
            }
            if !domain_min.is_finite()
                || !domain_max.is_finite()
                || domain_min >= domain_max
                || domain_min.abs() > MAX_GRAPH_DOMAIN_ABS
                || domain_max.abs() > MAX_GRAPH_DOMAIN_ABS
            {
                return Err("assistant graph domain is invalid".into());
            }
            validate_graph_expression(document, expression)?;
            Ok(())
        }
    }
}

fn apply_operation(document: &mut Document, operation: AssistantOperation) -> Result<(), String> {
    match operation {
        AssistantOperation::SetVariable { name, value } => document.try_set_variable(name, value),
        AssistantOperation::CreateGraph {
            expression,
            variable: _,
            domain_min,
            domain_max,
        } => {
            let mut graph = FunctionObj::new(expression);
            graph.domain_min = Some(domain_min);
            graph.domain_max = Some(domain_max);
            document.try_add_object(GeoObject::Function(graph))?;
            Ok(())
        }
    }
}

fn describe_operation(operation: &AssistantOperation) -> String {
    match operation {
        AssistantOperation::SetVariable { name, value } => format!("Set variable {name} = {value}"),
        AssistantOperation::CreateGraph {
            expression,
            domain_min,
            domain_max,
            ..
        } => format!("Create graph y = {expression} for x in [{domain_min}, {domain_max}]"),
    }
}

fn build_receipt(
    plan: &ProposedPlan,
    change_set: &ChangeSet,
) -> Result<AssistantPlanReceipt, String> {
    let persisted_before = persistence_canonical_document(&change_set.before)?;
    let persisted_after = persistence_canonical_document(&change_set.after)?;
    let staged_object_ids = staged_object_id_map(&persisted_before, &persisted_after)?;
    require_persistence_equivalence(
        &change_set.before,
        &persisted_before,
        &BTreeMap::new(),
        "assistant plan requires a persistence-normalized document state",
    )?;
    require_persistence_equivalence(
        &change_set.after,
        &persisted_after,
        &staged_object_ids,
        "assistant plan would change state during persistence normalization",
    )?;
    let base = receipt_state(
        &persisted_before,
        change_set.before.version,
        &BTreeMap::new(),
    )?;
    let staged = receipt_state(
        &persisted_after,
        change_set.after.version,
        &staged_object_ids,
    )?;
    let delta = receipt_delta(plan, &persisted_before, &persisted_after)?;
    let mut receipt = AssistantPlanReceipt {
        schema_version: ASSISTANT_PLAN_RECEIPT_SCHEMA_VERSION,
        policy_version: ASSISTANT_PLAN_RECEIPT_POLICY_VERSION,
        digest_algorithm: AssistantPlanReceiptDigestAlgorithm::Sha256,
        plan_commitment: plan_commitment(plan)?,
        base,
        staged,
        delta,
        evidence_commitment: String::new(),
    };
    receipt.evidence_commitment = evidence_commitment(&receipt)?;
    receipt.validate()?;
    Ok(receipt)
}

fn receipt_state(
    document: &Document,
    revision: u64,
    staged_object_ids: &BTreeMap<String, String>,
) -> Result<AssistantPlanReceiptState, String> {
    let semantic_commitment = semantic_document_commitment(document, staged_object_ids)?;
    Ok(AssistantPlanReceiptState {
        document_schema_version: CURRENT_DOCUMENT_SCHEMA_VERSION,
        revision,
        context_digest: format!("sha256:{semantic_commitment}"),
        semantic_commitment,
    })
}

fn same_persisted_receipt_state(
    left: &AssistantPlanReceiptState,
    right: &AssistantPlanReceiptState,
) -> bool {
    left.document_schema_version == right.document_schema_version
        && left.context_digest == right.context_digest
        && left.semantic_commitment == right.semantic_commitment
}

fn receipt_delta(
    plan: &ProposedPlan,
    before: &Document,
    after: &Document,
) -> Result<AssistantPlanReceiptDelta, String> {
    let operation_count = receipt_count(plan.operations.len())?;
    let set_variable_count = receipt_count(
        plan.operations
            .iter()
            .filter(|operation| matches!(operation, AssistantOperation::SetVariable { .. }))
            .count(),
    )?;
    let create_graph_count = receipt_count(
        plan.operations
            .iter()
            .filter(|operation| matches!(operation, AssistantOperation::CreateGraph { .. }))
            .count(),
    )?;
    let created_object_count = receipt_count(
        after
            .object_count()
            .checked_sub(before.object_count())
            .ok_or_else(|| "assistant plan unexpectedly removed objects".to_string())?,
    )?;
    let variable_names = before
        .variables()
        .keys()
        .chain(after.variables().keys())
        .collect::<BTreeSet<_>>();
    let changed_variable_count = receipt_count(
        variable_names
            .iter()
            .filter(|name| before.get_variable(name) != after.get_variable(name))
            .count(),
    )?;
    Ok(AssistantPlanReceiptDelta {
        operation_count,
        set_variable_count,
        create_graph_count,
        created_object_count,
        changed_variable_count,
    })
}

fn receipt_count(value: usize) -> Result<u8, String> {
    u8::try_from(value).map_err(|_| "assistant receipt count exceeds its byte limit".into())
}

fn plan_commitment(plan: &ProposedPlan) -> Result<String, String> {
    sha256_value(serde_json::to_value(plan).map_err(|error| error.to_string())?)
}

fn require_persistence_equivalence(
    raw: &Document,
    persisted: &Document,
    staged_object_ids: &BTreeMap<String, String>,
    error: &str,
) -> Result<(), String> {
    if semantic_document_commitment(raw, staged_object_ids)?
        != semantic_document_commitment(persisted, staged_object_ids)?
    {
        return Err(error.into());
    }
    Ok(())
}

fn semantic_document_commitment(
    document: &Document,
    staged_object_ids: &BTreeMap<String, String>,
) -> Result<String, String> {
    let mut value = serde_json::to_value(document).map_err(|error| error.to_string())?;
    normalize_staged_object_ids(&mut value, staged_object_ids);
    normalize_receipt_unordered_fields(&mut value);
    if let serde_json::Value::Object(fields) = &mut value {
        if let Some(serde_json::Value::Object(objects)) = fields.remove("objects") {
            // Object IDs are allocated during staging. Commit object contents in a
            // stable order so replay can verify an equivalent newly-created graph.
            let mut objects = objects
                .into_values()
                .map(canonicalize_json)
                .collect::<Vec<_>>();
            objects.sort_unstable_by_key(canonical_json_sort_key);
            fields.insert("objects".into(), serde_json::Value::Array(objects));
        }
    }
    sha256_value(value)
}

fn persistence_canonical_document(document: &Document) -> Result<Document, String> {
    let serialized = serialize_document(document).map_err(|error| error.to_string())?;
    deserialize_document(&serialized).map_err(|error| error.to_string())
}

fn normalize_receipt_unordered_fields(value: &mut serde_json::Value) {
    let serde_json::Value::Object(document) = value else {
        return;
    };
    sort_receipt_array_field(document, "spreadsheet_variables");
    if let Some(serde_json::Value::Object(constraints)) = document.get_mut("constraints") {
        // ConstraintGraph rebuilds these indexes and allocators on deserialize;
        // they are not durable semantic state for receipt replay.
        constraints.remove("dependents");
        constraints.remove("creator");
        constraints.remove("next_id");
        constraints.remove("next_order");
        sort_receipt_array_field(constraints, "free_objects");
    }
}

fn sort_receipt_array_field(fields: &mut serde_json::Map<String, serde_json::Value>, key: &str) {
    let Some(serde_json::Value::Array(values)) = fields.get_mut(key) else {
        return;
    };
    values.sort_unstable_by_key(canonical_json_sort_key);
}

fn staged_object_id_map(
    before: &Document,
    after: &Document,
) -> Result<BTreeMap<String, String>, String> {
    let before = serde_json::to_value(before).map_err(|error| error.to_string())?;
    let after = serde_json::to_value(after).map_err(|error| error.to_string())?;
    let before_ids = document_objects(&before)
        .map(|objects| objects.keys().cloned().collect::<BTreeSet<_>>())
        .unwrap_or_default();
    let mut staged = document_objects(&after)
        .into_iter()
        .flat_map(|objects| objects.iter())
        .filter(|(id, _)| !before_ids.contains(*id))
        .map(|(id, object)| {
            let mut object = object.clone();
            remove_object_identity_fields(&mut object);
            (
                id.clone(),
                canonical_json_sort_key(&canonicalize_json(object)),
            )
        })
        .collect::<Vec<_>>();
    staged.sort_unstable_by(|left, right| left.1.cmp(&right.1).then_with(|| left.0.cmp(&right.0)));
    Ok(staged
        .into_iter()
        .enumerate()
        .map(|(index, (id, _))| (id, format!("assistant-staged-object:{index}")))
        .collect())
}

fn document_objects(
    value: &serde_json::Value,
) -> Option<&serde_json::Map<String, serde_json::Value>> {
    value.as_object()?.get("objects")?.as_object()
}

fn remove_object_identity_fields(value: &mut serde_json::Value) {
    match value {
        serde_json::Value::Array(values) => {
            for value in values {
                remove_object_identity_fields(value);
            }
        }
        serde_json::Value::Object(fields) => {
            fields.remove("id");
            for value in fields.values_mut() {
                remove_object_identity_fields(value);
            }
        }
        _ => {}
    }
}

fn normalize_staged_object_ids(
    value: &mut serde_json::Value,
    staged_object_ids: &BTreeMap<String, String>,
) {
    if staged_object_ids.is_empty() {
        return;
    }
    let serde_json::Value::Object(document) = value else {
        return;
    };
    if let Some(serde_json::Value::Object(objects)) = document.get_mut("objects") {
        let entries = std::mem::take(objects).into_iter().collect::<Vec<_>>();
        for (id, mut object) in entries {
            normalize_object_identity_fields(&mut object, staged_object_ids);
            let id = staged_object_ids.get(&id).cloned().unwrap_or(id);
            objects.insert(id, object);
        }
    }
    if let Some(serde_json::Value::Object(constraints)) = document.get_mut("constraints") {
        normalize_object_id_array_field(constraints, "free_objects", staged_object_ids);
        if let Some(serde_json::Value::Object(constraints)) = constraints.get_mut("constraints") {
            for constraint in constraints.values_mut() {
                if let serde_json::Value::Object(constraint) = constraint {
                    normalize_object_id_array_field(constraint, "inputs", staged_object_ids);
                    normalize_object_id_array_field(constraint, "outputs", staged_object_ids);
                }
            }
        }
    }
    if let Some(serde_json::Value::Object(points)) =
        document.get_mut("spreadsheet_coordinate_points")
    {
        for point in points.values_mut() {
            normalize_object_id_value(point, staged_object_ids);
        }
    }
}

fn normalize_object_identity_fields(
    value: &mut serde_json::Value,
    staged_object_ids: &BTreeMap<String, String>,
) {
    match value {
        serde_json::Value::Array(values) => {
            for value in values {
                normalize_object_identity_fields(value, staged_object_ids);
            }
        }
        serde_json::Value::Object(fields) => {
            for (key, value) in fields {
                if key == "id" {
                    normalize_object_id_value(value, staged_object_ids);
                } else {
                    normalize_object_identity_fields(value, staged_object_ids);
                }
            }
        }
        _ => {}
    }
}

fn normalize_object_id_array_field(
    fields: &mut serde_json::Map<String, serde_json::Value>,
    key: &str,
    staged_object_ids: &BTreeMap<String, String>,
) {
    let Some(serde_json::Value::Array(values)) = fields.get_mut(key) else {
        return;
    };
    for value in values {
        normalize_object_id_value(value, staged_object_ids);
    }
}

fn normalize_object_id_value(
    value: &mut serde_json::Value,
    staged_object_ids: &BTreeMap<String, String>,
) {
    let serde_json::Value::String(id) = value else {
        return;
    };
    if let Some(replacement) = staged_object_ids.get(id) {
        *id = replacement.clone();
    }
}

fn canonical_json_sort_key(value: &serde_json::Value) -> String {
    serde_json::to_string(value).unwrap_or_default()
}

fn evidence_commitment(receipt: &AssistantPlanReceipt) -> Result<String, String> {
    sha256_value(serde_json::json!({
        "schema_version": receipt.schema_version,
        "policy_version": receipt.policy_version,
        "digest_algorithm": receipt.digest_algorithm,
        "plan_commitment": receipt.plan_commitment,
        "base": receipt.base,
        "staged": receipt.staged,
        "delta": receipt.delta,
    }))
}

fn sha256_value(value: serde_json::Value) -> Result<String, String> {
    let bytes = serde_json::to_vec(&canonicalize_json(value)).map_err(|error| error.to_string())?;
    Ok(format!("{:x}", Sha256::digest(bytes)))
}

fn canonicalize_json(value: serde_json::Value) -> serde_json::Value {
    match value {
        serde_json::Value::Array(values) => {
            serde_json::Value::Array(values.into_iter().map(canonicalize_json).collect())
        }
        serde_json::Value::Object(values) => {
            let mut entries = values.into_iter().collect::<Vec<_>>();
            entries.sort_unstable_by(|left, right| left.0.cmp(&right.0));
            let mut canonical = serde_json::Map::new();
            for (key, value) in entries {
                canonical.insert(key, canonicalize_json(value));
            }
            serde_json::Value::Object(canonical)
        }
        value => value,
    }
}

fn validate_graph_expression(document: &Document, expression: &str) -> Result<(), String> {
    let mut variables = document
        .variables()
        .iter()
        .filter(|(_, value)| value.is_finite())
        .map(|(name, value)| (name.clone(), *value))
        .collect::<Vec<_>>();
    variables.retain(|(name, _)| name != "x");
    let mut finite_sample = false;
    for sample in [-1.0, 0.0, 1.0] {
        let mut scoped = variables.clone();
        scoped.push(("x".into(), sample));
        match evaluate(expression, &scoped) {
            Ok(value) if value.is_finite() => finite_sample = true,
            Ok(_) | Err(_) => {}
        }
    }
    if finite_sample {
        Ok(())
    } else {
        Err("assistant graph expression could not be safely evaluated".into())
    }
}

fn contains_forbidden_graph_token(expression: &str) -> bool {
    let lower = expression.to_ascii_lowercase();
    ["script", "file", "save", "export", "delete", "network", ";"]
        .iter()
        .any(|token| lower.contains(token))
}

fn is_variable_name(name: &str) -> bool {
    if name.is_empty() || name.len() > 64 {
        return false;
    }
    let mut chars = name.chars();
    matches!(chars.next(), Some(first) if first.is_ascii_alphabetic() || first == '_')
        && chars.all(|character| character.is_ascii_alphanumeric() || character == '_')
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::assistant_context::document_context;
    use grafito_assistant_types::{AssistantOperation, ProposedPlan, MAX_PROPOSED_PLAN_OPERATIONS};
    use grafito_core::{Document, GeoObject, TextObj};
    use grafito_geometry::Point2;

    fn snapshot(document: &Document) -> (serde_json::Value, u64) {
        (serde_json::to_value(document).unwrap(), document.version)
    }

    #[test]
    fn stale_or_invalid_plans_cannot_mutate_the_document() {
        let mut document = Document::new();
        let basis = document_context(&document).basis();
        document.set_variable("changed".into(), 1.0);
        let before = snapshot(&document);
        let stale = ProposedPlan::new(
            basis,
            vec![AssistantOperation::SetVariable {
                name: "a".into(),
                value: 2.0,
            }],
        );

        assert!(apply_plan(&mut document, &stale).is_err());
        assert_eq!(snapshot(&document), before);

        let invalid = ProposedPlan::new(
            document_context(&document).basis(),
            vec![AssistantOperation::SetVariable {
                name: "a".into(),
                value: f64::NAN,
            }],
        );
        assert!(apply_plan(&mut document, &invalid).is_err());
        assert_eq!(snapshot(&document), before);
    }

    #[test]
    fn preview_and_apply_allow_only_variables_and_simple_graphs() {
        let mut document = Document::new();
        let plan = ProposedPlan::new(
            document_context(&document).basis(),
            vec![
                AssistantOperation::SetVariable {
                    name: "a".into(),
                    value: 2.0,
                },
                AssistantOperation::CreateGraph {
                    expression: "x".into(),
                    variable: "x".into(),
                    domain_min: -5.0,
                    domain_max: 5.0,
                },
            ],
        );

        let preview = preview_plan(&document, &plan).unwrap();
        assert!(preview.changes.iter().any(|change| change.contains("a")));
        apply_plan(&mut document, &plan).unwrap();
        assert_eq!(document.get_variable("a"), Some(2.0));
        assert_eq!(document.object_count(), 1);
    }

    #[test]
    fn staging_applies_operations_sequentially_without_mutating_the_live_document() {
        let mut document = Document::new();
        let before = snapshot(&document);
        let plan = ProposedPlan::new(
            document_context(&document).basis(),
            vec![
                AssistantOperation::SetVariable {
                    name: "a".into(),
                    value: 2.0,
                },
                AssistantOperation::CreateGraph {
                    expression: "a*x".into(),
                    variable: "x".into(),
                    domain_min: -2.0,
                    domain_max: 2.0,
                },
            ],
        );

        let staged = stage_plan(&document, &plan).expect("dependent plan stages");
        let receipt = staged.receipt().clone();
        assert_eq!(snapshot(&document), before);
        assert_eq!(staged.preview().changes.len(), 2);
        assert_eq!(receipt.delta.changed_variable_count, 1);
        assert_eq!(receipt.delta.created_object_count, 1);

        let result = apply_staged_plan(&mut document, staged).expect("staged plan applies");
        assert_eq!(document.get_variable("a"), Some(2.0));
        assert_eq!(document.object_count(), 1);
        assert_eq!(result.receipt, receipt);
    }

    #[test]
    fn staging_rejects_a_plan_without_a_semantic_delta() {
        let mut document = Document::new();
        document.set_variable("a".into(), 2.0);
        let before = snapshot(&document);
        let plan = ProposedPlan::new(
            document_context(&document).basis(),
            vec![AssistantOperation::SetVariable {
                name: "a".into(),
                value: 2.0,
            }],
        );

        assert!(stage_plan(&document, &plan).is_err());
        assert_eq!(snapshot(&document), before);
    }

    #[test]
    fn semantic_commitment_preserves_user_text_that_looks_like_an_object_id() {
        let mut document = Document::new();
        let mut text = TextObj::new("", Point2::new(0.0, 0.0));
        text.content = text.id.0.to_string();
        let id = document
            .try_add_object(GeoObject::Text(text))
            .expect("text inserts");
        let mut different = document.detached_clone_for_staging();
        let Some(GeoObject::Text(text)) = different.get_object_mut(id) else {
            panic!("inserted text remains available");
        };
        text.content = "object:0".into();

        assert_ne!(
            semantic_document_commitment(&document, &BTreeMap::new()).expect("first commitment"),
            semantic_document_commitment(&different, &BTreeMap::new()).expect("second commitment"),
        );
    }

    #[test]
    fn graph_commands_and_unknown_operations_cannot_bypass_the_allowlist() {
        let mut document = Document::new();
        let before = snapshot(&document);
        let forbidden = ProposedPlan::new(
            document_context(&document).basis(),
            vec![AssistantOperation::CreateGraph {
                expression: "Script[DeleteAll[]]".into(),
                variable: "x".into(),
                domain_min: -1.0,
                domain_max: 1.0,
            }],
        );

        assert!(apply_plan(&mut document, &forbidden).is_err());
        assert_eq!(snapshot(&document), before);

        let raw_script = r#"{"operation":"script","source":"DeleteAll[]"}"#;
        assert!(serde_json::from_str::<AssistantOperation>(raw_script).is_err());
    }

    #[test]
    fn oversized_plan_is_rejected_before_any_document_mutation() {
        let mut document = Document::new();
        let before = snapshot(&document);
        let plan = ProposedPlan::new(
            document_context(&document).basis(),
            (0..=MAX_PROPOSED_PLAN_OPERATIONS)
                .map(|index| AssistantOperation::SetVariable {
                    name: format!("v{index}"),
                    value: index as f64,
                })
                .collect(),
        );

        assert!(apply_plan(&mut document, &plan).is_err());
        assert_eq!(snapshot(&document), before);
    }
}
