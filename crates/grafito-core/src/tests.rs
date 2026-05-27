#[cfg(test)]
mod tests {
    use crate::*;
    use grafito_geometry::*;

    #[test]
    fn test_object_id_creation() {
        let id1 = ObjectId::new();
        let id2 = ObjectId::new();
        assert_ne!(id1, id2);
    }

    #[test]
    fn test_document_add_point() {
        let mut doc = Document::new();
        let obj = GeoObject::Point(PointObj::new(Point2::new(1.0, 2.0)));
        let id = doc.add_object(obj);
        assert_eq!(doc.object_count(), 1);
        assert!(doc.get_object(id).is_some());
    }

    #[test]
    fn test_document_auto_label() {
        let mut doc = Document::new();
        let obj = GeoObject::Point(PointObj::new(Point2::new(0.0, 0.0)));
        let id = doc.add_object(obj);
        let stored = doc.get_object(id).unwrap();
        assert_eq!(stored.label(), "P1"); // Auto-labeled as P1 (Point → P)
    }

    #[test]
    fn test_document_auto_label_multiple() {
        let mut doc = Document::new();
        doc.add_object(GeoObject::Point(PointObj::new(Point2::new(0.0, 0.0))));
        doc.add_object(GeoObject::Point(PointObj::new(Point2::new(1.0, 1.0))));
        doc.add_object(GeoObject::Line(LineObj::new(Point2::new(0.0, 0.0), Point2::new(1.0, 1.0))));
        assert_eq!(doc.object_count(), 3);
    }

    #[test]
    fn test_document_remove_object() {
        let mut doc = Document::new();
        let obj = GeoObject::Point(PointObj::new(Point2::new(0.0, 0.0)));
        let id = doc.add_object(obj);
        assert!(doc.remove_object(id).is_some());
        assert_eq!(doc.object_count(), 0);
    }

    #[test]
    fn test_document_selection() {
        let mut doc = Document::new();
        let id = doc.add_object(GeoObject::Point(PointObj::new(Point2::new(0.0, 0.0))));
        doc.select(id);
        assert!(doc.is_selected(id));
        doc.deselect(id);
        assert!(!doc.is_selected(id));
    }

    #[test]
    fn test_document_clear() {
        let mut doc = Document::new();
        doc.add_object(GeoObject::Point(PointObj::new(Point2::new(0.0, 0.0))));
        doc.add_object(GeoObject::Point(PointObj::new(Point2::new(1.0, 1.0))));
        doc.clear();
        assert_eq!(doc.object_count(), 0);
    }

    #[test]
    fn test_document_variables() {
        let mut doc = Document::new();
        doc.set_variable("a".into(), 42.0);
        assert_eq!(doc.get_variable("a"), Some(42.0));
        assert_eq!(doc.get_variable("b"), None);
    }

    #[test]
    fn test_document_visible_filtering() {
        let mut doc = Document::new();
        doc.add_object(GeoObject::Point(PointObj::new(Point2::new(0.0, 0.0))));
        let all_visible = doc.objects_iter().all(|(_, obj)| obj.is_visible());
        assert!(all_visible);
    }

    #[test]
    fn test_pick_object_point() {
        let mut doc = Document::new();
        doc.set_view(ViewTransform::new(800.0, 600.0));
        doc.add_object(GeoObject::Point(PointObj::new(Point2::new(2.0, 3.0))));
        // Pick near the point
        let result = doc.pick_object(Point2::new(2.0, 3.0), 1.0);
        assert!(result.is_some());
        // Pick far away
        let result = doc.pick_object(Point2::new(100.0, 100.0), 1.0);
        assert!(result.is_none());
    }

    #[test]
    fn test_spatial_index_insert_and_query() {
        let mut idx = SpatialIndex::new();
        idx.insert(1.0, 2.0);
        idx.insert(3.0, 4.0);
        idx.insert(10.0, 10.0);
        assert_eq!(idx.len(), 3);
        let nearest = idx.nearest(1.5, 2.5, 2.0).unwrap();
        assert_eq!(nearest.0, 0); // First inserted point
    }

    #[test]
    fn test_constraint_graph_free_object() {
        let mut cg = ConstraintGraph::new();
        let id = ObjectId::new();
        cg.add_free_object(id);
        assert!(cg.is_free(&id));
        assert_eq!(cg.free_count(), 1);
    }
}
