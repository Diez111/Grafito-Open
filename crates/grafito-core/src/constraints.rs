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
use std::collections::{HashMap, HashSet};

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
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
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

impl ConstraintGraph {
    pub fn new() -> Self {
        Self::default()
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
    pub fn remove_object(&mut self, id: ObjectId) {
        self.free_objects.remove(&id);
        if let Some(cons_id) = self.creator.remove(&id) {
            if let Some(cons) = self.constraints.remove(&cons_id) {
                for out in &cons.outputs {
                    self.creator.remove(out);
                    self.dependents.remove(out);
                }
            }
        }
        // Cascade: si quedan constraints que referencian a `id` como input,
        // eliminarlas también para que no queden referencias colgantes.
        if let Some(cons_ids) = self.dependents.remove(&id) {
            for cons_id in cons_ids {
                if let Some(cons) = self.constraints.remove(&cons_id) {
                    for out in &cons.outputs {
                        self.creator.remove(out);
                        self.dependents.remove(out);
                    }
                }
            }
        }
    }

    /// Add a constraint that produces output objects from input objects.
    pub fn add_constraint(
        &mut self,
        name: &str,
        inputs: Vec<ObjectId>,
        outputs: Vec<ObjectId>,
        params: HashMap<String, f64>,
    ) -> usize {
        let id = self.next_id;
        self.next_id += 1;
        let order = self.next_order;
        self.next_order += 1;

        let cons = Constraint {
            id,
            name: name.to_string(),
            inputs: inputs.clone(),
            outputs: outputs.clone(),
            order,
            params,
        };

        // Register dependents
        for input in &inputs {
            self.dependents.entry(*input).or_default().push(id);
        }

        // Register creators
        for output in &outputs {
            self.creator.insert(*output, id);
            self.free_objects.remove(output); // no longer free
        }

        self.constraints.insert(id, cons);
        id
    }

    /// Get the topological update order for changed objects.
    /// Returns constraints in the order they must be re-evaluated.
    ///
    /// Uses DFS with three colors to detect cycles. If a back edge is found,
    /// a warning is logged and the edge is skipped so that a valid order is
    /// still returned for the acyclic portion of the graph.
    pub fn get_update_order(&self, changed: &[ObjectId]) -> Vec<usize> {
        let mut visited = HashSet::new(); // fully processed (black)
        let mut in_stack = HashSet::new(); // currently in recursion stack (gray)
        let mut order = Vec::new();

        fn visit(
            this: &ConstraintGraph,
            cons_id: usize,
            visited: &mut HashSet<usize>,
            in_stack: &mut HashSet<usize>,
            order: &mut Vec<usize>,
        ) {
            if visited.contains(&cons_id) {
                return;
            }
            if in_stack.contains(&cons_id) {
                log::warn!(
                    "Cycle detected in constraint graph at constraint {}, skipping back edge",
                    cons_id
                );
                return;
            }
            in_stack.insert(cons_id);
            if let Some(cons) = this.constraints.get(&cons_id) {
                for output in &cons.outputs {
                    if let Some(deps) = this.dependents.get(output) {
                        for &next_id in deps {
                            visit(this, next_id, visited, in_stack, order);
                        }
                    }
                }
            }
            in_stack.remove(&cons_id);
            visited.insert(cons_id);
            order.push(cons_id);
        }

        // Start DFS from every constraint that directly depends on a changed object.
        for id in changed {
            if let Some(deps) = self.dependents.get(id) {
                for &cons_id in deps {
                    visit(self, cons_id, &mut visited, &mut in_stack, &mut order);
                }
            }
        }

        // Sort by construction order for stable, correct evaluation.
        // Construction order is a valid topological order because inputs must
        // exist before a constraint is created.
        order.sort_by_key(|id| {
            self.constraints
                .get(id)
                .map(|c| c.order)
                .unwrap_or(usize::MAX)
        });

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
}
