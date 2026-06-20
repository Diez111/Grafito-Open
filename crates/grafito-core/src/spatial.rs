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
}

impl std::fmt::Debug for SpatialIndex {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "SpatialIndex")
    }
}

impl SpatialIndex {
    pub fn new() -> Self {
        Self { tree: RTree::new() }
    }

    pub fn insert(&mut self, id: ObjectId, min_x: f64, min_y: f64, max_x: f64, max_y: f64) {
        let aabb = AABB::from_corners([min_x, min_y], [max_x, max_y]);
        self.tree.insert(SpatialItem { id, aabb });
    }

    pub fn rebuild(&mut self, items: Vec<(ObjectId, f64, f64, f64, f64)>) {
        let sp: Vec<_> = items
            .into_iter()
            .map(|(id, min_x, min_y, max_x, max_y)| SpatialItem {
                id,
                aabb: AABB::from_corners([min_x, min_y], [max_x, max_y]),
            })
            .collect();
        self.tree = rstar::RTree::bulk_load(sp);
    }

    pub fn candidates(&self, x: f64, y: f64, tolerance: f64) -> Vec<ObjectId> {
        let query_aabb = AABB::from_corners(
            [x - tolerance, y - tolerance],
            [x + tolerance, y + tolerance],
        );
        self.tree
            .locate_in_envelope_intersecting(&query_aabb)
            .map(|item| item.id)
            .collect()
    }

    pub fn len(&self) -> usize {
        self.tree.size()
    }

    pub fn is_empty(&self) -> bool {
        self.tree.size() == 0
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
}
