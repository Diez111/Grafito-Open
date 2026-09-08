#![allow(clippy::unwrap_used, clippy::expect_used)]
//! Frente C2 (planilla + probabilidad): serie lineal/geométrica,
//! ajustes con R² de referencia e inversas T/Chi²/F por comando.
//!
//! Lo que YA existía y NO se duplicó (blindaje explícito abajo):
//! FillColumn/FillRow/FillCells, FitLinear/FitPoly/FitExp/FitLog/FitPow/
//! FitSin, Normal/Binomial/Poisson, InverseNormal/InverseT/
//! InverseChiSquared/InverseF, TTest/TTest2/TTestPaired/ZTest/ChiSqTest.

use grafito_command::command_registry;
use grafito_command::commands::{process_input, CommandOutcome};
use grafito_core::{Document, GeoObject};

fn run(document: &mut Document, command: &str) -> CommandOutcome {
    process_input(document, &mut command.to_string())
}

fn table_label(document: &Document) -> String {
    document
        .objects_iter()
        .find_map(|(_, object)| match object {
            GeoObject::DataTable(table) => Some(table.label.clone()),
            _ => None,
        })
        .expect("la tabla debe existir")
}

fn message_text(outcome: CommandOutcome) -> String {
    match outcome {
        CommandOutcome::Message(text) => text,
        other => panic!("se esperaba Message, llegó {other:?}"),
    }
}

/// Número tras el último `=` del mensaje (`InverseT[..] = 2.228139`,
/// `... R²=0.666...` usa `r_squared_after`).
fn number_after_equals(message: &str) -> f64 {
    message
        .rsplit('=')
        .next()
        .expect("mensaje con =")
        .trim()
        .parse::<f64>()
        .expect("número tras =")
}

fn r_squared_after(message: &str) -> f64 {
    let (_, tail) = message.split_once("R²=").expect("mensaje de ajuste con R²");
    tail.trim().parse::<f64>().expect("R² numérico")
}

#[test]
fn fill_series_linear_fills_column_range() {
    let mut document = Document::new();
    let text = message_text(run(&mut document, "FillSeries[A1:A5, 1, 2]"));
    assert!(text.contains("5 celdas"), "{text}");
    assert!(text.contains("lineal"), "{text}");
    for (row, expected) in [1.0, 3.0, 5.0, 7.0, 9.0].iter().enumerate() {
        assert_eq!(
            document.eval_spreadsheet_cell(row, 0),
            Some(*expected),
            "A{}",
            row + 1
        );
    }
}

#[test]
fn fill_series_geometric_fills_row_with_explicit_mode() {
    let mut document = Document::new();
    let text = message_text(run(&mut document, "FillSeries[B2:D2, 2, 3, geom]"));
    assert!(text.contains("3 celdas"), "{text}");
    for (col, expected) in [2.0, 6.0, 18.0].iter().enumerate() {
        assert_eq!(
            document.eval_spreadsheet_cell(1, col + 1),
            Some(*expected),
            "col {}",
            col + 2
        );
    }
    // Modo por defecto = lineal, con comillas y decimales.
    let mut plain = Document::new();
    let text = message_text(run(&mut plain, "FillSeries[\"A1:A3\", 0, 0.5]"));
    assert!(text.contains("lineal"), "{text}");
    assert_eq!(plain.eval_spreadsheet_cell(0, 0), Some(0.0));
    assert_eq!(plain.eval_spreadsheet_cell(1, 0), Some(0.5));
    assert_eq!(plain.eval_spreadsheet_cell(2, 0), Some(1.0));
}

#[test]
fn fill_series_rejects_and_writes_nothing_partially() {
    let mut document = Document::new();
    let before = serde_json::to_value(&document).unwrap();
    // Rectángulo 2D: solo rangos 1D.
    let outcome = run(&mut document, "FillSeries[A1:B2, 1, 1]");
    assert!(matches!(outcome, CommandOutcome::Error(_)), "{outcome:?}");
    // Modo inválido.
    let outcome = run(&mut document, "FillSeries[A1:A3, 1, 1, cuad]");
    assert!(matches!(outcome, CommandOutcome::Error(_)), "{outcome:?}");
    // Aridad inválida.
    let outcome = run(&mut document, "FillSeries[A1]");
    assert!(matches!(outcome, CommandOutcome::Error(_)), "{outcome:?}");
    // Celda inválida.
    let outcome = run(&mut document, "FillSeries[A0:A3, 1, 1]");
    assert!(matches!(outcome, CommandOutcome::Error(_)), "{outcome:?}");
    assert_eq!(
        serde_json::to_value(&document).unwrap(),
        before,
        "error de serie no debe mutar la planilla"
    );
    assert_eq!(document.eval_spreadsheet_cell(0, 0), None);
}

#[test]
fn fit_linear_reports_r_squared_one_on_perfect_line() {
    // Recta perfecta y = 2x + 1.
    let mut document = Document::new();
    assert!(matches!(
        run(&mut document, "DataTable[{0,1,2,3}, {1,3,5,7}]"),
        CommandOutcome::Message(_)
    ));
    let label = table_label(&document);
    let text = message_text(run(&mut document, &format!("FitLinear[{label}]")));
    assert!(text.contains("R²=1.000000"), "{text}");
    assert!(text.contains("RMSE=0.000000"), "{text}");
    assert!(document.objects_iter().any(|(_, object)| {
        matches!(object, GeoObject::Function(function) if function.fit.is_some())
    }));
}

#[test]
fn fit_linear_matches_anscombe_reference() {
    // Anscombe I: pendiente 0.5, ordenada 3.0, R² ≈ 0.667.
    let mut document = Document::new();
    assert!(matches!(
        run(
            &mut document,
            "DataTable[{10,8,13,9,11,14,6,4,12,7,5}, {8.04,6.95,7.58,8.81,8.33,9.96,7.24,4.26,10.84,4.82,5.68}]"
        ),
        CommandOutcome::Message(_)
    ));
    let label = table_label(&document);
    let text = message_text(run(&mut document, &format!("FitLinear[{label}]")));
    let r_squared = r_squared_after(&text);
    assert!(
        (r_squared - 0.666_7).abs() < 0.005,
        "R² Anscombe I = {r_squared}"
    );
    // El motor reporta pendiente/ordenada de referencia por la misma vía
    // (`predict` desnormaliza el dominio interno; los coeficientes crudos
    // viven en x normalizada y no son y = a·x + b directos).
    let fit = grafito_geometry::statistics::fit_xy(
        grafito_geometry::statistics::FitKind::Linear,
        &[10.0, 8.0, 13.0, 9.0, 11.0, 14.0, 6.0, 4.0, 12.0, 7.0, 5.0],
        &[
            8.04, 6.95, 7.58, 8.81, 8.33, 9.96, 7.24, 4.26, 10.84, 4.82, 5.68,
        ],
    )
    .expect("ajuste Anscombe I");
    let slope = (fit.predict(11.0) - fit.predict(9.0)) / 2.0;
    assert!((slope - 0.5).abs() < 0.01, "pendiente = {slope}");
    assert!((fit.predict(0.0) - 3.0).abs() < 0.05, "ordenada");
}

#[test]
fn inverse_commands_match_textbook_quantiles() {
    let mut document = Document::new();
    // t_{0.975,10} ≈ 2.2281.
    let text = message_text(run(&mut document, "InverseT[0.975, 10]"));
    let q = number_after_equals(&text);
    assert!((q - 2.228_1).abs() < 1e-3, "{text}");
    // χ²_{0.95,5} ≈ 11.0705.
    let text = message_text(run(&mut document, "InverseChiSquared[0.95, 5]"));
    let q = number_after_equals(&text);
    assert!((q - 11.070_5).abs() < 1e-2, "{text}");
    // F_{0.95,5,10} ≈ 3.3258.
    let text = message_text(run(&mut document, "InverseF[0.95, 5, 10]"));
    let q = number_after_equals(&text);
    assert!((q - 3.325_8).abs() < 1e-2, "{text}");
}

#[test]
fn c2_does_not_duplicate_existing_commands() {
    // Canónicos existentes que el frente pidió verificar: no se agregaron
    // alias duplicados (FillDown→FillColumn, ChiSquareTest→ChiSqTest,
    // InverseChiSquare→InverseChiSquared).
    // Oleada 1 P2 EXCEPTÚA FitLine/Fit/FitLineX: ahora son S visibles con
    // dispatch_key FitLinear + brazo delegante + e2e (tests/oleada1.rs),
    // no fantasmas (ver registry_counts 309/263).
    for existing in [
        "FitLinear",
        "FillColumn",
        "FillRow",
        "ChiSqTest",
        "InverseChiSquared",
        "InverseT",
        "InverseF",
        "TTest",
        "FillSeries",
    ] {
        assert!(
            command_registry::resolve(existing).is_some(),
            "{existing} debe seguir registrado"
        );
    }
    for duplicate in ["FillDown", "ChiSquareTest", "InverseChiSquare"] {
        assert!(
            command_registry::resolve(duplicate).is_none(),
            "{duplicate} sería un duplicado fantasma"
        );
    }
}
