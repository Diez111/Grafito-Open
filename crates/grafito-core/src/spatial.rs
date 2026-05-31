//! Grafito Spatial Index — R-tree for O(log n) hit testing and view culling.
use rstar::{RTree, RTreeObject, AABB};
use crate::id::ObjectId;

#[derive(Clone, Debug, PartialEq)]
pub struct SpatialItem {
    pub id: ObjectId,
    pub aabb: AABB<[f64; 2]>,
}

impl RTreeObject for SpatialItem {
    type Envelope = AABB<[f64; 2]>;
    fn envelope(&self) -> Self::Envelope {
        self.aabb.clone()
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
    pub fn new() -> Self { Self { tree: RTree::new() } }

    pub fn insert(&mut self, id: ObjectId, min_x: f64, min_y: f64, max_x: f64, max_y: f64) {
        let aabb = AABB::from_corners([min_x, min_y], [max_x, max_y]);
        self.tree.insert(SpatialItem { id, aabb });
    }

    pub fn rebuild(&mut self, items: Vec<(ObjectId, f64, f64, f64, f64)>) {
        let sp: Vec<_> = items.into_iter()
            .map(|(id, min_x, min_y, max_x, max_y)| SpatialItem { 
                id, 
                aabb: AABB::from_corners([min_x, min_y], [max_x, max_y]) 
            }).collect();
        self.tree = rstar::RTree::bulk_load(sp);
    }

    pub fn candidates(&self, x: f64, y: f64, tolerance: f64) -> Vec<ObjectId> {
        let query_aabb = AABB::from_corners(
            [x - tolerance, y - tolerance],
            [x + tolerance, y + tolerance],
        );
        self.tree.locate_in_envelope_intersecting(&query_aabb)
            .map(|item| item.id)
            .collect()
    }

    pub fn len(&self) -> usize { self.tree.size() }
}
