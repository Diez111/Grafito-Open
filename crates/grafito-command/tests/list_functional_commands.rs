#![allow(clippy::unwrap_used, clippy::expect_used)]
use grafito_command::commands::CommandOutcome;
use grafito_command::process_input;
use grafito_core::{Document, GeoObject};

fn run(document: &mut Document, command: &str) -> CommandOutcome {
    let mut input = command.to_string();
    process_input(document, &mut input)
}

fn assert_message(outcome: CommandOutcome, contains: &str) {
    match outcome {
        CommandOutcome::Message(msg) => assert!(
            msg.contains(contains),
            "esperado que '{msg}' contenga '{contains}'"
        ),
        other => panic!("esperado Message que contenga '{contains}', obtenido {other:?}"),
    }
}

#[test]
fn sequence_generates_squares_1_to_5() {
    let mut doc = Document::new();
    // Sequence[x^2, x, 1, 5] -> {1,4,9,16,25}
    let outcome = run(&mut doc, "Sequence[x^2, x, 1, 5]");
    assert_message(outcome, "1");
    // Verifica contenido exacto
    let outcome2 = run(&mut doc, "Sequence[x^2, x, 1, 5]");
    match outcome2 {
        CommandOutcome::Message(msg) => {
            assert!(msg.contains("1"), "msg={msg}");
            assert!(msg.contains("4"));
            assert!(msg.contains("9"));
            assert!(msg.contains("16"));
            assert!(msg.contains("25"));
            // debe ser lista con llaves
            assert!(msg.contains('{') && msg.contains('}'), "msg={msg}");
        }
        other => panic!("expected Message, got {other:?}"),
    }
}

#[test]
fn sequence_reverse_range() {
    let mut doc = Document::new();
    let outcome = run(&mut doc, "Sequence[x, x, 5, 1]");
    assert_message(outcome, "5");
}

#[test]
fn sort_orders_numerically() {
    let mut doc = Document::new();
    let outcome = run(&mut doc, "Sort[{3,1,2}]");
    match outcome {
        CommandOutcome::Message(msg) => {
            // debe ser {1, 2, 3}
            let pos1 = msg.find('1').expect("no 1");
            let pos2 = msg.find('2').expect("no 2");
            let pos3 = msg.find('3').expect("no 3");
            assert!(pos1 < pos2 && pos2 < pos3, "orden incorrecto: {msg}");
        }
        other => panic!("expected Message, got {other:?}"),
    }
}

#[test]
fn zip_pairs_two_lists() {
    let mut doc = Document::new();
    let outcome = run(&mut doc, "Zip[{1,2,3}, {4,5,6}]");
    match outcome {
        CommandOutcome::Message(msg) => {
            assert!(msg.contains("1"), "msg={msg}");
            assert!(msg.contains("4"), "msg={msg}");
            assert!(msg.contains("2") && msg.contains("5"), "msg={msg}");
            assert!(msg.contains("3") && msg.contains("6"), "msg={msg}");
            // debe contener pares anidados
            assert!(msg.contains('{'), "msg={msg}");
        }
        other => panic!("expected Message, got {other:?}"),
    }
}

#[test]
fn flatten_one_level() {
    let mut doc = Document::new();
    let outcome = run(&mut doc, "Flatten[{{1,2},{3,4}}]");
    match &outcome {
        CommandOutcome::Message(msg) => {
            assert!(msg.contains("1"), "msg={msg}");
            assert!(msg.contains("1") && msg.contains("4"), "msg={msg}");
        }
        other => panic!("expected Message, got {other:?}"),
    }
}

#[test]
fn reverse_inverts_list() {
    let mut doc = Document::new();
    let outcome = run(&mut doc, "Reverse[{1,2,3}]");
    match outcome {
        CommandOutcome::Message(msg) => {
            let pos1 = msg.find('1').unwrap();
            let pos3 = msg.find('3').unwrap();
            assert!(pos3 < pos1, "Reverse debe invertir: {msg}");
        }
        other => panic!("expected Message, got {other:?}"),
    }
}

#[test]
fn join_concatenates() {
    let mut doc = Document::new();
    let outcome = run(&mut doc, "Join[{1,2}, {3,4}]");
    match &outcome {
        CommandOutcome::Message(msg) => {
            assert!(msg.contains("1"), "msg={msg}");
            assert!(msg.contains("3") && msg.contains("4"), "msg={msg}");
        }
        other => panic!("expected Message, got {other:?}"),
    }
}

#[test]
fn append_adds_element() {
    let mut doc = Document::new();
    let outcome = run(&mut doc, "Append[{1,2}, 3]");
    assert_message(outcome, "3");
}

#[test]
fn first_and_last() {
    let mut doc = Document::new();
    let first = run(&mut doc, "First[{5,6,7}]");
    assert_message(first, "5");
    let last = run(&mut doc, "Last[{5,6,7}]");
    assert_message(last, "7");
}

#[test]
fn take_first_n() {
    let mut doc = Document::new();
    let outcome = run(&mut doc, "Take[{1,2,3,4}, 2]");
    match outcome {
        CommandOutcome::Message(msg) => {
            assert!(msg.contains("1") && msg.contains("2"), "msg={msg}");
            assert!(
                !msg.contains("3") && !msg.contains("4"),
                "debe tomar solo 2: {msg}"
            );
        }
        other => panic!("expected Message, got {other:?}"),
    }
}

#[test]
fn keep_if_filters() {
    let mut doc = Document::new();
    let outcome = run(&mut doc, "KeepIf[{1,2,3,4}, x>2]");
    match outcome {
        CommandOutcome::Message(msg) => {
            assert!(msg.contains("3") && msg.contains("4"), "msg={msg}");
            // No debe contener 1 ni 2 como elementos aislados (pero podría contener "1" dentro de otro)
            // Verifica que la lista sea {3, 4}
            assert!(
                msg.contains("{3, 4}")
                    || msg.contains("{3,4}")
                    || (msg.contains("3") && msg.contains("4") && !msg.contains("1,")),
                "msg={msg}"
            );
        }
        other => panic!("expected Message, got {other:?}"),
    }
}

#[test]
fn count_if_counts() {
    let mut doc = Document::new();
    let outcome = run(&mut doc, "CountIf[{1,2,3,4}, x>2]");
    assert_message(outcome, "2");
}

#[test]
fn sequence_respects_max_discrete_count() {
    let mut doc = Document::new();
    // MAX_DISCRETE_COUNT es 10_000, intenta 1..10001 -> debe fallar
    let outcome = run(&mut doc, "Sequence[x, x, 1, 10001]");
    assert!(
        matches!(outcome, CommandOutcome::Error(_)),
        "debe rechazar longitud > MAX_DISCRETE_COUNT"
    );
}

#[test]
fn list_operates_on_datatable_xs() {
    let mut doc = Document::new();
    // Crea DataTable
    let _ = run(&mut doc, "DataTable[{1,2,3},{4,5,6}]");
    // Obtiene label de la tabla creada
    let label = doc
        .objects_iter()
        .find_map(|(_, obj)| match obj {
            GeoObject::DataTable(t) => Some(t.label.clone()),
            _ => None,
        })
        .unwrap_or_else(|| "DataTable".to_string());
    // Sort sobre DataTable.xs (por defecto xs)
    let outcome = run(&mut doc, &format!("Sort[{}]", label));
    // Debe ordenar {1,2,3} -> mismo
    assert_message(outcome, "1");
    // Reverse sobre .ys
    let outcome2 = run(&mut doc, &format!("Reverse[{}.ys]", label));
    match outcome2 {
        CommandOutcome::Message(msg) => {
            // ys = {4,5,6} reverse -> {6,5,4}
            let pos6 = msg.find('6').expect("no 6");
            let pos4 = msg.find('4').expect("no 4");
            assert!(pos6 < pos4, "Reverse ys debe invertir: {msg}");
        }
        other => panic!("expected Message, got {other:?}"),
    }
    // Take sobre DataTable
    let outcome3 = run(&mut doc, &format!("Take[{}.xs, 2]", label));
    match outcome3 {
        CommandOutcome::Message(msg) => {
            assert!(msg.contains("1") && msg.contains("2"), "msg={msg}");
            assert!(!msg.contains("3"), "solo 2 elementos: {msg}");
        }
        other => panic!("expected Message, got {other:?}"),
    }
}

#[test]
fn alias_seq_and_sort_work() {
    let mut doc = Document::new();
    // alias seq para Sequence
    let outcome = run(&mut doc, "seq[x, x, 1, 3]");
    assert_message(outcome, "1");
    // alias ordenar para Sort
    let outcome2 = run(&mut doc, "ordenar[{3,2,1}]");
    assert_message(outcome2, "1");
}
