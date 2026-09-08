#![allow(clippy::unwrap_used, clippy::expect_used)]
#![allow(deprecated)]
use grafito_command::{
    command_registry::{self, ArgumentKind, MutationClass, RiskLevel},
    commands::{parse_cas_command, process_input, CommandOutcome},
};
use grafito_core::Document;
use std::collections::HashSet;

#[test]
fn registry_metadata_is_unique_and_complete() {
    let mut ids = HashSet::new();
    let mut canonicals = HashSet::new();

    for spec in command_registry::all() {
        assert!(ids.insert(spec.id), "duplicate command id: {}", spec.id);
        assert!(
            canonicals.insert(spec.canonical.to_ascii_lowercase()),
            "duplicate canonical command: {}",
            spec.canonical
        );
        assert!(!spec.signatures.is_empty(), "{} needs a signature", spec.id);
        assert!(!spec.help.is_empty(), "{} needs help", spec.id);
        assert!(!spec.category.is_empty(), "{} needs a category", spec.id);
        assert!(
            !spec.dispatch_key.is_empty(),
            "{} needs a handler key",
            spec.id
        );
        assert!(!spec.insertion.is_empty(), "{} needs an insertion", spec.id);
        assert!(
            spec.insertion.starts_with(spec.canonical),
            "{} insertion must start with its canonical name",
            spec.id
        );

        for signature in spec.signatures {
            assert!(
                signature.syntax.starts_with(spec.canonical),
                "{} signature must use the canonical command name",
                spec.id
            );
            for argument in signature.arguments {
                assert!(
                    !argument.name.is_empty(),
                    "{} has an unnamed argument",
                    spec.id
                );
                assert_ne!(
                    argument.kind,
                    ArgumentKind::Unspecified,
                    "{} has an untyped argument",
                    spec.id
                );
            }
        }

        assert_ne!(
            spec.mutation,
            MutationClass::Unclassified,
            "{} needs a mutation classification",
            spec.id
        );
        assert_ne!(
            spec.risk,
            RiskLevel::Unclassified,
            "{} needs a risk classification",
            spec.id
        );
    }
}

#[test]
fn registry_covers_stable_palette_and_documentation_commands() {
    for canonical in [
        "Point",
        "Circle",
        "Function",
        "Ellipse",
        "Distance",
        "Derivative",
        "Integral",
        "PolygonUnion",
        "ComplexMapping",
        "JacobianMatrix",
        "Determinant",
        "Thomas",
        "Mandelbrot",
        "Segment3D",
        "Tetrahedron",
        "Surface3D",
    ] {
        assert!(
            command_registry::resolve(canonical).is_some(),
            "{canonical} must have authoritative metadata"
        );
    }
}

#[test]
fn assistant_graph_commands_are_all_backed_by_registered_metadata() {
    for (canonical, accepted_counts) in [
        ("Piecewise", &[3, 4, 64][..]),
        ("Contour", &[6, 7, 13, 21, 22][..]),
        ("PhasePortrait", &[2][..]),
        ("ComplexGrid", &[1, 6, 7][..]),
        ("HeatMap", &[1, 6, 7][..]),
        ("Quadrants", &[0, 4][..]),
    ] {
        let spec = command_registry::resolve(canonical)
            .expect("assistant executable commands must have registered metadata");
        for count in accepted_counts {
            assert!(
                spec.accepts_argument_count(*count),
                "{canonical} arity {count} must remain registered"
            );
        }
    }

    assert!(!command_registry::resolve("Piecewise")
        .expect("registered Piecewise")
        .accepts_argument_count(2));
    assert!(command_registry::resolve("Contour")
        .expect("registered Contour")
        .accepts_argument_count(22));
    assert!(command_registry::resolve("PhasePortrait")
        .expect("registered PhasePortrait")
        .accepts_argument_count(3));
}

#[test]
fn registry_exposes_locus_and_gd_action_objects() {
    // Frente G-D: Button deja de ser placeholder y tiene metadata estable con
    // brazo despachador; Image sigue sin metadata (stub honesto sin registro).
    assert!(
        command_registry::resolve("Image").is_none(),
        "Image must not have stable command metadata"
    );

    let button = command_registry::resolve("Button").expect("G-D Button needs stable metadata");
    assert!(button.palette_visible);
    assert_eq!(button.signatures[0].syntax, "Button[rotulo, guion]");

    let sampled = command_registry::resolve("SampledGraph")
        .expect("the static function sampler needs stable metadata");
    assert!(sampled.palette_visible);
    assert_eq!(sampled.signatures[0].syntax, "SampledGraph[expr, range]");
    assert!(sampled.help.contains("201"));
    assert!(sampled.help.contains("poligono estatico"));
    assert!(sampled.help.contains("no es un lugar geometrico dinamico"));

    let locus = command_registry::resolve("Locus")
        .expect("the persistent local locus needs stable metadata");
    assert!(locus.palette_visible);
    assert_eq!(locus.signatures[0].syntax, "Locus[driver, target]");
    assert!(locus.help.contains("sin eventos de puntero"));
}

#[test]
fn locus_equation_help_es_aproximacion_no_exacta() {
    // P1a-5 red-first: el help no debe vender "Groebner mock" como exacto.
    let spec = command_registry::resolve("LocusEquation").expect("LocusEquation registrado");
    let lower = spec.help.to_lowercase();
    assert!(
        lower.contains("aproximación por regresión"),
        "help honesto esperado, fue: {}",
        spec.help
    );
    assert!(
        lower.contains("no exacta"),
        "help debe decir no exacta, fue: {}",
        spec.help
    );
    assert!(
        !lower.contains("mock"),
        "ya no debe decir mock, fue: {}",
        spec.help
    );
}

#[test]
fn locus_equation_mensaje_es_aproximacion_no_exacta() {
    // P1a-5: el mensaje de éxito avisa que es regresión, no exacta. Sin cambiar matemática.
    use grafito_core::GeoObject;
    use grafito_geometry::Point2;
    let mut doc = Document::new();
    let driver = doc
        .try_add_point(Point2::new(0.0, 0.0))
        .expect("driver fixture");
    let target = doc
        .try_add_point(Point2::new(1.0, 0.0))
        .expect("target fixture");
    let (locus_id, _) = doc.try_add_locus(driver, target).expect("locus fixture");
    // Muestras de círculo para que la regresión grado 2 converja.
    if let Some(GeoObject::Pencil(pencil)) = doc.get_object_mut(locus_id) {
        pencil.points.clear();
        for idx in 0..50 {
            let angle = idx as f64 / 50.0 * std::f64::consts::TAU;
            pencil.points.push(Point2::new(angle.cos(), angle.sin()));
        }
    } else {
        panic!("locus esperado");
    }
    let label = doc
        .get_object(locus_id)
        .map(|o| o.label().to_string())
        .expect("etiqueta locus");
    let mut input = format!("LocusEquation[{label}]");
    match process_input(&mut doc, &mut input) {
        CommandOutcome::Message(msg) => {
            assert!(
                msg.contains("aproximación por regresión"),
                "mensaje honesto esperado, fue: {msg}"
            );
            assert!(msg.contains("no exacta"), "fue: {msg}");
            assert!(msg.contains("RMSE"), "fue: {msg}");
        }
        other => panic!("LocusEquation debe dar Message, fue: {other:?}"),
    }
}

#[test]
fn markdown_reference_is_the_registry_projection() {
    const RUNTIME_VALIDITY_NOTES: &str = "\n## Valores validos\n";
    let documentation = include_str!("../../../docs/commands.md");
    let (generated_reference, notes) = documentation
        .split_once(RUNTIME_VALIDITY_NOTES)
        .expect("docs must contain the runtime validity notes");

    assert_eq!(
        format!("{generated_reference}\n"),
        command_registry::render_markdown(),
        "the command reference before the runtime notes must be regenerated from the registry metadata"
    );
    assert!(notes.contains("t0 < t1"));
    assert!(notes.contains("x_min < x_max"));
    assert!(notes.contains("SetValue[nombre, valor]"));
    assert!(
        generated_reference.contains("`Rotate[punto, angulo]`"),
        "all documented Rotate forms must appear in the generated reference"
    );
    assert!(!generated_reference.contains("`Image["));
    assert!(generated_reference.contains("`Locus[driver, target]`"));
    assert!(generated_reference.contains("`SampledGraph[expr, range]`"));
}

#[test]
fn readme_claims_only_implemented_dynamic_locus_support() {
    let readme = include_str!("../../../README.en.md");

    assert!(readme.contains("Locus"));
    assert!(!readme.contains("Slider, Button, Image"));
}

#[test]
fn aliases_resolve_to_the_same_canonical_command() {
    let derivative = command_registry::resolve("derivada").expect("Spanish alias should resolve");
    assert_eq!(derivative.id, "cas.derivative");
    assert_eq!(command_registry::canonicalize("diff"), Some("Derivative"));

    let thomas =
        command_registry::resolve("butterfly").expect("legacy palette name should resolve");
    assert_eq!(thomas.canonical, "Thomas");
    assert_eq!(thomas.insertion, "Thomas[");
}

#[test]
fn parser_uses_registry_canonical_commands_for_registered_aliases() {
    for (input, expected) in [
        ("derivada[x^2, x]", "Derivative"),
        ("lim[x/x, x, 1]", "Limit"),
        ("butterfly[1]", "Thomas"),
        ("SolveSystem[[1], [1]]", "LinearSolve"),
    ] {
        let parsed = parse_cas_command(input).expect("command should parse");
        assert_eq!(parsed.command, expected, "input: {input}");
    }
}

#[test]
fn implicit_regions_are_normalized_to_the_verified_implicit_curve_command() {
    assert_eq!(
        command_registry::canonicalize("ImplicitRegion"),
        Some("ImplicitCurve")
    );

    let parsed = parse_cas_command("ImplicitRegion[(x^2 + y^2 - 1)^3 - x^2*y^3 = 0]")
        .expect("implicit region alias should parse");
    assert_eq!(parsed.command, "ImplicitCurve");
}

#[test]
fn construction_and_transformation_signatures_match_the_handlers() {
    let parallel = command_registry::resolve("Parallel").unwrap();
    assert_eq!(
        parallel.signatures[0].arguments[0].kind,
        ArgumentKind::Point
    );
    assert_eq!(
        parallel.signatures[0].arguments[1].kind,
        ArgumentKind::Object
    );

    for command in ["Translate", "Rotate", "Dilate"] {
        let spec = command_registry::resolve(command).unwrap();
        assert!(
            spec.signatures
                .iter()
                .all(|signature| signature.arguments[0].kind == ArgumentKind::Point),
            "{command} only transforms points in its current handler"
        );
    }

    let reflect = command_registry::resolve("Reflect").unwrap();
    assert_eq!(
        reflect.signatures[0].arguments[0].kind,
        ArgumentKind::Object
    );

    let tangent = command_registry::resolve("Tangent").unwrap();
    assert!(tangent.accepts_argument_count(2));
    assert!(tangent.accepts_argument_count(3));
    assert!(tangent.signatures.iter().any(|signature| {
        signature.arguments.len() == 3
            && signature.arguments[0].kind == ArgumentKind::Point
            && signature.arguments[1].kind == ArgumentKind::Number
            && signature.arguments[2].kind == ArgumentKind::Point
    }));
}

#[test]
fn regular_polytope_registry_metadata_has_exact_arity_and_nonconflicting_aliases() {
    for (canonical, aliases, accepted_counts) in [
        (
            "Pentachoron4D",
            &["fivecell4d", "5cell4d"][..],
            &[0, 1, 2][..],
        ),
        ("Tesseract4D", &["hypercube4d"][..], &[0, 1, 2][..]),
        ("SixteenCell4D", &["16cell4d"][..], &[0, 1, 2][..]),
        ("TwentyFourCell4D", &["24cell4d"][..], &[0, 1, 2][..]),
        ("OneTwentyCell4D", &["120cell4d"][..], &[0, 1, 2][..]),
        ("SixHundredCell4D", &["600cell4d"][..], &[0, 1, 2][..]),
        ("SimplexND", &["simplex_nd"][..], &[1, 2, 3][..]),
        ("HypercubeND", &["hypercube_nd"][..], &[1, 2, 3][..]),
        (
            "CrossPolytopeND",
            &["cross_polytope_nd"][..],
            &[1, 2, 3][..],
        ),
    ] {
        let spec = command_registry::resolve(canonical).expect("typed polytope metadata");
        assert_eq!(spec.aliases, aliases);
        for count in 0..=4 {
            assert_eq!(
                spec.accepts_argument_count(count),
                accepted_counts.contains(&count),
                "{canonical} arity {count}"
            );
        }
        for alias in aliases {
            assert_eq!(command_registry::canonicalize(alias), Some(canonical));
            assert_eq!(
                parse_cas_command(&format!("{alias}[]"))
                    .expect("registered aliases parse")
                    .command,
                canonical
            );
        }
    }

    assert_eq!(
        command_registry::canonicalize("Tetrahedron"),
        Some("Tetrahedron")
    );
    assert_eq!(
        command_registry::canonicalize("Hypercube"),
        Some("Hypercube")
    );
    assert_eq!(
        command_registry::canonicalize("tesseract"),
        Some("Hypercube")
    );
    assert_eq!(
        command_registry::canonicalize("Tesseract4D"),
        Some("Tesseract4D")
    );
}

#[test]
fn registry_tracks_handler_supported_optional_visualization_and_attractor_parameters() {
    let domain_coloring = command_registry::resolve("DomainColoring")
        .expect("domain coloring metadata must be registered");
    for count in 1..=6 {
        assert!(
            domain_coloring.accepts_argument_count(count),
            "DomainColoring arity {count} must preserve its handler defaults"
        );
    }
    assert!(!domain_coloring.accepts_argument_count(0));
    assert!(domain_coloring.signatures[0].arguments[1..]
        .iter()
        .all(|argument| argument.optional));

    for (canonical, maximum) in [
        ("Aizawa", 6),
        ("Chen", 3),
        ("Halvorsen", 4),
        ("Dadras", 5),
        ("Chua", 4),
    ] {
        let spec = command_registry::resolve(canonical).expect("attractor metadata");
        for count in 0..=maximum {
            assert!(
                spec.accepts_argument_count(count),
                "{canonical} arity {count} must match parse_attractor_params"
            );
        }
        assert!(
            !spec.accepts_argument_count(maximum + 1),
            "{canonical} must reject arguments beyond its handler defaults"
        );
    }
}

#[test]
fn registry_preserves_taylor_handler_defaults() {
    let taylor = command_registry::resolve("Taylor").expect("Taylor metadata must be registered");
    for count in 2..=4 {
        assert!(
            taylor.accepts_argument_count(count),
            "Taylor arity {count} must preserve its handler defaults"
        );
    }
    assert!(!taylor.accepts_argument_count(1));
    assert!(!taylor.accepts_argument_count(5));
}

#[test]
fn taylor_handler_accepts_every_documented_optional_form() {
    for command in ["Taylor[x, x]", "Taylor[x, x, 1]", "Taylor[x, x, 1, 4]"] {
        let mut document = Document::new();
        let mut input = command.to_owned();
        assert!(
            matches!(
                process_input(&mut document, &mut input),
                CommandOutcome::Message(_)
            ),
            "{command} must pass the registry arity gate and execute"
        );
    }
}

// Frente W1: puerta simbólica cableada a paleta (ida/vuelta spec→brazo→test).
// Cada comando nuevo delega al motor sin duplicar matemática; el `Err` se
// vuelve mensaje honesto con el límite del subset.
#[test]
fn w1_symbolic_gate_specs_resolve_and_match_handlers() {
    for (canonical, aliases, counts) in [
        ("SolveODE2", &["edo2", "edo_2"][..], &[4, 5][..]),
        ("ODESystem2", &["sistemaedo2", "odesys2"][..], &[4, 5][..]),
        (
            "LaplaceT",
            &["transformadalaplace", "laplace_t"][..],
            &[1, 2, 3][..],
        ),
        (
            "InvLaplaceT",
            &["laplaceinversa", "invlaplace_t"][..],
            &[1, 2, 3][..],
        ),
        ("RischInt", &["risch", "risch_int"][..], &[1, 2, 4][..]),
        (
            "GroebnerBasis",
            &["groebner_basis", "basegroebner"][..],
            &[2][..],
        ),
    ] {
        let spec = command_registry::resolve(canonical)
            .unwrap_or_else(|| panic!("{canonical} must have stable metadata"));
        assert!(spec.palette_visible, "{canonical} must be palette-visible");
        assert_eq!(spec.dispatch_key, canonical);
        for count in counts {
            assert!(
                spec.accepts_argument_count(*count),
                "{canonical} arity {count} must remain registered"
            );
        }
        for alias in aliases {
            assert_eq!(
                command_registry::canonicalize(alias),
                Some(canonical),
                "alias {alias} must resolve to {canonical}"
            );
        }
    }
    // Sin formas fantasma: aridades no documentadas se rechazan en el gate.
    assert!(!command_registry::resolve("RischInt")
        .expect("RischInt registered")
        .accepts_argument_count(3));
    assert!(!command_registry::resolve("GroebnerBasis")
        .expect("GroebnerBasis registered")
        .accepts_argument_count(1));
    // Colisiones evitadas: la distribución sigue legado sin spec registrada.
    assert!(command_registry::resolve("Laplace").is_none());
    assert_eq!(
        command_registry::canonicalize("groebnerbasis"),
        Some("GroebnerBasis")
    );
}

#[test]
fn w1_symbolic_gate_handlers_delegate_to_motor() {
    fn run(command: &str) -> CommandOutcome {
        let mut document = Document::new();
        let mut input = command.to_owned();
        process_input(&mut document, &mut input)
    }
    for (command, needle) in [
        ("SolveODE2[1, -3, 2, exp(x)]", "C1"),
        ("SolveODE2[1, -3, 2, exp(x), x]", "C1"),
        ("ODESystem2[0, 1, -2, -3]", "exp(-1*t)"),
        ("ODESystem2[0, 1, -2, -3, t]", "exp(-1*t)"),
        ("LaplaceT[sin(t)]", "s^2+1"),
        ("LaplaceT[sin(t), t, s]", "s^2+1"),
        ("LaplaceT[1]", "1/s"),
        ("InvLaplaceT[1/(s+1)]", "exp(-1*t)"),
        ("InvLaplaceT[1/(s+1), s, t]", "exp(-1*t)"),
        ("RischInt[x^2]", "∫ x^2 dx"),
        ("RischInt[x^2, x]", "∫ x^2 dx"),
        ("RischInt[x^2, x, 0, 1]", "0.33333333"),
        ("GroebnerBasis[{x+y-3, x-y-1}, {x, y}]", "S-polinomios"),
    ] {
        match run(command) {
            CommandOutcome::Message(message) => assert!(
                message.contains(needle),
                "{command} → {message} (esperaba '{needle}')"
            ),
            other => panic!("{command} debe dar Message, dio {other:?}"),
        }
    }
}

#[test]
fn w1_symbolic_gate_errors_are_honest_with_subset_limits() {
    fn run(command: &str) -> CommandOutcome {
        let mut document = Document::new();
        let mut input = command.to_owned();
        process_input(&mut document, &mut input)
    }
    let over_budget: Vec<String> = (0..20).map(|i| format!("x + {i}")).collect();
    let over_budget_cmd = format!("GroebnerBasis[{{{}}}, {{x}}]", over_budget.join(", "));
    let cases: Vec<(String, Vec<&str>)> = vec![
        (
            "SolveODE2[0, 1, 1, x]".to_string(),
            vec!["SolveODE2", "1er orden"],
        ),
        ("ODESystem2[t, 1, 0, 1]".to_string(), vec!["ODESystem2"]),
        ("LaplaceT[exp(t^2)]".to_string(), vec!["LaplaceT"]),
        ("InvLaplaceT[1/(s^4+1)]".to_string(), vec!["InvLaplaceT"]),
        ("RischInt[exp(x^2)]".to_string(), vec!["RischInt"]),
        (
            "GroebnerBasis[{sin(x)+y, x-y}, {x, y}]".to_string(),
            vec!["GroebnerBasis"],
        ),
        (over_budget_cmd, vec!["GroebnerBasis", "Eliminate"]),
    ];
    for (command, needles) in cases {
        match run(&command) {
            CommandOutcome::Error(message) => {
                for needle in needles {
                    assert!(
                        message.contains(needle),
                        "{command} → {message} (esperaba '{needle}')"
                    );
                }
            }
            other => panic!("{command} debe dar Error honesto, dio {other:?}"),
        }
    }
}

#[test]
fn w1_symbolic_gate_validates_max_expr_length_budget() {
    fn run(command: &str) -> CommandOutcome {
        let mut document = Document::new();
        let mut input = command.to_owned();
        process_input(&mut document, &mut input)
    }
    let big = "x".repeat(2001);
    for command in [
        format!("SolveODE2[1, 1, 1, {big}]"),
        format!("RischInt[{big}]"),
        format!("LaplaceT[{big}]"),
        format!("GroebnerBasis[{big}, {{x}}]"),
    ] {
        match run(&command) {
            CommandOutcome::Error(message) => assert!(
                message.contains("MAX_EXPR_LENGTH"),
                "{message} debe citar el presupuesto"
            ),
            other => panic!("entrada >2000 debe dar Error, dio {other:?}"),
        }
    }
}

#[test]
fn resolve_alias_escolares_resuelve() {
    // Onda 1 red-first: aliases escolares que morían sin spec.
    // `Laplace` NO resuelve (es distribución legado sin spec); el help de
    // `LaplaceT` desambigua con "¿Buscabas LaplaceT[expr]?".
    for (alias, canonical) in [
        ("SolveODE", "SolveODEN"),
        ("solveode", "SolveODEN"),
        ("BinomialDist", "Binomial"),
        ("binomialdist", "Binomial"),
        ("NormalDist", "Normal"),
        ("normaldist", "Normal"),
    ] {
        let spec = command_registry::resolve(alias)
            .unwrap_or_else(|| panic!("alias escolar {alias} debe resolver"));
        assert_eq!(spec.canonical, canonical, "alias {alias}");
        assert_eq!(
            command_registry::canonicalize(alias),
            Some(canonical),
            "canonicalize {alias}"
        );
        let parsed = parse_cas_command(&format!("{alias}[x]"))
            .unwrap_or_else(|| panic!("alias {alias} debe parsear"));
        assert_eq!(parsed.command, canonical, "parse {alias}");
    }
    // Laplace sigue siendo distribución (sin spec), no alias de LaplaceT.
    assert!(command_registry::resolve("Laplace").is_none());
    let laplace_t = command_registry::resolve("LaplaceT").expect("LaplaceT registrado");
    assert!(
        laplace_t.help.contains("Laplace es distribución"),
        "help desambigua, fue: {}",
        laplace_t.help
    );
    assert!(
        laplace_t.help.contains("¿Buscabas LaplaceT[expr]?"),
        "help sugiere forma, fue: {}",
        laplace_t.help
    );
    // Normal acepta 2 (crea) y 3 (evalúa PDF/CDF vía statistics).
    let normal = command_registry::resolve("Normal").expect("Normal registrado");
    assert!(normal.accepts_argument_count(2));
    assert!(normal.accepts_argument_count(3));
}

#[test]
fn alias_escolares_e2e_por_alias() {
    fn run(command: &str) -> CommandOutcome {
        let mut document = Document::new();
        let mut input = command.to_owned();
        process_input(&mut document, &mut input)
    }
    // SolveODE alias delega al mismo brazo que SolveODEN.
    match run("SolveODE[{1,0,1}, 0]") {
        CommandOutcome::Message(msg) => assert!(msg.contains("cos"), "fue: {msg}"),
        other => panic!("SolveODE alias debe dar Message, dio {other:?}"),
    }
    // BinomialDist alias evalúa igual que Binomial.
    match run("BinomialDist[10, 0.5, 5]") {
        CommandOutcome::Message(msg) => assert!(msg.contains("P(X=5)"), "fue: {msg}"),
        other => panic!("BinomialDist alias debe dar Message, dio {other:?}"),
    }
    // NormalDist alias crea igual que Normal[mu, sigma].
    match run("NormalDist[0, 1]") {
        CommandOutcome::Message(msg) => assert!(msg.contains("Normal"), "fue: {msg}"),
        other => panic!("NormalDist alias debe dar Message, dio {other:?}"),
    }
    // Normal[mu, sigma, x] evalúa PDF/CDF vía statistics.
    match run("Normal[0, 1, 0]") {
        CommandOutcome::Message(msg) => {
            assert!(msg.contains("PDF"), "fue: {msg}");
            assert!(msg.contains("CDF"), "fue: {msg}");
        }
        other => panic!("Normal 3 args debe dar Message, dio {other:?}"),
    }
    // Laplace distribución intacta.
    match run("Laplace[0, 1]") {
        CommandOutcome::Message(msg) => assert!(msg.contains("PDF"), "fue: {msg}"),
        other => panic!("Laplace debe seguir distribución, dio {other:?}"),
    }
}

#[test]
fn w1_new_names_do_not_shadow_laplace_distribution_or_numeric_ode() {
    fn run(command: &str) -> CommandOutcome {
        let mut document = Document::new();
        let mut input = command.to_owned();
        process_input(&mut document, &mut input)
    }
    // La distribución Laplace[media, b] sigue viva (legado sin spec).
    match run("Laplace[0, 1]") {
        CommandOutcome::Message(message) => {
            assert!(
                message.contains("PDF"),
                "distribución intacta, got {message}"
            )
        }
        other => panic!("Laplace[0, 1] debe seguir siendo la distribución, dio {other:?}"),
    }
    // El integrador numérico conserva su spec y dispatch.
    assert_eq!(
        command_registry::canonicalize("ODESystem"),
        Some("ODESystem")
    );
    assert_eq!(
        command_registry::canonicalize("ODESystem2"),
        Some("ODESystem2")
    );
}
