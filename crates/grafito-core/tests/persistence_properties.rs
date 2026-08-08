use grafito_core::{
    deserialize_document, serialize_document, AnimationMode, CasWorksheetStatus, CircleObj,
    DataTableObj, Document, FitMetadata, Fractal2DObj, FunctionObj, GeoObject, PencilObj, PointObj,
    VariableMeta,
};
use grafito_geometry::{
    statistics::{fit_xy, FitKind},
    Point2,
};
use std::panic::{catch_unwind, AssertUnwindSafe};

fn next_u64(state: &mut u64) -> u64 {
    *state = state.wrapping_mul(6364136223846793005).wrapping_add(1);
    *state
}

fn bounded_json_corpus() -> Vec<String> {
    let alphabet = br#"{}[],:\"0123456789abcdefghijklmnopqrstuvwxyzABCDEFGHIJKLMNOPQRSTUVWXYZ -_"#;
    let mut state = 0xFACE_FEED_1234_5678_u64;
    let mut corpus = vec![
        String::new(),
        "{".to_string(),
        "[]".to_string(),
        r#"{"schema_version":999999}"#.to_string(),
        r#"{"objects":[]}"#.to_string(),
    ];

    for _ in 0..96 {
        let length = (next_u64(&mut state) % 128) as usize;
        let mut input = String::with_capacity(length);
        for _ in 0..length {
            input.push(alphabet[(next_u64(&mut state) as usize) % alphabet.len()] as char);
        }
        corpus.push(input);
    }

    corpus
}

#[test]
fn bounded_persisted_documents_round_trip_or_fail_gracefully() {
    let edges = [
        -0.0,
        f64::MIN_POSITIVE,
        f64::EPSILON,
        -f64::EPSILON,
        1.0 / 3.0,
        -1.0 / 3.0,
        1.0e-100,
        -1.0e100,
    ];

    for index in 0..edges.len() {
        let mut document = Document::new();
        document.set_variable("edge".to_string(), edges[index]);
        document.add_point(Point2::new(edges[index], edges[(index + 3) % edges.len()]));
        let encoded = serialize_document(&document).expect("finite document should serialize");
        let decoded =
            deserialize_document(&encoded).expect("serialized document should deserialize");

        assert_eq!(
            serde_json::to_value(decoded).expect("decoded document should serialize"),
            serde_json::to_value(document).expect("source document should serialize")
        );
    }

    for input in bounded_json_corpus() {
        let result = catch_unwind(AssertUnwindSafe(|| deserialize_document(&input)));
        assert!(result.is_ok(), "deserialization panicked for {input:?}");

        if let Ok(document) = result.expect("panic was checked above") {
            assert!(
                serialize_document(&document).is_ok(),
                "accepted persisted document must be serializable: {input:?}"
            );
        }
    }
}

#[test]
fn persistent_cas_worksheet_round_trips_and_schema_v2_defaults_to_empty() {
    let mut document = Document::new();
    document
        .try_append_cas_worksheet_cell(
            "Simplify[x + 0]".to_string(),
            "x".to_string(),
            CasWorksheetStatus::Success,
        )
        .expect("bounded worksheet cell is accepted");

    let encoded = serialize_document(&document).expect("worksheet document serializes");
    let decoded = deserialize_document(&encoded).expect("worksheet document deserializes");
    assert_eq!(decoded.cas_worksheet(), document.cas_worksheet());

    let mut legacy: serde_json::Value =
        serde_json::from_str(&encoded).expect("serialized document is JSON");
    legacy["schema_version"] = serde_json::Value::from(2);
    legacy["document"]
        .as_object_mut()
        .expect("document envelope contains an object")
        .remove("cas_worksheet");

    let legacy = deserialize_document(&legacy.to_string()).expect("schema v2 remains readable");
    assert!(legacy.cas_worksheet().is_empty());
}

#[test]
fn persisted_cas_worksheet_enforces_cell_and_raw_array_bounds() {
    let mut document = Document::new();
    let oversized = "x".repeat(Document::MAX_CAS_WORKSHEET_INPUT_BYTES + 1);
    assert!(document
        .try_append_cas_worksheet_cell(
            oversized,
            "resultado".to_string(),
            CasWorksheetStatus::Success,
        )
        .is_err());
    assert!(document.cas_worksheet().is_empty());

    let mut raw = serde_json::to_value(Document::new()).expect("serialize empty document");
    raw["cas_worksheet"] = serde_json::Value::Array(
        (0..=Document::MAX_CAS_WORKSHEET_CELLS)
            .map(|_| {
                serde_json::json!({
                    "input": "Simplify[x]",
                    "output": "x",
                    "status": "Success",
                })
            })
            .collect(),
    );
    let error = deserialize_document(&raw.to_string())
        .expect_err("raw worksheet array above its cap must fail before document construction");
    assert!(error.to_string().contains("CAS worksheet"), "{error}");
}

#[test]
fn cas_worksheet_append_preserves_document_saveability() {
    fn document_with_spreadsheet_cell_size(cell_size: usize) -> Document {
        let mut document = Document::new();
        document.spreadsheet = vec![vec!["x".repeat(cell_size); Document::MAX_SPREADSHEET_COLS]; 3];
        document
    }

    let mut low = 0usize;
    let mut high = grafito_core::validation::MAX_STRING_LENGTH;
    while low < high {
        let middle = (low + high).div_ceil(2);
        let candidate = document_with_spreadsheet_cell_size(middle);
        if serialize_document(&candidate).is_ok() {
            low = middle;
        } else {
            high = middle - 1;
        }
    }

    let mut document = document_with_spreadsheet_cell_size(low);
    assert!(serialize_document(&document).is_ok());
    let error = document
        .try_append_cas_worksheet_cell(
            "i".repeat(Document::MAX_CAS_WORKSHEET_INPUT_BYTES),
            "o".repeat(Document::MAX_CAS_WORKSHEET_OUTPUT_BYTES),
            CasWorksheetStatus::Success,
        )
        .expect_err("worksheet insertion must not make a saveable document unsaveable");

    assert!(error.contains("Document size"), "{error}");
    assert!(document.cas_worksheet().is_empty());
    assert!(serialize_document(&document).is_ok());
}

#[test]
fn persistence_rejects_fractals_exceeding_the_shared_work_budget() {
    let mut fractal = Fractal2DObj::mandelbrot().with_resolution(400);
    fractal.max_iter = 401;
    let object = GeoObject::Fractal2D(fractal);
    let id = object.id();
    let mut raw = serde_json::to_value(Document::new()).expect("serialize empty document");
    raw["objects"]
        .as_object_mut()
        .expect("objects are represented as a map")
        .insert(
            id.0.to_string(),
            serde_json::to_value(object).expect("serialize unchecked fractal"),
        );
    let document: Document =
        serde_json::from_value(raw).expect("deserialize unchecked test document");

    let save_error = serialize_document(&document)
        .expect_err("persistence must reject render work above the fractal budget");
    assert!(save_error.to_string().contains("work"), "{save_error}");

    let raw_document = serde_json::to_string(&document).expect("document JSON serializes");
    let load_error = deserialize_document(&raw_document)
        .expect_err("loading must enforce the same fractal work budget");
    assert!(load_error.to_string().contains("work"), "{load_error}");
}

#[test]
fn maximum_spreadsheet_round_trips_within_the_raw_json_node_budget() {
    let mut document = Document::new();
    document.spreadsheet =
        vec![vec![String::new(); Document::MAX_SPREADSHEET_COLS]; Document::MAX_SPREADSHEET_ROWS];

    let serialized = serialize_document(&document)
        .expect("a spreadsheet at the documented maximum must serialize");
    let deserialized = deserialize_document(&serialized)
        .expect("a serialized maximum spreadsheet must pass raw JSON validation");

    assert_eq!(
        deserialized.spreadsheet, document.spreadsheet,
        "persistence must preserve every accepted spreadsheet cell"
    );
}

#[test]
fn persistence_rejects_more_active_spreadsheet_cells_than_it_can_recompute() {
    let spreadsheet_with_active_cells = |count: usize| {
        let mut document = Document::new();
        for index in 0..count {
            let row = index / Document::MAX_SPREADSHEET_COLS;
            let col = index % Document::MAX_SPREADSHEET_COLS;
            document
                .set_spreadsheet_cell(row, col, "1".to_string())
                .expect("fixture cell is within the spreadsheet dimensions");
        }
        document
    };

    let supported = spreadsheet_with_active_cells(Document::MAX_SPREADSHEET_RECOMPUTE_CELLS);
    let encoded = serialize_document(&supported).expect("supported active cells serialize");
    deserialize_document(&encoded).expect("supported active cells deserialize");

    let overflow = spreadsheet_with_active_cells(Document::MAX_SPREADSHEET_RECOMPUTE_CELLS + 1);
    let error = serialize_document(&overflow)
        .expect_err("saved documents must not exceed the load-time recomputation limit");
    assert!(error.to_string().contains("recomputation"), "{error}");
}

#[test]
fn raw_json_rejects_oversized_spreadsheet_dimensions_before_document_deserialization() {
    let rows = std::iter::repeat("[]")
        .take(Document::MAX_SPREADSHEET_ROWS + 1)
        .collect::<Vec<_>>()
        .join(",");
    let cells = std::iter::repeat("null")
        .take(Document::MAX_SPREADSHEET_COLS + 1)
        .collect::<Vec<_>>()
        .join(",");

    for raw in [
        format!(r#"{{"spreadsheet":[{rows}]}}"#),
        format!(r#"{{"spreadsheet":[[{cells}]]}}"#),
    ] {
        let error = grafito_core::validation::parse_document_json(&raw)
            .expect_err("oversized spreadsheet JSON must fail before Document deserialization");
        assert!(error.contains("Spreadsheet"), "unexpected error: {error}");
    }
}

#[test]
fn spreadsheet_setter_rejects_oversized_dimensions_before_allocating_storage() {
    let mut document = Document::new();

    assert!(document
        .set_spreadsheet_cell(Document::MAX_SPREADSHEET_ROWS, 0, "1".to_string())
        .is_err());
    assert!(document
        .set_spreadsheet_cell(0, Document::MAX_SPREADSHEET_COLS, "1".to_string())
        .is_err());
    assert!(document.spreadsheet.is_empty());
}

#[test]
fn clearing_a_document_clears_spreadsheet_variable_ownership() {
    let mut document = Document::new();
    document
        .set_spreadsheet_cell(0, 0, "1".to_string())
        .expect("cell is in range");
    document
        .recompute_spreadsheet_variables()
        .expect("spreadsheet computes");
    assert_eq!(document.get_variable("A1"), Some(1.0));

    document.clear();
    document.set_variable("A1".to_string(), 42.0);
    document
        .recompute_spreadsheet_variables()
        .expect("empty spreadsheet recomputes");

    assert_eq!(document.get_variable("A1"), Some(42.0));
}

#[test]
fn loading_reconciles_stale_spreadsheet_variable_ownership() {
    let mut document = Document::new();
    document
        .set_spreadsheet_cell(0, 0, "1".to_string())
        .expect("cell is in range");
    document
        .recompute_spreadsheet_variables()
        .expect("spreadsheet computes");
    document.spreadsheet.clear();

    let serialized = serialize_document(&document).expect("stale ownership document serializes");
    let mut loaded = deserialize_document(&serialized).expect("document loads");

    assert_eq!(loaded.get_variable("A1"), None);
    loaded.set_variable("A1".to_string(), 42.0);
    loaded
        .recompute_spreadsheet_variables()
        .expect("empty spreadsheet recomputes");
    assert_eq!(loaded.get_variable("A1"), Some(42.0));
}

#[test]
fn spreadsheet_formula_updates_do_not_accept_animation_metadata() {
    let mut document = Document::new();
    document
        .set_spreadsheet_cell(0, 0, "1".to_string())
        .expect("fixture cell is in range");
    document
        .recompute_spreadsheet_variables()
        .expect("initial formula resolves");
    document
        .configure_variable_animation("A1", 0.0, 10.0, 1.0, AnimationMode::Loop)
        .expect_err("spreadsheet source must reject animation metadata");

    document
        .set_spreadsheet_cell(0, 0, "2 + 3".to_string())
        .expect("fixture cell is in range");
    document
        .recompute_spreadsheet_variables()
        .expect("updated formula resolves");

    assert_eq!(document.get_variable("A1"), Some(5.0));
    assert!(document.variable_meta("A1").is_none());
    grafito_core::validation::validate_document(&document)
        .expect("resolved spreadsheet document remains valid");
}

#[test]
fn staged_spreadsheet_batch_uses_final_sources_without_mutating_the_live_document() {
    let mut document = Document::new();
    document
        .set_spreadsheet_cell(0, 0, "1".to_string())
        .expect("seed A1");
    document
        .set_spreadsheet_cell(0, 1, "A1 + 1".to_string())
        .expect("seed B1");
    document
        .recompute_spreadsheet_variables()
        .expect("initial spreadsheet resolves");
    let before = serde_json::to_value(&document).expect("document should serialize");
    let version_before = document.version;

    let staged = document
        .stage_spreadsheet_cell_edits(&[(0, 0, String::new()), (0, 1, "2".to_string())])
        .expect("final spreadsheet sources should stage");

    assert_eq!(document.version, version_before);
    assert_eq!(
        serde_json::to_value(&document).expect("document should serialize"),
        before
    );
    assert_eq!(staged.get_variable("A1"), None);
    assert_eq!(staged.get_variable("B1"), Some(2.0));
    assert!(staged.variable_meta("B1").is_none());
    assert_eq!(staged.version, version_before.wrapping_add(1));
}

#[test]
fn staged_spreadsheet_batch_propagates_scalar_bound_points_to_constructed_dependents() {
    let mut document = Document::new();
    document
        .set_spreadsheet_cell(0, 0, "0".to_string())
        .expect("seed A1");
    document
        .recompute_spreadsheet_variables()
        .expect("initial spreadsheet resolves");

    let mut point = PointObj::new(Point2::new(0.0, 0.0)).with_label("P");
    point.x_expr = Some("A1".to_string());
    let point = document.add_object(GeoObject::Point(point));
    let fixed = document.add_object(GeoObject::Point(
        PointObj::new(Point2::new(4.0, 0.0)).with_label("Q"),
    ));
    let (midpoint, _) = document
        .try_add_constructed_object(
            GeoObject::Point(PointObj::new(Point2::new(0.0, 0.0)).with_label("M")),
            "Midpoint",
            &[point, fixed],
        )
        .expect("midpoint fixture is valid");

    let staged = document
        .stage_spreadsheet_cell_edits(&[(0, 0, "2".to_string())])
        .expect("scalar spreadsheet batch should stage");

    assert!(matches!(
        staged.get_object(point),
        Some(GeoObject::Point(point)) if point.position == Point2::new(2.0, 0.0)
    ));
    assert!(matches!(
        staged.get_object(midpoint),
        Some(GeoObject::Point(point)) if point.position == Point2::new(3.0, 0.0)
    ));
}

#[test]
fn staged_spreadsheet_batch_captures_only_the_final_mixed_root_locus_sample() {
    let document = Document::new();
    let mut document = document
        .stage_spreadsheet_cell_edits(&[(0, 0, "0".to_string()), (0, 1, "(2, 0)".to_string())])
        .expect("initial spreadsheet sources should stage");
    let coordinate = document
        .spreadsheet_coordinate_point("B1")
        .expect("B1 owns its coordinate point");
    let mut driver = PointObj::new(Point2::new(0.0, 0.0)).with_label("P");
    driver.x_expr = Some("A1".to_string());
    let driver = document.add_object(GeoObject::Point(driver));
    let (target, _) = document
        .try_add_constructed_object(
            GeoObject::Point(PointObj::new(Point2::new(0.0, 0.0)).with_label("M")),
            "Midpoint",
            &[driver, coordinate],
        )
        .expect("midpoint fixture is valid");
    let (locus, _) = document
        .try_add_locus(driver, target)
        .expect("locus fixture is valid");

    let staged = document
        .stage_spreadsheet_cell_edits(&[(0, 0, "4".to_string()), (0, 1, "(6, 0)".to_string())])
        .expect("mixed spreadsheet batch should stage");

    assert!(matches!(
        staged.get_object(target),
        Some(GeoObject::Point(point)) if point.position == Point2::new(5.0, 0.0)
    ));
    assert!(matches!(
        staged.get_object(locus),
        Some(GeoObject::Pencil(pencil)) if pencil.points == vec![
            Point2::new(1.0, 0.0),
            Point2::new(5.0, 0.0),
        ]
    ));
}

#[test]
fn standalone_spreadsheet_recomputation_propagates_bound_dependents() {
    let mut document = Document::new();
    document
        .set_spreadsheet_cell(0, 0, "0".to_string())
        .expect("seed A1");
    document
        .recompute_spreadsheet_variables()
        .expect("initial spreadsheet resolves");

    let mut point = PointObj::new(Point2::new(0.0, 0.0)).with_label("P");
    point.x_expr = Some("A1".to_string());
    let point = document.add_object(GeoObject::Point(point));
    let fixed = document.add_object(GeoObject::Point(
        PointObj::new(Point2::new(4.0, 0.0)).with_label("Q"),
    ));
    let (midpoint, _) = document
        .try_add_constructed_object(
            GeoObject::Point(PointObj::new(Point2::new(0.0, 0.0)).with_label("M")),
            "Midpoint",
            &[point, fixed],
        )
        .expect("midpoint fixture is valid");

    document
        .set_spreadsheet_cell(0, 0, "2".to_string())
        .expect("update A1 source");
    document
        .recompute_spreadsheet_variables()
        .expect("standalone spreadsheet recomputation succeeds");

    assert!(matches!(
        document.get_object(point),
        Some(GeoObject::Point(point)) if point.position == Point2::new(2.0, 0.0)
    ));
    assert!(matches!(
        document.get_object(midpoint),
        Some(GeoObject::Point(point)) if point.position == Point2::new(3.0, 0.0)
    ));
}

#[test]
fn public_bound_parameter_recomputation_propagates_constructed_dependents() {
    let mut document = Document::new();
    document.set_variable("a".to_string(), 0.0);
    let mut point = PointObj::new(Point2::new(0.0, 0.0)).with_label("P");
    point.x_expr = Some("a".to_string());
    let point = document.add_object(GeoObject::Point(point));
    let fixed = document.add_object(GeoObject::Point(
        PointObj::new(Point2::new(4.0, 0.0)).with_label("Q"),
    ));
    let (midpoint, _) = document
        .try_add_constructed_object(
            GeoObject::Point(PointObj::new(Point2::new(0.0, 0.0)).with_label("M")),
            "Midpoint",
            &[point, fixed],
        )
        .expect("midpoint fixture is valid");

    document.variables.insert("a".to_string(), 2.0);
    document.recompute_bound_parameters();

    assert!(matches!(
        document.get_object(point),
        Some(GeoObject::Point(point)) if point.position == Point2::new(2.0, 0.0)
    ));
    assert!(matches!(
        document.get_object(midpoint),
        Some(GeoObject::Point(point)) if point.position == Point2::new(3.0, 0.0)
    ));
}

#[test]
fn public_bound_parameter_recomputation_is_a_no_op_without_bound_changes() {
    let mut document = Document::new();
    document.set_variable("a".to_string(), 1.0);
    let mut point = PointObj::new(Point2::new(1.0, 0.0)).with_label("P");
    point.x_expr = Some("a".to_string());
    let point = document.add_object(GeoObject::Point(point));
    document.rebuild_spatial_index();
    let before = serde_json::to_value(&document).expect("document serializes");
    let version_before = document.version;
    let spatial_len_before = document.spatial.len();
    let spatial_candidates_before = document.spatial.candidates(1.0, 0.0, 0.1);

    document.recompute_bound_parameters();

    assert_eq!(document.version, version_before);
    assert!(!document.spatial_dirty);
    assert_eq!(document.spatial.len(), spatial_len_before);
    assert_eq!(
        document.spatial.candidates(1.0, 0.0, 0.1),
        spatial_candidates_before
    );
    assert_eq!(serde_json::to_value(&document).unwrap(), before);
    assert_eq!(document.point_position(point), Some(Point2::new(1.0, 0.0)));
}

#[test]
fn failed_public_bound_parameter_recomputation_leaves_live_state_and_caches_untouched() {
    let mut document = Document::new();
    document.set_variable("a".to_string(), 1.0);
    let mut bound = PointObj::new(Point2::new(0.0, 1.0)).with_label("P");
    bound.y_expr = Some("a".to_string());
    let bound = document.add_object(GeoObject::Point(bound));
    let left = document.add_object(GeoObject::Point(
        PointObj::new(Point2::new(0.0, 0.0)).with_label("Q"),
    ));
    let right = document.add_object(GeoObject::Point(
        PointObj::new(Point2::new(2.0, 0.0)).with_label("R"),
    ));
    let (circle, _) = document
        .try_add_constructed_object(
            GeoObject::Circle(CircleObj::new(Point2::new(0.0, 0.0), 1.0).with_label("C")),
            "CircleByThreePoints",
            &[bound, left, right],
        )
        .expect("non-collinear points define a circle");
    document.rebuild_spatial_index();
    document.variables.insert("a".to_string(), 0.0);
    let before = serde_json::to_value(&document).expect("document serializes");
    let bound_before = document
        .get_object(bound)
        .cloned()
        .expect("bound point exists");
    let circle_before = document
        .get_object(circle)
        .cloned()
        .expect("constructed circle exists");
    let version_before = document.version;
    let spatial_len_before = document.spatial.len();
    let spatial_candidates_before = document.spatial.candidates(0.0, 1.0, 0.1);

    document.recompute_bound_parameters();

    assert_eq!(document.version, version_before);
    assert!(!document.spatial_dirty);
    assert_eq!(document.spatial.len(), spatial_len_before);
    assert_eq!(
        document.spatial.candidates(0.0, 1.0, 0.1),
        spatial_candidates_before
    );
    assert_eq!(serde_json::to_value(&document).unwrap(), before);
    assert_eq!(document.get_object(bound), Some(&bound_before));
    assert_eq!(document.get_object(circle), Some(&circle_before));
}

#[test]
fn persistence_recomputation_propagates_bound_dependents() {
    let mut document = Document::new();
    document
        .set_spreadsheet_cell(0, 0, "0".to_string())
        .expect("seed A1");
    document
        .recompute_spreadsheet_variables()
        .expect("initial spreadsheet resolves");

    let mut point = PointObj::new(Point2::new(0.0, 0.0)).with_label("P");
    point.x_expr = Some("A1".to_string());
    let point = document.add_object(GeoObject::Point(point));
    let fixed = document.add_object(GeoObject::Point(
        PointObj::new(Point2::new(4.0, 0.0)).with_label("Q"),
    ));
    let (midpoint, _) = document
        .try_add_constructed_object(
            GeoObject::Point(PointObj::new(Point2::new(0.0, 0.0)).with_label("M")),
            "Midpoint",
            &[point, fixed],
        )
        .expect("midpoint fixture is valid");
    document
        .set_spreadsheet_cell(0, 0, "2".to_string())
        .expect("update A1 source without recomputing it");

    let serialized = serialize_document(&document).expect("stale spreadsheet document serializes");
    let loaded = deserialize_document(&serialized).expect("loading recomputes spreadsheet values");

    assert!(matches!(
        loaded.get_object(point),
        Some(GeoObject::Point(point)) if point.position == Point2::new(2.0, 0.0)
    ));
    assert!(matches!(
        loaded.get_object(midpoint),
        Some(GeoObject::Point(point)) if point.position == Point2::new(3.0, 0.0)
    ));
}

#[test]
fn failed_spreadsheet_batch_leaves_the_live_document_untouched() {
    let mut document = Document::new();
    document
        .set_spreadsheet_cell(0, 0, "1".to_string())
        .expect("seed A1");
    document
        .recompute_spreadsheet_variables()
        .expect("initial spreadsheet resolves");

    let mut bound = PointObj::new(Point2::new(0.0, 1.0)).with_label("P");
    bound.y_expr = Some("A1".to_string());
    let bound = document.add_object(GeoObject::Point(bound));
    let left = document.add_object(GeoObject::Point(
        PointObj::new(Point2::new(0.0, 0.0)).with_label("Q"),
    ));
    let right = document.add_object(GeoObject::Point(
        PointObj::new(Point2::new(2.0, 0.0)).with_label("R"),
    ));
    document
        .try_add_constructed_object(
            GeoObject::Circle(CircleObj::new(Point2::new(0.0, 0.0), 1.0).with_label("C")),
            "CircleByThreePoints",
            &[bound, left, right],
        )
        .expect("non-collinear points define a circle");
    let before = serde_json::to_value(&document).expect("document serializes");
    let version_before = document.version;

    let error = document
        .stage_spreadsheet_cell_edits(&[(0, 0, "0".to_string())])
        .expect_err("collinear scalar-bound circle inputs must reject the batch");

    assert!(error.contains("CircleByThreePoints"), "{error}");
    assert_eq!(document.version, version_before);
    assert_eq!(serde_json::to_value(&document).unwrap(), before);
    assert!(matches!(
        document.get_object(bound),
        Some(GeoObject::Point(point)) if point.position == Point2::new(0.0, 1.0)
    ));
}

#[test]
fn staged_spreadsheet_batch_rejects_out_of_bounds_sources_without_mutating_the_live_document() {
    let document = Document::new();
    let before = serde_json::to_value(&document).expect("document should serialize");

    let error = document
        .stage_spreadsheet_cell_edits(&[
            (0, 0, "1".to_string()),
            (Document::MAX_SPREADSHEET_ROWS, 0, "2".to_string()),
        ])
        .expect_err("out-of-bounds source must reject the complete batch");

    assert!(error.contains("row"), "unexpected error: {error}");
    assert_eq!(
        serde_json::to_value(&document).expect("document should serialize"),
        before
    );
}

#[test]
fn spreadsheet_cells_reject_animation_metadata_and_clear_cleanly() {
    let mut document = Document::new();
    document
        .set_spreadsheet_cell(0, 0, "1".to_string())
        .expect("fixture cell is in range");
    document
        .recompute_spreadsheet_variables()
        .expect("initial formula resolves");
    assert!(document
        .configure_variable_animation("A1", 0.0, 10.0, 1.0, AnimationMode::Loop)
        .is_err());
    assert!(document
        .try_replace_variable_meta_with_previous(
            "A1",
            VariableMeta {
                position: Point2::new(0.0, 0.0),
                min: 0.0,
                max: 10.0,
                step: 0.1,
                visible: true,
                animating: true,
                animation_speed: 1.0,
                animation_mode: AnimationMode::Loop,
            },
        )
        .is_err());

    document
        .set_spreadsheet_cell(0, 0, String::new())
        .expect("fixture cell is in range");
    document
        .recompute_spreadsheet_variables()
        .expect("cleared spreadsheet recomputes");

    assert_eq!(document.get_variable("A1"), None);
    assert!(document.variable_meta("A1").is_none());
    grafito_core::validation::validate_document(&document)
        .expect("cleared spreadsheet document remains valid");
    let serialized =
        serialize_document(&document).expect("cleared spreadsheet document serializes");
    deserialize_document(&serialized).expect("cleared spreadsheet document deserializes");
}

#[test]
fn unresolved_spreadsheet_cells_reserve_their_labels() {
    let mut document = Document::new();
    document
        .set_spreadsheet_cell(0, 0, "(".to_string())
        .expect("fixture cell is in range");
    document
        .recompute_spreadsheet_variables()
        .expect("invalid formulas are retained without a scalar value");
    let before = serde_json::to_value(&document).expect("document serializes");

    assert_eq!(document.get_variable("A1"), None);
    assert!(document.variable_meta("A1").is_none());
    assert!(document.try_set_variable("A1".to_string(), 2.0).is_err());
    assert!(document
        .configure_variable_animation("A1", 0.0, 10.0, 1.0, AnimationMode::Loop)
        .is_err());
    assert_eq!(
        serde_json::to_value(&document).expect("document serializes"),
        before
    );
    grafito_core::validation::validate_document(&document)
        .expect("invalid spreadsheet formula document remains valid");
    let serialized = serialize_document(&document).expect("invalid spreadsheet formula serializes");
    deserialize_document(&serialized).expect("invalid spreadsheet formula deserializes");
}

#[test]
fn legacy_spreadsheet_animation_metadata_is_ignored() {
    let mut document = Document::new();
    document.set_variable("phase".to_string(), 0.0);
    document
        .set_spreadsheet_cell(0, 0, "1".to_string())
        .expect("fixture cell is in range");
    document
        .recompute_spreadsheet_variables()
        .expect("fixture formula resolves");
    document
        .configure_variable_animation("phase", 0.0, 1.0, 1.0, AnimationMode::Loop)
        .expect("independent variable is animatable");

    let mut serialized: serde_json::Value =
        serde_json::from_str(&serialize_document(&document).expect("fixture document serializes"))
            .expect("fixture envelope parses");
    let mut legacy_metadata = serialized["document"]["variable_meta"]["phase"].clone();
    legacy_metadata["animation_mode"] =
        serde_json::to_value(AnimationMode::PingPong).expect("animation mode serializes");
    serialized["document"]["variable_meta"]["A1"] = legacy_metadata;
    let mut loaded =
        deserialize_document(&serde_json::to_string(&serialized).expect("legacy fixture encodes"))
            .expect("legacy spreadsheet metadata remains loadable");

    assert!(loaded.variable_meta("A1").is_some());
    assert!(loaded.advance_variable_animations(0.25));
    assert_eq!(loaded.get_variable("A1"), Some(1.0));
    assert_eq!(
        loaded
            .variable_meta("A1")
            .expect("legacy metadata remains available")
            .animation_speed,
        1.0
    );
}

#[test]
fn spreadsheet_recomputation_limit_prunes_cell_variables() {
    let mut document = Document::new();
    document
        .set_spreadsheet_cell(0, 0, "1".to_string())
        .expect("fixture cell is in range");
    document
        .recompute_spreadsheet_variables()
        .expect("initial formula resolves");

    for index in 0..=Document::MAX_SPREADSHEET_RECOMPUTE_CELLS {
        let row = index / Document::MAX_SPREADSHEET_COLS;
        let col = index % Document::MAX_SPREADSHEET_COLS;
        document
            .set_spreadsheet_cell(row, col, "1".to_string())
            .expect("fixture cell is in range");
    }

    let error = document
        .recompute_spreadsheet_variables()
        .expect_err("too many active cells reject recomputation");

    assert!(error.contains("recomputation limit"), "{error}");
    assert_eq!(document.get_variable("A1"), None);
    assert!(document.variable_meta("A1").is_none());
}

#[test]
fn legacy_variable_metadata_defaults_to_ping_pong_animation() {
    let mut document = Document::new();
    document.set_variable("phase".into(), 0.0);
    document
        .configure_variable_animation(
            "phase",
            0.0,
            std::f64::consts::TAU,
            1.0,
            AnimationMode::Loop,
        )
        .expect("fixture animation is valid");
    let mut encoded: serde_json::Value =
        serde_json::from_str(&serialize_document(&document).expect("document serializes"))
            .expect("serialized document is JSON");
    encoded["document"]["variable_meta"]["phase"]
        .as_object_mut()
        .expect("phase metadata is serialized")
        .remove("animation_mode");

    let loaded = deserialize_document(&serde_json::to_string(&encoded).unwrap())
        .expect("legacy metadata remains readable");

    assert_eq!(
        loaded
            .variable_meta("phase")
            .expect("phase metadata remains readable")
            .animation_mode,
        AnimationMode::PingPong
    );

    encoded["document"]
        .as_object_mut()
        .expect("document envelope is an object")
        .remove("variable_meta");
    let loaded_without_metadata = deserialize_document(&serde_json::to_string(&encoded).unwrap())
        .expect("legacy documents without metadata remain readable");

    assert_eq!(loaded_without_metadata.get_variable("phase"), Some(0.0));
    assert!(loaded_without_metadata.variable_meta("phase").is_none());
}

#[test]
fn local_data_tables_and_linked_fits_round_trip_without_a_source_path() {
    let mut document = Document::new();
    let table = DataTableObj::new("time", "distance", vec![0.0, 1.0, 2.0], vec![1.0, 3.0, 5.0]);
    let table_id = table.id;
    document
        .try_add_object(GeoObject::DataTable(table))
        .expect("finite local table is valid");

    let fit =
        fit_xy(FitKind::Linear, &[0.0, 1.0, 2.0], &[1.0, 3.0, 5.0]).expect("fixture fit succeeds");
    let function = FunctionObj::new(fit.expression())
        .with_label("fit")
        .with_fit(FitMetadata::from_result(table_id, fit));
    let function_id = document
        .try_add_object(GeoObject::Function(function))
        .expect("linked fit is valid");

    let serialized = serialize_document(&document).expect("local analysis serializes");
    assert!(!serialized.contains("source_path"));
    assert!(!serialized.contains("/tmp/"));
    let loaded = deserialize_document(&serialized).expect("local analysis deserializes");

    assert!(
        matches!(loaded.get_object(table_id), Some(GeoObject::DataTable(table)) if table.x_name == "time" && table.y_name == "distance")
    );
    assert!(
        matches!(loaded.get_object(function_id), Some(GeoObject::Function(function)) if function.fit.as_ref().is_some_and(|fit| fit.source == table_id && fit.diagnostics.residuals.len() == 3))
    );

    let mut loaded = loaded;
    loaded.remove_object(table_id);
    assert!(
        loaded.get_object(function_id).is_none(),
        "deleting data cascades to its fit"
    );
}

#[test]
fn persistence_rejects_an_orphaned_dynamic_locus_binding() {
    let mut document = Document::new();
    let driver = document.add_object(GeoObject::Point(
        PointObj::new(Point2::new(0.0, 0.0)).with_label("A"),
    ));
    let target = document.add_object(GeoObject::Point(
        PointObj::new(Point2::new(1.0, 0.0)).with_label("B"),
    ));
    let orphan = PencilObj::new(vec![Point2::new(1.0, 0.0)]).with_locus_binding(driver, target);
    document
        .try_add_object(GeoObject::Pencil(orphan))
        .expect("references exist before document-level validation");

    let error = serialize_document(&document)
        .expect_err("a dynamic locus without its Locus constraint must not persist");
    assert!(
        error.to_string().contains("matching Locus constraint"),
        "{error}"
    );
}

#[test]
fn persistence_rejects_a_locus_constraint_with_the_same_driver_and_target() {
    let mut document = Document::new();
    let driver = document.add_object(GeoObject::Point(
        PointObj::new(Point2::new(0.0, 0.0)).with_label("A"),
    ));
    let target = document.add_object(GeoObject::Point(
        PointObj::new(Point2::new(1.0, 0.0)).with_label("B"),
    ));
    let (locus, _) = document
        .try_add_locus(driver, target)
        .expect("fixture locus should be valid");
    let mut encoded: serde_json::Value =
        serde_json::from_str(&serialize_document(&document).expect("fixture serializes"))
            .expect("fixture is JSON");
    let driver_json = serde_json::to_value(driver).expect("driver id serializes");
    encoded["document"]["objects"][locus.0.to_string()]["Pencil"]["locus_binding"]["target"] =
        driver_json.clone();
    let constraints = encoded["document"]["constraints"]["constraints"]
        .as_object_mut()
        .expect("constraints are serialized as a map");
    let constraint = constraints
        .values_mut()
        .find(|constraint| constraint["name"] == "Locus")
        .expect("fixture has one Locus constraint");
    constraint["inputs"] = serde_json::json!([driver, driver]);

    let error = deserialize_document(&serde_json::to_string(&encoded).unwrap())
        .expect_err("a self-referential locus must not deserialize");
    assert!(error.to_string().contains("distintos"), "{error}");
}
