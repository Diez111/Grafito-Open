//! Corpus semilla de referencia del solver local del asistente (Fase A).
//!
//! Cada caso es un (problema, estado esperado, fragmento esperado). Versionar
//! estos casos detecta regresiones del solver determinista sin red.

use grafito_assistant::solve_local;
use grafito_assistant_types::{AssistantRequest, ImmutableDocumentContext, LocalAssistantStatus};

fn solve(problem: &str) -> grafito_assistant_types::AssistantResponse {
    let request = AssistantRequest::local(problem, ImmutableDocumentContext::empty(0));
    solve_local(&request)
}

#[test]
fn corpus_arithmetic_reference_cases() {
    let cases = [
        ("2 + 2", "4"),
        ("3 * 5", "15"),
        ("10 - 4", "6"),
        ("100 / 4", "25"),
        ("2 + 3 * 4", "14"),
        ("(1 + 2) * 3", "9"),
        ("2.5 + 1.5", "4"),
        ("0 - 5", "-5"),
    ];
    for (problem, expected) in cases {
        let response = solve(problem);
        assert_eq!(response.status, LocalAssistantStatus::Solved, "{problem}");
        assert!(
            response.answer.contains(expected),
            "{problem} -> {}",
            response.answer
        );
    }
}

#[test]
fn corpus_linear_equation_reference_cases() {
    let cases = [
        ("2*x + 3 = 11", "x = 4"),
        ("x = 5", "x = 5"),
        ("3*x = 15", "x = 5"),
        ("x + 10 = -2", "x = -12"),
        ("2.5*x = 5", "x = 2"),
    ];
    for (problem, expected) in cases {
        let response = solve(problem);
        assert_eq!(response.status, LocalAssistantStatus::Solved, "{problem}");
        assert!(
            response.answer.contains(expected),
            "{problem} -> {}",
            response.answer
        );
    }
}

#[test]
fn corpus_quadratic_equation_reference_cases() {
    let cases = [
        ("x^2 - 5*x + 6 = 0", "x = 2"),
        ("x^2 - 5*x + 6 = 0", "x = 3"),
        ("x^2 = 4", "x = -2"),
        ("x^2 = 4", "x = 2"),
        ("x^2 + 1 = 0", "No real solutions"),
        ("x^2 + x + 1 = 0", "No real solutions"),
        ("x^2 - 0.6*x + 0.09 = 0", "x = 0.3"),
    ];
    for (problem, expected) in cases {
        let response = solve(problem);
        assert_eq!(response.status, LocalAssistantStatus::Solved, "{problem}");
        assert!(
            response.answer.contains(expected),
            "{problem} -> {}",
            response.answer
        );
    }
}

#[test]
fn corpus_unsupported_inputs_are_rejected_without_network() {
    for problem in [
        "",
        "x^3 - 2 = 0",
        "2 + 2 = 4 = 4",
        "x^2 + y^2 = 1",
        "solve integral of sin(x)",
    ] {
        let response = solve(problem);
        let status = response.status;
        assert!(
            matches!(
                status,
                LocalAssistantStatus::Unsupported | LocalAssistantStatus::Rejected
            ),
            "{problem} -> {status:?}"
        );
    }
}

#[test]
fn corpus_cas_pedagogical_requests_resolve_without_network() {
    for (problem, expected) in [
        ("derivar x^2", "d/dx(x^2)"),
        ("derivada de x^3 + x", "d/dx(x^3 + x)"),
        ("integrar 2*x", "∫ 2*x dx"),
        ("integral de sin(x)", "∫ sin(x) dx"),
    ] {
        let response = solve(problem);
        assert_eq!(response.status, LocalAssistantStatus::Solved, "{problem}");
        assert!(
            response.answer.contains(expected),
            "{problem} -> {}",
            response.answer
        );
        assert!(
            response
                .derivation
                .iter()
                .any(|step| step.rule.contains("CAS")),
            "{problem} debe citar la regla CAS"
        );
    }
    let limit = solve("limite de sin(x)/x en 0");
    assert_eq!(limit.status, LocalAssistantStatus::Solved);
    assert!(limit.answer.contains("lim x→0"), "{}", limit.answer);
    assert!(limit.answer.contains('='), "{}", limit.answer);
    assert!(
        limit
            .derivation
            .iter()
            .any(|step| step.rule.contains("Límite")),
        "{}",
        limit
            .derivation
            .last()
            .map(|s| s.rule.as_str())
            .unwrap_or("")
    );
}

#[test]
fn corpus_graph_requests_build_a_typed_plan() {
    for problem in [
        "graph x^2",
        "graficar y=2*x",
        "grafica sin(x)",
        "plot y = x",
    ] {
        let response = solve(problem);
        assert_eq!(response.status, LocalAssistantStatus::Solved, "{problem}");
        assert!(
            response.plan.is_some(),
            "{problem} debe ofrecer una propuesta de grafica"
        );
    }
}
