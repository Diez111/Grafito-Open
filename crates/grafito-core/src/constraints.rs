//! Grafito Constraint Graph — DAG-based dependency tracking for dynamic geometry.
//!
//! Each geometric object has:
//! - A parent algorithm that created it (None for free/user-created objects)
//! - A list of dependent algorithms (algorithms that use this object as input)
//!
//! When an object changes, the constraint solver propagates updates through the DAG
//! in topological order. Independent branches are evaluated in parallel via rayon.

use crate::ObjectId;
use std::collections::{HashMap, HashSet};

/// A geometric constraint / construction algorithm.
#[derive(Debug, Clone)]
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
}

/// The constraint graph: a DAG of dependencies between geometric objects.
#[derive(Debug, Clone, Default)]
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
    pub fn add_constraint(&mut self, name: &str, inputs: Vec<ObjectId>, outputs: Vec<ObjectId>) -> usize {
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
    pub fn get_update_order(&self, changed: &[ObjectId]) -> Vec<usize> {
        let mut visited = HashSet::new();
        let mut order = Vec::new();
        let mut stack: Vec<usize> = Vec::new();

        // Find all constraints that depend on changed objects
        for id in changed {
            if let Some(deps) = self.dependents.get(id) {
                for cons_id in deps {
                    if visited.insert(*cons_id) {
                        stack.push(*cons_id);
                    }
                }
            }
        }

        // BFS through the dependency graph
        while let Some(cons_id) = stack.pop() {
            order.push(cons_id);
            if let Some(cons) = self.constraints.get(&cons_id) {
                for output in &cons.outputs {
                    if let Some(deps) = self.dependents.get(output) {
                        for next_id in deps {
                            if visited.insert(*next_id) {
                                stack.push(*next_id);
                            }
                        }
                    }
                }
            }
        }

        // Sort by construction order for correct evaluation
        order.sort_by_key(|id| {
            self.constraints.get(id).map(|c| c.order).unwrap_or(usize::MAX)
        });

        order
    }

    /// Check if an object is free (user-created, no parent constraint).
    pub fn is_free(&self, id: &ObjectId) -> bool {
        self.free_objects.contains(id)
    }

    /// Get the constraint that created an object.
    pub fn creator_of(&self, id: &ObjectId) -> Option<&Constraint> {
        self.creator.get(id).and_then(|cid| self.constraints.get(cid))
    }

    pub fn constraint_count(&self) -> usize { self.constraints.len() }
    pub fn free_count(&self) -> usize { self.free_objects.len() }
}
