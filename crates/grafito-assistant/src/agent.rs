//! Transporte agéntico: proveedor real con tool calling y herramientas seguras.
//!
//! Conecta el núcleo de grafito-agent (loop, schema, router) con el transporte
//! OpenAI-compatible existente. Las herramientas nunca mutan el documento; sólo
//! producen resultados acotados para el modelo. El Apply de propuestas gráficas
//! sigue siendo una decisión explícita del usuario en la capa de UI.
//!
//! F3.2 — PedagogyDispatcher: 6 tools pedagógicas puras (scaffold, generate_exercise,
//! assess_answer, get_curriculum, suggest_next, generate_animation) orquestables
//! vía OpenCode Go sin salir del chat. Todas son puras, sin I/O ni mutación de Document.

use crate::ProviderSettings;
use grafito_agent::ledger::{JSpaceLedger, MAX_LEDGER_RENDER_BYTES};
use grafito_agent::loop_engine::{
    run_agent, run_agent_with_ledger, AgentBudget, AgentChatResponse, AgentCompleter, AgentOutcome,
    Cancellation, ToolDispatcher,
};
use grafito_agent::schema::{ToolCall, ToolResult, ToolSchema};
use grafito_agent::AgentEvent;
use serde_json::{json, Value};
use std::sync::mpsc::Receiver;
use std::thread::JoinHandle;
use std::time::{Duration, Instant};

/// Límite del cuerpo de la respuesta del agente.
const MAX_AGENT_RESPONSE_BYTES: usize = 32 * 1024;

/// Proveedor de chat agéntico sobre un endpoint OpenAI-compatible.
pub struct RemoteAgentCompleter {
    settings: ProviderSettings,
    api_key: Option<String>,
}

impl RemoteAgentCompleter {
    pub fn new(settings: ProviderSettings, api_key: Option<String>) -> Self {
        Self { settings, api_key }
    }
}

impl AgentCompleter for RemoteAgentCompleter {
    fn complete(
        &self,
        messages: &[Value],
        tools: &[ToolSchema],
        max_output_tokens: usize,
        timeout: Duration,
        cancellation: &Cancellation,
    ) -> Result<AgentChatResponse, String> {
        request_agent_completion(
            &self.settings,
            self.api_key.as_deref(),
            messages,
            tools,
            max_output_tokens,
            timeout,
            cancellation,
        )
    }
}

/// Ejecuta el loop completo en un hilo de trabajo y expone sus eventos.
pub fn request_agent_on_worker(
    settings: ProviderSettings,
    api_key: Option<String>,
    system: String,
    user_messages: Vec<Value>,
    tools: Vec<ToolSchema>,
    budget: AgentBudget,
    cancellation: Cancellation,
) -> (
    JoinHandle<Result<AgentOutcome, String>>,
    Receiver<AgentEvent>,
) {
    let (sender, receiver) = std::sync::mpsc::sync_channel(128);
    // F10-FIX mitigación (no fix): este worker despacha args DEL MODELO a
    // `evaluate` (`SafeGrafitoDispatcher::evaluate_expr`). 8 MiB dan aire a
    // expresiones profundas pero acotadas; el fix real es la guarda de
    // presupuesto en `grafito_geometry::expr::evaluate` + el freno de 2000
    // bytes en el dispatcher. Sin `unwrap`: si el spawn con nombre falla se
    // degrada a un worker que retorna el error honesto.
    let handle = match std::thread::Builder::new()
        .name("grafito-agent-worker".into())
        .stack_size(8 << 20)
        .spawn(move || {
            // Muse Spark sólo responde por Responses API; el resto de modelos sigue
            // por Chat Completions vía `run_agent` (path intacto).
            if uses_responses_agent_transport(&settings) {
                let dispatcher = SafeGrafitoDispatcher;
                return run_responses_agent_loop(
                    &settings,
                    api_key.as_deref(),
                    &system,
                    &user_messages,
                    &tools,
                    &budget,
                    None,
                    &dispatcher,
                    &cancellation,
                    |event| {
                        // Bounded channel (128) evita crecimiento ilimitado si la UI no drena;
                        // ante backpressure se abandona el envío (canal lleno o desconectado).
                        let _ = sender.try_send(event);
                    },
                );
            }
            let completer = RemoteAgentCompleter::new(settings, api_key);
            let dispatcher = SafeGrafitoDispatcher;
            run_agent(
                &completer,
                &dispatcher,
                &system,
                &user_messages,
                &tools,
                &budget,
                &cancellation,
                |event| {
                    // Bounded channel (128) evita crecimiento ilimitado si la UI no drena;
                    // ante backpressure se abandona el envío (canal lleno o desconectado).
                    let _ = sender.try_send(event);
                },
            )
        }) {
        Ok(handle) => handle,
        Err(error) => {
            std::thread::spawn(move || Err(format!("agent worker spawn failed: {error}")))
        }
    };
    (handle, receiver)
}

/// Igual que request_agent_on_worker pero inyecta un ledger J-Space de tarea.
#[allow(clippy::too_many_arguments)]
pub fn request_agent_on_worker_with_ledger(
    settings: ProviderSettings,
    api_key: Option<String>,
    system: String,
    user_messages: Vec<Value>,
    tools: Vec<ToolSchema>,
    budget: AgentBudget,
    ledger: Option<JSpaceLedger>,
    cancellation: Cancellation,
) -> (
    JoinHandle<Result<AgentOutcome, String>>,
    Receiver<AgentEvent>,
) {
    let (sender, receiver) = std::sync::mpsc::sync_channel(128);
    // F10-FIX mitigación (no fix): idem `request_agent_on_worker` —este worker
    // con ledger también evalúa args del modelo vía `SafeGrafitoDispatcher`.
    let handle = match std::thread::Builder::new()
        .name("grafito-agent-worker-ledger".into())
        .stack_size(8 << 20)
        .spawn(move || {
            // Misma bifurcación que `request_agent_on_worker`: Spark por Responses.
            if uses_responses_agent_transport(&settings) {
                let dispatcher = SafeGrafitoDispatcher;
                return run_responses_agent_loop(
                    &settings,
                    api_key.as_deref(),
                    &system,
                    &user_messages,
                    &tools,
                    &budget,
                    ledger.as_ref(),
                    &dispatcher,
                    &cancellation,
                    |event| {
                        let _ = sender.try_send(event);
                    },
                );
            }
            let completer = RemoteAgentCompleter::new(settings, api_key);
            let dispatcher = SafeGrafitoDispatcher;
            run_agent_with_ledger(
                &completer,
                &dispatcher,
                &system,
                &user_messages,
                &tools,
                &budget,
                ledger.as_ref(),
                &cancellation,
                |event| {
                    let _ = sender.try_send(event);
                },
            )
        }) {
        Ok(handle) => handle,
        Err(error) => {
            std::thread::spawn(move || Err(format!("agent worker spawn failed: {error}")))
        }
    };
    (handle, receiver)
}

/// Despachador de las herramientas incorporadas y seguras de Grafito.
///
/// Ninguna de estas herramientas muta el documento, accede a archivos ni
/// ejecuta código; sólo evalúan matemática y consultan conocimiento local.
pub struct SafeGrafitoDispatcher;

/// Despachador pedagógico puro (6 tools F3.2).
///
/// Expone sólo las herramientas pedagógicas; es útil para tests aislados o
/// para orquestación pedagógica sin las tools base (evaluate_expr, etc.).
/// `SafeGrafitoDispatcher` ya incluye estas 6 más las base, por compatibilidad.
pub struct PedagogyDispatcher;

impl ToolDispatcher for SafeGrafitoDispatcher {
    fn dispatch(&self, call: &ToolCall) -> ToolResult {
        dispatch_safe_tool(call)
    }
}

impl ToolDispatcher for PedagogyDispatcher {
    fn dispatch(&self, call: &ToolCall) -> ToolResult {
        dispatch_pedagogy_tool(call)
    }
}

/// `ask_user` real vía evento pendiente (S2): nunca ejecuta en silencio.
///
/// Pura y no bloqueante: devuelve `ok=false` + JSON `needs_user:true` con
/// `question` + `options` que la UI muestra como botones en el turno. La
/// respuesta del usuario vuelve al loop como `function_call_output`/mensaje
/// vía `grafito_agent::tools::{ask_user_answer_function_output,
/// ask_user_answer_tool_message}`. Reutiliza el saneado del dispatcher base
/// (`grafito-agent`), sin duplicar topes ni mutar `Document`.
fn ask_user_tool(call: &ToolCall) -> ToolResult {
    match grafito_agent::tools::parse_ask_user_request(call) {
        Ok(request) => ToolResult::text(
            &call.id,
            false,
            grafito_agent::tools::format_ask_user_pending(&request),
        ),
        Err(error) => ToolResult::text(&call.id, false, error.to_string()),
    }
}

fn dispatch_safe_tool(call: &ToolCall) -> ToolResult {
    if let Some(rejected) = reject_oversized_string_args(call) {
        return rejected;
    }
    match call.name.as_str() {
        "evaluate_expr" => evaluate_expr_tool(call),
        "grafito_docs" => grafito_docs_tool(call),
        "ask_user" => ask_user_tool(call),
        // Pedagogy tools (F3.2) — puras, sin Document, sin I/O
        "scaffold" => scaffold_tool(call),
        "generate_exercise" => generate_exercise_tool(call),
        "assess_answer" => assess_answer_tool(call),
        "get_curriculum" => get_curriculum_tool(call),
        "suggest_next" => suggest_next_tool(call),
        "generate_animation" => generate_animation_tool(call),
        unknown => ToolResult::text(
            &call.id,
            false,
            format!("tool '{unknown}' is not available in this session"),
        ),
    }
}

fn dispatch_pedagogy_tool(call: &ToolCall) -> ToolResult {
    if let Some(rejected) = reject_oversized_string_args(call) {
        return rejected;
    }
    match call.name.as_str() {
        "scaffold" => scaffold_tool(call),
        "generate_exercise" => generate_exercise_tool(call),
        "assess_answer" => assess_answer_tool(call),
        "get_curriculum" => get_curriculum_tool(call),
        "suggest_next" => suggest_next_tool(call),
        "generate_animation" => generate_animation_tool(call),
        unknown => ToolResult::text(
            &call.id,
            false,
            format!("pedagogy tool '{unknown}' no disponible; tools válidas: scaffold, generate_exercise, assess_answer, get_curriculum, suggest_next, generate_animation"),
        ),
    }
}

fn string_arg(call: &ToolCall, key: &str) -> Option<String> {
    call.arguments
        .get(key)
        .and_then(Value::as_str)
        .filter(|value| !value.trim().is_empty() && value.len() <= 2_000)
        .map(str::to_owned)
}

/// Rechaza cualquier argumento de cadena >2000 bytes antes de despachar la tool.
///
/// Mitiga DoS por payload excesivo sin necesidad de parsear el contenido.
fn reject_oversized_string_args(call: &ToolCall) -> Option<ToolResult> {
    // Recorre recursivamente todos los strings en arguments (incluye objetos anidados)
    fn check_value(call_id: &str, key: &str, value: &Value) -> Option<ToolResult> {
        if let Some(text) = value.as_str() {
            if text.len() > 2_000 {
                return Some(ToolResult::text(
                    call_id,
                    false,
                    format!("argument '{key}' exceeds 2000 byte limit"),
                ));
            }
        } else if let Some(map) = value.as_object() {
            for (nested_key, nested_value) in map {
                if let Some(rejected) = check_value(call_id, nested_key, nested_value) {
                    return Some(rejected);
                }
            }
        } else if let Some(array) = value.as_array() {
            for element in array {
                if let Some(text) = element.as_str() {
                    if text.len() > 2_000 {
                        return Some(ToolResult::text(
                            call_id,
                            false,
                            format!("argument '{key}' array element exceeds 2000 byte limit"),
                        ));
                    }
                }
            }
        }
        None
    }
    if let Some(map) = call.arguments.as_object() {
        for (key, value) in map {
            if let Some(rejected) = check_value(&call.id, key, value) {
                return Some(rejected);
            }
        }
    }
    None
}

// ── Helpers de nivel pedagógico ─────────────────────────────────────────────

/// Parsea un nivel pedagógico desde texto libre; default Secondary si vacío/desconocido.
fn parse_pedagogical_level(raw: Option<&str>) -> grafito_pedagogy::PedagogicalLevel {
    use grafito_pedagogy::{PedagogicalLevel, UTNProgram};
    let Some(text) = raw.map(|s| s.trim().to_lowercase()) else {
        return PedagogicalLevel::Secondary;
    };
    match text.as_str() {
        "primary" | "primaria" | "primario" => PedagogicalLevel::Primary,
        "secondary" | "secundaria" | "secundario" => PedagogicalLevel::Secondary,
        "university" | "universidad" | "universitario" => PedagogicalLevel::University,
        "utn_am1" | "utnam1" | "am1" | "utn am1" => PedagogicalLevel::UTN(UTNProgram::AM1),
        "utn_am2" | "utnam2" | "am2" | "utn am2" => PedagogicalLevel::UTN(UTNProgram::AM2),
        "utn_algebra" | "algebra" | "álgebra" => PedagogicalLevel::UTN(UTNProgram::Algebra),
        "utn_probabilidad" | "probabilidad" | "prob" | "utn_prob" => {
            PedagogicalLevel::UTN(UTNProgram::Probabilidad)
        }
        _ => PedagogicalLevel::Secondary,
    }
}

fn level_label(level: grafito_pedagogy::PedagogicalLevel) -> String {
    level.label().to_string()
}

// ── Tools base ──────────────────────────────────────────────────────────────

fn evaluate_expr_tool(call: &ToolCall) -> ToolResult {
    // F10-FIX (SIGABRT): la tool se defiende sola aunque la llamen directa
    // (bypass de `reject_oversized_string_args`): jamás pasar a `evaluate`
    // una expresión sobre el presupuesto —el stack overflow aborta el proceso.
    let Some(raw) = call.arguments.get("expression").and_then(Value::as_str) else {
        return ToolResult::text(
            &call.id,
            false,
            "evaluate_expr requires an 'expression' string",
        );
    };
    if raw.trim().is_empty() {
        return ToolResult::text(
            &call.id,
            false,
            "evaluate_expr requires an 'expression' string",
        );
    }
    if raw.len() > grafito_geometry::expr::MAX_EXPR_LENGTH {
        return ToolResult::text(
            &call.id,
            false,
            format!(
                "argument 'expression' exceeds {} byte limit",
                grafito_geometry::expr::MAX_EXPR_LENGTH
            ),
        );
    }
    let expression = raw.to_owned();
    let mut variables = Vec::new();
    if let Some(object) = call.arguments.get("variables").and_then(Value::as_object) {
        for (name, value) in object {
            if let Some(number) = value.as_f64().filter(|number| number.is_finite()) {
                variables.push((name.clone(), number));
            }
        }
    }
    match grafito_geometry::expr::evaluate(&expression, &variables) {
        Ok(value) if value.is_finite() => {
            ToolResult::text(&call.id, true, crate::format_number(value))
        }
        Ok(_) => ToolResult::text(
            &call.id,
            false,
            "the expression evaluated to a non-finite value",
        ),
        Err(error) => ToolResult::text(&call.id, false, format!("evaluation failed: {error}")),
    }
}

fn grafito_docs_tool(call: &ToolCall) -> ToolResult {
    let query = string_arg(call, "query").unwrap_or_default();
    let catalog = grafito_command::assistant_context::assistant_tool_catalog(&query, 2_048);
    if catalog.trim().is_empty() {
        return ToolResult::text(
            &call.id,
            false,
            "there is no catalogued Grafito command matching this query",
        );
    }
    ToolResult::text(&call.id, true, catalog)
}

// ── Tools pedagógicas F3.2 ──────────────────────────────────────────────────

/// scaffold(concept, level) — andamiaje socrático puro vía ScaffoldEngine.
fn scaffold_tool(call: &ToolCall) -> ToolResult {
    let Some(concept) = string_arg(call, "concept") else {
        return ToolResult::text(&call.id, false, "scaffold requiere 'concept' no vacío");
    };
    let level = parse_pedagogical_level(string_arg(call, "level").as_deref());
    let engine = grafito_pedagogy::ScaffoldEngine;
    // Histórico vacío por pureza; el LLM puede pasar contexto adicional vía concept.
    let scaffold = engine.scaffold(&concept, level, &[]);
    let payload = json!({
        "concept": concept,
        "level": level_label(level),
        "question": scaffold.question,
        "hint": scaffold.hint,
        "explanation": scaffold.explanation,
    });
    ToolResult::text(&call.id, true, payload.to_string())
}

/// generate_exercise(lo_id, level, seed?) — ejercicio determinista vía ExerciseGenerator.
fn generate_exercise_tool(call: &ToolCall) -> ToolResult {
    let lo_id = string_arg(call, "lo_id")
        .or_else(|| string_arg(call, "learning_objective_id"))
        .or_else(|| string_arg(call, "id"))
        .or_else(|| string_arg(call, "concept"));
    let Some(lo_id) = lo_id else {
        return ToolResult::text(
            &call.id,
            false,
            "generate_exercise requiere 'lo_id' (ej. am1-der)",
        );
    };
    let level = parse_pedagogical_level(string_arg(call, "level").as_deref());
    let seed = call
        .arguments
        .get("seed")
        .and_then(Value::as_u64)
        .unwrap_or(0);
    let lo = match grafito_pedagogy::Curriculum::get(&lo_id) {
        Some(found) => found,
        None => {
            let candidates = grafito_pedagogy::Curriculum::find_for_concept(&lo_id);
            match candidates.into_iter().next() {
                Some(found) => found,
                None => {
                    return ToolResult::text(
                        &call.id,
                        false,
                        format!("LearningObjective no encontrado: '{lo_id}'"),
                    )
                }
            }
        }
    };
    let gen = grafito_pedagogy::ExerciseGenerator;
    let exercise = gen.generate_with_seed(&lo, level, seed);
    if let Err(reason) = exercise.validate() {
        return ToolResult::text(&call.id, false, format!("ejercicio inválido: {reason}"));
    }
    let payload = json!({
        "lo_id": exercise.lo_id,
        "prompt": exercise.prompt,
        "solution": exercise.solution,
        "kind": format!("{:?}", exercise.kind),
        "difficulty": format!("{:?}", exercise.difficulty),
        "params": exercise.params,
        "seed": exercise.seed,
        "validator": format!("{:?}", exercise.validator),
        "level": level_label(level),
    });
    ToolResult::text(&call.id, true, payload.to_string())
}

/// assess_answer(exercise_id?/lo_id?, answer, level?, seed?) — feedback vía FeedbackEngine.
fn assess_answer_tool(call: &ToolCall) -> ToolResult {
    let answer = match string_arg(call, "answer") {
        Some(value) => value,
        None => {
            return ToolResult::text(&call.id, false, "assess_answer requiere 'answer' no vacío")
        }
    };
    let lo_id_opt = string_arg(call, "exercise_id")
        .or_else(|| string_arg(call, "lo_id"))
        .or_else(|| string_arg(call, "learning_objective_id"))
        .or_else(|| string_arg(call, "id"));
    if let Some(lo_id) = lo_id_opt {
        let lo = match grafito_pedagogy::Curriculum::get(&lo_id) {
            Some(found) => found,
            None => {
                let candidates = grafito_pedagogy::Curriculum::find_for_concept(&lo_id);
                match candidates.into_iter().next() {
                    Some(found) => found,
                    None => {
                        return ToolResult::text(
                            &call.id,
                            false,
                            format!("LearningObjective no encontrado para assess: '{lo_id}'"),
                        )
                    }
                }
            }
        };
        let seed = call
            .arguments
            .get("seed")
            .and_then(Value::as_u64)
            .unwrap_or(0);
        let level = parse_pedagogical_level(string_arg(call, "level").as_deref());
        let exercise = grafito_pedagogy::ExerciseGenerator.generate_with_seed(&lo, level, seed);
        let engine = grafito_pedagogy::FeedbackEngine;
        let feedback = engine.assess(&exercise, &answer);
        let payload = json!({
            "lo_id": lo.id,
            "exercise_prompt": exercise.prompt,
            "expected": exercise.solution,
            "answer": answer,
            "correct": feedback.correct,
            "misconception": format!("{:?}", feedback.misconception),
            "message": feedback.message,
            "next_step": feedback.next_step,
        });
        // ok = true incluso si correct=false, porque la evaluación sí se hizo; sólo ok=false si faltan datos.
        ToolResult::text(&call.id, true, payload.to_string())
    } else {
        let payload = json!({
            "answer": answer,
            "correct": false,
            "misconception": "Concept",
            "message": "No se proporcionó exercise_id/lo_id; no se puede validar contra un ejercicio concreto. Provee lo_id para evaluación precisa.",
            "next_step": "Provee lo_id (ej. am1-der) junto a answer para evaluación exacta.",
            "hint": "Ejemplo: assess_answer {\"lo_id\": \"am1-der\", \"answer\": \"2*x+1\"}"
        });
        ToolResult::text(&call.id, false, payload.to_string())
    }
}

/// get_curriculum(query) — busca LOs vía Curriculum::find_for_concept.
fn get_curriculum_tool(call: &ToolCall) -> ToolResult {
    let query = string_arg(call, "query")
        .or_else(|| string_arg(call, "concept"))
        .or_else(|| string_arg(call, "q"))
        .unwrap_or_default();
    if query.trim().is_empty() {
        return ToolResult::text(&call.id, false, "get_curriculum requiere 'query' no vacío");
    }
    let results = grafito_pedagogy::Curriculum::find_for_concept(&query);
    if results.is_empty() {
        return ToolResult::text(&call.id, false, format!("sin resultados para '{query}'"));
    }
    let items: Vec<Value> = results
        .into_iter()
        .take(5)
        .map(|lo| {
            json!({
                "id": lo.id,
                "title": lo.title,
                "description": lo.description,
                "program": lo.program.map(|p| p.label().to_string()),
                "level_min": lo.level_min,
                "requires": lo.requires,
                "tags": lo.tags,
                "estimated_hours": lo.estimated_hours,
            })
        })
        .collect();
    let payload = json!({
        "query": query,
        "count": items.len(),
        "results": items,
    });
    ToolResult::text(&call.id, true, payload.to_string())
}

/// suggest_next — sugiere el próximo LO usando StudentProfile::recommend_next (mock puro).
fn suggest_next_tool(call: &ToolCall) -> ToolResult {
    // Perfil mock determinista, sin Document ni I/O.
    // Si el LLM pasa "branch_id" o "level", lo consideramos; si no, usamos perfil genérico.
    let mut profile = grafito_profile::StudentProfile::new("estudiante_mock");
    // Semilla simple: 2 ramas con distinto mastery para que recommend_next sea determinista
    // Si el caller pasa "mastery_hint" lo usamos, si no dejamos datos por defecto.
    // Para mantener pureza, no leemos disco.
    let _ = string_arg(call, "branch_id");
    // Construir un perfil con progreso variado:
    // - am1-func dominada parcialmente
    // - am1-lim pendiente (más débil)
    // - am1-der no vista
    // Esto garantiza que recommend_next devuelva algo ordenado por mastery ascendente.
    if let Some(idx) = profile.ensure_branch("am1-func", "Funciones") {
        if let Some(branch) = profile.branches.get_mut(idx) {
            branch.mastery = 0.6;
            branch.covered = false;
        }
    }
    if let Some(idx) = profile.ensure_branch("am1-lim", "Límites") {
        if let Some(branch) = profile.branches.get_mut(idx) {
            branch.mastery = 0.2;
            branch.covered = false;
        }
    }
    if let Some(idx) = profile.ensure_branch("am1-der", "Derivadas") {
        if let Some(branch) = profile.branches.get_mut(idx) {
            branch.mastery = 0.0;
            branch.covered = false;
        }
    }
    let next = profile.recommend_next();
    let items: Vec<Value> = next
        .iter()
        .take(3)
        .map(|branch| {
            json!({
                "id": branch.id,
                "name": branch.name,
                "mastery": branch.mastery,
                "covered": branch.covered,
                "box_level": branch.box_level,
                "next_review_epoch": branch.next_review_epoch,
            })
        })
        .collect();
    if items.is_empty() {
        // Fallback si el mock no produjo nada (no debería)
        let lo = grafito_pedagogy::Curriculum::get("am1-func");
        let fallback = lo.map(|value| {
            json!({
                "id": value.id,
                "title": value.title,
                "description": value.description,
            })
        });
        let payload = json!({
            "mock": true,
            "next": fallback,
            "note": "perfil mock vacío; se sugiere am1-func por defecto",
        });
        return ToolResult::text(&call.id, true, payload.to_string());
    }
    let payload = json!({
        "mock": true,
        "count": items.len(),
        "next": items,
        "note": "perfil mock puro; en la app real se usa StudentProfile persistido (recommend_next)",
    });
    ToolResult::text(&call.id, true, payload.to_string())
}

/// Extrae canvas desde los argumentos del schema con fallback a 640x480.
///
/// Acepta `canvas: [w,h]`, `width`/`height` top-level, o `params.width`/`params.height`/
/// `params.canvas_width`/`params.canvas_height`. Valida 64..=4096; fuera de rango ignora y usa default.
fn canvas_from_call(call: &ToolCall) -> (u32, u32) {
    const DEFAULT: (u32, u32) = (640, 480);
    const MIN: u64 = 64;
    const MAX: u64 = 4096;
    // 1. canvas como array [w,h]
    if let Some(array) = call.arguments.get("canvas").and_then(Value::as_array) {
        if array.len() == 2 {
            if let (Some(width), Some(height)) = (array[0].as_u64(), array[1].as_u64()) {
                if (MIN..=MAX).contains(&width) && (MIN..=MAX).contains(&height) {
                    return (width as u32, height as u32);
                }
            }
        }
    }
    // 2. width/height top-level o dentro de params
    let mut width_opt = call
        .arguments
        .get("width")
        .and_then(Value::as_u64)
        .filter(|value| (MIN..=MAX).contains(value))
        .map(|value| value as u32);
    let mut height_opt = call
        .arguments
        .get("height")
        .and_then(Value::as_u64)
        .filter(|value| (MIN..=MAX).contains(value))
        .map(|value| value as u32);
    if let Some(object) = call.arguments.get("params").and_then(Value::as_object) {
        if width_opt.is_none() {
            width_opt = object
                .get("width")
                .or_else(|| object.get("canvas_width"))
                .and_then(Value::as_u64)
                .filter(|value| (MIN..=MAX).contains(value))
                .map(|value| value as u32);
        }
        if height_opt.is_none() {
            height_opt = object
                .get("height")
                .or_else(|| object.get("canvas_height"))
                .and_then(Value::as_u64)
                .filter(|value| (MIN..=MAX).contains(value))
                .map(|value| value as u32);
        }
    }
    match (width_opt, height_opt) {
        (Some(width), Some(height)) => (width, height),
        (Some(width), None) => (width, DEFAULT.1),
        (None, Some(height)) => (DEFAULT.0, height),
        (None, None) => DEFAULT,
    }
}

/// generate_animation(template, concept, params, pedido) — valida sin ejecutar motor.
///
/// Dos vías (puras, sin Python):
/// - `pedido` (nuevo, AS4): texto libre en lenguaje natural que se infiere a
///   `ParametricAnim` 100% Rust (barrido, traza, transición, lugar, tangente
///   o área móvil). Si falta algo, el error dice qué falta con un ejemplo;
///   jamás se inventa matemática. Tiene precedencia sobre template/concept.
/// - template/concept/params (histórica): valida `AnimRequest` por concepto.
fn generate_animation_tool(call: &ToolCall) -> ToolResult {
    // AS4: vía pedido en lenguaje natural → ParametricAnim (sin Python).
    if let Some(pedido) = string_arg(call, "pedido") {
        return propose_parametric_tool(&call.id, &pedido);
    }
    let template_raw = string_arg(call, "template").unwrap_or_default();
    let concept_raw = string_arg(call, "concept").unwrap_or_default();
    let mut params_map = std::collections::BTreeMap::new();
    if let Some(object) = call.arguments.get("params").and_then(Value::as_object) {
        for (key, value) in object {
            if let Some(number) = value.as_f64() {
                if number.is_finite() {
                    params_map.insert(key.clone(), number);
                }
            }
        }
    }
    if template_raw.trim().is_empty() && concept_raw.trim().is_empty() {
        return ToolResult::text(
            &call.id,
            false,
            "generate_animation requiere al menos 'concept' o 'template' no vacío",
        );
    }
    // Concept puede venir como query si template está vacío
    let concept = if concept_raw.trim().is_empty() {
        template_raw.clone()
    } else {
        concept_raw.clone()
    };
    let template = grafito_anim::protocol::sanitize_template(&template_raw, &concept);
    let normalized_concept = grafito_anim::protocol::normalize_concept(&concept);
    let canvas = canvas_from_call(call);
    let resolution =
        grafito_anim::protocol::Resolution::try_new(canvas.0, canvas.1).unwrap_or_default();
    let duration = grafito_anim::protocol::AnimDuration::try_new(2.0).unwrap_or_default();
    let anim_params = grafito_anim::protocol::AnimParams {
        template: template.clone(),
        concept: normalized_concept.clone(),
        params: params_map.clone(),
        duration,
        resolution,
        export: grafito_anim::ExportFormat::Gif,
        spec: None,
    };
    let request = anim_params.into_request();
    if let Err(error) = request.validate() {
        return ToolResult::text(&call.id, false, format!("AnimRequest inválido: {error}"));
    }
    let mut payload = json!({
        "template": template,
        "concept": normalized_concept,
        "params": params_map,
        "export": request.export.as_str(),
        "canvas": request.canvas,
        "protocol_version": grafito_anim::protocol::ANIM_PROTOCOL_VERSION,
        "note": "solicitud validada; el motor de animación se ejecuta en la capa UI tras aprobación explícita"
    });
    // N1: la vía template/concept de integral no trae función: declara la
    // canónica que va a renderizar (f(x)=x^2 en [0,2]) para que la prosa no
    // pregunte lo que la vista ya muestra.
    if payload
        .get("template")
        .and_then(|t| t.as_str())
        .is_some_and(|t| t == "integral-area")
    {
        payload["canonical"] = json!(true);
        payload["canonical_expr"] = json!(grafito_anim::parametric::INTEGRAL_CANONICAL_EXPR);
        payload["canonical_range"] = json!([
            grafito_anim::parametric::INTEGRAL_CANONICAL_P0,
            grafito_anim::parametric::INTEGRAL_CANONICAL_P1
        ]);
        payload["canonical_prose"] = json!(grafito_anim::parametric::INTEGRAL_CANONICAL_PROSA);
    }
    ToolResult::text(&call.id, true, payload.to_string())
}

/// Propone una animación paramétrica desde un pedido en lenguaje natural.
///
/// Puro y honesto: delega en `infer_parametric_anim` (reglas sin inventos) y
/// devuelve el plan como JSON con la pista humanizada para el chat (nombres
/// del mapa de controles en español — deslizador, reproducir, pausar —,
/// nunca identificadores literales). El render lo hace la UI en Rust nativo
/// tras aprobación explícita; aquí no hay E/S ni motor.
///
/// N1: si el pedido menciona integral/área va por `infer_area_anim`
/// (canónica declarada sin función, explícita con función válida, `Err`
/// honesto con función inválida).
fn propose_parametric_tool(call_id: &str, pedido: &str) -> ToolResult {
    if grafito_anim::parametric::pedido_menciona_area(pedido) {
        return propose_area_tool(call_id, pedido);
    }
    match grafito_anim::parametric::infer_parametric_anim(pedido) {
        Err(error) => ToolResult::text(call_id, false, error.to_string()),
        Ok(anim) => {
            let hint = grafito_anim::parametric::parametric_hint(&anim);
            let payload = json!({
                "kind": anim.kind.as_str(),
                "kind_label": anim.kind.en_espanol(),
                "expr_a": anim.expr_a,
                "expr_b": anim.expr_b,
                "param": anim.param.as_str(),
                "range": [anim.p0, anim.p1],
                "frames": anim.frame_count(),
                "viewport": [anim.viewport.width, anim.viewport.height],
                "hint": hint,
                "protocol_version": grafito_anim::protocol::ANIM_PROTOCOL_VERSION,
                "note": "plan paramétrico validado en Rust nativo; la vista previa se genera en la UI tras aprobación explícita"
            });
            ToolResult::text(call_id, true, payload.to_string())
        }
    }
}

/// Propuesta de integral/área (N1): canónica declarada, explícita o `Err`.
///
/// La rama canónica agrega `"canonical": true` y la prosa que la declara
/// (`INTEGRAL_CANONICAL_PROSA`): la UI renderiza `x^2` en `[0,2]` Y lo dice;
/// jamás pregunta y muestra a la vez. Puro, sin E/S ni motor.
fn propose_area_tool(call_id: &str, pedido: &str) -> ToolResult {
    match grafito_anim::parametric::infer_area_anim(pedido) {
        Err(error) => ToolResult::text(call_id, false, error.to_string()),
        Ok(resuelto) => {
            let anim = resuelto.anim();
            let mut hint = grafito_anim::parametric::parametric_hint(anim);
            if resuelto.es_canonica() {
                hint.push(' ');
                hint.push_str(grafito_anim::parametric::INTEGRAL_CANONICAL_PROSA);
            }
            let payload = json!({
                "kind": anim.kind.as_str(),
                "kind_label": anim.kind.en_espanol(),
                "expr_a": anim.expr_a,
                "expr_b": anim.expr_b,
                "param": anim.param.as_str(),
                "range": [anim.p0, anim.p1],
                "frames": anim.frame_count(),
                "viewport": [anim.viewport.width, anim.viewport.height],
                "canonical": resuelto.es_canonica(),
                "hint": hint,
                "protocol_version": grafito_anim::protocol::ANIM_PROTOCOL_VERSION,
                "note": "plan paramétrico validado en Rust nativo; la vista previa se genera en la UI tras aprobación explícita"
            });
            ToolResult::text(call_id, true, payload.to_string())
        }
    }
}

// ── Schemas pedagógicos (para exponer al LLM vía tool_catalog) ─────────────

/// Schema de `scaffold(concept, level)`.
pub fn scaffold_tool_schema() -> ToolSchema {
    ToolSchema::new(
        "scaffold",
        "Genera un andamiaje socrático puro para un concepto y nivel (pregunta, pista, explicación) sin mutar el documento.",
        json!({
            "type": "object",
            "properties": {
                "concept": {"type": "string", "description": "Concepto a andamiar, ej. derivada, integral, taylor"},
                "level": {"type": "string", "description": "Nivel pedagógico: primary, secondary, university, utn_am1, utn_am2, utn_algebra, utn_probabilidad"}
            },
            "required": ["concept"]
        }),
    )
}

/// Schema de `generate_exercise(lo_id, level, seed)`.
pub fn generate_exercise_tool_schema() -> ToolSchema {
    ToolSchema::new(
        "generate_exercise",
        "Genera un ejercicio determinista para un LearningObjective y nivel; devuelve prompt, solución y validador sin I/O.",
        json!({
            "type": "object",
            "properties": {
                "lo_id": {"type": "string", "description": "ID del objetivo de aprendizaje, ej. am1-der, sec-trig, am1-int"},
                "level": {"type": "string", "description": "Nivel pedagógico opcional"},
                "seed": {"type": "integer", "description": "Semilla opcional para variante determinista"}
            },
            "required": ["lo_id"]
        }),
    )
}

/// Schema de `assess_answer(exercise_id?, answer)`.
pub fn assess_answer_tool_schema() -> ToolSchema {
    ToolSchema::new(
        "assess_answer",
        "Evalúa una respuesta del estudiante contra un ejercicio (por lo_id/exercise_id) y devuelve feedback formativo con misconception.",
        json!({
            "type": "object",
            "properties": {
                "exercise_id": {"type": "string", "description": "ID del LO del ejercicio evaluado, ej. am1-der (alias lo_id)"},
                "lo_id": {"type": "string", "description": "Alias de exercise_id"},
                "answer": {"type": "string", "description": "Respuesta del estudiante"},
                "level": {"type": "string", "description": "Nivel opcional para regenerar el ejercicio"},
                "seed": {"type": "integer", "description": "Semilla opcional usada al generar el ejercicio"}
            },
            "required": ["answer"]
        }),
    )
}

/// Schema de `get_curriculum(query)`.
pub fn get_curriculum_tool_schema() -> ToolSchema {
    ToolSchema::new(
        "get_curriculum",
        "Busca objetivos de aprendizaje del currículum que matchean un concepto (título, descripción, tags); devuelve hasta 5.",
        json!({
            "type": "object",
            "properties": {
                "query": {"type": "string", "description": "Texto a buscar, ej. derivada, taylor, integral"},
                "concept": {"type": "string", "description": "Alias de query"}
            },
            "required": ["query"]
        }),
    )
}

/// Schema de `suggest_next`.
pub fn suggest_next_tool_schema() -> ToolSchema {
    ToolSchema::new(
        "suggest_next",
        "Sugiere el siguiente objetivo de aprendizaje usando el perfil pedagógico (mock puro en el agente; en la app usa StudentProfile::recommend_next persistido).",
        json!({
            "type": "object",
            "properties": {},
            "required": []
        }),
    )
}

/// Schema de `generate_animation(template, concept, params, pedido)`.
///
/// `canvas`/`width`/`height` son opcionales; si vienen del LLM se usan con validación
/// 64..=4096, con fallback a 640x480.
///
/// `pedido` (nuevo, AS4): texto libre que se infiere a plan paramétrico 100%
/// Rust sin Python (ej. «barrido de f(x)=x^2+p·x con p en [-2,2]»). Tiene
/// precedencia sobre template/concept; si falta algo, el error dice qué
/// falta. El tamaño va dentro del pedido («en 320x240»); canvas/width/height
/// se ignoran en la vía pedido.
pub fn generate_animation_tool_schema() -> ToolSchema {
    ToolSchema::new(
        "generate_animation",
        "Valida y propone una solicitud de animación didáctica (template, concept, params) sin ejecutar el motor; usa protocolo AnimRequest. Con 'pedido' en lenguaje natural propone un plan paramétrico 100% Rust (barrido, traza, transición, lugar, tangente o área móvil) sin Python.",
        json!({
            "type": "object",
            "properties": {
                "template": {"type": "string", "description": "Plantilla opcional: derivative-slope, integral-area, taylor-series, conformal-map, pitagoras, auto"},
                "concept": {"type": "string", "description": "Concepto en lenguaje natural, ej. derivada como pendiente"},
                "params": {"type": "object", "description": "Mapa opcional de parámetros numéricos finitos", "additionalProperties": {"type": "number"}},
                "pedido": {"type": "string", "description": "Pedido libre para plan paramétrico, ej. barrido de f(x)=x^2+p·x con p en [-2,2] (tiene precedencia; el tamaño puede ir dentro, ej. en 320x240)"},
                "canvas": {"type": "array", "description": "Resolución opcional [width, height] 64..4096 (solo vía template/concept)", "items": {"type": "integer"}, "minItems": 2, "maxItems": 2},
                "width": {"type": "integer", "description": "Ancho opcional 64..4096 (fallback 640; solo vía template/concept)"},
                "height": {"type": "integer", "description": "Alto opcional 64..4096 (fallback 480; solo vía template/concept)"}
            },
            "required": []
        }),
    )
}

/// Todas las tools pedagógicas F3.2 para exponer al LLM vía OpenCode Go.
pub fn pedagogy_tool_schemas() -> Vec<ToolSchema> {
    vec![
        scaffold_tool_schema(),
        generate_exercise_tool_schema(),
        assess_answer_tool_schema(),
        get_curriculum_tool_schema(),
        suggest_next_tool_schema(),
        generate_animation_tool_schema(),
    ]
}

/// Conjunto completo seguro (base + pedagógicas) para el loop del agente.
pub fn all_safe_tool_schemas() -> Vec<ToolSchema> {
    let mut schemas = vec![
        ToolSchema::new(
            "evaluate_expr",
            "Evalúa una expresión matemática con variables opcionales; devuelve un número finito o un error de dominio.",
            json!({
                "type": "object",
                "properties": {
                    "expression": {"type": "string"},
                    "variables": {"type": "object", "additionalProperties": {"type": "number"}}
                },
                "required": ["expression"]
            }),
        ),
        ToolSchema::new(
            "grafito_docs",
            "Devuelve el catálogo acotado de comandos verificados de Grafito que coinciden con una consulta.",
            json!({
                "type": "object",
                "properties": {"query": {"type": "string"}},
                "required": ["query"]
            }),
        ),
        ToolSchema::new(
            "ask_user",
            "Hace una única pregunta corta de aclaración matemática al usuario cuando falta un valor obligatorio. La UI la muestra como botones; nunca se ejecuta en silencio.",
            json!({
                "type": "object",
                "properties": {
                    "question": {"type": "string"},
                    "options": {"type": "array", "items": {"type": "string"}}
                },
                "required": ["question"]
            }),
        )
        .with_consent(true),
    ];
    schemas.extend(pedagogy_tool_schemas());
    schemas
}

/// Construye el payload OpenAI con la lista de herramientas del agente.
fn build_agent_payload(
    settings: &ProviderSettings,
    messages: &[Value],
    tools: &[ToolSchema],
    max_output_tokens: usize,
) -> Result<Value, String> {
    settings.validate()?;
    let mut tool_json = Vec::with_capacity(tools.len());
    for tool in tools {
        tool_json.push(tool.openai_tool()?);
    }
    Ok(json!({
        "model": settings.model,
        "stream": false,
        "max_tokens": max_output_tokens,
        "messages": messages,
        "tools": tool_json,
    }))
}

/// Error HTTP del transporte agente con freno de cuota.
///
/// Ante 429 registra la pausa global (con `Retry-After` si vino) para que
/// los reintentos automáticos del loop Chat (`complete_with_retries` en
/// `grafito-agent/loop_engine.rs`: 2 reintentos con 400/800ms, fuera de este
/// archivo) fallen honesto sin red en vez de martillar la cuota. El formato
/// conserva el prefijo `assistant agent returned HTTP {status}` + sufijo de
/// espera sólo con header legible (sin header queda byte-idéntico al
/// anterior, y los tests de detección siguen pasando).
#[cfg(feature = "assistant-net")]
fn agent_http_status_error(response: reqwest::blocking::Response) -> String {
    let status = response.status().as_u16();
    if status == 429 {
        let retry_after = crate::retry_after_secs_from_headers(response.headers());
        crate::record_rate_limited(retry_after);
        if let Some(secs) = retry_after {
            return format!("assistant agent returned HTTP 429 (reintentá en {secs}s)");
        }
    }
    format!("assistant agent returned HTTP {status}")
}

/// Envía una petición agéntica y devuelve texto final o llamadas de herramienta.
///
/// Sin `assistant-net`: stub honesto que siempre retorna `Err(NoNetwork)`.
#[cfg(feature = "assistant-net")]
fn request_agent_completion(
    settings: &ProviderSettings,
    api_key: Option<&str>,
    messages: &[Value],
    tools: &[ToolSchema],
    max_output_tokens: usize,
    timeout: Duration,
    cancellation: &Cancellation,
) -> Result<AgentChatResponse, String> {
    if cancellation.is_cancelled() {
        return Err("assistant agent request was cancelled".into());
    }
    // Muse Spark no responde por Chat Completions (500 instantáneo verificado
    // contra el servidor real): se atiende por Responses API en un solo turno;
    // el loop exterior (`run_agent`) o `run_responses_agent_loop` re-postea con
    // los `function_call_output` hasta converger. El resto de modelos usa el
    // path Chat Completions intacto de abajo.
    if uses_responses_agent_transport(settings) {
        return request_responses_agent_turn(
            settings,
            api_key,
            messages,
            tools,
            max_output_tokens,
            timeout,
            cancellation,
        );
    }
    if crate::remote_protocol(settings) != crate::RemoteProtocol::OpenAiChatCompletions {
        return Err("assistant agent requires an OpenAI-compatible chat endpoint (Muse Spark usa Responses API: el modo agente con herramientas aún no está soportado, usá el chat simple o deepseek)".into());
    }
    let payload = build_agent_payload(settings, messages, tools, max_output_tokens)?;
    // Freno 429 compartido: en pausa se falla honesto sin red (cubre también
    // los reintentos automáticos de `complete_with_retries` del loop Chat).
    if crate::check_rate_limit_cooldown().is_err() {
        return Err(crate::rate_limit_paused_error());
    }
    let client = crate::shared_http_client()?;
    let mut call = client
        .post(crate::chat_completion_endpoint(settings)?)
        .json(&payload)
        .timeout(timeout);
    if let Some(key) = api_key {
        call = call.bearer_auth(crate::sanitize_api_key(key)?);
    }
    if cancellation.is_cancelled() {
        return Err("assistant agent request was cancelled".into());
    }
    let response = call
        .send()
        .map_err(|error| crate::transport_error("assistant agent", &error, Some(timeout)))?;
    if cancellation.is_cancelled() {
        return Err("assistant agent request was cancelled".into());
    }
    if !response.status().is_success() {
        return Err(agent_http_status_error(response));
    }
    let response_bytes = crate::read_bounded_response_body(response, MAX_AGENT_RESPONSE_BYTES)?;
    let body: Value = serde_json::from_slice(&response_bytes)
        .map_err(|_| "assistant agent response JSON is invalid".to_string())?;
    parse_agent_completion(&body)
}

/// Stub sin red: conserva la firma para `RemoteAgentCompleter`, falla honesto.
#[cfg(not(feature = "assistant-net"))]
fn request_agent_completion(
    _settings: &ProviderSettings,
    _api_key: Option<&str>,
    _messages: &[Value],
    _tools: &[ToolSchema],
    _max_output_tokens: usize,
    _timeout: Duration,
    _cancellation: &Cancellation,
) -> Result<AgentChatResponse, String> {
    Err(crate::NO_NETWORK_MESSAGE.into())
}

/// Parsea la primera elección de un completion agéntico (texto o tool_calls).
fn parse_agent_completion(body: &Value) -> Result<AgentChatResponse, String> {
    let choices = body
        .get("choices")
        .and_then(Value::as_array)
        .ok_or_else(|| "assistant agent response has no choices array".to_string())?;
    let choice = choices
        .first()
        .and_then(Value::as_object)
        .ok_or_else(|| "assistant agent response has no first choice".to_string())?;
    let message = choice
        .get("message")
        .and_then(Value::as_object)
        .ok_or_else(|| "assistant agent response has no assistant message".to_string())?;
    if message.get("role").and_then(Value::as_str) != Some("assistant") {
        return Err("assistant agent response message is not an assistant message".into());
    }
    let calls = grafito_agent::schema::parse_tool_calls(&Value::Object(message.clone()));
    if let Ok(parsed) = calls {
        if !parsed.is_empty() {
            return Ok(AgentChatResponse::ToolCalls { calls: parsed });
        }
    }
    let content = message
        .get("content")
        .ok_or_else(|| "assistant agent response has no text content".to_string())?;
    let text = match content {
        Value::String(text) => Ok(text.clone()),
        Value::Array(blocks) => {
            let mut text = String::new();
            for block in blocks {
                let block = block.as_object().ok_or_else(|| {
                    "assistant agent response text block is malformed".to_string()
                })?;
                if block.get("type").and_then(Value::as_str) != Some("text") {
                    return Err("assistant agent response contains non-text blocks".into());
                }
                let block_text = block
                    .get("text")
                    .and_then(Value::as_str)
                    .ok_or_else(|| "assistant agent response text block has no text".to_string())?;
                text.push_str(block_text);
            }
            if text.is_empty() {
                return Err("assistant agent response text block is empty".into());
            }
            Ok(text)
        }
        _ => Err("assistant agent response content is not displayable text".to_string()),
    }?;
    let truncated = choice
        .get("finish_reason")
        .and_then(Value::as_str)
        .is_some_and(|reason| reason == "length" || reason == "max_tokens");
    if text.trim().is_empty() {
        return Err("assistant agent response is empty".into());
    }
    Ok(AgentChatResponse::Text {
        content: text,
        truncated,
    })
}

// ── Transporte Responses API (Muse Spark) ────────────────────────────────────
//
// Muse Spark sólo responde por `POST {base}/responses` (por Chat Completions
// el proveedor devuelve 500 instantáneo con cualquier payload, verificado
// 2026-09-04 contra el servidor real). El formato verificado es:
// request  `{model, instructions, input, tools:[{type:function,name,
//            description,parameters}], max_output_tokens}`
// response `{output:[{type:message,content:[{type:output_text,text}]},
//            {type:function_call,call_id,name,arguments}]}`.
// Cada `function_call` se despacha con el dispatcher existente de este archivo
// (`dispatch_safe_tool` vía `ToolDispatcher`, sin inventar tools) y su
// resultado vuelve como `{"type":"function_call_output","call_id","output"}`
// que se agrega al `input` antes de re-postear.

/// Cap del cuerpo en Responses API: los items `reasoning` traen
/// `encrypted_content` opaco que supera el presupuesto de texto útil.
/// 256 KiB transitorios por request (duplica `RESPONSES_MAX_BODY_BYTES` de
/// `crate::` que es privado; el texto útil sigue acotado por el budget).
const MAX_RESPONSES_BODY_BYTES: usize = 256 * 1024;

/// Tope de argumentos serializados por `function_call`.
/// Duplica `MAX_TOOL_RESULT_CHARS` (2048) de `grafito-agent::schema`.
const MAX_RESPONSES_ARGS_CHARS: usize = 2_048;

/// Resumen de args para `AgentEvent::ToolStarted` (paridad con el loop_engine).
const MAX_RESPONSES_ARGS_SUMMARY_CHARS: usize = 160;

/// Tope acumulado de caracteres del `input` Responses entre turnos.
///
/// Paridad con `AgentBudget::default().max_total_chars = 48_000` que el loop
/// Chat (`grafito-agent::loop_engine`) ya enforcea por iteración: el loop
/// Responses acumulaba `function_call` + `function_call_output` sin cota
/// (solo `max_tool_turns`), así que una tool verborrágica podía inflar el
/// payload turno a turno. Fail-closed con el mismo mensaje que el loop Chat.
const MAX_RESPONSES_INPUT_CHARS: usize = 48_000;

/// Peor caso de red por turno del modo agente (documentado y testeado).
///
/// - Responses (`run_responses_agent_loop`, acá): un POST por turno y ningún
///   reintento interno → `max_tool_turns + 1 = 5` con el budget default.
///   Ante 429 el loop corta en el primer POST (propaga `Err`): quema 1.
/// - Chat (`run_agent` en `grafito-agent/loop_engine.rs`): cada turno pasa
///   por `complete_with_retries` (`max_retries = 2`) → `(4+1) × (1+2) = 15`
///   POST en el peor caso SIN pausa previa. Con la pausa global de `crate::`
///   el primer 429 la arma y los reintentos/turnos siguientes fallan honesto
///   sin red (1 POST de descubrimiento por turno como máximo, 0 si ya había
///   pausa). Los valores se verifican contra `AgentBudget::default` en tests
///   (sólo lectura: los budgets no se tocan).
pub const MAX_AGENT_RESPONSES_HTTP_REQUESTS_PER_TURN: usize = 5;
pub const MAX_AGENT_CHAT_HTTP_REQUESTS_PER_TURN: usize = 15;

/// Suma el tamaño serializado del `input` Responses (paridad con
/// `message_chars` de `loop_engine`: `Value::to_string().len()`).
fn responses_input_chars(input: &[Value]) -> usize {
    input
        .iter()
        .map(|item| item.to_string().len())
        .fold(0_usize, |acc, len| acc.saturating_add(len))
}

/// ¿Este modelo viaja por Responses API en vez de Chat Completions?
///
/// Duplica la lógica mínima de `uses_responses_api` de `crate::` (privado en
/// la raíz: `model.contains("muse-spark")`, que cubre futuras 1.x). No se toca
/// `lib.rs`; el router Chat queda intacto para el resto de modelos.
fn uses_responses_agent_transport(settings: &ProviderSettings) -> bool {
    settings.model.contains("muse-spark")
}

/// Convierte los schemas al formato de tools de Responses API.
///
/// `ToolSchema::openai_tool()` emite `{type:function,function:{...}}` (Chat);
/// Responses espera plano `{type:function,name,description,parameters}`.
fn responses_agent_tools(tools: &[ToolSchema]) -> Result<Vec<Value>, String> {
    let mut rendered = Vec::with_capacity(tools.len());
    for tool in tools {
        tool.validate()?;
        rendered.push(json!({
            "type": "function",
            "name": tool.name,
            "description": tool.description,
            "parameters": tool.parameters,
        }));
    }
    Ok(rendered)
}

/// Presupuesto `max_output_tokens` para Responses: incluye razonamiento
/// (verificado: un "ok" consume 61 reasoning + 0 output). Duplica la lógica
/// mínima de `responses_token_limit_for_chars` de `crate::` (privado).
fn responses_agent_token_budget(max_output_chars: usize) -> usize {
    max_output_chars.saturating_mul(2).clamp(2_048, 16_384)
}

/// Construye el payload Responses del agente con instructions+input+tools.
///
/// No se reutiliza `crate::build_responses_payload` (público pero ata a
/// `AssistantRequest` y no admite tools); se duplica lo mínimo sin tocar
/// `lib.rs`. Sin secretos: sólo `model`, texto e `input` ya saneados.
fn build_responses_agent_payload(
    settings: &ProviderSettings,
    instructions: &str,
    input: &[Value],
    tools: &[ToolSchema],
    max_output_tokens: usize,
) -> Result<Value, String> {
    settings.validate()?;
    Ok(json!({
        "model": settings.model,
        "instructions": instructions,
        "input": input,
        "tools": responses_agent_tools(tools)?,
        "max_output_tokens": max_output_tokens.max(16),
    }))
}

/// Separa `instructions` (system) del `input` Responses.
///
/// Traduce el historial Chat del loop exterior al formato Responses:
/// - `system` (string) → `instructions` (concatenado con `\n\n`).
/// - `assistant` con `tool_calls` (Chat) → items `function_call`.
/// - `tool` con `tool_call_id` → items `function_call_output`.
/// - el resto (`user`, `assistant` de texto) se clona tal cual.
fn split_responses_instructions_and_input(messages: &[Value]) -> (String, Vec<Value>) {
    let mut instructions = Vec::new();
    let mut input = Vec::with_capacity(messages.len());
    for message in messages {
        let role = message.get("role").and_then(Value::as_str).unwrap_or("");
        if role == "system" {
            if let Some(text) = message.get("content").and_then(Value::as_str) {
                if !text.trim().is_empty() {
                    instructions.push(text.to_owned());
                }
            }
            continue;
        }
        if role == "assistant" {
            if let Some(calls) = message.get("tool_calls").and_then(Value::as_array) {
                for call in calls {
                    let id = call
                        .get("id")
                        .and_then(Value::as_str)
                        .or_else(|| call.get("call_id").and_then(Value::as_str))
                        .unwrap_or_default();
                    let (name, arguments) = call
                        .get("function")
                        .and_then(Value::as_object)
                        .map(|function| {
                            (
                                function
                                    .get("name")
                                    .and_then(Value::as_str)
                                    .unwrap_or_default(),
                                function
                                    .get("arguments")
                                    .and_then(Value::as_str)
                                    .map(str::to_owned)
                                    .unwrap_or_else(|| "{}".to_string()),
                            )
                        })
                        .unwrap_or_default();
                    input.push(json!({
                        "type": "function_call",
                        "call_id": id,
                        "name": name,
                        "arguments": arguments,
                    }));
                }
                if let Some(text) = message.get("content").and_then(Value::as_str) {
                    if !text.trim().is_empty() {
                        input.push(json!({"role": "assistant", "content": text}));
                    }
                }
                continue;
            }
        }
        if role == "tool" {
            let id = message
                .get("tool_call_id")
                .and_then(Value::as_str)
                .or_else(|| message.get("call_id").and_then(Value::as_str))
                .or_else(|| message.get("id").and_then(Value::as_str))
                .unwrap_or_default();
            let output = match message.get("content") {
                Some(Value::String(text)) => text.clone(),
                Some(other) if other.is_null() => String::new(),
                Some(other) => other.to_string(),
                None => String::new(),
            };
            input.push(json!({
                "type": "function_call_output",
                "call_id": id,
                "output": output,
            }));
            continue;
        }
        input.push(message.clone());
    }
    (instructions.join("\n\n"), input)
}

/// Parsea un `function_call` de Responses a `ToolCall` del dispatcher.
///
/// `arguments` llega como string JSON (o ya objeto); se acota a
/// `MAX_RESPONSES_ARGS_CHARS` como el parser Chat.
fn parse_responses_tool_call(item: &Value, index: usize) -> Result<ToolCall, String> {
    let object = item
        .as_object()
        .ok_or_else(|| format!("assistant agent responses call {index} is not an object"))?;
    let id = object
        .get("call_id")
        .and_then(Value::as_str)
        .or_else(|| object.get("id").and_then(Value::as_str))
        .unwrap_or_default()
        .to_owned();
    let name = object
        .get("name")
        .and_then(Value::as_str)
        .unwrap_or_default()
        .trim()
        .to_owned();
    if name.is_empty() || name.chars().count() > 64 {
        return Err(format!(
            "assistant agent responses call {index} has an invalid name"
        ));
    }
    let raw_arguments = match object.get("arguments") {
        None | Some(Value::Null) => "{}".to_string(),
        Some(Value::String(text)) => text.clone(),
        Some(other) => other.to_string(),
    };
    if raw_arguments.chars().count() > MAX_RESPONSES_ARGS_CHARS {
        return Err(format!(
            "assistant agent responses call {index} arguments exceed the budget"
        ));
    }
    let arguments: Value = serde_json::from_str(&raw_arguments).map_err(|error| {
        format!("assistant agent responses call {index} arguments JSON is invalid: {error}")
    })?;
    Ok(ToolCall {
        id,
        name,
        arguments,
    })
}

/// Parsea el `output` de Responses: acumula `output_text` y extrae calls.
///
/// Duplica lo mínimo de `responses_completion_text` de `crate::` (privado:
/// junta `output_text` de items `message`, honra `error`/`failed`/`cancelled`
/// e `incomplete` como truncado) y además extrae los items `function_call`.
/// Los items `reasoning` se ignoran (no se muestran).
/// Si hay texto y calls a la vez se devuelven los calls (el texto final llega
/// en el turno de convergencia tras ejecutar las tools).
fn parse_responses_agent_turn(body: &Value) -> Result<AgentChatResponse, String> {
    if let Some(error) = body.get("error") {
        if !error.is_null() {
            let detail = error
                .get("message")
                .and_then(Value::as_str)
                .unwrap_or("unknown provider error");
            let detail: String = detail.chars().take(200).collect();
            return Err(format!(
                "assistant agent responses API returned an error: {detail}"
            ));
        }
    }
    let status = body.get("status").and_then(Value::as_str).unwrap_or("");
    if status == "failed" || status == "cancelled" {
        return Err("assistant agent responses API did not complete the response".into());
    }
    let truncated = status == "incomplete";
    let output = body
        .get("output")
        .and_then(Value::as_array)
        .ok_or_else(|| "assistant agent responses response has no output array".to_string())?;
    let mut texts = Vec::new();
    let mut calls = Vec::new();
    for (index, item) in output.iter().enumerate() {
        match item.get("type").and_then(Value::as_str) {
            Some("message") => {
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
            Some("function_call") => calls.push(parse_responses_tool_call(item, index)?),
            _ => {}
        }
    }
    if !calls.is_empty() {
        return Ok(AgentChatResponse::ToolCalls { calls });
    }
    let text = texts.join("\n\n");
    if text.trim().is_empty() && !truncated {
        return Err("assistant agent responses response contained no displayable text".into());
    }
    Ok(AgentChatResponse::Text {
        content: text,
        truncated,
    })
}

/// POST crudo a Responses API con bound de cuerpo y JSON parseado.
///
/// Reusa `crate::responses_endpoint` (público), `crate::sanitize_api_key` y
/// `crate::transport_error` (`pub(crate)`), más el cliente compartido y la
/// lectura acotada ya usados por el path Chat de este archivo.
///
/// Sin `assistant-net`: stub honesto que siempre retorna `Err(NoNetwork)`.
#[cfg(feature = "assistant-net")]
fn post_responses_output(
    settings: &ProviderSettings,
    api_key: Option<&str>,
    payload: &Value,
    timeout: Duration,
    cancellation: &Cancellation,
) -> Result<Value, String> {
    if cancellation.is_cancelled() {
        return Err("assistant agent request was cancelled".into());
    }
    let client = crate::shared_http_client()?;
    let mut call = client
        .post(crate::responses_endpoint(settings)?)
        .json(payload)
        .timeout(timeout);
    if crate::check_rate_limit_cooldown().is_err() {
        return Err(crate::rate_limit_paused_error());
    }
    if let Some(key) = api_key {
        call = call.bearer_auth(crate::sanitize_api_key(key)?);
    }
    if cancellation.is_cancelled() {
        return Err("assistant agent request was cancelled".into());
    }
    let response = call
        .send()
        .map_err(|error| crate::transport_error("assistant agent", &error, Some(timeout)))?;
    if cancellation.is_cancelled() {
        return Err("assistant agent request was cancelled".into());
    }
    if !response.status().is_success() {
        return Err(agent_http_status_error(response));
    }
    let response_bytes = crate::read_bounded_response_body(response, MAX_RESPONSES_BODY_BYTES)?;
    serde_json::from_slice(&response_bytes)
        .map_err(|_| "assistant agent responses response JSON is invalid".to_string())
}

/// Stub sin red: conserva la firma para `request_responses_agent_turn`, falla honesto.
#[cfg(not(feature = "assistant-net"))]
fn post_responses_output(
    _settings: &ProviderSettings,
    _api_key: Option<&str>,
    _payload: &Value,
    _timeout: Duration,
    _cancellation: &Cancellation,
) -> Result<Value, String> {
    Err(crate::NO_NETWORK_MESSAGE.into())
}

/// Un turno Responses: traduce los mensajes Chat del loop a
/// instructions+input, postea una vez y devuelve texto o tool calls.
///
/// Lo usa `RemoteAgentCompleter` para Spark dentro del loop exterior
/// (`run_agent`, que ya respeta `AgentBudget` y cancelación turno a turno).
#[allow(clippy::too_many_arguments)]
fn request_responses_agent_turn(
    settings: &ProviderSettings,
    api_key: Option<&str>,
    messages: &[Value],
    tools: &[ToolSchema],
    max_output_tokens: usize,
    timeout: Duration,
    cancellation: &Cancellation,
) -> Result<AgentChatResponse, String> {
    let (instructions, input) = split_responses_instructions_and_input(messages);
    let payload =
        build_responses_agent_payload(settings, &instructions, &input, tools, max_output_tokens)?;
    let body = post_responses_output(settings, api_key, &payload, timeout, cancellation)?;
    parse_responses_agent_turn(&body)
}

fn summarize_responses_args(arguments: &Value) -> String {
    let summary = arguments.to_string();
    if summary.chars().count() > MAX_RESPONSES_ARGS_SUMMARY_CHARS {
        let mut clipped = summary
            .chars()
            .take(MAX_RESPONSES_ARGS_SUMMARY_CHARS.saturating_sub(1))
            .collect::<String>();
        clipped.push('…');
        clipped
    } else {
        summary
    }
}

/// Loop agente nativo por Responses API para Muse Spark.
///
/// Replica el contrato de `run_agent` con formato Responses en vez de Chat:
/// POST → acumula `output_text` de items `message`; por cada item
/// `function_call` despacha la tool existente (`ToolDispatcher`, en producción
/// `SafeGrafitoDispatcher`) y agrega
/// `{"type":"function_call_output","call_id","output"}` al `input` (junto al
/// `function_call` que lo originó, para contexto stateless) antes de
/// re-postear. Respeta `AgentBudget` (`max_tool_turns`, `per_turn_timeout`,
/// `total_span`) y `Cancellation` en cada iteración.
#[allow(clippy::too_many_arguments)]
fn run_responses_agent_loop<D: ToolDispatcher>(
    settings: &ProviderSettings,
    api_key: Option<&str>,
    system: &str,
    user_messages: &[Value],
    tools: &[ToolSchema],
    budget: &AgentBudget,
    ledger: Option<&JSpaceLedger>,
    dispatcher: &D,
    cancellation: &Cancellation,
    mut on_event: impl FnMut(AgentEvent),
) -> Result<AgentOutcome, String> {
    for tool in tools {
        tool.validate()?;
    }
    let mut instructions_owned = system.to_owned();
    if let Some(ledger) = ledger {
        ledger.validate()?;
        let render = ledger.render_bounded(MAX_LEDGER_RENDER_BYTES);
        if !render.trim().is_empty() {
            on_event(AgentEvent::Ledger {
                render: render.clone(),
            });
            instructions_owned.push_str("\n\nLedger de tarea:\n");
            instructions_owned.push_str(&render);
        }
    }
    let (extra_instructions, mut input) = split_responses_instructions_and_input(user_messages);
    if !extra_instructions.trim().is_empty() {
        if !instructions_owned.trim().is_empty() {
            instructions_owned.push_str("\n\n");
        }
        instructions_owned.push_str(&extra_instructions);
    }
    let started = Instant::now();
    let max_turns = budget.max_tool_turns.max(1);
    let mut accumulated_chars =
        responses_input_chars(&input).saturating_add(instructions_owned.len());
    for turn in 0..=max_turns {
        if cancellation.is_cancelled() {
            return Err("assistant agent request was cancelled".into());
        }
        let remaining = budget.total_span.saturating_sub(started.elapsed());
        if remaining.is_zero() {
            return Err("assistant agent loop exceeded its total span".into());
        }
        if accumulated_chars > MAX_RESPONSES_INPUT_CHARS {
            return Err("assistant agent loop exceeded its total char budget".into());
        }
        let per_turn_timeout = budget.per_turn_timeout.min(remaining);
        let payload = build_responses_agent_payload(
            settings,
            &instructions_owned,
            &input,
            tools,
            responses_agent_token_budget(budget.max_output_chars),
        )?;
        let body =
            post_responses_output(settings, api_key, &payload, per_turn_timeout, cancellation)?;
        match parse_responses_agent_turn(&body)? {
            AgentChatResponse::Text { content, truncated } => {
                on_event(AgentEvent::Finalized {
                    text: content.clone(),
                });
                let lower = content.to_ascii_lowercase();
                let verified = !lower.contains("pendiente")
                    && !lower.contains("sin verificar")
                    && !lower.contains("no pude");
                return Ok(AgentOutcome {
                    final_text: content,
                    truncated,
                    tool_turns: turn,
                    verified,
                });
            }
            AgentChatResponse::ToolCalls { calls } => {
                if calls.is_empty() {
                    return Err("assistant agent received an empty tool_calls list".into());
                }
                if turn == max_turns {
                    return Err("assistant agent exceeded its tool-call turn budget".into());
                }
                // Eco stateless: el servidor no recuerda turnos, el `input`
                // acumula los `function_call` y sus `function_call_output`.
                for call in &calls {
                    let arguments =
                        serde_json::to_string(&call.arguments).unwrap_or_else(|_| "{}".to_string());
                    let item = json!({
                        "type": "function_call",
                        "call_id": call.id,
                        "name": call.name,
                        "arguments": arguments,
                    });
                    accumulated_chars = accumulated_chars.saturating_add(item.to_string().len());
                    input.push(item);
                }
                for call in &calls {
                    on_event(AgentEvent::ToolStarted {
                        name: call.name.clone(),
                        args_summary: summarize_responses_args(&call.arguments),
                    });
                    if cancellation.is_cancelled() {
                        return Err("assistant agent request was cancelled".into());
                    }
                    let result = dispatcher.dispatch(call);
                    on_event(AgentEvent::ToolFinished {
                        name: call.name.clone(),
                        ok: result.ok,
                    });
                    let item = json!({
                        "type": "function_call_output",
                        "call_id": result.call_id,
                        "output": result.content,
                    });
                    accumulated_chars = accumulated_chars.saturating_add(item.to_string().len());
                    input.push(item);
                }
            }
        }
    }
    Err("assistant agent loop did not converge".into())
}

// ── F10 Aula/P2P+IA: telemetría, costos, cascada, juez (S/M honestos) ───────
//
// Cerebro puro: sin I/O, sin spawn, sin red. Todo local y acotado por
// `RequestBudget` (8192 in / 2048 out / 8 pasos / 60s) y `AttachmentLimits`
// (512 KiB / 1 MiB). PII nunca sale: solo nombres de tool + conteos.
//
// - `TurnTelemetry` + `AgentTelemetry`: por turno + costos visibles.
// - `ModelCascade`: cadena primaria + fallbacks (ej. spark→deepseek).
// - `judge_telling_heuristic`: contrato `revise > block` (S) + calibración
//   `over-blocking ≤5%`. El juez LLM completo es L → `llm_judge_stub`.
// - `ocr_local_stub`: OCR manuscrito local es L → siempre `Err` honesto.

/// Tope de turnos registrados (igual que `RequestBudget::max_steps = 8`).
pub const MAX_TELEMETRY_TURNS: usize = 8;
/// Tope de modelos en cascada (primaria + 3 fallbacks).
pub const MAX_CASCADE_MODELS: usize = 4;
/// Longitud máxima de un nombre de modelo (igual que `ToolCall` name 64).
pub const MAX_MODEL_NAME_LEN: usize = 64;

/// Nombre de modelo validado: `1..=64`, ASCII alfanumérico + `-`/`_`<code>.</code>`+`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ModelName(String);

impl ModelName {
    /// Valida y construye. `Err` si vacío, largo o con caracteres no permitidos.
    pub fn try_new(raw: &str) -> Result<Self, String> {
        let trimmed = raw.trim();
        if trimmed.is_empty() {
            return Err("model name vacío".to_string());
        }
        if trimmed.len() > MAX_MODEL_NAME_LEN {
            return Err(format!("model name excede {MAX_MODEL_NAME_LEN} bytes"));
        }
        if !trimmed
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_' || c == '.' || c == '+')
        {
            return Err("model name solo admite [A-Za-z0-9-_.+]".to_string());
        }
        Ok(Self(trimmed.to_string()))
    }

    /// Vista del nombre (ya validado).
    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

/// Telemetría de un turno del agente (S): latencia + chars + tool + éxito.
///
/// Todo local: no guarda prompts, respuestas ni PII — solo conteos y el
/// nombre de la tool (ya allowlisted). `input_chars ≤ 8192`,
/// `output_chars ≤ 2048`, `latency_ms ≤ 120_000` (cap absoluto del budget).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TurnTelemetry {
    /// Índice de turno `0..=7` (igual que `max_steps`).
    pub turn: usize,
    /// Tool ejecutada en el turno (`None` si fue turno de texto final).
    pub tool_name: Option<String>,
    /// ¿La tool respondió `ok`? (texto final siempre `true`).
    pub ok: bool,
    /// Latencia del turno en ms (medida por el caller, saturante).
    pub latency_ms: u64,
    /// Chars de entrada consumidos en el turno (prompt + tools previas).
    pub input_chars: usize,
    /// Chars de salida producidos en el turno (texto o args serializados).
    pub output_chars: usize,
}

impl TurnTelemetry {
    /// Construye validando presupuestos. `Err` si excede topes.
    pub fn try_new(
        turn: usize,
        tool_name: Option<&str>,
        ok: bool,
        latency_ms: u64,
        input_chars: usize,
        output_chars: usize,
    ) -> Result<Self, String> {
        if turn >= MAX_TELEMETRY_TURNS {
            return Err(format!("turn {turn} excede {MAX_TELEMETRY_TURNS} turnos"));
        }
        if latency_ms > 120_000 {
            return Err("latency_ms excede 120000".to_string());
        }
        if input_chars > 8_192 {
            return Err("input_chars excede 8192".to_string());
        }
        if output_chars > 2_048 {
            return Err("output_chars excede 2048".to_string());
        }
        let clean_tool = match tool_name {
            None => None,
            Some(raw) => {
                let trimmed = raw.trim();
                if trimmed.is_empty() {
                    None
                } else {
                    if trimmed.len() > 64 {
                        return Err("tool_name excede 64 bytes".to_string());
                    }
                    if !trimmed
                        .chars()
                        .all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '-')
                    {
                        return Err("tool_name inválido".to_string());
                    }
                    Some(trimmed.to_string())
                }
            }
        };
        Ok(Self {
            turn,
            tool_name: clean_tool,
            ok,
            latency_ms,
            input_chars,
            output_chars,
        })
    }
}

/// Acumulador de telemetría del loop (costos visibles para la UI).
///
/// Cap `MAX_TELEMETRY_TURNS = 8`: `try_record` falla honesto si se excede
/// (el loop real nunca supera `max_tool_turns ≤ 8` por `AgentBudget`).
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct AgentTelemetry {
    turns: Vec<TurnTelemetry>,
}

impl AgentTelemetry {
    /// Acumulador vacío.
    #[must_use]
    pub fn new() -> Self {
        Self { turns: Vec::new() }
    }

    /// Registra un turno. `Err` si ya hay 8 (fail-closed, sin truncar).
    pub fn try_record(&mut self, turn: TurnTelemetry) -> Result<(), String> {
        if self.turns.len() >= MAX_TELEMETRY_TURNS {
            return Err(format!(
                "telemetría llena (máximo {MAX_TELEMETRY_TURNS} turnos)"
            ));
        }
        // Orden creciente de turnos (contrato barato para la UI).
        if let Some(last) = self.turns.last() {
            if turn.turn < last.turn {
                return Err("turn desordenado".to_string());
            }
        }
        self.turns.push(turn);
        Ok(())
    }

    /// Turnos registrados.
    #[must_use]
    pub fn turn_count(&self) -> usize {
        self.turns.len()
    }

    /// Tools ejecutadas (turnos con `tool_name`).
    #[must_use]
    pub fn tool_calls(&self) -> usize {
        self.turns.iter().filter(|t| t.tool_name.is_some()).count()
    }

    /// Tools con `ok = true`.
    #[must_use]
    pub fn tools_ok(&self) -> usize {
        self.turns
            .iter()
            .filter(|t| t.tool_name.is_some() && t.ok)
            .count()
    }

    /// Suma de `input_chars` (saturante).
    #[must_use]
    pub fn total_input_chars(&self) -> usize {
        self.turns.iter().map(|t| t.input_chars).sum()
    }

    /// Suma de `output_chars` (saturante).
    #[must_use]
    pub fn total_output_chars(&self) -> usize {
        self.turns.iter().map(|t| t.output_chars).sum()
    }

    /// Latencia total del loop en ms (saturante).
    #[must_use]
    pub fn total_latency_ms(&self) -> u64 {
        self.turns.iter().map(|t| t.latency_ms).sum()
    }

    /// ¿Supera el `RequestBudget`? (`in > 8192` o `out > 2048` o `turnos > 8`).
    #[must_use]
    pub fn is_over_budget(&self, budget: &grafito_assistant_types::RequestBudget) -> bool {
        self.total_input_chars() > budget.max_input_chars
            || self.total_output_chars() > budget.max_output_chars
            || self.turn_count() > budget.max_steps
    }

    /// Resumen visible para la UI (una línea, sin PII):
    /// `"3 turnos · 2 tools (ok 1) · 1200/8192 in · 800/2048 out · 900ms"`.
    #[must_use]
    pub fn visible_summary(&self, budget: &grafito_assistant_types::RequestBudget) -> String {
        format!(
            "{} turnos · {} tools (ok {}) · {}/{} in · {}/{} out · {}ms",
            self.turn_count(),
            self.tool_calls(),
            self.tools_ok(),
            self.total_input_chars(),
            budget.max_input_chars,
            self.total_output_chars(),
            budget.max_output_chars,
            self.total_latency_ms(),
        )
    }
}

/// Cascada de modelos (S-M): primaria + fallbacks en orden visible.
///
/// No ejecuta red: solo define el orden que la app recorre (ej.
/// `muse-spark → deepseek-v4-flash`, fallback de sesión ya existente en
/// `grafito-app/src/assistant.rs:2470-2485`). Pura y acotada a 4 modelos.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ModelCascade {
    primary: ModelName,
    fallbacks: Vec<ModelName>,
}

impl ModelCascade {
    /// Construye validando nombres y topes. `Err` si primaria vacía,
    /// duplicados o más de 3 fallbacks.
    pub fn try_new(primary: &str, fallbacks: &[&str]) -> Result<Self, String> {
        let primary_name = ModelName::try_new(primary)?;
        if fallbacks.len() > MAX_CASCADE_MODELS - 1 {
            return Err(format!("cascada excede {} modelos", MAX_CASCADE_MODELS));
        }
        let mut seen = vec![primary_name.as_str().to_string()];
        let mut clean_fallbacks = Vec::with_capacity(fallbacks.len());
        for raw in fallbacks {
            let name = ModelName::try_new(raw)?;
            if seen.iter().any(|s| s == name.as_str()) {
                return Err(format!("modelo duplicado en cascada: {}", name.as_str()));
            }
            seen.push(name.as_str().to_string());
            clean_fallbacks.push(name);
        }
        Ok(Self {
            primary: primary_name,
            fallbacks: clean_fallbacks,
        })
    }

    /// Cadena completa `[primaria, fallback...]` (para mostrar en la UI).
    #[must_use]
    pub fn chain(&self) -> Vec<String> {
        let mut out = Vec::with_capacity(1 + self.fallbacks.len());
        out.push(self.primary.as_str().to_string());
        for fallback in &self.fallbacks {
            out.push(fallback.as_str().to_string());
        }
        out
    }

    /// Siguiente modelo tras `failed`, o `None` si es el último o desconocido.
    #[must_use]
    pub fn next_after(&self, failed: &str) -> Option<String> {
        let chain = self.chain();
        let position = chain.iter().position(|name| name == failed)?;
        chain.get(position.saturating_add(1)).cloned()
    }

    /// Primaria (para el payload inicial).
    #[must_use]
    pub fn primary(&self) -> &str {
        self.primary.as_str()
    }
}

/// Acción del juez ante una respuesta (contrato `revise > block`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum JudgeAction {
    /// Respuesta aceptada tal cual.
    Allow,
    /// Telling temprano: NO se bloquea, se re-pregunta con el scaffold
    /// (repara una vez vía `repair_feedback`, igual que `enforce_telling_guard`).
    Revise,
    /// Reservado para política futura (PII, inyección): hoy nunca se emite
    /// por telling — el contrato exige `revise` antes que `block`.
    Block,
}

/// Veredicto del juez heurístico (S, calibrado abajo).
#[derive(Debug, Clone, PartialEq)]
pub struct TellingJudgeVerdict {
    /// ¿Detectó telling (`attempts<2` + marcador de solución)?
    pub is_telling: bool,
    /// Confianza `0..=1` (0.9 telling, 0.1 no-telling, 0.5 vacío).
    pub confidence: f64,
    /// Acción según contrato (`Revise` si telling, `Allow` si no).
    pub action: JudgeAction,
    /// Pista de reparación (pregunta heurística) si `Revise`.
    pub repair_hint: Option<String>,
}

/// Juez heurístico de telling (S, puro, sin LLM ni red).
///
/// - `attempts >= 2` (`can_reveal`) → `Allow` (aunque haya solución).
/// - `attempts < 2` + marcador de solución (`crate::contains_telling_markers`)
///   → `Revise` con `repair_hint` (contrato: nunca `Block` por telling).
/// - Sin marcador → `Allow` (preguntar de más también enseña mal).
/// - Texto vacío → `Allow` con confianza 0.5 (el loop pedirá aclaración).
pub fn judge_telling_heuristic(
    attempts: u8,
    response_text: &str,
    repair_hint: Option<&str>,
) -> TellingJudgeVerdict {
    if response_text.trim().is_empty() {
        return TellingJudgeVerdict {
            is_telling: false,
            confidence: 0.5,
            action: JudgeAction::Allow,
            repair_hint: None,
        };
    }
    if attempts >= 2 {
        return TellingJudgeVerdict {
            is_telling: false,
            confidence: 0.9,
            action: JudgeAction::Allow,
            repair_hint: None,
        };
    }
    if crate::contains_telling_markers(response_text) {
        return TellingJudgeVerdict {
            is_telling: true,
            confidence: 0.9,
            action: JudgeAction::Revise,
            repair_hint: repair_hint
                .map(|hint| hint.trim().to_string())
                .filter(|hint| !hint.is_empty())
                .map(|hint| hint.chars().take(500).collect()),
        };
    }
    TellingJudgeVerdict {
        is_telling: false,
        confidence: 0.9,
        action: JudgeAction::Allow,
        repair_hint: None,
    }
}

/// Tasa de over-blocking sobre fixtures `(texto, es_telling_real)`.
///
/// Fracción de NO-telling marcados como telling (`0..=1`). El contrato F10
/// exige `≤5%` (máx 1 falso positivo cada 20 turnos, igual que `telling_rate`).
/// Pura, `None` si no hay fixtures (evita división por cero).
#[must_use]
pub fn telling_overblocking_rate(fixtures: &[(&str, bool)]) -> Option<f64> {
    let mut negatives = 0_usize;
    let mut false_positives = 0_usize;
    for (text, is_real_telling) in fixtures {
        if *is_real_telling {
            continue;
        }
        negatives = negatives.saturating_add(1);
        // attempts=0: el caso más estricto (can_reveal=false).
        let verdict = judge_telling_heuristic(0, text, None);
        if verdict.is_telling {
            false_positives = false_positives.saturating_add(1);
        }
    }
    if negatives == 0 {
        return None;
    }
    Some(false_positives as f64 / negatives as f64)
}

/// Stub honesto del juez LLM completo (L): requiere modelo remoto + N≥200.
///
/// El juez calibrado con LLM auditaría `telling` ambiguo (ironía, pasos
/// parciales) con un modelo remoto y calibración EM sobre `≥200` turnos
/// etiquetados. Fuera del frente (red + dataset). Hoy: heurístico S arriba.
pub fn llm_judge_stub() -> Result<String, String> {
    Err("llm-judge no implementado: diseño F10.W5 (auditoría remota con calibración N≥200); hoy juez heurístico revise>block".to_string())
}

/// Stub honesto de OCR manuscrito local (L): requiere visión local.
///
/// Transcribiría trazos/imagen a texto editable sin red (modelo on-device).
/// Fuera del frente (peso + dataset). Hoy: el usuario edita la transcripción
/// o usa un proveedor de visión explícito (ver `lib.rs:132`).
pub fn ocr_local_stub() -> Result<String, String> {
    Err("ocr-local no implementado: diseño F10.W5 (visión on-device acotada a 1MiP); hoy transcripción editable o visión remota explícita".to_string())
}

// ── G-G Vibecoder: error de compilador → negocio + 2-3 botones (S) ─────────
//
// `/vibecoder-guide`: cada error técnico se explica en lenguaje de negocio y
// se ofrece un menú de 2-3 acciones (nunca un muro de texto ni un solo
// "reintentar"). Cerebro puro: sin I/O, sin red, todo acotado. La Piel
// renderiza `options` como botones; el agente nunca ejecuta la acción solo.

/// Mínimo/máximo de botones por error (la Piel muestra 2-3, nunca 1 ni 4+).
pub const MIN_VIBE_OPTIONS: usize = 2;
/// Máximo de botones.
pub const MAX_VIBE_OPTIONS: usize = 3;
/// Tope del título de negocio (una línea).
pub const MAX_VIBE_TITLE_CHARS: usize = 80;
/// Tope de la explicación de negocio (dos líneas).
pub const MAX_VIBE_EXPLANATION_CHARS: usize = 500;
/// Tope por etiqueta de botón.
pub const MAX_VIBE_LABEL_CHARS: usize = 32;
/// Tope por hint de botón.
pub const MAX_VIBE_HINT_CHARS: usize = 200;

/// Clase del error técnico (para elegir explicación y botones).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum VibecoderKind {
    /// Tipos que no coinciden (`mismatched types`, `expected X found Y`).
    TypeMismatch,
    /// Símbolo mal puesto (`syntax`, `parse`, `unexpected`, paréntesis).
    Syntax,
    /// Sin resultado en el dominio (`non-finite`, `NaN`, división por cero).
    Domain,
    /// Tardó demasiado (`timed out`, `deadline`, presupuesto excedido).
    Timeout,
    /// Sin red (`NoNetwork`, `assistant-net`, `connection`).
    Network,
    /// Resto (mensaje crudo truncado como contexto, sin PII).
    Unknown,
}

/// Clasifica un mensaje técnico en `VibecoderKind` (case-insensitive, puro).
#[must_use]
pub fn vibecoder_classify(compiler_message: &str) -> VibecoderKind {
    let lower = compiler_message.to_lowercase();
    if lower.contains("mismatched")
        || lower.contains("mismatch")
        || (lower.contains("expected") && lower.contains("found"))
        || lower.contains("tipo")
        || lower.contains("type")
            && (lower.contains("bool") || lower.contains("string") || lower.contains("number"))
    {
        VibecoderKind::TypeMismatch
    } else if lower.contains("syntax")
        || lower.contains("parse")
        || lower.contains("unexpected")
        || lower.contains("parenthes")
        || lower.contains("paréntesis")
        || lower.contains("sintaxis")
    {
        VibecoderKind::Syntax
    } else if lower.contains("non-finite")
        || lower.contains("non finite")
        || lower.contains("nan")
        || lower.contains("infinite")
        || lower.contains("infinito")
        || lower.contains("division")
        || lower.contains("división")
        || lower.contains("dominio")
        || lower.contains("domain")
    {
        VibecoderKind::Domain
    } else if lower.contains("timed out")
        || lower.contains("timeout")
        || lower.contains("deadline")
        || lower.contains("exceeded")
        || lower.contains("tardó")
        || lower.contains("presupuesto")
    {
        VibecoderKind::Timeout
    } else if lower.contains("nonetwork")
        || lower.contains("no_network")
        || lower.contains("assistant-net")
        || lower.contains("connection")
        || lower.contains("conexión")
        || lower.contains("sin red")
    {
        VibecoderKind::Network
    } else {
        VibecoderKind::Unknown
    }
}

/// Un botón del menú de reparación (etiqueta + hint, ambos acotados).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct VibecoderOption {
    /// Texto del botón (`1..=32` chars).
    pub label: String,
    /// Qué hace el botón (`1..=200` chars, sin ejecutar nada).
    pub hint: String,
}

impl VibecoderOption {
    /// Valida y construye. `Err` si etiqueta/hint vacíos o largos.
    pub fn try_new(label: &str, hint: &str) -> Result<Self, String> {
        let clean_label = label.trim();
        if clean_label.is_empty() {
            return Err("botón sin etiqueta".to_string());
        }
        if clean_label.chars().count() > MAX_VIBE_LABEL_CHARS {
            return Err(format!("etiqueta excede {MAX_VIBE_LABEL_CHARS} chars"));
        }
        let clean_hint = hint.trim();
        if clean_hint.is_empty() {
            return Err("botón sin hint".to_string());
        }
        if clean_hint.chars().count() > MAX_VIBE_HINT_CHARS {
            return Err(format!("hint excede {MAX_VIBE_HINT_CHARS} chars"));
        }
        Ok(Self {
            label: clean_label.to_string(),
            hint: clean_hint.to_string(),
        })
    }
}

/// Error explicado en negocio + menú de 2-3 botones (la Piel lo renderiza).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct VibecoderError {
    /// Clase técnica (para telemetría, sin PII).
    pub kind: VibecoderKind,
    /// Título de negocio (`1..=80` chars, ej. "Tipos que no coinciden").
    pub title: String,
    /// Explicación en negocio (`1..=500` chars, sin jerga de compilador).
    pub explanation: String,
    /// Menú de reparación (siempre 2-3 botones, nunca mudos).
    pub options: Vec<VibecoderOption>,
}

impl VibecoderError {
    /// Valida y construye (`Err` si título/explicación vacíos o botones ≠2-3).
    pub fn try_new(
        kind: VibecoderKind,
        title: &str,
        explanation: &str,
        options: Vec<VibecoderOption>,
    ) -> Result<Self, String> {
        let clean_title = title.trim();
        if clean_title.is_empty() {
            return Err("título vacío".to_string());
        }
        if clean_title.chars().count() > MAX_VIBE_TITLE_CHARS {
            return Err(format!("título excede {MAX_VIBE_TITLE_CHARS} chars"));
        }
        let clean_explanation = explanation.trim();
        if clean_explanation.is_empty() {
            return Err("explicación vacía".to_string());
        }
        if clean_explanation.chars().count() > MAX_VIBE_EXPLANATION_CHARS {
            return Err(format!(
                "explicación excede {MAX_VIBE_EXPLANATION_CHARS} chars"
            ));
        }
        if !(MIN_VIBE_OPTIONS..=MAX_VIBE_OPTIONS).contains(&options.len()) {
            return Err(format!(
                "se requieren {}-{} botones, fueron {}",
                MIN_VIBE_OPTIONS,
                MAX_VIBE_OPTIONS,
                options.len()
            ));
        }
        Ok(Self {
            kind,
            title: clean_title.to_string(),
            explanation: clean_explanation.to_string(),
            options,
        })
    }

    /// Etiquetas de los botones (para la Piel, en orden).
    #[must_use]
    pub fn option_labels(&self) -> Vec<String> {
        self.options.iter().map(|o| o.label.clone()).collect()
    }
}

/// Explica un error técnico en negocio + 2-3 botones (siempre `Ok`, sin `Err`).
///
/// Pura y acotada: el mensaje crudo solo se usa para clasificar y, en
/// `Unknown`, como contexto truncado a 200 chars (sin PII: sin prompts ni
/// respuestas, solo el error del compilador). Español rioplatense, conciso.
#[must_use]
pub fn vibecoder_explain(compiler_message: &str, context: &str) -> VibecoderError {
    let kind = vibecoder_classify(compiler_message);
    let trimmed_context = context.trim();
    let context_suffix = if trimmed_context.is_empty() {
        String::new()
    } else {
        let clipped: String = trimmed_context.chars().take(80).collect();
        format!(" Estabas en: {clipped}.")
    };
    let (title, explanation, buttons): (&str, String, Vec<(&str, &str)>) = match kind {
        VibecoderKind::TypeMismatch => (
            "Tipos que no coinciden",
            format!(
                "La cuenta mezcla cosas distintas (texto donde va un número o al revés). Revisá qué pusiste en cada casillero.{context_suffix}"
            ),
            vec![
                ("Ver ejemplo", "Muestra un ejemplo con los tipos correctos."),
                ("Probar otro valor", "Probá con un número simple para aislar el casillero."),
                ("Pedir pista", "Te pregunto qué quisiste poner en cada lugar."),
            ],
        ),
        VibecoderKind::Syntax => (
            "Falta o sobra un símbolo",
            format!(
                "Hay un paréntesis o signo mal puesto y no se entiende la cuenta. Revisá el principio y el final.{context_suffix}"
            ),
            vec![
                ("Ver dónde", "Te marco el símbolo que confunde al lector."),
                ("Probar simple", "Probá con la cuenta más corta que falle."),
            ],
        ),
        VibecoderKind::Domain => (
            "Acá no tiene resultado",
            format!(
                "Con esos valores la cuenta no da (división por cero o fuera del dominio). Cambiá el valor problemático.{context_suffix}"
            ),
            vec![
                ("Probar otro valor", "Probá con un valor dentro del dominio."),
                ("Ver dominio", "Te muestro qué valores sí valen."),
                ("Pedir pista", "Te pregunto qué esperabas que diera."),
            ],
        ),
        VibecoderKind::Timeout => (
            "Tardó demasiado",
            format!(
                "El cálculo superó el tiempo (presupuesto 60s / 8 pasos). Partilo en pasos más chicos.{context_suffix}"
            ),
            vec![
                ("Partir en pasos", "Dividimos la cuenta en dos partes."),
                ("Simplificar", "Probamos con números más chicos primero."),
            ],
        ),
        VibecoderKind::Network => (
            "Sin conexión al modelo",
            format!(
                "No hay red para el modelo remoto (PII igual queda local). Podés seguir con lo local o reintentar.{context_suffix}"
            ),
            vec![
                ("Seguir local", "Usamos evaluación y ejercicios locales."),
                ("Reintentar", "Probamos de nuevo la conexión."),
            ],
        ),
        VibecoderKind::Unknown => {
            let clipped: String = compiler_message
                .trim()
                .chars()
                .filter(|c| !c.is_control() || *c == '\n' || *c == '\t')
                .take(200)
                .collect();
            let detail = if clipped.is_empty() {
                "Revisemos juntos qué pasó.".to_string()
            } else {
                format!("Detalle: {clipped}.")
            };
            (
                "Algo no salió",
                format!("No pude completar ese paso. {detail}{context_suffix}"),
                vec![
                    ("Reintentar simple", "Probamos con un caso mínimo."),
                    ("Pedir pista", "Te hago una pregunta para ubicarnos."),
                    ("Ver ejemplo", "Te muestro un ejemplo parecido resuelto."),
                ],
            )
        }
    };
    let options: Vec<VibecoderOption> = buttons
        .into_iter()
        .filter_map(|(label, hint)| VibecoderOption::try_new(label, hint).ok())
        .collect();
    // `try_new` debajo siempre tiene 2-3 (los literales de arriba cumplen);
    // si algo fallara, el fallback garantiza 2 botones válidos sin pánico.
    VibecoderError::try_new(kind, title, &explanation, options).unwrap_or_else(|_| {
        let fallback_options = vec![
            VibecoderOption {
                label: "Reintentar".to_string(),
                hint: "Probamos de nuevo con un caso mínimo.".to_string(),
            },
            VibecoderOption {
                label: "Pedir pista".to_string(),
                hint: "Te hago una pregunta para ubicarnos.".to_string(),
            },
        ];
        VibecoderError {
            kind,
            title: "Algo no salió".to_string(),
            explanation: "No pude completar ese paso. Probemos de a poco.".to_string(),
            options: fallback_options,
        }
    })
}

// ── S3: fix sintáctico seguro para fence/propuesta inválida ──────────────────
//
// Solo reescritura sintáctica (corchetes, coma trailing, mayúscula inicial).
// Jamás cambio semántico silencioso: el candidato solo se acepta si
// `parse_assistant_command` lo reconoce Y difiere del crudo únicamente por
// esos retoques. Puro, acotado (≤1024 bytes como el reconocedor) y sin `unwrap`.

/// Tope del crudo evaluado (paridad con `MAX_ASSISTANT_COMMAND_BYTES` del reconocedor).
pub const MAX_VIBE_FIX_BYTES: usize = 1_024;

/// ¿El fix solo toca sintaxis (sin cambiar números, nombres ni operadores)?
///
/// Compara crudo vs fix tras `trim`: permite únicamente agregar `]`/`)`
/// faltantes al final, quitar una coma trailing antes de `]` y capitalizar la
/// inicial del nombre (`function` → `Function`). Cualquier otra diferencia
/// (incluido cambiar el interior) es semántica y se rechaza.
fn is_safe_syntactic_fix(raw: &str, fixed: &str) -> bool {
    let raw = raw.trim();
    let fixed = fixed.trim();
    if raw.is_empty() || fixed.is_empty() || raw == fixed {
        return false;
    }
    // Normaliza capitalización inicial para comparar el resto.
    let mut raw_chars: Vec<char> = raw.chars().collect();
    let mut fixed_chars: Vec<char> = fixed.chars().collect();
    // Permite capitalizar la primera letra del nombre (antes de `[`).
    if let (Some(raw_open), Some(fixed_open)) = (raw.find('['), fixed.find('[')) {
        let (raw_name, fixed_name) = (&raw[..raw_open], &fixed[..fixed_open]);
        if raw_name.len() == fixed_name.len()
            && raw_name
                .chars()
                .zip(fixed_name.chars())
                .enumerate()
                .all(|(idx, (a, b))| {
                    if idx == 0 {
                        a.eq_ignore_ascii_case(&b)
                    } else {
                        a == b
                    }
                })
        {
            raw_chars = raw[raw_open..].chars().collect();
            fixed_chars = fixed[fixed_open..].chars().collect();
        } else if raw_name != fixed_name {
            return false;
        }
    } else {
        return false;
    }
    let raw_body: String = raw_chars.into_iter().collect();
    let fixed_body: String = fixed_chars.into_iter().collect();
    // 0. Solo cambió la mayúscula inicial (`function[x]` → `Function[x]`):
    //    cuerpos idénticos tras normalizar el nombre.
    if raw_body == fixed_body {
        return true;
    }
    // Casos permitidos sobre el cuerpo `[…]`:
    // 1. Agregar `]` (o `)]`) faltante al final.
    if fixed_body.starts_with(&raw_body) {
        let suffix = &fixed_body[raw_body.len()..];
        return matches!(suffix, "]" | ")]" | "))]" | "]]");
    }
    // 2. Quitar coma trailing antes de `]` (`[x,]` → `[x]`).
    if raw_body.ends_with(",]") && fixed_body == raw_body[..raw_body.len() - 2].to_owned() + "]" {
        return true;
    }
    // 3. Combinación: coma trailing + `]` faltante (`[x,` → `[x]`).
    if raw_body.ends_with(',') && fixed_body == raw_body[..raw_body.len() - 1].to_owned() + "]" {
        return true;
    }
    false
}

/// Candidatos sintácticos en orden (primero el que valide gana).
fn vibecoder_fix_candidates(raw: &str) -> Vec<String> {
    let trimmed = raw.trim().to_owned();
    if trimmed.is_empty() || trimmed.len() > MAX_VIBE_FIX_BYTES {
        return Vec::new();
    }
    let mut out = Vec::new();
    // 1. Recorta texto trailing tras el `]` balanceado (el LLM suele agregar
    //    "explicación" tras el comando): mismo recorte que `ApplyRawCommand`.
    if let Some(open) = trimmed.find('[') {
        let mut depth = 0i32;
        let mut close_idx: Option<usize> = None;
        for (idx, ch) in trimmed[open..].char_indices() {
            if ch == '[' {
                depth += 1;
            } else if ch == ']' {
                depth -= 1;
                if depth == 0 {
                    close_idx = Some(open + idx);
                    break;
                }
            }
        }
        if let Some(close) = close_idx {
            if trimmed.len() > close + 1 && !trimmed[close + 1..].trim().is_empty() {
                out.push(trimmed[..=close].trim().to_owned());
            }
        }
    }
    // 2. Agrega `]` faltante.
    if trimmed.contains('[') && !trimmed.ends_with(']') {
        out.push(format!("{trimmed}]"));
    }
    // 3. Quita coma trailing (`[x,]` → `[x]`, `[x,` → `[x]`).
    if trimmed.ends_with(",]") {
        out.push(trimmed[..trimmed.len() - 2].to_owned() + "]");
    } else if trimmed.ends_with(',') {
        out.push(trimmed[..trimmed.len() - 1].to_owned() + "]");
    }
    // 4. Capitaliza la inicial (`function[x]` → `Function[x]`).
    if let Some(open) = trimmed.find('[') {
        let (name, rest) = trimmed.split_at(open);
        let name = name.trim();
        if let Some(first) = name.chars().next() {
            if first.is_ascii_lowercase() {
                let mut fixed_name = first.to_ascii_uppercase().to_string();
                fixed_name.push_str(&name[first.len_utf8()..]);
                out.push(format!("{fixed_name}{rest}"));
                // Capitalizada + `]` faltante.
                if (rest.contains('[') || trimmed.contains('[')) && !rest.trim_end().ends_with(']')
                {
                    out.push(format!("{fixed_name}{rest}]"));
                }
            }
        }
    }
    // 5. Combinación coma + capitalización (se genera vía 3+4 en validación).
    out
}

/// Sugiere un fix sintáctico seguro para una entrada rota típica.
///
/// Devuelve el texto canónico (`canonical_text`) si algún candidato valida con
/// `parse_assistant_command` Y es solo retoque sintáctico; `None` en cualquier
/// otro caso (incluido comando semánticamente distinto como `Script[Save[]]`,
/// que jamás se reescribe en silencio). Pura, sin `unwrap`.
#[must_use]
pub fn vibecoder_suggest_fix(raw: &str) -> Option<String> {
    let trimmed = raw.trim();
    if trimmed.is_empty() || trimmed.len() > MAX_VIBE_FIX_BYTES {
        return None;
    }
    // Si ya valida, no hay nada que arreglar (no se propone a sí misma).
    if grafito_command::assistant_proposals::parse_assistant_command(trimmed).is_some() {
        return None;
    }
    for candidate in vibecoder_fix_candidates(trimmed) {
        if candidate.len() > MAX_VIBE_FIX_BYTES {
            continue;
        }
        let Some(invocation) =
            grafito_command::assistant_proposals::parse_assistant_command(&candidate)
        else {
            continue;
        };
        let canonical = invocation.canonical_text();
        // El canónico debe validar y ser retoque sintáctico del crudo.
        if grafito_command::assistant_proposals::parse_assistant_command(&canonical).is_none() {
            continue;
        }
        if is_safe_syntactic_fix(trimmed, &candidate) || is_safe_syntactic_fix(trimmed, &canonical)
        {
            return Some(canonical);
        }
    }
    None
}

/// Explicación + fix para un fence/propuesta inválida (S3).
///
/// Combina `vibecoder_explain` (negocio en rioplatense, 2-3 botones) con
/// `vibecoder_suggest_fix` (solo sintaxis). El fix es `Some` únicamente cuando
/// es seguro en 1 click; el caller lo ofrece vía bloque fenced (que ya tiene
/// botón Aplicar) o vía `InsertCommand`, jamás auto-aplicado en silencio.
#[must_use]
pub fn explain_invalid_proposal(raw: &str, context: &str) -> (VibecoderError, Option<String>) {
    let clipped: String = raw.trim().chars().take(200).collect();
    let compiler_hint = if clipped.is_empty() {
        "syntax error: propuesta vacía".to_owned()
    } else if raw.trim().len() > MAX_VIBE_FIX_BYTES {
        "syntax error: propuesta excede el tope".to_owned()
    } else {
        format!("syntax error: propuesta inválida ({clipped})")
    };
    let explained = vibecoder_explain(&compiler_hint, context);
    let fix = vibecoder_suggest_fix(raw);
    (explained, fix)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn agent_payload_contains_tools_and_messages_without_secrets() {
        let settings = ProviderSettings::for_profile(crate::ProviderProfile::OllamaLocal, "local");
        let tool = ToolSchema::new(
            "evaluate_expr",
            "Evalúa una expresión.",
            json!({"type": "object", "properties": {"expression": {"type": "string"}}}),
        );
        let payload = build_agent_payload(
            &settings,
            &[json!({"role": "user", "content": "hola"})],
            &[tool],
            256,
        )
        .unwrap();
        let serialized = payload.to_string();
        assert!(serialized.contains("\"tools\""));
        assert!(serialized.contains("evaluate_expr"));
        assert!(serialized.contains("\"role\":\"user\""));
        assert!(!serialized.contains("api_key"));
    }

    #[test]
    fn responses_input_budget_matches_chat_loop() {
        // Paridad con `AgentBudget::default().max_total_chars` (loop_engine).
        assert_eq!(
            MAX_RESPONSES_INPUT_CHARS,
            AgentBudget::default().max_total_chars
        );
        assert_eq!(MAX_RESPONSES_INPUT_CHARS, 48_000);
    }

    #[test]
    fn agent_worst_case_request_counts_match_default_budgets() {
        // El peor caso documentado sigue a `AgentBudget::default` (sólo
        // lectura, los budgets no se tocan): Responses 1 POST/turno, Chat
        // 1 + reintentos por turno. Si el budget sube, este test avisa que
        // la amplificación también sube.
        let budget = AgentBudget::default();
        assert_eq!(budget.max_tool_turns, 4);
        assert_eq!(budget.max_retries, 2);
        assert_eq!(
            MAX_AGENT_RESPONSES_HTTP_REQUESTS_PER_TURN,
            budget.max_tool_turns.saturating_add(1)
        );
        assert_eq!(MAX_AGENT_RESPONSES_HTTP_REQUESTS_PER_TURN, 5);
        assert_eq!(
            MAX_AGENT_CHAT_HTTP_REQUESTS_PER_TURN,
            budget
                .max_tool_turns
                .saturating_add(1)
                .saturating_mul(budget.max_retries as usize + 1)
        );
        assert_eq!(MAX_AGENT_CHAT_HTTP_REQUESTS_PER_TURN, 15);
        // Coherencia con el transporte simple (crate raíz).
        assert_eq!(crate::MAX_SIMPLE_CHAT_HTTP_REQUESTS_PER_TURN, 2);
        assert_eq!(crate::MAX_FUSION_HTTP_REQUESTS_PER_TURN, 2);
    }

    #[cfg(feature = "assistant-net")]
    #[test]
    fn agent_transport_stays_quiet_while_rate_limited() {
        use std::sync::atomic::{AtomicUsize, Ordering};
        use std::sync::Arc;
        crate::clear_rate_limit_for_tests();
        // Stub que cuenta conexiones: si el freno falla, el POST llega y el
        // contador lo delata. Ventana de 400ms y cierre limpio.
        let listener = std::net::TcpListener::bind("127.0.0.1:0").expect("stub binds");
        let port = listener.local_addr().expect("stub addr").port();
        let _ = listener.set_nonblocking(true);
        let hits = Arc::new(AtomicUsize::new(0));
        let counter = hits.clone();
        let server = std::thread::spawn(move || {
            let start = std::time::Instant::now();
            while start.elapsed() < Duration::from_millis(400) {
                match listener.accept() {
                    Ok(_) => {
                        counter.fetch_add(1, Ordering::SeqCst);
                    }
                    Err(_) => std::thread::sleep(Duration::from_millis(10)),
                }
            }
        });
        crate::record_rate_limited(Some(60));
        let settings = crate::ProviderSettings::for_profile(
            crate::ProviderProfile::OllamaLocal,
            "deepseek-v4-flash",
        )
        .with_endpoint(format!("http://127.0.0.1:{port}/v1"))
        .expect("stub endpoint is valid");
        let error = request_agent_completion(
            &settings,
            Some("test-key"),
            &[],
            &[],
            64,
            Duration::from_secs(1),
            &Cancellation::default(),
        )
        .unwrap_err();
        // Se limpia ANTES de esperar al stub: la pausa de 60s no debe
        // contaminar a los stubs 200/429 que corren en paralelo.
        crate::clear_rate_limit_for_tests();
        server.join().expect("stub joins");
        assert!(
            error.starts_with(crate::RATE_LIMIT_PAUSED_MARKER),
            "falla honesto con marcador, sin tocar la red: {error}"
        );
        assert_eq!(hits.load(Ordering::SeqCst), 0, "cero conexiones en pausa");
    }

    #[cfg(feature = "assistant-net")]
    #[test]
    fn agent_post_records_cooldown_on_429_with_retry_after() {
        use std::io::{Read, Write};
        crate::clear_rate_limit_for_tests();
        let listener = std::net::TcpListener::bind("127.0.0.1:0").expect("stub binds");
        let port = listener.local_addr().expect("stub addr").port();
        // Accept con deadline (no bloqueante): si una pausa ajena (otro test
        // en paralelo) hiciera fallar el POST antes de conectar, el join
        // igual termina y el test falla rápido en vez de colgarse.
        let server = std::thread::spawn(move || {
            let _ = listener.set_nonblocking(true);
            let start = std::time::Instant::now();
            while start.elapsed() < Duration::from_secs(10) {
                match listener.accept() {
                    Ok((mut stream, _)) => {
                        let mut buffer = [0; 4096];
                        let _ = stream.read(&mut buffer);
                        let body = "busy";
                        write!(
                            stream,
                            "HTTP/1.1 429 Too Many Requests\r\nContent-Type: text/plain\r\nRetry-After: 3\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
                            body.len(),
                            body
                        )
                        .expect("stub writes");
                        break;
                    }
                    Err(_) => std::thread::sleep(Duration::from_millis(10)),
                }
            }
        });
        let settings = crate::ProviderSettings::for_profile(
            crate::ProviderProfile::OllamaLocal,
            "muse-spark-test",
        )
        .with_endpoint(format!("http://127.0.0.1:{port}/v1"))
        .expect("stub endpoint is valid");
        let error = post_responses_output(
            &settings,
            Some("test-key"),
            &serde_json::json!({"model": "muse-spark-test"}),
            Duration::from_secs(5),
            &Cancellation::default(),
        )
        .unwrap_err();
        server.join().expect("stub joins");
        assert!(error.contains("429"), "{error}");
        assert!(error.contains("reintentá en 3s"), "{error}");
        let remaining = crate::rate_limit_cooldown_remaining_secs().expect("pausa armada");
        assert!(
            (1..=3).contains(&remaining),
            "respeta Retry-After: {remaining}"
        );
        crate::clear_rate_limit_for_tests();
    }

    #[test]
    fn responses_input_chars_sums_serialized_len() {
        let input = vec![
            json!({"role": "user", "content": "hola"}),
            json!({"type": "function_call", "call_id": "c1", "name": "t", "arguments": "{}"}),
        ];
        let expected: usize = input.iter().map(|item| item.to_string().len()).sum();
        assert_eq!(responses_input_chars(&input), expected);
        assert_eq!(responses_input_chars(&[]), 0);
        // Un turno típico (2 calls + 2 outputs de 2048 chars) cabe holgado.
        let big_output = "x".repeat(2_048);
        let turn = vec![
            json!({"type": "function_call", "call_id": "c1", "name": "t", "arguments": "{}"}),
            json!({"type": "function_call_output", "call_id": "c1", "output": big_output}),
        ];
        assert!(responses_input_chars(&turn) < MAX_RESPONSES_INPUT_CHARS);
        // 24 outputs de 2048 chars (peor caso ×6 del budget de 4 turnos) exceden.
        let flood: Vec<Value> = (0..24)
            .map(|i| json!({"type": "function_call_output", "call_id": i, "output": big_output}))
            .collect();
        assert!(responses_input_chars(&flood) > MAX_RESPONSES_INPUT_CHARS);
    }

    #[test]
    fn agent_completion_parses_text_and_tool_calls() {
        let text_body = json!({
            "choices": [{
                "finish_reason": "stop",
                "message": {"role": "assistant", "content": "final"}
            }]
        });
        match parse_agent_completion(&text_body).unwrap() {
            AgentChatResponse::Text { content, truncated } => {
                assert_eq!(content, "final");
                assert!(!truncated);
            }
            _ => panic!("expected text"),
        }

        let tool_body = json!({
            "choices": [{
                "finish_reason": "tool_calls",
                "message": {
                    "role": "assistant",
                    "content": null,
                    "tool_calls": [{
                        "id": "call-1",
                        "type": "function",
                        "function": {"name": "evaluate_expr", "arguments": "{\"expression\":\"2+2\"}"}
                    }]
                }
            }]
        });
        match parse_agent_completion(&tool_body).unwrap() {
            AgentChatResponse::ToolCalls { calls } => {
                assert_eq!(calls.len(), 1);
                assert_eq!(calls[0].name, "evaluate_expr");
            }
            _ => panic!("expected tool calls"),
        }
    }

    #[test]
    fn safe_dispatcher_never_writes_documents_or_files() {
        let call = ToolCall {
            id: "call-e".into(),
            name: "evaluate_expr".into(),
            arguments: json!({"expression": "sin(0)", "variables": {}}),
        };
        let result = dispatch_safe_tool(&call);
        assert!(result.ok);
        assert!(result.content.contains('0'));

        let unknown = ToolCall {
            id: "call-u".into(),
            name: "bash".into(),
            arguments: json!({"command": "rm -rf /"}),
        };
        let denied = dispatch_safe_tool(&unknown);
        assert!(!denied.ok);
        assert!(denied.content.contains("not available"));
    }

    #[test]
    fn evaluate_expr_rechaza_tool_arg_gigante_sin_abortar() {
        // F10-FIX red-first: arg DEL MODELO de 16KB (`"x+".repeat(8000)`,
        // el que abortaba `evaluate` en prod) → `Err` honesto, jamás SIGABRT.
        // El solo hecho de completar el test prueba que no hay abort.
        let hostile = "x+".repeat(8_000);
        assert_eq!(hostile.len(), 16_000);
        let call = ToolCall {
            id: "call-hostil".into(),
            name: "evaluate_expr".into(),
            arguments: json!({"expression": hostile, "variables": {}}),
        };
        // Vía dispatcher (con freno genérico de 2000 bytes).
        let via_dispatcher = dispatch_safe_tool(&call);
        assert!(!via_dispatcher.ok);
        assert!(via_dispatcher.content.contains("2000"));
        // Directo a la tool (bypass del freno genérico): también `Err`.
        let direct = evaluate_expr_tool(&call);
        assert!(!direct.ok);
        assert!(direct.content.contains("2000"));
        // Parens gigantes anidados: también `Err` en ambos niveles.
        let nested = format!("{}1{}", "(".repeat(8_000), ")".repeat(8_000));
        let nested_call = ToolCall {
            id: "call-hostil-paren".into(),
            name: "evaluate_expr".into(),
            arguments: json!({"expression": nested, "variables": {}}),
        };
        assert!(!dispatch_safe_tool(&nested_call).ok);
        assert!(!evaluate_expr_tool(&nested_call).ok);
    }

    #[test]
    fn ask_user_requires_explicit_consent_and_never_runs_silently() {
        let call = ToolCall {
            id: "call-a".into(),
            name: "ask_user".into(),
            arguments: json!({"question": "cuánto es x?"}),
        };
        let result = dispatch_safe_tool(&call);
        assert!(!result.ok);
        assert!(result.content.contains("explicit user answer"));
    }

    #[test]
    fn ask_user_pending_round_trip_via_evento_sin_bloquear() {
        // Dispatcher → pendiente estructurado → parse → function_call_output.
        // Puro y no bloqueante (sin threads, sin Document, sin I/O).
        let call = ToolCall {
            id: "call-q".into(),
            name: "ask_user".into(),
            arguments: json!({"question": "¿qué valor le doy a x?", "options": ["0", "1"]}),
        };
        let result = dispatch_safe_tool(&call);
        assert!(!result.ok, "ask_user nunca ejecuta en silencio");
        let pending = grafito_agent::tools::parse_ask_user_pending(&result.content)
            .expect("pendiente estructurado parseable");
        assert_eq!(pending.question(), "¿qué valor le doy a x?");
        assert_eq!(pending.options(), &["0".to_owned(), "1".to_owned()]);
        let output = grafito_agent::tools::ask_user_answer_function_output(&result.call_id, "1")
            .expect("respuesta no vacía");
        assert_eq!(output["type"], "function_call_output");
        assert_eq!(output["call_id"], result.call_id);
        assert_eq!(output["output"], "1");
    }

    #[test]
    fn ask_user_rechaza_pregunta_vacia_y_respuesta_vacia() {
        let call = ToolCall {
            id: "call-e".into(),
            name: "ask_user".into(),
            arguments: json!({}),
        };
        let result = dispatch_safe_tool(&call);
        assert!(!result.ok);
        assert!(grafito_agent::tools::parse_ask_user_pending(&result.content).is_none());
        assert!(grafito_agent::tools::ask_user_answer_function_output("c1", "   ").is_none());
    }

    // ── Tests pedagógicos F3.2 ──────────────────────────────────────────────

    #[test]
    fn scaffold_tool_returns_question_and_hint() {
        let call = ToolCall {
            id: "s1".into(),
            name: "scaffold".into(),
            arguments: json!({"concept": "derivada", "level": "secondary"}),
        };
        let result = dispatch_safe_tool(&call);
        assert!(result.ok, "scaffold should succeed: {}", result.content);
        let value: Value = serde_json::from_str(&result.content).expect("valid json");
        assert_eq!(value["concept"], "derivada");
        assert!(value["question"].as_str().unwrap_or_default().len() > 5);
        assert!(value["explanation"].as_str().unwrap_or_default().len() > 5);
    }

    #[test]
    fn scaffold_tool_rejects_empty_concept() {
        let call = ToolCall {
            id: "s2".into(),
            name: "scaffold".into(),
            arguments: json!({"concept": ""}),
        };
        let result = dispatch_safe_tool(&call);
        assert!(!result.ok);
    }

    #[test]
    fn generate_exercise_tool_is_deterministic_and_valid() {
        let call = ToolCall {
            id: "g1".into(),
            name: "generate_exercise".into(),
            arguments: json!({"lo_id": "am1-der", "level": "university", "seed": 42}),
        };
        let result = dispatch_safe_tool(&call);
        assert!(result.ok, "{}", result.content);
        let value: Value = serde_json::from_str(&result.content).expect("json");
        assert_eq!(value["lo_id"], "am1-der");
        assert!(value["prompt"]
            .as_str()
            .unwrap_or_default()
            .contains("Deriva"));
        // determinismo: segunda llamada igual
        let result2 = dispatch_safe_tool(&call);
        assert_eq!(result.content, result2.content);
    }

    #[test]
    fn generate_exercise_fuzzy_concept_lookup() {
        let call = ToolCall {
            id: "g2".into(),
            name: "generate_exercise".into(),
            arguments: json!({"lo_id": "derivada"}),
        };
        let result = dispatch_safe_tool(&call);
        // "derivada" no es id exacto pero find_for_concept debe resolver a am1-der
        assert!(result.ok, "fuzzy lookup should work: {}", result.content);
    }

    #[test]
    fn assess_answer_tool_detects_correct_and_misconception() {
        // Generar ejercicio am1-der seed 0 => solution determinista
        let exercise_call = ToolCall {
            id: "tmp".into(),
            name: "generate_exercise".into(),
            arguments: json!({"lo_id": "am1-der", "seed": 0}),
        };
        let exercise_result = dispatch_safe_tool(&exercise_call);
        let exercise_value: Value = serde_json::from_str(&exercise_result.content).unwrap();
        let expected = exercise_value["solution"].as_str().unwrap().to_string();

        let correct_call = ToolCall {
            id: "a1".into(),
            name: "assess_answer".into(),
            arguments: json!({"exercise_id": "am1-der", "answer": expected, "seed": 0}),
        };
        let correct = dispatch_safe_tool(&correct_call);
        assert!(correct.ok);
        let value: Value = serde_json::from_str(&correct.content).unwrap();
        assert_eq!(value["correct"], true);

        let wrong_call = ToolCall {
            id: "a2".into(),
            name: "assess_answer".into(),
            arguments: json!({"exercise_id": "am1-der", "answer": "99999", "seed": 0}),
        };
        let wrong = dispatch_safe_tool(&wrong_call);
        assert!(wrong.ok); // evaluación ok aunque respuesta incorrecta
        let value2: Value = serde_json::from_str(&wrong.content).unwrap();
        assert_eq!(value2["correct"], false);
        assert!(value2["misconception"].as_str().unwrap_or_default().len() > 2);
    }

    #[test]
    fn assess_answer_requires_answer_field() {
        let call = ToolCall {
            id: "a3".into(),
            name: "assess_answer".into(),
            arguments: json!({"exercise_id": "am1-der"}),
        };
        let result = dispatch_safe_tool(&call);
        assert!(!result.ok);
    }

    #[test]
    fn get_curriculum_tool_finds_derivada() {
        let call = ToolCall {
            id: "c1".into(),
            name: "get_curriculum".into(),
            arguments: json!({"query": "derivada"}),
        };
        let result = dispatch_safe_tool(&call);
        assert!(result.ok, "{}", result.content);
        let value: Value = serde_json::from_str(&result.content).unwrap();
        assert!(value["count"].as_u64().unwrap_or(0) > 0);
        let first_id = value["results"][0]["id"].as_str().unwrap_or("");
        assert!(first_id.contains("der") || first_id.contains("am1"));
    }

    #[test]
    fn get_curriculum_rejects_empty_query() {
        let call = ToolCall {
            id: "c2".into(),
            name: "get_curriculum".into(),
            arguments: json!({"query": ""}),
        };
        let result = dispatch_safe_tool(&call);
        assert!(!result.ok);
    }

    #[test]
    fn suggest_next_tool_returns_mock_recommendation() {
        let call = ToolCall {
            id: "n1".into(),
            name: "suggest_next".into(),
            arguments: json!({}),
        };
        let result = dispatch_safe_tool(&call);
        assert!(result.ok, "{}", result.content);
        let value: Value = serde_json::from_str(&result.content).unwrap();
        assert_eq!(value["mock"], true);
        assert!(value["next"].is_array() || value["next"].is_object());
    }

    #[test]
    fn generate_animation_tool_validates_and_normalizes() {
        let call = ToolCall {
            id: "anim1".into(),
            name: "generate_animation".into(),
            arguments: json!({"concept": "derivada como pendiente", "template": "derivative-slope", "params": {"a": 1.0}}),
        };
        let result = dispatch_safe_tool(&call);
        assert!(result.ok, "{}", result.content);
        let value: Value = serde_json::from_str(&result.content).unwrap();
        assert_eq!(value["template"], "derivative-slope");
        assert!(value["concept"].as_str().unwrap_or_default().len() > 3);
        assert_eq!(value["export"], "gif");
    }

    #[test]
    fn generate_animation_auto_template_when_empty() {
        let call = ToolCall {
            id: "anim2".into(),
            name: "generate_animation".into(),
            arguments: json!({"concept": "integral área bajo curva"}),
        };
        let result = dispatch_safe_tool(&call);
        assert!(result.ok, "{}", result.content);
        let value: Value = serde_json::from_str(&result.content).unwrap();
        assert_eq!(value["template"], "integral-area");
    }

    #[test]
    fn generate_animation_rejects_empty_both() {
        let call = ToolCall {
            id: "anim3".into(),
            name: "generate_animation".into(),
            arguments: json!({"template": "", "concept": ""}),
        };
        let result = dispatch_safe_tool(&call);
        assert!(!result.ok);
    }

    // ── AS4: vía pedido en lenguaje natural → plan paramétrico ──────────
    #[test]
    fn generate_animation_pedido_barrido_ok() {
        let call = ToolCall {
            id: "as4-1".into(),
            name: "generate_animation".into(),
            arguments: json!({"pedido": "barrido de f(x)=x^2+p*x con p en [-2,2]"}),
        };
        let result = dispatch_safe_tool(&call);
        assert!(result.ok, "{}", result.content);
        let value: Value = serde_json::from_str(&result.content).unwrap();
        assert_eq!(value["kind"], "sweep");
        assert_eq!(value["expr_a"], "x^2+p*x");
        assert_eq!(value["param"], "p");
        assert_eq!(value["frames"], 24);
        // Pista humanizada, sin identificadores literales de control.
        let hint = value["hint"].as_str().unwrap_or_default();
        assert!(hint.contains("deslizador"));
        assert!(hint.contains("reproducir"));
        assert!(hint.contains("pausar"));
        for id in [
            "PlayPause",
            "Slider",
            "Button",
            "Tangent",
            "Select",
            "Parallel",
            "Midpoint",
            "Distance",
            "Angle",
            "Area",
            "Function",
            "Polygon",
            "Circle",
            "Line",
            "Point",
            "Vector",
            "Segment",
            "Ray",
            "Eraser",
            "Pencil",
        ] {
            assert!(
                !result.content.contains(id),
                "el payload no debe traer {id}"
            );
        }
    }

    #[test]
    fn generate_animation_pedido_tangente_ok() {
        let call = ToolCall {
            id: "as4-2".into(),
            name: "generate_animation".into(),
            arguments: json!({"pedido": "animá la recta tangente móvil de f(x)=x^2 con p en [-1,1]"}),
        };
        let result = dispatch_safe_tool(&call);
        assert!(result.ok, "{}", result.content);
        let value: Value = serde_json::from_str(&result.content).unwrap();
        assert_eq!(value["kind"], "tangent");
        assert_eq!(value["kind_label"], "recta tangente móvil");
    }

    #[test]
    fn generate_animation_pedido_tres_errores_honestos() {
        // 1. Sin tipo: hay expresión pero no dice qué animar.
        let sin_tipo = ToolCall {
            id: "as4-e1".into(),
            name: "generate_animation".into(),
            arguments: json!({"pedido": "dibujame f(x)=x^2"}),
        };
        let r1 = dispatch_safe_tool(&sin_tipo);
        assert!(!r1.ok);
        assert!(r1.content.contains("qué tipo"), "r1: {}", r1.content);
        // 2. Sin expresión.
        let sin_expr = ToolCall {
            id: "as4-e2".into(),
            name: "generate_animation".into(),
            arguments: json!({"pedido": "barrido con p en [-2,2]"}),
        };
        let r2 = dispatch_safe_tool(&sin_expr);
        assert!(!r2.ok);
        assert!(r2.content.contains("expresión"), "r2: {}", r2.content);
        // 3. Sin rango.
        let sin_rango = ToolCall {
            id: "as4-e3".into(),
            name: "generate_animation".into(),
            arguments: json!({"pedido": "barrido de f(x)=x^2+p*x"}),
        };
        let r3 = dispatch_safe_tool(&sin_rango);
        assert!(!r3.ok);
        assert!(r3.content.contains("rango"), "r3: {}", r3.content);
    }

    // ── N1: pedido de integral — canónica / explícita / inválida ────────
    #[test]
    fn generate_animation_pedido_integral_sin_funcion_usa_canonica() {
        let call = ToolCall {
            id: "n1-a".into(),
            name: "generate_animation".into(),
            arguments: json!({"pedido": "haceme una animacion de una integral (nativa)"}),
        };
        let result = dispatch_safe_tool(&call);
        assert!(result.ok, "{}", result.content);
        let value: Value = serde_json::from_str(&result.content).unwrap();
        assert_eq!(value["kind"], "area");
        assert_eq!(value["expr_a"], "x^2");
        assert_eq!(value["param"], "p");
        assert_eq!(value["canonical"], true);
        // La pista declara la canónica en rioplatense, con nombres humanos.
        let hint = value["hint"].as_str().unwrap_or_default();
        assert!(hint.contains("pedime otra"), "{hint}");
        assert!(hint.contains("x²"), "{hint}");
        assert!(hint.contains("deslizador"), "{hint}");
    }

    #[test]
    fn generate_animation_pedido_integral_con_funcion_es_explicita() {
        let call = ToolCall {
            id: "n1-b".into(),
            name: "generate_animation".into(),
            arguments: json!({"pedido": "animacion de la integral de f(x)=x^3 de 0 a 2"}),
        };
        let result = dispatch_safe_tool(&call);
        assert!(result.ok, "{}", result.content);
        let value: Value = serde_json::from_str(&result.content).unwrap();
        assert_eq!(value["kind"], "area");
        assert_eq!(value["expr_a"], "x^3");
        assert_eq!(value["canonical"], false);
        let hint = value["hint"].as_str().unwrap_or_default();
        assert!(!hint.contains("pedime otra"), "{hint}");
    }

    #[test]
    fn generate_animation_pedido_integral_invalida_falla_sin_plan() {
        let call = ToolCall {
            id: "n1-c".into(),
            name: "generate_animation".into(),
            arguments: json!({"pedido": "animacion de la integral de f(x)=foo(x) con p en [0,1]"}),
        };
        let result = dispatch_safe_tool(&call);
        assert!(
            !result.ok,
            "función inválida no propone plan: {}",
            result.content
        );
        assert!(result.content.contains("foo(x)"), "{}", result.content);
        assert!(
            result.content.contains("x^2"),
            "da ejemplo: {}",
            result.content
        );
    }

    #[test]
    fn generate_animation_template_integral_declara_canonica() {
        // Vía template/concept sin pedido: también declara la canónica que
        // va a renderizar, para que la prosa no pregunte por la función.
        let call = ToolCall {
            id: "n1-d".into(),
            name: "generate_animation".into(),
            arguments: json!({"template": "integral-area", "concept": "integral"}),
        };
        let result = dispatch_safe_tool(&call);
        assert!(result.ok, "{}", result.content);
        let value: Value = serde_json::from_str(&result.content).unwrap();
        assert_eq!(value["template"], "integral-area");
        assert_eq!(value["canonical"], true);
        assert_eq!(value["canonical_expr"], "x^2");
        assert!(value["canonical_range"].is_array(), "{}", result.content);
        assert!(
            value["canonical_prose"]
                .as_str()
                .unwrap_or_default()
                .contains("pedime otra"),
            "{}",
            result.content
        );
    }

    #[test]
    fn generate_animation_pedido_typo_screenshot_es_canonica() {
        // Input EXACTO del screenshot (vía agente): el typo "integrela" va a
        // canónica declarada, igual que Submit y la inferencia. Sin doble
        // carril: el agente no pregunta lo que la vista muestra.
        let call = ToolCall {
            id: "n1-typo".into(),
            name: "generate_animation".into(),
            arguments: json!({"pedido": "hace una animacion de una integrela (nativa)"}),
        };
        let result = dispatch_safe_tool(&call);
        assert!(result.ok, "{}", result.content);
        let value: Value = serde_json::from_str(&result.content).unwrap();
        assert_eq!(value["kind"], "area");
        assert_eq!(value["expr_a"], "x^2");
        assert_eq!(value["canonical"], true);
        let hint = value["hint"].as_str().unwrap_or_default();
        assert!(hint.contains("pedime otra"), "{hint}");
        assert!(hint.contains("x²"), "{hint}");
        assert!(hint.contains("deslizador"), "{hint}");
    }

    #[test]
    fn pedagogy_schemas_are_valid_openai_tools() {
        for schema in pedagogy_tool_schemas() {
            assert!(schema.validate().is_ok(), "schema {} invalid", schema.name);
            let openai = schema.openai_tool().expect("openai_tool");
            assert_eq!(openai["type"], "function");
            assert_eq!(openai["function"]["name"], schema.name);
        }
        assert_eq!(pedagogy_tool_schemas().len(), 6);
        assert!(all_safe_tool_schemas().len() >= 9);
    }

    #[test]
    fn pedagogy_dispatcher_only_handles_pedagogy_tools() {
        let pedagogy = PedagogyDispatcher;
        let call = ToolCall {
            id: "p1".into(),
            name: "scaffold".into(),
            arguments: json!({"concept": "integral"}),
        };
        let result = pedagogy.dispatch(&call);
        assert!(result.ok);

        let base_call = ToolCall {
            id: "p2".into(),
            name: "evaluate_expr".into(),
            arguments: json!({"expression": "2+2"}),
        };
        let denied = pedagogy.dispatch(&base_call);
        assert!(!denied.ok);
        assert!(denied.content.contains("no disponible"));
    }

    #[test]
    fn safe_dispatcher_covers_all_pedagogy_tools() {
        for name in [
            "scaffold",
            "generate_exercise",
            "assess_answer",
            "get_curriculum",
            "suggest_next",
            "generate_animation",
        ] {
            let call = ToolCall {
                id: "cov".into(),
                name: name.into(),
                arguments: match name {
                    "scaffold" => json!({"concept": "derivada"}),
                    "generate_exercise" => json!({"lo_id": "am1-der"}),
                    "assess_answer" => json!({"answer": "2", "lo_id": "am1-der"}),
                    "get_curriculum" => json!({"query": "derivada"}),
                    "suggest_next" => json!({}),
                    "generate_animation" => json!({"concept": "derivada"}),
                    _ => json!({}),
                },
            };
            let result = dispatch_safe_tool(&call);
            // assess_answer con answer 2 puede ser incorrecto pero dispatch ok
            // todas deben al menos no ser "not available"
            assert!(
                !result.content.contains("not available"),
                "tool {name} should be available, got {}",
                result.content
            );
        }
    }

    // ── Tests Responses API (Muse Spark, sin imágenes) ───────────────────────

    #[cfg(feature = "assistant-net")]
    fn spark_stub_settings(port: u16) -> ProviderSettings {
        ProviderSettings::for_profile(crate::ProviderProfile::OllamaLocal, "muse-spark-test")
            .with_endpoint(format!("http://127.0.0.1:{port}/v1"))
            .expect("loopback stub endpoint is valid")
    }

    #[cfg(feature = "assistant-net")]
    fn spark_budget(per_turn: Duration, total: Duration) -> AgentBudget {
        AgentBudget {
            max_tool_turns: 4,
            per_turn_timeout: per_turn,
            total_span: total,
            max_output_chars: 2_048,
            ..Default::default()
        }
    }

    /// Sirve `scripted` como respuestas 200 JSON en orden, guardando cada body.
    #[cfg(feature = "assistant-net")]
    fn spawn_responses_stub(
        scripted: Vec<Value>,
        seen: std::sync::Arc<std::sync::Mutex<Vec<String>>>,
    ) -> (u16, std::thread::JoinHandle<()>) {
        use std::io::{Read, Write};
        let listener = std::net::TcpListener::bind("127.0.0.1:0").expect("stub binds loopback");
        let port = listener.local_addr().expect("stub has addr").port();
        let handle = std::thread::spawn(move || {
            for body in scripted {
                let (mut stream, _) = listener.accept().expect("stub accepts");
                stream
                    .set_read_timeout(Some(Duration::from_secs(10)))
                    .expect("stub timeout");
                let mut raw = Vec::new();
                let mut chunk = [0_u8; 4096];
                loop {
                    let read = stream.read(&mut chunk).expect("stub reads");
                    if read == 0 {
                        break;
                    }
                    raw.extend_from_slice(&chunk[..read]);
                    if raw.windows(4).any(|w| w == b"\r\n\r\n") {
                        break;
                    }
                }
                let header_end = raw
                    .windows(4)
                    .position(|w| w == b"\r\n\r\n")
                    .map(|position| position + 4)
                    .unwrap_or(raw.len());
                let headers = String::from_utf8_lossy(&raw[..header_end]).into_owned();
                let content_len: usize = headers
                    .lines()
                    .filter_map(|line| line.split_once(':'))
                    .find_map(|(key, value)| {
                        if key.trim().eq_ignore_ascii_case("content-length") {
                            value.trim().parse().ok()
                        } else {
                            None
                        }
                    })
                    .unwrap_or(0);
                let mut body_bytes = raw[header_end..].to_vec();
                while body_bytes.len() < content_len {
                    let read = stream.read(&mut chunk).expect("stub reads body");
                    if read == 0 {
                        break;
                    }
                    body_bytes.extend_from_slice(&chunk[..read]);
                }
                seen.lock()
                    .expect("stub lock")
                    .push(String::from_utf8_lossy(&body_bytes).into_owned());
                let payload = serde_json::to_string(&body).expect("stub json");
                let response = format!(
                    "HTTP/1.1 200 OK\r\ncontent-type: application/json\r\ncontent-length: {}\r\nconnection: close\r\n\r\n{payload}",
                    payload.len()
                );
                stream.write_all(response.as_bytes()).expect("stub writes");
                stream.flush().expect("stub flushes");
            }
        });
        (port, handle)
    }

    #[test]
    fn responses_router_matches_spark_only() {
        let spark = ProviderSettings::for_profile(
            crate::ProviderProfile::OllamaLocal,
            "muse-spark-1.3-contributor",
        );
        assert!(uses_responses_agent_transport(&spark));
        let deepseek =
            ProviderSettings::for_profile(crate::ProviderProfile::OllamaLocal, "deepseek-v4-flash");
        assert!(!uses_responses_agent_transport(&deepseek));
    }

    #[test]
    fn responses_payload_uses_instructions_input_and_function_tools() {
        let settings =
            ProviderSettings::for_profile(crate::ProviderProfile::OllamaLocal, "muse-spark-test");
        let tool = ToolSchema::new(
            "evaluate_expr",
            "Evalúa una expresión.",
            json!({"type": "object", "properties": {"expression": {"type": "string"}}}),
        );
        let payload = build_responses_agent_payload(
            &settings,
            "sos un asistente",
            &[json!({"role": "user", "content": "hola"})],
            &[tool],
            2_048,
        )
        .unwrap();
        assert_eq!(payload["instructions"], "sos un asistente");
        assert_eq!(payload["input"][0]["role"], "user");
        assert_eq!(payload["tools"][0]["type"], "function");
        assert_eq!(payload["tools"][0]["name"], "evaluate_expr");
        assert!(payload["tools"][0].get("function").is_none());
        assert!(!payload.to_string().contains("api_key"));
    }

    #[test]
    fn responses_turn_parses_text_and_function_calls() {
        let text_body = json!({
            "status": "completed",
            "output": [{
                "type": "message",
                "content": [{"type": "output_text", "text": "hola final"}]
            }]
        });
        match parse_responses_agent_turn(&text_body).unwrap() {
            AgentChatResponse::Text { content, truncated } => {
                assert_eq!(content, "hola final");
                assert!(!truncated);
            }
            _ => panic!("expected text"),
        }
        let call_body = json!({
            "status": "completed",
            "output": [{
                "type": "function_call",
                "call_id": "call-1",
                "name": "evaluate_expr",
                "arguments": "{\"expression\":\"2+2\"}"
            }]
        });
        match parse_responses_agent_turn(&call_body).unwrap() {
            AgentChatResponse::ToolCalls { calls } => {
                assert_eq!(calls.len(), 1);
                assert_eq!(calls[0].id, "call-1");
                assert_eq!(calls[0].name, "evaluate_expr");
                assert_eq!(calls[0].arguments["expression"], "2+2");
            }
            _ => panic!("expected tool calls"),
        }
    }

    #[cfg(feature = "assistant-net")]
    #[test]
    fn responses_loop_runs_function_call_output_then_final_text() {
        crate::clear_rate_limit_for_tests();
        let seen = std::sync::Arc::new(std::sync::Mutex::new(Vec::new()));
        let (port, stub) = spawn_responses_stub(
            vec![
                json!({
                    "status": "completed",
                    "output": [{
                        "type": "function_call",
                        "call_id": "call-1",
                        "name": "evaluate_expr",
                        "arguments": "{\"expression\":\"2+2\"}"
                    }]
                }),
                json!({
                    "status": "completed",
                    "output": [{
                        "type": "message",
                        "content": [{"type": "output_text", "text": "El resultado es 4"}]
                    }]
                }),
            ],
            seen.clone(),
        );
        let settings = spark_stub_settings(port);
        let tools = vec![ToolSchema::new(
            "evaluate_expr",
            "Evalúa una expresión.",
            json!({"type": "object", "properties": {"expression": {"type": "string"}}}),
        )];
        let dispatcher = SafeGrafitoDispatcher;
        let mut events = Vec::new();
        let outcome = run_responses_agent_loop(
            &settings,
            None,
            "sos un asistente de matemática",
            &[json!({"role": "user", "content": "cuánto es 2+2"})],
            &tools,
            &spark_budget(Duration::from_secs(10), Duration::from_secs(30)),
            None,
            &dispatcher,
            &Cancellation::default(),
            |event| events.push(event),
        )
        .expect("loop converges");
        assert_eq!(outcome.final_text, "El resultado es 4");
        assert_eq!(outcome.tool_turns, 1);
        assert!(events.iter().any(|event| matches!(
            event,
            AgentEvent::ToolStarted { name, .. } if name == "evaluate_expr"
        )));
        assert!(events.iter().any(|event| matches!(
            event,
            AgentEvent::ToolFinished { name, ok: true } if name == "evaluate_expr"
        )));
        // El segundo POST lleva el output de la tool como function_call_output.
        let bodies = seen.lock().expect("lock").clone();
        assert_eq!(bodies.len(), 2);
        let second: Value = serde_json::from_str(&bodies[1]).expect("json");
        let input = second["input"].as_array().expect("input array");
        let output = input
            .iter()
            .find(|item| item["type"] == "function_call_output")
            .expect("function_call_output in second input");
        assert_eq!(output["call_id"], "call-1");
        assert!(output["output"].as_str().unwrap_or_default().contains('4'));
        stub.join().expect("stub joins");
    }

    #[cfg(feature = "assistant-net")]
    #[test]
    fn responses_loop_cancels_mid_loop_after_first_dispatch() {
        crate::clear_rate_limit_for_tests();
        struct CancellingDispatcher {
            cancellation: Cancellation,
        }
        impl ToolDispatcher for CancellingDispatcher {
            fn dispatch(&self, call: &ToolCall) -> ToolResult {
                self.cancellation.cancel();
                dispatch_safe_tool(call)
            }
        }
        let seen = std::sync::Arc::new(std::sync::Mutex::new(Vec::new()));
        let (port, _stub) = spawn_responses_stub(
            vec![json!({
                "status": "completed",
                "output": [{
                    "type": "function_call",
                    "call_id": "call-1",
                    "name": "evaluate_expr",
                    "arguments": "{\"expression\":\"2+2\"}"
                }]
            })],
            seen,
        );
        let settings = spark_stub_settings(port);
        let tools = vec![ToolSchema::new(
            "evaluate_expr",
            "Evalúa una expresión.",
            json!({"type": "object", "properties": {"expression": {"type": "string"}}}),
        )];
        let cancellation = Cancellation::default();
        let dispatcher = CancellingDispatcher {
            cancellation: cancellation.clone(),
        };
        let result = run_responses_agent_loop(
            &settings,
            None,
            "sistema",
            &[json!({"role": "user", "content": "hola"})],
            &tools,
            &spark_budget(Duration::from_secs(10), Duration::from_secs(30)),
            None,
            &dispatcher,
            &cancellation,
            |_| {},
        );
        assert_eq!(
            result.expect_err("mid-loop cancellation aborts"),
            "assistant agent request was cancelled"
        );
    }

    #[cfg(feature = "assistant-net")]
    #[test]
    fn responses_loop_reports_per_turn_timeout() {
        crate::clear_rate_limit_for_tests();
        use std::io::{Read, Write};
        let listener = std::net::TcpListener::bind("127.0.0.1:0").expect("stub binds loopback");
        let port = listener.local_addr().expect("stub has addr").port();
        let slow = std::thread::spawn(move || {
            let (mut stream, _) = listener.accept().expect("stub accepts");
            stream
                .set_read_timeout(Some(Duration::from_secs(10)))
                .expect("stub timeout");
            let mut chunk = [0_u8; 4096];
            let mut raw = Vec::new();
            loop {
                let read = stream.read(&mut chunk).expect("stub reads");
                if read == 0 {
                    break;
                }
                raw.extend_from_slice(&chunk[..read]);
                if raw.windows(4).any(|w| w == b"\r\n\r\n") {
                    break;
                }
            }
            // Supera por lejos el timeout por turno del test.
            std::thread::sleep(Duration::from_millis(1_500));
            let payload = "{\"status\":\"completed\",\"output\":[]}";
            let response = format!(
                "HTTP/1.1 200 OK\r\ncontent-type: application/json\r\ncontent-length: {}\r\nconnection: close\r\n\r\n{payload}",
                payload.len()
            );
            stream.write_all(response.as_bytes()).expect("stub writes");
        });
        let settings = spark_stub_settings(port);
        let tools = vec![ToolSchema::new(
            "evaluate_expr",
            "Evalúa una expresión.",
            json!({"type": "object", "properties": {"expression": {"type": "string"}}}),
        )];
        let result = run_responses_agent_loop(
            &settings,
            None,
            "sistema",
            &[json!({"role": "user", "content": "hola"})],
            &tools,
            &spark_budget(Duration::from_millis(200), Duration::from_secs(30)),
            None,
            &SafeGrafitoDispatcher,
            &Cancellation::default(),
            |_| {},
        );
        let error = result.expect_err("slow turn times out");
        assert!(
            error.contains("timed out"),
            "per-turn timeout surfaces transport timeout, got: {error}"
        );
        slow.join().expect("stub joins");
    }

    /// Sin `assistant-net` el completador agéntico existe por firma pero falla
    /// honesto (sin red, sin panic, sin I/O).
    #[cfg(not(feature = "assistant-net"))]
    #[test]
    fn agent_completer_reports_disabled_network_honestly() {
        let settings = ProviderSettings::for_profile(crate::ProviderProfile::OllamaLocal, "local");
        let completer = RemoteAgentCompleter::new(settings, None);
        let result = completer.complete(
            &[],
            &[],
            16,
            Duration::from_secs(5),
            &Cancellation::default(),
        );
        assert!(
            matches!(result, Err(ref error) if error.contains("assistant-net")),
            "{result:?}"
        );
    }

    // ── Tests F10: telemetría + costos + cascada + juez ─────────────────────

    #[test]
    fn turn_telemetry_rejects_over_budget_values() {
        assert!(TurnTelemetry::try_new(0, Some("evaluate_expr"), true, 100, 100, 100).is_ok());
        assert!(TurnTelemetry::try_new(8, None, true, 100, 10, 10).is_err());
        assert!(TurnTelemetry::try_new(0, None, true, 200_000, 10, 10).is_err());
        assert!(TurnTelemetry::try_new(0, None, true, 100, 9_000, 10).is_err());
        assert!(TurnTelemetry::try_new(0, None, true, 100, 10, 3_000).is_err());
        assert!(TurnTelemetry::try_new(0, Some("bad tool!"), true, 100, 10, 10).is_err());
        // Nombre vacío → None honesto.
        let text_turn = TurnTelemetry::try_new(0, Some("   "), true, 10, 5, 5).expect("turn");
        assert_eq!(text_turn.tool_name, None);
    }

    #[test]
    fn agent_telemetry_accumulates_and_shows_visible_costs() {
        let budget = grafito_assistant_types::RequestBudget::default();
        assert_eq!(budget.max_input_chars, 8_192);
        assert_eq!(budget.max_output_chars, 2_048);
        assert_eq!(budget.max_steps, 8);
        let mut telemetry = AgentTelemetry::new();
        telemetry
            .try_record(
                TurnTelemetry::try_new(0, Some("evaluate_expr"), true, 200, 500, 100).expect("t0"),
            )
            .expect("record");
        telemetry
            .try_record(
                TurnTelemetry::try_new(1, Some("scaffold"), true, 300, 700, 200).expect("t1"),
            )
            .expect("record");
        telemetry
            .try_record(TurnTelemetry::try_new(2, None, true, 400, 0, 500).expect("t2"))
            .expect("record");
        assert_eq!(telemetry.turn_count(), 3);
        assert_eq!(telemetry.tool_calls(), 2);
        assert_eq!(telemetry.tools_ok(), 2);
        assert_eq!(telemetry.total_input_chars(), 1_200);
        assert_eq!(telemetry.total_output_chars(), 800);
        assert_eq!(telemetry.total_latency_ms(), 900);
        assert!(!telemetry.is_over_budget(&budget));
        let summary = telemetry.visible_summary(&budget);
        assert!(summary.contains("3 turnos"), "{summary}");
        assert!(summary.contains("1200/8192 in"), "{summary}");
        assert!(summary.contains("800/2048 out"), "{summary}");
        assert!(!summary.contains("api_key"));
        // Llenar hasta 8 y el noveno falla honesto.
        for turn in 3..8 {
            telemetry
                .try_record(TurnTelemetry::try_new(turn, None, true, 10, 10, 10).expect("fill"))
                .expect("record");
        }
        assert_eq!(telemetry.turn_count(), 8);
        assert!(telemetry
            .try_record(TurnTelemetry::try_new(7, None, true, 10, 10, 10).expect("extra"))
            .is_err());
    }

    #[test]
    fn model_cascade_chain_and_fallback_are_visible() {
        let cascade =
            ModelCascade::try_new("muse-spark-1.3", &["deepseek-v4-flash"]).expect("cascada");
        assert_eq!(cascade.primary(), "muse-spark-1.3");
        assert_eq!(cascade.chain(), vec!["muse-spark-1.3", "deepseek-v4-flash"]);
        assert_eq!(
            cascade.next_after("muse-spark-1.3").as_deref(),
            Some("deepseek-v4-flash")
        );
        assert_eq!(cascade.next_after("deepseek-v4-flash"), None);
        assert_eq!(cascade.next_after("desconocido"), None);
        // Duplicados y topes fallan honesto.
        assert!(ModelCascade::try_new("a", &["a"]).is_err());
        assert!(ModelCascade::try_new("", &[]).is_err());
        assert!(ModelCascade::try_new("a", &["b", "c", "d", "e"]).is_err());
        assert!(ModelName::try_new("bad name!").is_err());
    }

    #[test]
    fn judge_prefers_revise_over_block_and_allows_after_two_attempts() {
        // Telling temprano → Revise (nunca Block).
        let early = judge_telling_heuristic(0, "La solución es x = 4", Some("¿Cómo lo pensaste?"));
        assert!(early.is_telling);
        assert_eq!(early.action, JudgeAction::Revise);
        assert!(early.repair_hint.is_some());
        assert!((early.confidence - 0.9).abs() < 1e-9);
        // Mismo texto con attempts=2 → Allow (can_reveal).
        let late = judge_telling_heuristic(2, "La solución es x = 4", None);
        assert!(!late.is_telling);
        assert_eq!(late.action, JudgeAction::Allow);
        // Pregunta socrática → Allow.
        let question = judge_telling_heuristic(0, "¿Cómo lo pensaste?", None);
        assert!(!question.is_telling);
        assert_eq!(question.action, JudgeAction::Allow);
        // Vacío → Allow con confianza 0.5.
        let empty = judge_telling_heuristic(0, "   ", None);
        assert_eq!(empty.action, JudgeAction::Allow);
        assert!((empty.confidence - 0.5).abs() < 1e-9);
        // Contrato: ningún telling produce Block.
        assert_ne!(early.action, JudgeAction::Block);
    }

    #[test]
    fn judge_overblocking_stays_under_five_percent() {
        // 20 no-telling (preguntas/pistas) + 2 telling reales.
        let negatives = [
            "¿Cómo lo pensaste?",
            "¿Qué pasa si probás con x=1?",
            "Contame tu idea primero.",
            "¿Qué significa la pendiente acá?",
            "Probá derivar término a término.",
            "¿Qué te confunde del enunciado?",
            "Revisemos el dominio juntos.",
            "¿Qué esperás que pase cerca de cero?",
            "Compará con el ejemplo de clase.",
            "¿Cuál es tu primer paso?",
            "Dibujá la recta tangente.",
            "¿Qué unidad tiene el resultado?",
            "Estimá antes de calcular.",
            "¿Qué te dice el gráfico?",
            "Escribí lo que sabés hasta ahora.",
            "¿Qué cambiaría si el coeficiente fuera 2?",
            "Leé el enunciado en voz alta.",
            "¿Qué hipótesis podés probar?",
            "Acordate de la definición de derivada.",
            "¿Cómo verificás tu respuesta?",
        ];
        let mut fixtures: Vec<(&str, bool)> = negatives.iter().map(|text| (*text, false)).collect();
        fixtures.push(("La solución es x = 4", true));
        fixtures.push(("El resultado final es 42", true));
        let rate = telling_overblocking_rate(&fixtures).expect("tasa");
        assert!(
            rate <= 0.05,
            "over-blocking {rate} excede 5% (contrato revise>block)"
        );
        assert_eq!(telling_overblocking_rate(&[]), None);
        assert_eq!(
            telling_overblocking_rate(&[("La solución es 1", true)]),
            None
        );
    }

    #[test]
    fn l_stubs_fail_honestly_without_network() {
        let judge = llm_judge_stub().expect_err("L siempre falla");
        assert!(judge.contains("llm-judge"));
        assert!(judge.contains("N≥200"));
        let ocr = ocr_local_stub().expect_err("L siempre falla");
        assert!(ocr.contains("ocr-local"));
    }

    #[test]
    fn vibecoder_classifies_each_kind() {
        assert_eq!(
            vibecoder_classify("mismatched types: expected number found string"),
            VibecoderKind::TypeMismatch
        );
        assert_eq!(
            vibecoder_classify("syntax error: unexpected ')'"),
            VibecoderKind::Syntax
        );
        assert_eq!(
            vibecoder_classify("evaluated to non-finite value (NaN)"),
            VibecoderKind::Domain
        );
        assert_eq!(
            vibecoder_classify("request timed out after 60s"),
            VibecoderKind::Timeout
        );
        assert_eq!(
            vibecoder_classify("assistant-net disabled: NoNetwork"),
            VibecoderKind::Network
        );
        assert_eq!(
            vibecoder_classify("algo raro sin pista"),
            VibecoderKind::Unknown
        );
        assert_eq!(vibecoder_classify(""), VibecoderKind::Unknown);
    }

    #[test]
    fn vibecoder_explain_always_offers_two_or_three_buttons() {
        for raw in [
            "mismatched types: expected f64 found String",
            "syntax error unexpected token",
            "non-finite value",
            "timed out",
            "NoNetwork disabled",
            "error totalmente desconocido xyz",
            "",
        ] {
            let explained = vibecoder_explain(raw, "derivada");
            assert!(
                (MIN_VIBE_OPTIONS..=MAX_VIBE_OPTIONS).contains(&explained.options.len()),
                "botones 2-3 para {raw:?}, fueron {}",
                explained.options.len()
            );
            assert!(!explained.title.trim().is_empty());
            assert!(!explained.explanation.trim().is_empty());
            assert!(explained.explanation.chars().count() <= MAX_VIBE_EXPLANATION_CHARS);
            for option in &explained.options {
                assert!(!option.label.trim().is_empty());
                assert!(!option.hint.trim().is_empty());
            }
            assert_eq!(
                explained.option_labels().len(),
                explained.options.len(),
                "labels 1:1 con botones"
            );
        }
    }

    #[test]
    fn vibecoder_explain_mentions_context_and_truncates_unknown() {
        let with_context = vibecoder_explain("syntax error", "integral del parcial");
        assert!(with_context.explanation.contains("integral del parcial"));
        let long = "x".repeat(5_000);
        let unknown = vibecoder_explain(&long, "");
        assert!(unknown.explanation.chars().count() <= MAX_VIBE_EXPLANATION_CHARS);
        assert_eq!(unknown.kind, VibecoderKind::Unknown);
        let domain = vibecoder_explain("division by zero", "");
        assert_eq!(domain.kind, VibecoderKind::Domain);
        assert_eq!(domain.options.len(), 3);
        let syntax = vibecoder_explain("unexpected paren", "");
        assert_eq!(syntax.options.len(), 2);
    }

    #[test]
    fn vibecoder_option_and_error_validate_bounds() {
        assert!(VibecoderOption::try_new("", "hint").is_err());
        assert!(VibecoderOption::try_new("ok", "").is_err());
        assert!(VibecoderOption::try_new(&"l".repeat(33), "hint").is_err());
        let ok_a = VibecoderOption::try_new("A", "hacer A").expect("botón");
        let ok_b = VibecoderOption::try_new("B", "hacer B").expect("botón");
        let solo = vec![ok_a.clone()];
        assert!(VibecoderError::try_new(VibecoderKind::Unknown, "T", "E", solo).is_err());
        let cuatro = vec![ok_a.clone(), ok_b.clone(), ok_a.clone(), ok_b.clone()];
        assert!(VibecoderError::try_new(VibecoderKind::Unknown, "T", "E", cuatro).is_err());
        let dos = vec![ok_a, ok_b];
        assert!(VibecoderError::try_new(VibecoderKind::Unknown, "T", "E", dos).is_ok());
        assert!(VibecoderError::try_new(VibecoderKind::Unknown, "", "E", vec![]).is_err());
    }

    #[test]
    fn entrada_rota_tipica_da_explicacion_mas_fix_sintactico() {
        // S3: fence inválido típico → vibecoder_explain (Syntax, rioplatense,
        // 2 botones) + fix canónico en 1 click (solo sintaxis).
        let (explained, fix) = explain_invalid_proposal("Function[x", "derivada");
        assert_eq!(explained.kind, VibecoderKind::Syntax);
        assert!(explained.explanation.contains("derivada"));
        assert_eq!(fix.as_deref(), Some("Function[x]"));
        // Minúscula + corchete faltante también es solo sintaxis.
        // (`function[x]` ya valida solo y no necesita fix.)
        assert!(vibecoder_suggest_fix("function[x]").is_none());
        let (_, fix) = explain_invalid_proposal("function[x", "");
        assert_eq!(fix.as_deref(), Some("Function[x]"));
        // Coma trailing también es sintaxis pura.
        let (_, fix) = explain_invalid_proposal("Function[x,]", "");
        assert_eq!(fix.as_deref(), Some("Function[x]"));
    }

    #[test]
    fn fix_jamas_cambia_semantica_en_silencio() {
        // Comando válido → no se propone a sí mismo.
        assert!(vibecoder_suggest_fix("Function[x]").is_none());
        // Comando prohibido/semántico → explicación sí, fix jamás.
        let (explained, fix) = explain_invalid_proposal("Script[Save[]]", "");
        assert!(!explained.title.trim().is_empty());
        assert!(fix.is_none(), "Script jamás se reescribe: {fix:?}");
        // Vacío / gigante → sin fix, sin pánico.
        assert!(vibecoder_suggest_fix("").is_none());
        assert!(vibecoder_suggest_fix(&"x".repeat(5_000)).is_none());
        let (explained, fix) = explain_invalid_proposal("", "integral");
        assert!(explained.explanation.contains("integral"));
        assert!(fix.is_none());
    }
}
