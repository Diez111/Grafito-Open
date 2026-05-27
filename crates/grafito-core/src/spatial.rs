//! Grafito Spatial Index — R-tree for O(log n) hit testing and view culling.
use rstar::{RTree, AABB};

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct SpatialPoint {
    pub id: usize,
    pub x: f64,
    pub y: f64,
}

impl rstar::Point for SpatialPoint {
    type Scalar = f64;
    const DIMENSIONS: usize = 2;
    fn generate(mut g: impl FnMut(usize) -> Self::Scalar) -> Self {
        SpatialPoint { id: 0, x: g(0), y: g(1) }
    }
    fn nth(&self, i: usize) -> Self::Scalar { if i == 0 { self.x } else { self.y } }
    fn nth_mut(&mut self, i: usize) -> &mut Self::Scalar { if i == 0 { &mut self.x } else { &mut self.y } }
}

pub struct SpatialIndex {
    tree: RTree<SpatialPoint>,
    next_id: usize,
}

impl SpatialIndex {
    pub fn new() -> Self { Self { tree: RTree::new(), next_id: 0 } }

    pub fn insert(&mut self, x: f64, y: f64) -> usize {
        let id = self.next_id; self.next_id += 1;
        self.tree.insert(SpatialPoint { id, x, y });
        id
    }

    pub fn rebuild(&mut self, points: &[(f64, f64)]) {
        self.next_id = points.len();
        let sp: Vec<_> = points.iter().enumerate()
            .map(|(i, (x, y))| SpatialPoint { id: i, x: *x, y: *y }).collect();
        self.tree = rstar::RTree::bulk_load(sp);
    }

    pub fn nearest(&self, x: f64, y: f64, max_dist: f64) -> Option<(usize, f64)> {
        let _d2 = max_dist * max_dist;
        let point = SpatialPoint { id: 0, x, y };
        self.tree.nearest_neighbor(&point)
            .map(|p| {
                let dx = p.x - x; let dy = p.y - y;
                (p.id, (dx*dx + dy*dy).sqrt())
            })
            .filter(|(_, d)| *d <= max_dist)
    }

    pub fn in_bbox(&self, min_x: f64, min_y: f64, max_x: f64, max_y: f64) -> Vec<usize> {
        let aabb = AABB::from_corners(
            SpatialPoint { id: 0, x: min_x, y: min_y },
            SpatialPoint { id: 0, x: max_x, y: max_y },
        );
        self.tree.locate_in_envelope(&aabb).map(|p| p.id).collect()
    }

    pub fn len(&self) -> usize { self.tree.size() }
}
