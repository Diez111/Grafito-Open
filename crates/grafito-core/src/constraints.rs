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
    pub fn remove_object(&mut self, id: ObjectId) {
        self.free_objects.remove(&id);
        self.dependents.remove(&id);
        if let Some(cons_id) = self.creator.remove(&id) {
            if let Some(cons) = self.constraints.remove(&cons_id) {
                for out in &cons.outputs {
                    self.creator.remove(out);
                    self.dependents.remove(out);
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
}
