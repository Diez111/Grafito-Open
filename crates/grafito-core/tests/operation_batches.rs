use grafito_core::{
    validation::validate_document, ComplexMappingObj, Document, FunctionCacheKey, FunctionObj,
    GeoObject, ImplicitCurveObj, ObjectId, OperationBatch, RelationOperator,
};
use grafito_geometry::Point2;
use std::collections::HashMap;

fn semantic_snapshot(document: &Document) -> serde_json::Value {
    serde_json::to_value(document).expect("document should serialize")
}

#[test]
fn batch_commits_multiple_mutations_as_one_revision() {
    let mut document = Document::new();
    let initial_version = document.version;
    let mut batch = OperationBatch::new();
    batch.push(|document| {
        document.add_point(Point2::new(1.0, 2.0));
        Ok(())
    });
    batch.push(|document| {
        document.add_point(Point2::new(3.0, 4.0));
        Ok(())
    });

    let changes = document.commit(batch).expect("batch should commit");

    assert_eq!(document.object_count(), 2);
    assert_eq!(document.version, initial_version.wrapping_add(1));
    assert_eq!(
        semantic_snapshot(&changes.after),
        semantic_snapshot(&document)
    );
}

#[test]
fn failed_operation_leaves_the_document_and_revision_unchanged() {
    let mut document = Document::new();
    let before = semantic_snapshot(&document);
    let initial_version = document.version;
    let mut batch = OperationBatch::new();
    batch.push(|document| {
        document.add_point(Point2::new(1.0, 2.0));
        Err("intentional operation failure".to_string())
    });

    assert!(document.commit(batch).is_err());

    assert_eq!(semantic_snapshot(&document), before);
    assert_eq!(document.version, initial_version);
}

#[test]
fn validation_failure_leaves_the_document_and_revision_unchanged() {
    let mut document = Document::new();
    let before = semantic_snapshot(&document);
    let initial_version = document.version;
    let missing = ObjectId::new();
    let mut batch = OperationBatch::new();
    batch.push(move |document| {
        document
            .constraints
            .add_constraint("Missing input", vec![missing], vec![], HashMap::new());
        Ok(())
    });

    assert!(document.commit(batch).is_err());

    assert_eq!(semantic_snapshot(&document), before);
    assert_eq!(document.version, initial_version);
}

#[test]
fn empty_or_noop_batches_do_not_create_revisions() {
    let mut document = Document::new();
    let initial_version = document.version;

    let empty_changes = document
        .commit(OperationBatch::new())
        .expect("empty batches are valid no-ops");
    assert_eq!(document.version, initial_version);
    empty_changes
        .undo(&mut document)
        .expect("undoing an empty batch is also a no-op");
    assert_eq!(document.version, initial_version);

    let mut noop = OperationBatch::new();
    noop.push(|_| Ok(()));
    document.commit(noop).expect("no-op batches are valid");
    assert_eq!(document.version, initial_version);
}

#[test]
fn failed_batches_do_not_invalidate_live_runtime_caches() {
    let mut document = Document::new();
    let function_id = document.add_object(GeoObject::Function(FunctionObj::new("x")));
    let function = match document.get_object(function_id).expect("function exists") {
        GeoObject::Function(function) => function,
        _ => panic!("expected function object"),
    };
    *function.cached_key.write().expect("cache lock") = Some(FunctionCacheKey {
        expr: "x".to_string(),
        domain: (-1.0, 1.0),
        grid_size: 16,
        variables_hash: 0,
        is_integral: false,
        integral_var: String::new(),
        integral_lower: 0.0,
    });

    let mut batch = OperationBatch::new();
    batch.push(|staged| {
        staged.invalidate_all_caches();
        Err("intentional operation failure".to_string())
    });

    assert!(document.commit(batch).is_err());
    let function = match document.get_object(function_id).expect("function remains") {
        GeoObject::Function(function) => function,
        _ => panic!("expected function object"),
    };
    assert!(
        function.cached_key.read().expect("cache lock").is_some(),
        "a failed staged operation must not mutate the live cache"
    );
}

#[test]
fn undo_and_redo_restore_semantics_while_advancing_the_revision() {
    let mut document = Document::new();
    let before = semantic_snapshot(&document);
    let mut batch = OperationBatch::new();
    batch.push(|document| {
        document.add_point(Point2::new(1.0, 2.0));
        Ok(())
    });

    let changes = document.commit(batch).expect("batch should commit");
    let after = semantic_snapshot(&document);

    changes.undo(&mut document).expect("undo should validate");
    assert_eq!(semantic_snapshot(&document), before);
    assert_eq!(document.version, 2);

    changes.redo(&mut document).expect("redo should validate");
    assert_eq!(semantic_snapshot(&document), after);
    assert_eq!(document.version, 3);
}

#[test]
fn changeset_rejects_undo_after_an_unrelated_mutation() {
    let mut document = Document::new();
    let mut batch = OperationBatch::new();
    batch.push(|document| {
        document.add_point(Point2::new(1.0, 2.0));
        Ok(())
    });
    let changes = document.commit(batch).expect("batch should commit");
    document.add_point(Point2::new(3.0, 4.0));
    let current = semantic_snapshot(&document);

    assert!(changes.undo(&mut document).is_err());
    assert_eq!(semantic_snapshot(&document), current);
}

#[test]
fn changeset_rejects_redo_after_an_unrelated_mutation() {
    let mut document = Document::new();
    let mut batch = OperationBatch::new();
    batch.push(|document| {
        document.add_point(Point2::new(1.0, 2.0));
        Ok(())
    });
    let changes = document.commit(batch).expect("batch should commit");
    changes.undo(&mut document).expect("undo should succeed");
    document.add_point(Point2::new(3.0, 4.0));
    let current = semantic_snapshot(&document);

    assert!(changes.redo(&mut document).is_err());
    assert_eq!(semantic_snapshot(&document), current);
}

#[test]
fn referenced_object_deletion_remains_valid_across_undo_and_redo() {
    let mut document = Document::new();
    let target = document.add_object(GeoObject::ImplicitCurve(ImplicitCurveObj::new(
        "x^2 + y^2",
        "1",
        RelationOperator::Eq,
    )));
    let mapping = document.add_object(GeoObject::ComplexMapping(ComplexMappingObj::new(
        "z^2", target,
    )));

    let mut batch = OperationBatch::new();
    batch.push(move |staged| {
        staged.remove_object(target);
        Ok(())
    });
    let changes = document
        .commit(batch)
        .expect("target deletion should cascade before validation");
    assert!(document.get_object(target).is_none());
    assert!(document.get_object(mapping).is_none());

    changes.undo(&mut document).expect("undo remains valid");
    assert!(document.get_object(target).is_some());
    assert!(document.get_object(mapping).is_some());
    validate_document(&document).expect("undo state validates");

    changes.redo(&mut document).expect("redo remains valid");
    assert!(document.get_object(target).is_none());
    assert!(document.get_object(mapping).is_none());
    validate_document(&document).expect("redo state validates");
}
