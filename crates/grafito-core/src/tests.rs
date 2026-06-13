#[cfg(test)]
#[allow(clippy::module_inception)]
mod tests {
    use crate::*;
    use grafito_geometry::*;
    use std::collections::HashMap;

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
        assert_eq!(stored.label(), "P"); // First point gets label "P" (not "P1")
    }

    #[test]
    fn test_document_auto_label_multiple() {
        let mut doc = Document::new();
        doc.add_object(GeoObject::Point(PointObj::new(Point2::new(0.0, 0.0))));
        doc.add_object(GeoObject::Point(PointObj::new(Point2::new(1.0, 1.0))));
        doc.add_object(GeoObject::Line(LineObj::new(
            Point2::new(0.0, 0.0),
            Point2::new(1.0, 1.0),
        )));
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
        let id1 = ObjectId::new();
        let id2 = ObjectId::new();
        let id3 = ObjectId::new();

        // Insert three objects with bounding boxes
        idx.insert(id1, 0.0, 0.0, 2.0, 2.0); // Box around (1,1)
        idx.insert(id2, 2.0, 2.0, 4.0, 4.0); // Box around (3,3)
        idx.insert(id3, 9.0, 9.0, 11.0, 11.0); // Box around (10,10)

        assert_eq!(idx.len(), 3);

        // Query near (1,1) should find id1
        let candidates = idx.candidates(1.0, 1.0, 0.5);
        assert!(candidates.contains(&id1));
        assert!(!candidates.contains(&id3));

        // Query near (10,10) should find id3
        let candidates = idx.candidates(10.0, 10.0, 0.5);
        assert!(candidates.contains(&id3));
        assert!(!candidates.contains(&id1));
    }

    #[test]
    fn test_constraint_graph_free_object() {
        let mut cg = ConstraintGraph::new();
        let id = ObjectId::new();
        cg.add_free_object(id);
        assert!(cg.is_free(&id));
        assert_eq!(cg.free_count(), 1);
    }

    #[test]
    fn test_constraint_graph_add_constraint() {
        let mut cg = ConstraintGraph::new();
        let a = ObjectId::new();
        let b = ObjectId::new();
        let m = ObjectId::new();
        cg.add_free_object(a);
        cg.add_free_object(b);
        let cons_id = cg.add_constraint("Midpoint", vec![a, b], vec![m], HashMap::new());
        assert_eq!(cg.constraint_count(), 1);
        assert!(!cg.is_free(&m));
        assert!(cg.is_free(&a));
        assert_eq!(cg.creator_of(&m).unwrap().id, cons_id);
    }

    #[test]
    fn test_constraint_graph_update_order() {
        let mut cg = ConstraintGraph::new();
        let a = ObjectId::new();
        let b = ObjectId::new();
        let m = ObjectId::new();
        let m2 = ObjectId::new();
        cg.add_free_object(a);
        cg.add_free_object(b);
        // M = Midpoint[A, B]
        cg.add_constraint("Midpoint", vec![a, b], vec![m], HashMap::new());
        // M2 = Midpoint[M, B]
        cg.add_constraint("Midpoint", vec![m, b], vec![m2], HashMap::new());

        // When A changes, both constraints should be in update order
        let order = cg.get_update_order(&[a]);
        assert_eq!(order.len(), 2); // Both constraints need re-evaluation
    }

    #[test]
    fn test_constraint_graph_cycle_detection() {
        let mut cg = ConstraintGraph::new();
        let a = ObjectId::new();
        let b = ObjectId::new();
        let c = ObjectId::new();
        cg.add_free_object(a);

        // C1: A -> B
        cg.add_constraint("C1", vec![a], vec![b], HashMap::new());
        // C2: B -> C
        cg.add_constraint("C2", vec![b], vec![c], HashMap::new());
        // C3: C -> B (creates a cycle B -> C -> B)
        cg.add_constraint("C3", vec![c], vec![b], HashMap::new());

        let order = cg.get_update_order(&[a]);
        assert_eq!(
            order.len(),
            3,
            "all reachable constraints should be returned"
        );
        assert!(order.contains(&0));
        assert!(order.contains(&1));
        assert!(order.contains(&2));
        // No duplicates
        let unique: std::collections::HashSet<_> = order.iter().collect();
        assert_eq!(unique.len(), order.len());
    }

    #[test]
    fn test_document_add_constructed_object() {
        let mut doc = Document::new();
        let a = doc.add_object(GeoObject::Point(PointObj::new(Point2::new(0.0, 0.0))));
        let b = doc.add_object(GeoObject::Point(PointObj::new(Point2::new(4.0, 0.0))));
        let (m, _) = doc.add_constructed_object(
            GeoObject::Point(PointObj::new(Point2::new(2.0, 0.0)).with_label("M")),
            "Midpoint",
            &[a, b],
        );
        assert!(!doc.is_free_object(&m));
        assert!(doc.is_free_object(&a));
        assert_eq!(doc.constraints.constraint_count(), 1);
    }

    #[test]
    fn test_document_move_point_propagation() {
        let mut doc = Document::new();
        let a = doc.add_object(GeoObject::Point(PointObj::new(Point2::new(0.0, 0.0))));
        let b = doc.add_object(GeoObject::Point(PointObj::new(Point2::new(4.0, 0.0))));
        doc.add_constructed_object(
            GeoObject::Point(PointObj::new(Point2::new(2.0, 0.0)).with_label("M")),
            "Midpoint",
            &[a, b],
        );
        // Move A to (2, 2) - should mark M as affected
        let affected = doc.move_point(a, Point2::new(2.0, 2.0));
        assert!(affected.contains(&a)); // A itself is affected
                                        // The midpoint object ID should also be in the affected list
        assert!(affected.len() >= 2);
    }

    #[test]
    fn test_document_propagation_order() {
        let mut doc = Document::new();
        let a = doc.add_object(GeoObject::Point(PointObj::new(Point2::new(0.0, 0.0))));
        let b = doc.add_object(GeoObject::Point(PointObj::new(Point2::new(4.0, 0.0))));
        doc.add_constructed_object(
            GeoObject::Point(PointObj::new(Point2::new(2.0, 0.0)).with_label("M")),
            "Midpoint",
            &[a, b],
        );
        let order = doc.propagation_order(&[a]);
        assert_eq!(order.len(), 1); // One constraint to re-evaluate
    }

    #[test]
    fn test_document_remove_cleans_constraints() {
        let mut doc = Document::new();
        let a = doc.add_object(GeoObject::Point(PointObj::new(Point2::new(0.0, 0.0))));
        let b = doc.add_object(GeoObject::Point(PointObj::new(Point2::new(4.0, 0.0))));
        let (m, _) = doc.add_constructed_object(
            GeoObject::Point(PointObj::new(Point2::new(2.0, 0.0)).with_label("M")),
            "Midpoint",
            &[a, b],
        );
        assert_eq!(doc.constraints.constraint_count(), 1);
        doc.remove_object(m);
        assert_eq!(doc.constraints.constraint_count(), 0);
    }

    #[test]
    fn test_document_clear_resets_constraints() {
        let mut doc = Document::new();
        let a = doc.add_object(GeoObject::Point(PointObj::new(Point2::new(0.0, 0.0))));
        let b = doc.add_object(GeoObject::Point(PointObj::new(Point2::new(4.0, 0.0))));
        doc.add_constructed_object(
            GeoObject::Point(PointObj::new(Point2::new(2.0, 0.0)).with_label("M")),
            "Midpoint",
            &[a, b],
        );
        assert_eq!(doc.constraints.free_count(), 2);
        doc.clear();
        assert_eq!(doc.constraints.constraint_count(), 0);
        assert_eq!(doc.constraints.free_count(), 0);
    }

    #[test]
    fn test_implicit_curve_evaluation_and_caching() {
        let ic = ImplicitCurveObj::new("x^3 + y^3", "3*x*y", RelationOperator::Eq);
        let vars = std::collections::HashMap::new();
        let view_bounds = (-3.0, 3.0, -3.0, 3.0);
        let grid_size = 40;

        // First call computes and caches.
        let segs1 = {
            let guard = implicit_curve::segments_or_compute(&ic, view_bounds, grid_size, &vars);
            guard.clone()
        };
        assert!(
            !segs1.is_empty(),
            "folium of descartes should produce segments"
        );

        // Second call with identical parameters returns the cached value.
        let segs2 = {
            let guard = implicit_curve::segments_or_compute(&ic, view_bounds, grid_size, &vars);
            guard.clone()
        };
        assert_eq!(segs1.len(), segs2.len());

        // A different view invalidates and recomputes.
        let segs3 = {
            let guard =
                implicit_curve::segments_or_compute(&ic, (-1.0, 1.0, -1.0, 1.0), grid_size, &vars);
            guard.clone()
        };
        // May be empty if zoomed into a region without the curve, but cache key changed.
        assert!(ic
            .cached_key
            .read()
            .unwrap()
            .as_ref()
            .map(|k| k.view_bounds == (-1.0, 1.0, -1.0, 1.0))
            .unwrap_or(false));
        let _ = segs3;
    }

    #[test]
    fn test_constraint_params_serialize_roundtrip() {
        let mut doc = Document::new();
        let a = doc.add_object(GeoObject::Point(PointObj::new(Point2::new(0.0, 0.0))));
        let mut params = HashMap::new();
        params.insert("dx".to_string(), 3.0);
        params.insert("dy".to_string(), 4.0);
        let (_p, cons_id) = doc.add_constructed_object_with_params(
            GeoObject::Point(PointObj::new(Point2::new(3.0, 4.0)).with_label("P'")),
            "Translate",
            &[a],
            params,
        );

        let json = serde_json::to_string(&doc).expect("serialize document");
        let loaded: Document = serde_json::from_str(&json).expect("deserialize document");

        let cons = loaded
            .constraints
            .get_constraint(cons_id)
            .expect("constraint should survive roundtrip");
        assert_eq!(cons.params.get("dx"), Some(&3.0));
        assert_eq!(cons.params.get("dy"), Some(&4.0));
    }

    #[test]
    fn test_perpendicular_constraint() {
        let mut doc = Document::new();
        let a = doc.add_object(GeoObject::Point(PointObj::new(Point2::new(0.0, 0.0))));
        let b = doc.add_object(GeoObject::Point(PointObj::new(Point2::new(4.0, 0.0))));
        let line = doc.add_object(GeoObject::Line(LineObj::new(
            Point2::new(0.0, 0.0),
            Point2::new(4.0, 0.0),
        )));
        let p = doc.add_object(GeoObject::Point(PointObj::new(Point2::new(2.0, 3.0))));
        let (perp, _) = doc.add_constructed_object(
            GeoObject::Line(
                LineObj::new_with_kind(
                    Point2::new(0.0, 0.0),
                    Point2::new(1.0, 1.0),
                    LineKind::Line,
                )
                .with_label("P"),
            ),
            "Perpendicular",
            &[line, p],
        );
        let order = doc.propagation_order(&[a, b, p]);
        doc.re_evaluate_constraints(&order);
        let perp_obj = doc.get_object(perp).unwrap();
        if let GeoObject::Line(l) = perp_obj {
            let dx = l.end.x - l.start.x;
            let dy = l.end.y - l.start.y;
            // Direction should be vertical (perpendicular to horizontal reference)
            assert!((dx).abs() < 1e-9, "perpendicular should be vertical");
            assert!(dy.abs() > 1e-9);
            // Should pass through point (2, 3)
            assert!((l.start.x - 2.0).abs() < 1e-9);
            let min_y = l.start.y.min(l.end.y);
            let max_y = l.start.y.max(l.end.y);
            assert!(min_y <= 3.0 + 1e-9 && max_y >= 3.0 - 1e-9);
        } else {
            panic!("expected line");
        }
    }

    #[test]
    fn test_parallel_constraint() {
        let mut doc = Document::new();
        let a = doc.add_object(GeoObject::Point(PointObj::new(Point2::new(0.0, 0.0))));
        let b = doc.add_object(GeoObject::Point(PointObj::new(Point2::new(4.0, 2.0))));
        let line = doc.add_object(GeoObject::Line(LineObj::new(
            Point2::new(0.0, 0.0),
            Point2::new(4.0, 2.0),
        )));
        let p = doc.add_object(GeoObject::Point(PointObj::new(Point2::new(1.0, 5.0))));
        let (parallel, _) = doc.add_constructed_object(
            GeoObject::Line(
                LineObj::new_with_kind(
                    Point2::new(0.0, 0.0),
                    Point2::new(1.0, 1.0),
                    LineKind::Line,
                )
                .with_label("L"),
            ),
            "Parallel",
            &[line, p],
        );
        let order = doc.propagation_order(&[a, b, p]);
        doc.re_evaluate_constraints(&order);
        let parallel_obj = doc.get_object(parallel).unwrap();
        if let GeoObject::Line(l) = parallel_obj {
            let dx = l.end.x - l.start.x;
            let dy = l.end.y - l.start.y;
            // Slope should be 0.5 (same as reference line)
            assert!((dy / dx - 0.5).abs() < 1e-9);
            // Should pass through point (1, 5)
            let t = (1.0 - l.start.x) / dx;
            let y_at_x1 = l.start.y + t * dy;
            assert!((y_at_x1 - 5.0).abs() < 1e-9);
        } else {
            panic!("expected line");
        }
    }

    #[test]
    fn test_circle_by_center_radius_constraint() {
        let mut doc = Document::new();
        let center = doc.add_object(GeoObject::Point(PointObj::new(Point2::new(2.0, 3.0))));
        let mut params = HashMap::new();
        params.insert("radius".to_string(), 5.0);
        let (circle, _) = doc.add_constructed_object_with_params(
            GeoObject::Circle(CircleObj::new(Point2::new(0.0, 0.0), 1.0).with_label("C")),
            "CircleByCenterRadius",
            &[center],
            params,
        );
        let order = doc.propagation_order(&[center]);
        doc.re_evaluate_constraints(&order);
        let circle_obj = doc.get_object(circle).unwrap();
        if let GeoObject::Circle(c) = circle_obj {
            assert!((c.center.x - 2.0).abs() < 1e-9);
            assert!((c.center.y - 3.0).abs() < 1e-9);
            assert!((c.radius - 5.0).abs() < 1e-9);
        } else {
            panic!("expected circle");
        }
    }

    #[test]
    fn test_circle_by_three_points_constraint() {
        let mut doc = Document::new();
        let a = doc.add_object(GeoObject::Point(PointObj::new(Point2::new(1.0, 0.0))));
        let b = doc.add_object(GeoObject::Point(PointObj::new(Point2::new(0.0, 1.0))));
        let c = doc.add_object(GeoObject::Point(PointObj::new(Point2::new(-1.0, 0.0))));
        let (circle, _) = doc.add_constructed_object(
            GeoObject::Circle(CircleObj::new(Point2::new(0.0, 0.0), 1.0).with_label("C")),
            "CircleByThreePoints",
            &[a, b, c],
        );
        let order = doc.propagation_order(&[a, b, c]);
        doc.re_evaluate_constraints(&order);
        let circle_obj = doc.get_object(circle).unwrap();
        if let GeoObject::Circle(circ) = circle_obj {
            assert!((circ.center.x - 0.0).abs() < 1e-9);
            assert!((circ.center.y - 0.0).abs() < 1e-9);
            assert!((circ.radius - 1.0).abs() < 1e-9);
        } else {
            panic!("expected circle");
        }
    }

    #[test]
    fn test_constraint_params_backward_compatible() {
        // Serialize a document with a constraint, then strip the params field to
        // simulate an old JSON document that has no params field on Constraint.
        let mut doc = Document::new();
        let a = doc.add_object(GeoObject::Point(PointObj::new(Point2::new(0.0, 0.0))));
        let (_m, _) = doc.add_constructed_object(
            GeoObject::Point(PointObj::new(Point2::new(1.0, 1.0)).with_label("M")),
            "Midpoint",
            &[a, a],
        );

        let mut json = serde_json::to_string(&doc).expect("serialize document");
        // Remove the params field from the serialized constraint to mimic old saves.
        json = json.replace(",\"params\":{}", "");

        let loaded: Document = serde_json::from_str(&json).expect("deserialize old document");
        let cons = loaded
            .constraints
            .get_constraint(0)
            .expect("constraint 0 should exist");
        assert!(cons.params.is_empty());
    }

    #[test]
    fn test_circle_radius_expr_recomputes() {
        let mut doc = Document::new();
        doc.set_variable("r".to_string(), 3.0);
        let mut circle = CircleObj::new(Point2::new(0.0, 0.0), 1.0);
        circle.radius_expr = Some("r".to_string());
        let id = doc.add_object(GeoObject::Circle(circle));
        doc.recompute_bound_parameters();
        if let GeoObject::Circle(c) = doc.get_object(id).unwrap() {
            assert!((c.radius - 3.0).abs() < 1e-9);
        } else {
            panic!("expected circle");
        }

        doc.set_variable("r".to_string(), 5.0);
        if let GeoObject::Circle(c) = doc.get_object(id).unwrap() {
            assert!((c.radius - 5.0).abs() < 1e-9);
        } else {
            panic!("expected circle");
        }
    }

    #[test]
    fn test_point_expr_recomputes() {
        let mut doc = Document::new();
        doc.set_variable("a".to_string(), 2.0);
        doc.set_variable("b".to_string(), 3.0);
        let mut point = PointObj::new(Point2::new(0.0, 0.0));
        point.x_expr = Some("a+1".to_string());
        point.y_expr = Some("2*b".to_string());
        let id = doc.add_object(GeoObject::Point(point));
        doc.recompute_bound_parameters();
        if let GeoObject::Point(p) = doc.get_object(id).unwrap() {
            assert!((p.position.x - 3.0).abs() < 1e-9);
            assert!((p.position.y - 6.0).abs() < 1e-9);
        } else {
            panic!("expected point");
        }
    }

    #[test]
    fn test_expression_fields_backward_compatible() {
        // Old JSON without x_expr/y_expr/radius_expr should deserialize to None.
        let mut doc = Document::new();
        let id = doc.add_object(GeoObject::Point(PointObj::new(Point2::new(1.0, 2.0))));
        let mut json = serde_json::to_string(&doc).expect("serialize document");
        // Strip the new optional fields if present to mimic old saves.
        json = json.replace(",\"x_expr\":null", "");
        json = json.replace(",\"y_expr\":null", "");

        let loaded: Document = serde_json::from_str(&json).expect("deserialize old document");
        if let GeoObject::Point(p) = loaded.get_object(id).unwrap() {
            assert!(p.x_expr.is_none());
            assert!(p.y_expr.is_none());
            assert_eq!(p.position, Point2::new(1.0, 2.0));
        } else {
            panic!("expected point");
        }
    }
}
