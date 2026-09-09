#![allow(clippy::unwrap_used, clippy::expect_used)]
//! R3.5 `BarChart`/`PieChart` desde `DataTable` — cierran el stub que derivaba
//! a Histogram en `exchange.rs`. E2E vía `process_input`: tabla → barras/torta
//! reales desde rango de planilla + comando visible + validación honesta.

use grafito_command::command_registry;
use grafito_command::commands::{find_object_by_label, process_input, CommandOutcome};
use grafito_core::{Document, GeoObject};

fn run(document: &mut Document, command: &str) -> CommandOutcome {
    process_input(document, &mut command.to_string())
}

fn doc_con_tabla() -> (Document, String) {
    let mut doc = Document::new();
    let outcome = run(&mut doc, "DataTable[{1, 2, 3}, {2, 4, 6}]");
    assert!(
        matches!(outcome, CommandOutcome::Ok | CommandOutcome::Message(_)),
        "{outcome:?}"
    );
    let tabla = doc
        .objects_iter()
        .find_map(|(_, o)| matches!(o, GeoObject::DataTable(_)).then(|| o.label().to_string()))
        .expect("tabla");
    (doc, tabla)
}

#[test]
fn charts_siguen_visibles_con_firma_tabla() {
    for (canon, firma) in [
        ("BarChart", "BarChart[tabla]"),
        ("PieChart", "PieChart[tabla]"),
    ] {
        let spec = command_registry::resolve(canon).expect("registrado");
        assert!(spec.palette_visible, "{canon}: REGLA DE ORO paleta visible");
        assert!(
            spec.signatures.iter().any(|s| s.syntax == firma),
            "{canon} debe exponer {firma}"
        );
    }
}

#[test]
fn barchart_desde_tabla_usa_ys() {
    let (mut doc, tabla) = doc_con_tabla();
    let outcome = run(&mut doc, &format!("BarChart[{tabla}]"));
    assert!(matches!(outcome, CommandOutcome::Message(_)), "{outcome:?}");
    let hay_ys = doc.objects_iter().any(|(_, o)| match o {
        GeoObject::BarChart(b) => b.data == vec![2.0, 4.0, 6.0],
        _ => false,
    });
    assert!(hay_ys, "columna y por defecto");
    // Sufijo explícito .xs.
    let outcome = run(&mut doc, &format!("BarChart[{tabla}.xs]"));
    assert!(matches!(outcome, CommandOutcome::Message(_)), "{outcome:?}");
    let hay_xs = doc.objects_iter().any(|(_, o)| match o {
        GeoObject::BarChart(b) => b.data == vec![1.0, 2.0, 3.0],
        _ => false,
    });
    assert!(hay_xs, "columna xs explícita");
}

#[test]
fn piechart_desde_tabla_y_errores_honestos() {
    let (mut doc, tabla) = doc_con_tabla();
    let outcome = run(&mut doc, &format!("PieChart[{tabla}.ys]"));
    assert!(matches!(outcome, CommandOutcome::Message(_)), "{outcome:?}");
    let torta = doc
        .objects_iter()
        .filter_map(|(_, o)| match o {
            GeoObject::PieChart(p) => Some(p.data.clone()),
            _ => None,
        })
        .last()
        .expect("torta creada");
    assert_eq!(torta, vec![2.0, 4.0, 6.0]);
    // Etiqueta que no es tabla: honesto, sin objeto inventado.
    let antes = doc.object_count();
    let outcome = run(&mut doc, "PieChart[ZZZ_no_existe]");
    assert!(
        matches!(&outcome, CommandOutcome::Error(m) if m.contains("DataTable")),
        "{outcome:?}"
    );
    assert_eq!(doc.object_count(), antes);
    // Literal clásico sigue andando.
    assert!(matches!(
        run(&mut doc, "BarChart[{1, 2, 3}]"),
        CommandOutcome::Message(_)
    ));
    let _ = find_object_by_label(&doc, &tabla);
}
