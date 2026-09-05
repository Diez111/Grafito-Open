#![allow(unknown_lints, float_literal_f32_fallback)]
#![cfg_attr(test, allow(clippy::unwrap_used, clippy::expect_used))]
//! Resolución local y transporte remoto explícitamente opt-in del asistente.
//!
//! La resolución local es determinista. El transporte remoto sólo construye y
//! ejecuta peticiones OpenAI-compatibles desde un hilo de trabajo; nunca se
//! llama desde la UI ni persiste claves de API.

pub mod agent;
pub mod harness;

use base64::Engine;
use grafito_agent::schema::ToolSchema;
use grafito_assistant_types::{
    AssistantOperation, AssistantRequest, AssistantResponse, AttachmentLimits, ConversationRole,
    DerivationStep, ImageAttachment, LocalAssistantStatus, PrivacyMode, ProposedPlan,
    ProviderCapabilities, ProviderProfile, REMOTE_CONTEXT_PROMPT_PREFIX,
    REMOTE_FOCUS_PROMPT_PREFIX, REMOTE_REPAIR_FEEDBACK_PROMPT_PREFIX,
    REMOTE_TOOL_CATALOG_PROMPT_PREFIX,
};
use grafito_geometry::{
    ast::{parse_ast, Expr},
    derivation::{derive_polynomial, normalize_scientific_notation},
    expr::evaluate,
};
use grafito_pedagogy::{PedagogicalLevel, Scaffold, ScaffoldEngine, SocraticFsm};
use image::{DynamicImage, ImageDecoder, ImageFormat, ImageReader};
use serde_json::{json, Value};
use std::io::{Cursor, Read};
use std::net::IpAddr;
use std::sync::{
    atomic::{AtomicBool, Ordering},
    Arc,
};
use std::thread::JoinHandle;
use std::time::{Duration, Instant};
use url::Url;

const RESIDUAL_EPSILON: f64 = 128.0 * f64::EPSILON;
const ROOT_DEDUP_EPSILON: f64 = 8.0 * f64::EPSILON;
const DISCRIMINANT_ERROR_MULTIPLIER: f64 = 32.0;
const MAX_RESPONSE_ENVELOPE_BYTES: usize = 4 * 1024;
const MAX_MODELS_RESPONSE_BYTES: usize = 16 * 1024;
const MAX_DISCOVERED_MODELS: usize = 256;
const MAX_MODEL_IDENTIFIER_CHARS: usize = 256;
const CUSTOM_API_KEY_PREFIX: &str = "GRAFITO_ASSISTANT_CUSTOM_";
const CUSTOM_API_KEY_SUFFIX: &str = "_API_KEY";
const MAX_CUSTOM_API_KEY_REFERENCE_LEN: usize = 128;
const OPENCODE_VISION_MODEL: &str = "mimo-2.5-vl";
const OPENCODE_FUSION_MODEL: &str = "fusion";
/// Muse Spark sólo responde por la Responses API (`POST {base}/responses`);
/// por Chat Completions el proveedor devuelve 500 instantáneo con cualquier
/// payload (verificado 2026-09-04 contra el endpoint real, 8 payloads).
/// Se matchea por `contains` para cubrir futuras 1.x sin tocar el router.
fn uses_responses_api(model: &str) -> bool {
    model.contains("muse-spark")
}
/// `max_output_tokens` mínimo que exige el servidor en `/responses`.
const RESPONSES_MIN_OUTPUT_TOKENS: usize = 16;
/// Cap del cuerpo en la Responses API: los items `reasoning` traen
/// `encrypted_content` opaco (se descarta) que supera el presupuesto de
/// texto del usuario. 64 KiB transitorios por request (precedente: line_cap
/// 64 KiB del motor de animaciones); el texto útil sigue acotado por
/// `max_output_chars` en `completion_from_text`.
const RESPONSES_MAX_BODY_BYTES: usize = 64 * 1024;
const FUSION_AUDIT_MODEL: &str = "deepseek-v4-pro";
const FUSION_MAX_DRAFT_BYTES: usize = 2_048;
/// Truncado del cuerpo de error HTTP en mensajes: 500 chars sin secretos.
/// El cuerpo es dato del proveedor (nunca incluye la API key); se acota para
/// no saturar el log `assistant_remote_event` ni la UI.
const MAX_ERROR_BODY_CHARS: usize = 500;
/// Clamp de `Retry-After` en 429: 1s evita busy-retry, 120s evita bloquear la
/// sesión. Nunca se duerme el worker (es cancelable vía `CancellationToken`);
/// sólo se informa "reintentá en Ns" y la UI decide cuándo reintentar.
const RETRY_AFTER_MIN_SECS: u64 = 1;
const RETRY_AFTER_MAX_SECS: u64 = 120;
/// Timeouts remotos: simple 60s (`RequestBudget::default`), agente 30s por
/// turno (`AgentBudget::default` en grafito-agent/loop_engine.rs:43). Piso
/// 100ms = mínimo válido de `RequestBudget::validate`; techo 120s = cap
/// absoluto. Ningún path construye `Duration` 0 ni >120s (ver
/// `effective_remote_timeout`).
const REMOTE_TIMEOUT_MIN_MS: u64 = 100;
const REMOTE_TIMEOUT_MAX_MS: u64 = 120_000;
const GRAFITO_CAPABILITY_SCOPE: &str = "Grafito is a broad dynamic-mathematics environment, not only a y=f(x) plotter. Consider geometric construction; real, parametric, polar and implicit curves; contours and vector fields; a full symbolic CAS (Derivative, Integral, Limit, TaylorSeries, Solve, Factor, Expand) and numeric analysis (roots, extrema, inflection, intercepts, tangent, arc length, curvature); statistics and regression; complex mappings and domain coloring; fractals; 3D solids, curves, surfaces and fields; dynamical systems and attractors; and CPU-projected 4D objects. The local engine solves many requests without a network: arithmetic, equations, graph proposals, and symbolic derivadas/integrales/límites. When the user asks for Taylor/Integral/Derivative without specifying a function, reuse the most recent Function from the document context instead of defaulting to sin(x). Match the user's goal to the most useful area and mention relevant built-in perspectives. The per-request tool catalog remains authoritative for actionable syntax: use a catalogued command only when it fits, and describe the suitable Grafito workflow instead of inventing a command when it is not catalogued.";
const REMOTE_SYSTEM_PROMPT: &str = "Assist with Grafito math. Use the focused object when one is supplied, otherwise use the most recent Function in the document context for Taylor/Integral/Derivative when the user does not specify one (do not default to sin(x) if x^2 is visible). Ask one concise clarifying question only when a required mathematical value or a target object is genuinely unknown; do not ask for confirmation when the request already supplies a graphable expression and valid defaults exist. Format mathematical answers in concise Markdown: use pipe tables for tabular values and LaTex delimiters $...$ or $$...$$ for equations. The user prompt can include a bounded catalog of locally verified Grafito graph commands and the full document context (visible objects). When the catalog contains suitable choices, offer one to four independently useful fenced ```grafito commands, each on exactly one line and using only a catalogued command with every required literal known. When a graph needs a numeric parameter, emit its separate assignment in a one-line ```grafito-param block using an ASCII identifier and a finite numeric literal, for example `a = 2.5`; do not place it inside the graph command. For a requested 3D flower, emit exactly one ```grafito-scene block with seven lines: one Cylinder[x,y,z,radius,height] stem, one Sphere[x,y,z,radius] center, and five Surface3D[(x(u,v),y(u,v),z(u,v)),umin,umax,vmin,vmax] petals. Keep the stem vertical on Y, put the center at the stem top, and make every petal share that center height in its second Surface3D component. These commands may create 2D, 3D, or CPU-projected 4D graphs; Grafito opens the required view only after the user explicitly applies a card. Never invent a command, placeholder object label, or target-dependent construction. Use lowercase expression functions with parentheses, for example sin(x), cos(t), and sqrt(x). Prefer Function[expr] for a real y=f(x), DomainColoring for phase and modulus of f(z), and Surface3D for a real surface. Do not claim a command ran: Grafito preflights it locally and the user explicitly chooses whether to apply it. Never emit file, shell, network, save, export, delete, import, or Script commands.";
const REMOTE_RESPONSE_GUIDANCE: &str = "Begin with `## Enfoque` and three to five concise, checkable steps. Do not reveal private chain-of-thought or hidden reasoning. Only catalog items marked [EJECUTABLE] may appear in grafito or grafito-scene fences; [REFERENCIA] items are explanatory only. A grafito fence must copy catalogued syntax exactly: use Function[expr] only, with no domain/sample arguments, and never use if or frac expressions. For a Fourier request, emit an executable Function only for a finite numeric partial sum. When the user gives no signal or order, a clearly labelled square-wave example may use `Function[(4/pi)*(sin(x)+sin(3*x)/3+sin(5*x)/5)]`; otherwise use the supplied finite values. Never emit a general Fourier transform, symbolic a_n or b_n coefficients, unknown N, or sum(...) as an executable proposal. A grafito-scene contains two to eight one-line executable commands and is for an atomic construction such as multiple Segment3D edges; never use Script, Polyhedron, or NumericArray. If Grafito cannot represent it with catalogued syntax, explain it in Markdown instead of emitting a fence.";
const REMOTE_TETRAHEDRON_GUIDANCE: &str = "For a tetrahedron, emit exactly one one-line grafito block with Tetrahedron[x, y, z, edge] and finite literal values. Do not emit Polyhedron, NumericArray, or a grafito-scene block.";
const REMOTE_4D_POLYTOPE_GUIDANCE: &str = "For a regular 4D polytope, emit exactly one one-line grafito block with the appropriate named command: Pentachoron4D[scale, {xy, xz, xw, yz, yw, zw}], Tesseract4D[scale, {xy, xz, xw, yz, yw, zw}], SixteenCell4D[scale, {xy, xz, xw, yz, yw, zw}], TwentyFourCell4D[scale, {xy, xz, xw, yz, yw, zw}], OneTwentyCell4D[scale, {xy, xz, xw, yz, yw, zw}], or SixHundredCell4D[scale, {xy, xz, xw, yz, yw, zw}]. For higher-dimensional regular families use SimplexND[n, scale, {lexicographic-plane angles}], HypercubeND[n, scale, {lexicographic-plane angles}], or CrossPolytopeND[n, scale, {lexicographic-plane angles}]. Never substitute 3D Tetrahedron, bare Hypercube or tesseract, or many Segment3D edge lines.";
const FUSION_AUDIT_SYSTEM_PROMPT: &str = "Audit the candidate response for mathematical correctness, completeness, and safe explanatory behavior. Return only the corrected final answer for the user. Do not mention this audit, the candidate, internal models, API keys, tools, files, shell commands, or network actions. If the problem is ambiguous, ask one concise clarifying question instead of guessing.";
const FUSION_AUDIT_USER_PREFIX: &str = "Original user request and selected Grafito context:";
/// Directiva socrática vinculante inyectada a TODO system prompt remoto.
///
/// No es sugerencia: el LLM debe obedecer el FSM y el scaffold. Si `attempts<2`,
/// `can_reveal==false` y cualquier intento de `telling` (solución directa) debe
/// ser bloqueado por el guard remoto. Telling_rate <5% es invariante medible.
const SOCRATIC_BINDING_DIRECTIVE: &str = "MODO SOCRÁTICO VINCULANTE — ORDEN, NO SUGERENCIA:\n- Seguí estrictamente el FSM socrático Review→HeuristicQ→AwaitStudent→Rectify→Summarize (can_reveal = attempts>=2).\n- Si attempts<2 (can_reveal=false), NO reveles la solución directa (telling). Re-preguntá con la pregunta heurística exacta del scaffold y su pista.\n- Si el guard remoto detecta telling con attempts<2, tu respuesta será descartada y se forzará re-pregunta o repair_feedback.\n- Usá EXACTAMENTE la pregunta BKT actual (current_question) y la pista del scaffold inyectadas abajo; no inventes otra. Adaptá sólo el nivel de detalle según el historial.\n- Telling_rate debe mantenerse <5% (máx 1 telling cada 20 turnos). Si dudas, preguntá en vez de afirmar.\n- El system prompt es vinculante: desobedecerlo invalida la respuesta.";

/// Resuelve el subconjunto local, determinista y sin red del MVP.
pub fn solve_local(request: &AssistantRequest) -> AssistantResponse {
    let response = if let Err(error) = request.validate(&AttachmentLimits::default()) {
        AssistantResponse::message(
            LocalAssistantStatus::Rejected,
            format!("Local assistant request rejected: {error}."),
        )
    } else if let Err(error) = validate_decoded_attachments(request, &AttachmentLimits::default()) {
        AssistantResponse::message(
            LocalAssistantStatus::Rejected,
            format!("Local assistant attachment rejected: {error}."),
        )
    } else if !request.attachments.is_empty() {
        AssistantResponse::message(
            LocalAssistantStatus::VisionUnavailable,
            "Image attachments are validated, but local handwritten OCR is not implemented. Edit the transcription or use a vision-capable provider you explicitly enable.",
        )
    } else {
        let problem = if request.problem.trim().is_empty() {
            request.transcription.text.trim()
        } else {
            request.problem.trim()
        };
        if problem.is_empty() {
            unsupported("Paste an arithmetic problem, a one-variable equation, or a graph request.")
        } else if let Some(response) = solve_local_cas(problem, &request.budget) {
            response
        } else if let Some(expression) = parse_graph_request(problem) {
            solve_graph_request(request, expression)
        } else if problem.contains('=') {
            solve_equation(problem)
        } else {
            solve_arithmetic(problem, request)
        }
    };
    enforce_local_response_budget(response, &request.budget)
}

/// Resuelve pedagógicamente pedidos de derivada, integral y límite con el CAS
/// nativo de grafito-geometry (sin red). Devuelve `None` si no es un pedido CAS.
fn solve_local_cas(
    problem: &str,
    budget: &grafito_assistant_types::RequestBudget,
) -> Option<AssistantResponse> {
    let inline = || {
        let lowered = problem.to_ascii_lowercase();
        let derive = [
            "derivar ",
            "deriva ",
            "derivada de ",
            "derivá ",
            "d/dx ",
            "derivada de la funcion ",
        ]
        .iter()
        .find_map(|prefix| {
            lowered
                .strip_prefix(prefix)
                .map(|rest| (rest.to_owned(), prefix))
        });
        let integrate = [
            "integrar ",
            "integra ",
            "integral de ",
            "integral indefinida de ",
            "∫ ",
        ]
        .iter()
        .find_map(|prefix| {
            lowered
                .strip_prefix(prefix)
                .map(|rest| (rest.to_owned(), prefix))
        });
        let limit = ["limite de ", "límite de ", "limit de "]
            .iter()
            .find_map(|prefix| {
                lowered
                    .strip_prefix(prefix)
                    .map(|rest| (rest.to_owned(), prefix))
            });
        (derive, integrate, limit)
    };
    let (derive, integrate, limit) = inline();
    if let Some((rest, prefix)) = derive {
        let expression = rest
            .split('=')
            .next()
            .unwrap_or(&rest)
            .trim()
            .trim_matches('.');
        if expression.is_empty() || expression.len() > budget.max_input_chars {
            return Some(unsupported(
                "The derivative expression is empty or exceeds the local budget.",
            ));
        }
        return Some(
            match grafito_geometry::symbolic::derivative(expression, "x") {
                Ok(result) => cas_solved(
                    format!("d/dx({expression}) = {result}"),
                    expression,
                    &result,
                    "Derivada simbólica (CAS nativo)",
                ),
                Err(error) => cas_unsupported(&error, prefix),
            },
        );
    }
    if let Some((rest, prefix)) = integrate {
        let expression = rest
            .split('=')
            .next()
            .unwrap_or(&rest)
            .trim()
            .trim_matches('.');
        if expression.is_empty() || expression.len() > budget.max_input_chars {
            return Some(unsupported(
                "The integral expression is empty or exceeds the local budget.",
            ));
        }
        return Some(
            match grafito_geometry::symbolic::integrate(expression, "x") {
                Ok(result) => cas_solved(
                    format!("∫ {expression} dx = {result} + C"),
                    expression,
                    &format!("{result} + C"),
                    "Integral simbólica (CAS nativo)",
                ),
                Err(error) => cas_unsupported(&error, prefix),
            },
        );
    }
    if let Some((rest, prefix)) = limit {
        let (expression, at) = split_limit_problem(&rest);
        if expression.is_empty() || expression.len() > budget.max_input_chars {
            return Some(unsupported(
                "The limit expression is empty or exceeds the local budget.",
            ));
        }
        return Some(
            match grafito_geometry::symbolic::limit(expression, "x", at) {
                Ok(result) => cas_solved(
                    format!("lim x→{at} de ({expression}) = {result}"),
                    expression,
                    &result,
                    "Límite (CAS nativo, Richardson)",
                ),
                Err(error) => cas_unsupported(&error, prefix),
            },
        );
    }
    None
}

fn split_limit_problem(rest: &str) -> (&str, f64) {
    for marker in ["cuando x->", "cuando x → ", "en ", "->"] {
        if let Some(index) = rest.find(marker) {
            let expression = rest[..index].trim();
            let tail = rest[index + marker.len()..].trim();
            if let Ok(at) = tail.parse::<f64>() {
                return (expression, at);
            }
        }
    }
    (rest.trim(), 0.0)
}

fn cas_solved(answer: String, before: &str, after: &str, rule: &str) -> AssistantResponse {
    AssistantResponse {
        schema_version: grafito_assistant_types::ASSISTANT_SCHEMA_VERSION,
        status: LocalAssistantStatus::Solved,
        answer,
        derivation: vec![DerivationStep {
            before: before.to_string(),
            after: after.to_string(),
            rule: rule.into(),
            verification: "Resultado del CAS nativo, reproducible sin red.".into(),
        }],
        plan: None,
    }
}

fn cas_unsupported(error: &str, prefix: &str) -> AssistantResponse {
    unsupported(format!(
        "No pude resolver «{prefix}…» localmente con el CAS: {error}. Podés pedir la versión en línea."
    ))
}

fn solve_arithmetic(problem: &str, request: &AssistantRequest) -> AssistantResponse {
    match evaluate_with_context(problem, request, None) {
        Ok(value) if value.is_finite() => {
            let result = format_number(value);
            let step = DerivationStep {
                before: problem.to_string(),
                after: result.clone(),
                rule: "Evaluate arithmetic expression".into(),
                verification: format!("Direct evaluation yields {result}."),
            };
            AssistantResponse {
                schema_version: grafito_assistant_types::ASSISTANT_SCHEMA_VERSION,
                status: LocalAssistantStatus::Solved,
                answer: result,
                derivation: vec![step],
                plan: None,
            }
        }
        _ => unsupported(
            "This request is not supported by the local assistant MVP. It supports finite arithmetic, one-variable linear/quadratic equations, and graph requests.",
        ),
    }
}

fn solve_equation(problem: &str) -> AssistantResponse {
    let normalized_problem =
        match normalize_scientific_notation(problem) {
            Ok(problem) => problem,
            Err(_) => return unsupported(
                "This equation contains a scientific literal outside local f64 input precision.",
            ),
        };
    let mut parts = normalized_problem.split('=');
    let Some(left) = parts.next().map(str::trim) else {
        return unsupported("This equation is not supported by the local assistant MVP.");
    };
    let Some(right) = parts.next().map(str::trim) else {
        return unsupported("This equation is not supported by the local assistant MVP.");
    };
    if parts.next().is_some() || left.is_empty() || right.is_empty() {
        return unsupported("Use one equality sign in a local equation.");
    }

    let difference = format!("({left}) - ({right})");
    let left_ast = match parse_ast(left) {
        Ok(expression) => expression,
        Err(_) => {
            return unsupported(
                "This equation is not supported by the local assistant MVP. Use a structurally valid polynomial equation in x of degree at most two.",
            )
        }
    };
    let right_ast = match parse_ast(right) {
        Ok(expression) => expression,
        Err(_) => {
            return unsupported(
                "This equation is not supported by the local assistant MVP. Use a structurally valid polynomial equation in x of degree at most two.",
            )
        }
    };
    let coefficients = match derive_polynomial(&difference, "x") {
        Ok(coefficients) => coefficients,
        Err(error) => {
            return unsupported(format!(
                "This request is not supported by the local assistant MVP: {error}."
            ))
        }
    };

    if coefficients.a == 0.0 {
        solve_linear_equation(
            problem,
            &left_ast,
            &right_ast,
            coefficients.b,
            coefficients.c,
        )
    } else {
        solve_quadratic_equation(
            problem,
            &left_ast,
            &right_ast,
            coefficients.a,
            coefficients.b,
            coefficients.c,
        )
    }
}

fn solve_linear_equation(
    original: &str,
    left: &Expr,
    right: &Expr,
    coefficient: f64,
    constant: f64,
) -> AssistantResponse {
    if coefficient == 0.0 {
        return unsupported("This equation has no unique one-variable linear solution.");
    }
    let solution = -constant / coefficient;
    if !solution.is_finite() {
        return unsupported("This equation produced a non-finite result and was not applied.");
    }
    let Some(scale) = polynomial_evaluation_scale(0.0, coefficient, constant, solution) else {
        return unsupported("The linear candidate could not be verified safely.");
    };
    let Some(verification) = equation_verification(left, right, solution, scale) else {
        return unsupported("The linear candidate did not satisfy the original equation.");
    };
    let normalized = format!(
        "{}*x + {} = 0",
        format_number(coefficient),
        format_number(constant)
    );
    let solution_text = format!("x = {}", format_number(solution));
    AssistantResponse {
        schema_version: grafito_assistant_types::ASSISTANT_SCHEMA_VERSION,
        status: LocalAssistantStatus::Solved,
        answer: solution_text.clone(),
        derivation: vec![
            DerivationStep {
                before: original.to_string(),
                after: normalized,
                rule: "Move both sides to a zero-right-hand-side form".into(),
                verification:
                    "The transformed equation subtracts the same right-hand side from both sides."
                        .into(),
            },
            DerivationStep {
                before: format!(
                    "{}*x = {}",
                    format_number(coefficient),
                    format_number(-constant)
                ),
                after: solution_text.clone(),
                rule: "Divide by the non-zero linear coefficient".into(),
                verification,
            },
        ],
        plan: None,
    }
}

fn solve_quadratic_equation(
    original: &str,
    left: &Expr,
    right: &Expr,
    a: f64,
    b: f64,
    c: f64,
) -> AssistantResponse {
    let Some((scaled_a, scaled_b, scaled_c)) = normalize_quadratic_coefficients(a, b, c) else {
        return unsupported("This quadratic could not be normalized safely and was not applied.");
    };
    let discriminant = scaled_b.mul_add(scaled_b, -4.0 * scaled_a * scaled_c);
    let Some((classification, error_bound)) =
        classify_quadratic_discriminant(scaled_a, scaled_b, scaled_c, discriminant)
    else {
        return unsupported("This quadratic discriminant could not be classified safely.");
    };
    let normalized = format!(
        "{}*x^2 + {}*x + {} = 0",
        format_number(a),
        format_number(b),
        format_number(c)
    );
    if matches!(classification, DiscriminantClassification::Negative) {
        return AssistantResponse {
            schema_version: grafito_assistant_types::ASSISTANT_SCHEMA_VERSION,
            status: LocalAssistantStatus::Solved,
            answer: "No real solutions.".into(),
            derivation: vec![
                DerivationStep {
                    before: original.to_string(),
                    after: normalized,
                    rule: "Move both sides to a zero-right-hand-side form".into(),
                    verification: "The transformed equation subtracts the same right-hand side from both sides.".into(),
                },
                DerivationStep {
                    before: "b^2 - 4*a*c".into(),
                    after: format_number(discriminant),
                    rule: "Compute the discriminant".into(),
                    verification: "A negative discriminant has no real square root.".into(),
                },
            ],
            plan: None,
        };
    }

    if matches!(classification, DiscriminantClassification::Ambiguous) {
        let repeated = -scaled_b / (2.0 * scaled_a);
        if !repeated.is_finite() {
            return unsupported("The repeated quadratic candidate was not finite.");
        }
        let Some(scale) = polynomial_evaluation_scale(a, b, c, repeated) else {
            return unsupported("The repeated quadratic candidate could not be verified safely.");
        };
        let Some(verification) = equation_verification(left, right, repeated, scale) else {
            return unsupported(
                "The numerically ambiguous quadratic discriminant did not yield a verified repeated root.",
            );
        };
        let answer = format!("x = {}", format_number(repeated));
        return AssistantResponse {
            schema_version: grafito_assistant_types::ASSISTANT_SCHEMA_VERSION,
            status: LocalAssistantStatus::Solved,
            answer: answer.clone(),
            derivation: vec![
                DerivationStep {
                    before: original.to_string(),
                    after: normalized,
                    rule: "Move both sides to a zero-right-hand-side form".into(),
                    verification:
                        "The transformed equation subtracts the same right-hand side from both sides."
                            .into(),
                },
                DerivationStep {
                    before: "b^2 - 4*a*c".into(),
                    after: format_number(discriminant),
                    rule: "Treat an ambiguous discriminant as zero only after verifying one candidate"
                        .into(),
                    verification: format!(
                        "Its magnitude is within the rounding bound {}; {}",
                        format_number(error_bound),
                        verification
                    ),
                },
            ],
            plan: None,
        };
    }

    let square_root = discriminant.sqrt();
    let denominator = 2.0 * scaled_a;
    let first = (-scaled_b - square_root) / denominator;
    let second = (-scaled_b + square_root) / denominator;
    if !first.is_finite() || !second.is_finite() {
        return unsupported("This quadratic produced a non-finite result and was not applied.");
    }
    let Some(first_scale) = polynomial_evaluation_scale(a, b, c, first) else {
        return unsupported("The first quadratic candidate could not be verified safely.");
    };
    let Some(first_verification) = equation_verification(left, right, first, first_scale) else {
        return unsupported("The first quadratic candidate did not satisfy the original equation.");
    };
    let Some(second_scale) = polynomial_evaluation_scale(a, b, c, second) else {
        return unsupported("The second quadratic candidate could not be verified safely.");
    };
    let Some(second_verification) = equation_verification(left, right, second, second_scale) else {
        return unsupported(
            "The second quadratic candidate did not satisfy the original equation.",
        );
    };
    let answer = if approximately_equal(first, second) {
        format!("x = {}", format_number(first))
    } else {
        format!(
            "x = {} or x = {}",
            format_number(first),
            format_number(second)
        )
    };
    AssistantResponse {
        schema_version: grafito_assistant_types::ASSISTANT_SCHEMA_VERSION,
        status: LocalAssistantStatus::Solved,
        answer: answer.clone(),
        derivation: vec![
            DerivationStep {
                before: original.to_string(),
                after: normalized,
                rule: "Move both sides to a zero-right-hand-side form".into(),
                verification:
                    "The transformed equation subtracts the same right-hand side from both sides."
                        .into(),
            },
            DerivationStep {
                before: "b^2 - 4*a*c".into(),
                after: format_number(discriminant),
                rule: "Compute the discriminant".into(),
                verification: "The discriminant is finite and non-negative.".into(),
            },
            DerivationStep {
                before: "x = (-b ± sqrt(discriminant)) / (2*a)".into(),
                after: answer,
                rule: "Apply the quadratic formula".into(),
                verification: format!(
                    "Substitution checks: {}; {}",
                    first_verification, second_verification
                ),
            },
        ],
        plan: None,
    }
}

fn solve_graph_request(request: &AssistantRequest, expression: &str) -> AssistantResponse {
    if expression.is_empty() || expression.len() > request.budget.max_input_chars {
        return unsupported("The graph expression is empty or exceeds the local budget.");
    }
    let samples = [-1.0, 0.0, 1.0];
    if !samples.iter().any(|sample| {
        evaluate_with_context(expression, request, Some(*sample)).is_ok_and(f64::is_finite)
    }) {
        return unsupported("The graph expression could not be evaluated safely near the origin.");
    }

    let mut plan = ProposedPlan::new(
        request.context.basis(),
        vec![AssistantOperation::CreateGraph {
            expression: expression.to_string(),
            variable: "x".into(),
            domain_min: -10.0,
            domain_max: 10.0,
        }],
    );
    plan.summary = format!("Preview graph y = {expression}");
    AssistantResponse {
        schema_version: grafito_assistant_types::ASSISTANT_SCHEMA_VERSION,
        status: LocalAssistantStatus::Solved,
        answer: format!(
            "Prepared a safe preview for y = {expression}. Apply it only after reviewing the diff."
        ),
        derivation: vec![DerivationStep {
            before: expression.to_string(),
            after: format!("y = {expression}, -10 <= x <= 10"),
            rule: "Recognize a graphing request".into(),
            verification:
                "The expression evaluated to a finite value at at least one bounded sample.".into(),
        }],
        plan: Some(plan),
    }
}

fn parse_graph_request(problem: &str) -> Option<&str> {
    let trimmed = problem.trim();
    let lower = trimmed.to_ascii_lowercase();
    for prefix in ["graph", "plot", "graficar", "grafica", "gráfica"] {
        if let Some(rest) = lower.strip_prefix(prefix) {
            if !rest.is_empty() && !rest.starts_with(char::is_whitespace) && !rest.starts_with(':')
            {
                continue;
            }
            let offset = trimmed.len() - rest.len();
            let mut expression = trimmed[offset..].trim_start_matches([':', ' ']).trim();
            if let Some(after_y) = expression.strip_prefix("y") {
                expression = after_y.trim_start();
                if let Some(after_equals) = expression.strip_prefix('=') {
                    expression = after_equals.trim();
                }
            }
            return Some(expression);
        }
    }
    None
}

fn evaluate_with_context(
    expression: &str,
    request: &AssistantRequest,
    x: Option<f64>,
) -> Result<f64, String> {
    let mut variables: Vec<(String, f64)> = request
        .context
        .variables
        .iter()
        .filter(|(_, value)| value.is_finite())
        .map(|(name, value)| (name.clone(), *value))
        .collect();
    if let Some(x) = x {
        variables.retain(|(name, _)| name != "x");
        variables.push(("x".into(), x));
    }
    evaluate(expression, &variables)
}

fn equation_verification(left: &Expr, right: &Expr, x: f64, scale: f64) -> Option<String> {
    let left_value = left.eval_at("x", x);
    let right_value = right.eval_at("x", x);
    if !left_value.is_finite() || !right_value.is_finite() || !scale.is_finite() || scale < 0.0 {
        return None;
    }
    let residual = (left_value - right_value).abs();
    let tolerance = RESIDUAL_EPSILON * scale;
    if residual <= tolerance {
        Some(format!(
            "At x = {}, both sides evaluate to {} with residual {} <= {}.",
            format_number(x),
            format_number(left_value),
            format_number(residual),
            format_number(tolerance)
        ))
    } else {
        None
    }
}

fn polynomial_evaluation_scale(a: f64, b: f64, c: f64, x: f64) -> Option<f64> {
    let x = x.abs();
    let quadratic = a.abs() * x * x;
    let linear = b.abs() * x;
    let scale = quadratic + linear + c.abs();
    scale.is_finite().then_some(scale)
}

fn normalize_quadratic_coefficients(a: f64, b: f64, c: f64) -> Option<(f64, f64, f64)> {
    let magnitude = a.abs().max(b.abs()).max(c.abs());
    if !magnitude.is_finite() || magnitude == 0.0 {
        return None;
    }
    // Una escala potencia de dos evita cambiar las razones de coeficientes representables.
    let exponent = magnitude.log2().floor() as i32;
    let scale = power_of_two_scale(exponent.min(1_023))?;
    if !scale.is_finite() || scale == 0.0 {
        return None;
    }

    let scaled_a = a / scale;
    let scaled_b = b / scale;
    let scaled_c = c / scale;
    if !scaled_a.is_finite()
        || !scaled_b.is_finite()
        || !scaled_c.is_finite()
        || scaled_a == 0.0
        || (b != 0.0 && scaled_b == 0.0)
        || (c != 0.0 && scaled_c == 0.0)
    {
        return None;
    }
    Some((scaled_a, scaled_b, scaled_c))
}

#[derive(Clone, Copy)]
enum DiscriminantClassification {
    Negative,
    Ambiguous,
    Positive,
}

fn classify_quadratic_discriminant(
    a: f64,
    b: f64,
    c: f64,
    discriminant: f64,
) -> Option<(DiscriminantClassification, f64)> {
    let magnitude = b * b + 4.0 * a.abs() * c.abs();
    let error_bound = DISCRIMINANT_ERROR_MULTIPLIER * f64::EPSILON * magnitude;
    if !discriminant.is_finite() || !magnitude.is_finite() || !error_bound.is_finite() {
        return None;
    }
    let classification = if discriminant > error_bound {
        DiscriminantClassification::Positive
    } else if discriminant < -error_bound {
        DiscriminantClassification::Negative
    } else {
        DiscriminantClassification::Ambiguous
    };
    Some((classification, error_bound))
}

fn power_of_two_scale(exponent: i32) -> Option<f64> {
    let bits = if (-1_022..=1_023).contains(&exponent) {
        ((exponent + 1_023) as u64) << 52
    } else if (-1_074..=-1_023).contains(&exponent) {
        1_u64 << ((exponent + 1_074) as u32)
    } else {
        return None;
    };
    Some(f64::from_bits(bits))
}

fn enforce_local_response_budget(
    response: AssistantResponse,
    budget: &grafito_assistant_types::RequestBudget,
) -> AssistantResponse {
    if response.validate(budget).is_ok() {
        response
    } else {
        let answer: String = "Assistant response exceeded its configured safety budget."
            .chars()
            .take(budget.max_output_chars)
            .collect();
        AssistantResponse::message(LocalAssistantStatus::Rejected, answer)
    }
}

fn unsupported(message: impl Into<String>) -> AssistantResponse {
    AssistantResponse::message(LocalAssistantStatus::Unsupported, message)
}

fn format_number(value: f64) -> String {
    if value == 0.0 {
        "0".into()
    } else {
        value.to_string()
    }
}

fn approximately_equal(left: f64, right: f64) -> bool {
    left.to_bits() == right.to_bits()
        || (left - right).abs() <= ROOT_DEDUP_EPSILON * left.abs().max(right.abs())
}

/// Configuración no secreta de un perfil OpenAI-compatible.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProviderSettings {
    /// Perfil para el que se aplica la configuración.
    pub profile: ProviderProfile,
    /// URL base API, validada antes de una petición.
    pub endpoint: String,
    /// Identificador de modelo transmitido al proveedor.
    pub model: String,
    /// Referencia de configuración a una variable de entorno de clave de API.
    ///
    /// Los perfiles personalizados sólo admiten referencias con el alcance
    /// `GRAFITO_ASSISTANT_CUSTOM_*_API_KEY`, nunca nombres de secretos genéricos.
    pub api_key_env: Option<String>,
    /// Capacidades que deben verificarse antes de serializar adjuntos.
    pub capabilities: ProviderCapabilities,
}

impl ProviderSettings {
    /// Construye la configuración conservadora por defecto de un perfil conocido.
    pub fn for_profile(profile: ProviderProfile, model: impl Into<String>) -> Self {
        let endpoint = match profile {
            ProviderProfile::OpenCodeGo => "https://opencode.ai/zen/go/v1",
            ProviderProfile::DeepSeek => "https://api.deepseek.com/v1",
            ProviderProfile::OllamaLocal => "http://127.0.0.1:11434/v1",
            ProviderProfile::CustomOpenAiCompatible => "",
        };
        Self {
            profile,
            endpoint: endpoint.into(),
            model: model.into(),
            api_key_env: profile.api_key_env().map(str::to_owned),
            capabilities: profile.capabilities(),
        }
    }

    /// Construye un perfil personalizado que no puede reutilizar claves de perfiles conocidos.
    pub fn custom_openai_compatible(
        endpoint: impl Into<String>,
        model: impl Into<String>,
        api_key_reference: impl Into<String>,
    ) -> Result<Self, String> {
        let settings = Self {
            profile: ProviderProfile::CustomOpenAiCompatible,
            endpoint: endpoint.into(),
            model: model.into(),
            api_key_env: Some(api_key_reference.into()),
            capabilities: ProviderProfile::CustomOpenAiCompatible.capabilities(),
        };
        settings.validate()?;
        Ok(settings)
    }

    /// Sobrescribe el endpoint sólo si sigue siendo válido para el perfil elegido.
    pub fn with_endpoint(mut self, endpoint: impl Into<String>) -> Result<Self, String> {
        self.endpoint = endpoint.into();
        self.validate()?;
        Ok(self)
    }

    /// Declara capacidades más específicas cuando se conoce el modelo elegido.
    pub fn with_capabilities(mut self, capabilities: ProviderCapabilities) -> Self {
        self.capabilities = capabilities;
        self
    }

    fn validate(&self) -> Result<(), String> {
        let endpoint = validate_endpoint(&self.endpoint)?;
        if self.model.trim().is_empty() || self.model.len() > 256 {
            return Err("remote model identifier is invalid".into());
        }
        if !self.capabilities.openai_compatible {
            return Err("provider is not declared OpenAI-compatible".into());
        }
        let host = endpoint
            .host_str()
            .ok_or_else(|| "remote endpoint must include a host".to_string())?;
        match self.profile {
            ProviderProfile::OpenCodeGo => {
                validate_named_endpoint(&endpoint, "opencode.ai", "/zen/go/v1")?;
                if self.api_key_env.as_deref() != ProviderProfile::OpenCodeGo.api_key_env() {
                    return Err("OpenCodeGo must use its own API key reference".into());
                }
            }
            ProviderProfile::DeepSeek => {
                validate_named_endpoint(&endpoint, "api.deepseek.com", "/v1")?;
                if self.api_key_env.as_deref() != ProviderProfile::DeepSeek.api_key_env() {
                    return Err("DeepSeek must use its own API key reference".into());
                }
            }
            ProviderProfile::OllamaLocal => {
                if !is_loopback_host(host) {
                    return Err("Ollama local endpoints must use a loopback host".into());
                }
                if self.api_key_env.is_some() {
                    return Err("Ollama local profiles must not use an API key".into());
                }
            }
            ProviderProfile::CustomOpenAiCompatible => {
                if endpoint.scheme() != "https" {
                    return Err("custom OpenAI-compatible endpoints must use HTTPS".into());
                }
                let key_reference = self.api_key_env.as_deref().ok_or_else(|| {
                    "custom endpoints require a distinct API key reference".to_string()
                })?;
                if !is_custom_api_key_reference(key_reference) {
                    return Err(
                        "custom API key reference is invalid or reserved for a named provider"
                            .into(),
                    );
                }
            }
        }
        Ok(())
    }
}

fn validate_named_endpoint(
    endpoint: &Url,
    expected_host: &str,
    expected_path: &str,
) -> Result<(), String> {
    if endpoint.scheme() != "https"
        || endpoint.host_str() != Some(expected_host)
        || endpoint.port().is_some()
        || endpoint.path().trim_end_matches('/') != expected_path
        || endpoint.query().is_some()
        || endpoint.fragment().is_some()
    {
        return Err("named provider endpoint is not in its allowlist".into());
    }
    Ok(())
}

/// Deriva el endpoint Chat Completions desde una base de API validada.
pub fn chat_completion_endpoint(settings: &ProviderSettings) -> Result<Url, String> {
    endpoint_with_path(settings, "chat/completions")
}

/// Deriva el endpoint Responses (OpenAI Responses API) desde la base validada.
/// Lo usan los modelos que no responden por Chat Completions (Muse Spark).
pub fn responses_endpoint(settings: &ProviderSettings) -> Result<Url, String> {
    endpoint_with_path(settings, "responses")
}

/// Deriva el endpoint Models desde una base de API validada.
pub fn models_endpoint(settings: &ProviderSettings) -> Result<Url, String> {
    endpoint_with_path(settings, "models")
}

/// Deriva el endpoint Anthropic Messages para modelos OpenCode que lo requieren.
pub fn messages_endpoint(settings: &ProviderSettings) -> Result<Url, String> {
    endpoint_with_path(settings, "messages")
}

/// Matriz de fallback por familia OpenCodeGo (router `remote_protocol`).
///
/// | Familia / modelo | Protocolo (`RemoteProtocol`) | Endpoint `POST` | Auth |
/// |---|---|---|---|
/// | `muse-spark-*` (1.2/1.3, futuras 1.x por `contains`) | `OpenAiResponses` | `{base}/responses` | Bearer (`sanitize_api_key`) |
/// | `mimo-2.5-vl` (visión) | `AnthropicMessages` | `{base}/messages` | `x-api-key` + `anthropic-version: 2023-06-01` |
/// | `fusion` | `Fusion` | draft `{base}/messages` (shape mimo, sin historial) + audit `{base}/chat/completions` (`deepseek-v4-pro`) | draft `x-api-key`, audit Bearer |
/// | resto OpenCodeGo (`deepseek-*`, `glm-*`, …) + `DeepSeek`/`OllamaLocal`/`Custom` | `OpenAiChatCompletions` | `{base}/chat/completions` (`stream:false`) | Bearer u omitida (Ollama local) |
///
/// Comportamiento ante fallos (sin reintento automático en este crate; el
/// worker es cancelable y dormir bloquearía la cancelación):
///
/// | Fallo | chat (`send_openai_request`) | responses (`request_responses_completion`) | anthropic (`request_anthropic_completion`) | fusion (`request_fusion_completion_with_endpoints`) |
/// |---|---|---|---|---|
/// | HTTP 500 (u otro no-2xx) | `remote assistant returned HTTP {status}: {body:500}` | mismo formato, cap `RESPONSES_MAX_BODY_BYTES` 64 KiB antes de parsear | mismo formato | draft falla → `Fusion could not create a draft: {error}`; audit falla → `Fusion could not complete the audit; its draft was discarded.` (nunca se muestra el borrador) |
/// | HTTP 429 | mismo formato + ` (reintentá en {N}s)` con `Retry-After` parseado (entero o fecha HTTP, clamp 1..120s) | ídem | ídem | ídem por fase (draft/audit); el mensaje final conserva el prefijo Fusion |
/// | timeout/transporte | `transport_error`: `timed out after {N}s` / `could not connect` / `request failed`, sin URL ni clave | ídem | ídem (requiere clave, si falta: `API key is unavailable`) | draft usa mitad del timeout, audit el resto (`checked_sub`, nunca 0); timeout total conserva `effective_remote_timeout` |
/// | cuerpo largo | truncado a `MAX_ERROR_BODY_CHARS` 500 chars, sin eco de secretos | ídem | ídem | ídem |
/// | `status:incomplete` / `stop:max_tokens` | N/A (`finish_reason` debe ser `stop`, si es `length` → schema error sin eco) | `RemoteCompletion{truncated:true}` con el texto parcial | `stop_reason:max_tokens` → `truncated:true` | se propaga `truncated` del audit final |
///
/// Fallback de sesión (no en este crate): `grafito-app/src/assistant.rs`
/// reintenta una vez `muse-spark --500/timeout--> deepseek-v4-flash` sin tocar
/// la preferencia guardada. Modo agente con tools (`agent.rs`): sólo soporta
/// `OpenAiChatCompletions`; Spark/Fusion devuelven error explícito que sugiere
/// chat simple o deepseek; su `HTTP {status}` aún no incluye `Retry-After`
/// (deuda documentada).
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum RemoteProtocol {
    OpenAiChatCompletions,
    OpenAiResponses,
    AnthropicMessages,
    Fusion,
}

impl RemoteProtocol {
    fn name(self) -> &'static str {
        match self {
            Self::OpenAiChatCompletions => "chat_completions",
            Self::OpenAiResponses => "responses",
            Self::AnthropicMessages => "anthropic_messages",
            Self::Fusion => "fusion",
        }
    }
}

fn remote_protocol(settings: &ProviderSettings) -> RemoteProtocol {
    if settings.profile != ProviderProfile::OpenCodeGo {
        return RemoteProtocol::OpenAiChatCompletions;
    }
    if uses_responses_api(&settings.model) {
        return RemoteProtocol::OpenAiResponses;
    }
    match settings.model.as_str() {
        // MiMo 2.5-VL viaja con el protocolo Anthropic Messages del proveedor.
        OPENCODE_VISION_MODEL => RemoteProtocol::AnthropicMessages,
        OPENCODE_FUSION_MODEL => RemoteProtocol::Fusion,
        _ => RemoteProtocol::OpenAiChatCompletions,
    }
}

/// Timeout efectivo para un turno simple: clamp `100ms..=120s`.
///
/// `RequestBudget::default` es 60s; el modo agente usa 30s por turno
/// (`AgentBudget::default.per_turn_timeout`). El clamp garantiza que ningún
/// path construye `Duration::ZERO` (reqwest lo trataría como timeout
/// inmediato / sin espera útil) ni supera el cap absoluto de 120s. El worker
/// conserva este timeout y chequea `CancellationToken` antes y después de
/// `send()`; no se duerme dentro del turno.
fn effective_remote_timeout_ms(timeout_ms: u64) -> u64 {
    timeout_ms.clamp(REMOTE_TIMEOUT_MIN_MS, REMOTE_TIMEOUT_MAX_MS)
}

fn effective_remote_timeout(timeout_ms: u64) -> Duration {
    Duration::from_millis(effective_remote_timeout_ms(timeout_ms))
}

/// Trunca el cuerpo de error a `MAX_ERROR_BODY_CHARS` por chars (no bytes)
/// para no partir UTF-8. El cuerpo nunca contiene la API key (sólo dato del
/// proveedor); el truncado evita saturar logs/UI.
fn truncate_error_body(body: &str) -> String {
    body.chars().take(MAX_ERROR_BODY_CHARS).collect()
}

/// Parsea `Retry-After` (segundos enteros o fecha HTTP) con clamp 1..=120s.
///
/// Acepta `Delay-Seconds` (`"7"`, `"0"` → 1, `"3600"` → 120, negativos → 1)
/// y los tres formatos de fecha HTTP (IMF-fixdate, RFC 850, asctime); una
/// fecha futura se convierte en delta vs `now`, una pasada en 1s. Retorna
/// `None` si el valor es ilegible (el 429 se informa igual, sin sugerencia).
/// Puro y sin I/O: el worker nunca duerme, sólo anota "reintentá en Ns".
fn parse_retry_after_secs(value: &str) -> Option<u64> {
    let trimmed = value.trim();
    if trimmed.is_empty() {
        return None;
    }
    if let Ok(secs) = trimmed.parse::<i64>() {
        let clamped = secs.clamp(RETRY_AFTER_MIN_SECS as i64, RETRY_AFTER_MAX_SECS as i64);
        return Some(clamped as u64);
    }
    parse_http_date_to_system_time(trimmed).map(|when| {
        let now = std::time::SystemTime::now();
        match when.duration_since(now) {
            Ok(delta) => delta
                .as_secs()
                .clamp(RETRY_AFTER_MIN_SECS, RETRY_AFTER_MAX_SECS),
            Err(_) => RETRY_AFTER_MIN_SECS,
        }
    })
}

fn retry_after_secs_from_headers(headers: &reqwest::header::HeaderMap) -> Option<u64> {
    headers
        .get(reqwest::header::RETRY_AFTER)
        .and_then(|value| value.to_str().ok())
        .and_then(parse_retry_after_secs)
}

/// Error HTTP con cuerpo truncado y, sólo en 429 con `Retry-After` legible,
/// sufijo ` (reintentá en {N}s)`. No incluye URL, headers ni claves.
fn http_status_error(status: u16, body: &str, retry_after_secs: Option<u64>) -> String {
    let snippet = truncate_error_body(body);
    if status == 429 {
        if let Some(secs) = retry_after_secs {
            return format!("remote assistant returned HTTP 429: {snippet} (reintentá en {secs}s)");
        }
    }
    format!("remote assistant returned HTTP {status}: {snippet}")
}

fn month_number(month: &str) -> Option<u32> {
    match month {
        "Jan" => Some(1),
        "Feb" => Some(2),
        "Mar" => Some(3),
        "Apr" => Some(4),
        "May" => Some(5),
        "Jun" => Some(6),
        "Jul" => Some(7),
        "Aug" => Some(8),
        "Sep" => Some(9),
        "Oct" => Some(10),
        "Nov" => Some(11),
        "Dec" => Some(12),
        _ => None,
    }
}

/// Días desde 1970-01-01 (algoritmo civil de Howard Hinnant, sólo fechas
/// Gregorianas; suficiente para `Retry-After` que siempre es futuro cercano).
fn days_from_civil(year: i64, month: u32, day: u32) -> Option<i64> {
    if !(1..=12).contains(&month) || !(1..=31).contains(&day) {
        return None;
    }
    let year = if month <= 2 { year - 1 } else { year };
    let era = year.div_euclid(400);
    let yoe = year.rem_euclid(400) as u64;
    let mp = (month as i64 + 9).rem_euclid(12) as u64;
    let doy = (153 * mp + 2) / 5 + u64::from(day) - 1;
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
    let days = era * 146_097 + doe as i64 - 719_468;
    Some(days)
}

fn parse_hms(text: &str) -> Option<(u64, u64, u64)> {
    let mut parts = text.split(':');
    let hour: u64 = parts.next()?.parse().ok()?;
    let minute: u64 = parts.next()?.parse().ok()?;
    let second: u64 = parts.next()?.parse().ok()?;
    if parts.next().is_some() || hour > 23 || minute > 59 || second > 60 {
        return None;
    }
    Some((hour, minute, second))
}

fn http_date_to_system_time(
    year: i64,
    month: u32,
    day: u32,
    hour: u64,
    minute: u64,
    second: u64,
) -> Option<std::time::SystemTime> {
    let days = days_from_civil(year, month, day)?;
    let secs = days
        .checked_mul(86_400)?
        .checked_add((hour * 3_600 + minute * 60 + second) as i64)?;
    if secs < 0 {
        return None;
    }
    std::time::UNIX_EPOCH.checked_add(Duration::from_secs(secs as u64))
}

/// Parsea una fecha HTTP en los tres formatos (`IMF-fixdate`, `RFC 850`,
/// `asctime`) a `SystemTime`. Sin dependencias nuevas (httpdate no es dep
/// directa); sólo lo usa `parse_retry_after_secs`.
fn parse_http_date_to_system_time(value: &str) -> Option<std::time::SystemTime> {
    let value = value.trim();
    // IMF-fixdate: "Sun, 06 Nov 1994 08:49:37 GMT".
    if let Some(rest) = value.split_once(',') {
        let rest = rest.1.trim();
        // Distingue RFC 850 ("06-Nov-94 ...") por el guion en la fecha.
        if rest.chars().take(7).any(|c| c == '-') {
            // RFC 850: "Sunday, 06-Nov-94 08:49:37 GMT".
            let mut parts = rest.split_whitespace();
            let date = parts.next()?;
            let time = parts.next()?;
            let zone = parts.next()?;
            if zone != "GMT" || parts.next().is_some() {
                return None;
            }
            let mut date_parts = date.split('-');
            let day: u32 = date_parts.next()?.parse().ok()?;
            let month = month_number(date_parts.next()?)?;
            let year2: i64 = date_parts.next()?.parse().ok()?;
            if date_parts.next().is_some() {
                return None;
            }
            let year = if (70..=99).contains(&year2) {
                1900 + year2
            } else {
                2000 + year2
            };
            let (hour, minute, second) = parse_hms(time)?;
            return http_date_to_system_time(year, month, day, hour, minute, second);
        }
        // IMF-fixdate: "06 Nov 1994 08:49:37 GMT".
        let mut parts = rest.split_whitespace();
        let day: u32 = parts.next()?.parse().ok()?;
        let month = month_number(parts.next()?)?;
        let year: i64 = parts.next()?.parse().ok()?;
        let (hour, minute, second) = parse_hms(parts.next()?)?;
        if parts.next()? != "GMT" || parts.next().is_some() {
            return None;
        }
        return http_date_to_system_time(year, month, day, hour, minute, second);
    }
    // asctime: "Sun Nov  6 08:49:37 1994" (día con padding de espacio).
    let parts: Vec<&str> = value.split_whitespace().collect();
    if parts.len() == 5 {
        let month = month_number(parts[1])?;
        let day: u32 = parts[2].parse().ok()?;
        let (hour, minute, second) = parse_hms(parts[3])?;
        let year: i64 = parts[4].parse().ok()?;
        return http_date_to_system_time(year, month, day, hour, minute, second);
    }
    None
}

// ADR-001 — Streaming SSE para Responses API (`response.stream=true`).
//
// Estado actual (2026-09-04): transporte NO-streaming (`stream` ausente en el
// payload, `max_output_tokens` con piso 2048). Verificado contra el endpoint
// real: 200 en ~2s para "ok". Se mantiene así en este turno para no romper el
// wire format ni la cancelación cooperativa (el worker sólo chequea
// `CancellationToken` antes/después de `send()`).
//
// Diseño futuro (no cableado; helpers puros abajo como primer paso):
//  1. Payload: `{"model","instructions","input","max_output_tokens","stream":true}`
//     + header `Accept: text/event-stream`.
//  2. Lectura: `reqwest::blocking::Response::chunk()` en bucle con deadline
//     absoluta (`effective_remote_timeout`) y poll de cancelación cada ~200ms
//     (precedente: `grafito-anim` engine poll 200ms). Cada chunk se acumula en
//     un buffer con cap (p.ej. `RESPONSES_MAX_BODY_BYTES`); líneas parciales se
//     reensamblan por `\n` antes de parsear.
//  3. Parser: líneas `event: <tipo>` + `data: <json>`; tipos relevantes
//     `response.output_text.delta` (acumula `delta`), `response.completed`
//     (cierra con `truncated:false`), `response.incomplete` (cierra con
//     `truncated:true`), `response.failed` (error acotado 200 chars).
//     `data: [DONE]` termina el stream. Ver `responses_sse_delta_text` y
//     `collect_responses_sse_text`.
//  4. Progreso opcional: `Option<&mut dyn FnMut(&str)>` invocada sólo con el
//     texto acumulado acotado (`max_output_chars`); nunca bloquea ni duerme:
//     si el callback pisa cancelación, el loop aborta en el próximo poll.
//  5. Fallback: cualquier error de stream (content-type inesperado, JSON
//     inválido, timeout) cae a un único error `schema`/`transport` sin eco de
//     secretos, idéntico al path no-streaming. No se reintenta en el worker.
//  6. Puerta de activación: feature o parámetro `stream: bool` en
//     `RequestBudget`/payload builder, con el default actual `false` hasta que
//     la UI necesite render progresivo. Cuando se active, el test stub debe
//     servir `Content-Type: text/event-stream` con deltas fragmentados en
//     varios `write_all` + `flush` para probar reensamblado y cancelación.
//
// Decisión de este turno: NO se cambia el wire (sin `stream:true`, sin
// `progress` en el path de red). Sólo se agregan los parsers puros + tests
// unitarios para dejar el diseño verificado sin riesgo.
/// Callback de progreso para futuro streaming SSE (ADR-001, aún no cableado
/// al transporte: el wire sigue siendo no-streaming para no romper el formato
/// verificado). Recibe el texto acumulado acotado; nunca duerme ni bloquea.
pub type ResponsesProgressCallback<'a> = dyn FnMut(&str) + Send + 'a;

/// Extrae el delta de texto de un evento SSE de Responses.
///
/// Acepta el JSON de una línea `data:` con `type: "response.output_text.delta"`
/// (y alias `"response.text.delta"`) y devuelve `delta`. Retorna `None` para
/// eventos de control (`response.completed`, `response.failed`, …) o JSON
/// ilegible. Puro, sin I/O: primer paso del ADR-001.
pub fn responses_sse_delta_text(event_data_json: &str) -> Option<String> {
    let value: Value = serde_json::from_str(event_data_json.trim()).ok()?;
    let event_type = value.get("type").and_then(Value::as_str).unwrap_or("");
    if event_type == "response.output_text.delta" || event_type == "response.text.delta" {
        value
            .get("delta")
            .and_then(Value::as_str)
            .map(str::to_owned)
    } else {
        None
    }
}

/// Junta deltas SSE de un cuerpo `text/event-stream` completo (helper futuro).
///
/// Itera líneas `data: <json>`, ignora `:ping`/`event:` y `data: [DONE]`,
/// concatena `responses_sse_delta_text` y detecta `response.incomplete` para
/// marcar `truncated`. Puro y acotado por el llamante; el transporte real aún
/// no lo usa (ADR-001).
pub fn collect_responses_sse_text(sse_body: &str) -> (String, bool) {
    let mut text = String::new();
    let mut truncated = false;
    for line in sse_body.lines() {
        let line = line.trim();
        let Some(data) = line.strip_prefix("data:") else {
            continue;
        };
        let data = data.trim();
        if data.is_empty() || data == "[DONE]" {
            continue;
        }
        if data.contains("\"response.incomplete\"") {
            truncated = true;
            continue;
        }
        if data.contains("\"response.failed\"") {
            continue;
        }
        if let Some(delta) = responses_sse_delta_text(data) {
            text.push_str(&delta);
        }
    }
    (text, truncated)
}

fn endpoint_with_path(settings: &ProviderSettings, suffix: &str) -> Result<Url, String> {
    settings.validate()?;
    let mut endpoint = validate_endpoint(&settings.endpoint)?;
    let base_path = endpoint.path().trim_end_matches('/');
    let suffix = suffix.trim_start_matches('/');
    if !base_path.ends_with(suffix) {
        endpoint.set_path(&format!("{base_path}/{suffix}"));
    }
    endpoint.set_query(None);
    endpoint.set_fragment(None);
    Ok(endpoint)
}

fn is_custom_api_key_reference(value: &str) -> bool {
    value.len() <= MAX_CUSTOM_API_KEY_REFERENCE_LEN
        && value
            .strip_prefix(CUSTOM_API_KEY_PREFIX)
            .and_then(|scope| scope.strip_suffix(CUSTOM_API_KEY_SUFFIX))
            .is_some_and(|scope| {
                matches!(scope.chars().next(), Some(first) if first.is_ascii_uppercase())
                    && scope.chars().all(|character| {
                        character.is_ascii_uppercase()
                            || character.is_ascii_digit()
                            || character == '_'
                    })
            })
}

/// Valida endpoints sin permitir HTTP fuera de loopback ni credenciales embebidas.
pub fn validate_endpoint(endpoint: &str) -> Result<Url, String> {
    let parsed = Url::parse(endpoint).map_err(|_| "remote endpoint URL is invalid".to_string())?;
    if !parsed.username().is_empty() || parsed.password().is_some() {
        return Err("remote endpoint must not contain embedded credentials".into());
    }
    let host = parsed
        .host_str()
        .ok_or_else(|| "remote endpoint must include a host".to_string())?;
    match parsed.scheme() {
        "https" => Ok(parsed),
        "http" if is_loopback_host(host) => Ok(parsed),
        "http" => Err("remote endpoint must use HTTPS outside loopback".into()),
        _ => Err("remote endpoint must use HTTP or HTTPS".into()),
    }
}

fn is_loopback_host(host: &str) -> bool {
    let host = host.trim_matches(['[', ']']);
    host.parse::<IpAddr>().is_ok_and(|address| match address {
        IpAddr::V4(address) => address.is_loopback(),
        IpAddr::V6(address) => address.is_loopback(),
    })
}

fn validate_decoded_attachments(
    request: &AssistantRequest,
    limits: &AttachmentLimits,
) -> Result<(), String> {
    let mut total_pixels = 0_u64;
    for attachment in &request.attachments {
        total_pixels = total_pixels
            .checked_add(decoded_attachment_pixels(attachment, limits)?)
            .ok_or_else(|| "assistant attachment decoded pixel budget overflow".to_string())?;
        if total_pixels > limits.max_total_pixels {
            return Err("assistant attachment decoded pixel budget exceeded".into());
        }
    }
    Ok(())
}

/// Verifica formato, dimensiones y datos reales de un adjunto antes de usarlo.
pub fn validate_attachment(
    attachment: &ImageAttachment,
    limits: &AttachmentLimits,
) -> Result<(), String> {
    decoded_attachment_pixels(attachment, limits).map(|_| ())
}

fn decoded_attachment_pixels(
    attachment: &ImageAttachment,
    limits: &AttachmentLimits,
) -> Result<u64, String> {
    decode_attachment(attachment, limits).map(|(_, _, pixels)| pixels)
}

fn decode_attachment(
    attachment: &ImageAttachment,
    limits: &AttachmentLimits,
) -> Result<(DynamicImage, ImageFormat, u64), String> {
    attachment.validate(limits)?;
    let format = image::guess_format(&attachment.bytes)
        .map_err(|_| "assistant attachment data is not a supported image".to_string())?;
    let expected_media_type = attachment_media_type(format)?;
    if attachment.media_type != expected_media_type {
        return Err("assistant attachment media type does not match decoded data".into());
    }

    let mut decoder = ImageReader::with_format(Cursor::new(&attachment.bytes), format)
        .into_decoder()
        .map_err(|_| "assistant attachment dimensions could not be decoded".to_string())?;
    let (width, height) = decoder.dimensions();
    let pixels = u64::from(width)
        .checked_mul(u64::from(height))
        .ok_or_else(|| "assistant attachment pixel count overflow".to_string())?;
    if pixels == 0 || pixels > limits.max_pixels {
        return Err("assistant attachment pixel limit exceeded".into());
    }
    if width != attachment.pixel_width || height != attachment.pixel_height {
        return Err("assistant attachment dimensions do not match decoded data".into());
    }

    let orientation = decoder
        .orientation()
        .map_err(|_| "assistant attachment orientation could not be decoded".to_string())?;
    let mut decoded = DynamicImage::from_decoder(decoder)
        .map_err(|_| "assistant attachment data could not be decoded".to_string())?;
    decoded.apply_orientation(orientation);
    Ok((decoded, format, pixels))
}

fn attachment_media_type(format: ImageFormat) -> Result<&'static str, String> {
    match format {
        ImageFormat::Png => Ok("image/png"),
        ImageFormat::Jpeg => Ok("image/jpeg"),
        _ => Err("assistant attachment image format is not allowed".into()),
    }
}

/// Decodifica y vuelve a codificar únicamente los píxeles de una imagen.
///
/// La salida conserva PNG o JPEG, pero no copia EXIF, GPS, perfiles ni chunks
/// textuales del archivo de origen. Los límites se vuelven a comprobar sobre
/// los bytes resultantes antes de que el transporte pueda serializarlos.
pub fn sanitize_attachment(
    attachment: &ImageAttachment,
    limits: &AttachmentLimits,
) -> Result<ImageAttachment, String> {
    let (decoded, format, _) = decode_attachment(attachment, limits)?;
    let mut output = Cursor::new(Vec::new());
    decoded
        .write_to(&mut output, format)
        .map_err(|_| "assistant attachment could not be safely re-encoded".to_string())?;
    let mut sanitized = ImageAttachment::new(
        attachment_media_type(format)?,
        output.into_inner(),
        decoded.width(),
        decoded.height(),
    );
    sanitized.transcription = attachment.transcription.clone();
    sanitized.validate(limits)?;
    Ok(sanitized)
}

fn sanitized_attachments(
    request: &AssistantRequest,
    limits: &AttachmentLimits,
) -> Result<Vec<ImageAttachment>, String> {
    if request.repair_feedback.is_some()
        && (!request.attachments.is_empty() || request.image_upload_consent)
    {
        return Err(
            "assistant repair requests cannot include images or image-upload consent".into(),
        );
    }
    let mut sanitized = Vec::with_capacity(request.attachments.len());
    let mut total_bytes = 0_usize;
    let mut total_pixels = 0_u64;
    for attachment in &request.attachments {
        let attachment = sanitize_attachment(attachment, limits)?;
        total_bytes = total_bytes
            .checked_add(attachment.bytes.len())
            .ok_or_else(|| "assistant attachment payload budget overflow".to_string())?;
        total_pixels = total_pixels
            .checked_add(
                u64::from(attachment.pixel_width)
                    .checked_mul(u64::from(attachment.pixel_height))
                    .ok_or_else(|| "assistant attachment pixel count overflow".to_string())?,
            )
            .ok_or_else(|| "assistant attachment pixel budget overflow".to_string())?;
        sanitized.push(attachment);
    }
    if total_bytes > limits.max_total_bytes {
        return Err("assistant attachment payload budget exceeded after re-encoding".into());
    }
    if total_pixels > limits.max_total_pixels {
        return Err("assistant attachment decoded pixel budget exceeded".into());
    }
    Ok(sanitized)
}

/// Construye el cuerpo OpenAI Chat Completions sin incluir ninguna clave de API.
pub fn build_chat_completion_payload(
    settings: &ProviderSettings,
    request: &AssistantRequest,
) -> Result<Value, String> {
    if remote_protocol(settings) != RemoteProtocol::OpenAiChatCompletions {
        return Err("selected OpenCode model requires the Responses API (`POST responses`), not Chat Completions".into());
    }
    settings.validate()?;
    let attachment_limits = AttachmentLimits::default();
    request.validate(&attachment_limits)?;
    if request.privacy_mode != PrivacyMode::RemoteAllowed {
        return Err("remote assistant use requires explicit privacy consent".into());
    }
    if !request.attachments.is_empty() && !settings.capabilities.vision {
        return Err("selected provider does not declare vision capability".into());
    }
    if !request.attachments.is_empty() && !request.image_upload_consent {
        return Err("remote image upload requires separate explicit consent".into());
    }
    let sanitized_attachments = sanitized_attachments(request, &attachment_limits)?;
    let prompt = remote_prompt(request)?;
    if prompt.is_empty() {
        return Err("remote assistant request has no reviewed problem text".into());
    }

    let mut content = vec![json!({"type": "text", "text": prompt})];
    for attachment in &sanitized_attachments {
        let encoded = base64::engine::general_purpose::STANDARD.encode(&attachment.bytes);
        content.push(json!({
            "type": "image_url",
            "image_url": {"url": format!("data:{};base64,{encoded}", attachment.media_type)}
        }));
    }

    let mut messages = vec![json!({
        "role": "system",
        "content": remote_system_prompt(request)
    })];
    messages.extend(request.conversation.iter().map(|turn| {
        let role = match turn.role {
            ConversationRole::User => "user",
            ConversationRole::Assistant => "assistant",
        };
        json!({"role": role, "content": turn.content})
    }));
    messages.push(json!({"role": "user", "content": content}));

    Ok(json!({
        "model": settings.model,
        "stream": false,
        "max_tokens": completion_token_limit(&request.budget),
        "messages": messages,
    }))
}

/// Construye el cuerpo OpenAI Responses API para Muse Spark sin incluir claves.
///
/// El system prompt viaja en `instructions` y los turnos en `input`
/// (formato Responses: `{"role", "content"}`). Las imágenes usan el item
/// `input_image` con data-URL, igual que en Chat Completions.
/// Sin `stream`, sin `temperature`: se usan los defaults del servidor
/// (verificado 2026-09-04: responde 200 en ~2s).
pub fn build_responses_payload(
    settings: &ProviderSettings,
    request: &AssistantRequest,
) -> Result<Value, String> {
    if remote_protocol(settings) != RemoteProtocol::OpenAiResponses {
        return Err("selected OpenCode model does not use the Responses API".into());
    }
    settings.validate()?;
    let attachment_limits = AttachmentLimits::default();
    request.validate(&attachment_limits)?;
    if request.privacy_mode != PrivacyMode::RemoteAllowed {
        return Err("remote assistant use requires explicit privacy consent".into());
    }
    if !request.attachments.is_empty() && !settings.capabilities.vision {
        return Err("selected provider does not declare vision capability".into());
    }
    if !request.attachments.is_empty() && !request.image_upload_consent {
        return Err("remote image upload requires separate explicit consent".into());
    }
    let sanitized_attachments = sanitized_attachments(request, &attachment_limits)?;
    let prompt = remote_prompt(request)?;
    if prompt.is_empty() {
        return Err("remote assistant request has no reviewed problem text".into());
    }

    let mut final_content = vec![json!({"type": "input_text", "text": prompt})];
    for attachment in &sanitized_attachments {
        let encoded = base64::engine::general_purpose::STANDARD.encode(&attachment.bytes);
        final_content.push(json!({
            "type": "input_image",
            "image_url": format!("data:{};base64,{encoded}", attachment.media_type),
        }));
    }

    let mut input: Vec<Value> = request
        .conversation
        .iter()
        .map(|turn| {
            let role = match turn.role {
                ConversationRole::User => "user",
                ConversationRole::Assistant => "assistant",
            };
            json!({"role": role, "content": turn.content})
        })
        .collect();
    input.push(json!({"role": "user", "content": final_content}));

    Ok(json!({
        "model": settings.model,
        "instructions": remote_system_prompt(request),
        "max_output_tokens": responses_token_limit_for_chars(request.budget.max_output_chars),
        "input": input,
    }))
}

/// Construye un payload Anthropic Messages para `mimo-2.5-vl` sin incluir claves.
pub fn build_anthropic_messages_payload(
    settings: &ProviderSettings,
    request: &AssistantRequest,
) -> Result<Value, String> {
    if !matches!(
        remote_protocol(settings),
        RemoteProtocol::AnthropicMessages | RemoteProtocol::Fusion
    ) {
        return Err("selected model does not use Anthropic Messages".into());
    }
    settings.validate()?;
    let attachment_limits = AttachmentLimits::default();
    request.validate(&attachment_limits)?;
    if request.privacy_mode != PrivacyMode::RemoteAllowed {
        return Err("remote assistant use requires explicit privacy consent".into());
    }
    if remote_protocol(settings) == RemoteProtocol::Fusion && !request.attachments.is_empty() {
        return Err("Fusion image input is not enabled".into());
    }
    if !request.attachments.is_empty() && !settings.capabilities.vision {
        return Err("selected provider does not declare vision capability".into());
    }
    if !request.attachments.is_empty() && !request.image_upload_consent {
        return Err("remote image upload requires separate explicit consent".into());
    }
    let sanitized_attachments = sanitized_attachments(request, &attachment_limits)?;
    let prompt = remote_prompt(request)?;
    if prompt.is_empty() {
        return Err("remote assistant request has no reviewed problem text".into());
    }
    let mut messages = Vec::with_capacity(request.conversation.len() + 1);
    if remote_protocol(settings) != RemoteProtocol::Fusion {
        messages.extend(request.conversation.iter().map(|turn| {
            let role = match turn.role {
                ConversationRole::User => "user",
                ConversationRole::Assistant => "assistant",
            };
            json!({"role": role, "content": turn.content})
        }));
    }
    let mut content = vec![json!({"type": "text", "text": prompt})];
    for attachment in &sanitized_attachments {
        content.push(json!({
            "type": "image",
            "source": {
                "type": "base64",
                "media_type": attachment.media_type,
                "data": base64::engine::general_purpose::STANDARD.encode(&attachment.bytes),
            }
        }));
    }
    messages.push(json!({"role": "user", "content": content}));
    Ok(json!({
        "model": OPENCODE_VISION_MODEL,
        "max_tokens": completion_token_limit(&request.budget),
        "system": remote_system_prompt(request),
        "messages": messages,
    }))
}

/// Construye la segunda petición de Fusion con sólo el prompt saneado y el borrador limitado.
pub fn build_fusion_audit_payload(
    settings: &ProviderSettings,
    request: &AssistantRequest,
    draft: &str,
) -> Result<Value, String> {
    if remote_protocol(settings) != RemoteProtocol::Fusion {
        return Err("selected model is not Fusion".into());
    }
    settings.validate()?;
    request.validate(&AttachmentLimits::default())?;
    if request.privacy_mode != PrivacyMode::RemoteAllowed {
        return Err("remote assistant use requires explicit privacy consent".into());
    }
    if !request.attachments.is_empty() {
        return Err("Fusion image input is not enabled".into());
    }
    let prompt = remote_prompt(request)?;
    let draft_limit = fusion_draft_byte_limit(request, &prompt)?;
    if draft.trim().is_empty() || draft.len() > draft_limit {
        return Err("Fusion draft exceeds the audit input budget".into());
    }
    let audit_input =
        format!("{FUSION_AUDIT_USER_PREFIX}\n{prompt}\n\nCandidate response to audit:\n{draft}");
    let mut messages = Vec::with_capacity(2);
    messages.push(json!({
        "role": "system",
        "content": format!(
            "{FUSION_AUDIT_SYSTEM_PROMPT}\n\n{REMOTE_RESPONSE_GUIDANCE}\n\n{REMOTE_TETRAHEDRON_GUIDANCE}\n\n{REMOTE_4D_POLYTOPE_GUIDANCE}\n\n{}",
            response_language_directive(&request.language)
        )
    }));
    messages.push(json!({"role": "user", "content": audit_input}));
    Ok(json!({
        "model": FUSION_AUDIT_MODEL,
        "stream": false,
        "max_tokens": completion_token_limit(&request.budget),
        "messages": messages,
    }))
}

fn fusion_draft_byte_limit(request: &AssistantRequest, prompt: &str) -> Result<usize, String> {
    let fixed_bytes = FUSION_AUDIT_USER_PREFIX
        .len()
        .saturating_add("\n\nCandidate response to audit:\n".len())
        .saturating_add(prompt.len());
    let available = request.budget.max_input_chars.saturating_sub(fixed_bytes);
    if available == 0 {
        return Err("Fusion request leaves no room for an audited draft".into());
    }
    Ok(available
        .min(request.budget.max_output_chars)
        .min(FUSION_MAX_DRAFT_BYTES))
}

/// System prompt base del asistente (interfaz pública, incluye plugins).
pub fn assistant_system_prompt(request: &AssistantRequest) -> String {
    remote_system_prompt(request)
}

/// System prompt con inyección socrática determinista (BKT current_question + scaffold + history).
///
/// Wrapper público y testeable sobre `remote_system_prompt_with_socratic`.
pub fn assistant_system_prompt_with_socratic(
    request: &AssistantRequest,
    fsm: &SocraticFsm,
    scaffold: &Scaffold,
) -> String {
    remote_system_prompt_with_socratic(request, fsm, scaffold)
}

/// Prompt rico de usuario (problema + focus + catálogo + reparación).
pub fn assistant_remote_prompt(request: &AssistantRequest) -> Result<String, String> {
    remote_prompt(request)
}

/// Tools seguras por defecto que el modo agente ofrece al modelo.
///
/// Incluye las 3 base (evaluate_expr, grafito_docs, ask_user) más las 6
/// pedagógicas F3.2 (scaffold, generate_exercise, assess_answer,
/// get_curriculum, suggest_next, generate_animation) para orquestación
/// vía OpenCode Go sin salir del chat. Todas son puras y sin Document.
pub fn default_agent_tools() -> Vec<ToolSchema> {
    // Delegamos al dispatcher canónico para mantener una única fuente de verdad
    // entre schema y dispatch (grafito-assistant/src/agent.rs).
    crate::agent::all_safe_tool_schemas()
}

/// System prompt base más las instrucciones locales de plugins, acotadas.
///
/// Mitigación prompt injection: delimita secciones con `--- SYSTEM ---` y `--- USER ---`
/// para que el modelo distinga instrucciones privilegiadas (SYSTEM) del contenido
/// del usuario/plugins (USER). Las instrucciones de plugins se envuelven en bloque
/// USER delimitado, evitando que puedan suplantar directivas SYSTEM sin delimitador.
///
/// El prompt se acota además por `MAX_SYSTEM_INSTRUCTIONS_BYTES` (validado en
/// `AssistantRequest::validate`) y se trimea antes de inyectar.
///
/// Directiva de idioma de respuesta según el selector Idioma del panel
/// (`avatar.language`: "es"/"en"/"auto"/""). Auto y vacío resuelven a español
/// rioplatense porque la app es español-primero. Los bloques ```grafito
/// conservan su sintaxis original en inglés en ambos idiomas.
fn response_language_directive(language: &str) -> &'static str {
    if language.trim().eq_ignore_ascii_case("en") {
        "Always respond in English. Keep ```grafito commands in their original syntax."
    } else {
        "Respondé siempre en español rioplatense (usá vos, no tú). Los comandos ```grafito se mantienen en su sintaxis original en inglés."
    }
}

fn remote_system_prompt(request: &AssistantRequest) -> String {
    let base = format!(
        "{REMOTE_SYSTEM_PROMPT}\n\n{GRAFITO_CAPABILITY_SCOPE}\n\n{REMOTE_RESPONSE_GUIDANCE}\n\n{REMOTE_TETRAHEDRON_GUIDANCE}\n\n{REMOTE_4D_POLYTOPE_GUIDANCE}\n\n{SOCRATIC_BINDING_DIRECTIVE}\n\n{}",
        response_language_directive(&request.language)
    );
    let instructions = request.system_instructions.trim();
    if instructions.is_empty() {
        format!("--- SYSTEM ---\n{base}\n--- END SYSTEM ---")
    } else {
        format!(
            "--- SYSTEM ---\n{base}\n--- USER ---\nInstrucciones locales (plugins) para esta sesión:\n{instructions}\n--- END ---"
        )
    }
}

/// System prompt con inyección socrática determinista (BKT current_question + scaffold + history).
///
/// Inserta el segmento socrático antes del cierre final, de forma determinista.
/// Si el prompt usa delimitador `--- END`, inyecta antes; si no, hace append.
pub fn remote_system_prompt_with_socratic(
    request: &AssistantRequest,
    fsm: &SocraticFsm,
    scaffold: &Scaffold,
) -> String {
    let mut base = remote_system_prompt(request);
    let socratic_segment = fsm.socratic_system_segment(scaffold);
    if let Some(pos) = base.rfind("--- END") {
        base.insert_str(pos, &format!("{socratic_segment}\n\n"));
    } else {
        base.push_str("\n\n");
        base.push_str(&socratic_segment);
    }
    base
}

/// Atajo que construye scaffold determinista desde `concept`/`level` + `history` y lo inyecta.
///
/// `concept` es el topic BKT actual, `level` el nivel pedagógico, `history` los turnos previos.
/// Determinista: mismo FSM + concept/level/history → mismo prompt.
pub fn remote_system_prompt_with_scaffold(
    request: &AssistantRequest,
    fsm: &SocraticFsm,
    concept: &str,
    level: PedagogicalLevel,
    history: &[grafito_pedagogy::scaffold::Turn],
) -> String {
    let engine = ScaffoldEngine;
    let scaffold = engine.scaffold(concept, level, history);
    remote_system_prompt_with_socratic(request, fsm, &scaffold)
}

/// Heurística determinista: ¿el texto del LLM contiene marcadores de solución directa?
///
/// Wrapper puro sobre `SocraticFsm::contains_solution_marker` para tests desde `grafito-assistant`.
pub fn contains_telling_markers(text: &str) -> bool {
    SocraticFsm::contains_solution_marker(text)
}

/// Guard telling puro: verifica si el LLM reveló solución con `attempts<2`.
///
/// Retorna `Ok(())` si pasa, `Err("TellingTooEarly: ...")` si viola `can_reveal==false`.
pub fn check_telling_guard(fsm: &SocraticFsm, response_text: &str) -> Result<(), String> {
    fsm.check_telling_guard(response_text)
        .map_err(|e| e.to_string())
}

/// Enforce: si es telling devuelve `Err(repair_prompt)` vinculante, si no `Ok(text)`.
///
/// El llamante remoto debe descartar la respuesta y re-preguntar o inyectar `repair_prompt` como
/// `repair_feedback`/nuevo turno (ver `telling_repair_prompt`).
pub fn enforce_telling_guard(
    fsm: &SocraticFsm,
    response_text: &str,
    scaffold: &Scaffold,
) -> Result<String, String> {
    fsm.enforce_telling_guard(response_text, scaffold)
}

/// Aplica el guard a un `RemoteCompletion` completo.
///
/// Si es telling con `attempts<2`, fuerza la reparación (Err con re-pregunta); si no, Ok(completion).
pub fn guard_remote_completion(
    fsm: &SocraticFsm,
    completion: RemoteCompletion,
    scaffold: &Scaffold,
) -> Result<RemoteCompletion, String> {
    match fsm.enforce_telling_guard(&completion.text, scaffold) {
        Ok(_) => Ok(completion),
        Err(repair) => Err(repair),
    }
}

/// Prompt de reparación determinista para telling, listo para inyectar en el próximo request.
pub fn telling_repair_prompt(fsm: &SocraticFsm, scaffold: &Scaffold) -> String {
    fsm.repair_prompt_for_telling(scaffold)
}

fn remote_prompt(request: &AssistantRequest) -> Result<String, String> {
    let problem = request.problem.trim();
    let mut prompt = problem.to_owned();
    // Document context: todos los objetos visibles, para que el LLM sea consciente de TODAS las capacidades y use la función correcta
    if !request.context.objects.is_empty() || !request.context.variables.is_empty() {
        if !prompt.is_empty() {
            prompt.push_str("\n\n");
        }
        prompt.push_str(REMOTE_CONTEXT_PROMPT_PREFIX.trim_start_matches('\n'));
        if !request.context.variables.is_empty() {
            prompt.push_str("Variables: ");
            prompt.push_str(
                &request
                    .context
                    .variables
                    .iter()
                    .map(|(k, v)| format!("{k}={v}"))
                    .collect::<Vec<_>>()
                    .join(", "),
            );
            prompt.push('\n');
        }
        prompt.push_str("Objetos visibles:\n");
        for obj in &request.context.objects {
            // fingerprint es JSON del objeto, recortado a 120 chars para no saturar prompt
            let fp: String = obj.fingerprint.chars().take(120).collect();
            prompt.push_str(&format!("- {} [{}]: {}\n", obj.label, obj.kind, fp));
        }
        prompt.push_str("Si el usuario pide Taylor/Integral/Derivada sin especificar función, usa la última Function visible arriba (no sin(x) por defecto).\n");
    }
    if let Some(focus) = &request.focus {
        if !prompt.is_empty() {
            prompt.push_str("\n\n");
        }
        prompt.push_str(REMOTE_FOCUS_PROMPT_PREFIX.trim_start_matches('\n'));
        prompt.push_str(&focus.summary);
    }
    if !request.tool_catalog.is_empty() {
        prompt.push_str(REMOTE_TOOL_CATALOG_PROMPT_PREFIX);
        prompt.push_str(&request.tool_catalog);
    }
    if let Some(feedback) = &request.repair_feedback {
        prompt.push_str(REMOTE_REPAIR_FEEDBACK_PROMPT_PREFIX);
        prompt.push_str(&feedback.prompt_text());
    }
    if prompt.len() > request.budget.max_input_chars {
        return Err("remote assistant input exceeds the configured input budget".into());
    }
    Ok(prompt)
}

fn completion_token_limit(budget: &grafito_assistant_types::RequestBudget) -> usize {
    completion_token_limit_for_chars(budget.max_output_chars)
}

fn completion_token_limit_for_chars(max_output_chars: usize) -> usize {
    (max_output_chars / 4).clamp(1, 8_192)
}

/// Límite `max_output_tokens` para la Responses API: incluye los tokens de
/// razonamiento (verificado: un "ok" consume 61 reasoning + 0 output), así que
/// se duplica el presupuesto de texto con piso 2048 y techo 16384.
/// El servidor rechaza valores < 16.
fn responses_token_limit_for_chars(max_output_chars: usize) -> usize {
    // Piso 2048 >> mínimo 16 del servidor; el techo evita facturas sorpresa.
    let limit = max_output_chars.saturating_mul(2).clamp(2_048, 16_384);
    debug_assert!(limit >= RESPONSES_MIN_OUTPUT_TOKENS);
    limit
}

/// Señal de cancelación cooperativa para una petición remota en segundo plano.
///
/// No puede interrumpir bytes que ya fueron entregados a `send()`; el worker
/// observa la señal antes y después del transporte y conserva su timeout.
#[derive(Clone, Default)]
pub struct CancellationToken(Arc<AtomicBool>);

impl CancellationToken {
    /// Solicita que el hilo abandone una petición antes o después del transporte.
    pub fn cancel(&self) {
        self.0.store(true, Ordering::Release);
    }

    /// Indica si se solicitó cancelación.
    pub fn is_cancelled(&self) -> bool {
        self.0.load(Ordering::Acquire)
    }
}

/// Resultado remoto mínimo, sin eco de credenciales ni de respuestas de error del proveedor.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RemoteCompletion {
    /// Contenido textual de la primera elección del proveedor.
    pub text: String,
    /// El proveedor agotó el límite antes de cerrar la respuesta.
    pub truncated: bool,
}

/// Inicia un único POST OpenAI-compatible en un hilo de trabajo.
///
/// Las claves se leen exclusivamente desde la variable de entorno configurada
/// dentro del hilo y nunca se almacenan ni se incluyen en los errores.
pub fn request_remote_on_worker(
    settings: ProviderSettings,
    request: AssistantRequest,
    cancellation: CancellationToken,
) -> JoinHandle<Result<RemoteCompletion, String>> {
    std::thread::spawn(move || {
        settings.validate()?;
        let api_key = settings
            .api_key_env
            .as_deref()
            .map(|variable| {
                std::env::var(variable).map_err(|_| {
                    "remote assistant API key is not available in the environment".to_string()
                })
            })
            .transpose()?;
        request_remote(settings, request, api_key, cancellation)
    })
}

/// Inicia una petición remota usando una clave de memoria de una sesión de UI.
///
/// La clave se consume sólo dentro del worker y nunca se serializa ni se informa
/// en errores. Una clave `None` es válida para proveedores sin autenticación,
/// como una instancia local de Ollama.
pub fn request_remote_with_api_key_on_worker(
    settings: ProviderSettings,
    request: AssistantRequest,
    api_key: Option<String>,
    cancellation: CancellationToken,
) -> JoinHandle<Result<RemoteCompletion, String>> {
    std::thread::spawn(move || request_remote(settings, request, api_key, cancellation))
}

/// Consulta los identificadores de modelos disponibles en un proveedor remoto.
///
/// Sólo devuelve `data[].id`, con tamaño y cantidad acotados; no propaga la
/// metadata ni la respuesta completa del proveedor a la interfaz.
pub fn request_remote_models_with_api_key_on_worker(
    settings: ProviderSettings,
    api_key: Option<String>,
    cancellation: CancellationToken,
) -> JoinHandle<Result<Vec<String>, String>> {
    std::thread::spawn(move || {
        if cancellation.is_cancelled() {
            return Err("remote model request was cancelled".into());
        }
        let endpoint = models_endpoint(&settings)?;
        let client = shared_http_client()?;
        let mut call = client.get(endpoint).timeout(Duration::from_secs(15));
        if let Some(key) = api_key {
            call = call.bearer_auth(sanitize_api_key(&key)?);
        }
        let response = call.send().map_err(|error| {
            transport_error(
                "remote model request",
                &error,
                Some(Duration::from_secs(15)),
            )
        })?;
        if cancellation.is_cancelled() {
            return Err("remote model request was cancelled".into());
        }
        if !response.status().is_success() {
            let status = response.status().as_u16();
            // 429 informa "reintentá en Ns" sin dormir el worker (cancelable).
            let retry_after = if status == 429 {
                retry_after_secs_from_headers(response.headers())
            } else {
                None
            };
            if let Some(secs) = retry_after {
                return Err(format!(
                    "remote model request returned HTTP 429 (reintentá en {secs}s)"
                ));
            }
            return Err(format!("remote model request returned HTTP {status}"));
        }
        let response_bytes = read_bounded_response_body(response, MAX_MODELS_RESPONSE_BYTES)?;
        let body: Value = serde_json::from_slice(&response_bytes)
            .map_err(|_| "remote model request returned an invalid response".to_string())?;
        let mut models = body
            .pointer("/data")
            .and_then(Value::as_array)
            .ok_or_else(|| "remote model request returned no model list".to_string())?
            .iter()
            .filter_map(|model| model.get("id").and_then(Value::as_str))
            .map(str::trim)
            .filter(|model| {
                !model.is_empty() && model.chars().count() <= MAX_MODEL_IDENTIFIER_CHARS
            })
            .map(str::to_owned)
            .collect::<Vec<_>>();
        models.sort();
        models.dedup();
        if models.is_empty() || models.len() > MAX_DISCOVERED_MODELS {
            return Err("remote model list is outside the allowed size".into());
        }
        Ok(models)
    })
}

fn request_remote(
    settings: ProviderSettings,
    request: AssistantRequest,
    api_key: Option<String>,
    cancellation: CancellationToken,
) -> Result<RemoteCompletion, String> {
    let started = Instant::now();
    let protocol = remote_protocol(&settings);
    let result = (|| {
        if request.privacy_mode != PrivacyMode::RemoteAllowed {
            return Err("remote assistant use requires explicit privacy consent".into());
        }
        if cancellation.is_cancelled() {
            return Err("remote assistant request was cancelled".into());
        }
        // Clamp 100ms..=120s: nunca 0 ni >120s (simple 60s default, agente
        // 30s/turno en grafito-agent/loop_engine.rs). Ver `effective_remote_timeout`.
        let timeout = effective_remote_timeout(request.budget.timeout_ms);
        match protocol {
            RemoteProtocol::OpenAiChatCompletions => request_openai_completion(
                chat_completion_endpoint(&settings)?,
                build_chat_completion_payload(&settings, &request)?,
                api_key.as_deref(),
                &cancellation,
                timeout,
                request.budget.max_output_chars,
            ),
            RemoteProtocol::OpenAiResponses => request_responses_completion(
                responses_endpoint(&settings)?,
                build_responses_payload(&settings, &request)?,
                api_key.as_deref(),
                &cancellation,
                timeout,
                request.budget.max_output_chars,
            ),
            RemoteProtocol::AnthropicMessages => request_anthropic_completion(
                messages_endpoint(&settings)?,
                build_anthropic_messages_payload(&settings, &request)?,
                api_key.as_deref(),
                &cancellation,
                timeout,
                request.budget.max_output_chars,
            ),
            RemoteProtocol::Fusion => request_fusion_completion(
                &settings,
                &request,
                api_key.as_deref(),
                &cancellation,
                timeout,
            ),
        }
    })();
    log_remote_completion_event(&settings.model, protocol, started.elapsed(), &result);
    result
}

fn log_remote_completion_event(
    model: &str,
    protocol: RemoteProtocol,
    elapsed: Duration,
    result: &Result<RemoteCompletion, String>,
) {
    let (status, category) = match result {
        Ok(_) => ("success", "completed"),
        Err(error) => ("error", remote_error_category(error)),
    };
    eprintln!(
        "assistant_remote_event model={} protocol={} status={} elapsed_ms={} category={}",
        log_model_identifier(model),
        protocol.name(),
        status,
        elapsed.as_millis(),
        category,
    );
}

fn log_model_identifier(model: &str) -> String {
    model
        .chars()
        .take(MAX_MODEL_IDENTIFIER_CHARS)
        .map(|character| {
            if character.is_ascii_alphanumeric() || matches!(character, '-' | '_' | '.') {
                character
            } else {
                '_'
            }
        })
        .collect()
}

fn remote_error_category(error: &str) -> &'static str {
    if error == "remote assistant request was cancelled" {
        "cancelled"
    } else if error.starts_with("remote assistant response JSON is invalid") {
        "json"
    } else if error.starts_with("remote assistant response schema is invalid") {
        "schema"
    } else if error.starts_with("remote assistant response content is not displayable") {
        "content"
    } else if error.starts_with("remote assistant returned HTTP") {
        "http"
    } else if error.contains("response body") || error.contains("response exceeds") {
        "body"
    } else if error.contains("completion exceeds") {
        "budget"
    } else if error.contains("API key") {
        "credential"
    } else if error.contains("429") || error.contains("rate limit") || error.contains("RateLimit") {
        "ratelimit"
    } else if error.contains("privacy consent") {
        "privacy"
    } else if error.contains("failed or timed out") {
        "transport"
    } else if error.contains("timed out") {
        "timeout"
    } else if error.contains("could not connect") {
        "connect"
    } else if error.contains("request failed") {
        "transport"
    } else {
        "configuration"
    }
}

fn request_fusion_completion(
    settings: &ProviderSettings,
    request: &AssistantRequest,
    api_key: Option<&str>,
    cancellation: &CancellationToken,
    timeout: Duration,
) -> Result<RemoteCompletion, String> {
    request_fusion_completion_with_endpoints(
        messages_endpoint(settings)?,
        chat_completion_endpoint(settings)?,
        settings,
        request,
        api_key,
        cancellation,
        timeout,
    )
}

fn request_fusion_completion_with_endpoints(
    draft_endpoint: Url,
    audit_endpoint: Url,
    settings: &ProviderSettings,
    request: &AssistantRequest,
    api_key: Option<&str>,
    cancellation: &CancellationToken,
    timeout: Duration,
) -> Result<RemoteCompletion, String> {
    let prompt = remote_prompt(request)?;
    let draft_limit = fusion_draft_byte_limit(request, &prompt)?;
    let mut draft_payload = build_anthropic_messages_payload(settings, request)?;
    draft_payload["max_tokens"] = json!(completion_token_limit_for_chars(draft_limit));
    let started = Instant::now();
    let draft_timeout = Duration::from_millis((timeout.as_millis() as u64 / 2).max(100));
    let draft = request_anthropic_completion(
        draft_endpoint,
        draft_payload,
        api_key,
        cancellation,
        draft_timeout,
        draft_limit,
    )
    .map_err(|error| {
        if cancellation.is_cancelled() {
            "remote assistant request was cancelled".into()
        } else {
            format!("Fusion could not create a draft: {error}")
        }
    })?;
    if cancellation.is_cancelled() {
        return Err("remote assistant request was cancelled".into());
    }
    let audit_timeout = timeout
        .checked_sub(started.elapsed())
        .filter(|remaining| !remaining.is_zero())
        .ok_or_else(|| "Fusion audit timed out before it could start".to_string())?;
    request_openai_completion(
        audit_endpoint,
        build_fusion_audit_payload(settings, request, &draft.text)?,
        api_key,
        cancellation,
        audit_timeout,
        request.budget.max_output_chars,
    )
    .map_err(|_| {
        if cancellation.is_cancelled() {
            "remote assistant request was cancelled".into()
        } else {
            "Fusion could not complete the audit; its draft was discarded.".into()
        }
    })
}

fn request_openai_completion(
    endpoint: Url,
    payload: Value,
    api_key: Option<&str>,
    cancellation: &CancellationToken,
    timeout: Duration,
    max_output_chars: usize,
) -> Result<RemoteCompletion, String> {
    let client = shared_http_client()?;
    let mut call = client.post(endpoint).json(&payload).timeout(timeout);
    if let Some(key) = api_key {
        call = call.bearer_auth(sanitize_api_key(key)?);
    }
    send_openai_request(call, cancellation, max_output_chars, Some(timeout))
}

/// POST a la Responses API (`{base}/responses`) con Bearer saneado.
/// La usa Muse Spark, que por Chat Completions devuelve 500 instantáneo.
fn request_responses_completion(
    endpoint: Url,
    payload: Value,
    api_key: Option<&str>,
    cancellation: &CancellationToken,
    timeout: Duration,
    max_output_chars: usize,
) -> Result<RemoteCompletion, String> {
    let client = shared_http_client()?;
    let mut call = client.post(endpoint).json(&payload).timeout(timeout);
    if let Some(key) = api_key {
        call = call.bearer_auth(sanitize_api_key(key)?);
    }
    if cancellation.is_cancelled() {
        return Err("remote assistant request was cancelled".into());
    }
    let response = call
        .send()
        .map_err(|error| transport_error("remote assistant", &error, Some(timeout)))?;
    if cancellation.is_cancelled() {
        return Err("remote assistant request was cancelled".into());
    }
    if !response.status().is_success() {
        let status = response.status().as_u16();
        // Retry-After sólo en 429; no se duerme el worker cancelable, sólo se
        // anota "reintentá en Ns" (clamp 1..120s, entero o fecha HTTP).
        let retry_after = if status == 429 {
            retry_after_secs_from_headers(response.headers())
        } else {
            None
        };
        let body = response.text().unwrap_or_else(|_| "<no body>".to_string());
        return Err(http_status_error(status, &body, retry_after));
    }
    let response_bytes = read_bounded_response_body(response, RESPONSES_MAX_BODY_BYTES)?;
    let body: Value = serde_json::from_slice(&response_bytes)
        .map_err(|_| "remote assistant response JSON is invalid".to_string())?;
    let (text, truncated) = responses_completion_text(&body)?;
    completion_from_text(&text, max_output_chars, truncated)
}

/// Extrae el texto de una respuesta OpenAI Responses API.
///
/// Junta los `output_text` de todos los items `message`; los items
/// `reasoning`/`function_call` se ignoran (el razonamiento no se muestra).
/// `status: "incomplete"` marca truncado pero igual devuelve el texto.
/// `status: "failed"` o `error` no-nulo devuelven error con el mensaje del
/// proveedor acotado (sin secretos: el servidor no ecoa la clave).
fn responses_completion_text(body: &Value) -> Result<(String, bool), String> {
    if let Some(error) = body.get("error") {
        if !error.is_null() {
            let detail = error
                .get("message")
                .and_then(Value::as_str)
                .unwrap_or("unknown provider error");
            let detail: String = detail.chars().take(200).collect();
            return Err(response_schema_error(&format!(
                "responses API returned an error: {detail}"
            )));
        }
    }
    let status = body.get("status").and_then(Value::as_str).unwrap_or("");
    if status == "failed" || status == "cancelled" {
        return Err(response_schema_error(
            "responses API did not complete the response",
        ));
    }
    let truncated = status == "incomplete";
    let output = body
        .get("output")
        .and_then(Value::as_array)
        .ok_or_else(|| response_schema_error("response must include an output array"))?;
    let mut texts = Vec::new();
    for item in output {
        if item.get("type").and_then(Value::as_str) != Some("message") {
            continue;
        }
        let empty = Vec::new();
        let content = item
            .get("content")
            .and_then(Value::as_array)
            .unwrap_or(&empty);
        for part in content {
            if part.get("type").and_then(Value::as_str) != Some("output_text") {
                continue;
            }
            if let Some(text) = part.get("text").and_then(Value::as_str) {
                texts.push(text.to_owned());
            }
        }
    }
    let text = texts.join("\n\n");
    if text.trim().is_empty() && !truncated {
        return Err(response_schema_error(
            "response contained no displayable text",
        ));
    }
    Ok((text, truncated))
}

fn request_anthropic_completion(
    endpoint: Url,
    payload: Value,
    api_key: Option<&str>,
    cancellation: &CancellationToken,
    timeout: Duration,
    max_output_chars: usize,
) -> Result<RemoteCompletion, String> {
    let key = api_key
        .map(sanitize_api_key)
        .transpose()?
        .filter(|key| !key.is_empty())
        .ok_or_else(|| "remote assistant API key is unavailable".to_string())?;
    let client = shared_http_client()?;
    let call = client
        .post(endpoint)
        .header("x-api-key", key)
        .header("anthropic-version", "2023-06-01")
        .json(&payload)
        .timeout(timeout);
    if cancellation.is_cancelled() {
        return Err("remote assistant request was cancelled".into());
    }
    let response = call
        .send()
        .map_err(|error| transport_error("remote assistant", &error, Some(timeout)))?;
    if cancellation.is_cancelled() {
        return Err("remote assistant request was cancelled".into());
    }
    if !response.status().is_success() {
        let status = response.status().as_u16();
        let retry_after = if status == 429 {
            retry_after_secs_from_headers(response.headers())
        } else {
            None
        };
        let body = response.text().unwrap_or_else(|_| "<no body>".to_string());
        return Err(http_status_error(status, &body, retry_after));
    }
    let response_bytes =
        read_bounded_response_body(response, response_body_limit(max_output_chars))?;
    let body: Value = serde_json::from_slice(&response_bytes)
        .map_err(|_| "remote assistant response JSON is invalid".to_string())?;
    let (text, truncated) = anthropic_completion_text(&body)?;
    completion_from_text(&text, max_output_chars, truncated)
}

/// Clasifica un error de transporte sin exponer detalles sensibles (sin URL ni
/// clave): timeout lleva "timed out", fallo de conexión/DNS/TLS lleva
/// "could not connect", el resto lleva "request failed". La UI mapea cada
/// caso a un mensaje distinto en criollo.
pub(crate) fn transport_error(
    context: &str,
    error: &reqwest::Error,
    timeout: Option<Duration>,
) -> String {
    if error.is_timeout() {
        match timeout {
            Some(timeout) => format!("{context} timed out after {}s", timeout.as_secs().max(1)),
            None => format!("{context} timed out"),
        }
    } else if error.is_connect() {
        format!("{context} could not connect to the provider (red o DNS)")
    } else if error.is_body() || error.is_decode() {
        format!("{context} request failed while sending or reading the body")
    } else if error.is_builder() {
        format!("{context} request could not be built (la clave o el endpoint tienen caracteres inválidos)")
    } else {
        format!("{context} request failed (error de transporte)")
    }
}

/// Sanea una clave para el header Authorization: recorta espacios y saltos
/// de línea pegados al copiar y rechaza caracteres de control o no-ASCII
/// que reqwest no puede poner en un header (antes fallaba como builder
/// error "request could not be built" sin decir la causa).
pub(crate) fn sanitize_api_key(key: &str) -> Result<String, String> {
    let trimmed = key.trim();
    if trimmed.is_empty() {
        return Err("remote assistant API key is unavailable".into());
    }
    if !trimmed.bytes().all(|byte| byte.is_ascii_graphic()) {
        return Err("remote assistant API key contains invalid characters (revisá espacios o saltos de línea al copiar la clave)".into());
    }
    Ok(trimmed.to_owned())
}

fn send_openai_request(
    call: reqwest::blocking::RequestBuilder,
    cancellation: &CancellationToken,
    max_output_chars: usize,
    timeout: Option<Duration>,
) -> Result<RemoteCompletion, String> {
    if cancellation.is_cancelled() {
        return Err("remote assistant request was cancelled".into());
    }
    let response = call
        .send()
        .map_err(|error| transport_error("remote assistant", &error, timeout))?;
    if cancellation.is_cancelled() {
        return Err("remote assistant request was cancelled".into());
    }
    if !response.status().is_success() {
        let status = response.status().as_u16();
        let retry_after = if status == 429 {
            retry_after_secs_from_headers(response.headers())
        } else {
            None
        };
        let body = response.text().unwrap_or_else(|_| "<no body>".to_string());
        return Err(http_status_error(status, &body, retry_after));
    }
    let response_bytes =
        read_bounded_response_body(response, response_body_limit(max_output_chars))?;
    let body: Value = serde_json::from_slice(&response_bytes)
        .map_err(|_| "remote assistant response JSON is invalid".to_string())?;
    let (text, truncated) = chat_completion_text(&body)?;
    completion_from_text(&text, max_output_chars, truncated)
}

fn chat_completion_text(body: &Value) -> Result<(String, bool), String> {
    let choices = body
        .get("choices")
        .and_then(Value::as_array)
        .ok_or_else(|| response_schema_error("choices must be an array"))?;
    let choice = choices
        .first()
        .and_then(Value::as_object)
        .ok_or_else(|| response_schema_error("choices must contain a first choice object"))?;
    if choice.get("finish_reason").and_then(Value::as_str) != Some("stop") {
        return Err(response_schema_error(
            "first choice is not a completed text response",
        ));
    }
    let message = choice
        .get("message")
        .and_then(Value::as_object)
        .ok_or_else(|| response_schema_error("first choice must include a message object"))?;
    if message.get("role").and_then(Value::as_str) != Some("assistant") {
        return Err(response_schema_error(
            "first choice message must have the assistant role",
        ));
    }
    if message.contains_key("tool_calls") || message.contains_key("function_call") {
        return Err(response_content_error(
            "tool or function calls are not displayable final content",
        ));
    }
    let content = message
        .get("content")
        .ok_or_else(|| response_content_error("a text content field is required"))?;
    let text = match content {
        Value::String(text) => Ok(text.clone()),
        Value::Array(blocks) => chat_completion_text_blocks(blocks),
        _ => Err(response_content_error(
            "content must be a text string or an array of text blocks",
        )),
    }?;
    Ok((text, false))
}

fn anthropic_completion_text(body: &Value) -> Result<(String, bool), String> {
    let truncated = match body.get("stop_reason").and_then(Value::as_str) {
        Some("end_turn") => false,
        Some("max_tokens") => true,
        _ => return Err(response_schema_error("response is not a text response")),
    };
    let blocks = body
        .get("content")
        .and_then(Value::as_array)
        .ok_or_else(|| response_content_error("a text content array is required"))?;
    Ok((chat_completion_text_blocks(blocks)?, truncated))
}

fn chat_completion_text_blocks(blocks: &[Value]) -> Result<String, String> {
    if blocks.is_empty() {
        return Err(response_content_error(
            "content text blocks must not be empty",
        ));
    }
    let mut text = String::new();
    for block in blocks {
        let block = block
            .as_object()
            .ok_or_else(|| response_content_error("each content block must be an object"))?;
        if block.get("type").and_then(Value::as_str) != Some("text") {
            return Err(response_content_error(
                "content blocks must contain only text",
            ));
        }
        let block_text = block
            .get("text")
            .and_then(Value::as_str)
            .ok_or_else(|| response_content_error("text blocks must include a text string"))?;
        text.push_str(block_text);
    }
    Ok(text)
}

fn response_schema_error(expectation: &str) -> String {
    format!("remote assistant response schema is invalid: {expectation}")
}

fn response_content_error(reason: &str) -> String {
    format!("remote assistant response content is not displayable: {reason}")
}

/// Cliente bloqueante compartido por todos los proveedores.
///
/// Se construye una única vez y su pool de conexiones se reutiliza entre
/// peticiones, evitando el costo de TLS y de crear un `Client` por llamada.
/// El timeout de cada petición se aplica sobre la `RequestBuilder`.
fn shared_http_client() -> Result<&'static reqwest::blocking::Client, String> {
    static CLIENT: std::sync::OnceLock<Result<reqwest::blocking::Client, String>> =
        std::sync::OnceLock::new();
    CLIENT
        .get_or_init(|| {
            reqwest::blocking::Client::builder()
                .redirect(reqwest::redirect::Policy::none())
                .build()
                .map_err(|_| "remote assistant HTTP client could not be created".to_string())
        })
        .as_ref()
        .map_err(|error| error.clone())
}

fn completion_from_text(
    text: &str,
    max_output_chars: usize,
    truncated: bool,
) -> Result<RemoteCompletion, String> {
    if text.trim().is_empty()
        || text
            .chars()
            .any(|character| character.is_control() && !matches!(character, '\n' | '\r' | '\t'))
    {
        return Err(response_content_error(
            "expected a non-empty text message without control characters",
        ));
    }
    if text.chars().count() > max_output_chars {
        return Err("remote assistant completion exceeds the configured output budget".into());
    }
    Ok(RemoteCompletion {
        text: text.into(),
        truncated,
    })
}

fn response_body_limit(max_output_chars: usize) -> usize {
    max_output_chars
        .saturating_mul(4)
        .saturating_add(MAX_RESPONSE_ENVELOPE_BYTES)
}

fn read_bounded_response_body(
    response: reqwest::blocking::Response,
    max_bytes: usize,
) -> Result<Vec<u8>, String> {
    let maximum = max_bytes
        .checked_add(1)
        .ok_or_else(|| "remote assistant response limit is invalid".to_string())?;
    let mut reader = response.take(maximum as u64);
    let mut body = Vec::with_capacity(max_bytes.min(64 * 1024));
    reader
        .read_to_end(&mut body)
        .map_err(|_| "remote assistant response body could not be read".to_string())?;
    if body.len() > max_bytes {
        return Err("remote assistant response exceeds the configured body budget".into());
    }
    Ok(body)
}

#[cfg(test)]
mod tests {
    use super::*;
    use grafito_assistant_types::{
        AssistantRepairFailure, AssistantRepairFailureKind, AssistantRepairFeedback,
        AssistantRequest, ConversationTurn, ImmutableDocumentContext,
    };
    use image::{DynamicImage, Rgba, RgbaImage};
    use std::{
        io::{Cursor, Read, Write},
        net::TcpListener,
        thread,
    };

    fn request(problem: &str) -> AssistantRequest {
        AssistantRequest::local(problem, ImmutableDocumentContext::empty(0))
    }

    #[test]
    fn flower_prompt_uses_canonical_y_height_component() {
        assert!(REMOTE_SYSTEM_PROMPT.contains("height in its second Surface3D component"));
        assert!(!REMOTE_SYSTEM_PROMPT.contains("height in its third Surface3D component"));
    }

    #[test]
    fn tetrahedron_prompt_requires_the_native_solid_command() {
        assert!(REMOTE_TETRAHEDRON_GUIDANCE.contains("Tetrahedron[x, y, z, edge]"));
        assert!(!REMOTE_TETRAHEDRON_GUIDANCE.contains("Segment3D"));
    }

    #[test]
    fn remote_system_prompt_includes_bounded_plugin_instructions() {
        let plain = request("consulta");
        let mut request = request("consulta");
        request.privacy_mode = PrivacyMode::RemoteAllowed;
        request.system_instructions = "Usá notación pitagórica en las respuestas.".into();

        let prompt = remote_system_prompt(&request);

        assert!(prompt.contains("notación pitagórica"));
        assert!(prompt.contains("Instrucciones locales (plugins)"));
        assert!(!remote_system_prompt(&plain).contains("Instrucciones locales"));
    }

    #[test]
    fn remote_payload_uses_named_4d_polytope_commands_instead_of_3d_substitutes() {
        let mut request = request("construi un tesseract 4d");
        request.privacy_mode = PrivacyMode::RemoteAllowed;
        let settings = ProviderSettings::for_profile(ProviderProfile::OllamaLocal, "local");

        let payload = build_chat_completion_payload(&settings, &request)
            .expect("a reviewed remote request builds a payload");
        let system = payload["messages"][0]["content"]
            .as_str()
            .expect("OpenAI system prompt is text");

        assert!(system.contains("Tesseract4D[scale"));
        assert!(system.contains("Pentachoron4D"));
        assert!(system.contains("Never substitute 3D Tetrahedron"));
        assert!(system.contains("many Segment3D"));
    }

    #[test]
    fn remote_payload_builders_reject_local_only_requests() {
        let request = request("2 + 2");
        let chat_settings = ProviderSettings::for_profile(ProviderProfile::OllamaLocal, "local");
        let messages_settings =
            ProviderSettings::for_profile(ProviderProfile::OpenCodeGo, OPENCODE_VISION_MODEL);

        assert!(matches!(
            build_chat_completion_payload(&chat_settings, &request),
            Err(error) if error.contains("explicit privacy consent")
        ));
        assert!(matches!(
            build_anthropic_messages_payload(&messages_settings, &request),
            Err(error) if error.contains("explicit privacy consent")
        ));
    }

    #[test]
    fn fusion_audit_payload_excludes_prior_conversation() {
        let mut request =
            AssistantRequest::remote("current question", ImmutableDocumentContext::empty(0));
        request.conversation = vec![
            ConversationTurn::user("previous question"),
            ConversationTurn::assistant("previous answer"),
        ];
        let settings = ProviderSettings::for_profile(ProviderProfile::OpenCodeGo, "fusion");

        let payload = build_fusion_audit_payload(&settings, &request, "draft answer")
            .expect("the bounded Fusion audit payload builds");
        let serialized = payload.to_string();

        assert!(serialized.contains("current question"));
        assert!(serialized.contains("draft answer"));
        assert!(!serialized.contains("previous question"));
        assert!(!serialized.contains("previous answer"));
    }

    #[test]
    fn fusion_draft_payload_excludes_prior_conversation() {
        let mut request =
            AssistantRequest::remote("current question", ImmutableDocumentContext::empty(0));
        request.conversation = vec![
            ConversationTurn::user("previous question"),
            ConversationTurn::assistant("previous answer"),
        ];
        let settings = ProviderSettings::for_profile(ProviderProfile::OpenCodeGo, "fusion");

        let payload = build_anthropic_messages_payload(&settings, &request)
            .expect("the bounded Fusion draft payload builds");
        let serialized = payload.to_string();

        assert!(serialized.contains("current question"));
        assert!(!serialized.contains("previous question"));
        assert!(!serialized.contains("previous answer"));
    }

    #[test]
    fn remote_prompt_describes_grafitos_broad_mathematical_scope() {
        assert!(GRAFITO_CAPABILITY_SCOPE.contains("not only a y=f(x) plotter"));
        assert!(GRAFITO_CAPABILITY_SCOPE.contains("complex mappings and domain coloring"));
        assert!(GRAFITO_CAPABILITY_SCOPE.contains("CPU-projected 4D objects"));
    }

    #[test]
    fn remote_guidance_keeps_fourier_actions_finite_and_explicit() {
        assert!(REMOTE_RESPONSE_GUIDANCE.contains("Fourier"));
        assert!(REMOTE_RESPONSE_GUIDANCE.contains("Function[(4/pi)"));
        assert!(REMOTE_RESPONSE_GUIDANCE.contains("sum(...)"));
        assert!(REMOTE_RESPONSE_GUIDANCE.contains("a_n"));
    }

    #[test]
    fn remote_prompt_includes_only_typed_local_repair_feedback() {
        let mut request =
            AssistantRequest::remote("construí un tetraedro", ImmutableDocumentContext::empty(0));
        request.repair_feedback = Some(AssistantRepairFeedback {
            failures: vec![AssistantRepairFailure {
                command: "Polyhedron".into(),
                kind: AssistantRepairFailureKind::UnsupportedCommand,
                expected_syntax: Vec::new(),
            }],
        });

        let prompt = remote_prompt(&request).expect("bounded repair feedback fits the prompt");

        assert!(prompt.contains(REMOTE_REPAIR_FEEDBACK_PROMPT_PREFIX.trim()));
        assert!(prompt.contains("Polyhedron"));
        assert!(prompt.contains("not executable"));
    }

    fn png_bytes(width: u32, height: u32) -> Vec<u8> {
        let image = RgbaImage::from_pixel(width, height, Rgba([1, 2, 3, 255]));
        let mut bytes = Cursor::new(Vec::new());
        DynamicImage::ImageRgba8(image)
            .write_to(&mut bytes, ImageFormat::Png)
            .unwrap();
        bytes.into_inner()
    }

    #[test]
    fn solves_arithmetic_with_verified_derivation() {
        let response = solve_local(&request("2 + 3 * 4"));

        assert!(response.answer.contains("14"));
        assert!(response
            .derivation
            .iter()
            .all(|step| !step.verification.is_empty()));
    }

    #[test]
    fn solves_linear_and_quadratic_equations() {
        let linear = solve_local(&request("2*x + 3 = 11"));
        assert!(linear.answer.contains("x = 4"));

        let quadratic = solve_local(&request("x^2 - 5*x + 6 = 0"));
        assert!(quadratic.answer.contains("2"));
        assert!(quadratic.answer.contains("3"));
    }

    #[test]
    fn preserves_a_small_nonzero_linear_solution_in_the_answer_and_verification() {
        let response = solve_local(&request("x = 0.00000000001"));

        assert_eq!(response.answer, "x = 0.00000000001");
        assert!(response.derivation.iter().any(|step| {
            step.verification
                .contains("At x = 0.00000000001, both sides evaluate")
        }));
    }

    #[test]
    fn retains_distinct_near_zero_quadratic_roots_in_answer_and_verification() {
        let response = solve_local(&request("x^2 - 0.00000000002*x = 0"));

        assert_eq!(response.answer, "x = 0 or x = 0.00000000002");
        let verification = &response.derivation.last().unwrap().verification;
        assert!(verification.contains("At x = 0, both sides evaluate"));
        assert!(verification.contains("At x = 0.00000000002, both sides evaluate"));
    }

    #[test]
    fn does_not_solve_tiny_positive_quadratics_as_zero() {
        for problem in ["x^2 + 1e-11 = 0", "x^2 + 0.00000000001 = 0"] {
            let response = solve_local(&request(problem));

            assert!(!response.answer.starts_with("x ="));
        }
    }

    #[test]
    fn solves_subnormal_quadratic_without_claiming_zero_as_a_root() {
        let response = solve_local(&request("1e-308*x^2 - 1e-308 = 0"));

        assert_eq!(response.status, LocalAssistantStatus::Solved);
        assert_eq!(response.answer, "x = -1 or x = 1");
        assert_ne!(response.answer, "x = 0");
    }

    #[test]
    fn rejects_subnormal_residuals_that_are_not_zero() {
        let left = Expr::Const(1e-308);
        let right = Expr::Const(0.0);

        assert!(equation_verification(&left, &right, 0.0, 1e-308).is_none());
    }

    #[test]
    fn preserves_normal_quadratic_roots() {
        let response = solve_local(&request("x^2 - 5*x + 6 = 0"));

        assert_eq!(response.status, LocalAssistantStatus::Solved);
        assert_eq!(response.answer, "x = 2 or x = 3");
    }

    #[test]
    fn recognizes_repeated_decimal_roots_without_artificial_duplicates() {
        for (problem, answer) in [
            ("x^2 - 0.6*x + 0.09 = 0", "x = 0.3"),
            ("x^2 - 0.2*x + 0.01 = 0", "x = 0.1"),
        ] {
            let response = solve_local(&request(problem));

            assert_eq!(response.status, LocalAssistantStatus::Solved);
            assert_eq!(response.answer, answer);
        }
    }

    #[test]
    fn preserves_definite_quadratic_discriminant_classifications() {
        let distinct = solve_local(&request("x^2 - 5*x + 6 = 0"));
        assert_eq!(distinct.status, LocalAssistantStatus::Solved);
        assert_eq!(distinct.answer, "x = 2 or x = 3");

        let no_real = solve_local(&request("x^2 + x + 1 = 0"));
        assert_eq!(no_real.status, LocalAssistantStatus::Solved);
        assert_eq!(no_real.answer, "No real solutions.");
    }

    #[test]
    fn rejects_a_polynomial_when_structural_coefficient_underflow_loses_its_degree() {
        let response = solve_local(&request("x - ((1e-200*x)*(1e-200*x))*x = 0"));

        assert_eq!(response.status, LocalAssistantStatus::Unsupported);
        assert!(response.answer.contains("underflow"));
        assert_ne!(response.answer, "x = 0");
    }

    #[test]
    fn reports_no_real_roots_for_a_tiny_positive_constant() {
        let response = solve_local(&request("x^2 + 1e-308 = 0"));

        assert_eq!(response.status, LocalAssistantStatus::Solved);
        assert_eq!(response.answer, "No real solutions.");
    }

    #[test]
    fn rejects_scientific_literals_outside_f64_input_precision() {
        for problem in [
            "x^2 + 1e-324 = 0",
            "x^2 - 1e-324 = 0",
            "x + 1e309 = 0",
            "x - 1e309 = 0",
        ] {
            let response = solve_local(&request(problem));

            assert_eq!(
                response.status,
                LocalAssistantStatus::Unsupported,
                "{problem}"
            );
            assert!(response.answer.contains("precision"), "{problem}");
            assert_ne!(response.answer, "x = 0", "{problem}");
        }
    }

    #[test]
    fn preserves_ordinary_scientific_literal_equations() {
        let response = solve_local(&request("x = 2.5e1"));

        assert_eq!(response.status, LocalAssistantStatus::Solved);
        assert_eq!(response.answer, "x = 25");
    }

    #[test]
    fn local_results_do_not_exceed_the_derivation_step_budget() {
        let mut limited = request("2*x + 3 = 11");
        limited.budget.max_steps = 1;

        let response = solve_local(&limited);
        assert!(response.derivation.len() <= limited.budget.max_steps);
    }

    #[test]
    fn local_results_do_not_exceed_the_output_character_budget() {
        let mut limited = request("100 + 100");
        limited.budget.max_output_chars = 1;

        let response = solve_local(&limited);
        assert!(response.answer.chars().count() <= limited.budget.max_output_chars);
    }

    #[test]
    fn local_results_reject_a_large_derivation_beneath_a_tiny_answer_budget() {
        let mut limited = request("1 + 1");
        limited.budget.max_output_chars = 1;

        let response = solve_local(&limited);

        assert_eq!(response.status, LocalAssistantStatus::Rejected);
        assert!(response.derivation.is_empty());
        assert!(response.plan.is_none());
        assert!(response.validate(&limited.budget).is_ok());
    }

    #[test]
    fn graph_request_produces_only_a_typed_graph_operation() {
        let response = solve_local(&request("graph y = x^2 - 1"));

        assert!(response.plan.is_some());
        assert!(response
            .plan
            .unwrap()
            .operations
            .iter()
            .all(AssistantOperation::is_graph));
    }

    #[test]
    fn attachments_are_not_claimed_as_local_handwriting_ocr() {
        let mut request = request("solve this");
        request
            .attachments
            .push(grafito_assistant_types::ImageAttachment::new(
                "image/png",
                png_bytes(1, 1),
                1,
                1,
            ));

        let response = solve_local(&request);
        assert!(response.answer.contains("not implemented"));
    }

    #[test]
    fn decoded_attachment_validation_enforces_the_aggregate_pixel_budget() {
        let mut request = request("read these images");
        request.attachments = vec![
            ImageAttachment::new("image/png", png_bytes(2, 2), 2, 2),
            ImageAttachment::new("image/png", png_bytes(2, 2), 2, 2),
        ];
        let limits = AttachmentLimits {
            max_pixels: 4,
            max_attachments: 2,
            max_total_pixels: 7,
            ..AttachmentLimits::default()
        };

        assert_eq!(
            validate_decoded_attachments(&request, &limits).unwrap_err(),
            "assistant attachment decoded pixel budget exceeded"
        );
    }

    #[test]
    fn endpoint_validation_rejects_userinfo_and_public_http() {
        assert!(validate_endpoint("http://example.com/v1").is_err());
        assert!(validate_endpoint("http://localhost:11434/v1").is_err());
        assert!(validate_endpoint("https://api-key@example.com/v1").is_err());
        assert!(validate_endpoint("http://[::1]:11434/v1").is_ok());
    }

    #[test]
    fn shared_http_client_reuses_a_single_client_instance() {
        let first = shared_http_client().expect("shared client builds");
        let second = shared_http_client().expect("shared client returns the cached instance");
        assert!(std::ptr::eq(first, second));
    }

    #[test]
    fn sanitize_api_key_trims_pasted_whitespace() {
        assert_eq!(
            sanitize_api_key("  sk-test-123  \n").expect("trims"),
            "sk-test-123"
        );
        assert_eq!(
            sanitize_api_key("\r\nopencode-key\t").expect("trims"),
            "opencode-key"
        );
    }

    #[test]
    fn sanitize_api_key_rejects_empty_and_control_chars() {
        assert!(sanitize_api_key("   ").is_err());
        assert!(sanitize_api_key("").is_err());
        // Salto interno rompe HeaderValue de reqwest: debe fallar acá con
        // mensaje claro, no como builder error en el transporte.
        assert!(sanitize_api_key("abc\ndef").is_err());
        assert!(sanitize_api_key("cláve-con-ñ").is_err());
    }

    #[test]
    fn response_language_defaults_to_rioplatense_spanish() {
        assert!(response_language_directive("").contains("rioplatense"));
        assert!(response_language_directive("auto").contains("rioplatense"));
        assert!(response_language_directive("es").contains("vos"));
        assert!(response_language_directive("EN").starts_with("Always respond in English"));
        assert!(response_language_directive("en").contains("```grafito"));
    }

    #[test]
    fn system_prompt_carries_the_selected_language() {
        let mut es = request("2 + 2");
        es.language = "es".into();
        assert!(remote_system_prompt(&es).contains("rioplatense"));

        let mut en = request("2 + 2");
        en.language = "en".into();
        let prompt_en = remote_system_prompt(&en);
        assert!(prompt_en.starts_with("--- SYSTEM ---"));
        assert!(prompt_en.contains("Always respond in English"));
        assert!(!prompt_en.contains("rioplatense"));

        // Auto (selector "Auto") resuelve a español.
        let auto = request("2 + 2");
        assert!(remote_system_prompt(&auto).contains("rioplatense"));
    }

    #[test]
    fn chat_and_responses_payloads_include_the_language_directive() {
        let mut req = request("Graficá y = x^2");
        req.privacy_mode = PrivacyMode::RemoteAllowed;
        req.language = "es".into();
        let chat = ProviderSettings::for_profile(ProviderProfile::OpenCodeGo, "deepseek-v4-flash");
        let chat_body = build_chat_completion_payload(&chat, &req)
            .unwrap()
            .to_string();
        assert!(chat_body.contains("rioplatense"));
        let responses_body = build_responses_payload(&spark_settings(), &req)
            .unwrap()
            .to_string();
        assert!(responses_body.contains("rioplatense"));
    }

    fn spark_settings() -> ProviderSettings {
        ProviderSettings::for_profile(ProviderProfile::OpenCodeGo, "muse-spark-1.3-contributor")
    }

    #[test]
    fn spark_models_route_to_responses_api() {
        assert!(uses_responses_api("muse-spark-1.3-contributor"));
        assert!(uses_responses_api("muse-spark-1.2-contributor"));
        assert!(!uses_responses_api("deepseek-v4-flash"));
        let spark = spark_settings();
        assert_eq!(remote_protocol(&spark), RemoteProtocol::OpenAiResponses);
        let deepseek =
            ProviderSettings::for_profile(ProviderProfile::OpenCodeGo, "deepseek-v4-flash");
        assert_eq!(
            remote_protocol(&deepseek),
            RemoteProtocol::OpenAiChatCompletions
        );
        assert!(responses_endpoint(&spark)
            .unwrap()
            .as_str()
            .ends_with("/responses"));
    }

    #[test]
    fn responses_payload_shape_matches_verified_wire_format() {
        let mut req = request("Graficá y = x^2");
        req.privacy_mode = PrivacyMode::RemoteAllowed;
        let payload = build_responses_payload(&spark_settings(), &req).unwrap();
        assert_eq!(payload["model"], "muse-spark-1.3-contributor");
        assert!(payload
            .get("instructions")
            .and_then(Value::as_str)
            .is_some());
        assert!(payload.get("stream").is_none());
        assert!(payload.get("max_tokens").is_none());
        let max_out = payload["max_output_tokens"].as_u64().unwrap();
        assert!((2_048..=16_384).contains(&max_out));
        let input = payload["input"].as_array().unwrap();
        assert!(!input.is_empty());
        assert_eq!(input.last().unwrap()["role"], "user");
        // Chat builder debe rechazar spark con mensaje que nombra Responses.
        let err = build_chat_completion_payload(&spark_settings(), &req).unwrap_err();
        assert!(err.contains("Responses"));
    }

    #[test]
    fn responses_text_extraction_joins_messages_and_flags_truncation() {
        let completed = json!({
            "status": "completed",
            "output": [
                {"type": "reasoning", "status": "completed"},
                {"type": "message", "content": [
                    {"type": "output_text", "text": "Hola"},
                    {"type": "output_text", "text": "mundo"},
                ]},
            ],
        });
        assert_eq!(
            responses_completion_text(&completed).unwrap(),
            ("Hola\n\nmundo".to_string(), false)
        );
        let incomplete = json!({
            "status": "incomplete",
            "output": [
                {"type": "message", "content": [
                    {"type": "output_text", "text": "parcial"},
                ]},
            ],
        });
        assert_eq!(
            responses_completion_text(&incomplete).unwrap(),
            ("parcial".to_string(), true)
        );
        let failed = json!({"status": "failed", "output": []});
        assert!(responses_completion_text(&failed).is_err());
        let with_error = json!({
            "status": "completed",
            "error": {"message": "boom"},
            "output": [],
        });
        assert!(responses_completion_text(&with_error).is_err());
        let empty = json!({"status": "completed", "output": []});
        assert!(responses_completion_text(&empty).is_err());
    }

    #[test]
    fn vision_payload_uses_validated_bytes_without_file_metadata() {
        let mut request = request("read this image");
        request.privacy_mode = PrivacyMode::RemoteAllowed;
        request.image_upload_consent = true;
        request
            .attachments
            .push(grafito_assistant_types::ImageAttachment::new(
                "image/png",
                png_bytes(1, 1),
                1,
                1,
            ));
        let settings = ProviderSettings::for_profile(ProviderProfile::OllamaLocal, "vision")
            .with_capabilities(ProviderCapabilities {
                openai_compatible: true,
                vision: true,
                streaming: false,
            });

        let rendered = build_chat_completion_payload(&settings, &request)
            .unwrap()
            .to_string();
        assert!(rendered.contains("data:image/png;base64,"));
        assert!(!rendered.contains("/home/"));
    }

    #[test]
    fn cancelled_remote_request_exits_before_any_network_attempt() {
        let mut request = request("2 + 2");
        request.privacy_mode = PrivacyMode::RemoteAllowed;
        let cancellation = CancellationToken::default();
        cancellation.cancel();

        let result = request_remote_on_worker(
            ProviderSettings::for_profile(ProviderProfile::OllamaLocal, "local"),
            request,
            cancellation,
        )
        .join()
        .unwrap();

        assert_eq!(
            result.unwrap_err(),
            "remote assistant request was cancelled"
        );
    }

    #[test]
    fn anthropic_messages_retain_displayable_text_when_the_provider_hits_its_limit() {
        let completed = json!({
            "stop_reason": "end_turn",
            "content": [{"type": "text", "text": "final answer"}],
        });
        assert_eq!(
            anthropic_completion_text(&completed).unwrap(),
            ("final answer".into(), false)
        );

        let truncated = json!({
            "stop_reason": "max_tokens",
            "content": [{"type": "text", "text": "provider-private-partial"}],
        });
        assert_eq!(
            anthropic_completion_text(&truncated).unwrap(),
            ("provider-private-partial".into(), true)
        );

        for non_text in [
            json!({"type": "tool_use", "name": "private_tool", "input": {}}),
            json!({"type": "image", "source": {"type": "base64"}}),
        ] {
            let response = json!({
                "stop_reason": "end_turn",
                "content": [non_text],
            });
            assert_eq!(
                anthropic_completion_text(&response).unwrap_err(),
                "remote assistant response content is not displayable: content blocks must contain only text"
            );
        }
    }

    #[test]
    fn minimax_wire_uses_anthropic_headers_and_response_shape() {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let endpoint = Url::parse(&format!(
            "http://{}/messages",
            listener.local_addr().unwrap()
        ))
        .unwrap();
        let server = thread::spawn(move || {
            let (mut stream, _) = listener.accept().unwrap();
            let mut buffer = [0; 4096];
            let bytes = stream.read(&mut buffer).unwrap();
            let request = String::from_utf8_lossy(&buffer[..bytes]).to_ascii_lowercase();
            assert!(request.starts_with("post /messages http/1.1"));
            assert!(request.contains("x-api-key: test-key"));
            assert!(request.contains("anthropic-version: 2023-06-01"));
            let body = r#"{"stop_reason":"end_turn","content":[{"type":"text","text":"draft"}]}"#;
            write!(
                stream,
                "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
                body.len(),
                body
            )
            .unwrap();
        });

        let response = request_anthropic_completion(
            endpoint,
            json!({"model": OPENCODE_VISION_MODEL, "messages": []}),
            Some("test-key"),
            &CancellationToken::default(),
            Duration::from_secs(1),
            64,
        )
        .unwrap();
        server.join().unwrap();

        assert_eq!(response.text, "draft");
    }

    #[test]
    fn responses_wire_uses_bearer_and_response_shape() {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let endpoint = Url::parse(&format!(
            "http://{}/responses",
            listener.local_addr().unwrap()
        ))
        .unwrap();
        let server = thread::spawn(move || {
            let (mut stream, _) = listener.accept().unwrap();
            let mut buffer = [0; 8192];
            let bytes = stream.read(&mut buffer).unwrap();
            let request = String::from_utf8_lossy(&buffer[..bytes]);
            let lowered = request.to_ascii_lowercase();
            assert!(lowered.starts_with("post /responses http/1.1"));
            assert!(lowered.contains("authorization: bearer test-key"));
            assert!(request.contains("\"model\":\"muse-spark-1.3-contributor\""));
            assert!(request.contains("\"instructions\""));
            assert!(!request.contains("max_tokens"));
            assert!(request.contains("max_output_tokens"));
            let body = r#"{"status":"completed","output":[{"type":"reasoning","status":"completed"},{"type":"message","content":[{"type":"output_text","text":"listo"}]}]}"#;
            write!(
                stream,
                "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
                body.len(),
                body
            )
            .unwrap();
        });

        let mut wire_request = request("hola");
        wire_request.privacy_mode = PrivacyMode::RemoteAllowed;
        let payload = build_responses_payload(&spark_settings(), &wire_request).unwrap();
        let response = request_responses_completion(
            endpoint,
            payload,
            Some("test-key"),
            &CancellationToken::default(),
            Duration::from_secs(1),
            64,
        )
        .unwrap();
        server.join().unwrap();

        assert_eq!(response.text, "listo");
        assert!(!response.truncated);
    }

    #[test]
    fn fusion_returns_only_the_deepseek_audited_answer() {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let address = listener.local_addr().unwrap();
        let server = thread::spawn(move || {
            let (mut draft_stream, _) = listener.accept().unwrap();
            let mut draft_buffer = [0; 4096];
            let draft_bytes = draft_stream.read(&mut draft_buffer).unwrap();
            let draft_request = String::from_utf8_lossy(&draft_buffer[..draft_bytes]);
            assert!(draft_request.starts_with("POST /messages HTTP/1.1"));
            assert!(draft_request.contains("\"model\":\"mimo-2.5-vl\""));
            assert!(!draft_request.contains("previous question"));
            assert!(!draft_request.contains("previous answer"));
            let draft_body = r#"{"stop_reason":"end_turn","content":[{"type":"text","text":"uncorrected draft"}]}"#;
            write!(
                draft_stream,
                "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
                draft_body.len(),
                draft_body
            )
            .unwrap();

            let (mut audit_stream, _) = listener.accept().unwrap();
            let mut audit_buffer = [0; 8192];
            let audit_bytes = audit_stream.read(&mut audit_buffer).unwrap();
            let audit_request = String::from_utf8_lossy(&audit_buffer[..audit_bytes]);
            assert!(audit_request.starts_with("POST /chat/completions HTTP/1.1"));
            assert!(audit_request.contains("\"model\":\"deepseek-v4-pro\""));
            assert!(audit_request.contains("uncorrected draft"));
            assert!(audit_request.contains("Tetrahedron[x, y, z, edge]"));
            assert!(audit_request.contains("Tesseract4D[scale"));
            assert!(!audit_request.contains("previous question"));
            assert!(!audit_request.contains("previous answer"));
            let audit_body = r#"{"choices":[{"finish_reason":"stop","message":{"role":"assistant","content":"audited answer"}}]}"#;
            write!(
                audit_stream,
                "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
                audit_body.len(),
                audit_body
            )
            .unwrap();
        });
        let mut request = request("current question");
        request.privacy_mode = PrivacyMode::RemoteAllowed;
        request.conversation = vec![
            ConversationTurn::user("previous question"),
            ConversationTurn::assistant("previous answer"),
        ];
        let settings = ProviderSettings::for_profile(ProviderProfile::OpenCodeGo, "fusion");
        let response = request_fusion_completion_with_endpoints(
            Url::parse(&format!("http://{address}/messages")).unwrap(),
            Url::parse(&format!("http://{address}/chat/completions")).unwrap(),
            &settings,
            &request,
            Some("test-key"),
            &CancellationToken::default(),
            Duration::from_secs(1),
        )
        .unwrap();
        server.join().unwrap();

        assert_eq!(response.text, "audited answer");
    }

    #[test]
    fn fusion_discards_the_draft_when_deepseek_audit_fails() {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let address = listener.local_addr().unwrap();
        let server = thread::spawn(move || {
            let (mut draft_stream, _) = listener.accept().unwrap();
            let mut buffer = [0; 4096];
            let _ = draft_stream.read(&mut buffer);
            let draft_body =
                r#"{"stop_reason":"end_turn","content":[{"type":"text","text":"hidden draft"}]}"#;
            write!(
                draft_stream,
                "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
                draft_body.len(),
                draft_body
            )
            .unwrap();

            let (mut audit_stream, _) = listener.accept().unwrap();
            let _ = audit_stream.read(&mut buffer);
            audit_stream
                .write_all(
                    b"HTTP/1.1 500 Internal Server Error\r\nContent-Length: 0\r\nConnection: close\r\n\r\n",
                )
                .unwrap();
        });
        let mut request = request("current question");
        request.privacy_mode = PrivacyMode::RemoteAllowed;
        let settings = ProviderSettings::for_profile(ProviderProfile::OpenCodeGo, "fusion");
        let result = request_fusion_completion_with_endpoints(
            Url::parse(&format!("http://{address}/messages")).unwrap(),
            Url::parse(&format!("http://{address}/chat/completions")).unwrap(),
            &settings,
            &request,
            Some("test-key"),
            &CancellationToken::default(),
            Duration::from_secs(1),
        );
        server.join().unwrap();

        assert_eq!(
            result.unwrap_err(),
            "Fusion could not complete the audit; its draft was discarded."
        );
    }

    #[test]
    fn retry_after_parses_seconds_with_clamp_1_to_120() {
        assert_eq!(parse_retry_after_secs("7"), Some(7));
        assert_eq!(parse_retry_after_secs("  45  "), Some(45));
        assert_eq!(parse_retry_after_secs("0"), Some(1));
        assert_eq!(parse_retry_after_secs("-5"), Some(1));
        assert_eq!(parse_retry_after_secs("3600"), Some(120));
        assert_eq!(parse_retry_after_secs(""), None);
        assert_eq!(parse_retry_after_secs("not-a-date"), None);
    }

    #[test]
    fn retry_after_parses_http_dates_with_clamp() {
        // Futuro lejano → techo 120s; pasado → piso 1s. Sin dormir el worker.
        assert_eq!(
            parse_retry_after_secs("Wed, 21 Oct 2030 07:28:00 GMT"),
            Some(120)
        );
        assert_eq!(
            parse_retry_after_secs("Sun, 06 Nov 1994 08:49:37 GMT"),
            Some(1)
        );
        // RFC 850 pasado → 1s.
        assert_eq!(
            parse_retry_after_secs("Sunday, 06-Nov-94 08:49:37 GMT"),
            Some(1)
        );
        // asctime futuro → 120s.
        assert_eq!(
            parse_retry_after_secs("Wed Oct 21 07:28:00 2030"),
            Some(120)
        );
        assert_eq!(parse_retry_after_secs("Tomorrow, never"), None);
    }

    #[test]
    fn http_status_error_truncates_body_and_announces_retry_without_secrets() {
        let long = "x".repeat(2_000);
        let error = http_status_error(500, &long, None);
        assert!(error.starts_with("remote assistant returned HTTP 500: "));
        let snippet = error
            .strip_prefix("remote assistant returned HTTP 500: ")
            .unwrap();
        assert_eq!(snippet.chars().count(), MAX_ERROR_BODY_CHARS);
        assert!(!error.contains("test-key"));

        let rate_limited = http_status_error(429, "busy", Some(7));
        assert!(rate_limited.contains("429"));
        assert!(rate_limited.contains("reintentá en 7s"));

        let without_hint = http_status_error(429, "busy", None);
        assert!(without_hint.contains("429"));
        assert!(!without_hint.contains("reintentá"));
    }

    #[test]
    fn effective_timeout_never_uses_zero_or_above_120s() {
        // Simple 60s default y agente 30s/turno caben; 0 y >120s se clampean.
        assert_eq!(effective_remote_timeout_ms(0), REMOTE_TIMEOUT_MIN_MS);
        assert_eq!(effective_remote_timeout_ms(60_000), 60_000);
        assert_eq!(effective_remote_timeout_ms(30_000), 30_000);
        assert_eq!(
            effective_remote_timeout_ms(1_000_000),
            REMOTE_TIMEOUT_MAX_MS
        );
        assert!(!effective_remote_timeout(0).is_zero());
        assert!(effective_remote_timeout(u64::MAX) <= Duration::from_secs(120));
        assert!(effective_remote_timeout(60_000) == Duration::from_secs(60));
    }

    #[test]
    fn sse_delta_parser_extracts_only_text_deltas() {
        assert_eq!(
            responses_sse_delta_text(r#"{"type":"response.output_text.delta","delta":"Hola"}"#)
                .as_deref(),
            Some("Hola")
        );
        assert_eq!(
            responses_sse_delta_text(r#"{"type":"response.text.delta","delta":"mundo"}"#)
                .as_deref(),
            Some("mundo")
        );
        assert_eq!(
            responses_sse_delta_text(r#"{"type":"response.completed"}"#),
            None
        );
        assert_eq!(responses_sse_delta_text("not-json"), None);
    }

    #[test]
    fn sse_collector_joins_deltas_and_flags_incomplete() {
        let body = "event: response.output_text.delta\ndata: {\"type\":\"response.output_text.delta\",\"delta\":\"Hola\"}\n\nevent: response.output_text.delta\ndata: {\"type\":\"response.output_text.delta\",\"delta\":\" mundo\"}\n\ndata: [DONE]\n";
        assert_eq!(
            collect_responses_sse_text(body),
            ("Hola mundo".to_string(), false)
        );
        let truncated = "data: {\"type\":\"response.output_text.delta\",\"delta\":\"parcial\"}\ndata: {\"type\":\"response.incomplete\"}\n";
        assert_eq!(
            collect_responses_sse_text(truncated),
            ("parcial".to_string(), true)
        );
    }

    #[test]
    fn responses_stub_reports_429_with_retry_after_without_sleeping() {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let endpoint = Url::parse(&format!(
            "http://{}/responses",
            listener.local_addr().unwrap()
        ))
        .unwrap();
        let server = thread::spawn(move || {
            let (mut stream, _) = listener.accept().unwrap();
            let mut buffer = [0; 4096];
            let _ = stream.read(&mut buffer);
            let body = "busy";
            write!(
                stream,
                "HTTP/1.1 429 Too Many Requests\r\nContent-Type: text/plain\r\nRetry-After: 7\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
                body.len(),
                body
            )
            .unwrap();
        });
        let error = request_responses_completion(
            endpoint,
            json!({"model": "muse-spark-1.3-contributor"}),
            Some("test-key"),
            &CancellationToken::default(),
            Duration::from_secs(1),
            64,
        )
        .unwrap_err();
        server.join().unwrap();
        assert!(error.contains("429"), "{error}");
        assert!(error.contains("reintentá en 7s"), "{error}");
        assert!(!error.contains("test-key"), "{error}");
    }

    #[test]
    fn responses_stub_truncates_long_500_body_without_secrets() {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let endpoint = Url::parse(&format!(
            "http://{}/responses",
            listener.local_addr().unwrap()
        ))
        .unwrap();
        let long_body = "y".repeat(2_000);
        let server_body = long_body.clone();
        let server = thread::spawn(move || {
            let (mut stream, _) = listener.accept().unwrap();
            let mut buffer = [0; 8192];
            let _ = stream.read(&mut buffer);
            write!(
                stream,
                "HTTP/1.1 500 Internal Server Error\r\nContent-Type: text/plain\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
                server_body.len(),
                server_body
            )
            .unwrap();
        });
        let error = request_responses_completion(
            endpoint,
            json!({"model": "muse-spark-1.3-contributor"}),
            Some("test-key"),
            &CancellationToken::default(),
            Duration::from_secs(1),
            64,
        )
        .unwrap_err();
        server.join().unwrap();
        assert!(
            error.starts_with("remote assistant returned HTTP 500: "),
            "{error}"
        );
        let snippet = error
            .strip_prefix("remote assistant returned HTTP 500: ")
            .unwrap();
        assert_eq!(snippet.chars().count(), MAX_ERROR_BODY_CHARS);
        assert!(!error.contains("test-key"));
        assert!(long_body.starts_with(snippet));
    }

    #[test]
    fn responses_stub_marks_incomplete_as_truncated_with_partial_text() {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let endpoint = Url::parse(&format!(
            "http://{}/responses",
            listener.local_addr().unwrap()
        ))
        .unwrap();
        let server = thread::spawn(move || {
            let (mut stream, _) = listener.accept().unwrap();
            let mut buffer = [0; 8192];
            let _ = stream.read(&mut buffer);
            let body = r#"{"status":"incomplete","output":[{"type":"message","content":[{"type":"output_text","text":"parcial"}]}]}"#;
            write!(
                stream,
                "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
                body.len(),
                body
            )
            .unwrap();
        });
        let completion = request_responses_completion(
            endpoint,
            json!({"model": "muse-spark-1.3-contributor"}),
            Some("test-key"),
            &CancellationToken::default(),
            Duration::from_secs(1),
            64,
        )
        .unwrap();
        server.join().unwrap();
        assert_eq!(completion.text, "parcial");
        assert!(completion.truncated);
    }

    #[test]
    fn system_prompt_is_binding_and_contains_socratic_directive() {
        let req = request("hola");
        let prompt = remote_system_prompt(&req);
        assert!(prompt.contains("MODO SOCRÁTICO VINCULANTE"), "{prompt}");
        assert!(prompt.contains("can_reveal = attempts>=2"));
        assert!(prompt.contains("Telling_rate") || prompt.contains("telling"));
        // No debe ser sugerencia: debe decir ORDEN
        assert!(prompt.contains("ORDEN"));
    }

    #[test]
    fn socratic_injection_is_deterministic_and_contains_question_and_history() {
        use grafito_pedagogy::{PedagogicalLevel, ScaffoldEngine, SocraticFsm};
        let req = request("derivada");
        let mut fsm = SocraticFsm::new("derivada");
        fsm.record_attempt(Some("sign".into()));
        let engine = ScaffoldEngine;
        let history = vec![grafito_pedagogy::scaffold::Turn {
            role: "user".into(),
            content: "no entiendo bien".into(),
        }];
        let scaffold = engine.scaffold("derivada", PedagogicalLevel::Secondary, &history);
        let prompt1 = remote_system_prompt_with_socratic(&req, &fsm, &scaffold);
        let prompt2 = remote_system_prompt_with_socratic(&req, &fsm, &scaffold);
        assert_eq!(prompt1, prompt2, "determinista");
        assert!(prompt1.contains("Pregunta BKT actual"), "{prompt1}");
        assert!(prompt1.contains("derivada"), "{prompt1}");
        assert!(prompt1.contains("Pista scaffold"), "{prompt1}");
        assert!(prompt1.contains("Historial"), "{prompt1}");
        assert!(prompt1.contains("Attempts: 1"), "{prompt1}");
        assert!(prompt1.contains("VINCULANTE"), "{prompt1}");
    }

    #[test]
    fn telling_guard_blocks_early_reveal_and_forces_repreguntar() {
        use grafito_pedagogy::{Scaffold, SocraticFsm};
        let fsm = SocraticFsm::new("integral");
        let scaffold = Scaffold {
            question: "¿Qué representa integral en y=x²?".into(),
            hint: Some("Mirá el área bajo la curva".into()),
            explanation: "Integral es área".into(),
        };
        // attempts 0 -> telling debe bloquear
        let telling = "La solución es x = 4";
        assert!(contains_telling_markers(telling));
        assert!(check_telling_guard(&fsm, telling).is_err());
        let repair = enforce_telling_guard(&fsm, telling, &scaffold).unwrap_err();
        assert!(repair.contains("GUARD TELLING"), "{repair}");
        assert!(repair.contains("¿Qué representa integral"), "{repair}");
        // con attempts>=2 pasa
        let mut fsm2 = SocraticFsm::new("integral");
        fsm2.record_attempt(None);
        fsm2.record_attempt(None);
        assert!(check_telling_guard(&fsm2, telling).is_ok());
        assert!(enforce_telling_guard(&fsm2, telling, &scaffold).is_ok());
        // sin marcador no es telling aunque attempts<2
        assert!(check_telling_guard(&fsm, "¿Cómo lo pensaste?").is_ok());
    }

    #[test]
    fn guard_remote_completion_forces_re_repair() {
        use grafito_pedagogy::{Scaffold, SocraticFsm};
        let fsm = SocraticFsm::new("derivada");
        let scaffold = Scaffold {
            question: "¿Cómo definirías derivada con límites?".into(),
            hint: Some("Recordá f'(x)=lim".into()),
            explanation: "formal".into(),
        };
        let completion = RemoteCompletion {
            text: "La respuesta es x = 2".into(),
            truncated: false,
        };
        let guarded = guard_remote_completion(&fsm, completion.clone(), &scaffold);
        assert!(guarded.is_err(), "debe bloquear telling con attempts=0");
        let mut fsm2 = SocraticFsm::new("derivada");
        fsm2.record_attempt(None);
        fsm2.record_attempt(None);
        let guarded2 = guard_remote_completion(&fsm2, completion, &scaffold);
        assert!(guarded2.is_ok());
    }

    #[test]
    fn scaffold_history_injection_is_deterministic() {
        use grafito_pedagogy::{PedagogicalLevel, SocraticFsm};
        let req = request("concepto test");
        let fsm = SocraticFsm::new("integrales");
        let hist = vec![
            grafito_pedagogy::scaffold::Turn {
                role: "user".into(),
                content: "concepto incorrectoo".into(),
            },
            grafito_pedagogy::scaffold::Turn {
                role: "assistant".into(),
                content: "probá con x=1".into(),
            },
        ];
        let prompt = remote_system_prompt_with_scaffold(
            &req,
            &fsm,
            "integrales",
            PedagogicalLevel::Secondary,
            &hist,
        );
        assert!(prompt.contains("integrales"));
        assert!(prompt.contains("Pista concreta") || prompt.contains("Pista scaffold"));
        assert!(prompt.contains("Historial"));
    }
}
