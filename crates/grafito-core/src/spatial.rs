//! Grafito Spatial Index — R-tree for O(log n) hit testing and view culling.
use crate::id::ObjectId;
use rstar::{RTree, RTreeObject, AABB};

#[derive(Clone, Debug, PartialEq)]
pub struct SpatialItem {
    pub id: ObjectId,
    pub aabb: AABB<[f64; 2]>,
}

impl RTreeObject for SpatialItem {
    type Envelope = AABB<[f64; 2]>;
    fn envelope(&self) -> Self::Envelope {
        self.aabb
    }
}

#[derive(Clone, Default)]
pub struct SpatialIndex {
    tree: RTree<SpatialItem>,
    unbounded: Vec<ObjectId>,
}

impl std::fmt::Debug for SpatialIndex {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "SpatialIndex")
    }
}

impl SpatialIndex {
    pub fn new() -> Self {
        Self {
            tree: RTree::new(),
            unbounded: Vec::new(),
        }
    }

    pub fn insert(&mut self, id: ObjectId, min_x: f64, min_y: f64, max_x: f64, max_y: f64) {
        // Guard: non-finite bounds would corrupt `bulk_load` (NaN breaks R-tree ordering).
        if !min_x.is_finite() || !min_y.is_finite() || !max_x.is_finite() || !max_y.is_finite() {
            return;
        }
        let aabb = AABB::from_corners([min_x, min_y], [max_x, max_y]);
        self.tree.insert(SpatialItem { id, aabb });
    }

    /// Fallible variant — returns `Err` if any bound is non-finite.
    pub fn try_insert(
        &mut self,
        id: ObjectId,
        min_x: f64,
        min_y: f64,
        max_x: f64,
        max_y: f64,
    ) -> Result<(), String> {
        if !min_x.is_finite() || !min_y.is_finite() || !max_x.is_finite() || !max_y.is_finite() {
            return Err(format!(
                "SpatialIndex::insert: non-finite bounds [{min_x},{min_y}]-[{max_x},{max_y}]"
            ));
        }
        let aabb = AABB::from_corners([min_x, min_y], [max_x, max_y]);
        self.tree.insert(SpatialItem { id, aabb });
        Ok(())
    }

    pub fn rebuild(&mut self, items: Vec<(ObjectId, f64, f64, f64, f64)>) {
        self.rebuild_with_unbounded(items, Vec::new());
    }

    /// Rebuilds finite envelopes while retaining objects whose true envelope is
    /// unbounded. Those IDs are conservatively returned for every point query.
    /// Items with non-finite bounds are silently dropped to keep `bulk_load` sound
    /// (NaN would corrupt R-tree ordering and break `locate_in_envelope`).
    /// El bulk_load es determinista: ordena por ObjectId antes de construir el R-tree.
    pub fn rebuild_with_unbounded(
        &mut self,
        items: Vec<(ObjectId, f64, f64, f64, f64)>,
        mut unbounded: Vec<ObjectId>,
    ) {
        let mut sp: Vec<_> = items
            .into_iter()
            .filter(|(_, min_x, min_y, max_x, max_y)| {
                min_x.is_finite() && min_y.is_finite() && max_x.is_finite() && max_y.is_finite()
            })
            .map(|(id, min_x, min_y, max_x, max_y)| SpatialItem {
                id,
                aabb: AABB::from_corners([min_x, min_y], [max_x, max_y]),
            })
            .collect();
        // Determinismo: ordenar por ObjectId para que bulk_load no dependa del
        // orden de iteración de HashMap/BTreeMap del Document.
        sp.sort_by_key(|item| item.id);
        self.tree = rstar::RTree::bulk_load(sp);
        unbounded.sort_unstable();
        unbounded.dedup();
        self.unbounded = unbounded;
    }

    /// Variante fallible que rechaza AABB no-finito con error tipado.
    /// Retorna `Err` si algún bound es NaN/Inf; de lo contrario hace bulk_load determinista.
    pub fn try_rebuild_with_unbounded(
        &mut self,
        items: Vec<(ObjectId, f64, f64, f64, f64)>,
        unbounded: Vec<ObjectId>,
    ) -> Result<(), String> {
        for (id, min_x, min_y, max_x, max_y) in &items {
            if !min_x.is_finite() || !min_y.is_finite() || !max_x.is_finite() || !max_y.is_finite()
            {
                return Err(format!(
                    "SpatialIndex::try_rebuild_with_unbounded: non-finite bounds for {id} [{min_x},{min_y}]-[{max_x},{max_y}]"
                ));
            }
        }
        self.rebuild_with_unbounded(items, unbounded);
        Ok(())
    }

    /// Variante fallible para `rebuild` simple.
    pub fn try_rebuild(
        &mut self,
        items: Vec<(ObjectId, f64, f64, f64, f64)>,
    ) -> Result<(), String> {
        self.try_rebuild_with_unbounded(items, Vec::new())
    }

    pub fn candidates(&self, x: f64, y: f64, tolerance: f64) -> Vec<ObjectId> {
        let query_aabb = AABB::from_corners(
            [x - tolerance, y - tolerance],
            [x + tolerance, y + tolerance],
        );
        let mut candidates: Vec<_> = self
            .tree
            .locate_in_envelope_intersecting(&query_aabb)
            .map(|item| item.id)
            .collect();
        candidates.extend(self.unbounded.iter().copied());
        candidates.sort_unstable();
        candidates.dedup();
        candidates
    }

    pub fn len(&self) -> usize {
        self.tree.size() + self.unbounded.len()
    }

    pub fn is_empty(&self) -> bool {
        self.tree.size() == 0 && self.unbounded.is_empty()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn id(n: u64) -> ObjectId {
        // Deterministic ids for tests via the public ObjectId::new would be
        // random; instead we build them from raw uuids so queries are stable.
        ObjectId(uuid::Uuid::from_u128(n.into()))
    }

    #[test]
    fn new_index_is_empty() {
        let idx = SpatialIndex::new();
        assert!(idx.is_empty());
        assert_eq!(idx.len(), 0);
        assert!(idx.candidates(0.0, 0.0, 1.0).is_empty());
    }

    #[test]
    fn insert_and_locate_single_point() {
        let mut idx = SpatialIndex::new();
        idx.insert(id(1), 5.0, 5.0, 5.0, 5.0);
        assert_eq!(idx.len(), 1);
        // Query near the point finds it.
        let near = idx.candidates(5.0, 5.0, 0.5);
        assert!(near.contains(&id(1)));
        // Query far away does not.
        let far = idx.candidates(100.0, 100.0, 0.5);
        assert!(!far.contains(&id(1)));
    }

    #[test]
    fn nearest_neighbor_query_finds_closest() {
        let mut idx = SpatialIndex::new();
        // Three points at increasing distances from the origin.
        idx.insert(id(1), 1.0, 0.0, 1.0, 0.0);
        idx.insert(id(2), 5.0, 0.0, 5.0, 0.0);
        idx.insert(id(3), 10.0, 0.0, 10.0, 0.0);

        // A small tolerance around (1.2, 0) should only catch the closest point.
        let found = idx.candidates(1.2, 0.0, 0.5);
        assert!(
            found.contains(&id(1)),
            "expected id(1) near (1.2,0), got {:?}",
            found
        );
        assert!(!found.contains(&id(2)));
        assert!(!found.contains(&id(3)));
    }

    #[test]
    fn range_query_aabb_returns_overlapping_items() {
        let mut idx = SpatialIndex::new();
        // Two axis-aligned boxes.
        idx.insert(id(10), 0.0, 0.0, 2.0, 2.0);
        idx.insert(id(20), 10.0, 10.0, 12.0, 12.0);
        idx.insert(id(30), 1.0, 1.0, 3.0, 3.0);

        // Query AABB covering [0,4]x[0,4] should hit boxes 10 and 30 but not 20.
        let found = idx.candidates(2.0, 2.0, 2.0);
        assert!(found.contains(&id(10)));
        assert!(found.contains(&id(30)));
        assert!(!found.contains(&id(20)));
    }

    #[test]
    fn rebuild_replaces_index_contents() {
        let mut idx = SpatialIndex::new();
        idx.insert(id(1), 0.0, 0.0, 0.0, 0.0);
        assert_eq!(idx.len(), 1);

        idx.rebuild(vec![
            (id(2), 1.0, 1.0, 1.0, 1.0),
            (id(3), 2.0, 2.0, 2.0, 2.0),
        ]);
        assert_eq!(idx.len(), 2);
        // Old item is gone after rebuild.
        assert!(!idx.candidates(0.0, 0.0, 0.5).contains(&id(1)));
        assert!(idx.candidates(1.0, 1.0, 0.5).contains(&id(2)));
    }

    #[test]
    fn bulk_load_is_deterministic_regardless_of_input_order() {
        let items_a = vec![
            (id(3), 3.0, 3.0, 3.0, 3.0),
            (id(1), 1.0, 1.0, 1.0, 1.0),
            (id(2), 2.0, 2.0, 2.0, 2.0),
        ];
        let items_b = vec![
            (id(1), 1.0, 1.0, 1.0, 1.0),
            (id(2), 2.0, 2.0, 2.0, 2.0),
            (id(3), 3.0, 3.0, 3.0, 3.0),
        ];
        let mut idx_a = SpatialIndex::new();
        idx_a.rebuild(items_a);
        let mut idx_b = SpatialIndex::new();
        idx_b.rebuild(items_b);
        assert_eq!(idx_a.len(), idx_b.len());
        // Candidates con tolerancia amplia deben coincidir ordenados.
        let cand_a = idx_a.candidates(2.0, 2.0, 5.0);
        let cand_b = idx_b.candidates(2.0, 2.0, 5.0);
        assert_eq!(cand_a, cand_b);
        assert_eq!(cand_a, vec![id(1), id(2), id(3)]);
    }

    #[test]
    fn try_rebuild_rejects_non_finite_aabb() {
        let mut idx = SpatialIndex::new();
        let err = idx
            .try_rebuild(vec![(id(1), f64::NAN, 0.0, 1.0, 1.0)])
            .expect_err("NaN bounds must be rejected");
        assert!(err.contains("non-finite"));
        // Después de error, índice sigue vacío (no se corrompe).
        assert!(idx.is_empty());
        // rebuild silencioso sigue drop pero determinista.
        idx.rebuild(vec![(id(1), f64::INFINITY, 0.0, 1.0, 1.0)]);
        assert!(idx.is_empty(), "infinite bounds should be dropped");
        // try_insert también rechaza
        assert!(idx.try_insert(id(2), f64::NAN, 0.0, 1.0, 1.0).is_err());
        assert!(idx.try_insert(id(2), 0.0, 0.0, 1.0, 1.0).is_ok());
    }
}
