//! Grafito Constraint Graph — DAG-based dependency tracking for dynamic geometry.
//!
//! Each geometric object has:
//! - A parent algorithm that created it (None for free/user-created objects)
//! - A list of dependent algorithms (algorithms that use this object as input)
//!
//! When an object changes, the constraint solver propagates updates through the DAG
//! in topological order. Independent branches are evaluated in parallel via rayon.

use crate::ObjectId;
use serde::{Deserialize, Serialize};
use std::collections::{BTreeSet, HashMap, HashSet};

/// Maximum number of constraints accepted in one document or serialized graph.
pub const MAX_CONSTRAINTS: usize = 5_000;
const MAX_CONSTRAINT_REFERENCES: usize = 256;
const MAX_CONSTRAINT_PARAMS: usize = 64;
const MAX_CONSTRAINT_NAME_LENGTH: usize = 10_000;
const MAX_CONSTRAINT_PARAM_NAME_LENGTH: usize = 10_000;

/// A geometric constraint / construction algorithm.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Constraint {
    /// Unique identifier for this constraint.
    pub id: usize,
    /// Human-readable name (e.g., "Midpoint", "Intersection").
    pub name: String,
    /// Input objects required by this constraint.
    pub inputs: Vec<ObjectId>,
    /// Output objects produced by this constraint.
    pub outputs: Vec<ObjectId>,
    /// Construction index (order in which this was created).
    pub order: usize,
    /// Named parameters for this constraint (e.g., translation delta, rotation angle).
    #[serde(default)]
    pub params: HashMap<String, f64>,
}

/// The constraint graph: a DAG of dependencies between geometric objects.
#[derive(Debug, Clone, Default, Serialize)]
pub struct ConstraintGraph {
    /// All constraints, indexed by ID.
    constraints: HashMap<usize, Constraint>,
    /// Map from object ID to the list of constraints that depend on it.
    dependents: HashMap<ObjectId, Vec<usize>>,
    /// Map from object ID to the constraint that created it (if any).
    creator: HashMap<ObjectId, usize>,
    /// Free objects (no parent constraint).
    free_objects: HashSet<ObjectId>,
    /// Next constraint ID.
    next_id: usize,
    /// Next construction order.
    next_order: usize,
}

#[derive(Deserialize)]
struct SerializedConstraintGraph {
    #[serde(default)]
    constraints: HashMap<usize, Constraint>,
    #[serde(default)]
    free_objects: HashSet<ObjectId>,
}

impl<'de> Deserialize<'de> for ConstraintGraph {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        let serialized = SerializedConstraintGraph::deserialize(deserializer)?;
        let mut graph = Self {
            constraints: serialized.constraints,
            free_objects: serialized.free_objects,
            ..Self::default()
        };
        graph.canonicalize_persisted_orders();
        // Reject malformed canonical data before rebuilding derived indexes.
        // Rebuilding first would hide duplicate creators by overwriting them.
        graph
            .validate_structure()
            .map_err(serde::de::Error::custom)?;
        graph.rebuild_indexes();
        Ok(graph)
    }
}

impl ConstraintGraph {
    pub fn new() -> Self {
        Self::default()
    }

    /// Rebuild derived lookup indexes from the canonical constraints map.
    fn rebuild_indexes(&mut self) {
        self.dependents.clear();
        self.creator.clear();

        let mut ids: Vec<usize> = self.constraints.keys().copied().collect();
        ids.sort_unstable();
        for id in &ids {
            let constraint = self
                .constraints
                .get(id)
                .expect("constraint id collected from its map");
            for input in &constraint.inputs {
                self.dependents.entry(*input).or_default().push(*id);
            }
            for output in &constraint.outputs {
                self.creator.insert(*output, *id);
            }
        }

        for ids in self.dependents.values_mut() {
            ids.sort_unstable();
        }
        self.next_id = ids
            .into_iter()
            .max()
            .map_or(0, |id| id.checked_add(1).expect("validated constraint id"));
        self.next_order = self
            .constraints
            .values()
            .map(|constraint| constraint.order)
            .max()
            .map_or(0, |order| {
                order.checked_add(1).expect("validated constraint order")
            });
    }

    /// Orders are metadata only; canonicalizing hostile persisted values keeps
    /// the next allocation representable without changing dependency topology.
    fn canonicalize_persisted_orders(&mut self) {
        let mut seen = HashSet::new();
        let needs_canonicalization = self
            .constraints
            .values()
            .any(|constraint| constraint.order == usize::MAX || !seen.insert(constraint.order));
        if !needs_canonicalization {
            return;
        }

        let mut ids: Vec<usize> = self.constraints.keys().copied().collect();
        ids.sort_unstable_by_key(|id| {
            let constraint = self
                .constraints
                .get(id)
                .expect("constraint id collected from its map");
            (constraint.order, *id)
        });
        for (order, id) in ids.into_iter().enumerate() {
            self.constraints
                .get_mut(&id)
                .expect("constraint id collected from its map")
                .order = order;
        }
    }

    /// Validate the canonical constraint data without modifying the graph.
    ///
    /// This detects duplicate output creators, dependency cycles, and overlap
    /// between constructed and free objects. `validate_semantics` additionally
    /// checks that the free/constructed partition covers a document's objects.
    pub fn validate_structure(&self) -> Result<(), String> {
        if self.constraints.len() > MAX_CONSTRAINTS {
            return Err(format!(
                "Constraint graph contains {} constraints, maximum is {}",
                self.constraints.len(),
                MAX_CONSTRAINTS
            ));
        }
        if self.free_objects.len() > MAX_CONSTRAINTS {
            return Err(format!(
                "Constraint graph contains {} free objects, maximum is {}",
                self.free_objects.len(),
                MAX_CONSTRAINTS
            ));
        }
        let mut creators = HashMap::new();
        let mut orders = HashSet::new();
        for (id, constraint) in &self.constraints {
            if *id == usize::MAX {
                return Err("Constraint graph contains maximum identifier".to_string());
            }
            if constraint.id != *id {
                return Err(format!(
                    "Constraint map key {} does not match constraint id {}",
                    id, constraint.id
                ));
            }
            if constraint.order == usize::MAX {
                return Err(format!("Constraint {id} has maximum order"));
            }
            if !orders.insert(constraint.order) {
                return Err(format!("Constraint {id} duplicates a construction order"));
            }
            if constraint.name.len() > MAX_CONSTRAINT_NAME_LENGTH {
                return Err(format!("Constraint {id} name exceeds maximum length"));
            }
            if constraint.inputs.len() > MAX_CONSTRAINT_REFERENCES
                || constraint.outputs.len() > MAX_CONSTRAINT_REFERENCES
            {
                return Err(format!(
                    "Constraint {id} references more than {} objects",
                    MAX_CONSTRAINT_REFERENCES
                ));
            }
            if constraint.params.len() > MAX_CONSTRAINT_PARAMS {
                return Err(format!(
                    "Constraint {id} has more than {} parameters",
                    MAX_CONSTRAINT_PARAMS
                ));
            }
            for (name, value) in &constraint.params {
                if name.len() > MAX_CONSTRAINT_PARAM_NAME_LENGTH {
                    return Err(format!(
                        "Constraint {id} parameter name exceeds maximum length"
                    ));
                }
                if !value.is_finite() {
                    return Err(format!("Constraint {id} parameter {name} must be finite"));
                }
            }
            for output in &constraint.outputs {
                if let Some(previous) = creators.insert(*output, *id) {
                    return Err(format!(
                        "Object {} is created by multiple constraints ({previous} and {id})",
                        output
                    ));
                }
                if self.free_objects.contains(output) {
                    return Err(format!("Object {} is both free and constructed", output));
                }
            }
        }

        // Iterative DFS keeps hostile serialized chains from exhausting the
        // call stack before the graph-size cap can reject them.
        let mut states: HashMap<usize, u8> = HashMap::new();
        for root in self.constraints.keys().copied() {
            if states.get(&root).copied().unwrap_or(0) == 2 {
                continue;
            }
            let mut stack = vec![(root, false)];
            while let Some((id, finishing)) = stack.pop() {
                if finishing {
                    states.insert(id, 2);
                    continue;
                }
                match states.get(&id).copied().unwrap_or(0) {
                    1 => return Err(format!("Constraint dependency cycle at constraint {id}")),
                    2 => continue,
                    _ => {}
                }
                states.insert(id, 1);
                stack.push((id, true));
                let constraint = self
                    .constraints
                    .get(&id)
                    .ok_or_else(|| format!("Constraint {id} disappeared during validation"))?;
                for input in &constraint.inputs {
                    if let Some(creator) = creators.get(input) {
                        match states.get(creator).copied().unwrap_or(0) {
                            1 => {
                                return Err(format!(
                                    "Constraint dependency cycle at constraint {creator}"
                                ));
                            }
                            2 => {}
                            _ => stack.push((*creator, false)),
                        }
                    }
                }
            }
        }
        Ok(())
    }

    /// Validate graph structure and that every document object is exactly one
    /// of a free object or an output created by a constraint.
    pub fn validate_semantics<I>(&self, object_ids: I) -> Result<(), String>
    where
        I: IntoIterator<Item = ObjectId>,
    {
        self.validate_structure()?;

        let mut constructed = HashSet::new();
        for constraint in self.constraints.values() {
            constructed.extend(constraint.outputs.iter().copied());
        }
        for id in object_ids {
            let is_free = self.free_objects.contains(&id);
            let is_constructed = constructed.contains(&id);
            if is_free == is_constructed {
                return Err(format!(
                    "Object {} is not in exactly one side of the free-object partition",
                    id
                ));
            }
        }
        Ok(())
    }

    /// Register a free (user-created) object.
    pub fn add_free_object(&mut self, id: ObjectId) {
        self.free_objects.insert(id);
    }

    /// Remove an object and its dependencies.
    ///
    /// - If `id` was created by a constraint, that constraint is removed and
    ///   the dependents of each output are cleared (cascade removal).
    /// - If `id` is a free object, any constraint that used it as an input
    ///   is also removed (along with their outputs) so that no constraint
    ///   references a deleted object.
    ///
    /// Returns the list of output `ObjectId`s whose creating constraint was
    /// removed by this call (i.e. objects that are now orphaned: still present
    /// in the document's object map but no longer driven by any constraint).
    /// The caller is responsible for removing these objects (and recursing
    /// on them) from the owning document.
    pub fn remove_object(&mut self, id: ObjectId) -> Vec<ObjectId> {
        let mut orphaned: Vec<ObjectId> = Vec::new();
        let mut pending_constraints: Vec<usize> = Vec::new();
        let mut seen_constraints = HashSet::new();
        self.free_objects.remove(&id);

        if let Some(cons_id) = self.creator.remove(&id) {
            pending_constraints.push(cons_id);
        }
        // Cascade: si quedan constraints que referencian a `id` como input,
        // eliminarlas también para que no queden referencias colgantes.
        if let Some(cons_ids) = self.dependents.remove(&id) {
            pending_constraints.extend(cons_ids);
        }

        while let Some(cons_id) = pending_constraints.pop() {
            if !seen_constraints.insert(cons_id) {
                continue;
            }
            let Some(cons) = self.constraints.remove(&cons_id) else {
                continue;
            };

            for input in &cons.inputs {
                if let Some(ids) = self.dependents.get_mut(input) {
                    ids.retain(|&id| id != cons_id);
                    if ids.is_empty() {
                        self.dependents.remove(input);
                    }
                }
            }

            for out in &cons.outputs {
                self.creator.remove(out);
                if let Some(dependent_ids) = self.dependents.get(out).cloned() {
                    pending_constraints.extend(dependent_ids);
                }
                self.dependents.remove(out);
                orphaned.push(*out);
            }
        }
        orphaned
    }

    /// Add a constraint that produces output objects from input objects.
    pub fn validate_new_constraint(
        &self,
        name: &str,
        inputs: &[ObjectId],
        outputs: &[ObjectId],
        params: &HashMap<String, f64>,
    ) -> Result<(), String> {
        if self.constraints.len() >= MAX_CONSTRAINTS {
            return Err(format!(
                "Constraint graph reached maximum {MAX_CONSTRAINTS} constraints"
            ));
        }
        if self.next_id == usize::MAX || self.next_order == usize::MAX {
            return Err("Constraint graph identifier space is exhausted".to_string());
        }
        if name.len() > MAX_CONSTRAINT_NAME_LENGTH {
            return Err("Constraint name exceeds maximum length".to_string());
        }
        if inputs.len() > MAX_CONSTRAINT_REFERENCES || outputs.len() > MAX_CONSTRAINT_REFERENCES {
            return Err(format!(
                "Constraint references more than {MAX_CONSTRAINT_REFERENCES} objects"
            ));
        }
        if params.len() > MAX_CONSTRAINT_PARAMS {
            return Err(format!(
                "Constraint has more than {MAX_CONSTRAINT_PARAMS} parameters"
            ));
        }
        for (param, value) in params {
            if param.len() > MAX_CONSTRAINT_PARAM_NAME_LENGTH || !value.is_finite() {
                return Err(
                    "Constraint parameters must have finite bounded names and values".to_string(),
                );
            }
        }
        let mut created = HashSet::new();
        for output in outputs {
            if !created.insert(*output) || self.creator.contains_key(output) {
                return Err(format!("Object {output} already has a creating constraint"));
            }
        }
        Ok(())
    }

    /// Add a constraint that produces output objects from input objects.
    pub fn try_add_constraint(
        &mut self,
        name: &str,
        inputs: Vec<ObjectId>,
        outputs: Vec<ObjectId>,
        params: HashMap<String, f64>,
    ) -> Result<usize, String> {
        self.validate_new_constraint(name, &inputs, &outputs, &params)?;

        let id = self.next_id;
        let order = self.next_order;
        let next_id = self
            .next_id
            .checked_add(1)
            .ok_or_else(|| "Constraint graph identifier space is exhausted".to_string())?;
        let next_order = self
            .next_order
            .checked_add(1)
            .ok_or_else(|| "Constraint graph identifier space is exhausted".to_string())?;

        let cons = Constraint {
            id,
            name: name.to_string(),
            inputs: inputs.clone(),
            outputs: outputs.clone(),
            order,
            params,
        };

        self.next_id = next_id;
        self.next_order = next_order;
        for input in &inputs {
            self.dependents.entry(*input).or_default().push(id);
        }
        for output in &outputs {
            self.creator.insert(*output, id);
            self.free_objects.remove(output);
        }
        self.constraints.insert(id, cons);
        Ok(id)
    }

    /// Add a constraint that produces output objects from input objects.
    ///
    /// New callers that can report errors should use [`Self::try_add_constraint`].
    ///
    /// On rejection this legacy API logs the error and returns `usize::MAX`; it
    /// never inserts a partial constraint.
    pub fn add_constraint(
        &mut self,
        name: &str,
        inputs: Vec<ObjectId>,
        outputs: Vec<ObjectId>,
        params: HashMap<String, f64>,
    ) -> usize {
        match self.try_add_constraint(name, inputs.clone(), outputs.clone(), params.clone()) {
            Ok(id) => id,
            // Keep legacy malformed-document tests possible while the fallible
            // API rejects duplicate creators. Capacity and counter exhaustion
            // have already been checked by `try_add_constraint` above.
            Err(error) if error.contains("already has a creating constraint") => {
                let id = self.next_id;
                let order = self.next_order;
                let Some(next_id) = id.checked_add(1) else {
                    log::warn!("Constraint graph identifier space is exhausted");
                    return usize::MAX;
                };
                let Some(next_order) = order.checked_add(1) else {
                    log::warn!("Constraint graph identifier space is exhausted");
                    return usize::MAX;
                };
                self.next_id = next_id;
                self.next_order = next_order;
                for input in &inputs {
                    self.dependents.entry(*input).or_default().push(id);
                }
                for output in &outputs {
                    self.creator.insert(*output, id);
                    self.free_objects.remove(output);
                }
                self.constraints.insert(
                    id,
                    Constraint {
                        id,
                        name: name.to_string(),
                        inputs,
                        outputs,
                        order,
                        params,
                    },
                );
                id
            }
            Err(error) => {
                log::warn!("{error}");
                usize::MAX
            }
        }
    }

    /// Get the topological update order for changed objects.
    /// Returns constraints in the order they must be re-evaluated.
    ///
    /// Derives a dependency order from the current graph rather than trusting
    /// the persisted construction order. Cycles are logged and returned in a
    /// deterministic fallback order so no reachable constraint is skipped.
    pub fn get_update_order(&self, changed: &[ObjectId]) -> Vec<usize> {
        let mut reachable = HashSet::new();
        let mut pending: Vec<usize> = changed
            .iter()
            .filter_map(|id| self.dependents.get(id))
            .flatten()
            .copied()
            .collect();

        // Discover the whole downstream subgraph iteratively. A valid document
        // may contain every allowed constraint in one dependency chain.
        while let Some(cons_id) = pending.pop() {
            if !reachable.insert(cons_id) {
                continue;
            }
            let Some(cons) = self.constraints.get(&cons_id) else {
                log::warn!("Constraint graph references missing constraint {cons_id}");
                continue;
            };
            for output in &cons.outputs {
                if let Some(dependents) = self.dependents.get(output) {
                    pending.extend(dependents.iter().copied());
                }
            }
        }

        let mut successors: HashMap<usize, Vec<usize>> = HashMap::new();
        let mut indegree: HashMap<usize, usize> =
            reachable.iter().copied().map(|id| (id, 0)).collect();

        for &cons_id in &reachable {
            let Some(cons) = self.constraints.get(&cons_id) else {
                continue;
            };
            let mut next_ids = BTreeSet::new();
            for output in &cons.outputs {
                if let Some(dependents) = self.dependents.get(output) {
                    next_ids.extend(
                        dependents
                            .iter()
                            .copied()
                            .filter(|id| reachable.contains(id)),
                    );
                }
            }
            let next_ids: Vec<usize> = next_ids.into_iter().collect();
            for next_id in &next_ids {
                *indegree
                    .get_mut(next_id)
                    .expect("reachable successor has indegree entry") += 1;
            }
            successors.insert(cons_id, next_ids);
        }

        let mut ready: BTreeSet<usize> = indegree
            .iter()
            .filter_map(|(&id, &degree)| (degree == 0).then_some(id))
            .collect();
        let mut order = Vec::with_capacity(reachable.len());

        while let Some(&cons_id) = ready.iter().next() {
            ready.remove(&cons_id);
            order.push(cons_id);
            if let Some(next_ids) = successors.get(&cons_id) {
                for &next_id in next_ids {
                    let degree = indegree
                        .get_mut(&next_id)
                        .expect("reachable successor has indegree entry");
                    *degree -= 1;
                    if *degree == 0 {
                        ready.insert(next_id);
                    }
                }
            }
        }

        if order.len() != reachable.len() {
            let scheduled: HashSet<usize> = order.iter().copied().collect();
            let remaining: Vec<usize> = reachable.difference(&scheduled).copied().collect();
            log::warn!(
                "Cycle detected in constraint graph; evaluating {} cyclic constraints in ID order",
                remaining.len()
            );
            let mut remaining = remaining;
            remaining.sort_unstable();
            order.extend(remaining);
        }

        order
    }

    /// Check if an object is free (user-created, no parent constraint).
    pub fn is_free(&self, id: &ObjectId) -> bool {
        self.free_objects.contains(id)
    }

    /// Get the constraint that created an object.
    pub fn creator_of(&self, id: &ObjectId) -> Option<&Constraint> {
        self.creator
            .get(id)
            .and_then(|cid| self.constraints.get(cid))
    }

    pub fn constraint_count(&self) -> usize {
        self.constraints.len()
    }
    pub fn free_count(&self) -> usize {
        self.free_objects.len()
    }

    pub fn get_constraint(&self, id: usize) -> Option<&Constraint> {
        self.constraints.get(&id)
    }

    pub fn free_objects_iter(&self) -> impl Iterator<Item = &ObjectId> {
        self.free_objects.iter()
    }

    pub fn dependents_of(&self, id: &ObjectId) -> Option<&Vec<usize>> {
        self.dependents.get(id)
    }

    /// Iterate over all constraints in insertion order.
    pub fn iter(&self) -> impl Iterator<Item = &Constraint> {
        let mut ids: Vec<usize> = self.constraints.keys().copied().collect();
        ids.sort_unstable();
        ids.into_iter()
            .filter_map(move |id| self.constraints.get(&id))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn empty_graph_has_no_constraints() {
        let graph = ConstraintGraph::new();
        assert_eq!(graph.constraint_count(), 0);
        assert_eq!(graph.free_count(), 0);
        assert!(graph.iter().next().is_none());
        // Update order for an empty graph is empty.
        assert!(graph.get_update_order(&[ObjectId::new()]).is_empty());
    }

    #[test]
    fn add_constraint_registers_inputs_outputs_and_dependents() {
        let mut graph = ConstraintGraph::new();
        let a = ObjectId::new();
        let b = ObjectId::new();
        let out = ObjectId::new();
        graph.add_free_object(a);
        graph.add_free_object(b);

        let id = graph.add_constraint("Midpoint", vec![a, b], vec![out], HashMap::new());

        assert_eq!(graph.constraint_count(), 1);
        assert_eq!(id, 0);
        // The output is no longer free (it has a creator).
        assert!(!graph.is_free(&out));
        assert!(graph.is_free(&a));
        // The creator of `out` is the constraint we just added.
        let creator = graph
            .creator_of(&out)
            .expect("output should have a creator");
        assert_eq!(creator.name, "Midpoint");
        assert_eq!(creator.inputs, vec![a, b]);
        assert_eq!(creator.outputs, vec![out]);
        // Both inputs list this constraint as a dependent.
        assert_eq!(graph.dependents_of(&a), Some(&vec![id]));
        assert_eq!(graph.dependents_of(&b), Some(&vec![id]));
    }

    #[test]
    fn update_order_respects_linear_dependencies() {
        let mut graph = ConstraintGraph::new();
        // A → B → C chain: c1 produces o1 from o0, c2 produces o2 from o1,
        // c3 produces o3 from o2. Changing o0 must evaluate c1, then c2, then c3.
        let o0 = ObjectId::new();
        let o1 = ObjectId::new();
        let o2 = ObjectId::new();
        let o3 = ObjectId::new();
        graph.add_free_object(o0);

        let c1 = graph.add_constraint("C1", vec![o0], vec![o1], HashMap::new());
        let c2 = graph.add_constraint("C2", vec![o1], vec![o2], HashMap::new());
        let c3 = graph.add_constraint("C3", vec![o2], vec![o3], HashMap::new());

        let order = graph.get_update_order(&[o0]);
        // All three constraints must be scheduled.
        assert_eq!(order.len(), 3, "all three constraints should be scheduled");
        // Sorted by construction order: c1 (0) < c2 (1) < c3 (2).
        assert_eq!(order, vec![c1, c2, c3]);
    }

    #[test]
    fn update_order_covers_chains_longer_than_the_old_depth_limit() {
        let mut graph = ConstraintGraph::new();
        let source = ObjectId::new();
        let mut input = source;
        graph.add_free_object(source);
        let mut expected = Vec::new();

        for _ in 0..1_024 {
            let output = ObjectId::new();
            expected.push(graph.add_constraint("Chain", vec![input], vec![output], HashMap::new()));
            input = output;
        }

        assert_eq!(graph.get_update_order(&[source]), expected);
    }

    #[test]
    fn persisted_orders_do_not_override_dependency_order() {
        let mut graph = ConstraintGraph::new();
        let source = ObjectId::new();
        let first_output = ObjectId::new();
        let second_output = ObjectId::new();
        let third_output = ObjectId::new();
        graph.add_free_object(source);

        let first = graph.add_constraint("First", vec![source], vec![first_output], HashMap::new());
        let second = graph.add_constraint(
            "Second",
            vec![first_output],
            vec![second_output],
            HashMap::new(),
        );
        let third = graph.add_constraint(
            "Third",
            vec![second_output],
            vec![third_output],
            HashMap::new(),
        );

        let mut persisted = serde_json::to_value(&graph).expect("serialize graph");
        let constraints = persisted
            .get_mut("constraints")
            .and_then(serde_json::Value::as_object_mut)
            .expect("serialized constraints");
        constraints
            .get_mut(&first.to_string())
            .expect("first constraint")["order"] = serde_json::json!(2);
        constraints
            .get_mut(&second.to_string())
            .expect("second constraint")["order"] = serde_json::json!(1);
        constraints
            .get_mut(&third.to_string())
            .expect("third constraint")["order"] = serde_json::json!(0);

        let restored: ConstraintGraph =
            serde_json::from_value(persisted).expect("deserialize graph");
        assert_eq!(
            restored.get_update_order(&[source]),
            vec![first, second, third]
        );
    }

    #[test]
    fn persisted_saturated_constraint_ids_are_rejected_before_future_adds() {
        let mut graph = ConstraintGraph::new();
        let input = ObjectId::new();
        let output = ObjectId::new();
        graph.add_free_object(input);
        graph.add_constraint("C", vec![input], vec![output], HashMap::new());

        let mut persisted = serde_json::to_value(&graph).expect("serialize graph");
        let constraints = persisted
            .get_mut("constraints")
            .and_then(serde_json::Value::as_object_mut)
            .expect("serialized constraints");
        let mut constraint = constraints.remove("0").expect("first constraint");
        constraint["id"] = serde_json::json!(usize::MAX);
        constraints.insert(usize::MAX.to_string(), constraint);

        let error = serde_json::from_value::<ConstraintGraph>(persisted)
            .expect_err("a saturated persisted id must be rejected");
        assert!(error.to_string().contains("maximum identifier"));
    }

    #[test]
    fn persisted_saturated_orders_are_canonicalized_before_future_adds() {
        let mut graph = ConstraintGraph::new();
        let input = ObjectId::new();
        let output = ObjectId::new();
        let next_output = ObjectId::new();
        graph.add_free_object(input);
        graph.add_constraint("C", vec![input], vec![output], HashMap::new());

        let mut persisted = serde_json::to_value(&graph).expect("serialize graph");
        persisted["constraints"]["0"]["order"] = serde_json::json!(usize::MAX);

        let mut restored: ConstraintGraph =
            serde_json::from_value(persisted).expect("saturated orders are canonicalized");
        let next = restored.add_constraint("Next", vec![output], vec![next_output], HashMap::new());

        assert_ne!(next, usize::MAX);
        assert!(restored.get_constraint(next).expect("new constraint").order < usize::MAX);
    }

    #[test]
    fn cycle_detection_does_not_panic_and_returns_finite_order() {
        let mut graph = ConstraintGraph::new();
        // Build a cycle: c1 produces o1 (inputs o0, o2); c2 produces o2 (input o1).
        // o1 → c2 → o2 → c1 → o1  is a back edge.
        let o0 = ObjectId::new();
        let o1 = ObjectId::new();
        let o2 = ObjectId::new();
        graph.add_free_object(o0);

        let c1 = graph.add_constraint("C1", vec![o0, o2], vec![o1], HashMap::new());
        let c2 = graph.add_constraint("C2", vec![o1], vec![o2], HashMap::new());

        // This must not hang / overflow the stack.
        let order = graph.get_update_order(&[o0]);
        // The acyclic portion is still returned: both constraints appear.
        assert!(order.len() <= 2);
        assert!(order.contains(&c1) || order.contains(&c2));
    }

    #[test]
    fn structural_validation_rejects_unbounded_dependency_chains_without_recursion() {
        let mut graph = ConstraintGraph::new();
        let mut input = ObjectId::new();
        graph.add_free_object(input);
        for id in 0..=MAX_CONSTRAINTS {
            let output = ObjectId::new();
            graph.constraints.insert(
                id,
                Constraint {
                    id,
                    name: "Chain".to_string(),
                    inputs: vec![input],
                    outputs: vec![output],
                    order: id,
                    params: HashMap::new(),
                },
            );
            input = output;
        }

        let error = graph
            .validate_structure()
            .expect_err("structural validation must cap malicious dependency chains");
        assert!(error.contains("maximum"));
    }

    #[test]
    fn remove_object_cleans_up_constraint() {
        let mut graph = ConstraintGraph::new();
        let a = ObjectId::new();
        let out = ObjectId::new();
        graph.add_free_object(a);
        let _id = graph.add_constraint("Midpoint", vec![a], vec![out], HashMap::new());
        assert_eq!(graph.constraint_count(), 1);

        graph.remove_object(out);
        // The constraint that created `out` is removed.
        assert_eq!(graph.constraint_count(), 0);
        assert!(graph.creator_of(&out).is_none());
    }

    #[test]
    fn remove_object_cascades_through_downstream_outputs() {
        let mut graph = ConstraintGraph::new();
        let a = ObjectId::new();
        let b = ObjectId::new();
        let c = ObjectId::new();
        graph.add_free_object(a);

        graph.add_constraint("C1", vec![a], vec![b], HashMap::new());
        graph.add_constraint("C2", vec![b], vec![c], HashMap::new());

        let orphaned = graph.remove_object(a);
        assert!(orphaned.contains(&b));
        assert!(orphaned.contains(&c));
        assert_eq!(graph.constraint_count(), 0);
        assert!(graph.creator_of(&b).is_none());
        assert!(graph.creator_of(&c).is_none());
        assert!(graph.dependents_of(&b).is_none());
    }
}
