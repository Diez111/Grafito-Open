use grafito_assistant::harness;
use grafito_assistant_types::{AssistantOperation, PrivacyMode, ProposedPlan};
use grafito_command::assistant_context::document_context;
use grafito_core::{deserialize_document, serialize_document, Document, GeoObject, PointObj};
use grafito_geometry::Point2;

fn snapshot(document: &Document) -> (serde_json::Value, u64) {
    (
        serde_json::to_value(document).expect("document serializes"),
        document.version,
    )
}

#[test]
fn local_request_stages_replays_and_applies_without_mutating_before_approval() {
    let mut document = Document::new();
    let request = harness::local_request(&document, "graph x^2");
    let result = harness::request(&document, &request).expect("local request succeeds");
    let plan = result
        .response
        .plan
        .clone()
        .expect("graph request produces a plan");
    let staged = result.staged_plan.expect("graph plan stages locally");
    let receipt = staged.receipt().clone();
    let before = snapshot(&document);
    let serialized_receipt = serde_json::to_string(&receipt).expect("receipt serializes");
    assert!(!serialized_receipt.contains("graph x^2"));
    assert!(!serialized_receipt.contains("x^2"));

    let restaged = harness::stage(&document, &plan).expect("same plan restages");
    assert_eq!(receipt.staged, restaged.receipt().staged);

    let replay = harness::replay(&document, &plan, &receipt).expect("receipt replays locally");
    assert_eq!(replay.changes, staged.preview().changes);
    assert_eq!(
        snapshot(&document),
        before,
        "replay must not mutate the document"
    );

    let applied = harness::apply(&mut document, staged).expect("explicit apply succeeds");
    assert_eq!(document.object_count(), 1);
    assert_eq!(document.version, before.1.wrapping_add(1));
    assert_eq!(applied.receipt, receipt);
}

#[test]
fn headless_harness_refuses_remote_or_tampered_inputs_without_mutation() {
    let document = Document::new();
    let mut remote_request = harness::local_request(&document, "graph x");
    remote_request.privacy_mode = PrivacyMode::RemoteAllowed;
    assert!(harness::request(&document, &remote_request).is_err());

    let request = harness::local_request(&document, "graph x");
    let result = harness::request(&document, &request).expect("local request succeeds");
    let plan = result.response.plan.expect("graph request produces a plan");
    let receipt = result
        .staged_plan
        .expect("graph plan stages locally")
        .receipt()
        .clone();
    let before = snapshot(&document);

    let mut altered_plan = plan.clone();
    altered_plan.summary = "different plan".into();
    assert!(harness::replay(&document, &altered_plan, &receipt).is_err());

    let mut altered_base = receipt.clone();
    altered_base.base.semantic_commitment = "c".repeat(64);
    assert!(harness::replay(&document, &plan, &altered_base).is_err());

    let mut altered_delta = receipt.clone();
    altered_delta.delta.created_object_count = 0;
    assert!(harness::replay(&document, &plan, &altered_delta).is_err());

    let mut altered_evidence = receipt;
    altered_evidence.evidence_commitment = "b".repeat(64);
    assert!(harness::replay(&document, &plan, &altered_evidence).is_err());
    assert_eq!(
        snapshot(&document),
        before,
        "replay failures must not mutate"
    );
}

#[test]
fn receipts_replay_after_reopen_with_multiple_staged_graphs() {
    let mut document = Document::new();
    document.set_variable("a".into(), 2.0);
    let plan = ProposedPlan::new(
        document_context(&document).basis(),
        vec![
            AssistantOperation::CreateGraph {
                expression: "x".into(),
                variable: "x".into(),
                domain_min: -2.0,
                domain_max: 2.0,
            },
            AssistantOperation::CreateGraph {
                expression: "a*x".into(),
                variable: "x".into(),
                domain_min: -2.0,
                domain_max: 2.0,
            },
        ],
    );
    let receipt = harness::stage(&document, &plan)
        .expect("multiple graphs stage")
        .receipt()
        .clone();
    let reopened = deserialize_document(&serialize_document(&document).expect("document saves"))
        .expect("document reopens");
    let before = snapshot(&reopened);

    let replay = harness::replay(&reopened, &plan, &receipt)
        .expect("receipt verifies against a semantically unchanged reopen");

    assert_eq!(replay.changes.len(), 2);
    assert_eq!(
        snapshot(&reopened),
        before,
        "replay must not mutate a reopen"
    );
}

#[test]
fn receipts_ignore_rebuilt_constraint_allocators_after_reopen() {
    let mut document = Document::new();
    let first = document
        .try_add_object(GeoObject::Point(PointObj::new(Point2::new(0.0, 0.0))))
        .expect("first point inserts");
    let second = document
        .try_add_object(GeoObject::Point(PointObj::new(Point2::new(1.0, 0.0))))
        .expect("second point inserts");
    document
        .try_add_distance_constraint(first, second, 1.0)
        .expect("distance constraint inserts");
    document.remove_object(first);
    let plan = ProposedPlan::new(
        document_context(&document).basis(),
        vec![AssistantOperation::CreateGraph {
            expression: "x".into(),
            variable: "x".into(),
            domain_min: -2.0,
            domain_max: 2.0,
        }],
    );
    let receipt = harness::stage(&document, &plan)
        .expect("graph stages after constraint removal")
        .receipt()
        .clone();
    let reopened = deserialize_document(&serialize_document(&document).expect("document saves"))
        .expect("document reopens");

    assert!(harness::replay(&reopened, &plan, &receipt).is_ok());
}

#[test]
fn receipts_replay_after_persistence_recomputes_spreadsheet_variables() {
    let document = Document::new()
        .stage_spreadsheet_cell_edits(&[(0, 0, "1".into())])
        .expect("spreadsheet source reconciles");
    let plan = ProposedPlan::new(
        document_context(&document).basis(),
        vec![AssistantOperation::CreateGraph {
            expression: "x".into(),
            variable: "x".into(),
            domain_min: -2.0,
            domain_max: 2.0,
        }],
    );
    let receipt = harness::stage(&document, &plan)
        .expect("graph stages")
        .receipt()
        .clone();
    let reopened = deserialize_document(&serialize_document(&document).expect("document saves"))
        .expect("document reopens");

    assert_eq!(reopened.get_variable("A1"), Some(1.0));
    assert!(harness::replay(&reopened, &plan, &receipt).is_ok());
}

#[test]
fn staging_rejects_writes_to_unresolved_spreadsheet_cell_labels() {
    let mut document = Document::new();
    document
        .set_spreadsheet_cell(0, 0, "(".into())
        .expect("spreadsheet source updates");
    let before = snapshot(&document);
    let plan = ProposedPlan::new(
        document_context(&document).basis(),
        vec![AssistantOperation::SetVariable {
            name: "A1".into(),
            value: 2.0,
        }, AssistantOperation::CreateGraph {
            expression: "x".into(),
            variable: "x".into(),
            domain_min: -1.0,
            domain_max: 1.0,
        }],
    );

    assert!(harness::stage(&document, &plan).is_err());
    assert_eq!(snapshot(&document), before);
}

#[test]
fn staging_rejects_variables_that_would_recompute_spreadsheet_dependencies() {
    let mut document = Document::new();
    document.set_variable("a".into(), 1.0);
    let document = document
        .stage_spreadsheet_cell_edits(&[(0, 0, "a".into())])
        .expect("spreadsheet dependency reconciles");
    let before = snapshot(&document);
    let plan = ProposedPlan::new(
        document_context(&document).basis(),
        vec![AssistantOperation::SetVariable {
            name: "a".into(),
            value: 2.0,
        }],
    );

    assert!(harness::stage(&document, &plan).is_err());
    assert_eq!(snapshot(&document), before);
}
