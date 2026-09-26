//! Smoke de ejecución (Ola 0.1): cada spec registrado se ejecuta con su
//! ejemplo mínimo (`command_registry::minimal_example`) sobre un documento vacío.
//!
//! Garantía: ningún handler entra en pánico, ningún comando queda huérfano
//! ("no reconocido") y ningún comando falla con "no implementado" salvo las
//! listas pinneadas de abajo.
//!
//! Fallar por aridad, falta de objetos, dominio o recursos ES honesto y el
//! test lo acepta.
//!
//! Nota de I/O: se verificó que ningún spec ejecuta I/O real con su ejemplo
//! mínimo (los únicos 3 con argumento `Path` —SetImage, PlaySound,
//! ExportImage— fallan honesto antes de tocar disco, y están pinneados abajo).
//! Si un futuro comando hiciera I/O con args mínimos, este test lo ejecutaría:
//! en ese caso hay que pinnearlo acá con motivo en vez de dejar el efecto
//! colateral en silencio.
//!
//! Si este test falla con un comando nuevo fuera de las listas, hay tres
//! opciones: implementar el comando, declararlo error honesto permanente
//! (pinnearlo con motivo) o corregir el bug. Nunca agrandar listas en silencio.

use grafito_command::command_registry;
use grafito_command::commands::{process_input, CommandOutcome};
use grafito_core::Document;

/// Comandos que siempre responden "no implementado / no soportado" por diseño
/// (contrato `p4_errores_honestos_permanentes`).
/// La Ola 0.3 implementó Delete/StartAnimation/StopAnimation (12 → 9).
const PINNED_UNIMPLEMENTED: &[&str] = &[
    "SetVisibleInView",
    "SetImage",
    "ToolImage",
    "PlaySound",
    "StartRecord",
    "SlowPlot",
    "ExportImage",
    "ConstructionStep",
    "SetConstructionStep",
];

/// Comandos implementados solo para un subconjunto de tipos: con el ejemplo
/// mínimo responden "no soportado" para ESE tipo, pero computan para otros.
/// Cada entrada documenta el porqué; achicar esta lista es trabajo de paridad.
const KNOWN_PARTIAL: &[&str] = &[
    // Shear rechaza el círculo (su imagen real es una elipse): registry `shear`.
    "Shear",
    // EDOs con coeficientes variables fuera del subset F3c (solo constantes,
    // Euler 2.º y Frobenius); la Ola 2.5 amplía el subset.
    "SolveODE2",
    "ODESystem2",
    "EulerODE",
    // Laplace directa/inversa cubre la tabla F3c; fuera de tabla, error honesto.
    // La Ola 2.6 amplía la tabla.
    "LaplaceT",
    "InvLaplaceT",
    "LaplaceInt",
    // SolveODEN solo orden 1..=8 con coeficientes constantes.
    "SolveODEN",
];

/// Fragmentos (en minúsculas) que indican "nada se computa".
/// Nota: "no soportado"/"not supported" NO están acá a propósito: también los
/// usan errores honestos de dominio ("color no soportado", "objeto no
/// soportado"). Esos casos se pinnean en KNOWN_PARTIAL si son subset real.
fn is_unimplemented_message(message: &str) -> bool {
    let lower = message.to_lowercase();
    [
        "no implementado",
        "no implementada",
        "not implemented",
        "unsupported",
    ]
    .iter()
    .any(|fragment| lower.contains(fragment))
}

#[test]
fn every_command_executes_without_panic_or_orphan() {
    let mut orphans = Vec::new();
    let mut surprise_unimplemented = Vec::new();
    let mut executed = 0usize;

    for spec in command_registry::all() {
        let mut document = Document::new();
        let mut input = command_registry::minimal_example(spec);
        executed += 1;
        // Un pánico acá falla el test: es exactamente lo que se quiere cazar.
        let outcome = process_input(&mut document, &mut input);
        if let CommandOutcome::Error(message) = &outcome {
            if message.contains("no reconocido") {
                orphans.push(format!("{}: {message}", spec.canonical));
            } else if is_unimplemented_message(message)
                && !PINNED_UNIMPLEMENTED.contains(&spec.canonical)
                && !KNOWN_PARTIAL.contains(&spec.canonical)
            {
                surprise_unimplemented.push(format!("{}: {message}", spec.canonical));
            }
        }
    }

    assert!(
        orphans.is_empty(),
        "comandos huérfanos ({}): {orphans:#?}",
        orphans.len()
    );
    assert!(
        surprise_unimplemented.is_empty(),
        "comandos con 'no implementado' fuera de las listas ({}): {surprise_unimplemented:#?}",
        surprise_unimplemented.len()
    );
    assert!(
        executed > 600,
        "el smoke ejecutó muy pocos comandos ({executed}): ¿cambió el registro?"
    );
}

/// Argumento mínimo válido por tipo de firma (presupuesto: todo queda
/// muy por debajo de MAX_EXPR_LENGTH 2000).
fn humo_arg_para(kind: command_registry::ArgumentKind) -> &'static str {
    match kind {
        command_registry::ArgumentKind::Expression => "x+1",
        command_registry::ArgumentKind::ComplexExpression => "z+1",
        command_registry::ArgumentKind::Variable => "x",
        command_registry::ArgumentKind::Number => "1",
        command_registry::ArgumentKind::Integer => "2",
        command_registry::ArgumentKind::Point => "(0, 0)",
        command_registry::ArgumentKind::Object => "A",
        command_registry::ArgumentKind::ObjectLabel => "A",
        command_registry::ArgumentKind::Vector => "(1, 2)",
        command_registry::ArgumentKind::Curve => "A",
        command_registry::ArgumentKind::Matrix => "[[1, 2], [3, 4]]",
        command_registry::ArgumentKind::Data => "{1, 2, 3}",
        command_registry::ArgumentKind::Path => "\"/tmp/humo.txt\"",
        command_registry::ArgumentKind::Domain => "[0, 1]",
        command_registry::ArgumentKind::Relation => "=",
        command_registry::ArgumentKind::ParameterList => "[x, y]",
        command_registry::ArgumentKind::Unspecified => "1",
    }
}

/// Invocación mínima y acotada para un spec: pesados con grilla chica
/// explícita, resto con la firma de menos requeridos y solo esos args.
fn humo_invocacion(spec: &command_registry::CommandSpec) -> String {
    match spec.canonical {
        // Fractales: max_iter chico (default 256, grilla pesada).
        "Mandelbrot" => return "Mandelbrot[5]".to_string(),
        "Julia" => return "Julia[0, 0, 5]".to_string(),
        // Grillas 2D: resolución/densidad mínima válida (16 o 2).
        "DomainColoring" => return "DomainColoring[z+1, -1, 1, -1, 1, 16]".to_string(),
        "ComplexGrid" => return "ComplexGrid[z+1, -1, 1, -1, 1, 2]".to_string(),
        "HeatMap" => return "HeatMap[x+y, -1, 1, -1, 1, 16]".to_string(),
        "ComplexSurface" => return "ComplexSurface[z+1, -1, 1, -1, 1, 16]".to_string(),
        // Un solo nivel, bounds chicos.
        "Contour" => return "Contour[x+y, -1, 1, -1, 1, 0]".to_string(),
        // res mínima válida 8..32, cubo chico.
        "ImplicitSurface" => {
            return "ImplicitSurface[x^2+y^2+z^2-1, -1, 1, -1, 1, -1, 1, 8]".to_string();
        }
        // Aridad real del handler (el registro declara 1 punto, pide 3).
        "Polygon" => return "Polygon[(0, 0), (1, 0), (0, 1)]".to_string(),
        "Polyline" => return "Polyline[(0, 0), (1, 0)]".to_string(),
        // Politopos 4D: forma vacía o dimensión mínima, sin rotaciones.
        "Pentachoron4D" | "Tesseract4D" | "SixteenCell4D" | "TwentyFourCell4D"
        | "OneTwentyCell4D" | "SixHundredCell4D" | "Hypercube" | "Hypersphere" | "BurningShip" => {
            return format!("{}[]", spec.canonical)
        }
        "SimplexND" | "HypercubeND" | "CrossPolytopeND" => {
            return format!("{}[3]", spec.canonical);
        }
        _ => {}
    }
    let mut mejor: Option<&command_registry::CommandSignature> = None;
    let mut mejor_requeridos = usize::MAX;
    for firma in spec.signatures {
        let requeridos = firma.arguments.iter().filter(|arg| !arg.optional).count();
        if requeridos < mejor_requeridos {
            mejor_requeridos = requeridos;
            mejor = Some(firma);
        }
    }
    let Some(firma) = mejor else {
        return format!("{}[]", spec.canonical);
    };
    let mut args = Vec::new();
    for arg in firma.arguments.iter().filter(|arg| !arg.optional) {
        args.push(humo_arg_para(arg.kind));
    }
    format!("{}[{}]", spec.canonical, args.join(", "))
}

#[test]
fn execution_smoke() {
    let total = command_registry::all().len();
    let mut ejecutados = 0usize;
    let mut panicos = Vec::new();

    for spec in command_registry::all() {
        let texto = humo_invocacion(spec);
        assert!(
            texto.len() <= 2000,
            "humo: '{}' excede MAX_EXPR_LENGTH 2000 con '{}'",
            spec.canonical,
            texto
        );
        ejecutados += 1;
        // Document fresco por comando, headless (Document::new sin egui/wgpu).
        // Error honesto vale (stubs sin motor, dominios); solo el pánico falla.
        let resultado = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            let mut documento = Document::new();
            let mut entrada = texto.clone();
            let salida = process_input(&mut documento, &mut entrada);
            matches!(
                salida,
                CommandOutcome::Ok | CommandOutcome::Message(_) | CommandOutcome::Error(_)
            )
        }));
        if resultado.is_err() {
            panicos.push(spec.canonical);
        }
    }

    assert!(
        panicos.is_empty(),
        "humo: {} comandos paniquearon: {panicos:?}, mirá el canonical listado",
        panicos.len()
    );
    assert_eq!(
        ejecutados, total,
        "humo: se ejecutaron {ejecutados} de {total}, falta cobertura del registro"
    );
    assert!(
        total >= 733,
        "humo: el registro trae {total}, se esperaban al menos 733 (¿se borraron comandos?)"
    );
}
