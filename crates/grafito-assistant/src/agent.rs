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
use grafito_agent::ledger::JSpaceLedger;
use grafito_agent::loop_engine::{
    run_agent, run_agent_with_ledger, AgentBudget, AgentChatResponse, AgentCompleter, AgentOutcome,
    Cancellation, ToolDispatcher,
};
use grafito_agent::schema::{ToolCall, ToolResult, ToolSchema};
use grafito_agent::AgentEvent;
use serde_json::{json, Value};
use std::sync::mpsc::Receiver;
use std::thread::JoinHandle;
use std::time::Duration;

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
    let handle = std::thread::spawn(move || {
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
    });
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
    let handle = std::thread::spawn(move || {
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
    });
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

fn dispatch_safe_tool(call: &ToolCall) -> ToolResult {
    if let Some(rejected) = reject_oversized_string_args(call) {
        return rejected;
    }
    match call.name.as_str() {
        "evaluate_expr" => evaluate_expr_tool(call),
        "grafito_docs" => grafito_docs_tool(call),
        "ask_user" => ToolResult::text(
            &call.id,
            false,
            "ask_user requires an explicit user answer in the Grafito chat; repeated it as a clarifying question instead".to_string(),
        ),
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
    let Some(expression) = string_arg(call, "expression") else {
        return ToolResult::text(
            &call.id,
            false,
            "evaluate_expr requires an 'expression' string",
        );
    };
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

/// generate_animation(template, concept, params) — valida AnimRequest sin ejecutar motor.
fn generate_animation_tool(call: &ToolCall) -> ToolResult {
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
    let payload = json!({
        "template": template,
        "concept": normalized_concept,
        "params": params_map,
        "export": request.export.as_str(),
        "canvas": request.canvas,
        "protocol_version": grafito_anim::protocol::ANIM_PROTOCOL_VERSION,
        "note": "solicitud validada; el motor de animación se ejecuta en la capa UI tras aprobación explícita"
    });
    ToolResult::text(&call.id, true, payload.to_string())
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

/// Schema de `generate_animation(template, concept, params)`.
///
/// `canvas`/`width`/`height` son opcionales; si vienen del LLM se usan con validación
/// 64..=4096, con fallback a 640x480.
pub fn generate_animation_tool_schema() -> ToolSchema {
    ToolSchema::new(
        "generate_animation",
        "Valida y propone una solicitud de animación didáctica (template, concept, params) sin ejecutar el motor; usa protocolo AnimRequest.",
        json!({
            "type": "object",
            "properties": {
                "template": {"type": "string", "description": "Plantilla opcional: derivative-slope, integral-area, taylor-series, conformal-map, pitagoras, auto"},
                "concept": {"type": "string", "description": "Concepto en lenguaje natural, ej. derivada como pendiente"},
                "params": {"type": "object", "description": "Mapa opcional de parámetros numéricos finitos", "additionalProperties": {"type": "number"}},
                "canvas": {"type": "array", "description": "Resolución opcional [width, height] 64..4096", "items": {"type": "integer"}, "minItems": 2, "maxItems": 2},
                "width": {"type": "integer", "description": "Ancho opcional 64..4096 (fallback 640)"},
                "height": {"type": "integer", "description": "Alto opcional 64..4096 (fallback 480)"}
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
            "Hace una única pregunta corta de aclaración matemática al usuario cuando falta un valor obligatorio.",
            json!({
                "type": "object",
                "properties": {"question": {"type": "string"}},
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

/// Envía una petición agéntica y devuelve texto final o llamadas de herramienta.
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
    if crate::remote_protocol(settings) != crate::RemoteProtocol::OpenAiChatCompletions {
        return Err("assistant agent requires an OpenAI-compatible chat endpoint".into());
    }
    let payload = build_agent_payload(settings, messages, tools, max_output_tokens)?;
    let client = crate::shared_http_client()?;
    let mut call = client
        .post(crate::chat_completion_endpoint(settings)?)
        .json(&payload)
        .timeout(timeout);
    if let Some(key) = api_key {
        if key.trim().is_empty() {
            return Err("assistant agent API key is unavailable".into());
        }
        call = call.bearer_auth(key);
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
        return Err(format!(
            "assistant agent returned HTTP {}",
            response.status().as_u16()
        ));
    }
    let response_bytes = crate::read_bounded_response_body(response, MAX_AGENT_RESPONSE_BYTES)?;
    let body: Value = serde_json::from_slice(&response_bytes)
        .map_err(|_| "assistant agent response JSON is invalid".to_string())?;
    parse_agent_completion(&body)
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
}
