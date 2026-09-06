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
