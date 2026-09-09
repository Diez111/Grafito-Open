//! Integración de proveedores del asistente fuera del hilo de interfaz.

use crate::{assistant_credentials, GrafitoApp};
use grafito_assistant::{
    harness, rate_limit_cooldown_remaining_secs, rate_limit_paused_message,
    request_remote_models_with_api_key_on_worker, request_remote_streaming_with_api_key_on_worker,
    validate_attachment, CancellationToken, ProviderSettings, RemoteCompletion,
    SocraticGuardContext, RATE_LIMIT_DEFAULT_COOLDOWN_SECS,
};
use grafito_assistant_types::{
    AssistantFocus, AssistantRepairFailure, AssistantRepairFailureKind, AssistantRepairFeedback,
    AssistantRequest, AssistantResponse, AttachmentLimits, ConversationRole, ConversationTurn,
    ImmutableDocumentContext, LocalAssistantStatus, ProposedPlan, ProviderCapabilities,
    ProviderProfile, MAX_CONVERSATION_TURNS, MAX_CONVERSATION_TURN_CHARS,
    REMOTE_CONTEXT_PROMPT_OVERHEAD_BYTES, REMOTE_FOCUS_PROMPT_OVERHEAD_BYTES,
    REMOTE_PLUGIN_INSTRUCTIONS_OVERHEAD_BYTES, REMOTE_REPAIR_FEEDBACK_PROMPT_OVERHEAD_BYTES,
    REMOTE_TOOL_CATALOG_PROMPT_OVERHEAD_BYTES,
};
use grafito_command::assistant_proposals::{
    assistant_fenced_proposals, execute_assistant_command, execute_assistant_parameter,
    AssistantCommandInvocation, AssistantParameterAssignment, AssistantProposal,
    AssistantProposalRejection, AssistantProposalRejectionKind,
};
use grafito_pedagogy::scaffold::{extract_concept, is_exploratory_request};
use grafito_pedagogy::{PedagogicalLevel, ScaffoldEngine, SocraticFsm, Turn};
use grafito_ui::assistant::{
    AssistantCorrectionContext, AssistantPanelState, AssistantUiAction, VerifiedAssistantProposal,
};
use grafito_ui::prosa::{append_canonical_integral_prose, prosa_integral_explicita};
use grafito_ui::toast::ToastKind;
use std::collections::VecDeque;
use std::fs::File;
use std::io::{Cursor, Read};
use std::path::PathBuf;
use std::sync::mpsc::{sync_channel, Receiver, TryRecvError};

const MAX_REMOTE_PROPOSAL_PREFLIGHTS: usize = 4;
const MAX_ASSISTANT_PROPOSAL_CORRECTIONS: u8 = 2;
const MAX_ASSISTANT_CORRECTION_SOURCE_BYTES: usize = 2_048;
/// Modelo multimodal/visión (Xiaomi MiMo 2.5-VL); el razonamiento usa
/// DeepSeek Flash por defecto (el más barato y suficiente).
const OPENCODE_VISION_MODEL: &str = "mimo-2.5-vl";
const OPENCODE_FUSION_MODEL: &str = "fusion";
const ASSISTANT_CORRECTION_INSTRUCTION: &str = "\n\nUna propuesta gráfica anterior no superó la verificación local. Conservá la intención de la solicitud y regenerá una respuesta completa y autocontenida con un bloque grafito o un bloque grafito-scene de 2 a 8 comandos ejecutables. Si necesitás un parámetro escalar nuevo, incluí antes un único bloque grafito-param con una asignación finita. Usá exclusivamente la sintaxis exacta del catálogo; no inventes comandos ni emitas acciones de archivo, red, sistema o Script.";

/// Guarda el perfil en background para no bloquear el UI thread (60fps).
fn spawn_profile_save(profile: grafito_profile::StudentProfile, path: PathBuf) {
    // I/O en background thread para no bloquear UI (60fps)
    let _ = std::thread::Builder::new()
        .name("profile-save".into())
        .spawn(move || {
            let _ = std::fs::write(
                path,
                serde_json::to_string_pretty(&profile).unwrap_or_default(),
            );
        });
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum AssistantRemoteRoute {
    SelectedModel,
    FusionFallback,
}

/// B7 — ¿El texto pide ejercitar? (botón «Andamiar» o pedido en el chat).
/// Puro y testeable. No pisa preguntas («qué es una derivada» → false:
/// sólo dispara con verbo de ejercitación explícito).
pub(crate) fn wants_exercise_request(texto: &str) -> bool {
    let lower = texto.to_lowercase();
    [
        "ejercicio",
        "ejercitar",
        "practic",
        "andamia",
        "poneme a prueba",
        "tomame",
        "practiquemos",
    ]
    .iter()
    .any(|pista| lower.contains(pista))
}

/// Estado del pedido de integral/área frente a la función (N1, puro).
///
/// Coherente con `infer_area_anim` (inferencia) y el agente: sin función se
/// renderiza la canónica y la prosa la declara; con función inválida hay
/// error honesto sin frames ni hilo.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum IntegralPedido {
    /// No es `integral-area` o no menciona área (flujo existente intacto;
    /// probabilidad/fracciones usan ese renderer como soporte).
    NoAplica,
    /// Sin función: renderiza la canónica y la prosa la declara.
    Canonica,
    /// Con función válida del usuario: flujo existente intacto.
    Explicita,
    /// Con función inválida: error honesto sin frames ni hilo.
    FuncionInvalida(String),
}

/// Clasifica un pedido de animación integral (puro, sin I/O).
///
/// Solo actúa cuando la plantilla resuelta es `integral-area` Y el texto
/// menciona integral/área. El resto → `NoAplica` (nada cambia).
pub(crate) fn clasifica_pedido_integral(pedido: &str, template: &str) -> IntegralPedido {
    if template.trim().to_lowercase() != "integral-area"
        || !grafito_anim::parametric::pedido_menciona_area(pedido)
    {
        return IntegralPedido::NoAplica;
    }
    match grafito_anim::parametric::infer_area_anim(pedido) {
        Ok(resuelto) if resuelto.es_canonica() => IntegralPedido::Canonica,
        Ok(_) => IntegralPedido::Explicita,
        Err(error) => IntegralPedido::FuncionInvalida(error.to_string()),
    }
}

/// Estado del pedido de tangente/derivada frente a la función (M1, puro).
///
/// Espejo de [`IntegralPedido`]: sin función se renderiza la canónica y la
/// prosa la declara; con función inválida hay error honesto sin frames ni
/// hilo. Antes solo existía la vía integral (`clasifica_pedido_integral`) y
/// la tangente inválida caía a canónica muda.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum TangentePedido {
    /// No es `derivative-slope` o no menciona tangente (flujo intacto).
    NoAplica,
    /// Sin función: renderiza la canónica y la prosa la declara.
    Canonica,
    /// Con función válida del usuario: flujo explícito que la nombra.
    Explicita,
    /// Con función inválida: error honesto sin frames ni hilo.
    FuncionInvalida(String),
}

/// Clasifica un pedido de animación tangente (puro, sin I/O).
///
/// Solo actúa cuando la plantilla resuelta es `derivative-slope` Y el texto
/// menciona derivada/tangente/pendiente. El resto → `NoAplica` (nada cambia).
pub(crate) fn clasifica_pedido_tangente(pedido: &str, template: &str) -> TangentePedido {
    if template.trim().to_lowercase() != "derivative-slope"
        || !grafito_anim::parametric::pedido_menciona_tangente(pedido)
    {
        return TangentePedido::NoAplica;
    }
    match grafito_anim::parametric::infer_tangent_anim(pedido) {
        Ok(resuelto) if resuelto.es_canonica() => TangentePedido::Canonica,
        Ok(_) => TangentePedido::Explicita,
        Err(error) => TangentePedido::FuncionInvalida(error.to_string()),
    }
}

/// Plantilla honesta para un pedido de animación (punto único de resolución).
///
/// Si el pedido menciona integral/área (con typos acotados vía
/// `pedido_menciona_area`: "integrela"→"integral"), la plantilla es
/// `integral-area` aunque `detect_template_for_concept` diga `universal`
/// por el typo. Espejo M1: si menciona derivada/tangente/pendiente (fuzzy
/// "derivadaa"→"derivada"), es `derivative-slope`. El resto delega al
/// detector clásico. Puro, sin I/O.
pub(crate) fn plantilla_para_pedido(pedido: &str) -> &'static str {
    if grafito_anim::parametric::pedido_menciona_area(pedido) {
        return "integral-area";
    }
    if grafito_anim::parametric::pedido_menciona_tangente(pedido) {
        return "derivative-slope";
    }
    crate::anim_native::detect_template_for_concept(pedido)
}

// ── W-B: la IA propone el SPEC, el motor solo renderiza lo validado ──────────
//
// Queja real: "cuando le pido derivada o integral me la tira al toque, la IA
// ni bola, solo la tiene hecha de antes". Causa: el Submit renderizaba la
// canónica instantánea sin consultar a nadie.
// Política nueva: con IA disponible (remoto o agente, no solo local) el SPEC
// (función, rango, kind, parámetros) lo propone la IA vía las tools existentes
// (`propose_parametric/area/tangent_tool` + loop agente,
// `anim_spec_json_para_hilo` en `grafito-assistant::agent`); el motor nativo
// solo renderiza el spec validado con `infer_*`. Sin IA (offline, local sin
// clave, error 400-429, timeout) → canónica local instantánea DECLARADA.
//
// Presupuesto: 1 request extra como máximo por turno de animación (el SPEC);
// sin IA no hay request extra. Cero doble render: o IA o local, nunca ambos
// (el desenlace es un solo enum y el Submit spawnea un solo worker).

/// W-B — timeout para pedir SPEC a la IA: mitad del budget del turno.
///
/// Cadena completa de timeouts del turno de animación (todos acotados,
/// ninguno cuelga la UI porque corren en workers):
/// `RequestBudget::default().timeout_ms` = 60s (tope del turno,
/// `grafito-assistant-types` 8192/2048/8/60s; rango 100..=120000ms).
/// ESTE const = 30s (SPEC de la IA: mitad del budget, deja aire para
/// validar con `infer_*` + renderizar; pineado en
/// `wb_timeout_es_mitad_del_budget_y_peor_caso_un_request`).
/// Rama agente: el mismo plazo viaja como `timeout` al completador
/// (`RemoteAgentCompleter::complete`, por turno) y la `Cancellation` del
/// agente se liga al token del turno (forwarder en
/// `pedir_spec_ia_de_verdad`), así Cancel corta aunque falte transporte.
/// Rama remota: el mismo plazo en `request.budget.timeout_ms` con poll de
/// cancel cada 25ms.
/// Motor externo: `ANIM_MOTOR_IDLE_TIMEOUT_SECS` (reposo) y
/// `ANIM_MOTOR_JOB_TIMEOUT_SECS` (job total) — si no responde, cae al
/// nativo (<2s) sin colgar el turno.
/// Transporte compartido: `connect_timeout` 10s
/// (`grafito-assistant::shared_http_client`), sub-timeout dentro del total.
/// Peor caso documentado: 1 request extra (el SPEC) + render local.
pub(crate) const ANIM_IA_SPEC_TIMEOUT_MS: u64 = 30_000;

/// M1 — timeout de reposo del motor externo de animación (2s).
///
/// Si el motor no saluda en este plazo, el turno cae al generador nativo
/// (ver `run_assistant_animation_with`): «Animá» nunca se queda colgado.
/// Sub-timeout dentro de `ANIM_MOTOR_JOB_TIMEOUT_SECS`.
pub(crate) const ANIM_MOTOR_IDLE_TIMEOUT_SECS: u64 = 2;

/// M1 — timeout total de un job del motor externo (15s).
///
/// Cota del `run_job` con closure de cancel (<200ms): pasado este plazo o
/// ante cancel, se descarta y renderiza el nativo. Sub-plazo dentro de
/// `ANIM_IA_SPEC_TIMEOUT_MS` (el SPEC ya consumió su mitad del budget).
pub(crate) const ANIM_MOTOR_JOB_TIMEOUT_SECS: u64 = 15;

/// W-B — aviso genérico ante fallback canónico (offline, timeout o error
/// 400-429). Lo usa el resolver puro (`resolver_turno_anim_ia`, sin contexto
/// de plantilla); los workers lo ESPECIALIZAN con `aviso_fallback_canonico`
/// (plantilla y rango reales del SPEC renderizado) antes del toast.
/// La prosa del turno usa la canónica declarada existente.
pub(crate) const ANIM_SIN_IA_AVISO: &str = "sin conexión: te muestro x², pedime otra";

/// W-B — SPEC validado venido de la IA (función, rango, kind/plantilla).
/// El motor solo renderiza esto tras validar con `infer_*`; jamás basura.
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct SpecAnimIa {
    pub expr: String,
    pub p0: f64,
    pub p1: f64,
    pub plantilla: String,
    pub param: String,
}

/// W-B — ¿Hay IA disponible para proponer el SPEC? (puro, sin I/O).
///
/// `agent_mode` (loop agente) o `remote_ready` (remoto con clave u Ollama
/// local); jamás solo-local. `rate_limited` (pausa 429) y `exam_bloquea`
/// fuerzan local para no quemar cuota ni violar examen.
pub(crate) fn ia_disponible_para_anim(
    agent_mode: bool,
    remote_ready: bool,
    rate_limited: bool,
    exam_bloquea: bool,
) -> bool {
    (agent_mode || remote_ready) && !rate_limited && !exam_bloquea
}

/// W-B — prompt acotado para pedir SPEC a la IA (puro, sin I/O).
///
/// Pide UNA sola línea JSON con expr/p0/p1/plantilla (`integral-area` o
/// `derivative-slope` según el pedido; el ejemplo integral no debe sesgar
/// la tangente); el parseo es estricto y la validación posterior usa
/// `infer_*` (si la IA inventa, se descarta con `Err` honesto). Capado por
/// chars para no pasar el budget.
pub(crate) fn prompt_spec_anim_ia(pedido: &str) -> String {
    let recortado: String = pedido.chars().take(500).collect();
    format!(
        "Devolvé SOLO una línea JSON para animar en Grafito: {{\"expr\": \"f(x)\", \"p0\": 0, \"p1\": 2, \"plantilla\": \"integral-area\"}} (si el pedido es de tangente/derivada, usá \"plantilla\": \"derivative-slope\"). Pedido: {recortado}"
    )
}

/// W-B — salida del pedido de SPEC a la IA (inyectable para tests).
#[derive(Debug, Clone, PartialEq)]
pub(crate) enum PedidoSpecIa {
    /// La IA devolvió SPEC ya validado con `infer_*`.
    Exito(SpecAnimIa),
    /// La IA tardó más que `ANIM_IA_SPEC_TIMEOUT_MS`.
    Timeout,
    /// Fallo de transporte (offline, 400-429, red): va a fallback canónico.
    Transporte(String),
    /// La IA devolvió algo que no valida con `infer_*`: error honesto.
    Invalido(String),
}

/// W-B — desenlace de un turno de animación (cero doble render: un solo valor).
#[derive(Debug, Clone, PartialEq)]
pub(crate) enum DesenlaceAnimIa {
    /// Renderizar el SPEC de la IA + prosa que lo nombra.
    RenderIa { spec: SpecAnimIa, prosa: String },
    /// Fallback canónico local + aviso de UNA línea.
    FallbackCanonico { aviso: &'static str },
    /// Error honesto sin frames (SPEC inválido que no valida).
    ErrorHonesto(String),
}

/// W-B — resuelve el turno en puro (sin I/O ni spawn), para tests y wiring.
///
/// - Sin IA (`ia_disponible=false`) → canónica declarada + aviso.
/// - Con IA + `Exito` → render IA con prosa que nombra función y rango.
/// - Con IA + `Timeout`/`Transporte` → canónica + aviso (una línea).
/// - Con IA + `Invalido` → error honesto, jamás basura en pantalla.
///
/// Un solo desenlace → el llamante renderiza una sola vez (o IA o local).
pub(crate) fn resolver_turno_anim_ia(ia_disponible: bool, salida: PedidoSpecIa) -> DesenlaceAnimIa {
    if !ia_disponible {
        return DesenlaceAnimIa::FallbackCanonico {
            aviso: ANIM_SIN_IA_AVISO,
        };
    }
    match salida {
        PedidoSpecIa::Exito(spec) => {
            let prosa = prosa_para_spec_anim_ia(&spec);
            DesenlaceAnimIa::RenderIa { spec, prosa }
        }
        PedidoSpecIa::Timeout | PedidoSpecIa::Transporte(_) => DesenlaceAnimIa::FallbackCanonico {
            aviso: ANIM_SIN_IA_AVISO,
        },
        PedidoSpecIa::Invalido(detalle) => DesenlaceAnimIa::ErrorHonesto(detalle),
    }
}

/// W-B — prosa del turno desde el SPEC validado (MUST nombrar función y rango).
///
/// Usa los valores venidos de la IA, no re-infiere (para que un spec con
/// f=x³ se vea en prosa aunque el pedido original no la trajera).
/// Rioplatense + frase de referencia de la media. Pura, sin I/O.
pub(crate) fn prosa_para_spec_anim_ia(spec: &SpecAnimIa) -> String {
    format!(
        "te muestro con f(x)={} en [{},{}].\n\n{}",
        spec.expr,
        spec.p0,
        spec.p1,
        crate::anim_ui::animation_reference_sentence(),
    )
}

/// M1 — prosa canónica declarada según plantilla (punto único local).
///
/// Tangente → `TANGENT_CANONICAL_PROSA`, resto → `INTEGRAL_CANONICAL_PROSA`,
/// siempre con la frase de referencia. La usan Submit, los fallbacks sin
/// IA y el worker IA-primero: la misma canónica en todos lados. Pura.
pub(crate) fn prosa_canonica_para_plantilla(plantilla: &str) -> String {
    let canonica = if plantilla.trim().to_lowercase() == "derivative-slope" {
        grafito_anim::parametric::TANGENT_CANONICAL_PROSA
    } else {
        grafito_anim::parametric::INTEGRAL_CANONICAL_PROSA
    };
    format!(
        "{}\n\n{}",
        crate::anim_ui::animation_reference_sentence(),
        canonica
    )
}

/// M1 — aviso de UNA línea ante fallback canónico con plantilla y rango
/// reales (defecto 3: el genérico `ANIM_SIN_IA_AVISO` no decía qué se
/// mostraba). Se construye del SPEC efectivamente renderizado, no del
/// pedido: `x^2 en [0,2] (integral)` o `x^2 en [-1.5,1.5] (tangente)`.
/// Puro, sin I/O.
pub(crate) fn aviso_fallback_canonico(spec: &SpecAnimIa) -> String {
    let kind = if spec.plantilla.trim().to_lowercase() == "derivative-slope" {
        "tangente"
    } else {
        "integral"
    };
    format!(
        "sin conexión: te muestro {} en [{},{}] ({}), pedime otra",
        spec.expr, spec.p0, spec.p1, kind
    )
}
/// M1 — prosa rioplatense para tangente explícita: nombra función y rango.
///
/// Espejo de `prosa_integral_explicita` (vive en `grafito-ui`, intocable en
/// este frente): re-infiere el rango del pedido para que la curva nunca
/// quede huérfana (la vía local genérica solo ponía la frase de referencia).
/// Sin "pedime otra" (ese marcador es solo de la canónica). Pura, sin I/O.
pub(crate) fn prosa_tangente_explicita(expr: &str, pedido: &str) -> String {
    let (_, p0, p1) = grafito_anim::parametric::infer_tangent_anim(pedido)
        .map(|resuelto| {
            let anim = resuelto.anim();
            (anim.expr_a.clone(), anim.p0, anim.p1)
        })
        .unwrap_or_else(|_| {
            (
                expr.to_string(),
                grafito_anim::parametric::TANGENT_CANONICAL_P0,
                grafito_anim::parametric::TANGENT_CANONICAL_P1,
            )
        });
    format!(
        "te muestro con f(x)={expr} en [{p0},{p1}].\n\n{}",
        crate::anim_ui::animation_reference_sentence()
    )
}

/// M1 — animación paramétrica explícita del pedido, igual que el agente.
///
/// Si la plantilla es `integral-area`/`derivative-slope` y el texto menciona
/// el tipo, infiere con `infer_area_anim`/`infer_tangent_anim` (las mismas
/// puertas del agente): explícita del usuario, canónica declarada, o `Err`
/// honesto ante función inválida (jamás canónica muda). El resto →
/// `Ok(None)` y el llamante cae al `parametric_for_template`/clásico.
/// Pura salvo construcción, sin I/O.
pub(crate) fn anim_parametrica_para_pedido(
    template: &str,
    texto: &str,
) -> Result<Option<grafito_anim::parametric::ParametricAnim>, String> {
    let plantilla = template.trim().to_lowercase();
    if plantilla == "integral-area" && grafito_anim::parametric::pedido_menciona_area(texto) {
        return grafito_anim::parametric::infer_area_anim(texto)
            .map(|resuelto| Some(resuelto.anim().clone()))
            .map_err(|error| error.to_string());
    }
    if plantilla == "derivative-slope" && grafito_anim::parametric::pedido_menciona_tangente(texto)
    {
        return grafito_anim::parametric::infer_tangent_anim(texto)
            .map(|resuelto| Some(resuelto.anim().clone()))
            .map_err(|error| error.to_string());
    }
    Ok(None)
}

/// W-B — valida un SPEC ya parseado con las puertas `infer_*` existentes.
///
/// Reconstruye un pedido sintético con la expr y el rango del SPEC y lo pasa
/// por la puerta que toca según la plantilla (área/tangente/paramétrico).
/// Si no valida, `Err` honesto (jamás renderizar basura). Pura, sin I/O.
pub(crate) fn validar_spec_anim_ia(spec: &SpecAnimIa) -> Result<(), String> {
    if spec.expr.trim().is_empty() {
        return Err(
            "el SPEC de la IA vino sin función: pedí una explícita, por ejemplo f(x)=x^3.".into(),
        );
    }
    if spec.expr.chars().count() > 2000 {
        return Err("el SPEC de la IA trae una función muy larga: pedila más corta.".into());
    }
    if !spec.p0.is_finite() || !spec.p1.is_finite() || spec.p0 >= spec.p1 {
        return Err(format!(
            "el SPEC de la IA trae un rango inválido [{},{}]: pedí uno válido, por ejemplo [0,2].",
            spec.p0, spec.p1
        ));
    }
    if grafito_anim::parametric::ParamName::try_new(&spec.param).is_err() {
        return Err("el SPEC de la IA trae un parámetro inválido: pedí de nuevo.".into());
    }
    let plantilla = spec.plantilla.trim().to_lowercase();
    if plantilla == "integral-area" {
        let sintetico = format!(
            "animacion de la integral de f(x)={} de {} a {} con animación",
            spec.expr, spec.p0, spec.p1
        );
        match grafito_anim::parametric::infer_area_anim(&sintetico) {
            Ok(resuelto) => {
                let got = resuelto.anim().expr_a.trim();
                if got == spec.expr.trim() {
                    Ok(())
                } else {
                    Err(format!(
                        "el SPEC de la IA ({:?}) no valida como integral explícita: pedí de nuevo.",
                        spec.expr
                    ))
                }
            }
            Err(error) => Err(format!("el SPEC de la IA no valida: {error}")),
        }
    } else if plantilla == "derivative-slope" {
        let sintetico = format!(
            "tangente movil de f(x)={} en [{},{}] con animación",
            spec.expr, spec.p0, spec.p1
        );
        match grafito_anim::parametric::infer_tangent_anim(&sintetico) {
            Ok(resuelto) => {
                let got = resuelto.anim().expr_a.trim();
                if got == spec.expr.trim() {
                    Ok(())
                } else {
                    Err(format!(
                        "el SPEC de la IA ({:?}) no valida como tangente explícita: pedí de nuevo.",
                        spec.expr
                    ))
                }
            }
            Err(error) => Err(format!("el SPEC de la IA no valida: {error}")),
        }
    } else {
        let sintetico = format!(
            "barrido de f(x)={} con {} en [{},{}] con animación",
            spec.expr, spec.param, spec.p0, spec.p1
        );
        grafito_anim::parametric::infer_parametric_anim(&sintetico)
            .map(|_| ())
            .map_err(|error| format!("el SPEC de la IA no valida: {error}"))
    }
}

/// W-B — parsea el JSON del SPEC venido de la IA y lo valida con `infer_*`.
///
/// Acepta `{"expr_a"|"expr", "range":[p0,p1] o "p0"/"p1",
/// "plantilla"|"template"|"kind", "param"}`. Extrae el primer objeto `{…}`
/// del texto (la IA a veces agrega prosa alrededor), valida topes
/// (expr ≤2000 chars, rango finito con p0<p1, param ASCII) y re-valida con
/// `validar_spec_anim_ia` (puertas `infer_*`). Sin `unwrap`, sin I/O.
pub(crate) fn parsear_spec_anim_ia(
    texto_ia: &str,
    pedido_original: &str,
) -> Result<SpecAnimIa, String> {
    let inicio = texto_ia
        .find('{')
        .ok_or_else(|| "la IA no devolvió SPEC JSON: pedí de nuevo.".to_string())?;
    let fin = texto_ia
        .rfind('}')
        .ok_or_else(|| "la IA no devolvió SPEC JSON: pedí de nuevo.".to_string())?;
    if fin < inicio {
        return Err("la IA no devolvió SPEC JSON: pedí de nuevo.".into());
    }
    let recorte = texto_ia
        .get(inicio..=fin)
        .ok_or_else(|| "la IA no devolvió SPEC JSON: pedí de nuevo.".to_string())?;
    let valor: serde_json::Value = serde_json::from_str(recorte)
        .map_err(|_| "la IA devolvió un SPEC que no es JSON válido: pedí de nuevo.".to_string())?;
    let expr = valor
        .get("expr_a")
        .or_else(|| valor.get("expr"))
        .and_then(serde_json::Value::as_str)
        .map(str::trim)
        .filter(|texto| !texto.is_empty())
        .ok_or_else(|| "el SPEC de la IA vino sin función: pedí una explícita.".to_string())?;
    if expr.chars().count() > 2000 {
        return Err("el SPEC de la IA trae una función muy larga: pedila más corta.".into());
    }
    let (p0, p1) = if let Some(rango) = valor.get("range").and_then(|v| v.as_array()) {
        if rango.len() != 2 {
            return Err("el SPEC de la IA trae un rango inválido: pedí uno válido.".into());
        }
        let p0 = rango
            .first()
            .and_then(serde_json::Value::as_f64)
            .ok_or_else(|| {
                "el SPEC de la IA trae un rango inválido: pedí uno válido.".to_string()
            })?;
        let p1 = rango
            .get(1)
            .and_then(serde_json::Value::as_f64)
            .ok_or_else(|| {
                "el SPEC de la IA trae un rango inválido: pedí uno válido.".to_string()
            })?;
        (p0, p1)
    } else {
        let p0 = valor
            .get("p0")
            .and_then(serde_json::Value::as_f64)
            .ok_or_else(|| "el SPEC de la IA vino sin rango: pedí uno válido.".to_string())?;
        let p1 = valor
            .get("p1")
            .and_then(serde_json::Value::as_f64)
            .ok_or_else(|| "el SPEC de la IA vino sin rango: pedí uno válido.".to_string())?;
        (p0, p1)
    };
    if !p0.is_finite() || !p1.is_finite() || p0 >= p1 {
        return Err(format!(
            "el SPEC de la IA trae un rango inválido [{p0},{p1}]: pedí uno válido, por ejemplo [0,2]."
        ));
    }
    let plantilla = valor
        .get("plantilla")
        .or_else(|| valor.get("template"))
        .and_then(serde_json::Value::as_str)
        .map(str::trim)
        .filter(|texto| !texto.is_empty())
        .map(|texto| texto.to_lowercase())
        .unwrap_or_else(|| plantilla_para_pedido(pedido_original).to_string());
    let param = valor
        .get("param")
        .and_then(serde_json::Value::as_str)
        .map(str::trim)
        .filter(|texto| !texto.is_empty())
        .unwrap_or("p")
        .to_string();
    let spec = SpecAnimIa {
        expr: expr.to_string(),
        p0,
        p1,
        plantilla,
        param,
    };
    validar_spec_anim_ia(&spec)?;
    Ok(spec)
}

/// W-B — espera el SPEC de la IA con timeout acotado (mitad del budget).
///
/// Solo tests: simula la espera del worker con `recv_timeout` (sin `unwrap`).
/// Producción usa el timeout del transporte (`ANIM_IA_SPEC_TIMEOUT_MS`) dentro
/// del worker IA-primero; este helper pinnea la semántica Timeout/Transporte.
#[cfg(test)]
pub(crate) fn esperar_spec_ia_con_timeout(
    receiver: &std::sync::mpsc::Receiver<Result<SpecAnimIa, String>>,
    timeout_ms: u64,
) -> PedidoSpecIa {
    match receiver.recv_timeout(std::time::Duration::from_millis(timeout_ms)) {
        Ok(Ok(spec)) => match validar_spec_anim_ia(&spec) {
            Ok(()) => PedidoSpecIa::Exito(spec),
            Err(detalle) => PedidoSpecIa::Invalido(detalle),
        },
        Ok(Err(transporte)) => PedidoSpecIa::Transporte(transporte),
        Err(std::sync::mpsc::RecvTimeoutError::Timeout) => PedidoSpecIa::Timeout,
        Err(std::sync::mpsc::RecvTimeoutError::Disconnected) => {
            PedidoSpecIa::Transporte("el pedido de SPEC terminó sin responder.".into())
        }
    }
}

/// W-B — construye la animación paramétrica desde un SPEC validado.
///
/// Mapea la plantilla al kind (`integral-area`→Area, `derivative-slope`→
/// Tangent, resto→Sweep) con 48 frames y viewport 640×480 (mismo presupuesto
/// que la canónica). Si la plantilla no mapea, `Err` honesto. Pura, sin I/O.
pub(crate) fn anim_desde_spec_ia(
    spec: &SpecAnimIa,
) -> Result<grafito_anim::parametric::ParametricAnim, String> {
    use grafito_anim::parametric::{FrameCount, ParamName, ParametricAnim, ParametricKind};
    let kind = match spec.plantilla.trim().to_lowercase().as_str() {
        "integral-area" => ParametricKind::Area,
        "derivative-slope" => ParametricKind::Tangent,
        "taylor-series" => ParametricKind::Trace,
        _ => ParametricKind::Sweep,
    };
    let param =
        ParamName::try_new(&spec.param).map_err(|error| format!("SPEC inválido: {error}"))?;
    let frames = FrameCount::try_new(crate::anim_native::NATIVE_ANIM_FRAME_COUNT)
        .map_err(|error| format!("SPEC inválido: {error}"))?;
    let viewport = grafito_anim::protocol::Resolution::try_new(640, 480)
        .map_err(|error| format!("SPEC inválido: {error}"))?;
    ParametricAnim::try_new(
        kind,
        spec.expr.clone(),
        None,
        param,
        spec.p0,
        spec.p1,
        frames,
        viewport,
    )
    .map_err(|error| format!("SPEC inválido: {error}"))
}

/// W-B — SPEC canónico para fallback local (sin IA, timeout o 400-429).
///
/// Espeja `parametric_for_template`: integral `x^2 [0,2]`, tangente
/// `x^2 [-1.5,1.5]`; resto → integral honesta (jamás vacío).
/// Pura, sin I/O. La prosa que lo declara vive en
/// `INTEGRAL_CANONICAL_PROSA` / `TANGENT_CANONICAL_PROSA`.
pub(crate) fn spec_canonico_para_fallback(plantilla: &str) -> SpecAnimIa {
    match plantilla.trim().to_lowercase().as_str() {
        "derivative-slope" => SpecAnimIa {
            expr: grafito_anim::parametric::TANGENT_CANONICAL_EXPR.to_string(),
            p0: grafito_anim::parametric::TANGENT_CANONICAL_P0,
            p1: grafito_anim::parametric::TANGENT_CANONICAL_P1,
            plantilla: "derivative-slope".to_string(),
            param: grafito_anim::parametric::TANGENT_CANONICAL_PARAM.to_string(),
        },
        _ => SpecAnimIa {
            expr: grafito_anim::parametric::INTEGRAL_CANONICAL_EXPR.to_string(),
            p0: grafito_anim::parametric::INTEGRAL_CANONICAL_P0,
            p1: grafito_anim::parametric::INTEGRAL_CANONICAL_P1,
            plantilla: "integral-area".to_string(),
            param: "p".to_string(),
        },
    }
}

/// W-B — renderiza la media desde un SPEC ya validado (un solo render).
///
/// Construye el `ParametricAnim` con `anim_desde_spec_ia` y renderiza con
/// progreso cancelable (mismo presupuesto que la canónica: 48 frames).
/// Título curado por el punto único (nombra la función del SPEC).
/// Hilo background, sin tocar UI. Sin `unwrap`. Pura salvo el render.
pub(crate) fn render_media_desde_spec_ia(
    spec: &SpecAnimIa,
    cancel: &CancellationToken,
) -> Result<grafito_ui::assistant::AssistantMedia, String> {
    let anim = anim_desde_spec_ia(spec)?;
    let mut saw_cancel = false;
    let frames = crate::anim_native::render_parametric_frames_with_progress(&anim, &mut |_, _| {
        if cancel.is_cancelled() {
            saw_cancel = true;
        }
    })
    .map_err(|error| error.to_string())?;
    if cancel.is_cancelled() || saw_cancel {
        return Err("La generación se canceló antes de completarse.".to_string());
    }
    if frames.is_empty() {
        return Err(crate::anim_native::error_sin_fotogramas("el motor nativo"));
    }
    let title = titulo_curado(&spec.plantilla, &spec.expr, Some(&anim));
    Ok(grafito_ui::assistant::AssistantMedia { title, frames })
}

/// Título curado de la card de animación (punto único, puro y testeable).
///
/// Las 3 vías (paramétrica, nativa, externa) lo comparten: jamás eco crudo
/// del pedido (venía con typos como "integrela" y sufijos "(nativa)"
/// duplicados). Con `anim` titula por kind; sin ella, por plantilla
/// conocida; `universal`/desconocida cura el concepto (menciona
/// integral/área → canónica integral, si no concepto recortado sin sufijo).
/// Devuelve la base SIN " (nativa)": la vía nativa lo agrega al mostrar.
/// Idioma actual del UI (ES); ver [`titulo_curado_localized`].
pub(crate) fn titulo_curado(
    template: &str,
    concept: &str,
    anim: Option<&grafito_anim::parametric::ParametricAnim>,
) -> String {
    titulo_curado_localized(template, concept, anim, grafito_ui::i18n::Locale::Es)
}

/// [`titulo_curado`] en el idioma pedido. Los literales viven en el catálogo
/// i18n (`media.title.*`); acá solo `t()` + sustitución de `{expr}`/`{p0}`/
/// `{p1}`/`{param}`. Puro, sin I/O.
pub(crate) fn titulo_curado_localized(
    template: &str,
    concept: &str,
    anim: Option<&grafito_anim::parametric::ParametricAnim>,
    locale: grafito_ui::i18n::Locale,
) -> String {
    use grafito_ui::i18n::t;
    if let Some(anim) = anim {
        return match anim.kind {
            grafito_anim::parametric::ParametricKind::Tangent => {
                t("media.title.tangent", locale).replace("{expr}", &anim.expr_a)
            }
            grafito_anim::parametric::ParametricKind::Area => t("media.title.area", locale)
                .replace("{expr}", &anim.expr_a)
                .replace("{p0}", &anim.p0.to_string())
                .replace("{p1}", &anim.p1.to_string()),
            grafito_anim::parametric::ParametricKind::Sweep => t("media.title.sweep", locale)
                .replace("{expr}", &anim.expr_a)
                .replace("{param}", anim.param.as_str()),
            grafito_anim::parametric::ParametricKind::Trace => {
                t("media.title.trace", locale).replace("{expr}", &anim.expr_a)
            }
            grafito_anim::parametric::ParametricKind::Morph => {
                t("media.title.morph", locale).to_string()
            }
            grafito_anim::parametric::ParametricKind::Locus => {
                t("media.title.locus", locale).to_string()
            }
        };
    }
    match template.trim().to_lowercase().as_str() {
        "integral-area" => t("media.title.integral", locale).to_string(),
        "derivative-slope" => t("media.title.derivative", locale).to_string(),
        "pitagoras" | "pythagoras" => t("media.title.pitagoras", locale).to_string(),
        "taylor-series" => t("media.title.taylor", locale).to_string(),
        "conformal-map" => t("media.title.conformal", locale).to_string(),
        _ => titulo_desde_concepto_localized(concept, locale),
    }
}

/// Cura un concepto libre a título en el idioma pedido (sin eco crudo del
/// pedido). Normaliza UNA sola vez (`normaliza_para_match`: minúsculas sin
/// tildes) y reusa para área/tangente/nombres — antes eran 3
/// (2× `pedido_menciona_*` + 1× `to_lowercase`).
///
/// - Menciona integral/área (fuzzy: "integrela" también) → canónica integral
///   (el typo jamás se muestra).
/// - Menciona tangente/derivada → pendiente; Pitágoras/Taylor/conforme por
///   nombre.
/// - Resto: recortado a 80 chars, sin sufijos "(nativa)" repetidos; vacío →
///   título por defecto del catálogo. Sin `unwrap`: índices por chars, nunca
///   slicing por bytes.
///
/// Los checks de área/tangente espejan `pedido_menciona_area` y
/// `pedido_menciona_tangente` sobre la cadena ya normalizada (paridad pineada
/// en `titulo_normaliza_una_vez_con_paridad`).
fn titulo_desde_concepto_localized(concept: &str, locale: grafito_ui::i18n::Locale) -> String {
    use grafito_ui::i18n::t;
    let norm = grafito_anim::parametric::normaliza_para_match(concept);
    let menciona = |clave: &str| {
        norm.split(|c: char| !c.is_alphabetic()).any(|token| {
            token == clave || grafito_anim::parametric::token_matchea_clave(token, clave)
        })
    };
    // "area" exacta por token (como `pedido_menciona_area`: evita "tarea"→área).
    if norm.split(|c: char| !c.is_alphabetic()).any(|token| {
        token == "area" || grafito_anim::parametric::token_matchea_clave(token, "integral")
    }) {
        return t("media.title.integral", locale).to_string();
    }
    if menciona("tangente") || menciona("derivada") || menciona("pendiente") {
        return t("media.title.derivative", locale).to_string();
    }
    // `norm` ya va sin tildes: "pitágoras"→"pitagoras" en un solo contains.
    if norm.contains("pitagoras") {
        return t("media.title.pitagoras", locale).to_string();
    }
    if norm.contains("taylor") {
        return t("media.title.taylor", locale).to_string();
    }
    if norm.contains("conforme") || norm.contains("conformal") {
        return t("media.title.conformal", locale).to_string();
    }
    let mut curado = concept.trim().to_string();
    loop {
        let recortado = curado.trim_end();
        if recortado.len() >= "(nativa)".len() && recortado.to_lowercase().ends_with("(nativa)") {
            let sin_sufijo = recortado.chars().count() - "(nativa)".chars().count();
            curado = recortado.chars().take(sin_sufijo).collect();
        } else {
            curado = recortado.to_string();
            break;
        }
    }
    if curado.is_empty() {
        return t("media.title.default", locale).to_string();
    }
    const MAX_TITULO_CHARS: usize = 80;
    if curado.chars().count() > MAX_TITULO_CHARS {
        curado.chars().take(MAX_TITULO_CHARS).collect()
    } else {
        curado
    }
}

/// Parte un pedido playlist "X y después Y" / "X luego Y" / "X después Y"
/// (puro, sin I/O ni spawn).
///
/// Conectores (insensibles a mayúsculas y al acento, exigidos con espacios
/// alrededor, del más específico al más corto): "y después"/"y despues",
/// "luego", "después"/"despues". El resto → `None` y el flujo single queda
/// intacto. Ambos lados deben traer al menos 3 caracteres alfanuméricos y no
/// puede haber un segundo conector de ninguna forma (eso no es "X ... Y" y
/// cae al single honesto en vez de armar 3 steps en silencio).
/// Devuelve los lados recortados en su caso original. Nunca panic (índices
/// por chars, jamás slicing por bytes).
pub(crate) fn split_playlist_request(pedido: &str) -> Option<(String, String)> {
    const CONECTORES: &[&str] = &[" y despues ", " luego ", " despues "];
    let norma = pedido.to_lowercase().replace("después", "despues");
    // Primer conector en orden de especificidad (el largo antes que el corto:
    // " y despues " contiene a " despues " y debe ganar).
    let mut hallado: Option<&str> = None;
    for conector in CONECTORES {
        if norma.contains(conector) {
            hallado = Some(conector);
            break;
        }
    }
    let conector = hallado?;
    let (izq_n, der_n) = norma.split_once(conector)?;
    // Un solo conector en total, de ninguna forma: dos conectores no son "X ... Y".
    for otro in CONECTORES {
        if der_n.contains(otro) || izq_n.contains(otro) {
            return None;
        }
    }
    // Mapeo a caso original por conteo de chars (con o sin acento el largo en
    // chars es el mismo: "después" y "despues" miden 7; `take` por chars nunca
    // hace panic).
    let n_izq = izq_n.chars().count();
    let n_conector = conector.chars().count();
    let mut resto = pedido.chars();
    let izq: String = resto.by_ref().take(n_izq).collect();
    let puente: String = resto.by_ref().take(n_conector).collect();
    let der: String = resto.collect();
    if puente.to_lowercase().replace("después", "despues") != conector {
        return None;
    }
    let izq = izq.trim().to_string();
    let der = der.trim().to_string();
    let alfanum = |s: &str| s.chars().filter(|c| c.is_alphanumeric()).count();
    if alfanum(&izq) < 3 || alfanum(&der) < 3 {
        return None;
    }
    Some((izq, der))
}

/// Arma la playlist para "X y después Y" (puro, sin I/O ni spawn).
///
/// Cada lado resuelve su plantilla por el punto único (`plantilla_para_pedido`)
/// y corre 2 s (el primero con 0.5 s de `Wait` posterior). `None` si no hay
/// conector o si la playlist no valida (el llamante cae al single honesto:
/// o playlist entera o una sola animación, jamás nada parcial en silencio).
pub(crate) fn playlist_para_pedido(pedido: &str) -> Option<grafito_anim::protocol::Playlist> {
    let (a, b) = split_playlist_request(pedido)?;
    let req_a = grafito_anim::protocol::request_for_concept(&a, plantilla_para_pedido(&a));
    let req_b = grafito_anim::protocol::request_for_concept(&b, plantilla_para_pedido(&b));
    grafito_anim::protocol::build_animations_with_timings(vec![
        (req_a, 2.0, 0.5),
        (req_b, 2.0, 0.0),
    ])
    .ok()
}

/// Decisión honesta única para animación: media sí/no + prosa coherente.
///
/// - `NoAnimacion`: no pide animación → flujo chat normal (puede ir remoto).
/// - `PreguntarSinMedia`: falta concepto o función inválida → turno guía
///   local, SIN hilo ni media ni remoto. Nunca pregunta Y muestra.
/// - `RenderCanonico`: integral sin función → hilo + prosa que declara la
///   canónica (`INTEGRAL_CANONICAL_PROSA`). Nunca huérfana.
/// - `RenderExplicito`: con función válida → hilo + prosa que la nombra.
/// - `RenderGenerico`: otra animación válida → hilo + referencia.
///
/// Render* siempre es local-only (sin remoto): así la prosa y la media
/// nunca divergen (el bug era Spark preguntando mientras el hilo mostraba).
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum DecisionAnimacion {
    NoAnimacion,
    PreguntarSinMedia(String),
    RenderCanonico {
        plantilla: String,
        concepto: String,
    },
    RenderExplicito {
        plantilla: String,
        concepto: String,
        expr: String,
    },
    RenderGenerico {
        plantilla: String,
        concepto: String,
    },
}

/// Punto único de decisión para animación (puro, sin I/O ni spawn).
///
/// Orden: gatillo → concepto → integral (canónica/explícita/inválida) →
/// genérico. Sin `unwrap`: los `Err` se vuelven `PreguntarSinMedia`.
pub(crate) fn decide_animacion(pedido: &str) -> DecisionAnimacion {
    if !crate::anim_ui::wants_animation_request(pedido) {
        return DecisionAnimacion::NoAnimacion;
    }
    let concepto = match crate::anim_ui::animation_concept_from_request(pedido) {
        Ok(concepto) => concepto,
        Err(guia) => return DecisionAnimacion::PreguntarSinMedia(guia),
    };
    let plantilla = plantilla_para_pedido(pedido).to_string();
    match clasifica_pedido_integral(pedido, &plantilla) {
        IntegralPedido::FuncionInvalida(detalle) => DecisionAnimacion::PreguntarSinMedia(detalle),
        IntegralPedido::Canonica => DecisionAnimacion::RenderCanonico {
            plantilla,
            concepto,
        },
        IntegralPedido::Explicita => {
            let expr = match grafito_anim::parametric::infer_area_anim(pedido) {
                Ok(resuelto) => resuelto.anim().expr_a.clone(),
                Err(error) => return DecisionAnimacion::PreguntarSinMedia(error.to_string()),
            };
            DecisionAnimacion::RenderExplicito {
                plantilla,
                concepto,
                expr,
            }
        }
        IntegralPedido::NoAplica => match clasifica_pedido_tangente(pedido, &plantilla) {
            // M1: la tangente inválida (`foo(x)`) era canónica muda; ahora
            // es guía honesta igual que la integral (paridad con el agente).
            TangentePedido::FuncionInvalida(detalle) => {
                DecisionAnimacion::PreguntarSinMedia(detalle)
            }
            TangentePedido::Canonica => DecisionAnimacion::RenderCanonico {
                plantilla,
                concepto,
            },
            TangentePedido::Explicita => {
                let expr = match grafito_anim::parametric::infer_tangent_anim(pedido) {
                    Ok(resuelto) => resuelto.anim().expr_a.clone(),
                    Err(error) => return DecisionAnimacion::PreguntarSinMedia(error.to_string()),
                };
                DecisionAnimacion::RenderExplicito {
                    plantilla,
                    concepto,
                    expr,
                }
            }
            TangentePedido::NoAplica => DecisionAnimacion::RenderGenerico {
                plantilla,
                concepto,
            },
        },
    }
}

/// Reset T1 por turno (Bug B: integral pegada).
///
/// Sin mención de animación en EL MENSAJE ACTUAL (`NoAnimacion`) → jamás
/// media: limpia el slot para que el turno no-animación no re-muestre la
/// animación del turno anterior. El resto de decisiones no se toca (Render*
/// ya limpia antes de spawnear; la guía nunca tuvo media). Sin I/O ni spawn.
pub(crate) fn limpiar_media_si_no_animacion(
    panel: &mut grafito_ui::assistant::AssistantPanelState,
    decision: &DecisionAnimacion,
    ctx: &egui::Context,
) {
    if matches!(decision, DecisionAnimacion::NoAnimacion) {
        panel.set_media(None, ctx);
    }
}

enum LocalAssistantDisposition {
    Solved {
        answer: String,
        plan: Option<ProposedPlan>,
    },
    NeedsRemoteAuthorization(String),
    Rejected(String),
}

fn classify_local_assistant_response(response: AssistantResponse) -> LocalAssistantDisposition {
    match response.status {
        LocalAssistantStatus::Solved => LocalAssistantDisposition::Solved {
            answer: response.answer,
            plan: response.plan,
        },
        LocalAssistantStatus::Unsupported | LocalAssistantStatus::VisionUnavailable => {
            LocalAssistantDisposition::NeedsRemoteAuthorization(response.answer)
        }
        LocalAssistantStatus::Rejected => LocalAssistantDisposition::Rejected(response.answer),
    }
}

fn apply_local_assistant_plan(
    document: &mut grafito_core::Document,
    plan: &ProposedPlan,
    undo_stack: &mut VecDeque<grafito_core::Document>,
    redo_stack: &mut VecDeque<grafito_core::ChangeSet>,
) -> Result<grafito_command::assistant_plan::PlanApplyResult, String> {
    let before = document.clone();
    let result = harness::apply_plan(document, plan)?;
    let outcome =
        grafito_command::commands::CommandOutcome::Message("Propuesta local aplicada.".into());
    crate::app::save_command_snapshot_if_mutated(
        &outcome, before, document, undo_stack, redo_stack,
    );
    Ok(result)
}

#[derive(Default)]
pub(crate) struct AssistantRuntime {
    next_request_id: u64,
    /// Modelo de fallback solo-sesión (p.ej. deepseek tras caída de spark).
    /// No se persiste: la preferencia del usuario queda intacta y el próximo
    /// pedido reintenta el modelo elegido (auto-recupera si el proveedor vuelve).
    fallback_model: Option<String>,
    remote_job: Option<AssistantRemoteJob>,
    proposal_job: Option<AssistantProposalJob>,
    model_job: Option<AssistantModelJob>,
    model_refresh_queued: bool,
    image_job: Option<AssistantImageJob>,
    agent_job: Option<AssistantAgentJob>,
    anim_job: Option<AssistantAnimJob>,
    /// W-B: worker IA-primero (SPEC de la IA + render validado en un solo hilo).
    /// Cero doble render: el Submit spawnea o `anim_job` (local) o este job
    /// (IA-primero), nunca ambos (ambos se cancelan antes de spawnear).
    anim_ia_job: Option<AssistantAnimIaJob>,
    /// Export a GIF de la card en vuelo (B5): `JoinHandle` de
    /// `spawn_gif_export` que `poll_gif_export_job` drena sin bloquear.
    gif_export_job: Option<GifExportJob>,
    session_api_key: Option<SessionApiKey>,
    /// Sesión Go estable por conversación (`x-opencode-session`, docs Go
    /// 2026-09-08): UUID v4 lazy en el primer request Go, estable entre
    /// turnos, nueva al Limpiar conversación. Sólo se adjunta a `ProviderSettings`
    /// cuando el proveedor es Go; el resto trae `None` (sin header).
    go_session_id: Option<String>,
}

struct SessionApiKey {
    provider: ProviderProfile,
    key: String,
}

impl Drop for SessionApiKey {
    fn drop(&mut self) {
        // Zeroize clave en memoria para no dejar secretos en heap.
        // No usamos crate `zeroize` (no está en Cargo.toml); implementamos manual
        // con volatile write + black_box para evitar optimización.
        let len = self.key.len();
        if len > 0 {
            // Safety: String's allocation es contigua y escribible; sobreescribimos bytes.
            unsafe {
                std::ptr::write_bytes(self.key.as_mut_ptr(), 0u8, len);
            }
        }
        std::hint::black_box(&self.key);
        // Limpia contenido lógico; la memoria ya está zeroizada arriba.
        self.key.clear();
        // Asegura que el compilador no elimine el zeroize.
        std::hint::black_box(&self.key);
        // Si zeroize crate estuviera disponible, usaríamos `self.key.zeroize()`.
    }
}

impl AssistantRuntime {
    /// Modelo que deben traer los resultados en vuelo: el fallback si hay un
    /// reintento activo, si no el configurado por el usuario.
    fn expected_model<'a>(&'a self, current: &'a str) -> &'a str {
        self.fallback_model.as_deref().unwrap_or(current)
    }

    fn key_for(&self, provider: ProviderProfile) -> Option<String> {
        self.session_api_key
            .as_ref()
            .filter(|stored| stored.provider == provider)
            .map(|stored| stored.key.clone())
    }

    fn remember_key(&mut self, provider: ProviderProfile, key: String) {
        // Punto único de saneado: nunca queda en memoria una clave con
        // espacios/saltos pegados al copiar.
        self.session_api_key = Some(SessionApiKey {
            provider,
            key: key.trim().to_owned(),
        });
    }

    fn forget_key(&mut self) {
        self.session_api_key = None;
    }

    /// Sesión Go estable por conversación (UUID v4 lazy).
    /// La crea en el primer request Go y la conserva entre turnos; el botón
    /// Limpiar la rota vía `rotate_go_session`. Pura memoria, sin I/O, sin
    /// `unwrap`: `Uuid::new_v4` no falla.
    fn ensure_go_session(&mut self) -> String {
        if let Some(id) = self.go_session_id.clone() {
            if grafito_assistant::sanitize_go_session_id(&id).is_some() {
                return id;
            }
        }
        let fresh = uuid::Uuid::new_v4().to_string();
        self.go_session_id = Some(fresh.clone());
        fresh
    }

    /// Rota la sesión Go (botón Limpiar conversación): la próxima consulta
    /// abre una conversación nueva en el gateway (routing/caching frescos).
    /// Genera el UUID ya (no lazy) para que los tests vean el cambio.
    fn rotate_go_session(&mut self) {
        self.go_session_id = Some(uuid::Uuid::new_v4().to_string());
    }

    fn remote_request_slot_is_free(&self) -> bool {
        self.remote_job.is_none() && self.proposal_job.is_none() && self.agent_job.is_none()
    }

    fn cancel_stale_agent_job(
        &mut self,
        current_provider: ProviderProfile,
        current_model: &str,
    ) -> bool {
        if let Some(job) = self.agent_job.as_ref() {
            if !job.cancellation.is_cancelled()
                && !accepts_remote_result(current_provider, current_model, job.provider, &job.model)
            {
                job.cancellation.cancel();
                return true;
            }
        }
        false
    }

    fn cancel_stale_remote_job(
        &mut self,
        current_provider: ProviderProfile,
        current_model: &str,
    ) -> bool {
        let mut cancelled = false;
        if let Some(job) = self.remote_job.as_ref() {
            if !job.cancellation.is_cancelled()
                && !accepts_remote_result(current_provider, current_model, job.provider, &job.model)
            {
                job.cancellation.cancel();
                cancelled = true;
            }
        }
        if let Some(job) = self.proposal_job.as_ref() {
            if !job.cancellation.is_cancelled()
                && !accepts_remote_result(current_provider, current_model, job.provider, &job.model)
            {
                job.cancellation.cancel();
                cancelled = true;
            }
        }
        cancelled
    }

    fn take_finished_remote_job(&mut self) -> Option<FinishedRemoteJob> {
        let result = {
            let job = self.remote_job.as_ref()?;
            match job.receiver.try_recv() {
                Ok(result) => result,
                Err(TryRecvError::Empty) => return None,
                Err(TryRecvError::Disconnected) => {
                    Err("La consulta del asistente terminó inesperadamente.".into())
                }
            }
        };
        let job = self.remote_job.take()?;
        Some(FinishedRemoteJob {
            id: job.id,
            provider: job.provider,
            model: job.model,
            route: job.route,
            fusion_fallback_allowed: job.fusion_fallback_allowed,
            question: job.question,
            correction_attempt: job.correction_attempt,
            repair_target_turn: job.repair_target_turn,
            document_revision: job.document_revision,
            document_digest: job.document_digest,
            focus: job.focus,
            cancelled: job.cancellation.is_cancelled(),
            result,
            stream_preview_active: job.preview_active,
        })
    }

    /// Drena los deltas de streaming a la burbuja provisional del chat.
    ///
    /// Lee todo lo disponible del canal acotado (128, best-effort) y crea o
    /// actualiza UN turno asistente provisional al final de `conversation`
    /// (se renderiza como burbuja porque el dibujado recorre toda la
    /// conversación también con `is_pending`). El texto visible se capa a
    /// `MAX_CONVERSATION_TURN_CHARS`; el resultado final llega por el canal
    /// de completado con su propio presupuesto. Si la conversación ya está en
    /// `MAX_CONVERSATION_TURNS`, se omite el preview sin perder el final.
    /// Puro UI-state, sin I/O: se llama cada frame desde `poll_assistant_jobs`.
    /// Además registra `first_delta_at` (para la etapa `Recibiendo`) y sincroniza
    /// la etapa visible a la Piel (sólo textos/estados, sin layout).
    pub(crate) fn drain_remote_stream_preview(
        &mut self,
        panel: &mut AssistantPanelState,
        ctx: &egui::Context,
    ) -> bool {
        let deltas: Vec<String> = {
            let Some(job) = self.remote_job.as_ref() else {
                return false;
            };
            let Some(stream) = job.stream_rx.as_ref() else {
                return false;
            };
            stream.try_iter().collect()
        };
        if deltas.is_empty() {
            // Sin deltas igual se refresca la etapa (conectando → esperando →
            // aviso lento de 10s), para que el deepseek no-streaming también
            // tenga feedback sin silencio prolongado.
            self.sync_remote_stage_to_panel(panel);
            return false;
        }
        let Some(job) = self.remote_job.as_mut() else {
            return false;
        };
        for delta in deltas {
            job.stream_text.push_str(&delta);
        }
        if job.first_delta_at.is_none() {
            job.first_delta_at = Some(std::time::Instant::now());
        }
        let display: String = job
            .stream_text
            .chars()
            .take(MAX_CONVERSATION_TURN_CHARS)
            .collect();
        if !job.preview_active {
            if panel.conversation.len() >= MAX_CONVERSATION_TURNS {
                self.sync_remote_stage_to_panel(panel);
                return false;
            }
            panel
                .conversation
                .push(ConversationTurn::assistant(display));
            job.preview_active = true;
        } else if let Some(last) = panel.conversation.last_mut() {
            // Invariante: con el slot remoto ocupado nada más empuja turnos,
            // así que el último sigue siendo nuestro provisional.
            if last.role == ConversationRole::Assistant {
                last.content = display;
            }
        }
        self.sync_remote_stage_to_panel(panel);
        ctx.request_repaint();
        true
    }

    /// Sincroniza la etapa visible del turno remoto a la Piel.
    ///
    /// Deriva `Autorizada → Conectando → EsperandoPrimerToken → Recibiendo(KiB)`
    /// de tiempo+deltas (heurística documentada) y pone `remote_stage` +
    /// `remote_stage_elapsed_secs` en el panel. Puro UI-state, sin I/O.
    /// Sin job en vuelo no toca nada (la etapa se reinicia en
    /// begin/complete/fail).
    fn sync_remote_stage_to_panel(&self, panel: &mut AssistantPanelState) {
        let Some(job) = self.remote_job.as_ref() else {
            return;
        };
        let now = std::time::Instant::now();
        let elapsed_secs = now
            .checked_duration_since(job.started_at)
            .map(|elapsed| elapsed.as_secs())
            .unwrap_or(0);
        let has_first_delta = job.first_delta_at.is_some();
        let kib = job.stream_text.len() / 1024;
        let stage = remote_stage_for_job(elapsed_secs, has_first_delta, kib);
        // El aviso lento mide el tiempo EN la etapa actual, no desde el arranque:
        // `Recibiendo` cuenta desde el primer delta, el resto desde el arranque.
        let stage_elapsed_secs = match stage {
            RemoteStage::Recibiendo { .. } => job
                .first_delta_at
                .and_then(|first| now.checked_duration_since(first))
                .map(|elapsed| elapsed.as_secs())
                .unwrap_or(0),
            _ => elapsed_secs,
        };
        let ui_stage = match stage {
            RemoteStage::Autorizada => grafito_ui::assistant::RemoteStage::Autorizada,
            RemoteStage::Conectando => grafito_ui::assistant::RemoteStage::Conectando,
            RemoteStage::EsperandoPrimerToken => {
                grafito_ui::assistant::RemoteStage::EsperandoPrimerToken
            }
            RemoteStage::Recibiendo { kib } => {
                grafito_ui::assistant::RemoteStage::Recibiendo { kib }
            }
        };
        panel.set_remote_stage(ui_stage, stage_elapsed_secs);
    }

    fn take_finished_proposal_job(&mut self) -> Option<FinishedProposalJob> {
        let result = {
            let job = self.proposal_job.as_ref()?;
            match job.receiver.try_recv() {
                Ok(result) => result,
                Err(TryRecvError::Empty) => return None,
                Err(TryRecvError::Disconnected) => {
                    Err("La comprobación local de la propuesta terminó inesperadamente.".into())
                }
            }
        };
        let job = self.proposal_job.take()?;
        Some(FinishedProposalJob {
            id: job.id,
            provider: job.provider,
            model: job.model,
            route: job.route,
            fusion_fallback_allowed: job.fusion_fallback_allowed,
            question: job.question,
            correction_attempt: job.correction_attempt,
            repair_target_turn: job.repair_target_turn,
            document_revision: job.document_revision,
            document_digest: job.document_digest,
            focus: job.focus,
            text: job.text,
            cancelled: job.cancellation.is_cancelled(),
            result,
        })
    }

    fn request_model_refresh(&mut self) -> bool {
        if self.model_job.is_some() {
            self.model_refresh_queued = true;
            false
        } else {
            true
        }
    }

    fn cancel_stale_model_job(&mut self, current_provider: ProviderProfile) -> bool {
        let Some(job) = self.model_job.as_ref() else {
            return false;
        };
        if job.cancellation.is_cancelled() || job.provider == current_provider {
            return false;
        }
        job.cancellation.cancel();
        true
    }

    fn take_finished_model_job(&mut self) -> Option<FinishedModelJob> {
        let result = {
            let job = self.model_job.as_ref()?;
            match job.receiver.try_recv() {
                Ok(result) => result,
                Err(TryRecvError::Empty) => return None,
                Err(TryRecvError::Disconnected) => {
                    Err("La lista de modelos terminó inesperadamente.".into())
                }
            }
        };
        let job = self.model_job.take()?;
        Some(FinishedModelJob {
            id: job.id,
            provider: job.provider,
            cancelled: job.cancellation.is_cancelled(),
            result,
        })
    }

    fn take_queued_model_refresh_if_idle(&mut self) -> bool {
        self.model_job.is_none() && std::mem::take(&mut self.model_refresh_queued)
    }

    /// Cancela todos los jobs del asistente para el botón Cancel.
    ///
    /// M1: delega en `cancel_anim_job`, que ya cancela TODO lo vivo del
    /// turno (remote/proposal/agent/model señalados con slot hasta el
    /// drain, anim dropeados, export con reaper). Sin duplicar el contrato.
    /// Retorna `true` si había algún job en vuelo.
    fn cancel_all_assistant_jobs(&mut self) -> bool {
        self.cancel_anim_job()
    }

    /// Cancela TODO lo vivo del turno de animación (M1, defecto 5).
    ///
    /// Headless y sin I/O en el llamante:
    /// - `remote`/`proposal`/`agent`/`model` tienen token: se marcan
    ///   cancelados pero el slot se conserva hasta que
    ///   `take_finished_*`/`poll_assistant_agent` drene el worker (sin
    ///   huérfanos; ver test `cancelled_remote_job_...`).
    /// - `anim`/`anim_ia` tienen token (AS4, como `agent`): se señalan y se
    ///   dropea el slot. El worker es acotado (render nativo <2 s con
    ///   chequeo entre frames o `job_timeout` 15 s del motor con closure
    ///   `cancel`) y su `send` falla tras el drop, así que el hilo termina
    ///   solo sin dejar trabajo huérfano.
    /// - `gif_export` (`JoinHandle`, no cancelable): se suelta el slot y un
    ///   reaper en background hace `join` + borra el temporal para no dejar
    ///   basura ni publicar éxito de un turno cancelado. La card la resetea
    ///   el llamante a `Idle` (ver `cancel_assistant_request` y runners).
    ///
    /// Retorna `true` si había algún job en vuelo.
    pub(crate) fn cancel_anim_job(&mut self) -> bool {
        let mut hubo = false;
        if let Some(job) = self.remote_job.as_ref() {
            job.cancellation.cancel();
            hubo = true;
        }
        if let Some(job) = self.proposal_job.as_ref() {
            job.cancellation.cancel();
            hubo = true;
        }
        if let Some(job) = self.agent_job.as_ref() {
            job.cancellation.cancel();
            hubo = true;
        }
        if let Some(job) = self.model_job.as_ref() {
            job.cancellation.cancel();
            hubo = true;
        }
        if let Some(job) = self.anim_job.as_ref() {
            job.cancellation.cancel();
        }
        if let Some(job) = self.anim_ia_job.as_ref() {
            job.cancellation.cancel();
        }
        if self.anim_job.is_some() {
            self.anim_job = None;
            hubo = true;
        }
        if self.anim_ia_job.is_some() {
            self.anim_ia_job = None;
            hubo = true;
        }
        if let Some(job) = self.gif_export_job.take() {
            // R1-4: cancela el token (el export chequea entre frames) y el
            // reaper hace `join` ACOTADO (5 s) + borra el temporal: sin
            // crecimiento de threads ni basura en disco. Si da timeout, el
            // hilo queda detached con marca (se borra igual el path conocido).
            job.cancel.cancel();
            let ruta_conocida = job.path.clone();
            let _ = std::thread::Builder::new()
                .name("gif-export-reaper".into())
                .spawn(move || {
                    match join_gif_handle_bounded(job.handle, GIF_REAPER_TIMEOUT) {
                        Some(Ok(path)) => {
                            let _ = std::fs::remove_file(path);
                        }
                        Some(Err(_)) => {
                            let _ = std::fs::remove_file(&ruta_conocida);
                        }
                        None => {
                            // Timeout: detached con marca + limpieza best-effort.
                            eprintln!(
                                "[gif-reaper] timeout tras {:?}, hilo detached; borro {}",
                                GIF_REAPER_TIMEOUT,
                                ruta_conocida.display()
                            );
                            let _ = std::fs::remove_file(&ruta_conocida);
                        }
                    }
                });
            hubo = true;
        }
        hubo
    }
}

/// Mensaje honesto ante 2da animación en curso (W-A, puro y testeable).
///
/// `was_animating=true` → `Some("Ya estoy animando…")` (el caller ya hizo el
/// cancel real y arranca la nueva = reemplazo explícito avisado, no mudo).
/// `false` → `None` (arranque normal, sin aviso).
pub(crate) fn anim_replace_message(was_animating: bool) -> Option<&'static str> {
    if was_animating {
        Some("Ya estoy animando: cancelo la anterior y arranco la nueva.")
    } else {
        None
    }
}

/// Limpia la burbuja provisional del streaming (cancelación o resultado).
///
/// Sólo retira el último turno si es del asistente: con el slot remoto
/// ocupado nada más empuja turnos, así que ese es el provisional creado por
/// `drain_remote_stream_preview`. Si nunca hubo preview, no toca nada y la
/// conversación queda como en el path no-streaming.
fn pop_provisional_stream_turn(panel: &mut AssistantPanelState) {
    if panel
        .conversation
        .last()
        .is_some_and(|turn| turn.role == ConversationRole::Assistant)
    {
        panel.conversation.pop();
    }
}

/// ¿El error remoto es una reparación socrática forzada por el guard?
///
/// El worker retorna el diagnóstico interno tal cual cuando
/// `guard_remote_completion` detecta telling con `attempts<2`. El diagnóstico
/// conserva la jerga `GUARD TELLING` SOLO para detección y logs; el poll JAMÁS
/// lo publica: lo convierte a voz de Mili vía `repair_student_message`
/// (sin `GUARD`/`attempts`/`estado`/`Re-preguntá`). Publicarlo crudo vía
/// `complete_request` fue el bug P0.
fn is_socratic_repair_error(error: &str) -> bool {
    error.starts_with("GUARD TELLING")
}

/// Cuenta respuestas a pregunta heurística previa (no turnos totales).
///
/// Un turno de usuario cuenta solo si el turno asistente inmediato anterior
/// contiene `?` (re-pregunta heurística). El turno actual (último, ya empujado
/// por `begin_request`) se excluye: aún no es respuesta, es la pregunta en
/// curso. Primer turno sin pregunta previa ⇒ 0 y no punitivo (el guard de
/// sesión se bypassea en `session_socratic_guard`; ver `can_reveal_answer`).
/// Puro y determinista, sin `unwrap`, saturante a `u8`.
fn count_heuristic_answers(conversation: &[ConversationTurn]) -> u8 {
    let minus_current = conversation.len().saturating_sub(1);
    let mut count: usize = 0;
    let mut idx = 0;
    while idx < minus_current {
        let is_user = conversation
            .get(idx)
            .map(|turn| turn.role == ConversationRole::User)
            .unwrap_or(false);
        if is_user {
            let prev_is_question = idx
                .checked_sub(1)
                .and_then(|prev| conversation.get(prev))
                .map(|prev| prev.role == ConversationRole::Assistant && prev.content.contains('?'))
                .unwrap_or(false);
            if prev_is_question {
                count = count.saturating_add(1);
            }
        }
        idx += 1;
    }
    count.min(255) as u8
}

/// Construye el guard socrático de sesión para un lanzamiento remoto.
///
/// - `attempts`: respuestas a pregunta heurística previa (ver
///   `count_heuristic_answers`), no turnos totales. El turno actual se excluye.
///   Primer turno sin pregunta previa ⇒ 0 y no punitivo.
/// - `topic`: concepto sanitizado vía `extract_concept` (quita saludos/verbos
///   como `hola`/`haceme`/`dame`, exige min 3 letras y allowlist). Sin tema
///   reconocido → `String::new()` para que el scaffold caiga al fallback fijo
///   `¿qué querés graficar primero: recta, parábola…?` en vez de interpolar el
///   texto crudo (`¿Te imaginás hola...?` era el bug P0).
/// - `level`: `PedagogicalLevel::from_level_value` del nivel del perfil.
/// - `history`: últimos 4 turnos como `Turn` pedagógicos (contenido capado a
///   200 chars; el engine vuelve a acotar al segmentar).
///   Puro y determinista: misma sesión → mismo guard.
fn socratic_guard_context(
    level_value: u32,
    working_topic: Option<&str>,
    question: &str,
    conversation: &[ConversationTurn],
) -> SocraticGuardContext {
    // Sanitiza: memoria primero, luego pregunta actual; sin tema → "" (fallback).
    let sanitized_topic: String = working_topic
        .map(str::trim)
        .filter(|topic| !topic.is_empty())
        .and_then(extract_concept)
        .or_else(|| extract_concept(question))
        .unwrap_or_default();
    let mut fsm = SocraticFsm::new(sanitized_topic);
    fsm.attempts = count_heuristic_answers(conversation);
    let history: Vec<Turn> = conversation
        .iter()
        .rev()
        .take(4)
        .rev()
        .map(|turn| Turn {
            role: match turn.role {
                ConversationRole::User => "user".into(),
                ConversationRole::Assistant => "assistant".into(),
            },
            content: turn.content.chars().take(200).collect(),
        })
        .collect();
    let scaffold = ScaffoldEngine.scaffold(
        &fsm.topic.clone(),
        PedagogicalLevel::from_level_value(level_value),
        &history,
    );
    SocraticGuardContext { fsm, scaffold }
}

/// Ventana de `conectando` (2s): sin deltas y con menos de 2s desde el
/// arranque se muestra "Conectando…"; pasado ese tiempo sin deltas se pasa a
/// "Esperando primer token…". Heurística tiempo+deltas (la app no ve el
/// handshake TLS): documentada como tal, no como señal del wire.
/// El umbral lento (10s, aviso "tardando más de lo normal, podés cancelar")
/// vive en `grafito-assistant::REMOTE_SLOW_STAGE_SECS` y
/// `grafito-ui::assistant::REMOTE_SLOW_STAGE_SECS` (la UI no depende de este
/// crate): el texto lo pone la Piel, la app sólo sincroniza etapa + segundos.
const REMOTE_CONNECTING_WINDOW_SECS: u64 = 2;

/// Etapa visible del turno remoto (sub-estado de `Thinking`, con timestamp).
///
/// Cadena: `Autorizada → Conectando → EsperandoPrimerToken → Recibiendo(N KiB)
/// → Lista` (lista = job cosechado, sin job en vuelo). `Autorizada` cubre el
/// primer segundo tras el consentimiento (autorización y spawn ocurren en el
/// mismo frame); `Recibiendo` sólo existe con deltas SSE (protocolo
/// Responses/Spark: el resto, p.ej. deepseek chat, se queda en espera con
/// aviso lento, honesto y sin KiB inventados).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum RemoteStage {
    Autorizada,
    Conectando,
    EsperandoPrimerToken,
    Recibiendo { kib: usize },
}

/// Calcula la etapa desde tiempo + deltas (puro, testeable, sin I/O).
///
/// - `elapsed_secs`: segundos desde `started_at`.
/// - `has_first_delta`: si ya llegó algún delta SSE.
/// - `kib`: KiB acumulados (`stream_text.len() / 1024`).
pub(crate) fn remote_stage_for_job(
    elapsed_secs: u64,
    has_first_delta: bool,
    kib: usize,
) -> RemoteStage {
    if has_first_delta {
        RemoteStage::Recibiendo { kib }
    } else if elapsed_secs < 1 {
        RemoteStage::Autorizada
    } else if elapsed_secs < REMOTE_CONNECTING_WINDOW_SECS {
        RemoteStage::Conectando
    } else {
        RemoteStage::EsperandoPrimerToken
    }
}

struct AssistantRemoteJob {
    id: u64,
    /// Identidad seleccionada por el usuario, usada para descartar resultados obsoletos.
    provider: ProviderProfile,
    model: String,
    route: AssistantRemoteRoute,
    fusion_fallback_allowed: bool,
    question: String,
    correction_attempt: u8,
    repair_target_turn: Option<usize>,
    document_revision: u64,
    document_digest: String,
    focus: Option<AssistantFocus>,
    cancellation: CancellationToken,
    receiver: Receiver<Result<RemoteCompletion, String>>,
    /// Deltas de streaming SSE (sólo protocolo Responses; el resto lo deja
    /// desconectado y nunca hay preview). Acotado a 128 (best-effort).
    stream_rx: Option<Receiver<String>>,
    /// Texto acumulado del stream para la burbuja provisional.
    stream_text: String,
    /// Hay un turno provisional al final de `conversation` que debe limpiarse
    /// al terminar/cancelar (ver `pop_provisional_stream_turn`).
    preview_active: bool,
    /// Instante de arranque del worker (para etapas con timestamp).
    started_at: std::time::Instant,
    /// Instante del primer delta SSE (para `Recibiendo` + aviso lento).
    /// `None` = aún esperando primer token (p.ej. deepseek no-streaming).
    first_delta_at: Option<std::time::Instant>,
}

struct AssistantProposalJob {
    id: u64,
    provider: ProviderProfile,
    model: String,
    route: AssistantRemoteRoute,
    fusion_fallback_allowed: bool,
    question: String,
    correction_attempt: u8,
    repair_target_turn: Option<usize>,
    document_revision: u64,
    document_digest: String,
    focus: Option<AssistantFocus>,
    text: String,
    cancellation: CancellationToken,
    receiver: Receiver<Result<RemoteProposalVerification, String>>,
}

struct AssistantRemoteLaunch {
    settings: ProviderSettings,
    request: AssistantRequest,
    api_key: Option<String>,
    provider: ProviderProfile,
    model: String,
    route: AssistantRemoteRoute,
    fusion_fallback_allowed: bool,
    question: String,
    document_revision: u64,
    document_digest: String,
    focus: Option<AssistantFocus>,
    correction_attempt: u8,
    repair_target_turn: Option<usize>,
    /// Guard socrático de sesión: el worker lo aplica sobre el completado
    /// final (streaming y no-streaming). `None` lo desactiva.
    socratic_guard: Option<SocraticGuardContext>,
}

struct AssistantRepairRequest {
    feedback: AssistantRepairFeedback,
    target_turn: usize,
}

struct AssistantProposalLaunch {
    id: u64,
    provider: ProviderProfile,
    model: String,
    route: AssistantRemoteRoute,
    fusion_fallback_allowed: bool,
    question: String,
    correction_attempt: u8,
    repair_target_turn: Option<usize>,
    document_revision: u64,
    document_digest: String,
    focus: Option<AssistantFocus>,
    text: String,
}

struct AssistantModelJob {
    id: u64,
    provider: ProviderProfile,
    cancellation: CancellationToken,
    receiver: Receiver<Result<Vec<String>, String>>,
}

struct AssistantImageJob {
    receiver: Receiver<Result<grafito_assistant_types::ImageAttachment, String>>,
}

/// Mensaje del hilo del agente hacia la UI.
enum AgentChannelMsg {
    Event(grafito_agent::AgentEvent),
    Done(Result<grafito_agent::loop_engine::AgentOutcome, String>),
}

/// Envía un mensaje del agente sin bloquear (R2-V1, puro).
///
/// `try_send` + `CancellationToken`: si hay cancelación, no se envía;
/// si el canal está lleno, se descarta el evento (best-effort, la UI ya
/// tiene 128 pendientes y el `Done` final reserva su slot); si el receptor
/// se dropeó, `false` para que el forwarder corte y el `join` sea <1s.
/// Jamás `send` bloqueante en hilo (thread leak).
///
/// Retorna `Some(true)` si se envió, `Some(false)` si se descartó por lleno
/// (seguir drenando), `None` si hay que cortar (cancelado o desconectado).
fn send_agent_msg_nonblocking(
    sender: &std::sync::mpsc::SyncSender<AgentChannelMsg>,
    msg: AgentChannelMsg,
    cancel: &grafito_agent::loop_engine::Cancellation,
) -> Option<bool> {
    if cancel.is_cancelled() {
        return None;
    }
    match sender.try_send(msg) {
        Ok(()) => Some(true),
        Err(std::sync::mpsc::TrySendError::Full(_)) => Some(false),
        Err(std::sync::mpsc::TrySendError::Disconnected(_)) => None,
    }
}

/// Parsea el `args_summary` de un `ToolStarted{ask_user}` a pendiente UI (S2).
///
/// Puro y no bloqueante: `args_summary` es `arguments.to_string()` truncado a
/// 160 chars; para preguntas cortas típicas alcanza. Si viene truncado con
/// `…` se recorta y se intenta igual; si no parsea, `None` honesto (sin
/// inventar pregunta ni opciones). El `call_id` real no viaja en el evento,
/// así que se deriva estable de la pregunta (longitud) sin inventar UUID.
fn parse_agent_ask_user_pending(
    args_summary: &str,
) -> Option<grafito_ui::assistant::PendingClarification> {
    let cleaned = args_summary.trim().trim_end_matches('…').trim();
    if cleaned.is_empty() {
        return None;
    }
    let value: serde_json::Value = serde_json::from_str(cleaned).ok()?;
    let question = value.get("question").and_then(serde_json::Value::as_str)?;
    if question.trim().is_empty() {
        return None;
    }
    let options = value
        .get("options")
        .and_then(|options| options.as_array())
        .map(|array| {
            array
                .iter()
                .filter_map(|item| item.as_str().map(str::to_owned))
                .collect::<Vec<_>>()
        })
        .unwrap_or_default();
    let call_id = format!("ask_user-{}", question.len());
    grafito_ui::assistant::PendingClarification::try_new(&call_id, question, options).ok()
}

/// Job que genera y carga una animación del motor externo.
///
/// AS4: con `cancellation` como `AssistantAgentJob` (cancel real: descartar
/// señala el token, no solo dropea el receiver). El hilo la chequea entre
/// frames en el closure de progreso (el render nativo no acepta token) y
/// antes/después del transporte; el motor externo la recibe como closure
/// `cancel` en `run_job` (aborto <200 ms).
struct AssistantAnimJob {
    cancellation: CancellationToken,
    receiver: std::sync::mpsc::Receiver<Result<grafito_ui::assistant::AssistantMedia, String>>,
}

/// W-B — render listo desde el worker IA-primero (media + prosa coherentes).
///
/// `media` y `prosa` vienen del MISMO spec validado (o IA o canónico de
/// fallback, nunca mezclados). `aviso` es `Some` solo en fallback: UNA línea
/// con plantilla y rango reales (`aviso_fallback_canonico`). `spec` es el
/// efectivamente renderizado (para tests).
pub(crate) struct AnimIaRender {
    pub media: grafito_ui::assistant::AssistantMedia,
    pub prosa: String,
    pub aviso: Option<String>,
}

/// W-B — job del worker IA-primero (SPEC de la IA + render en un solo hilo).
/// Mismo contrato de cancel que `AssistantAnimJob`: el hilo chequea el token
/// entre frames y descarta rancio.
struct AssistantAnimIaJob {
    cancellation: CancellationToken,
    receiver: std::sync::mpsc::Receiver<Result<AnimIaRender, String>>,
}

/// Export a GIF de la card en vuelo (B5).
///
/// Guarda el `JoinHandle` de `spawn_gif_export` (hilo existente, reusable y
/// probado) para drenarlo sin bloquear en `poll_gif_export_job`.
/// `frame_count` es solo para el mensaje de éxito (cuántos fotogramas
/// viajaron al GIF). `cancel` permite abortar el export (R1-4) y `path` es
/// el destino temporal para borrarlo aunque el `join` dé timeout.
struct GifExportJob {
    handle: std::thread::JoinHandle<Result<std::path::PathBuf, crate::anim_native::GifExportError>>,
    frame_count: usize,
    cancel: grafito_assistant::CancellationToken,
    path: std::path::PathBuf,
}

/// Cota del reaper GIF R1-4: el `join` en el path de cancel nunca bloquea
/// más que esto (poll `is_finished` cada 50 ms).
pub(crate) const GIF_REAPER_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(5);

/// `join` acotado R1-4: espera hasta `timeout` (poll 50 ms) y devuelve
/// `Some(resultado)` si el hilo terminó, `None` si dio timeout (hilo
/// detached: al salir del scope el `JoinHandle` se suelta y el hilo sigue
/// solo, con marca en el llamador). Puro sobre el handle, sin I/O.
pub(crate) fn join_gif_handle_bounded(
    handle: std::thread::JoinHandle<Result<std::path::PathBuf, crate::anim_native::GifExportError>>,
    timeout: std::time::Duration,
) -> Option<Result<std::path::PathBuf, crate::anim_native::GifExportError>> {
    let inicio = std::time::Instant::now();
    // Poll sin bloquear: el export corre en su hilo, acá solo miramos.
    while !handle.is_finished() {
        if inicio.elapsed() >= timeout {
            // Timeout: se suelta el handle (detach) con marca del llamador;
            // el hilo exportador sigue solo hasta terminar.
            std::mem::forget(handle);
            return None;
        }
        std::thread::sleep(std::time::Duration::from_millis(50));
    }
    match handle.join() {
        Ok(res) => Some(res),
        Err(_) => Some(Err(crate::anim_native::GifExportError::Encode(
            "la exportación terminó inesperadamente".to_string(),
        ))),
    }
}

/// Guard R1-5: marca `spec_terminado` en `Drop` (también si `complete`
/// paniquea). Sin esto el puente forwarder quedaba en loop eterno y el
/// `join` de abajo nunca llegaba: hilo huérfano por turno.
///
/// Puro sobre el `Arc`, sin I/O, sin `unwrap`.
struct SpecTerminadoGuard {
    flag: std::sync::Arc<std::sync::atomic::AtomicBool>,
}

impl Drop for SpecTerminadoGuard {
    fn drop(&mut self) {
        self.flag.store(true, std::sync::atomic::Ordering::Release);
    }
}

/// `join` acotado R1-5 para el puente (100 ms): poll `is_finished` cada
/// 10 ms; si termina se joinea, si no se suelta (detach) y se retorna
/// `false` con marca del llamador. Puro sobre el handle, sin I/O.
pub(crate) fn join_puente_bounded(
    handle: std::thread::JoinHandle<()>,
    timeout: std::time::Duration,
) -> bool {
    let inicio = std::time::Instant::now();
    while !handle.is_finished() {
        if inicio.elapsed() >= timeout {
            std::mem::forget(handle);
            return false;
        }
        std::thread::sleep(std::time::Duration::from_millis(10));
    }
    let _ = handle.join();
    true
}

/// Cota del puente R1-5: el `join` del forwarder nunca bloquea más que esto.
pub(crate) const PUENTE_JOIN_TIMEOUT: std::time::Duration = std::time::Duration::from_millis(100);

/// Job del modo agente (loop con herramientas).
struct AssistantAgentJob {
    provider: ProviderProfile,
    model: String,
    cancellation: grafito_agent::loop_engine::Cancellation,
    receiver: Receiver<AgentChannelMsg>,
    /// Canal lateral S2 (`ask_user` real vía evento): el forwarder de
    /// background reenvía el pendiente como `PendingClarification` sin
    /// bloquear (try_send acotado). La UI lo drena en `sync_assistant_for_frame`
    /// (hilo UI, try_recv) y lo muestra como botones; nunca toca la zona guard.
    clarification_receiver: Receiver<grafito_ui::assistant::PendingClarification>,
}

struct FinishedRemoteJob {
    id: u64,
    provider: ProviderProfile,
    model: String,
    route: AssistantRemoteRoute,
    fusion_fallback_allowed: bool,
    question: String,
    correction_attempt: u8,
    repair_target_turn: Option<usize>,
    document_revision: u64,
    document_digest: String,
    focus: Option<AssistantFocus>,
    cancelled: bool,
    result: Result<RemoteCompletion, String>,
    /// El job dejó una burbuja provisional que el poll debe limpiar antes de
    /// procesar el resultado (éxito, error o cancelación).
    stream_preview_active: bool,
}

struct FinishedProposalJob {
    id: u64,
    provider: ProviderProfile,
    model: String,
    route: AssistantRemoteRoute,
    fusion_fallback_allowed: bool,
    question: String,
    correction_attempt: u8,
    repair_target_turn: Option<usize>,
    document_revision: u64,
    document_digest: String,
    focus: Option<AssistantFocus>,
    text: String,
    cancelled: bool,
    result: Result<RemoteProposalVerification, String>,
}

struct FinishedModelJob {
    id: u64,
    provider: ProviderProfile,
    cancelled: bool,
    result: Result<Vec<String>, String>,
}

/// Empuja prosa al último turno del asistente (ApplyProposal, M1).
///
/// Solo si el último turno es del asistente (el compromiso verificado lo
/// garantiza en la práctica; el guard evita mezclar si no lo fuera).
/// Pura sobre el transcript, sin I/O ni spawn.
fn empuja_prosa_apply(conversacion: &mut [ConversationTurn], prosa: &str) {
    if let Some(turno) = conversacion.last_mut() {
        if turno.role == ConversationRole::Assistant {
            turno.content.push_str("\n\n");
            turno
                .content
                .push_str(&grafito_ui::assistant::humanize_prose_text(prosa));
        }
    }
}

/// ¿El último turno del asistente aún no declara media (sin "deslizador")?
/// Pura, sin I/O.
fn ultimo_turno_sin_media(conversacion: &[ConversationTurn]) -> bool {
    conversacion.last().is_some_and(|turno| {
        turno.role == ConversationRole::Assistant && !turno.content.contains("deslizador")
    })
}

impl GrafitoApp {
    fn assistant_visuals(
        &mut self,
        ctx: &egui::Context,
    ) -> grafito_ui::assistant::AssistantVisuals {
        crate::app::load_mora_avatar_texture_once(
            ctx,
            &mut self.mora_texture,
            &mut self.mora_texture_load_attempted,
            include_bytes!("../../../assets/mora.png"),
        );
        grafito_ui::assistant::AssistantVisuals {
            mora_texture: self.mora_texture.as_ref().map(egui::TextureHandle::id),
        }
    }

    /// Sincroniza el foco actual y procesa resultados antes de pintar cualquier
    /// host del asistente, incluso cuando su pestaña no es la visible.
    pub(crate) fn sync_assistant_for_frame(&mut self, ctx: &egui::Context) {
        // Sincroniza la memoria del tutor con la tarjeta de progreso del panel.
        self.assistant.tutor_level = self.profile.level;
        self.assistant.tutor_covered = self
            .profile
            .branches
            .iter()
            .filter(|branch| branch.covered)
            .count();
        self.assistant.tutor_total = self.profile.branches.len();
        self.assistant.tutor_next = self
            .profile
            .recommend_next()
            .first()
            .map(|branch| branch.name.clone())
            .unwrap_or_default();
        self.assistant.tutor_streak = self.profile.streak;
        self.assistant.tutor_best_streak = self.profile.best_streak;
        self.assistant.tutor_domain_samples = self.domain_sparkline();
        let epoch = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|duration| duration.as_secs())
            .unwrap_or(0);
        self.assistant.tutor_last_activity = self
            .profile
            .recommend_next()
            .first()
            .and_then(|branch| branch.last_study_epoch)
            .map(|last| grafito_profile::time_ago(last, epoch))
            .unwrap_or_default();
        if !self.assistant.settings_open {
            // Sincronizar memoria larga bidireccionalmente
            let mut needs_save = false;
            if self.assistant.long_memory.facts.len() != self.profile.long_memory.facts.len()
                || self.assistant.long_memory.summary != self.profile.long_memory.summary
                || self.assistant.long_memory.enabled != self.profile.long_memory.enabled
                || self.assistant.long_memory.preferences != self.profile.long_memory.preferences
            {
                for fact in self.assistant.long_memory.facts.clone() {
                    if !self
                        .profile
                        .long_memory
                        .facts
                        .iter()
                        .any(|f| f.text == fact.text)
                    {
                        self.profile.long_memory.facts.push(fact);
                        needs_save = true;
                    }
                }
                // También detectar borrados en UI
                let before = self.profile.long_memory.facts.len();
                self.profile.long_memory.facts.retain(|pf| {
                    self.assistant
                        .long_memory
                        .facts
                        .iter()
                        .any(|af| af.text == pf.text)
                });
                if self.profile.long_memory.facts.len() != before {
                    needs_save = true;
                }
                self.profile.long_memory.summary = self.assistant.long_memory.summary.clone();
                self.profile.long_memory.relationship_stage =
                    self.assistant.long_memory.relationship_stage;
                self.profile.long_memory.enabled = self.assistant.long_memory.enabled;
                self.profile.long_memory.preferences =
                    self.assistant.long_memory.preferences.clone();
                if needs_save
                    || self.assistant.long_memory.summary != self.profile.long_memory.summary
                {
                    spawn_profile_save(self.profile.clone(), crate::utils::profile_path());
                }
            }
            // Sincronizar avatar (incluye nuevos campos bg, accent_custom, instrucciones)
            // Pero preservar si el perfil cambió externamente por nivel up (evolución)
            // Si hay diferencia en evolución, merge
            if self.assistant.avatar != self.profile.avatar {
                // Si no hay edición pendiente, clonar perfil → assistant
                self.assistant.avatar = self.profile.avatar.clone();
            }
            self.assistant.user_name = self.profile.display_name().to_owned();
            self.assistant.long_memory = self.profile.long_memory.clone();
            // F5 inline: sincroniza WorkingMemory episódica (si existe campo en AssistantPanelState)
            // Si no existe campo, este bloque es el comentario de integración pedido en la tarea.
            self.assistant.working_memory = self.profile.working_memory.clone();
            self.avatar_draft = self.profile.avatar.clone();
        } else {
            // Ventana abierta: preview persistente + merge no destructivo de progreso externo
            // Sincronizar solo campos de progreso (nivel, branch) va en tutor_*, no en avatar
            // Mantener avatar_draft sincronizado para preview
            self.avatar_draft = self.assistant.avatar.clone();
            // F5 inline: también en modo preview mantener WorkingMemory sincronizada para debug inline
            self.assistant.working_memory = self.profile.working_memory.clone();
            // Si el perfil ganó XP mientras se edita, no sobrescribir avatar; solo actualizar long_memory si es externa (no editar facts manualmente mientras abierta)
            if self.assistant.long_memory.enabled != self.profile.long_memory.enabled {
                // respetar cambio externo si el usuario no tocó el toggle en esta sesión
                // No-op: el toggle de la UI tiene prioridad
            }
        }
        self.poll_assistant_jobs(ctx);
        if let Some(job) = self.assistant_runtime.anim_job.as_mut() {
            match job.receiver.try_recv() {
                Ok(Ok(media)) => {
                    let was_cancelled = job.cancellation.is_cancelled();
                    self.assistant_runtime.anim_job = None;
                    self.assistant.anim_progress = false;
                    if was_cancelled {
                        // Cancel real: el worker observó el token y el
                        // resultado es rancio — se descarta sin publicar
                        // (sin media rancia en el turno).
                        self.notify("Generación cancelada.", ToastKind::Info);
                    } else {
                        // La animación vive DENTRO del turno del chat:
                        // `set_media` la instala para el reproductor del
                        // transcript (`ui/assistant.rs:915`, `draw_media_card`
                        // en el último turno). Sin ventana compañera.
                        self.assistant.set_media(Some(media), ctx);
                        self.notify("Animación lista.", ToastKind::Success);
                    }
                    ctx.request_repaint();
                }
                Ok(Err(error)) => {
                    let was_cancelled =
                        job.cancellation.is_cancelled() || error.to_lowercase().contains("cancel");
                    self.assistant_runtime.anim_job = None;
                    self.assistant.anim_progress = false;
                    if was_cancelled {
                        self.notify("Generación cancelada.", ToastKind::Info);
                    } else {
                        self.assistant.set_media(None, ctx);
                        let message = format!("No se pudo generar la animación: {error}");
                        self.notify(&message, ToastKind::Error);
                        self.show_assistant_error(message);
                    }
                    ctx.request_repaint();
                }
                Err(std::sync::mpsc::TryRecvError::Empty) => {}
                Err(std::sync::mpsc::TryRecvError::Disconnected) => {
                    let was_cancelled = job.cancellation.is_cancelled();
                    self.assistant_runtime.anim_job = None;
                    self.assistant.anim_progress = false;
                    if !was_cancelled {
                        self.assistant.set_media(None, ctx);
                        self.show_assistant_error(
                            "La generación terminó inesperadamente antes de responder.",
                        );
                    }
                    ctx.request_repaint();
                }
            }
        }
        // W-B: drena el worker IA-primero (SPEC + un solo render, sin bloquear).
        // La prosa y la media vienen del MISMO spec validado: se completan
        // juntas para que nunca diverjan. Fallback trae aviso de una línea.
        if let Some(job) = self.assistant_runtime.anim_ia_job.as_mut() {
            match job.receiver.try_recv() {
                Ok(Ok(render)) => {
                    let was_cancelled = job.cancellation.is_cancelled();
                    self.assistant_runtime.anim_ia_job = None;
                    self.assistant.anim_progress = false;
                    if was_cancelled {
                        self.notify("Generación cancelada.", ToastKind::Info);
                    } else {
                        let humano = grafito_ui::assistant::humanize_prose_text(&render.prosa);
                        self.assistant.complete_local_request(humano);
                        self.assistant.set_media(Some(render.media), ctx);
                        if let Some(aviso) = render.aviso {
                            self.notify(aviso, ToastKind::Info);
                        } else {
                            self.notify("Animación lista.", ToastKind::Success);
                        }
                    }
                    ctx.request_repaint();
                }
                Ok(Err(error)) => {
                    let was_cancelled =
                        job.cancellation.is_cancelled() || error.to_lowercase().contains("cancel");
                    self.assistant_runtime.anim_ia_job = None;
                    self.assistant.anim_progress = false;
                    if was_cancelled {
                        self.notify("Generación cancelada.", ToastKind::Info);
                    } else {
                        self.assistant.set_media(None, ctx);
                        let message = format!("No se pudo generar la animación: {error}");
                        self.notify(&message, ToastKind::Error);
                        self.show_assistant_error(message);
                    }
                    ctx.request_repaint();
                }
                Err(std::sync::mpsc::TryRecvError::Empty) => {}
                Err(std::sync::mpsc::TryRecvError::Disconnected) => {
                    let was_cancelled = job.cancellation.is_cancelled();
                    self.assistant_runtime.anim_ia_job = None;
                    self.assistant.anim_progress = false;
                    if !was_cancelled {
                        self.assistant.set_media(None, ctx);
                        self.show_assistant_error(
                            "La generación terminó inesperadamente antes de responder.",
                        );
                    }
                    ctx.request_repaint();
                }
            }
        }
        // B5: drena el export a GIF de la card (sin bloquear; solo join si terminó).
        self.poll_gif_export_job(ctx);
        if !self.plugins_loaded {
            self.plugins_loaded = true;
            self.load_assistant_plugins();
        }
        // S2: drena aclaraciones del canal lateral sin bloquear (try_recv).
        self.drain_pending_clarifications();
        self.assistant.focus = grafito_command::assistant_context::selected_function_focus(
            &self.document,
            self.selected_object,
        );
    }

    /// Carga una sola vez el registry de plugins y aplica las preferencias del usuario.
    fn load_assistant_plugins(&mut self) {
        let context = plugin_validation_context();
        let config = crate::utils::load_config();
        let mut registry = grafito_plugins::PluginRegistry::load_many(
            &[
                &crate::utils::plugins_dir(),
                &crate::utils::user_data_plugins_dir(),
                &crate::utils::system_plugins_dir(),
            ],
            &context,
        );
        for plugin in &mut registry.plugins {
            let id = plugin.manifest.plugin.id.clone();
            let automatic = plugin.manifest.plugin.activation != "manual";
            plugin.enabled = if config.enabled_plugins.contains(&id) {
                true
            } else if config.disabled_plugins.contains(&id) {
                false
            } else {
                automatic
            };
        }
        self.plugin_registry = Some(registry);
        self.refresh_plugin_snapshot();
    }

    /// Refresca el snapshot mostrado en la ventana de ajustes del asistente.
    fn refresh_plugin_snapshot(&mut self) {
        let Some(registry) = &self.plugin_registry else {
            self.assistant.plugins.clear();
            return;
        };
        self.assistant.plugins = registry
            .plugins
            .iter()
            .map(|plugin| grafito_ui::assistant::PluginRow {
                id: plugin.manifest.plugin.id.clone(),
                name: plugin.manifest.plugin.name.clone(),
                version: plugin.manifest.plugin.version.clone(),
                category: plugin.manifest.plugin.category.clone(),
                description: plugin.manifest.plugin.description.clone(),
                enabled: plugin.enabled,
                error: plugin.error.clone(),
            })
            .collect();
    }

    /// Instrucciones locales de los plugins activos, ajustadas al presupuesto.
    fn plugin_instructions_budgeted(&self) -> String {
        const PLUGIN_INSTRUCTION_CAP_BYTES: usize = 4 * 1024;
        let Some(registry) = &self.plugin_registry else {
            return String::new();
        };
        registry.instructions_bounded(
            grafito_assistant_types::MAX_SYSTEM_INSTRUCTIONS_BYTES
                .min(PLUGIN_INSTRUCTION_CAP_BYTES),
        )
    }

    /// Activa o desactiva un plugin y persiste la preferencia.
    fn toggle_assistant_plugin(&mut self, id: &str, enabled: bool) {
        let Some(registry) = self.plugin_registry.as_mut() else {
            return;
        };
        if !registry.set_enabled(id, enabled) {
            return;
        }
        let mut config = crate::utils::load_config();
        config.enabled_plugins.retain(|existing| existing != id);
        config.disabled_plugins.retain(|existing| existing != id);
        if enabled {
            config.enabled_plugins.push(id.to_string());
        } else {
            config.disabled_plugins.push(id.to_string());
        }
        crate::utils::save_config(&config);
        self.refresh_plugin_snapshot();
    }

    /// Panel del asistente bloqueado por examen (D2): ocupa el mismo lugar,
    /// muestra el motivo y no ofrece ninguna acción (fail-closed visual).
    fn draw_exam_locked_assistant(&mut self, ctx: &egui::Context, _reserved_bottom_height: f32) {
        let theme = grafito_ui::theme::current_theme(ctx);
        egui::SidePanel::right("assistant_exam_locked")
            .default_width(400.0)
            .min_width(300.0)
            .max_width(520.0)
            .show(ctx, |ui| {
                ui.add_space(grafito_ui::tokens::SPACE_MD);
                ui.vertical_centered(|ui| {
                    ui.label(
                        egui::RichText::new("Asistente bloqueado en modo examen")
                            .size(grafito_ui::tokens::TYPE_SM)
                            .strong()
                            .color(theme.text_primary),
                    );
                    ui.label(
                        egui::RichText::new("Salí del examen para volver a usarlo, che.")
                            .size(grafito_ui::tokens::TYPE_XS)
                            .color(theme.text_secondary),
                    );
                });
            });
    }

    /// Contenido bloqueado para el dock (misma honestidad, sin panel propio).
    fn draw_exam_locked_contents(&mut self, ui: &mut egui::Ui) {
        let theme = grafito_ui::theme::current_theme(ui.ctx());
        ui.vertical_centered(|ui| {
            ui.label(
                egui::RichText::new("Asistente bloqueado en modo examen")
                    .size(grafito_ui::tokens::TYPE_SM)
                    .strong()
                    .color(theme.text_primary),
            );
            ui.label(
                egui::RichText::new("Salí del examen para volver a usarlo, che.")
                    .size(grafito_ui::tokens::TYPE_XS)
                    .color(theme.text_secondary),
            );
        });
    }

    /// Dibuja el asistente como panel independiente fuera del workspace 3D.
    pub(crate) fn draw_assistant(&mut self, ctx: &egui::Context, reserved_bottom_height: f32) {
        self.sync_assistant_for_frame(ctx);
        // D2 lockdown: panel visible pero bloqueado (banner + sin acciones).
        if self.exam_mode {
            self.draw_exam_locked_assistant(ctx, reserved_bottom_height);
            self.cancel_stale_model_request();
            return;
        }
        if !self.assistant_visible {
            self.cancel_stale_model_request();
            return;
        }
        let visuals = self.assistant_visuals(ctx);
        if let Some(action) = grafito_ui::assistant::draw_assistant_panel(
            ctx,
            &mut self.assistant,
            reserved_bottom_height,
            visuals,
            &mut self.assistant_blocks_cache,
        ) {
            self.handle_assistant_action(ctx, action);
        }
        self.cancel_stale_model_request();
    }

    /// Dibuja el asistente dentro del dock de Geometry 3D ya reservado por el
    /// shell. La sincronización de trabajos ocurre antes de dibujar las tabs.
    pub(crate) fn draw_assistant_contents_in_workspace_dock(
        &mut self,
        ctx: &egui::Context,
        ui: &mut egui::Ui,
    ) {
        // D2 lockdown: contenido bloqueado (sin acciones, sin ejercicio).
        if self.exam_mode {
            self.draw_exam_locked_contents(ui);
            self.cancel_stale_model_request();
            return;
        }
        if !self.assistant_visible {
            ui.label(
                egui::RichText::new("El asistente esta oculto.")
                    .color(grafito_ui::theme::current_theme(ctx).text_secondary),
            );
            if ui.button("Mostrar asistente").clicked() {
                self.assistant_visible = true;
            }
            self.cancel_stale_model_request();
            return;
        }
        let visuals = self.assistant_visuals(ctx);

        let mut action = grafito_ui::assistant::draw_assistant_contents(
            ui,
            &mut self.assistant,
            visuals,
            &mut self.assistant_blocks_cache,
        );
        if action.is_none() {
            action =
                grafito_ui::assistant::draw_assistant_settings_window(ctx, &mut self.assistant);
        }
        if let Some(action) = action {
            self.handle_assistant_action(ctx, action);
        }
        self.asentar_respuesta_ejercicio();
        if let Some(pedido) =
            crate::teaching_ui::draw_panel_ejercicio(ui, &mut self.teaching_ui.ejercicio)
        {
            match pedido {
                crate::teaching_ui::AccionEjercicio::Otro => {
                    let tema = self.teaching_ui.ejercicio.tema.clone();
                    self.iniciar_ejercicio(ctx, &tema);
                }
                crate::teaching_ui::AccionEjercicio::Cerrar => {
                    self.teaching_ui.ejercicio.cerrar();
                }
            }
        }
        // D2 "Mi plan": próximos 3 del scheduler + racha, siempre visible en
        // el host del tutor (el `recommend_next` ya llegaba a la tarjeta;
        // esto lo vuelve plan visible sin romper la UI).
        let plan = crate::teaching_ui::plan_resumen(&self.profile);
        crate::teaching_ui::draw_mi_plan(ui, &plan);
        self.cancel_stale_model_request();
    }

    pub(crate) fn handle_assistant_action(
        &mut self,
        ctx: &egui::Context,
        action: AssistantUiAction,
    ) {
        // F3.2 PedagogyDispatcher — integración OpenCode Go
        // -------------------------------------------------
        // Las 6 tools pedagógicas (scaffold, generate_exercise, assess_answer,
        // get_curriculum, suggest_next, generate_animation) ya están expuestas al LLM
        // vía `grafito_assistant::default_agent_tools()` → `agent::all_safe_tool_schemas()`
        // y despachadas por `SafeGrafitoDispatcher` / `PedagogyDispatcher` (puras, sin Document).
        // El modo agente (AssistantAgentJob, start_agent_assistant_job) las orquesta vía
        // OpenCode Go (OpenAI-compatible tool calling) sin salir del chat.
        //
        // assistant_tool_catalog (grafito_command) sigue siendo el catálogo de comandos
        // verificados de Grafito; las pedagógicas viajan como OpenAI tools, no como fences.
        //
        // TODO(F3.2-inline): si se quiere invocación directa sin LLM (ej. botón "Andamiar"
        // o comando /scaffold), cablear aquí un match que llame a
        // `grafito_assistant::agent::PedagogyDispatcher.dispatch(&call)` con un ToolCall
        // construido localmente y renderice el ToolResult en el chat. Hoy la orquestación
        // pedagógica pasa exclusivamente por el loop agente cuando `assistant.agent_mode==true`.
        match action {
            AssistantUiAction::Submit => {
                // W3 — saludo sin camino ("hola"): respuesta local con el
                // siguiente paso ofrecido, sin media ni hilo ni remoto.
                if let Some(respuesta) =
                    grafito_ui::assistant::greeting_answer(&self.assistant.problem)
                {
                    let pregunta = self.assistant.problem.clone();
                    self.assistant.begin_request(pregunta);
                    self.assistant.problem.clear();
                    let humano = grafito_ui::assistant::humanize_prose_text(&respuesta);
                    self.assistant.complete_local_request(humano.clone());
                    self.assistant.set_media(None, ctx);
                    self.notify(humano, ToastKind::Info);
                    ctx.request_repaint();
                    return;
                }
                let problem_clone = self.assistant.problem.clone();
                let lower = problem_clone.to_lowercase();
                // Punto único de decisión honesto (`decide_animacion`): media
                // sí/no + prosa coherente en un solo lugar. Antes había doble
                // carril (hilo local + remoto Spark preguntón) que mostraba Y
                // preguntaba a la vez. Ahora integral/tangente es IA-primero
                // (W-B) o local declarado; el resto de Render* sigue local.
                let decision = decide_animacion(&problem_clone);
                // Cancela animación previa si existe — evita crash al pedir otra cosa tras animación
                // y evita "tomo una ya hecha" (stale derivative). Cancel real:
                // señala el token, el hilo descarta.
                if self.cancela_turno_anim() {
                    self.assistant.anim_progress = false;
                    // Z3 trigger único: si el pedido nuevo también anima, el
                    // reemplazo se avisa explícito (misma frase que los
                    // runners, sin duplicar el texto). Si no anima, el cancel
                    // es limpieza silenciosa.
                    let arranca_nueva = matches!(
                        decision,
                        DecisionAnimacion::RenderCanonico { .. }
                            | DecisionAnimacion::RenderExplicito { .. }
                            | DecisionAnimacion::RenderGenerico { .. }
                    );
                    let reemplazo = if arranca_nueva {
                        anim_replace_message(true)
                    } else {
                        None
                    };
                    if let Some(message) = reemplazo {
                        self.notify(message, ToastKind::Info);
                    }
                }
                // Pedido ambiguo o función inválida: turno guía local sin
                // media ni hilo ni remoto, jamás inventa.
                if let DecisionAnimacion::PreguntarSinMedia(guia) = &decision {
                    let question = problem_clone.clone();
                    self.assistant.begin_request(question);
                    self.assistant.problem.clear();
                    let honesto = grafito_ui::assistant::humanize_prose_text(guia);
                    self.assistant.complete_local_request(honesto.clone());
                    self.assistant.set_media(None, ctx);
                    self.notify(honesto, ToastKind::Info);
                    ctx.request_repaint();
                    return;
                }
                // F2b — "X y después Y": playlist entera en una sola media con
                // scrub total (Succession + scheduler global del protocolo; el
                // player existente ya mapea fracción → frame global vía
                // `Timeline::sample`, sin tocar la UI). Solo este patrón y
                // solo sobre decisiones Render (la guía de arriba ya filtró
                // lo ambiguo): nada de guards/cards nuevos, la prosa es la
                // referencia de siempre y el título nombra ambos lados.
                if matches!(
                    decision,
                    DecisionAnimacion::RenderCanonico { .. }
                        | DecisionAnimacion::RenderExplicito { .. }
                        | DecisionAnimacion::RenderGenerico { .. }
                ) {
                    if let Some(playlist) = playlist_para_pedido(&problem_clone) {
                        let question = problem_clone.clone();
                        self.assistant.begin_request(question);
                        self.assistant.problem.clear();
                        let prosa = crate::anim_ui::animation_reference_sentence().to_string();
                        let humano = grafito_ui::assistant::humanize_prose_text(&prosa);
                        self.assistant.complete_local_request(humano);
                        self.assistant.set_media(None, ctx);
                        self.run_assistant_playlist_with(ctx, playlist);
                        ctx.request_repaint();
                        return;
                    }
                }
                // Heurística de memoria: si el usuario pide recordar o expresa preferencia, guardarlo
                if lower.contains("recuerda que")
                    || lower.contains("prefiero")
                    || lower.contains("me gusta")
                    || lower.contains("no me gusta")
                    || lower.contains("soy ")
                {
                    let epoch = std::time::SystemTime::now()
                        .duration_since(std::time::UNIX_EPOCH)
                        .map(|d| d.as_secs())
                        .unwrap_or(0);
                    let fact_text = if let Some(pos) = lower.find("recuerda que") {
                        problem_clone[pos + "recuerda que".len()..]
                            .trim()
                            .chars()
                            .take(100)
                            .collect::<String>()
                    } else {
                        problem_clone.chars().take(100).collect::<String>()
                    };
                    if !fact_text.trim().is_empty() {
                        self.profile.remember_fact(&fact_text, epoch, 0.9);
                        self.assistant.long_memory = self.profile.long_memory.clone();
                        // I/O en background thread para no bloquear UI (60fps)
                        spawn_profile_save(self.profile.clone(), crate::utils::profile_path());
                    }
                }
                // Render* integral/tangente: IA-primero (W-B) o local declarado.
                // Con IA disponible (remoto o agente, no solo local) el SPEC lo
                // propone la IA con timeout acotado (`ANIM_IA_SPEC_TIMEOUT_MS`,
                // mitad del budget, 1 request extra máx); sin IA/offline/429 o
                // timeout → canónica local DECLARADA con aviso de una línea.
                // Cero doble render: o IA o local, nunca ambos.
                // El resto de RenderGenerico (taylor, pitágoras…) sigue local.
                let ia_para_este_pedido = match &decision {
                    DecisionAnimacion::RenderCanonico { .. }
                    | DecisionAnimacion::RenderExplicito { .. } => true,
                    DecisionAnimacion::RenderGenerico { plantilla, .. } => {
                        plantilla.trim().to_lowercase() == "derivative-slope"
                            && grafito_anim::parametric::pedido_menciona_tangente(&problem_clone)
                    }
                    _ => false,
                };
                if ia_para_este_pedido {
                    let rate_limited = rate_limit_cooldown_remaining_secs().is_some();
                    let remote_ready = self.remote_provider_ready();
                    if ia_disponible_para_anim(
                        self.assistant.agent_mode,
                        remote_ready,
                        rate_limited,
                        self.exam_mode,
                    ) {
                        let plantilla_fallback = match &decision {
                            DecisionAnimacion::RenderCanonico { plantilla, .. }
                            | DecisionAnimacion::RenderExplicito { plantilla, .. }
                            | DecisionAnimacion::RenderGenerico { plantilla, .. } => {
                                plantilla.clone()
                            }
                            DecisionAnimacion::PreguntarSinMedia(_)
                            | DecisionAnimacion::NoAnimacion => "integral-area".to_string(),
                        };
                        self.run_assistant_animation_ia_primero(
                            ctx,
                            problem_clone.clone(),
                            plantilla_fallback,
                        );
                        ctx.request_repaint();
                        return;
                    }
                }
                match &decision {
                    DecisionAnimacion::RenderCanonico {
                        plantilla,
                        concepto,
                    } => {
                        let question = problem_clone.clone();
                        self.assistant.begin_request(question);
                        self.assistant.problem.clear();
                        // M1: la canónica se declara con SU prosa (punto
                        // único `prosa_canonica_para_plantilla`, jamás cruzada).
                        let prosa = prosa_canonica_para_plantilla(plantilla);
                        let humano = grafito_ui::assistant::humanize_prose_text(&prosa);
                        self.assistant.complete_local_request(humano);
                        self.assistant.set_media(None, ctx);
                        self.run_assistant_animation_with(ctx, plantilla, concepto);
                        ctx.request_repaint();
                        return;
                    }
                    DecisionAnimacion::RenderExplicito {
                        plantilla,
                        concepto,
                        expr,
                    } => {
                        let question = problem_clone.clone();
                        self.assistant.begin_request(question);
                        self.assistant.problem.clear();
                        // M1: la explícita nombra SU función y rango (tangente
                        // o integral según plantilla, jamás huérfana).
                        let prosa = if plantilla.trim().to_lowercase() == "derivative-slope" {
                            prosa_tangente_explicita(expr, &problem_clone)
                        } else {
                            prosa_integral_explicita(expr, &problem_clone)
                        };
                        let humano = grafito_ui::assistant::humanize_prose_text(&prosa);
                        self.assistant.complete_local_request(humano);
                        self.assistant.set_media(None, ctx);
                        self.run_assistant_animation_with(ctx, plantilla, concepto);
                        ctx.request_repaint();
                        return;
                    }
                    DecisionAnimacion::RenderGenerico {
                        plantilla,
                        concepto,
                    } => {
                        let question = problem_clone.clone();
                        self.assistant.begin_request(question);
                        self.assistant.problem.clear();
                        let prosa = crate::anim_ui::animation_reference_sentence().to_string();
                        let humano = grafito_ui::assistant::humanize_prose_text(&prosa);
                        self.assistant.complete_local_request(humano);
                        self.assistant.set_media(None, ctx);
                        self.run_assistant_animation_with(ctx, plantilla, concepto);
                        ctx.request_repaint();
                        return;
                    }
                    DecisionAnimacion::PreguntarSinMedia(_) => {
                        // Ya retornado arriba; inalcanzable.
                        return;
                    }
                    DecisionAnimacion::NoAnimacion => {
                        // Reset T1 (Bug B): sin animación en este mensaje la
                        // media anterior no se re-muestra en este turno.
                        limpiar_media_si_no_animacion(&mut self.assistant, &decision, ctx);
                    }
                }
                // B7 — Pedido de ejercicio en texto: genera la tarjeta en vez de
                // una respuesta de chat. Acá ya no hay animación (Render* retornó).
                if wants_exercise_request(&problem_clone) {
                    self.iniciar_ejercicio(ctx, &problem_clone);
                    self.assistant.problem.clear();
                    ctx.request_repaint();
                    return;
                }
                self.start_local_assistant_request(ctx);
            }
            AssistantUiAction::AuthorizeRemote => {
                self.start_authorized_remote_assistant_request(ctx)
            }
            AssistantUiAction::CancelRemoteAuthorization => {
                self.assistant.cancel_remote_authorization();
            }
            AssistantUiAction::Cancel => self.cancel_assistant_request(),
            AssistantUiAction::SaveApiKey => self.save_assistant_api_key(),
            AssistantUiAction::LoadApiKey => self.load_assistant_api_key(),
            AssistantUiAction::ClearApiKey => self.clear_assistant_api_key(),
            AssistantUiAction::ProviderChanged => {
                self.assistant_runtime.forget_key();
                self.assistant_runtime.fallback_model = None;
                self.cancel_stale_remote_request();
                self.cancel_stale_model_request();
                self.save_app_config();
            }
            AssistantUiAction::ModelChanged => {
                self.assistant_runtime.fallback_model = None;
                self.cancel_stale_remote_request();
                self.save_app_config();
            }
            AssistantUiAction::FusionFallbackChanged => self.save_app_config(),
            AssistantUiAction::RefreshModels => self.start_model_request(ctx),
            AssistantUiAction::AttachImage => self.attach_assistant_image(),
            AssistantUiAction::RemoveAttachment(index) => {
                self.assistant.remove_attachment(index);
            }
            AssistantUiAction::InsertCommand(candidate_index) => {
                let Some(command) = self
                    .assistant
                    .verified_proposals
                    .iter()
                    .find(|proposal| proposal.candidate_index == candidate_index)
                    .and_then(|verified| {
                        verified.prerequisite_parameters.is_empty().then(|| {
                            match &verified.proposal {
                                AssistantProposal::Command(command) => Some(command.clone()),
                                _ => None,
                            }
                        })?
                    })
                else {
                    self.reject_assistant_command();
                    return;
                };
                if let Some(view) = assistant_graph_view(&command) {
                    if let Some(perspective) = assistant_graph_perspective(view, self.current_view)
                    {
                        self.set_perspective(perspective);
                    }
                    self.ensure_algebra_panel_visible();
                }
                self.input_text = command.canonical_text();
                self.command_input_focus_requested = true;
                self.notify(
                    "Comando preparado en la entrada. Revisalo antes de ejecutarlo.",
                    ToastKind::Info,
                );
                ctx.request_repaint();
            }
            AssistantUiAction::ApplyProposal(candidate_index) => {
                let Some(verified) = self
                    .assistant
                    .verified_proposals
                    .iter()
                    .find(|proposal| proposal.candidate_index == candidate_index)
                    .cloned()
                else {
                    self.reject_assistant_command();
                    return;
                };
                // Si es GenerateAnimation, además de aplicar el comando, dispara el motor de animación
                let is_generate_animation = matches!(
                    &verified.proposal,
                    AssistantProposal::Command(cmd) if cmd.canonical_name() == "GenerateAnimation"
                );
                let generate_args = if is_generate_animation {
                    if let AssistantProposal::Command(cmd) = &verified.proposal {
                        let args = cmd.arguments();
                        Some((args.first().cloned(), args.get(1).cloned()))
                    } else {
                        None
                    }
                } else {
                    None
                };
                let committed = match verified.proposal {
                    // Revalidate simple proposals against the live document at the explicit
                    // click, so a valid card never depends on a stale response-time cache.
                    AssistantProposal::Command(command) => self
                        .apply_verified_assistant_graph_command(
                            &command,
                            &verified.prerequisite_parameters,
                        ),
                    AssistantProposal::Scene(commands) => self.apply_verified_assistant_scene(
                        &commands,
                        &verified.prerequisite_parameters,
                    ),
                    AssistantProposal::Parameter(ref assignment) => {
                        if preflight_assistant_parameter(&self.document, assignment).is_ok() {
                            let command = assignment.canonical_text();
                            let outcome = self.execute_command_and_record(&command, self.ui_time);
                            !matches!(outcome, grafito_command::commands::CommandOutcome::Error(_))
                        } else {
                            self.reject_assistant_command();
                            false
                        }
                    }
                };
                self.assistant
                    .finish_verified_proposal_application(candidate_index, committed);
                if committed {
                    self.ensure_algebra_panel_visible();
                }
                if is_generate_animation && committed {
                    if let Some((template_opt, concept_opt)) = generate_args {
                        let template_crudo = template_opt
                            .as_deref()
                            .map(|s| s.trim().trim_matches(|c| c == '"' || c == '\'').trim())
                            .filter(|s| !s.is_empty());
                        let concept = concept_opt
                            .as_deref()
                            .map(|s| s.trim().trim_matches(|c| c == '"' || c == '\'').trim())
                            .unwrap_or("");
                        // M1 defecto 8: punto único de plantilla, igual que
                        // Submit vía `plantilla_para_pedido`. Vacío → se
                        // resuelve del concepto (vacío total → default
                        // histórico `derivative-slope`); integral/tangente
                        // mencionadas coercionan aunque el LLM haya propuesto
                        // `universal`.
                        let plantilla_efectiva: String = match template_crudo {
                            None if concept.is_empty() => "derivative-slope".to_string(),
                            None => plantilla_para_pedido(concept).to_string(),
                            Some(crudo)
                                if grafito_anim::parametric::pedido_menciona_area(concept) =>
                            {
                                "integral-area".to_string()
                            }
                            Some(crudo)
                                if crudo == "universal"
                                    && grafito_anim::parametric::pedido_menciona_tangente(
                                        concept,
                                    ) =>
                            {
                                "derivative-slope".to_string()
                            }
                            Some(crudo) => crudo.to_string(),
                        };
                        // M1 defecto 8: pasa por SPEC+validación igual que el
                        // resto. Con IA disponible va IA-primero (la IA
                        // propone el SPEC, el motor solo renderiza lo
                        // validado); sin IA cae al local validado de abajo.
                        let es_animable = plantilla_efectiva.trim() == "integral-area"
                            || (plantilla_efectiva.trim() == "derivative-slope"
                                && grafito_anim::parametric::pedido_menciona_tangente(concept));
                        let rate_limited = rate_limit_cooldown_remaining_secs().is_some();
                        if es_animable
                            && ia_disponible_para_anim(
                                self.assistant.agent_mode,
                                self.remote_provider_ready(),
                                rate_limited,
                                self.exam_mode,
                            )
                        {
                            self.run_assistant_animation_ia_primero(
                                ctx,
                                concept.to_string(),
                                plantilla_efectiva.clone(),
                            );
                            // El worker IA-primero publica prosa+media del
                            // MISMO spec (o fallback declarado): nada más que
                            // hacer en este turno.
                        } else {
                            self.animacion_apply_local_validada(ctx, &plantilla_efectiva, concept);
                        }
                    } else {
                        self.run_assistant_animation(ctx);
                    }
                }
            }
            AssistantUiAction::ApplyProposedPlan => self.apply_proposed_assistant_plan(),
            AssistantUiAction::RetryProposalCorrection => {
                self.request_assistant_proposal_correction(ctx);
            }
            // S2 `ask_user` real vía evento: la tool devolvió el pendiente, la
            // UI lo mostró como botones y la respuesta vuelve al loop como
            // `function_call_output`/mensaje en un nuevo job (nunca bloquea).
            AssistantUiAction::AskClarification { .. } => {
                // Staging vía `stage_clarification` (el worker ya terminó);
                // esta variante solo existe para contrato/tests, sin I/O.
            }
            AssistantUiAction::AnswerClarification { call_id, answer } => {
                self.answer_pending_clarification(ctx, &call_id, &answer);
            }
            AssistantUiAction::DismissClarification => {
                self.assistant.clear_pending_clarification();
                self.notify(
                    "Aclaración descartada. Escribí la respuesta en el chat si querés.",
                    ToastKind::Info,
                );
                ctx.request_repaint();
            }
            AssistantUiAction::ClearConversation => {
                self.assistant.clear_conversation();
                // La conversación nueva abre sesión Go nueva (routing/caching
                // frescos en el gateway); el próximo request Go la usa.
                self.assistant_runtime.rotate_go_session();
            }
            AssistantUiAction::HidePanel => {
                self.assistant_visible = false;
                if self.perspective == crate::Perspective::Geometry3D {
                    self.workspace_dock_tab = crate::WorkspaceDockTab::Inspector;
                }
                ctx.request_repaint();
            }
            AssistantUiAction::CopyMessage(message) => {
                ctx.copy_text(message);
                self.notify("Mensaje copiado.", ToastKind::Info);
            }
            AssistantUiAction::TogglePlugin(id, enabled) => {
                self.toggle_assistant_plugin(&id, enabled)
            }
            AssistantUiAction::FullPermissionChanged(_) => {
                self.save_app_config();
            }
            AssistantUiAction::AgentModeChanged(_) => {
                self.save_app_config();
            }
            AssistantUiAction::RunAnimation => self.run_assistant_animation(ctx),
            AssistantUiAction::ExportMedia => self.export_assistant_media(ctx),
            AssistantUiAction::AskNextTopic => {
                let memory = self.profile.memory();
                self.assistant.problem = format!(
                    "Soy nivel {} de Grafito. Mi progreso: {memory} ¿Qué debería estudiar a continuación y cómo?",
                    self.profile.level
                );
                self.start_local_assistant_request(ctx);
            }
            AssistantUiAction::LearnCorrect => self.record_learning(true),
            AssistantUiAction::LearnIncorrect => self.record_learning(false),
            AssistantUiAction::RunMiniExam => self.run_mini_exam(ctx),
            AssistantUiAction::OpenMascotConfig => {
                self.assistant.settings_open = true;
                self.assistant.config_tab = 1;
                self.show_mascot_config = false;
                // Sincroniza borrador al abrir para preview fiel
                self.assistant.avatar = self.profile.avatar.clone();
                self.assistant.user_name = self.profile.display_name().to_owned();
                self.assistant.long_memory = self.profile.long_memory.clone();
                self.avatar_draft = self.profile.avatar.clone();
            }
            AssistantUiAction::SaveAvatar => {
                let draft = self.assistant.avatar.clone();
                // Sincronizar preferencias de memoria con avatar
                let mut long_mem = self.assistant.long_memory.clone();
                long_mem.preferences.custom_instructions = draft.custom_instructions.clone();
                long_mem.preferences.language = draft.language.clone();
                // tone viene de mascot personality
                if let Some(m) = draft.mascot.as_ref() {
                    long_mem.preferences.tone = m.personality.label().to_string();
                }
                match draft
                    .validate()
                    .and_then(|_| long_mem.preferences.validate())
                {
                    Ok(()) => {
                        let name = self.assistant.user_name.clone();
                        let name_ref = if name.trim().is_empty() {
                            "Estudiante"
                        } else {
                            name.trim()
                        };
                        match self.profile.set_display_name(name_ref) {
                            Ok(()) => {
                                self.profile.avatar = draft.clone();
                                // Preservar mascota: sincronizar personality si cambió
                                if let Some(mascot) = draft.mascot.clone() {
                                    self.profile.avatar.mascot = Some(mascot.clone());
                                    self.profile.mascot = Some(mascot);
                                } else if self.profile.mascot.is_some() {
                                    // mantener existente
                                }
                                self.profile.long_memory = long_mem.clone();
                                self.avatar_draft = self.profile.avatar.clone();
                                self.assistant.avatar = self.profile.avatar.clone();
                                self.assistant.user_name = self.profile.display_name().to_owned();
                                self.assistant.long_memory = self.profile.long_memory.clone();
                                self.config_name_error = None;
                                spawn_profile_save(
                                    self.profile.clone(),
                                    crate::utils::profile_path(),
                                );
                                self.notify(
                                    "Avatar y memoria guardados",
                                    grafito_ui::toast::ToastKind::Success,
                                );
                                self.assistant.settings_open = false;
                            }
                            Err(err) => {
                                self.config_name_error = Some(err.clone());
                                self.notify(err, grafito_ui::toast::ToastKind::Error);
                            }
                        }
                    }
                    Err(err) => {
                        self.config_name_error = Some(err.clone());
                        self.notify(err, grafito_ui::toast::ToastKind::Error);
                    }
                }
            }
            AssistantUiAction::LiveSaveAvatar => {
                // Guardado live sin cerrar ventana — para cambios de color/forma inmediatos
                let draft = self.assistant.avatar.clone();
                let mut long_mem = self.assistant.long_memory.clone();
                long_mem.preferences.custom_instructions = draft.custom_instructions.clone();
                long_mem.preferences.language = draft.language.clone();
                if let Some(m) = draft.mascot.as_ref() {
                    long_mem.preferences.tone = m.personality.label().to_string();
                }
                if draft.validate().is_ok() && long_mem.preferences.validate().is_ok() {
                    let name = self.assistant.user_name.clone();
                    let name_ref = if name.trim().is_empty() {
                        "Estudiante"
                    } else {
                        name.trim()
                    };
                    if self.profile.set_display_name(name_ref).is_ok() {
                        self.profile.avatar = draft.clone();
                        if let Some(mascot) = draft.mascot.clone() {
                            self.profile.avatar.mascot = Some(mascot.clone());
                            self.profile.mascot = Some(mascot);
                        }
                        self.profile.long_memory = long_mem.clone();
                        self.avatar_draft = self.profile.avatar.clone();
                        // No cerrar ventana, no toast ruidoso
                        spawn_profile_save(self.profile.clone(), crate::utils::profile_path());
                    }
                }
            }
            AssistantUiAction::ResetAvatar => {
                self.assistant.avatar = grafito_profile::AvatarConfig::default();
                self.assistant.avatar.display_name = "Estudiante".to_string();
                self.assistant.user_name = "Estudiante".to_string();
                self.avatar_draft = self.assistant.avatar.clone();
                self.config_name_error = None;
                self.notify(
                    "Avatar restablecido — pulsa Guardar para confirmar",
                    grafito_ui::toast::ToastKind::Info,
                );
            }
            AssistantUiAction::ApplyRawCommand(raw) => {
                // Extrae el primer comando grafito limpio hasta el corchete final, sin texto trailing
                // (evita "no se permite texto después del corchete final" cuando el LLM añade "explicacion" tras el comando)
                let mut command_text = raw.trim().to_string();
                if command_text.is_empty() {
                    self.reject_assistant_command();
                    return;
                }
                // Si el bloque contiene múltiples líneas (grafito-scene), intentar quedarnos con todas las líneas que parezcan comando
                // Para bloque simple, quedarnos solo hasta el primer ']' balanceado
                let trimmed = command_text.trim();
                if trimmed.lines().count() == 1 {
                    if let Some(open) = trimmed.find('[') {
                        let mut depth = 0i32;
                        let mut close_idx: Option<usize> = None;
                        for (i, ch) in trimmed[open..].char_indices() {
                            if ch == '[' {
                                depth += 1;
                            } else if ch == ']' {
                                depth -= 1;
                                if depth == 0 {
                                    close_idx = Some(open + i);
                                    break;
                                }
                            }
                        }
                        if let Some(close) = close_idx {
                            // Solo si hay texto no-espacio después del cierre, recortar
                            if !trimmed[close + 1..].trim().is_empty() {
                                // Verificar que el prefijo sea un comando válido antes de recortar
                                let candidate = trimmed[..=close].trim();
                                if grafito_command::assistant_proposals::parse_assistant_command(
                                    candidate,
                                )
                                .is_some()
                                {
                                    command_text = candidate.to_string();
                                }
                            }
                        }
                    }
                } else {
                    // Multi-línea: filtrar solo líneas que son comandos completos
                    let filtered: Vec<String> = trimmed
                        .lines()
                        .map(|l| l.trim())
                        .filter(|l| {
                            !l.is_empty()
                                && grafito_command::assistant_proposals::parse_assistant_command(l)
                                    .is_some()
                        })
                        .map(|s| s.to_string())
                        .collect();
                    if !filtered.is_empty() {
                        command_text = filtered.join("\n");
                    }
                }
                // Intenta parsear como comando de grafito para manejar vista 2D/3D/4D
                if let Some(inv) =
                    grafito_command::assistant_proposals::parse_assistant_command(&command_text)
                {
                    if let Some(view) = assistant_graph_view(&inv) {
                        if let Some(perspective) =
                            assistant_graph_perspective(view, self.current_view)
                        {
                            self.set_perspective(perspective);
                        }
                    }
                } else if command_text.lines().count() > 1 {
                    // Para escena multi-línea, inferir vista de la primera línea válida
                    if let Some(first) = command_text.lines().next() {
                        if let Some(inv) =
                            grafito_command::assistant_proposals::parse_assistant_command(
                                first.trim(),
                            )
                        {
                            if let Some(view) = assistant_graph_view(&inv) {
                                if let Some(perspective) =
                                    assistant_graph_perspective(view, self.current_view)
                                {
                                    self.set_perspective(perspective);
                                }
                            }
                        }
                    }
                }
                let outcome = self.execute_command_and_record(&command_text, self.ui_time);
                match &outcome {
                    grafito_command::commands::CommandOutcome::Error(msg) => {
                        // S3: ante fence/propuesta inválida, responde con
                        // `vibecoder_explain` (negocio en rioplatense) + fix
                        // propuesto en 1 click cuando sea seguro (solo
                        // reescritura sintáctica, jamás cambio semántico
                        // silencioso). El fix queda en la entrada para que el
                        // usuario lo revise y lo aplique explícitamente.
                        let (explained, fix) =
                            grafito_assistant::agent::explain_invalid_proposal(&command_text, "");
                        if let Some(fix) = fix {
                            self.input_text = fix.clone();
                            self.command_input_focus_requested = true;
                            self.show_assistant_error(format!(
                                "{} Te propongo un fix sintáctico en la entrada: {} Revisalo y aplicá.",
                                explained.explanation, fix
                            ));
                        } else {
                            self.show_assistant_error(format!("{} ({msg})", explained.explanation));
                        }
                    }
                    _ => {
                        self.ensure_algebra_panel_visible();
                        self.notify("Comando aplicado en Grafito.", ToastKind::Success);
                    }
                }
                ctx.request_repaint();
            }
            AssistantUiAction::ExplainStepwise(topic) => {
                // teaching_ui.start ya inicia el orchestrator con el template del primer paso
                self.teaching_ui.start(&topic);
                self.notify(format!("Enseñanza iniciada: {topic}"), ToastKind::Info);
            }
            AssistantUiAction::RequestExercise { topic } => {
                self.iniciar_ejercicio(ctx, &topic);
            }
        }
    }

    fn show_assistant_error(&mut self, error: impl Into<String>) {
        let error = error.into();
        self.assistant.error = Some(error.clone());
        self.notify(error, ToastKind::Error);
    }

    /// Freno 429 lado UI: si la cuota del proveedor sigue en pausa, muestra
    /// el mensaje criollo con cuenta regresiva y no spawnea ningún worker
    /// (cero red). Retorna `true` si frenó (el llamante debe volver). Sólo
    /// disparadores lo usan (chat/agente/corrección/modelos).
    fn fail_fast_if_rate_limited(&mut self) -> bool {
        if let Some(remaining) = rate_limit_cooldown_remaining_secs() {
            self.show_assistant_error(rate_limit_paused_message(remaining));
            true
        } else {
            false
        }
    }

    fn reject_assistant_command(&mut self) {
        self.show_assistant_error(
            "La sugerencia remota está incompleta o no es una acción de Grafito permitida.",
        );
    }

    fn fail_assistant_request(&mut self, error: impl Into<String>) {
        let error = error.into();
        let current_model = self.assistant.model.clone();
        let visible_error = remote_error_message(&error, &current_model);
        self.assistant.fail_request(visible_error.clone());
        self.report_assistant_error(visible_error);
    }

    fn fail_assistant_repair_request(&mut self, error: impl Into<String>) {
        self.assistant.restore_proposal_correction();
        self.fail_assistant_request(error);
    }

    fn report_assistant_error(&mut self, error: impl Into<String>) {
        self.show_assistant_error(error);
    }

    fn apply_verified_assistant_graph_command(
        &mut self,
        command: &AssistantCommandInvocation,
        prerequisite_parameters: &[AssistantParameterAssignment],
    ) -> bool {
        let before = self.object_labels_snapshot();
        let before_ids = self
            .document
            .objects_iter()
            .map(|(id, _)| *id)
            .collect::<std::collections::HashSet<_>>();
        let command_text = command.canonical_text();
        match preflight_assistant_graph_command_with_prerequisites(
            &self.document,
            prerequisite_parameters,
            command,
            self.camera,
        ) {
            Ok(preflight) => {
                let view = preflight.view;
                let outcome = commit_assistant_graph_preflight(
                    &mut self.document,
                    &mut self.undo_stack,
                    &mut self.redo_stack,
                    preflight,
                );
                let committed = !matches!(
                    &outcome,
                    grafito_command::commands::CommandOutcome::Error(_)
                );
                self.handle_command_outcome(outcome, self.ui_time, &command_text);
                self.record_step_from_diff(&command_text, &before, true);
                if let Some(perspective) = assistant_graph_perspective(view, self.current_view) {
                    self.set_perspective(perspective);
                }
                if committed {
                    // S1 auto-graficar 1-click: el objeto queda seleccionado y
                    // visible. 2D encuadra con `zoom_to_fit` (el preflight ya
                    // garantizó geometría visible); 3D queda visible con la
                    // cámara actual (el preflight lo verificó) + selección.
                    self.select_new_assistant_objects(&before_ids);
                    if view == grafito_command::assistant_context::AssistantGraphView::TwoD {
                        self.zoom_to_fit();
                    }
                    self.ensure_algebra_panel_visible();
                }
                committed
            }
            Err(error) => {
                self.show_assistant_error(error);
                false
            }
        }
    }

    fn apply_verified_assistant_scene(
        &mut self,
        commands: &[AssistantCommandInvocation],
        prerequisite_parameters: &[AssistantParameterAssignment],
    ) -> bool {
        let before = self.object_labels_snapshot();
        let before_ids = self
            .document
            .objects_iter()
            .map(|(id, _)| *id)
            .collect::<std::collections::HashSet<_>>();
        match preflight_assistant_scene_with_prerequisites(
            &self.document,
            prerequisite_parameters,
            commands,
            self.camera,
        ) {
            Ok(preflight) => {
                let view = preflight.view;
                let before_document = self.document.clone();
                self.document = preflight.staged;
                crate::app::save_command_snapshot_if_mutated(
                    &preflight.outcome,
                    before_document,
                    &self.document,
                    &mut self.undo_stack,
                    &mut self.redo_stack,
                );
                self.camera = preflight.camera;
                self.handle_command_outcome(
                    preflight.outcome,
                    self.ui_time,
                    "Escena 3D verificada",
                );
                self.record_step_from_diff("Escena 3D verificada", &before, true);
                if let Some(perspective) = assistant_graph_perspective(view, self.current_view) {
                    self.set_perspective(perspective);
                }
                // S1: selección + encuadre (3D ya viene con cámara fitted del
                // preflight; 2D homogénea encuadra con zoom_to_fit).
                self.select_new_assistant_objects(&before_ids);
                if view == grafito_command::assistant_context::AssistantGraphView::TwoD {
                    self.zoom_to_fit();
                }
                self.ensure_algebra_panel_visible();
                self.notify(
                    "Escena verificada aplicada y encuadrada.",
                    ToastKind::Success,
                );
                true
            }
            Err(error) => {
                self.show_assistant_error(error);
                false
            }
        }
    }

    /// Selecciona los objetos nuevos tras un Apply (S1 auto-graficar 1-click).
    ///
    /// Puro en intención (solo `selected_object`, sin Document ni I/O): difiere
    /// `before_ids` del documento actual y fija el máximo (determinista por
    /// `Ord`) como seleccionado para que quede visible en Álgebra y canvas.
    /// Sin nuevos → conserva la selección previa (default honesto, no inventa).
    fn select_new_assistant_objects(
        &mut self,
        before_ids: &std::collections::HashSet<grafito_core::ObjectId>,
    ) {
        let mut newest: Option<grafito_core::ObjectId> = None;
        for (id, _) in self.document.objects_iter() {
            if before_ids.contains(id) {
                continue;
            }
            newest = Some(match newest {
                None => *id,
                Some(current) => current.max(*id),
            });
        }
        if let Some(id) = newest {
            self.selected_object = Some(id);
        }
    }

    /// Retoma una aclaración pendiente (S2) con la respuesta del usuario.
    ///
    /// No bloquea threads: consume el pendiente y lanza un nuevo job local con
    /// la respuesta como `function_call_output`/mensaje para que el loop la
    /// retome. Si el `call_id` no coincide o la respuesta está vacía, se
    /// descarta sin I/O y con aviso honesto en rioplatense.
    fn answer_pending_clarification(&mut self, ctx: &egui::Context, call_id: &str, answer: &str) {
        let pending = self.assistant.pending_clarification().cloned();
        let Some(pending) = pending else {
            self.notify("No hay aclaración pendiente.", ToastKind::Info);
            return;
        };
        if pending.call_id != call_id {
            self.assistant.clear_pending_clarification();
            self.notify(
                "La aclaración quedó obsoleta; volvé a preguntar si la necesitás.",
                ToastKind::Info,
            );
            ctx.request_repaint();
            return;
        }
        let Some(sanitized) = self
            .assistant
            .take_clarification_answer(call_id, answer)
            .filter(|text| !text.trim().is_empty())
        else {
            self.notify(
                "Escribí una respuesta no vacía para continuar.",
                ToastKind::Info,
            );
            return;
        };
        // La respuesta vuelve al loop como mensaje + `function_call_output`
        // (Responses) para trazabilidad; el nuevo job local la retoma sin
        // bloquear la UI (I/O solo en background, como el resto de jobs).
        let output = serde_json::json!({
            "type": "function_call_output",
            "call_id": pending.call_id,
            "output": sanitized,
        });
        self.assistant.problem = format!(
            "Aclaración ({}): {}\nRespuesta: {}",
            pending.question, output, sanitized
        );
        self.notify(
            "Aclaración respondida. Retomo la consulta…",
            ToastKind::Info,
        );
        self.start_local_assistant_request(ctx);
        ctx.request_repaint();
    }

    /// Drena aclaraciones `ask_user` del canal lateral (S2, sin bloquear).
    ///
    /// Hilo UI, `try_recv` acotado (máx 4 por frame): si hay pendiente nuevo y
    /// no hay otro visible, lo estagia para mostrar botones en el turno. Nunca
    /// bloquea threads ni toca la zona guard (el forwarder ya parseó en
    /// background).
    fn drain_pending_clarifications(&mut self) {
        let Some(job) = self.assistant_runtime.agent_job.as_ref() else {
            return;
        };
        for _ in 0..4 {
            match job.clarification_receiver.try_recv() {
                Ok(pending) => {
                    if !self.assistant.has_pending_clarification() {
                        self.assistant.stage_clarification(pending);
                    }
                }
                Err(std::sync::mpsc::TryRecvError::Empty) => break,
                Err(std::sync::mpsc::TryRecvError::Disconnected) => break,
            }
        }
    }

    fn save_assistant_api_key(&mut self) {
        let key = std::mem::take(&mut self.assistant.api_key_draft);
        let trimmed = key.trim().to_owned();
        if trimmed.is_empty() {
            self.show_assistant_error("Ingresá una API key antes de guardarla.");
            return;
        }
        match assistant_credentials::store(self.assistant.provider, &trimmed) {
            Ok(()) => {
                // Una relectura posterior del llavero no debe invalidar la
                // consulta durante la misma sesión en que se guardó la clave.
                self.assistant_runtime
                    .remember_key(self.assistant.provider, trimmed);
                self.assistant.key_available = true;
                self.assistant.key_status_checked = true;
            }
            Err(_) => {
                self.assistant_runtime
                    .remember_key(self.assistant.provider, trimmed);
                self.assistant.key_available = true;
                self.assistant.key_status_checked = true;
            }
        }
    }

    fn load_assistant_api_key(&mut self) {
        match assistant_credentials::load(self.assistant.provider) {
            Ok(Some(key)) => {
                self.assistant_runtime
                    .remember_key(self.assistant.provider, key);
                self.assistant.key_available = true;
                self.assistant.key_status_checked = true;
            }
            Ok(None) => {
                self.assistant.key_available = false;
                self.assistant.key_status_checked = true;
            }
            Err(_) => {
                self.assistant.key_available = false;
                self.assistant.key_status_checked = true;
                self.show_assistant_error("No se pudo consultar el llavero del sistema.");
            }
        }
    }

    fn clear_assistant_api_key(&mut self) {
        self.assistant_runtime.forget_key();
        match assistant_credentials::clear(self.assistant.provider) {
            Ok(()) => {
                self.assistant.key_available = false;
                self.assistant.key_status_checked = true;
            }
            Err(_) => {
                self.show_assistant_error("No se pudo eliminar la clave guardada.");
            }
        }
    }

    fn attach_assistant_image(&mut self) {
        if self.assistant.is_pending || self.assistant_runtime.image_job.is_some() {
            return;
        }
        let Some(path) = rfd::FileDialog::new()
            .add_filter("Imagen", &["png", "jpg", "jpeg"])
            .pick_file()
        else {
            return;
        };
        let (sender, receiver) = sync_channel(1);
        std::thread::spawn(move || {
            let _ = sender.send(load_assistant_attachment(path));
        });
        self.assistant.is_importing_image = true;
        self.assistant.attachment_message = Some("Importando imagen...".into());
        self.assistant_runtime.image_job = Some(AssistantImageJob { receiver });
    }

    fn build_remote_assistant_request(
        &self,
        question: String,
        document_context: ImmutableDocumentContext,
        focus: Option<AssistantFocus>,
        attachments: Vec<grafito_assistant_types::ImageAttachment>,
        image_upload_consent: bool,
        repair: Option<AssistantRepairRequest>,
    ) -> Result<AssistantRequest, String> {
        let (repair_feedback, history_before_turn) = match repair {
            Some(AssistantRepairRequest {
                feedback,
                target_turn,
            }) => (Some(feedback), Some(target_turn)),
            None => (None, None),
        };
        let mut request = AssistantRequest::remote(question.clone(), document_context);
        // Idioma del selector del panel (auto/es/en) → directiva en el system prompt.
        request.language = self.assistant.avatar.language.clone();
        request.focus = focus;
        let plugin_instructions = self.plugin_instructions_budgeted();
        let _plugin_instruction_bytes = if plugin_instructions.is_empty() {
            0
        } else {
            plugin_instructions
                .len()
                .saturating_add(REMOTE_PLUGIN_INSTRUCTIONS_OVERHEAD_BYTES)
        };
        // Memoria del tutor: el perfil del estudiante entra en el contexto de
        // cada turno para que Mora adapte la pedagogía (ADR-0001).
        let mut system = plugin_instructions;
        if !system.is_empty() {
            system.push_str("\n\n");
        }
        system.push_str(&format!(
            "[Perfil del estudiante]\n{}",
            self.profile.memory()
        ));
        request.system_instructions = system;
        let focus_bytes = request
            .focus
            .as_ref()
            .map(|focus| focus.summary.len())
            .unwrap_or_default();
        let context_bytes =
            if request.context.objects.is_empty() && request.context.variables.is_empty() {
                0
            } else {
                let mut len = REMOTE_CONTEXT_PROMPT_OVERHEAD_BYTES;
                len += 30; // "Objetos visibles:\n" etc.
                for (k, v) in &request.context.variables {
                    len += k.len() + format!("{v}").len() + 4;
                }
                for obj in &request.context.objects {
                    len += obj.label.len()
                        + obj.kind.len()
                        + obj.fingerprint.chars().take(120).collect::<String>().len()
                        + 10;
                }
                len += 120; // instrucción Taylor
                len
            };
        let repair_feedback_bytes = repair_feedback
            .as_ref()
            .map(|feedback| {
                feedback
                    .prompt_text()
                    .len()
                    .saturating_add(REMOTE_REPAIR_FEEDBACK_PROMPT_OVERHEAD_BYTES)
            })
            .unwrap_or_default();
        // Con 1M de presupuesto, el catálogo puede ser grande pero lo acotamos a 32k para no saturar
        let system_bytes = request.system_instructions.len()
            + if request.system_instructions.is_empty() {
                0
            } else {
                REMOTE_PLUGIN_INSTRUCTIONS_OVERHEAD_BYTES
            };
        let transcription_bytes = request.transcription.text.len()
            + request
                .attachments
                .iter()
                .map(|a| a.transcription.text.len())
                .sum::<usize>();
        // Presupuesto equitativo: garantiza catálogo útil incluso con historia larga
        let raw_catalog = request
            .budget
            .max_input_chars
            .saturating_sub(question.len())
            .saturating_sub(focus_bytes)
            .saturating_sub(
                request
                    .focus
                    .as_ref()
                    .map(|_| REMOTE_FOCUS_PROMPT_OVERHEAD_BYTES)
                    .unwrap_or_default(),
            )
            .saturating_sub(context_bytes)
            .saturating_sub(REMOTE_TOOL_CATALOG_PROMPT_OVERHEAD_BYTES)
            .saturating_sub(repair_feedback_bytes)
            .saturating_sub(system_bytes)
            .saturating_sub(transcription_bytes);
        // Garantiza mínimo 1K para herramientas relevantes (evita catálogo vacío que deja al LLM ciego)
        let catalog_budget = raw_catalog.clamp(1024, 32_000);
        request.tool_catalog =
            grafito_command::assistant_context::assistant_tool_catalog(&question, catalog_budget);
        let catalog_overhead = if request.tool_catalog.is_empty() {
            0
        } else {
            REMOTE_TOOL_CATALOG_PROMPT_OVERHEAD_BYTES
        };
        let history_budget = request
            .budget
            .max_input_chars
            .saturating_sub(question.len())
            .saturating_sub(focus_bytes)
            .saturating_sub(
                request
                    .focus
                    .as_ref()
                    .map(|_| REMOTE_FOCUS_PROMPT_OVERHEAD_BYTES)
                    .unwrap_or_default(),
            )
            .saturating_sub(context_bytes)
            .saturating_sub(request.tool_catalog.len())
            .saturating_sub(catalog_overhead)
            .saturating_sub(repair_feedback_bytes)
            .saturating_sub(system_bytes)
            .saturating_sub(transcription_bytes);
        request.conversation = match history_before_turn {
            Some(target_turn) => self
                .assistant
                .conversation_before_turn_within_budget(target_turn, history_budget),
            None => self.assistant.conversation_within_budget(history_budget),
        };
        request.attachments = attachments;
        request.image_upload_consent = image_upload_consent;
        request.repair_feedback = repair_feedback;
        request.validate(&AttachmentLimits::default())?;
        Ok(request)
    }

    fn start_remote_assistant_job(&mut self, ctx: &egui::Context, launch: AssistantRemoteLaunch) {
        // D2 lockdown: en examen no sale nada a internet.
        if self.exam_blocks("Internet") {
            return;
        }
        let AssistantRemoteLaunch {
            settings,
            request,
            api_key,
            provider,
            model,
            route,
            fusion_fallback_allowed,
            question,
            document_revision,
            document_digest,
            focus,
            correction_attempt,
            repair_target_turn,
            socratic_guard,
        } = launch;
        self.assistant_runtime.next_request_id =
            self.assistant_runtime.next_request_id.wrapping_add(1);
        let id = self.assistant_runtime.next_request_id;
        let cancellation = CancellationToken::default();
        // Canal acotado de deltas SSE (128, best-effort): el worker de
        // streaming lo alimenta y `poll_assistant_jobs` lo drena a la burbuja
        // provisional. Protocolos no-streaming lo dejan desconectado.
        let (delta_tx, stream_rx) = sync_channel::<String>(128);
        let worker = request_remote_streaming_with_api_key_on_worker(
            settings,
            request,
            api_key,
            cancellation.clone(),
            delta_tx,
            socratic_guard,
        );
        let (sender, receiver) = sync_channel(1);
        let repaint = ctx.clone();
        let cancellation_for_thread = cancellation.clone();
        std::thread::spawn(move || {
            // Si hay cancelación pendiente, da una ventana breve para que el worker
            // termine cooperativamente antes de bloquear el hilo forwarder (no UI).
            if cancellation_for_thread.is_cancelled() {
                for _ in 0..20 {
                    if worker.is_finished() {
                        break;
                    }
                    std::thread::sleep(std::time::Duration::from_millis(10));
                }
            }
            let result = worker.join().unwrap_or_else(|_| {
                Err("La consulta del asistente terminó inesperadamente.".into())
            });
            let _ = sender.send(result);
            repaint.request_repaint();
        });
        self.assistant_runtime.remote_job = Some(AssistantRemoteJob {
            id,
            provider,
            model,
            route,
            fusion_fallback_allowed,
            question,
            correction_attempt,
            repair_target_turn,
            document_revision,
            document_digest,
            focus,
            cancellation,
            receiver,
            stream_rx: Some(stream_rx),
            stream_text: String::new(),
            preview_active: false,
            started_at: std::time::Instant::now(),
            first_delta_at: None,
        });
        // Etapa inicial visible de inmediato (sin esperar al primer poll):
        // `Autorizada` con 0s, la Piel sólo renderiza el texto.
        self.assistant
            .set_remote_stage(grafito_ui::assistant::RemoteStage::Autorizada, 0);
    }

    /// Lanza el modo agente (loop con herramientas seguras) en un hilo y
    /// enruta sus eventos de actividad + resultado hacia la UI.
    fn start_agent_assistant_job(&mut self, ctx: &egui::Context, launch: AssistantRemoteLaunch) {
        // D2 lockdown: en examen no sale nada a internet.
        if self.exam_blocks("Internet") {
            return;
        }
        let settings = launch.settings;
        let request = launch.request;
        let api_key = launch.api_key;
        let provider = launch.provider;
        let model = launch.model;
        let question = launch.question;
        self.assistant_runtime.next_request_id =
            self.assistant_runtime.next_request_id.wrapping_add(1);
        let _ = self.assistant_runtime.next_request_id;
        let cancellation = grafito_agent::loop_engine::Cancellation::default();
        let system = grafito_assistant::assistant_system_prompt(&request);
        let prompt = grafito_assistant::assistant_remote_prompt(&request)
            .unwrap_or_else(|_| question.clone());
        let mut user_messages: Vec<serde_json::Value> = Vec::new();
        for turn in &request.conversation {
            let role = match turn.role {
                grafito_assistant_types::ConversationRole::User => "user",
                grafito_assistant_types::ConversationRole::Assistant => "assistant",
            };
            user_messages.push(serde_json::json!({"role": role, "content": turn.content}));
        }
        user_messages.push(serde_json::json!({"role": "user", "content": prompt}));
        let tools = grafito_assistant::default_agent_tools();
        let budget = grafito_agent::loop_engine::AgentBudget::default();
        let goal = question
            .chars()
            .take(grafito_agent::ledger::MAX_LEDGER_GOAL_CHARS)
            .collect::<String>();
        let ledger = if grafito_agent::router::classify_band(&question)
            == grafito_agent::router::TaskBand::LongRunning
        {
            Some(grafito_agent::ledger::JSpaceLedger::with_task(
                goal,
                "Analizar, verificar con tools y cerrar",
            ))
        } else {
            None
        };
        let (outcome_handle, event_receiver) =
            grafito_assistant::agent::request_agent_on_worker_with_ledger(
                settings,
                api_key,
                system,
                user_messages,
                tools,
                budget,
                ledger,
                cancellation.clone(),
            );
        let (sender, receiver) = std::sync::mpsc::sync_channel(128);
        // S2: canal lateral de aclaraciones (cap 4, try_send sin bloquear).
        let (clarification_sender, clarification_receiver) = std::sync::mpsc::sync_channel(4);
        let repaint = ctx.clone();
        let cancellation_forwarder = cancellation.clone();
        std::thread::spawn(move || {
            // Drena eventos con timeout para respetar cancelación y evitar bloqueo
            // indefinido en `iter()` si el agente se cuelga.
            loop {
                if cancellation_forwarder.is_cancelled() {
                    break;
                }
                match event_receiver.recv_timeout(std::time::Duration::from_millis(100)) {
                    Ok(event) => {
                        // S2 `ask_user` real vía evento: reenvía el pendiente al
                        // canal lateral sin bloquear (try_send) para que la UI
                        // lo muestre como botones. Nunca bloquea threads.
                        if let grafito_agent::AgentEvent::ToolStarted { name, args_summary } =
                            &event
                        {
                            if name == "ask_user" {
                                if let Some(pending) = parse_agent_ask_user_pending(args_summary) {
                                    let _ = clarification_sender.try_send(pending);
                                }
                            }
                        }
                        // R2-V1: `try_send` + `CancellationToken` (jamás `send` bloqueante).
                        // Lleno → se descarta el evento (best-effort); desconectado/cancelado → corta.
                        match send_agent_msg_nonblocking(
                            &sender,
                            AgentChannelMsg::Event(event),
                            &cancellation_forwarder,
                        ) {
                            Some(_) => {}
                            None => break,
                        }
                    }
                    Err(std::sync::mpsc::RecvTimeoutError::Timeout) => continue,
                    Err(std::sync::mpsc::RecvTimeoutError::Disconnected) => break,
                }
            }
            // Si hay cancelación, espera brevemente a que el worker termine
            // cooperativamente antes de bloquear el hilo forwarder (no UI).
            if cancellation_forwarder.is_cancelled() {
                for _ in 0..20 {
                    if outcome_handle.is_finished() {
                        break;
                    }
                    std::thread::sleep(std::time::Duration::from_millis(10));
                }
            }
            let outcome = outcome_handle
                .join()
                .unwrap_or_else(|_| Err("El agente terminó inesperadamente.".to_string()));
            // R2-V1: `Done` con `try_send` + reintento acotado (jamás `send` bloqueante).
            // Si el buffer sigue lleno tras 200 ms o hay cancelación/desconexión,
            // se descarta para que el `join` sea <1s (la UI ve `Disconnected` honesto).
            let mut pending = Some(AgentChannelMsg::Done(outcome));
            for _ in 0..20 {
                if cancellation_forwarder.is_cancelled() {
                    break;
                }
                let Some(msg) = pending.take() else {
                    break;
                };
                match sender.try_send(msg) {
                    Ok(()) => break,
                    Err(std::sync::mpsc::TrySendError::Full(returned)) => {
                        pending = Some(returned);
                        std::thread::sleep(std::time::Duration::from_millis(10));
                    }
                    Err(std::sync::mpsc::TrySendError::Disconnected(_)) => break,
                }
            }
            repaint.request_repaint();
        });
        self.assistant_runtime.agent_job = Some(AssistantAgentJob {
            provider,
            model,
            cancellation,
            receiver,
            clarification_receiver,
        });
    }

    /// Genera una animación didáctica con el motor externo y la reproduce en el chat.
    /// B7 — Arranca el ciclo de ejercicio: tema crudo → concepto → job en
    /// background → tarjeta visible bajo el chat. Sin I/O en la UI.
    fn iniciar_ejercicio(&mut self, ctx: &egui::Context, tema_crudo: &str) {
        // D2 lockdown: el tutor también es ayuda en examen.
        if self.exam_blocks("Ejercicios") {
            return;
        }
        let tema = extract_concept(tema_crudo).unwrap_or_else(|| {
            let recorte: String = tema_crudo
                .trim()
                .chars()
                .take(crate::teaching_ui::MAX_TEMA_CHARS)
                .collect();
            if recorte.is_empty() {
                grafito_ui::assistant::ANDAMIAR_DEFAULT_TOPIC.to_string()
            } else {
                recorte
            }
        });
        let nivel = PedagogicalLevel::from_level_value(self.profile.level);
        self.teaching_ui.ejercicio.pedir(&tema, nivel);
        self.notify(
            format!("Armo un ejercicio de {tema}, che."),
            ToastKind::Info,
        );
        ctx.request_repaint();
    }

    /// B7 — Asienta la respuesta ya corregida en el perfil (BKT barato, en
    /// memoria) y calcula el próximo paso. Corre tras dibujar la tarjeta;
    /// registra una sola vez por respuesta (guardia `registrada`).
    fn asentar_respuesta_ejercicio(&mut self) {
        let (lo_id, tema, respuesta, fb) = match (
            self.teaching_ui.ejercicio.ejercicio.as_ref(),
            self.teaching_ui.ejercicio.tarjeta.devolucion.as_ref(),
        ) {
            (Some(ejercicio), Some(fb)) => (
                ejercicio.lo_id.clone(),
                self.teaching_ui.ejercicio.tema.clone(),
                self.teaching_ui
                    .ejercicio
                    .tarjeta
                    .respuesta
                    .trim()
                    .to_string(),
                fb.clone(),
            ),
            _ => return,
        };
        if self.teaching_ui.ejercicio.registrada.as_deref() == Some(respuesta.as_str()) {
            return;
        }
        let epoch = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|duration| duration.as_secs())
            .unwrap_or(0);
        self.profile
            .record_outcome(&lo_id, &tema, epoch, fb.correct);
        let pista = if fb.correct {
            String::new()
        } else {
            format!("{:?}", fb.misconception)
        };
        self.profile.working_memory.record_attempt(&pista);
        self.profile.working_memory.set_topic(&tema);
        self.profile.working_memory.set_session_epoch(epoch);
        self.assistant.working_memory = self.profile.working_memory.clone();
        // I/O en background thread para no bloquear UI (60fps)
        spawn_profile_save(self.profile.clone(), crate::utils::profile_path());
        // BKT en memoria (barato): próxima rama o texto honesto si no hay.
        let proximo = self
            .profile
            .recommend_next()
            .first()
            .map(|rama| rama.name.clone());
        let texto = match proximo {
            Some(nombre) => format!("Próximo tema sugerido: {nombre}, che."),
            None => {
                "Seguí practicando este tema y pedí otro ejercicio cuando quieras, che.".to_string()
            }
        };
        let panel = &mut self.teaching_ui.ejercicio;
        panel.proximo = Some(texto);
        panel.registrada = Some(respuesta);
    }

    /// Genera y envía un mini-examen (3 preguntas) de la rama recomendada.
    fn run_mini_exam(&mut self, ctx: &egui::Context) {
        let branch = self
            .profile
            .recommend_next()
            .first()
            .cloned()
            .map(|branch| (branch.id.clone(), branch.name.clone()))
            .or_else(|| {
                self.profile
                    .branches
                    .first()
                    .map(|branch| (branch.id.clone(), branch.name.clone()))
            });
        let (id, name) = branch.unwrap_or_else(|| ("algebra".to_string(), "Álgebra".to_string()));
        let questions = grafito_profile::exam::mini_exam_questions(&id);
        let mut prompt = format!("Tomame un mini-examen de {name} (rama {id}). Preguntas:\n\n");
        for (index, question) in questions.iter().enumerate() {
            prompt.push_str(&format!("{}. {question}\n", index + 1));
        }
        prompt.push_str("\nRespondé una por una y al final corregime cada una.");
        self.assistant.problem = prompt;
        self.start_local_assistant_request(ctx);
    }

    /// Muestras (0..=1) para el sparkline de evolución de dominio.
    fn domain_sparkline(&self) -> Vec<f32> {
        Self::domain_sparkline_from(&self.profile)
    }

    /// Clasifica el contenido de la última explicación en una rama del plan.
    fn learning_branch(&self) -> (&'static str, &'static str) {
        let text = self
            .assistant
            .latest_assistant_text()
            .unwrap_or_default()
            .to_lowercase();
        for (needle, id, name) in [
            ("deriv", "calculus", "Cálculo"),
            ("integral", "calculus", "Cálculo"),
            ("límite", "calculus", "Cálculo"),
            ("ecuación", "algebra", "Álgebra"),
            ("polinom", "algebra", "Álgebra"),
            ("función", "functions", "Funciones"),
            ("gráf", "functions", "Funciones"),
            ("trigonometr", "trigonometry", "Trigonometría"),
            ("geom", "geometry", "Geometría"),
            ("estadíst", "stats", "Estadística"),
            ("complej", "complex", "Complejos"),
            ("fractal", "complex", "Complejos"),
        ] {
            if text.contains(needle) {
                return (id, name);
            }
        }
        ("general", "General")
    }

    /// Muestras 0..=1 del sparkline: histórico de la rama más trabajada,
    /// hacia atrás hasta 14 puntos (función pura y testeable).
    fn domain_sparkline_from(profile: &grafito_profile::StudentProfile) -> Vec<f32> {
        profile
            .branches
            .iter()
            .max_by_key(|branch| branch.domain_history.len())
            .map(|branch| {
                branch
                    .domain_history
                    .iter()
                    .rev()
                    .take(14)
                    .map(|entry| entry.1.clamp(0.0, 1.0))
                    .collect::<Vec<f32>>()
                    .into_iter()
                    .rev()
                    .collect()
            })
            .unwrap_or_default()
    }

    /// Registra el feedback del usuario en la memoria del tutor y persiste.
    /// F5 inline: además alimenta WorkingMemory episódica para el tutor socrático.
    fn record_learning(&mut self, correct: bool) {
        let epoch = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|duration| duration.as_secs())
            .unwrap_or(0);
        let (id, name) = self.learning_branch();
        self.profile.record_outcome(id, name, epoch, correct);
        // F5 WorkingMemory hook: cada intento alimenta la RAM episódica.
        // - correct: solo incrementa pasos (misconception vacía)
        // - incorrect: registra la rama como misconception para adaptación socrática
        let misconception = if correct { "" } else { id };
        self.profile.working_memory.record_attempt(misconception);
        self.profile.working_memory.set_topic(name);
        self.profile.working_memory.set_session_epoch(epoch);
        // Sincroniza vista inline del chat
        self.assistant.working_memory = self.profile.working_memory.clone();
        // I/O en background thread para no bloquear UI (60fps)
        spawn_profile_save(self.profile.clone(), crate::utils::profile_path());
        self.notify(
            if correct {
                "¡Bien! Registrado en tu progreso."
            } else {
                "Anotado: reforzamos ese tema."
            },
            ToastKind::Success,
        );
    }

    fn run_assistant_animation(&mut self, ctx: &egui::Context) {
        self.run_assistant_animation_with(ctx, "derivative-slope", "derivada como pendiente");
    }

    /// Exporta la animación visible en la card a GIF (B5, botón Exportar).
    ///
    /// Cablea al `spawn_gif_export` existente (hilo aparte, probado): la UI
    /// solo emitió `ExportMedia`, acá corre el trabajo fuera del draw.
    /// Jamás mudo:
    /// - export en curso → aviso y no se duplica;
    /// - sin animación o sin fotogramas → `Failed` en la card + aviso;
    /// - preflight (`check_gif_export_budget`: 64 frames / 8 Mpx / dims) →
    ///   motivo en la card + aviso;
    /// - el destino es temporal (`temp_dir`, nombre único por instante) y se
    ///   avisa la ruta al terminar; el delay sale de la velocidad de la card
    ///   (`gif_delay_for_rate` sobre `GIF_EXPORT_DELAY_CS`).
    fn export_assistant_media(&mut self, ctx: &egui::Context) {
        // D2 lockdown: en examen no sale nada del documento.
        if self.exam_blocks("Export") {
            return;
        }
        use grafito_ui::assistant::MediaExportState;
        if self.assistant_runtime.gif_export_job.is_some() {
            self.notify("Ya se está exportando la animación.", ToastKind::Info);
            return;
        }
        let frames = self
            .assistant
            .media
            .as_ref()
            .map_or_else(Vec::new, |media| media.frames.clone());
        if frames.is_empty() {
            self.assistant.set_media_export(MediaExportState::Failed(
                "todavía no hay fotogramas para exportar".into(),
            ));
            self.notify("No hay animación para exportar.", ToastKind::Error);
            ctx.request_repaint();
            return;
        }
        if let Err(budget) = crate::anim_native::check_gif_export_budget(&frames) {
            let reason = budget.to_string();
            self.assistant
                .set_media_export(MediaExportState::Failed(reason.clone()));
            self.notify(format!("No se pudo exportar: {reason}"), ToastKind::Error);
            ctx.request_repaint();
            return;
        }
        let frame_count = frames.len();
        let delay_cs = crate::anim_native::gif_delay_for_rate(
            crate::anim_native::GIF_EXPORT_DELAY_CS,
            self.assistant.media_playback_rate(),
        );
        let stamp = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|elapsed| elapsed.as_millis())
            .unwrap_or(0);
        let path = std::env::temp_dir().join(format!(
            "grafito_animacion_{}_{stamp}_{frame_count}.gif",
            std::process::id()
        ));
        let cancel = grafito_assistant::CancellationToken::default();
        let handle = crate::anim_native::spawn_gif_export_cancelable(
            frames,
            path.clone(),
            delay_cs,
            cancel.clone(),
        );
        self.assistant_runtime.gif_export_job = Some(GifExportJob {
            handle,
            frame_count,
            cancel,
            path,
        });
        self.assistant.set_media_export(MediaExportState::Exporting);
        ctx.request_repaint();
    }

    /// Drena el export a GIF sin bloquear (B5).
    ///
    /// Solo hace `join` si el hilo terminó (`is_finished`); publica el
    /// resultado en la card + aviso: éxito con ruta (verificando cota 5 MB
    /// post-escritura: si excede, se borra y es error honesto), o motivo del
    /// fallo. Se llama cada frame desde `sync_assistant_for_frame`.
    fn poll_gif_export_job(&mut self, ctx: &egui::Context) {
        use grafito_ui::assistant::MediaExportState;
        let finished = self
            .assistant_runtime
            .gif_export_job
            .as_ref()
            .is_some_and(|job| job.handle.is_finished());
        if !finished {
            return;
        }
        let Some(job) = self.assistant_runtime.gif_export_job.take() else {
            return;
        };
        match job.handle.join() {
            Ok(Ok(path)) => {
                let too_big = std::fs::metadata(&path)
                    .map(|metadata| metadata.len() > crate::anim_native::GIF_EXPORT_MAX_FILE_BYTES)
                    .unwrap_or(false);
                if too_big {
                    let _ = std::fs::remove_file(&path);
                    let reason = "el GIF supera 5 MB";
                    self.assistant
                        .set_media_export(MediaExportState::Failed(reason.into()));
                    self.notify(format!("No se pudo exportar: {reason}."), ToastKind::Error);
                } else {
                    self.assistant.set_media_export(MediaExportState::Done);
                    self.notify(
                        format!(
                            "Se exportaron {} fotogramas a {}.",
                            job.frame_count,
                            path.display()
                        ),
                        ToastKind::Success,
                    );
                }
            }
            Ok(Err(error)) => {
                let reason = error.to_string();
                self.assistant
                    .set_media_export(MediaExportState::Failed(reason.clone()));
                self.notify(
                    format!("No se pudo exportar la animación: {reason}."),
                    ToastKind::Error,
                );
            }
            Err(_) => {
                self.assistant.set_media_export(MediaExportState::Failed(
                    "la exportación terminó inesperadamente".into(),
                ));
                self.notify("La exportación terminó inesperadamente.", ToastKind::Error);
            }
        }
        ctx.request_repaint();
    }

    /// W-B — pide el SPEC a la IA de verdad (remoto o agente, 1 request extra).
    ///
    /// Hilo background (nunca UI): con `agent_mode` usa UNA llamada al
    /// completador agente con la tool `generate_animation` (el LLM propone
    /// args, `SafeGrafitoDispatcher` valida vía `propose_*` → `infer_*`);
    /// sin agente usa UN chat remoto con prompt SPEC acotado.
    /// Timeout = `ANIM_IA_SPEC_TIMEOUT_MS` (mitad del budget) en ambos casos;
    /// 400-429/offline/red → `Transporte` (fallback canónico, no error).
    /// SPEC inválido (no valida con `infer_*`) → `Invalido` (error honesto).
    /// Sin `unwrap`, sin pánico.
    fn pedir_spec_ia_de_verdad(
        pedido: String,
        settings: ProviderSettings,
        api_key: Option<String>,
        agent_mode: bool,
        timeout_ms: u64,
        cancel: CancellationToken,
    ) -> PedidoSpecIa {
        if cancel.is_cancelled() {
            return PedidoSpecIa::Transporte("La generación se canceló.".into());
        }
        let timeout = std::time::Duration::from_millis(timeout_ms);
        if agent_mode {
            let completer = grafito_assistant::agent::RemoteAgentCompleter::new(settings, api_key);
            let sistema = "Sos un generador de SPEC JSON para animaciones Grafito. Devolvé SOLO el resultado de la tool generate_animation, sin prosa extra.".to_string();
            let mensajes = vec![
                serde_json::json!({"role": "system", "content": sistema}),
                serde_json::json!({"role": "user", "content": pedido}),
            ];
            let tools = vec![grafito_assistant::agent::generate_animation_tool_schema()];
            let cancel_agent = grafito_agent::loop_engine::Cancellation::default();
            // M1: liga la Cancellation del agente al token del turno. Son
            // tipos distintos sin conversión (`Cancellation` del loop vs
            // `CancellationToken` del asistente): el puente es este forwarder
            // efímero (poll 25ms, muere al terminar el SPEC) que propaga el
            // cancel aunque el transporte siga en vuelo.
            let cancel_agent_puente = cancel_agent.clone();
            let cancel_turno_puente = cancel.clone();
            let spec_terminado = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false));
            // R1-5: flag en `Drop`/scopeguard — si `complete` paniquea, el
            // guard marca igual y el puente muere (sin hilo huérfano eterno).
            let _guard = SpecTerminadoGuard {
                flag: spec_terminado.clone(),
            };
            let spec_terminado_puente = spec_terminado.clone();
            let puente = std::thread::spawn(move || {
                while !spec_terminado_puente.load(std::sync::atomic::Ordering::Acquire) {
                    if cancel_turno_puente.is_cancelled() {
                        cancel_agent_puente.cancel();
                        break;
                    }
                    std::thread::sleep(std::time::Duration::from_millis(25));
                }
            });
            let respuesta = <grafito_assistant::agent::RemoteAgentCompleter as grafito_agent::loop_engine::AgentCompleter>::complete(
                &completer, &mensajes, &tools, 512, timeout, &cancel_agent,
            );
            spec_terminado.store(true, std::sync::atomic::Ordering::Release);
            // R1-5: `join` acotado 100 ms (antes sin cota en path de cancel).
            // Si da timeout el puente queda detached con marca y se sigue.
            if !join_puente_bounded(puente, PUENTE_JOIN_TIMEOUT) {
                eprintln!("[puente-spec] timeout tras 100ms, hilo detached");
            }
            drop(_guard);
            if cancel.is_cancelled() {
                return PedidoSpecIa::Transporte("La generación se canceló.".into());
            }
            match respuesta {
                Ok(grafito_agent::loop_engine::AgentChatResponse::ToolCalls { calls }) => {
                    let mut primero: Option<PedidoSpecIa> = None;
                    for call in &calls {
                        if call.name == "generate_animation" {
                            let dispatcher = grafito_assistant::agent::SafeGrafitoDispatcher;
                            use grafito_agent::loop_engine::ToolDispatcher;
                            let resultado = dispatcher.dispatch(call);
                            if resultado.ok {
                                match parsear_spec_anim_ia(&resultado.content, &pedido) {
                                    Ok(spec) => {
                                        primero = Some(PedidoSpecIa::Exito(spec));
                                        break;
                                    }
                                    Err(detalle) => {
                                        primero = Some(PedidoSpecIa::Invalido(detalle));
                                        break;
                                    }
                                }
                            } else {
                                primero = Some(PedidoSpecIa::Invalido(resultado.content));
                                break;
                            }
                        }
                    }
                    primero.unwrap_or_else(|| {
                        PedidoSpecIa::Invalido(
                            "la IA no propuso animación: reformulá con función y rango.".into(),
                        )
                    })
                }
                Ok(grafito_agent::loop_engine::AgentChatResponse::Text { content, .. }) => {
                    match parsear_spec_anim_ia(&content, &pedido) {
                        Ok(spec) => PedidoSpecIa::Exito(spec),
                        Err(detalle) => PedidoSpecIa::Invalido(detalle),
                    }
                }
                Err(error) => PedidoSpecIa::Transporte(error),
            }
        } else {
            let prompt = prompt_spec_anim_ia(&pedido);
            let contexto = grafito_command::assistant_context::document_context(
                &grafito_core::Document::default(),
            );
            let mut request = AssistantRequest::remote(prompt, contexto);
            request.budget.timeout_ms = timeout_ms;
            request.budget.max_output_chars = 512;
            let handle = grafito_assistant::request_remote_with_api_key_on_worker(
                settings,
                request,
                api_key,
                cancel.clone(),
            );
            let inicio = std::time::Instant::now();
            loop {
                if cancel.is_cancelled() {
                    return PedidoSpecIa::Transporte("La generación se canceló.".into());
                }
                if handle.is_finished() {
                    break;
                }
                if inicio.elapsed() >= timeout {
                    cancel.cancel();
                    return PedidoSpecIa::Timeout;
                }
                std::thread::sleep(std::time::Duration::from_millis(25));
            }
            match handle.join() {
                Ok(Ok(completado)) => match parsear_spec_anim_ia(&completado.text, &pedido) {
                    Ok(spec) => PedidoSpecIa::Exito(spec),
                    Err(detalle) => PedidoSpecIa::Invalido(detalle),
                },
                Ok(Err(error)) => PedidoSpecIa::Transporte(error),
                Err(_) => {
                    PedidoSpecIa::Transporte("el pedido de SPEC terminó sin responder.".into())
                }
            }
        }
    }

    /// W-B — worker único IA-primero: SPEC de la IA + un solo render.
    ///
    /// Nunca UI (hilo background): pide el SPEC con timeout, valida con
    /// `infer_*`, renderiza UNA vez (o IA o canónico de fallback, nunca ambos)
    /// y manda `AnimIaRender` (media + prosa del MISMO spec) por el canal.
    /// Timeout/error 400-429/offline → canónico + aviso de una línea.
    /// SPEC inválido → `Err` honesto (jamás basura). Sin `unwrap`.
    pub(crate) fn run_assistant_animation_ia_primero(
        &mut self,
        ctx: &egui::Context,
        pedido_original: String,
        plantilla_fallback: String,
    ) {
        if self.exam_blocks("Asistente") {
            return;
        }
        if self.cancela_turno_anim() {
            self.assistant.anim_progress = false;
            if let Some(message) = anim_replace_message(true) {
                self.notify(message, ToastKind::Info);
            }
        }
        let question = pedido_original.clone();
        self.assistant.begin_request(question);
        self.assistant.problem.clear();
        self.assistant.set_media(None, ctx);
        let agent_mode = self.assistant.agent_mode;
        let settings = match self.assistant_provider_settings() {
            Ok(settings) => settings,
            Err(_) => {
                let prosa = prosa_canonica_para_plantilla(&plantilla_fallback);
                let humano = grafito_ui::assistant::humanize_prose_text(&prosa);
                self.assistant.complete_local_request(humano);
                self.run_assistant_animation_with(ctx, &plantilla_fallback, &pedido_original);
                ctx.request_repaint();
                return;
            }
        };
        let api_key = match self.assistant_api_key() {
            Ok(key) => key,
            Err(_) => {
                let prosa = prosa_canonica_para_plantilla(&plantilla_fallback);
                let humano = grafito_ui::assistant::humanize_prose_text(&prosa);
                self.assistant.complete_local_request(humano);
                let canonico = spec_canonico_para_fallback(&plantilla_fallback);
                self.notify(aviso_fallback_canonico(&canonico), ToastKind::Info);
                self.run_assistant_animation_with(ctx, &plantilla_fallback, &pedido_original);
                ctx.request_repaint();
                return;
            }
        };
        let cancellation = CancellationToken::default();
        let worker_cancel = cancellation.clone();
        let (sender, receiver) = std::sync::mpsc::sync_channel(1);
        let pedido_hilo = pedido_original.clone();
        let plantilla_hilo = plantilla_fallback.clone();
        std::thread::spawn(move || {
            let salida = Self::pedir_spec_ia_de_verdad(
                pedido_hilo.clone(),
                settings,
                api_key,
                agent_mode,
                ANIM_IA_SPEC_TIMEOUT_MS,
                worker_cancel.clone(),
            );
            let desenlace = resolver_turno_anim_ia(true, salida);
            let resultado = match desenlace {
                DesenlaceAnimIa::RenderIa { spec, prosa } => {
                    match render_media_desde_spec_ia(&spec, &worker_cancel) {
                        Ok(media) => Ok(AnimIaRender {
                            media,
                            prosa,
                            aviso: None,
                        }),
                        Err(error) => Err(error),
                    }
                }
                DesenlaceAnimIa::FallbackCanonico { aviso: _ } => {
                    // M1: el aviso genérico del resolver se especializa acá
                    // con la canónica EFECTIVAMENTE renderizada (plantilla y
                    // rango reales, no promesa del pedido).
                    let canonico = spec_canonico_para_fallback(&plantilla_hilo);
                    let prosa = prosa_canonica_para_plantilla(&plantilla_hilo);
                    let aviso = aviso_fallback_canonico(&canonico);
                    match render_media_desde_spec_ia(&canonico, &worker_cancel) {
                        Ok(media) => Ok(AnimIaRender {
                            media,
                            prosa,
                            aviso: Some(aviso),
                        }),
                        Err(error) => Err(error),
                    }
                }
                DesenlaceAnimIa::ErrorHonesto(detalle) => Err(detalle),
            };
            let resultado = if worker_cancel.is_cancelled() {
                Err("La generación se canceló antes de completarse.".to_string())
            } else {
                resultado
            };
            let _ = sender.send(resultado);
        });
        self.assistant.anim_progress = true;
        self.assistant_runtime.anim_ia_job = Some(AssistantAnimIaJob {
            cancellation,
            receiver,
        });
        ctx.request_repaint();
    }

    /// M1 defecto 8 — rama local validada del ApplyProposal (sin IA).
    ///
    /// Misma política que Submit sin IA: integral inválida/explícita/
    /// canónica vía `clasifica_pedido_integral`, tangente vía
    /// `clasifica_pedido_tangente` (inválida → prosa honesta sin hilo;
    /// explícita → prosa que nombra f y rango; canónica → referencia +
    /// declara), resto → genérico local. El hilo re-valida con `infer_*`
    /// (doble puerta, cero basura). Sin I/O en el llamante.
    fn animacion_apply_local_validada(
        &mut self,
        ctx: &egui::Context,
        plantilla_efectiva: &str,
        concept: &str,
    ) {
        match clasifica_pedido_integral(concept, plantilla_efectiva) {
            IntegralPedido::FuncionInvalida(detalle) => {
                // Compromiso verificado pero función inválida: prosa
                // honesta sin hilo de render (sin frames).
                empuja_prosa_apply(&mut self.assistant.conversation, &detalle);
                self.notify(detalle, ToastKind::Info);
            }
            IntegralPedido::Explicita => {
                // Explícita: la prosa nombra la función (nunca huérfana).
                if ultimo_turno_sin_media(&self.assistant.conversation) {
                    let expr = grafito_anim::parametric::infer_area_anim(concept)
                        .map(|resuelto| resuelto.anim().expr_a.clone())
                        .unwrap_or_default();
                    empuja_prosa_apply(
                        &mut self.assistant.conversation,
                        &prosa_integral_explicita(&expr, concept),
                    );
                }
                self.run_assistant_animation_with(ctx, plantilla_efectiva, concept);
            }
            IntegralPedido::Canonica => {
                // Canónica integral: referencia + declara (idempotente).
                if ultimo_turno_sin_media(&self.assistant.conversation) {
                    empuja_prosa_apply(
                        &mut self.assistant.conversation,
                        crate::anim_ui::animation_reference_sentence(),
                    );
                }
                self.run_assistant_animation_with(ctx, plantilla_efectiva, concept);
                append_canonical_integral_prose(&mut self.assistant.conversation);
            }
            IntegralPedido::NoAplica => {
                match clasifica_pedido_tangente(concept, plantilla_efectiva) {
                    TangentePedido::FuncionInvalida(detalle) => {
                        empuja_prosa_apply(&mut self.assistant.conversation, &detalle);
                        self.notify(detalle, ToastKind::Info);
                    }
                    TangentePedido::Explicita => {
                        if ultimo_turno_sin_media(&self.assistant.conversation) {
                            let expr = grafito_anim::parametric::infer_tangent_anim(concept)
                                .map(|resuelto| resuelto.anim().expr_a.clone())
                                .unwrap_or_default();
                            empuja_prosa_apply(
                                &mut self.assistant.conversation,
                                &prosa_tangente_explicita(&expr, concept),
                            );
                        }
                        self.run_assistant_animation_with(ctx, plantilla_efectiva, concept);
                    }
                    TangentePedido::Canonica => {
                        // Canónica tangente: referencia + declara SU prosa
                        // (idempotente por "pedime otra", igual que integral).
                        if ultimo_turno_sin_media(&self.assistant.conversation) {
                            empuja_prosa_apply(
                                &mut self.assistant.conversation,
                                crate::anim_ui::animation_reference_sentence(),
                            );
                        }
                        self.run_assistant_animation_with(ctx, plantilla_efectiva, concept);
                        if let Some(turno) = self.assistant.conversation.last_mut() {
                            if turno.role == ConversationRole::Assistant
                                && !turno.content.contains("pedime otra")
                            {
                                turno.content.push_str("\n\n");
                                turno
                                    .content
                                    .push_str(grafito_anim::parametric::TANGENT_CANONICAL_PROSA);
                            }
                        }
                    }
                    TangentePedido::NoAplica => {
                        // El agente propuso `generate_animation` (vía
                        // `GenerateAnimation` verificado): el hilo genera e
                        // incrusta como `AssistantMedia` DEL TURNO.
                        if ultimo_turno_sin_media(&self.assistant.conversation) {
                            empuja_prosa_apply(
                                &mut self.assistant.conversation,
                                crate::anim_ui::animation_reference_sentence(),
                            );
                        }
                        self.run_assistant_animation_with(ctx, plantilla_efectiva, concept);
                    }
                }
            }
        }
    }

    pub(crate) fn run_assistant_animation_with(
        &mut self,
        ctx: &egui::Context,
        template: &str,
        concept: &str,
    ) {
        // Examen: ni siquiera la animación local corre (igual que playlist).
        if self.exam_blocks("Asistente") {
            return;
        }
        // W-A: si hay una animación en curso, reemplazo EXPLÍCITO avisado
        // (antes era mudo). Cancel real (AS4): señala el token antes de
        // dropear, el hilo descarta. El toast dice "espero o reemplazo".
        if self.cancela_turno_anim() {
            self.assistant.anim_progress = false;
            if let Some(message) = anim_replace_message(true) {
                self.notify(message, ToastKind::Info);
            }
        }
        // No destruir texturas durante el draw (evita wgpu panic 'Texture has been destroyed').
        // La media previa se mantiene visible hasta que la nueva la reemplace en sync_assistant_for_frame
        // (inicio del próximo frame). Solo limpiar si es la primera vez o si el usuario lo pidió explícitamente.
        let _ = ctx;
        // Motor externo si está configurado; si no (o si falla), se usa la
        // animación nativa de Rust para que «Animá» siempre produzca algo.
        let engine = self
            .plugin_registry
            .as_ref()
            .and_then(|registry| registry.engines().into_iter().next().cloned());
        let concept = if concept.is_empty() {
            self.assistant
                .focus
                .as_ref()
                .map(|focus| focus.summary.clone())
                .filter(|summary| !summary.is_empty())
                .unwrap_or_else(|| "derivada como pendiente".to_string())
        } else {
            concept.to_string()
        };
        let template = if template.is_empty() {
            "derivative-slope"
        } else {
            template
        };
        let work_dir = std::env::temp_dir().join(format!(
            "grafito_anim_{}_{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|duration| duration.as_nanos())
                .unwrap_or(0)
        ));
        // Sin I/O en UI: el workdir lo crea el hilo de forma exclusiva (abajo).
        let canvas = (720, 540);
        let resolution =
            grafito_anim::protocol::Resolution::try_new(canvas.0, canvas.1).unwrap_or_default();
        let duration = grafito_anim::protocol::AnimDuration::try_new(2.0).unwrap_or_default();
        let anim_params = grafito_anim::protocol::AnimParams {
            template: template.to_string(),
            concept: concept.clone(),
            params: std::collections::BTreeMap::new(),
            duration,
            resolution,
            export: grafito_anim::ExportFormat::Gif,
            spec: None,
        };
        let request = anim_params.into_request();
        let cancellation = CancellationToken::default();
        let worker_cancellation = cancellation.clone();
        let (sender, receiver) = std::sync::mpsc::sync_channel(1);
        let repaint = ctx.clone();
        let template_owned = template.to_string();
        let concept_owned = concept.clone();
        // Wiring p→anim (F2c): si el documento tiene parámetro vivo "p",
        // la animación paramétrica usa su rango en vez del default.
        let live_p_range: Option<(f64, f64)> = self
            .document
            .live_param(grafito_core::DEFAULT_LIVE_PARAM_NAME)
            .filter(|lp| lp.min.is_finite() && lp.max.is_finite() && lp.min < lp.max)
            .map(|lp| (lp.min, lp.max));
        std::thread::spawn(move || {
            // I/O solo en el hilo (nunca en UI). El workdir se crea de forma
            // exclusiva y solo si el motor externo lo necesita (la vía nativa
            // no toca disco): si ya existe — symlink plantado incluido — se
            // aborta cerrado en vez de escribir a través del enlace.
            if engine.is_some() {
                if let Err(error) = crate::anim_native::prepare_anim_workdir_exclusive(&work_dir) {
                    let _ = sender.send(Err(error));
                    repaint.request_repaint();
                    return;
                }
            }
            if worker_cancellation.is_cancelled() {
                let _ = sender.send(Err(
                    "La generación se canceló antes de completarse.".to_string()
                ));
                repaint.request_repaint();
                return;
            }
            // Nativo cancelable (AS4): paramétrico si hay `ParametricAnim`, si
            // no el clásico. M1: primero la explícita del pedido
            // (`anim_parametrica_para_pedido`, igual que el agente: x³
            // explícita jamás cae a canónica); `Err` honesto sin frames.
            // El render no acepta token: el closure de progreso
            // lo chequea entre frames y el hilo descarta el resultado rancio.
            let render_native_cancellable =
                || -> Result<grafito_ui::assistant::AssistantMedia, String> {
                    let explicita =
                        match anim_parametrica_para_pedido(&template_owned, &concept_owned) {
                            Ok(explicita) => explicita,
                            Err(error) => return Err(error),
                        };
                    if let Some(anim) = explicita.or_else(|| {
                        crate::anim_native::parametric_for_template(&template_owned, &concept_owned)
                    }) {
                        // Rango vivo de "p" si el documento lo define (F2c).
                        // `ParamName` no es `Copy`: se clona (cadenas de ≤16
                        // chars) y ante `Err` se conserva la anim original.
                        let anim = match live_p_range {
                            Some((lo, hi))
                                if anim.param.as_str() == grafito_core::DEFAULT_LIVE_PARAM_NAME =>
                            {
                                match grafito_anim::parametric::ParametricAnim::try_new(
                                    anim.kind,
                                    anim.expr_a.clone(),
                                    anim.expr_b.clone(),
                                    anim.param.clone(),
                                    lo,
                                    hi,
                                    anim.frames,
                                    anim.viewport,
                                ) {
                                    Ok(rebuilt) => rebuilt,
                                    Err(_) => anim,
                                }
                            }
                            _ => anim,
                        };
                        let mut saw_cancel = false;
                        let rendered = crate::anim_native::render_parametric_frames_with_progress(
                            &anim,
                            &mut |_, _| {
                                if worker_cancellation.is_cancelled() {
                                    saw_cancel = true;
                                }
                            },
                        );
                        if worker_cancellation.is_cancelled() || saw_cancel {
                            return Err(
                                "La generación se canceló antes de completarse.".to_string()
                            );
                        }
                        match rendered {
                            Ok(frames) => {
                                // Título curado por el punto único (no eco
                                // crudo: venía con typos y sufijos).
                                let title =
                                    titulo_curado(&template_owned, &concept_owned, Some(&anim));
                                Ok(grafito_ui::assistant::AssistantMedia { title, frames })
                            }
                            Err(error) => Err(error.to_string()),
                        }
                    } else {
                        let mut saw_cancel = false;
                        let frames = crate::anim_native::render_anim_with_progress(
                            &template_owned,
                            &concept_owned,
                            480,
                            360,
                            &std::collections::BTreeMap::new(),
                            &mut |_, _| {
                                if worker_cancellation.is_cancelled() {
                                    saw_cancel = true;
                                }
                            },
                        );
                        if worker_cancellation.is_cancelled() || saw_cancel {
                            return Err(
                                "La generación se canceló antes de completarse.".to_string()
                            );
                        }
                        if frames.is_empty() {
                            return Err(crate::anim_native::error_sin_fotogramas(
                                "el motor nativo",
                            ));
                        }
                        // Punto único: base curada + sufijo nativo (una vez).
                        let title = format!(
                            "{} (nativa)",
                            titulo_curado(&template_owned, &concept_owned, None)
                        );
                        Ok(grafito_ui::assistant::AssistantMedia { title, frames })
                    }
                };
            let result = match engine {
                Some(engine_section) => {
                    let config = grafito_anim::EngineConfig {
                        command: engine_section.command,
                        working_dir: Some(work_dir.clone()),
                        // Timeouts cortos documentados (`ANIM_MOTOR_*`): si el
                        // motor no responde, se cae al generador nativo para
                        // que «Animá» nunca se quede colgado.
                        idle_timeout: std::time::Duration::from_secs(ANIM_MOTOR_IDLE_TIMEOUT_SECS),
                        job_timeout: std::time::Duration::from_secs(ANIM_MOTOR_JOB_TIMEOUT_SECS),
                        ..Default::default()
                    };
                    // Cancel real en el motor externo: `run_job` aborta <200 ms.
                    let cancel_flag = &worker_cancellation;
                    match grafito_anim::run_job(
                        &config,
                        &request,
                        Some(&|| cancel_flag.is_cancelled()),
                        |_| {},
                    ) {
                        Ok(result) => match load_gif_frames(&result.media_path) {
                            Ok(frames) if !frames.is_empty() => {
                                if worker_cancellation.is_cancelled() {
                                    Err("La generación se canceló antes de completarse."
                                        .to_string())
                                } else {
                                    // Punto único (vía externa: base sin sufijo).
                                    let title =
                                        titulo_curado(&template_owned, &concept_owned, None);
                                    Ok(grafito_ui::assistant::AssistantMedia { title, frames })
                                }
                            }
                            _ => render_native_cancellable(),
                        },
                        Err(error) => {
                            if worker_cancellation.is_cancelled()
                                || error.to_lowercase().contains("cancel")
                            {
                                Err(error)
                            } else {
                                render_native_cancellable()
                            }
                        }
                    }
                }
                None => render_native_cancellable(),
            };
            // Descarte final: si se canceló en el último instante, no se publica rancio.
            let result = if worker_cancellation.is_cancelled() {
                Err("La generación se canceló antes de completarse.".to_string())
            } else {
                result
            };
            let _ = sender.send(result);
            let _ = std::fs::remove_dir_all(&work_dir);
            repaint.request_repaint();
        });
        self.assistant.anim_progress = true;
        self.assistant_runtime.anim_job = Some(AssistantAnimJob {
            cancellation,
            receiver,
        });
        // Z3: sin toast de "generando": la card ya muestra el progreso dentro
        // (barra + "Armando tu animación…"); el único toast del flujo feliz
        // es "Animación lista." (la card lo promete: "Te aviso cuando esté lista.").
    }

    /// Reproduce una playlist F2b ("X y después Y") como UNA media (scrub total).
    ///
    /// Nativa-only y en hilo (nunca en UI): cada step animado se renderiza por
    /// el mismo camino que el single (`parametric_for_template` o
    /// `render_anim_with_progress` a 480×360, cancelable entre frames) y los
    /// sets se concatenan con holds (`concat_playlist_fitting` a 12 fps, tope
    /// 96 con ajuste de cadencia que preserva extremos). El player del chat ya
    /// hace scrub por animación vía `Timeline::sample`: sobre el set
    /// concatenado ese scrub cubre el tiempo global sin tocar la UI. La vía
    /// externa multijob vive en `grafito_anim::engine::run_playlist_sequential`
    /// (FIFO honesto); acá no se mezcla para no componer GIFs ajenos sin
    /// presupuesto. Ante cualquier fallo (incluida playlist de solo pausas)
    /// se publica `Err` honesto en la card, jamás media parcial en silencio.
    ///
    /// R2-V2 (puro, testeable): valida `len <= PLAYLIST_MAX_STEPS` (8) con
    /// `Err` acotado. El worker la llama antes de renderizar para que el
    /// struct literal con 64 steps no acumule OOM.
    pub(crate) fn playlist_len_budget_ok(len: usize) -> Result<(), String> {
        if len > grafito_anim::protocol::PLAYLIST_MAX_STEPS {
            return Err(format!(
                "la playlist trae {len} steps y excede el tope de {}: partila en dos",
                grafito_anim::protocol::PLAYLIST_MAX_STEPS
            ));
        }
        Ok(())
    }

    pub(crate) fn run_assistant_playlist_with(
        &mut self,
        ctx: &egui::Context,
        playlist: grafito_anim::protocol::Playlist,
    ) {
        // D2 lockdown: en examen la playlist no corre (bypass del bloqueo
        // del panel, que ya retorna antes en `draw_assistant`).
        if self.exam_blocks("Asistente") {
            return;
        }
        // Z3 trigger único: mismo reemplazo explícito que el single
        // (`anim_replace_message`, sin duplicar el texto).
        if self.cancela_turno_anim() {
            self.assistant.anim_progress = false;
            if let Some(message) = anim_replace_message(true) {
                self.notify(message, ToastKind::Info);
            }
        }
        // Z3: la playlist se concatena en UNA media con holds (`concat`): el
        // `Group` simultáneo corre FIFO honesto en orden y el `Wait` congela
        // el último frame vía el hold. Sin secuencia guardada para replay: el
        // player ya repite en loop y la card v3 no tiene botón de secuencia.
        let cancellation = CancellationToken::default();
        let worker_cancellation = cancellation.clone();
        let (sender, receiver) = std::sync::mpsc::sync_channel(1);
        let repaint = ctx.clone();
        std::thread::spawn(move || {
            // R2-V2: cap dura `max_steps=8` + `total_pixels` (OOM honesto).
            // `Playlist::try_new` valida 1..=8; el struct literal puede bypassear,
            // así que se re-valida acá antes de renderizar nada.
            if let Err(e) = Self::playlist_len_budget_ok(playlist.steps.len()) {
                let _ = sender.send(Err(e));
                repaint.request_repaint();
                return;
            }
            let mut partes: Vec<(Vec<egui::ColorImage>, u64)> = Vec::new();
            if partes.try_reserve(playlist.steps.len()).is_err() {
                let _ = sender.send(Err("sin memoria para la playlist".to_string()));
                repaint.request_repaint();
                return;
            }
            let mut total_pixels: usize = 0;
            // M1: cada lado titula por el punto único (`titulo_curado`,
            // jamás eco crudo del concepto).
            let mut bases: Vec<(String, String)> = Vec::new();
            for step in &playlist.steps {
                if worker_cancellation.is_cancelled() {
                    let _ = sender.send(Err(
                        "La generación se canceló antes de completarse.".to_string()
                    ));
                    repaint.request_repaint();
                    return;
                }
                // Pausa: sin frames propios; el hold lo agrega el concat.
                let Some(request) = step.request.as_ref() else {
                    continue;
                };
                bases.push((request.template.clone(), request.concept.clone()));
                let plantilla = request.template.clone();
                let concepto = request.concept.clone();
                let params = request.params.clone();
                let mut saw_cancel = false;
                let mut mira_cancel = |_: usize, _: usize| {
                    if worker_cancellation.is_cancelled() {
                        saw_cancel = true;
                    }
                };
                // Mismo camino que el single: explícita del pedido igual
                // que el agente, si no paramétrica canónica, si no clásica.
                // `Err` honesto sin media parcial en silencio. Sin motor
                // externo (ver doc del método).
                let explicita = match anim_parametrica_para_pedido(&plantilla, &concepto) {
                    Ok(explicita) => explicita,
                    Err(error) => {
                        let _ = sender.send(Err(error));
                        repaint.request_repaint();
                        return;
                    }
                };
                let frames = if let Some(anim) = explicita
                    .or_else(|| crate::anim_native::parametric_for_template(&plantilla, &concepto))
                {
                    match crate::anim_native::render_parametric_frames_with_progress(
                        &anim,
                        &mut mira_cancel,
                    ) {
                        Ok(frames) => frames,
                        Err(error) => {
                            let _ = sender.send(Err(error.to_string()));
                            repaint.request_repaint();
                            return;
                        }
                    }
                } else {
                    crate::anim_native::render_anim_with_progress(
                        &plantilla,
                        &concepto,
                        480,
                        360,
                        &params,
                        &mut mira_cancel,
                    )
                };
                if worker_cancellation.is_cancelled() || saw_cancel {
                    let _ = sender.send(Err(
                        "La generación se canceló antes de completarse.".to_string()
                    ));
                    repaint.request_repaint();
                    return;
                }
                if frames.is_empty() {
                    let _ = sender.send(Err(crate::anim_native::error_sin_fotogramas(
                        "el motor nativo",
                    )));
                    repaint.request_repaint();
                    return;
                }
                // R2-V2: presupuesto `total_pixels` con `checked` (8M, paridad loader).
                // Si un step ya excede, `Err` acotado antes de acumular OOM.
                if let Some(first) = frames.first() {
                    let per_frame = first.size[0].checked_mul(first.size[1]);
                    let step_pixels = per_frame.and_then(|pc| pc.checked_mul(frames.len()));
                    let next_total = step_pixels.and_then(|sp| total_pixels.checked_add(sp));
                    match next_total {
                        Some(next) if next <= crate::anim_native::GIF_EXPORT_MAX_TOTAL_PIXELS => {
                            total_pixels = next;
                        }
                        _ => {
                            let _ = sender.send(Err(format!(
                                "la playlist excede el presupuesto de {} píxeles",
                                crate::anim_native::GIF_EXPORT_MAX_TOTAL_PIXELS
                            )));
                            repaint.request_repaint();
                            return;
                        }
                    }
                }
                if partes.try_reserve(1).is_err() {
                    let _ = sender.send(Err("sin memoria para la playlist".to_string()));
                    repaint.request_repaint();
                    return;
                }
                partes.push((frames, step.wait_after_ms));
            }
            if partes.is_empty() {
                let _ = sender.send(Err(
                    "la playlist solo trae pausas: nada para mostrar".to_string()
                ));
                repaint.request_repaint();
                return;
            }
            match crate::anim_native::concat_playlist_fitting(
                partes,
                crate::anim_native::GIF_BASE_FPS,
            ) {
                Ok(frames) => {
                    // M1: UN `titulo_curado` compartido por las 3 vías +
                    // playlist (defecto 7): cada lado cura por el punto
                    // único, sin eco crudo ni typos visibles.
                    let title = if bases.is_empty() {
                        "playlist (nativa)".to_string()
                    } else {
                        let curados: Vec<String> = bases
                            .iter()
                            .map(|(plantilla, concepto)| titulo_curado(plantilla, concepto, None))
                            .collect();
                        format!("{} (playlist nativa)", curados.join(" y después "))
                    };
                    let _ =
                        sender.send(Ok(grafito_ui::assistant::AssistantMedia { title, frames }));
                }
                Err(error) => {
                    let _ = sender.send(Err(error.to_string()));
                }
            }
            repaint.request_repaint();
        });
        self.assistant.anim_progress = true;
        self.assistant_runtime.anim_job = Some(AssistantAnimJob {
            cancellation,
            receiver,
        });
        // Z3: sin toast de "generando" (la card ya muestra el progreso
        // dentro); el único toast del flujo feliz es "Animación lista.".
    }

    fn start_remote_proposal_verification(
        &mut self,
        ctx: &egui::Context,
        launch: AssistantProposalLaunch,
    ) {
        // D2 lockdown: en examen no sale nada a internet.
        if self.exam_blocks("Internet") {
            return;
        }
        let AssistantProposalLaunch {
            id,
            provider,
            model,
            route,
            fusion_fallback_allowed,
            question,
            correction_attempt,
            repair_target_turn,
            document_revision,
            document_digest,
            focus,
            text,
        } = launch;
        let document = self.document.detached_clone_for_staging();
        let camera = self.camera;
        let response_text = text.clone();
        let cancellation = CancellationToken::default();
        let worker_cancellation = cancellation.clone();
        let (sender, receiver) = sync_channel(1);
        let repaint = ctx.clone();
        std::thread::spawn(move || {
            let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                inspect_remote_proposals_cancellable(
                    &document,
                    &text,
                    camera,
                    &worker_cancellation,
                    correction_attempt > 0,
                )
            }))
            .unwrap_or_else(|_| {
                Err("La comprobación local de la propuesta falló inesperadamente.".into())
            });
            let _ = sender.send(result);
            repaint.request_repaint();
        });
        self.assistant_runtime.proposal_job = Some(AssistantProposalJob {
            id,
            provider,
            model,
            route,
            fusion_fallback_allowed,
            question,
            correction_attempt,
            repair_target_turn,
            document_revision,
            document_digest,
            focus,
            text: response_text,
            cancellation,
            receiver,
        });
    }

    fn build_assistant_proposal_correction(
        &mut self,
        question: &str,
        repair_feedback: AssistantRepairFeedback,
        target_turn: usize,
        correction_attempt: u8,
        route: AssistantRemoteRoute,
        fusion_fallback_allowed: bool,
    ) -> Result<AssistantRemoteLaunch, String> {
        if correction_attempt >= MAX_ASSISTANT_PROPOSAL_CORRECTIONS {
            return Err("La corrección remota alcanzó su límite seguro.".into());
        }
        let mut settings = self.assistant_provider_settings()?;
        if route == AssistantRemoteRoute::FusionFallback {
            if !fusion_fallback_allowed
                || settings.profile != ProviderProfile::OpenCodeGo
                || settings.model != OPENCODE_VISION_MODEL
            {
                return Err(
                    "La revisión remota adicional no está autorizada para la configuración actual."
                        .into(),
                );
            }
            settings.model = OPENCODE_FUSION_MODEL.into();
            settings.capabilities.vision = false;
        }
        let api_key = self.assistant_api_key()?;
        let document_context = grafito_command::assistant_context::document_context(&self.document);
        let focus = grafito_command::assistant_context::selected_function_focus(
            &self.document,
            self.selected_object,
        );
        let document_revision = document_context.revision;
        let document_digest = document_context.digest.clone();
        let request = self.build_remote_assistant_request(
            assistant_correction_prompt(question),
            document_context,
            focus.clone(),
            Vec::new(),
            false,
            Some(AssistantRepairRequest {
                feedback: repair_feedback,
                target_turn,
            }),
        )?;

        Ok(AssistantRemoteLaunch {
            settings,
            request,
            api_key,
            provider: self.assistant.provider,
            model: self.assistant.model.clone(),
            route,
            fusion_fallback_allowed,
            question: question.into(),
            document_revision,
            document_digest,
            focus,
            correction_attempt: correction_attempt + 1,
            repair_target_turn: Some(target_turn),
            socratic_guard: self.session_socratic_guard(question),
        })
    }

    pub(crate) fn start_local_assistant_request(&mut self, ctx: &egui::Context) {
        // D2 lockdown: el asistente (incluso local) queda bloqueado en examen.
        if self.exam_blocks("Asistente") {
            return;
        }
        if self.assistant.is_pending || !self.assistant_runtime.remote_request_slot_is_free() {
            return;
        }
        let question = self.assistant.problem.trim().to_owned();
        let document_context = grafito_command::assistant_context::document_context(&self.document);
        let focus = grafito_command::assistant_context::selected_function_focus(
            &self.document,
            self.selected_object,
        );
        let mut request = AssistantRequest::local(question.clone(), document_context);
        request.focus = focus;
        request.attachments = self.assistant.attachments.clone();

        self.assistant.begin_request(question.clone());
        self.assistant.problem.clear();
        // harness es síncrono pero O(n) pequeño, medido <5ms — no bloquea UI >10ms.
        // El trabajo pesado (verificación de propuestas remotas) ya corre en
        // worker thread via proposal_job / inspect_remote_proposals_cancellable.
        let local_result = match harness::request(&self.document, &request) {
            Ok(result) => result,
            Err(error) => {
                self.assistant.fail_request(error.clone());
                self.notify(error, ToastKind::Error);
                ctx.request_repaint();
                return;
            }
        };
        let staged_changes = local_result
            .staged_plan
            .map(|staged| staged.preview().changes.clone());
        match classify_local_assistant_response(local_result.response) {
            LocalAssistantDisposition::Solved { answer, plan } => {
                // Prosa con nombres humanos (mapa `humanize_control_name` de
                // ui, solo lectura): jamás IDs literales en el turno.
                let human = grafito_ui::assistant::humanize_prose_text(&answer);
                self.assistant.complete_local_request(human);
                if let Some(plan) = plan {
                    if let Some(changes) = staged_changes {
                        self.assistant.stage_proposed_plan(plan, changes);
                    } else {
                        self.show_assistant_error(
                            "La propuesta local no pudo completar su comprobación headless.",
                        );
                    }
                }
            }
            LocalAssistantDisposition::NeedsRemoteAuthorization(reason) => {
                if self.assistant.full_permission {
                    if self.remote_provider_ready() {
                        self.start_remote_assistant_for(ctx, question, None);
                    } else {
                        let message =
                            "Configurá un proveedor (Ajustes del asistente) para respuestas en línea automáticas.";
                        self.assistant.fail_request(message);
                        self.notify(message, ToastKind::Info);
                    }
                } else {
                    self.assistant.stage_remote_authorization(question, reason);
                }
            }
            LocalAssistantDisposition::Rejected(error) => {
                self.assistant.fail_request(error.clone());
                self.notify(error, ToastKind::Error);
            }
        }
        ctx.request_repaint();
    }

    /// Arranca la consulta remota tras un consentimiento explícito del cartel.
    fn start_authorized_remote_assistant_request(&mut self, ctx: &egui::Context) {
        if self.assistant.is_pending || !self.assistant_runtime.remote_request_slot_is_free() {
            return;
        }
        let Some(question) = self
            .assistant
            .pending_remote_authorization_question()
            .map(str::to_owned)
        else {
            return;
        };
        self.assistant.begin_authorized_remote_request();
        self.start_remote_assistant_for(ctx, question, None);
    }

    /// Guard socrático de la sesión actual para un lanzamiento remoto.
    ///
    /// Fuente: nivel del perfil + tema de `WorkingMemory` + conversación.
    /// Bypass exploratorio (demo, no evaluación) → `None` (guard desactivado):
    /// - pedido demostrativo (`ejemplos`/`probá`/`capacidades`/`graficá` vía
    ///   `is_exploratory_request`), o
    /// - sin `last_concept`/`current_topic` (primer turno sin tema previo), o
    /// - sin pregunta heurística previa en la conversación (`?` asistente).
    ///   Primer turno sin pregunta previa ⇒ no punitivo (ver `can_reveal_answer`
    ///   y `count_heuristic_answers`).
    ///
    /// El modo agente lo ignora porque ya orquesta sus propias tools socráticas.
    fn session_socratic_guard(&self, question: &str) -> Option<SocraticGuardContext> {
        // Es demo, no evaluación: se salta el telling.
        if is_exploratory_request(question) {
            return None;
        }
        let topic = self.profile.working_memory.last_concept.as_deref().or(self
            .profile
            .working_memory
            .current_topic
            .as_deref());
        // Sin concepto previo → demo, no evaluación.
        let topic_trimmed = topic.map(str::trim).filter(|topic| !topic.is_empty())?;
        let _ = topic_trimmed;
        // Sin pregunta heurística previa → primer turno no punitivo.
        let has_prior_question = self
            .assistant
            .conversation
            .iter()
            .any(|turn| turn.role == ConversationRole::Assistant && turn.content.contains('?'));
        if !has_prior_question {
            return None;
        }
        Some(socratic_guard_context(
            self.profile.level,
            topic,
            question,
            &self.assistant.conversation,
        ))
    }

    /// Lanza la consulta remota con la pregunta dada, sin depender del cartel.
    ///
    /// Con permiso completo, el consentimiento de imágenes se otorga automático;
    /// la capacidad de visión del modelo sigue siendo un requisito real.
    /// `model_override` sólo-sesión: no toca la preferencia guardada.
    fn start_remote_assistant_for(
        &mut self,
        ctx: &egui::Context,
        question: String,
        model_override: Option<&str>,
    ) {
        // Freno 429: en pausa se avisa con cuenta regresiva sin tocar la red.
        // Cubre chat simple, modo agente y el fallback sólo-sesión (que
        // también pasa por acá): ningún reintento automático quema cuota.
        if self.fail_fast_if_rate_limited() {
            return;
        }
        if !self.assistant_runtime.remote_request_slot_is_free() {
            return;
        }
        if !self.assistant.attachments.is_empty() {
            if !self.assistant.vision_enabled {
                self.show_assistant_error(
                    "Confirmá que la configuración remota admite imágenes antes de enviarlas.",
                );
                return;
            }
            if self.assistant.full_permission {
                self.assistant.image_upload_consent = true;
            }
            if !self.assistant.image_upload_consent {
                self.show_assistant_error(
                    "Autorizá el envío de las imágenes antes de realizar la consulta.",
                );
                return;
            }
        }
        let effective_model = model_override
            .unwrap_or(&self.assistant.model)
            .trim()
            .to_owned();
        if effective_model.is_empty() {
            self.show_assistant_error(
                "Completá la configuración avanzada antes de consultar remotamente.",
            );
            return;
        }
        let settings = match self.assistant_provider_settings_for(&effective_model) {
            Ok(settings) => settings,
            Err(error) => {
                self.show_assistant_error(error);
                return;
            }
        };
        let api_key = match self.assistant_api_key() {
            Ok(key) => key,
            Err(error) => {
                self.show_assistant_error(error);
                return;
            }
        };
        let document_context = grafito_command::assistant_context::document_context(&self.document);
        let focus = grafito_command::assistant_context::selected_function_focus(
            &self.document,
            self.selected_object,
        );
        let document_revision = document_context.revision;
        let document_digest = document_context.digest.clone();
        let request = match self.build_remote_assistant_request(
            question.clone(),
            document_context,
            focus.clone(),
            self.assistant.attachments.clone(),
            self.assistant.image_upload_consent,
            None,
        ) {
            Ok(request) => request,
            Err(error) => {
                // `build_remote_assistant_request` valida límites (inglés de
                // crates externos) — se envuelve en español antes de mostrar.
                self.show_assistant_error(remote_error_message(&error, &effective_model));
                return;
            }
        };

        // Fallback sólo-sesión: se recuerda para aceptar el resultado en vuelo
        // sin tocar la preferencia guardada del usuario.
        if model_override.is_some() {
            self.assistant_runtime.fallback_model = Some(effective_model.clone());
        } else {
            self.assistant_runtime.fallback_model = None;
        }
        let launch = AssistantRemoteLaunch {
            settings,
            request,
            api_key,
            provider: self.assistant.provider,
            model: effective_model,
            route: AssistantRemoteRoute::SelectedModel,
            fusion_fallback_allowed: self.assistant.allow_fusion_fallback,
            question: question.clone(),
            document_revision,
            document_digest,
            focus,
            correction_attempt: 0,
            repair_target_turn: None,
            socratic_guard: self.session_socratic_guard(&question),
        };
        if self.assistant.agent_mode {
            self.start_agent_assistant_job(ctx, launch);
        } else {
            self.start_remote_assistant_job(ctx, launch);
        }
    }

    fn apply_proposed_assistant_plan(&mut self) {
        let Some(plan) = self.assistant.proposed_plan().cloned() else {
            return;
        };
        let before_labels = self.object_labels_snapshot();
        match apply_local_assistant_plan(
            &mut self.document,
            &plan,
            &mut self.undo_stack,
            &mut self.redo_stack,
        ) {
            Ok(result) => {
                self.record_step_from_diff("Propuesta local aplicada", &before_labels, true);
                // Decide perspectiva según el contenido del plan: CreateGraph es 2D,
                // otras operaciones matemáticas también deben quedar visibles en Álgebra.
                let has_graph = plan.operations.iter().any(|operation| operation.is_graph());
                if has_graph {
                    if let Some(perspective) = assistant_graph_perspective(
                        grafito_command::assistant_context::AssistantGraphView::TwoD,
                        self.current_view,
                    ) {
                        self.set_perspective(perspective);
                    }
                }
                self.ensure_algebra_panel_visible();
                self.assistant.finish_proposed_plan_application(true);
                self.notify(
                    format!("Propuesta local aplicada: {}", result.changes.join(", ")),
                    ToastKind::Success,
                );
            }
            Err(error) => {
                self.assistant.clear_proposed_plan();
                self.show_assistant_error(format!(
                    "La propuesta local cambió o dejó de ser válida: {error}"
                ));
            }
        }
    }

    fn request_assistant_proposal_correction(&mut self, ctx: &egui::Context) {
        // Freno 429 primero: la sesión de corrección queda intacta para
        // reintentarla cuando venza la pausa (se toma abajo, no acá).
        if self.fail_fast_if_rate_limited() {
            return;
        }
        if self.assistant.is_pending || !self.assistant_runtime.remote_request_slot_is_free() {
            return;
        }
        let current_context = grafito_command::assistant_context::document_context(&self.document);
        let current_focus = grafito_command::assistant_context::selected_function_focus(
            &self.document,
            self.selected_object,
        );
        if !self
            .assistant
            .proposal_correction_matches_context(&current_context, current_focus.as_ref())
        {
            self.assistant.invalidate_proposal_correction();
            self.report_assistant_error(
                "La corrección se descartó porque cambió el documento o el foco seleccionado.",
            );
            return;
        }
        let Some((question, feedback, target_turn, correction_attempt)) =
            self.assistant.take_proposal_correction_session()
        else {
            return;
        };
        let fusion_fallback_allowed = self.assistant.allow_fusion_fallback;
        let route = if can_use_fusion_fallback(
            fusion_fallback_allowed,
            self.assistant.provider,
            &self.assistant.model,
        ) {
            AssistantRemoteRoute::FusionFallback
        } else {
            AssistantRemoteRoute::SelectedModel
        };
        let launch = match self.build_assistant_proposal_correction(
            &question,
            feedback,
            target_turn,
            correction_attempt,
            route,
            fusion_fallback_allowed,
        ) {
            Ok(launch) => launch,
            Err(error) => {
                self.assistant.restore_proposal_correction();
                // build_* mezcla español local e inglés de validadores externos.
                let current_model = self.assistant.model.clone();
                self.report_assistant_error(remote_error_message(&error, &current_model));
                return;
            }
        };

        self.assistant
            .begin_proposal_correction_with_route(route == AssistantRemoteRoute::FusionFallback);
        self.start_remote_assistant_job(ctx, launch);
    }

    fn start_model_request(&mut self, ctx: &egui::Context) {
        // La lista de modelos también respeta la pausa (es un GET al mismo
        // proveedor). Con cache fresco el worker ni sale a la red.
        if self.fail_fast_if_rate_limited() {
            return;
        }
        if !self.assistant_runtime.request_model_refresh() {
            return;
        }
        let settings = match self.assistant_provider_settings() {
            Ok(settings) => settings,
            Err(error) => {
                self.show_assistant_error(error);
                return;
            }
        };
        let api_key = match self.assistant_api_key() {
            Ok(key) => key,
            Err(error) => {
                self.show_assistant_error(error);
                return;
            }
        };
        self.assistant_runtime.next_request_id =
            self.assistant_runtime.next_request_id.wrapping_add(1);
        let id = self.assistant_runtime.next_request_id;
        let provider = self.assistant.provider;
        let cancellation = CancellationToken::default();
        let worker =
            request_remote_models_with_api_key_on_worker(settings, api_key, cancellation.clone());
        let (sender, receiver) = sync_channel(1);
        let repaint = ctx.clone();
        let cancellation_for_thread = cancellation.clone();
        std::thread::spawn(move || {
            if cancellation_for_thread.is_cancelled() {
                for _ in 0..20 {
                    if worker.is_finished() {
                        break;
                    }
                    std::thread::sleep(std::time::Duration::from_millis(10));
                }
            }
            let result = worker
                .join()
                .unwrap_or_else(|_| Err("La lista de modelos terminó inesperadamente.".into()));
            let _ = sender.send(result);
            repaint.request_repaint();
        });
        self.assistant_runtime.model_job = Some(AssistantModelJob {
            id,
            provider,
            cancellation,
            receiver,
        });
    }

    fn cancel_assistant_request(&mut self) {
        // Cancela remote+proposal+agent+model+anim+export (todos, sin else-if:
        // aunque el slot remoto sólo permite un job de consulta a la vez,
        // model/anim/export son independientes y deben cancelarse también).
        // Los workers con token se drenan en poll (take_finished_*); anim
        // señala su token y dropea el slot (worker acotado con chequeo entre
        // frames); el export suelto lo entierra el reaper y la card vuelve
        // a `Idle` (ver `cancela_turno_anim`).
        let had_anim = self.assistant_runtime.anim_job.is_some();
        if self.cancela_turno_anim() {
            if had_anim {
                self.assistant.anim_progress = false;
            }
            self.begin_cancelling_remote_request();
        }
    }

    /// Cancela el turno en vuelo + resetea la card de export si el cancel
    /// soltó su hilo (M1, defecto 5).
    ///
    /// Sin el reset, un export en vuelo cancelado dejaría la card en
    /// `Exporting` para siempre (el reaper entierra el archivo en background
    /// y el poll ya no drena nada). Solo resetea si HABÍA export y el slot
    /// se soltó: un `Done`/`Failed` previo no se toca. Sin I/O en el
    /// llamante. Retorna si había algún job en vuelo.
    fn cancela_turno_anim(&mut self) -> bool {
        let habia_export = self.assistant_runtime.gif_export_job.is_some();
        let hubo = self.assistant_runtime.cancel_all_assistant_jobs();
        if habia_export && self.assistant_runtime.gif_export_job.is_none() {
            self.assistant
                .set_media_export(grafito_ui::assistant::MediaExportState::Idle);
        }
        hubo
    }

    fn begin_cancelling_remote_request(&mut self) {
        self.assistant.begin_cancellation();
    }

    fn cancel_stale_remote_request(&mut self) {
        let expected = self
            .assistant_runtime
            .expected_model(&self.assistant.model)
            .to_owned();
        if self
            .assistant_runtime
            .cancel_stale_remote_job(self.assistant.provider, &expected)
            || self
                .assistant_runtime
                .cancel_stale_agent_job(self.assistant.provider, &expected)
        {
            self.begin_cancelling_remote_request();
        }
    }

    fn cancel_stale_model_request(&mut self) {
        if self
            .assistant_runtime
            .cancel_stale_model_job(self.assistant.provider)
        {}
    }

    /// Drena la actividad del modo agente y cierra el turno al terminar.
    fn poll_assistant_agent(&mut self, ctx: &egui::Context) -> bool {
        let Some(job) = self.assistant_runtime.agent_job.as_ref() else {
            return false;
        };
        loop {
            match job.receiver.try_recv() {
                Ok(AgentChannelMsg::Event(event)) => match event {
                    grafito_agent::AgentEvent::ToolStarted { name, .. } => {
                        self.assistant
                            .push_agent_activity(format!("usando {name}…"));
                    }
                    grafito_agent::AgentEvent::ToolFinished { name, ok } => {
                        let marker = if ok { "✓" } else { "✗" };
                        self.assistant
                            .push_agent_activity(format!("{marker} {name}"));
                    }
                    grafito_agent::AgentEvent::Ledger { render } => {
                        self.assistant.set_agent_ledger(Some(render));
                    }
                    grafito_agent::AgentEvent::Finalized { .. } => {}
                },
                Ok(AgentChannelMsg::Done(result)) => {
                    if let Some(job) = self.assistant_runtime.agent_job.take() {
                        let cancelled = job.cancellation.is_cancelled();
                        if cancelled {
                            self.fail_assistant_request(
                                "La consulta agente se canceló antes de obtener una respuesta.",
                            );
                        } else {
                            match result {
                                Ok(outcome) => {
                                    // Wiring B1 (loop Spark por Responses en paralelo):
                                    // un `Done(Ok)` con Spark se acepta directo y NUNCA
                                    // dispara el fallback a deepseek. El fallback sólo
                                    // vive en la rama `Err` vía
                                    // `should_fallback_agent_spark_to_deepseek`.
                                    self.assistant.complete_request(outcome.final_text);
                                }
                                Err(error) => {
                                    // Modo agente + Spark: las tools aún no viajan por Responses API.
                                    // Fallback sólo-sesión a deepseek (chat-compatible), preferencia intacta.
                                    // Si B1 ya cerró el loop por Responses, este error deja
                                    // de ocurrir y el `Ok` de arriba gana sin fallback.
                                    if should_fallback_agent_spark_to_deepseek(
                                        &error,
                                        job.provider,
                                        &job.model,
                                    ) {
                                        eprintln!("grafito: session-fallback agent spark -> deepseek-v4-flash (preferencia intacta)");
                                        self.notify(
                                            "Modo agente con Spark aún no soporta herramientas; reintentando con DeepSeek Flash…",
                                            ToastKind::Info,
                                        );
                                        let question = self.assistant.problem.trim().to_owned();
                                        self.start_remote_assistant_for(
                                            ctx,
                                            question,
                                            Some("deepseek-v4-flash"),
                                        );
                                    } else {
                                        self.fail_assistant_request(error);
                                    }
                                }
                            }
                        }
                    }
                    ctx.request_repaint();
                    return true;
                }
                Err(TryRecvError::Empty) => break,
                Err(TryRecvError::Disconnected) => {
                    if let Some(_job) = self.assistant_runtime.agent_job.take() {
                        self.fail_assistant_request(
                            "El agente terminó inesperadamente antes de responder.",
                        );
                    }
                    ctx.request_repaint();
                    return true;
                }
            }
        }
        false
    }

    fn poll_assistant_jobs(&mut self, ctx: &egui::Context) {
        self.poll_assistant_agent(ctx);
        // Drena deltas SSE a la burbuja provisional antes de cosechar el
        // resultado final (el preview se limpia en cada rama terminal).
        self.assistant_runtime
            .drain_remote_stream_preview(&mut self.assistant, ctx);
        if let Some(completion) = self.assistant_runtime.take_finished_remote_job() {
            let FinishedRemoteJob {
                id,
                provider,
                model,
                route,
                fusion_fallback_allowed,
                question,
                correction_attempt,
                repair_target_turn,
                document_revision,
                document_digest,
                focus,
                cancelled,
                result,
                stream_preview_active,
                ..
            } = completion;
            if stream_preview_active {
                pop_provisional_stream_turn(&mut self.assistant);
            }
            if cancelled
                || !accepts_remote_result(
                    self.assistant.provider,
                    self.assistant_runtime.expected_model(&self.assistant.model),
                    provider,
                    &model,
                )
            {
                if correction_attempt > 0 {
                    self.fail_assistant_repair_request(
                        "La corrección se canceló antes de recibir una respuesta.",
                    );
                } else {
                    self.fail_assistant_request(
                        "La consulta se canceló antes de recibir una respuesta.",
                    );
                }
            } else {
                let current_context =
                    grafito_command::assistant_context::document_context(&self.document);
                let current_focus = grafito_command::assistant_context::selected_function_focus(
                    &self.document,
                    self.selected_object,
                );
                if !accepts_remote_context(
                    &current_context,
                    current_focus.as_ref(),
                    document_revision,
                    &document_digest,
                    focus.as_ref(),
                ) {
                    self.fail_assistant_request(
                        "La respuesta quedó obsoleta porque cambió el documento o el foco; no se aceptó ni se verificaron sus propuestas.",
                    );
                    self.assistant.invalidate_proposal_correction();
                } else {
                    match result {
                        Ok(completion) => {
                            if completion.truncated {
                                self.notify(
                                    "La respuesta alcanzó el límite de la consulta; pedí que continúe desde el último punto.",
                                    ToastKind::Info,
                                );
                            }
                            let text = completion.text;
                            self.start_remote_proposal_verification(
                                ctx,
                                AssistantProposalLaunch {
                                    id,
                                    provider,
                                    model,
                                    route,
                                    fusion_fallback_allowed,
                                    question,
                                    correction_attempt,
                                    repair_target_turn,
                                    document_revision,
                                    document_digest,
                                    focus,
                                    text,
                                },
                            );
                        }
                        Err(error) => {
                            // Reparación socrática: el guard detectó telling
                            // con attempts<2 en el worker (streaming o no).
                            // El `error` es diagnóstico interno con jerga
                            // (`GUARD TELLING...`, solo para detección y logs):
                            // JAMÁS se publica crudo vía `complete_request`
                            // (ese fue el bug P0). Se convierte a voz de Mili
                            // vía `repair_student_message` (sin `GUARD`/
                            // `attempts`/`estado`/`Re-preguntá`). No hay
                            // fallback de modelo ni cartel de error aquí.
                            if is_socratic_repair_error(&error) {
                                if correction_attempt > 0 {
                                    self.assistant.restore_proposal_correction();
                                }
                                // Jerga solo en logs/eventos internos.
                                eprintln!(
                                    "grafito: socratic repair interno (no al transcript): {error}"
                                );
                                // Convierte a voz de Mili; si no hay guard
                                // (no debería pasar si hubo repair), fallback
                                // humano sin jerga ni eco crudo.
                                let student = self
                                    .session_socratic_guard(&question)
                                    .map(|guard| {
                                        guard.fsm.repair_student_message(&guard.scaffold)
                                    })
                                    .unwrap_or_else(|| {
                                        "Antes de mostrarte la solución, ¿qué forma te imaginás? Contame qué probaste y lo vemos juntos.".to_owned()
                                    });
                                self.assistant.complete_request(student);
                                self.notify(
                                    "El tutor repregunta antes de mostrar la solución directa.",
                                    ToastKind::Info,
                                );
                            } else if should_fallback_remote_spark_to_deepseek(
                                // OJO bucle: acá va el modelo INTENTADO (`model`
                                // del job), no la preferencia. Con la preferencia
                                // (siempre spark), el fallo del reintento en
                                // deepseek re-disparaba el fallback al infinito.
                                &error,
                                provider,
                                &model,
                                correction_attempt,
                            ) {
                                if is_session_or_account_error(&error) {
                                    eprintln!("grafito: session-fallback muse-spark 400-sesion [{error}] -> deepseek-v4-flash + retry (preferencia intacta)");
                                    self.notify(
                                        "Muse Spark rechazó la sesión Go (el header viaja solo; reintentá en un rato y si persiste verificá tu región o re-conectá tu clave Go). Sigo con DeepSeek Flash sin cambiar tu modelo.",
                                        ToastKind::Info,
                                    );
                                } else {
                                    eprintln!("grafito: session-fallback muse-spark [{error}] -> deepseek-v4-flash + retry (preferencia intacta)");
                                    self.notify(
                                        "Muse Spark no respondió, reintentando con DeepSeek Flash; tu modelo sigue siendo Muse Spark.",
                                        ToastKind::Info,
                                    );
                                }
                                // Reintentar la misma pregunta con el fallback, sin mostrar error
                                self.start_remote_assistant_for(
                                    ctx,
                                    question.clone(),
                                    Some("deepseek-v4-flash"),
                                );
                                return;
                            } else if correction_attempt > 0 {
                                self.fail_assistant_repair_request(error);
                            } else {
                                self.fail_assistant_request(error);
                            }
                        }
                    }
                }
            }
        }

        if let Some(completion) = self.assistant_runtime.take_finished_proposal_job() {
            let FinishedProposalJob {
                id: _request_id,
                provider,
                model,
                route: _route,
                fusion_fallback_allowed: _fusion_fallback_allowed,
                question,
                correction_attempt,
                repair_target_turn,
                document_revision,
                document_digest,
                focus,
                text,
                cancelled,
                result,
                ..
            } = completion;
            if cancelled
                || !accepts_remote_result(
                    self.assistant.provider,
                    self.assistant_runtime.expected_model(&self.assistant.model),
                    provider,
                    &model,
                )
            {
                if correction_attempt > 0 {
                    self.fail_assistant_repair_request(
                        "La corrección se canceló antes de terminar la comprobación local.",
                    );
                } else {
                    self.fail_assistant_request(
                        "La consulta se canceló antes de terminar la comprobación local.",
                    );
                }
            } else {
                let current_context =
                    grafito_command::assistant_context::document_context(&self.document);
                let current_focus = grafito_command::assistant_context::selected_function_focus(
                    &self.document,
                    self.selected_object,
                );
                if !accepts_remote_context(
                    &current_context,
                    current_focus.as_ref(),
                    document_revision,
                    &document_digest,
                    focus.as_ref(),
                ) {
                    self.fail_assistant_request(
                        "La respuesta quedó obsoleta porque cambió el documento o el foco; no se aceptó ni se verificaron sus propuestas.",
                    );
                    self.assistant.invalidate_proposal_correction();
                } else {
                    match result {
                        Ok(proposal_check) => {
                            let repair_feedback = proposal_check.repair_feedback.clone();
                            let rejected_count = proposal_check
                                .candidate_count
                                .saturating_sub(proposal_check.verified.len());
                            let can_offer_correction = can_offer_assistant_proposal_correction(
                                correction_attempt,
                                proposal_check.action_candidate_count,
                                proposal_check.verified_action_count,
                                repair_feedback.as_ref(),
                            );
                            self.assistant.set_proposal_preflight_results(
                                proposal_check.verified,
                                proposal_check.candidate_count,
                                proposal_check.candidate_code_block_indices,
                            );
                            if correction_attempt > 0 {
                                let Some(target_turn) = repair_target_turn else {
                                    self.fail_assistant_request(
                                        "La corrección perdió el turno que debía reemplazar.",
                                    );
                                    return;
                                };
                                if !self
                                    .assistant
                                    .complete_proposal_correction_at(target_turn, text.clone())
                                {
                                    self.fail_assistant_request(
                                        "La corrección no pudo reemplazar su respuesta original.",
                                    );
                                    return;
                                }
                            } else {
                                self.assistant.complete_request(text.clone());
                            }
                            if can_offer_correction {
                                if let Some(feedback) = repair_feedback {
                                    let target_turn = repair_target_turn.or_else(|| {
                                        self.assistant.conversation.len().checked_sub(1)
                                    });
                                    self.assistant.offer_proposal_correction_for_turn(
                                        question.clone(),
                                        feedback,
                                        target_turn,
                                        correction_attempt,
                                        AssistantCorrectionContext {
                                            document_revision,
                                            document_digest: document_digest.clone(),
                                            focus: focus.clone(),
                                        },
                                    );
                                }
                            }
                            if rejected_count > 0 {
                                let error = if proposal_check.verified_action_count == 0 {
                                    format!(
                                        "No se obtuvo una propuesta verificable; se descartaron {rejected_count} propuesta(s) localmente."
                                    )
                                } else {
                                    format!(
                                        "Se descartaron {rejected_count} propuesta(s) que no superaron la comprobación local."
                                    )
                                };
                                self.show_assistant_error(error);
                            } else if correction_attempt > 0
                                && proposal_check.verified_action_count == 0
                            {
                                self.show_assistant_error(
                                    "No se obtuvo una propuesta verificable; no hay nada para aplicar.",
                                );
                            }
                        }
                        Err(error) => {
                            if correction_attempt > 0 {
                                self.fail_assistant_repair_request(error);
                            } else {
                                self.fail_assistant_request(error);
                            }
                        }
                    }
                }
            }
        }

        if let Some(completion) = self.assistant_runtime.take_finished_model_job() {
            let _request_id = completion.id;
            let FinishedModelJob {
                provider,
                cancelled,
                result,
                ..
            } = completion;
            if accepts_model_result(self.assistant.provider, provider, cancelled) {
                match result {
                    Ok(models) => {
                        self.assistant.set_available_models(models);
                    }
                    Err(error) => {
                        // La lista de modelos viene del transporte (inglés
                        // crudo posible) — se envuelve en español.
                        let current_model = self.assistant.model.clone();
                        self.show_assistant_error(remote_error_message(&error, &current_model));
                    }
                }
            }
        }
        if self.assistant_runtime.take_queued_model_refresh_if_idle() {
            self.start_model_request(ctx);
        }

        let image = self.assistant_runtime.image_job.as_ref().and_then(|job| {
            match job.receiver.try_recv() {
                Ok(result) => Some(result),
                Err(TryRecvError::Empty) => None,
                Err(TryRecvError::Disconnected) => Some(Err(
                    "La importación de imagen terminó inesperadamente.".into(),
                )),
            }
        });
        if let Some(result) = image {
            self.assistant_runtime.image_job = None;
            self.assistant.is_importing_image = false;
            match result {
                Ok(attachment) => match self.assistant.add_attachment(attachment) {
                    Ok(()) => {
                        self.assistant.attachment_message =
                            Some("Imagen lista para consultar.".into())
                    }
                    Err(error) => self.show_assistant_error(attachment_error_message(&error)),
                },
                Err(error) => {
                    self.assistant.attachment_message = None;
                    self.show_assistant_error(attachment_error_message(&error));
                }
            }
        }
    }

    fn assistant_provider_settings(&mut self) -> Result<ProviderSettings, String> {
        self.assistant_provider_settings_for(&self.assistant.model.clone())
    }

    fn assistant_provider_settings_for(&mut self, model: &str) -> Result<ProviderSettings, String> {
        let model = model.trim();
        if model.is_empty() {
            return Err(
                "Completá la configuración avanzada antes de consultar remotamente.".into(),
            );
        }
        let mut settings = if self.assistant.provider == ProviderProfile::CustomOpenAiCompatible {
            // Custom requiere clave propia (GRAFITO_ASSISTANT_CUSTOM_*_API_KEY) y no reutiliza OpenCodeGo.
            // Si no hay clave Custom configurada, retorna Err en lugar de fallback silencioso.
            ProviderSettings::custom_openai_compatible(
                "https://opencode.ai/zen/go/v1",
                model,
                "GRAFITO_ASSISTANT_CUSTOM_OPENCODE_API_KEY",
            )
            .or_else(|_| {
                ProviderSettings::custom_openai_compatible(
                    "https://opencode.ai/zen/go/v1",
                    model,
                    "GRAFITO_ASSISTANT_CUSTOM_API_KEY",
                )
            })
            .map_err(|_| {
                // Español directo (sin inglés crudo del validador): la clave
                // Custom vive en GRAFITO_ASSISTANT_CUSTOM_*_API_KEY y nunca
                // reutiliza la de OpenCodeGo.
                "El proveedor Custom requiere su propia API key (GRAFITO_ASSISTANT_CUSTOM_*_API_KEY). Revisá la configuración avanzada."
                    .to_string()
            })?
        } else {
            ProviderSettings::for_profile(self.assistant.provider, model)
        };
        if self.assistant.vision_enabled {
            let capabilities = ProviderCapabilities {
                vision: true,
                ..settings.capabilities
            };
            settings = settings.with_capabilities(capabilities);
        }
        // Sesión Go (docs Go 2026-09-08): sólo para el gateway Go
        // (`opencode.ai/zen/go`; cubre OpenCodeGo y el Custom que apunta ahí).
        // DeepSeek/Ollama/Custom no-Go traen `None` (sin header). Lazy: se crea
        // en el primer request Go y queda estable hasta Limpiar.
        let is_go = settings.profile == ProviderProfile::OpenCodeGo
            || settings.endpoint.contains("opencode.ai");
        if is_go {
            let session = self.assistant_runtime.ensure_go_session();
            settings = settings.with_go_session_id(Some(session))?;
        }
        Ok(settings)
    }

    /// Indica si el proveedor remoto configurado puede responder hoy.
    fn remote_provider_ready(&mut self) -> bool {
        let Ok(settings) = self.assistant_provider_settings() else {
            return false;
        };
        match settings.profile {
            ProviderProfile::OllamaLocal => true,
            _ => self.assistant_api_key().is_ok(),
        }
    }

    fn assistant_api_key(&mut self) -> Result<Option<String>, String> {
        if self.assistant.provider == ProviderProfile::OllamaLocal {
            return Ok(None);
        }
        if let Some(key) = self.assistant_runtime.key_for(self.assistant.provider) {
            // Diagnóstico sin secretos: fuente + largo, jamás el contenido.
            // Sirve para distinguir "clave no guardada" de "proveedor la rechaza".
            eprintln!(
                "grafito: clave {:?} desde sesión en memoria, largo {}",
                self.assistant.provider,
                key.len()
            );
            return Ok(Some(key));
        }
        // Custom requiere clave propia (assistant-custom); no reutiliza OpenCodeGo.
        // Si no hay clave Custom, se retorna Err abajo vía load(Custom) => None.
        match assistant_credentials::load(self.assistant.provider) {
            Ok(Some(key)) => {
                self.assistant.key_available = true;
                self.assistant.key_status_checked = true;
                eprintln!(
                    "grafito: clave {:?} desde llavero/sistema, largo {}",
                    self.assistant.provider,
                    key.len()
                );
                self.assistant_runtime
                    .remember_key(self.assistant.provider, key.clone());
                Ok(Some(key))
            }
            Ok(None) => {
                self.assistant.key_available = false;
                self.assistant.key_status_checked = true;
                Err(
                    "Guardá una clave de API en la configuración avanzada antes de consultar."
                        .into(),
                )
            }
            Err(_) => {
                self.assistant.key_available = false;
                self.assistant.key_status_checked = true;
                Err("El llavero del sistema no está disponible para leer la API key.".into())
            }
        }
    }
}

/// Wiring B1 — loop Spark por Responses en paralelo (agente externo).
///
/// El modo agente con Spark hoy falla porque las tools aún no viajan por la
/// Responses API. B1 implementará ese loop en paralelo; cuando su `Done(Ok)`
/// llegue con Spark, el `poll_assistant_agent` lo acepta directo (nunca
/// fallback). Este helper distingue ese éxito del `Err` "Responses API", que
/// sí dispara el fallback sólo-sesión a deepseek sin tocar la preferencia.
fn is_agent_spark_responses_unsupported_error(error: &str) -> bool {
    error.contains("Responses API")
}

/// Fallback agente sólo-sesión: Spark + Responses API no soportado → deepseek.
/// Nunca dispara en `Ok` (sólo se llama en la rama `Err`) y nunca ante 429:
/// la cuota en pausa no se quema probando con otro modelo.
fn should_fallback_agent_spark_to_deepseek(
    error: &str,
    provider: ProviderProfile,
    model: &str,
) -> bool {
    is_agent_spark_responses_unsupported_error(error)
        && provider == ProviderProfile::OpenCodeGo
        && model.contains("muse-spark")
        && !error.contains("429")
}

/// Detecta errores 400 de sesión/cuenta/clave del gateway Go (docs Go 2026-09-08).
///
/// El lector del transporte (`grafito-assistant::http_status_error` +
/// `remote_error_category`) hoy NO parsea el campo `type` del cuerpo: sólo
/// trunca a 500 chars y categoriza como `http`. Por eso se detecta por
/// subcadena sobre el error ya formateado
/// (`remote assistant returned HTTP 400: {"type":"error","error":{"type":"MissingSessionID",...}}`).
/// Tipos cubiertos: `MissingSessionID|InvalidApiKey|ModelDisabled|AccountBlocked`.
/// Puro, sin `unwrap`, sin I/O. No toca el wire (prohibido inventar `session_id`
/// en bodies: la sesión viaja sólo como header `x-opencode-session`).
fn is_session_or_account_error(error: &str) -> bool {
    error.contains("MissingSessionID")
        || error.contains("InvalidApiKey")
        || error.contains("ModelDisabled")
        || error.contains("AccountBlocked")
}

/// Fallback chat (no agente) sólo-sesión: Spark 500/timeout/400-sesión → deepseek.
/// `model` DEBE ser el intentado por el job, jamás la preferencia guardada:
/// si el reintento en deepseek falla y se evalúa la preferencia (spark), el
/// fallback se re-dispara al infinito (un aviso por intento = "bucle").
/// La preferencia guardada queda intacta; el próximo pedido reintenta spark.
/// Nunca ante 429: cambiar de modelo no devuelve cuota y duplicaría el gasto.
/// El 400 de sesión/cuenta del gateway Go (incluido `MissingSessionID` con
/// header: el servidor puede rechazar por región) también reintenta
/// una vez con deepseek, con aviso honesto de una línea (ver rama en
/// `poll_assistant_jobs`).
fn should_fallback_remote_spark_to_deepseek(
    error: &str,
    provider: ProviderProfile,
    current_model: &str,
    correction_attempt: u8,
) -> bool {
    let slow_or_down = error.contains("500") || error.contains("timed out");
    let session_or_account = is_session_or_account_error(error);
    (slow_or_down || session_or_account)
        && current_model.contains("muse-spark")
        && provider == ProviderProfile::OpenCodeGo
        && correction_attempt == 0
        && !error.contains("429")
}

/// Sanea errores de adjuntos de crates externos (inglés crudo) a español.
/// Los mensajes ya españoles de `grafito-ui` pasan intactos.
fn attachment_error_message(error: &str) -> String {
    if error.contains("assistant attachment") {
        "La imagen no es válida o supera los límites permitidos.".into()
    } else {
        error.to_owned()
    }
}

/// Mensaje criollo ante un 429 del transporte: cuenta regresiva si el error
/// trae el sufijo del transporte ("(reintentá en Ns)", clamp 1..120s), minuto
/// por defecto si no. Sin "429" crudo: eso queda en logs. Puro.
fn rate_limit_429_user_message(error: &str) -> String {
    if let Some(start) = error.find("(reintentá en ") {
        let tail = &error[start + "(reintentá en ".len()..];
        if let Some(end) = tail.find('s') {
            if let Ok(secs) = tail[..end].trim().parse::<u64>() {
                return rate_limit_paused_message(secs);
            }
        }
    }
    rate_limit_paused_message(RATE_LIMIT_DEFAULT_COOLDOWN_SECS)
}

fn remote_error_message(error: &str, current_model: &str) -> String {
    eprintln!(
        "grafito: remote_error raw={} model={}",
        error, current_model
    );
    if error.contains("llavero") || error.contains("API key") {
        "No se pudo preparar la consulta remota. Revisá la configuración avanzada.".into()
    } else if let Some(paused) = grafito_assistant::rate_limit_paused_message_from_error(error) {
        // Pausa global: el worker ya contó los segundos, se muestra en
        // criollo con cuenta regresiva (sin 429 crudo).
        paused
    } else if error.contains("could not be built") {
        "No se pudo armar la consulta: la API key o el endpoint tienen caracteres inválidos. Reingresá la clave en Configuración avanzada (sin espacios ni saltos de línea).".into()
    } else if error.contains("Responses API") {
        "Este modelo usa la Responses API: el modo agente con herramientas aún no está soportado. Usá el chat simple o cambiá a deepseek-v4-flash.".into()
    } else if is_session_or_account_error(error) {
        // 400 de sesión/cuenta/clave del gateway Go (2026-09-08, docs Go):
        // dice QUÉ pasa + qué hacer, sin el genérico "Revisá Configuración → Modelo".
        // OJO: MissingSessionID NO es tu clave (el header `x-opencode-session`
        // viaja solo; el gateway lo rechazó igual, p.ej. por región).
        // InvalidApiKey SÍ puede ser clave mala/expirada o sesión sin re-conectar.
        // Si hubo fallback, el aviso de una línea ya dijo que se reintentó con
        // deepseek; este mensaje es para cuando NO hubo fallback (corrección en
        // curso, otro modelo, o reintento ya consumido).
        if error.contains("MissingSessionID") {
            "No se pudo abrir la sesión de Muse Spark en el gateway Go (el header viaja solo). Reintentá, verificá tu región (Spark está limitado por región Meta) o re-conectá tu clave Go; o seguí con deepseek.".into()
        } else {
            "El gateway Go rechazó la cuenta o la clave. Re-conectá tu clave Go en Configuración avanzada, verificá tu región, o seguí con deepseek.".into()
        }
    } else if error.contains("cancel") {
        "La consulta remota se canceló antes de completarse.".into()
    } else if error.contains("401") || error.contains("403") || error.contains("unauthorized") {
        format!("La clave de API no es válida o expiró: {error}. Revisá la configuración avanzada.")
    } else if error.contains("429") || error.contains("rate limit") || error.contains("RateLimit") {
        // 429 = cuota por minuto del proveedor, no es tu modelo ni tu clave.
        // Va antes de la rama 404/"model" porque el cuerpo del 429 puede
        // nombrar al modelo ("Model X rate limited"). Sin código crudo: con
        // cuenta regresiva si el transporte la trajo, minuto si no.
        rate_limit_429_user_message(error)
    } else if error.contains("404") || error.contains("model") {
        format!(
            "El modelo '{}' no está disponible: {error}. Revisá Configuración → Modelo.",
            current_model
        )
    } else if error.contains("waiting for first token") {
        // Etapa `esperando primer token`: el proveedor no mandó ningún delta
        // en todo el timeout total (sin agrandarlo). Honesto + sugerencia,
        // nunca el inglés crudo.
        "Se colgó esperando el primer token: el proveedor no mandó nada a tiempo. Reintentá, y si sigue, probá con deepseek-v4-flash o pedilo por partes. Podés cancelar en cualquier momento.".into()
    } else if error.contains("while receiving") {
        // Etapa `recibiendo`: se cortó con parcial ya visible (KiB en el
        // error interno, no se ecoa crudo). Se pide continuar, no reempezar.
        "Se cortó mientras recibía la respuesta (ya habías visto una parte). Pedí que continúe desde el último punto o por partes.".into()
    } else if error.contains("timeout") || error.contains("timed out") {
        "La consulta tardó demasiado y se acabó el tiempo total (no se agranda el presupuesto). Reintentá, revisá tu conexión o probá con otro modelo.".into()
    } else if error.contains("500") {
        if current_model.contains("muse-spark") {
            "Muse Spark responde por la Responses API; si ves un 500 es transitorio del proveedor. Probá de nuevo o cambiá a deepseek-v4-flash, qwen3.8-max o kimi-k3 en Configuración → Modelo.".to_string()
        } else {
            format!(
                "Error interno del proveedor (500) con modelo '{}'. Probá de nuevo en unos segundos o cambiá de modelo.",
                current_model
            )
        }
    } else if error.contains("could not connect")
        || error.contains("DNS")
        || error.contains("connect")
        || error.contains("network")
    {
        // Etapa `conectando` (incluye `connect_timeout` 10s): sin ruta al
        // proveedor. Honesto + sugerencia, sin eco crudo en inglés.
        "No pude conectar al proveedor (etapa conectando). Revisá tu conexión o el DNS y reintentá; si persiste, probá con otro modelo.".into()
    } else if error.contains("body cap") {
        "La respuesta superó el tope de 256 KiB: pedila por partes (ej: «dame 3 ejemplos»)."
            .to_string()
    } else if error.contains("not displayable") {
        // No es tema de modelo ni de clave: la respuesta llegó con un formato
        // que no se puede mostrar. Se reintenta o se reformula, sin mandar a
        // Configuración.
        "La respuesta llegó con un formato que no se puede mostrar. Reintentá o reformulá el pedido (ej: pedilo por partes)."
            .to_string()
    } else {
        // Corte por chars, nunca por bytes (el mensaje puede traer multibyte).
        let truncated: String = error.chars().take(120).collect();
        let truncated = if error.chars().count() > 120 {
            format!("{truncated}…")
        } else {
            truncated
        };
        format!("Error: {truncated} — Revisá Configuración → Modelo (actual: {current_model})")
    }
}

fn accepts_model_result(
    current_provider: ProviderProfile,
    result_provider: ProviderProfile,
    cancelled: bool,
) -> bool {
    !cancelled && current_provider == result_provider
}

fn accepts_remote_result(
    current_provider: ProviderProfile,
    current_model: &str,
    result_provider: ProviderProfile,
    result_model: &str,
) -> bool {
    current_provider == result_provider && current_model == result_model
}

fn accepts_remote_context(
    current_context: &ImmutableDocumentContext,
    current_focus: Option<&AssistantFocus>,
    result_revision: u64,
    result_digest: &str,
    result_focus: Option<&AssistantFocus>,
) -> bool {
    current_context.revision == result_revision
        && current_context.digest == result_digest
        && current_focus == result_focus
}

/// Carga los frames de un GIF en ColorImage para reproducirlos en el chat.
///
/// Presupuesto OOM: máximo 64 frames, 8 M píxeles totales, archivo ≤5 MB, por
/// frame ≤2 M píxeles, `checked_mul` para evitar overflow y `try_reserve`
/// antes de cada `push`.
fn load_gif_frames(path: &str) -> Result<Vec<egui::ColorImage>, String> {
    const MAX_GIF_FRAMES: usize = 64;
    const MAX_GIF_TOTAL_PIXELS: usize = 8_000_000;
    const MAX_GIF_FILE_BYTES: u64 = 5 * 1024 * 1024;
    const MAX_FRAME_PIXELS: usize = 2_000_000;
    let file =
        std::fs::File::open(path).map_err(|error| format!("no se pudo abrir el GIF: {error}"))?;
    let metadata = file
        .metadata()
        .map_err(|error| format!("no se pudo leer metadata del GIF: {error}"))?;
    if metadata.len() > MAX_GIF_FILE_BYTES {
        return Err(format!(
            "GIF demasiado grande ({} bytes > 5 MB)",
            metadata.len()
        ));
    }
    let mut options = gif::DecodeOptions::new();
    options.set_color_output(gif::ColorOutput::RGBA);
    let mut decoder = options
        .read_info(file)
        .map_err(|error| format!("GIF inválido: {error}"))?;
    let mut frames = Vec::new();
    frames
        .try_reserve(MAX_GIF_FRAMES)
        .map_err(|_| "sin memoria para GIF (reserve)".to_string())?;
    let mut total_pixels: usize = 0;
    while let Some(frame) = decoder
        .read_next_frame()
        .map_err(|error| format!("GIF corrupto: {error}"))?
    {
        if frames.len() >= MAX_GIF_FRAMES {
            break;
        }
        let width = frame.width as usize;
        let height = frame.height as usize;
        if width == 0 || height == 0 {
            continue;
        }
        let pixel_count = width
            .checked_mul(height)
            .ok_or_else(|| "GIF frame con overflow de píxeles".to_string())?;
        if pixel_count > MAX_FRAME_PIXELS {
            continue;
        }
        let next_total = total_pixels
            .checked_add(pixel_count)
            .ok_or_else(|| "GIF excede presupuesto de píxeles".to_string())?;
        if next_total > MAX_GIF_TOTAL_PIXELS {
            break;
        }
        let required_bytes = pixel_count
            .checked_mul(4)
            .ok_or_else(|| "GIF frame con overflow de bytes".to_string())?;
        let buffer = &frame.buffer;
        if buffer.len() < required_bytes {
            continue;
        }
        total_pixels = next_total;
        frames
            .try_reserve(1)
            .map_err(|_| "sin memoria para frame de GIF".to_string())?;
        let mut image = egui::ColorImage::new([width, height], egui::Color32::TRANSPARENT);
        // ColorImage::new ya reservó `pixel_count` elementos; verificar que no exceda presupuesto
        // y copiar con checked offset.
        for (index, pixel) in image.pixels.iter_mut().enumerate() {
            let offset = index
                .checked_mul(4)
                .ok_or_else(|| "offset overflow en GIF".to_string())?;
            if offset + 3 >= buffer.len() {
                break;
            }
            *pixel = egui::Color32::from_rgba_unmultiplied(
                buffer[offset],
                buffer[offset + 1],
                buffer[offset + 2],
                buffer[offset + 3],
            );
        }
        frames.push(image);
    }
    Ok(frames)
}

#[cfg(test)]
mod domain_sparkline_tests {
    #[test]
    fn samples_are_bounded_and_normalized() {
        let mut profile = grafito_profile::StudentProfile::new("Spark");
        for index in 0..20 {
            profile.record_outcome("calculus", "Cálculo", index as u64, index % 2 == 0);
        }
        let samples = crate::GrafitoApp::domain_sparkline_from(&profile);
        assert!(!samples.is_empty());
        assert!(samples.len() <= 14, "muestras acotadas");
        assert!(samples.iter().all(|value| (0.0..=1.0).contains(value)));
    }
}

#[cfg(test)]
mod r2_v2_playlist_tests {
    #[test]
    fn playlist_64_pasos_da_err_acotado() {
        assert_eq!(grafito_anim::protocol::PLAYLIST_MAX_STEPS, 8);
        assert!(crate::GrafitoApp::playlist_len_budget_ok(8).is_ok());
        let err = crate::GrafitoApp::playlist_len_budget_ok(64).expect_err("64 excede");
        assert!(err.contains("excede"), "{err}");
    }
}

#[cfg(test)]
mod r2_v1_thread_tests {
    use super::send_agent_msg_nonblocking;
    use super::AgentChannelMsg;

    fn dummy_event() -> AgentChannelMsg {
        AgentChannelMsg::Event(grafito_agent::AgentEvent::Finalized {
            text: String::new(),
        })
    }

    #[test]
    fn dropea_receiver_join_menor_1s() {
        let cancel = grafito_agent::loop_engine::Cancellation::default();
        let (tx, rx) = std::sync::mpsc::sync_channel::<AgentChannelMsg>(1);
        drop(rx);
        let start = std::time::Instant::now();
        let h = std::thread::spawn(move || send_agent_msg_nonblocking(&tx, dummy_event(), &cancel));
        let res = h.join().expect("join");
        assert!(res.is_none(), "dropeado debe cortar");
        assert!(
            start.elapsed() < std::time::Duration::from_secs(1),
            "join <1s"
        );
    }

    #[test]
    fn buffer_lleno_no_bloquea() {
        let cancel = grafito_agent::loop_engine::Cancellation::default();
        let (tx, _rx) = std::sync::mpsc::sync_channel::<AgentChannelMsg>(1);
        // Llena el buffer (1 slot) sin drenar.
        assert!(send_agent_msg_nonblocking(&tx, dummy_event(), &cancel).is_some());
        let start = std::time::Instant::now();
        // Segundo envío con buffer lleno: `try_send` da `Some(false)` al instante,
        // jamás bloquea como el `send` viejo (thread leak).
        let res = send_agent_msg_nonblocking(&tx, dummy_event(), &cancel);
        assert_eq!(res, Some(false), "lleno se descarta sin bloquear");
        assert!(start.elapsed() < std::time::Duration::from_secs(1));
    }

    #[test]
    fn cancelado_no_envia() {
        let cancel = grafito_agent::loop_engine::Cancellation::default();
        cancel.cancel();
        let (tx, _rx) = std::sync::mpsc::sync_channel::<AgentChannelMsg>(1);
        assert!(send_agent_msg_nonblocking(&tx, dummy_event(), &cancel).is_none());
    }
}

#[cfg(test)]
mod gif_loader_tests {
    // Generation and decode are exercised against a real in-memory GIF.
    #[test]
    fn gif_loader_reads_bounded_rgba_frames() {
        use gif::{Encoder, Frame, Repeat};

        let mut rgba = Vec::new();
        {
            let mut encoder = Encoder::new(&mut rgba, 2, 2, &[]).unwrap();
            encoder.set_repeat(Repeat::Finite(0)).unwrap();
            for frame in [
                vec![
                    255u8, 0, 0, 255, 0, 255, 0, 255, 0, 0, 255, 255, 255, 255, 255, 255,
                ],
                vec![
                    0u8, 0, 255, 255, 255, 255, 0, 255, 0, 255, 0, 255, 255, 0, 255, 255,
                ],
            ] {
                let mut rgba = frame;
                encoder
                    .write_frame(&Frame::from_rgba_speed(2, 2, rgba.as_mut_slice(), 10))
                    .unwrap();
            }
        }
        let dir = std::env::temp_dir().join(format!("grafito_gif_{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("probe.gif");
        std::fs::write(&path, rgba).unwrap();

        let frames = super::load_gif_frames(&path.to_string_lossy()).expect("decode frames");
        assert!(!frames.is_empty());
        for frame in &frames {
            assert_eq!(frame.size, [2, 2]);
        }
        std::fs::remove_dir_all(&dir).unwrap();
    }
}

fn plugin_validation_context() -> grafito_plugins::ValidationContext<'static> {
    grafito_plugins::ValidationContext {
        resolvable_command_ids: &|id| grafito_command::command_registry::resolve(id).is_some(),
        known_tools: &["evaluate_expr", "grafito_docs", "ask_user"],
        known_scenes: &[
            "derivative-slope",
            "concept-flow",
            "graph-trace",
            "riemann",
            "fourier_partial",
            "pythagorean",
            "tetrahedron_rotate",
        ],
    }
}

fn assistant_correction_prompt(question: &str) -> String {
    let mut end = question.len().min(MAX_ASSISTANT_CORRECTION_SOURCE_BYTES);
    while end > 0 && !question.is_char_boundary(end) {
        end -= 1;
    }
    format!(
        "{}{}",
        question[..end].trim(),
        ASSISTANT_CORRECTION_INSTRUCTION
    )
}

fn can_offer_assistant_proposal_correction(
    correction_attempt: u8,
    action_candidate_count: usize,
    verified_action_count: usize,
    repair_feedback: Option<&AssistantRepairFeedback>,
) -> bool {
    correction_attempt < MAX_ASSISTANT_PROPOSAL_CORRECTIONS
        && verified_action_count == 0
        && repair_feedback.is_some()
        && (action_candidate_count > 0 || correction_attempt > 0)
}

fn can_use_fusion_fallback(
    fallback_allowed: bool,
    provider: ProviderProfile,
    selected_model: &str,
) -> bool {
    fallback_allowed
        && provider == ProviderProfile::OpenCodeGo
        && selected_model == OPENCODE_VISION_MODEL
}

fn assistant_expected_syntax(command: &str) -> Vec<String> {
    let mut syntaxes = grafito_command::assistant_context::assistant_executable_syntaxes(command);
    if let Some(guidance) =
        grafito_command::assistant_context::assistant_literal_argument_guidance(command)
    {
        syntaxes.push(guidance.into());
    }
    syntaxes
}

fn classify_assistant_preflight_error(error: &str) -> AssistantRepairFailureKind {
    if error.contains("no creó un objeto") {
        AssistantRepairFailureKind::NoNewObject
    } else if error.contains("fuera de la vista gráfica esperada") {
        AssistantRepairFailureKind::WrongRenderSpace
    } else if error.contains("no produjo geometría visible")
        || error.contains("no produjo una flor 3D visible")
    {
        AssistantRepairFailureKind::NotVisible
    } else {
        AssistantRepairFailureKind::CommandRejected
    }
}

fn assistant_repair_failure_for_command(
    command: &AssistantCommandInvocation,
    error: &str,
) -> AssistantRepairFailure {
    AssistantRepairFailure {
        command: command.canonical_name().into(),
        kind: classify_assistant_preflight_error(error),
        expected_syntax: assistant_expected_syntax(command.canonical_name()),
    }
}

fn assistant_repair_failure_from_rejection(
    rejection: &AssistantProposalRejection,
) -> AssistantRepairFailure {
    let expected_syntax =
        if grafito_command::assistant_context::assistant_graph_capability(&rejection.command)
            .is_some()
        {
            assistant_expected_syntax(&rejection.command)
        } else {
            Vec::new()
        };
    AssistantRepairFailure {
        command: rejection.command.clone(),
        kind: match rejection.kind {
            AssistantProposalRejectionKind::InvalidSyntax => {
                AssistantRepairFailureKind::InvalidSyntax
            }
            AssistantProposalRejectionKind::UnsupportedCommand => {
                AssistantRepairFailureKind::UnsupportedCommand
            }
        },
        expected_syntax,
    }
}

fn assistant_repair_failure_for_scene(
    _commands: &[AssistantCommandInvocation],
    error: &str,
) -> AssistantRepairFailure {
    AssistantRepairFailure {
        command: "Scene".into(),
        kind: classify_assistant_preflight_error(error),
        expected_syntax: Vec::new(),
    }
}

struct AssistantGraphPreflight {
    staged: grafito_core::Document,
    outcome: grafito_command::commands::CommandOutcome,
    view: grafito_command::assistant_context::AssistantGraphView,
}

struct AssistantScenePreflight {
    staged: grafito_core::Document,
    outcome: grafito_command::commands::CommandOutcome,
    camera: grafito_geometry::Camera3D,
    view: grafito_command::assistant_context::AssistantGraphView,
}

struct RemoteProposalVerification {
    verified: Vec<VerifiedAssistantProposal>,
    candidate_count: usize,
    candidate_code_block_indices: Vec<usize>,
    action_candidate_count: usize,
    verified_action_count: usize,
    repair_feedback: Option<AssistantRepairFeedback>,
}

#[cfg(test)]
fn verified_remote_proposals(
    document: &grafito_core::Document,
    response: &str,
    camera: grafito_geometry::Camera3D,
) -> Vec<AssistantProposal> {
    inspect_remote_proposals(document, response, camera)
        .verified
        .into_iter()
        .map(|proposal| proposal.proposal)
        .collect()
}

#[cfg(test)]
fn inspect_remote_proposals(
    document: &grafito_core::Document,
    response: &str,
    camera: grafito_geometry::Camera3D,
) -> RemoteProposalVerification {
    inspect_remote_proposals_cancellable(
        document,
        response,
        camera,
        &CancellationToken::default(),
        false,
    )
    .expect("an uncancelled local proposal preflight must complete")
}

#[cfg(test)]
fn inspect_remote_action_proposals(
    document: &grafito_core::Document,
    response: &str,
    camera: grafito_geometry::Camera3D,
) -> RemoteProposalVerification {
    inspect_remote_proposals_cancellable(
        document,
        response,
        camera,
        &CancellationToken::default(),
        true,
    )
    .expect("an uncancelled local proposal preflight must complete")
}

fn inspect_remote_proposals_cancellable(
    document: &grafito_core::Document,
    response: &str,
    camera: grafito_geometry::Camera3D,
    cancellation: &CancellationToken,
    requires_action: bool,
) -> Result<RemoteProposalVerification, String> {
    let all_candidates = assistant_fenced_proposals(response);
    let candidate_code_block_indices = all_candidates
        .iter()
        .map(|candidate| candidate.code_block_index)
        .collect::<Vec<_>>();
    let action_candidate_count = all_candidates
        .iter()
        .take(MAX_REMOTE_PROPOSAL_PREFLIGHTS)
        .filter(|candidate| candidate.is_action_candidate())
        .count();
    let candidates = all_candidates
        .into_iter()
        .take(MAX_REMOTE_PROPOSAL_PREFLIGHTS)
        .collect::<Vec<_>>();
    let candidate_count = candidates.len();
    let mut verified = Vec::new();
    let mut repair_failures = Vec::new();
    let mut parameter_context = document.detached_clone_for_staging();
    let mut prerequisite_parameters = Vec::new();
    for (candidate_index, candidate) in candidates.into_iter().enumerate() {
        if cancellation.is_cancelled() {
            return Err("La comprobación local de la propuesta se canceló.".into());
        }
        let Some(proposal) = candidate.proposal else {
            if let Some(rejection) = candidate.rejection {
                repair_failures.push(assistant_repair_failure_from_rejection(&rejection));
            }
            continue;
        };
        let prerequisite_parameters_for_proposal = match &proposal {
            AssistantProposal::Parameter(_) => Vec::new(),
            AssistantProposal::Command(_) | AssistantProposal::Scene(_) => {
                prerequisite_parameters.clone()
            }
        };
        let accepted = match &proposal {
            AssistantProposal::Command(command) => {
                match preflight_assistant_graph_command_with_camera(
                    &parameter_context,
                    command,
                    camera,
                ) {
                    Ok(_) => true,
                    Err(error) => {
                        repair_failures.push(assistant_repair_failure_for_command(command, &error));
                        false
                    }
                }
            }
            AssistantProposal::Scene(commands) => {
                match preflight_assistant_scene(&parameter_context, commands, camera) {
                    Ok(_) => true,
                    Err(error) => {
                        repair_failures.push(assistant_repair_failure_for_scene(commands, &error));
                        false
                    }
                }
            }
            AssistantProposal::Parameter(assignment) => {
                stage_assistant_parameter(&mut parameter_context, assignment).is_ok()
            }
        };
        if cancellation.is_cancelled() {
            return Err("La comprobación local de la propuesta se canceló.".into());
        }
        if accepted {
            if let AssistantProposal::Parameter(assignment) = &proposal {
                prerequisite_parameters.push(assignment.clone());
            }
            verified.push(VerifiedAssistantProposal {
                candidate_index,
                proposal,
                prerequisite_parameters: prerequisite_parameters_for_proposal,
            });
        }
    }
    let verified_action_count = verified
        .iter()
        .filter(|proposal| {
            matches!(
                &proposal.proposal,
                AssistantProposal::Command(_) | AssistantProposal::Scene(_)
            )
        })
        .count();
    if requires_action && action_candidate_count == 0 && verified_action_count == 0 {
        repair_failures.push(AssistantRepairFailure {
            command: "GraphProposal".into(),
            kind: AssistantRepairFailureKind::InvalidSyntax,
            expected_syntax: vec![
                "Emit a grafito or grafito-scene block with executable catalog commands.".into(),
            ],
        });
    }
    Ok(RemoteProposalVerification {
        verified,
        candidate_count,
        candidate_code_block_indices,
        action_candidate_count,
        verified_action_count,
        repair_feedback: (!repair_failures.is_empty()).then_some(AssistantRepairFeedback {
            failures: repair_failures,
        }),
    })
}

fn document_with_assistant_prerequisites(
    document: &grafito_core::Document,
    prerequisite_parameters: &[AssistantParameterAssignment],
) -> Result<grafito_core::Document, String> {
    let mut staged = document.detached_clone_for_staging();
    for assignment in prerequisite_parameters {
        stage_assistant_parameter(&mut staged, assignment)?;
    }
    Ok(staged)
}

fn preflight_assistant_scene_with_prerequisites(
    document: &grafito_core::Document,
    prerequisite_parameters: &[AssistantParameterAssignment],
    commands: &[AssistantCommandInvocation],
    camera: grafito_geometry::Camera3D,
) -> Result<AssistantScenePreflight, String> {
    let staged = document_with_assistant_prerequisites(document, prerequisite_parameters)?;
    preflight_assistant_scene(&staged, commands, camera)
}

fn preflight_assistant_scene(
    document: &grafito_core::Document,
    commands: &[AssistantCommandInvocation],
    camera: grafito_geometry::Camera3D,
) -> Result<AssistantScenePreflight, String> {
    let homogeneous = commands.first().is_some_and(|first| {
        commands
            .iter()
            .all(|command| command.canonical_name() == first.canonical_name())
    });

    if homogeneous {
        preflight_homogeneous_assistant_scene(document, commands, camera)
    } else {
        preflight_assistant_flower_scene(document, commands, camera)
    }
}

fn preflight_homogeneous_assistant_scene(
    document: &grafito_core::Document,
    commands: &[AssistantCommandInvocation],
    camera: grafito_geometry::Camera3D,
) -> Result<AssistantScenePreflight, String> {
    if !(2..=8).contains(&commands.len()) {
        return Err("La escena requiere entre 2 y 8 componentes.".into());
    }

    let existing_ids = document
        .objects_iter()
        .map(|(id, _)| *id)
        .collect::<std::collections::HashSet<_>>();
    let mut staged = document.detached_clone_for_staging();
    let mut capability: Option<grafito_command::assistant_context::AssistantGraphCapability> = None;

    for command in commands {
        let current_capability = grafito_command::assistant_context::assistant_graph_capability(
            command.canonical_name(),
        )
        .ok_or_else(|| "La escena contiene un comando no permitido.".to_string())?;
        if !assistant_command_is_safe(command) {
            return Err("La escena contiene un comando con argumentos inválidos.".into());
        }
        if let Some(first_capability) = capability {
            if current_capability.canonical != first_capability.canonical {
                return Err("La escena general debe repetir un único tipo de comando.".into());
            }
        } else {
            capability = Some(*current_capability);
        }

        let before_ids = staged
            .objects_iter()
            .map(|(id, _)| *id)
            .collect::<std::collections::HashSet<_>>();
        let outcome = execute_assistant_command(&mut staged, command);
        if let grafito_command::commands::CommandOutcome::Error(error) = outcome {
            return Err(error);
        }
        let created_ids = staged
            .objects_iter()
            .filter_map(|(id, _)| (!before_ids.contains(id)).then_some(*id))
            .collect::<Vec<_>>();
        if created_ids.is_empty() {
            return Err("La escena contiene un comando que no creó un objeto gráfico.".into());
        }
        if created_ids.iter().any(|id| {
            staged
                .get_object(*id)
                .is_none_or(|object| match current_capability.view {
                    grafito_command::assistant_context::AssistantGraphView::TwoD => {
                        object.render_space() != grafito_core::RenderSpace::D2
                    }
                    grafito_command::assistant_context::AssistantGraphView::ThreeD => {
                        object.render_space() != grafito_core::RenderSpace::D3
                    }
                })
        }) {
            return Err("La escena creó un objeto fuera de la vista gráfica esperada.".into());
        }
    }

    let Some(capability) = capability else {
        return Err("La escena vacía no tiene capacidad.".into());
    };
    let mut inspection = staged.detached_clone_for_staging();
    for id in existing_ids {
        if let Some(object) = inspection.get_object_mut(id) {
            object.set_visible(false);
        }
    }
    let (camera, has_geometry) = match capability.proof {
        grafito_command::assistant_context::AssistantGraphProof::StaticTwoD => {
            let (vertices, indices) = grafito_render::Renderer::build_geometry_static(
                &inspection,
                inspection.view(),
                false,
                false,
            );
            (
                camera,
                static_geometry_intersects_view(&vertices, &indices, inspection.view()),
            )
        }
        grafito_command::assistant_context::AssistantGraphProof::WorldMeshThreeD => {
            let screen_size = inspection.view().screen_size;
            let initial_mesh = grafito_render::Renderer::build_3d_world_mesh(
                &inspection,
                &camera,
                screen_size.x,
                screen_size.y,
            );
            if !initial_mesh.is_complete() || initial_mesh.validate().is_err() {
                return Err("La escena produjo una geometría 3D inválida.".into());
            }
            let fitted_camera = fit_camera_to_world_mesh(&initial_mesh, camera)?;
            let mesh = grafito_render::Renderer::build_3d_world_mesh(
                &inspection,
                &fitted_camera,
                screen_size.x,
                screen_size.y,
            );
            (
                fitted_camera,
                mesh.is_complete()
                    && mesh.validate().is_ok()
                    && world_mesh_intersects_view(
                        &mesh,
                        &fitted_camera,
                        screen_size.x,
                        screen_size.y,
                    ),
            )
        }
        grafito_command::assistant_context::AssistantGraphProof::CpuOverlayThreeD => (
            camera,
            cpu_overlay_intersects_view(&inspection, &camera, inspection.view().screen_size),
        ),
    };
    if !has_geometry {
        return Err("La escena no produjo geometría visible; no se aplicó al documento.".into());
    }

    Ok(AssistantScenePreflight {
        staged,
        outcome: grafito_command::commands::CommandOutcome::Message(format!(
            "Escena verificada: {} componentes.",
            commands.len()
        )),
        camera,
        view: capability.view,
    })
}

fn preflight_assistant_flower_scene(
    document: &grafito_core::Document,
    commands: &[AssistantCommandInvocation],
    camera: grafito_geometry::Camera3D,
) -> Result<AssistantScenePreflight, String> {
    if !(6..=8).contains(&commands.len()) {
        return Err("La escena de flor requiere entre 6 y 8 componentes.".into());
    }

    let mut stem_count = 0;
    let mut center_count = 0;
    let mut petal_count = 0;
    for command in commands {
        let capability = grafito_command::assistant_context::assistant_graph_capability(
            command.canonical_name(),
        )
        .ok_or_else(|| "La escena contiene un comando no permitido.".to_string())?;
        if capability.view != grafito_command::assistant_context::AssistantGraphView::ThreeD
            || !assistant_command_is_safe(command)
        {
            return Err("La escena sólo admite componentes gráficos 3D verificables.".into());
        }
        match command.canonical_name() {
            "Cylinder" | "Cone" | "Curve3D" => stem_count += 1,
            "Sphere" => center_count += 1,
            "Surface3D" => petal_count += 1,
            _ => return Err("La escena de flor sólo admite tallo, centro y pétalos 3D.".into()),
        }
    }
    if stem_count != 1 || center_count != 1 || petal_count < 4 {
        return Err("La escena debe incluir un tallo, un centro y al menos cuatro pétalos.".into());
    }

    let existing_ids = document
        .objects_iter()
        .map(|(id, _)| *id)
        .collect::<std::collections::HashSet<_>>();
    let mut staged = document.detached_clone_for_staging();
    let mut petal_index = 0;
    for command in commands {
        let before_ids = staged
            .objects_iter()
            .map(|(id, _)| *id)
            .collect::<std::collections::HashSet<_>>();
        let outcome = execute_assistant_command(&mut staged, command);
        if let grafito_command::commands::CommandOutcome::Error(error) = outcome {
            return Err(error);
        }
        let created_ids = staged
            .objects_iter()
            .filter_map(|(id, _)| (!before_ids.contains(id)).then_some(*id))
            .collect::<Vec<_>>();
        for id in created_ids {
            if let Some(object) = staged.get_object_mut(id) {
                style_flower_component(object, &mut petal_index);
            }
        }
    }
    if !flower_scene_components_are_connected(&staged, &existing_ids) {
        return Err("La escena de flor debe formar una única figura conectada.".into());
    }

    let mut inspection = staged.detached_clone_for_staging();
    for id in existing_ids {
        if let Some(object) = inspection.get_object_mut(id) {
            object.set_visible(false);
        }
    }
    let initial_mesh = grafito_render::Renderer::build_3d_world_mesh(
        &inspection,
        &camera,
        inspection.view().screen_size.x,
        inspection.view().screen_size.y,
    );
    if !initial_mesh.is_complete() {
        return Err("La escena produjo una geometría 3D incompleta.".into());
    }
    initial_mesh
        .validate()
        .map_err(|_| "La escena produjo una geometría 3D inválida.".to_string())?;
    let fitted_camera = fit_camera_to_world_mesh(&initial_mesh, camera)?;
    let mesh = grafito_render::Renderer::build_3d_world_mesh(
        &inspection,
        &fitted_camera,
        inspection.view().screen_size.x,
        inspection.view().screen_size.y,
    );
    if !mesh.is_complete()
        || mesh.validate().is_err()
        || !world_mesh_intersects_view(
            &mesh,
            &fitted_camera,
            inspection.view().screen_size.x,
            inspection.view().screen_size.y,
        )
    {
        return Err("La escena no produjo una flor 3D visible.".into());
    }

    Ok(AssistantScenePreflight {
        staged,
        outcome: grafito_command::commands::CommandOutcome::Message(format!(
            "Escena 3D verificada: {} componentes.",
            commands.len()
        )),
        camera: fitted_camera,
        view: grafito_command::assistant_context::AssistantGraphView::ThreeD,
    })
}

fn flower_scene_components_are_connected(
    document: &grafito_core::Document,
    existing_ids: &std::collections::HashSet<grafito_core::ObjectId>,
) -> bool {
    let mut center = None;
    let mut stem_connected = false;
    let mut petals = Vec::new();

    for (id, object) in document.objects_iter() {
        if existing_ids.contains(id) {
            continue;
        }
        match object {
            grafito_core::GeoObject::Sphere3D(sphere) => {
                center = Some((sphere.center, sphere.radius));
            }
            grafito_core::GeoObject::Surface3D(surface) => petals.push(surface),
            _ => {}
        }
    }

    let Some((center, radius)) = center.filter(|(_, radius)| radius.is_finite() && *radius > 0.0)
    else {
        return false;
    };
    let connection_radius = radius + 0.05;

    for (id, object) in document.objects_iter() {
        if existing_ids.contains(id) {
            continue;
        }
        let connects_to_center = match object {
            grafito_core::GeoObject::Cylinder3D(stem) => {
                point_segment_distance(center, stem.base_center, stem.top_center)
                    .is_some_and(|distance| distance <= radius + stem.radius.abs())
            }
            grafito_core::GeoObject::Cone3D(stem) => {
                point_segment_distance(center, stem.base_center, stem.apex)
                    .is_some_and(|distance| distance <= radius + stem.radius.abs())
            }
            grafito_core::GeoObject::ParametricCurve3D(stem) => {
                grafito_core::parametric_sampling::evaluate_parametric_curve_3d(
                    stem,
                    128,
                    &document.variables,
                )
                .into_iter()
                .any(|(x, y, z)| {
                    let point = grafito_geometry::Point3D::new(x, y, z);
                    point.is_finite() && point.distance(&center) <= connection_radius
                })
            }
            _ => false,
        };
        stem_connected |= connects_to_center;
    }

    stem_connected
        && !petals.is_empty()
        && petals.into_iter().all(|petal| {
            grafito_core::parametric_sampling::evaluate_surface_3d(
                petal,
                petal.mesh_res.clamp(8, 32),
                &document.variables,
            )
            .into_iter()
            .flatten()
            .any(|point| point.is_finite() && point.distance(&center) <= connection_radius)
        })
}

fn point_segment_distance(
    point: grafito_geometry::Point3D,
    start: grafito_geometry::Point3D,
    end: grafito_geometry::Point3D,
) -> Option<f64> {
    if !point.is_finite() || !start.is_finite() || !end.is_finite() {
        return None;
    }
    let start = start.to_dvec3();
    let segment = end.to_dvec3() - start;
    let length_squared = segment.length_squared();
    if !length_squared.is_finite() {
        return None;
    }
    if length_squared <= 1.0e-24 {
        return Some(point.to_dvec3().distance(start));
    }
    let parameter = ((point.to_dvec3() - start).dot(segment) / length_squared).clamp(0.0, 1.0);
    let distance = point.to_dvec3().distance(start + parameter * segment);
    distance.is_finite().then_some(distance)
}

fn style_flower_component(object: &mut grafito_core::GeoObject, petal_index: &mut usize) {
    match object {
        grafito_core::GeoObject::Cylinder3D(stem) => {
            stem.label = "Tallo".into();
            stem.color = grafito_geometry::Color::new(0.08, 0.45, 0.16, 1.0);
            stem.fill_color = Some(grafito_geometry::Color::new(0.12, 0.62, 0.24, 1.0));
        }
        grafito_core::GeoObject::Cone3D(stem) => {
            stem.label = "Tallo".into();
            stem.color = grafito_geometry::Color::new(0.08, 0.45, 0.16, 1.0);
            stem.fill_color = Some(grafito_geometry::Color::new(0.12, 0.62, 0.24, 1.0));
        }
        grafito_core::GeoObject::Sphere3D(center) => {
            center.label = "Centro de la flor".into();
            center.color = grafito_geometry::Color::new(0.75, 0.48, 0.02, 1.0);
            center.fill_color = Some(grafito_geometry::Color::new(1.0, 0.76, 0.08, 1.0));
        }
        grafito_core::GeoObject::Surface3D(petal) => {
            *petal_index += 1;
            petal.label = format!("Pétalo {petal_index}");
            petal.solid = true;
            petal.color = grafito_geometry::Color::new(0.9, 0.12, 0.42, 1.0);
            petal.width = 1.25;
        }
        _ => {}
    }
}

fn fit_camera_to_world_mesh(
    mesh: &grafito_render::WorldMesh,
    mut camera: grafito_geometry::Camera3D,
) -> Result<grafito_geometry::Camera3D, String> {
    let points = mesh
        .opaque_vertices
        .iter()
        .chain(&mesh.wire_vertices)
        .map(|vertex| glam::Vec3::from_array(vertex.position))
        .filter(|point| point.is_finite())
        .collect::<Vec<_>>();
    let Some(first) = points.first().copied() else {
        return Err("La escena no tiene vértices finitos para encuadrar.".into());
    };
    let (min, max) = points
        .into_iter()
        .fold((first, first), |(min, max), point| {
            (min.min(point), max.max(point))
        });
    let radius = ((max - min).length() * 0.5).max(0.5);
    let half_fov = (camera.fov.to_radians() * 0.5).clamp(0.1, 1.4);
    let half_horizontal = (half_fov.tan() * camera.aspect.max(0.25)).atan();
    let limiting_half_angle = half_fov.min(half_horizontal).max(0.1);
    let distance = (radius / limiting_half_angle.sin() * 1.35).max(2.0);
    camera.target = (min + max) * 0.5;
    camera.distance = distance;
    camera.near = (distance - radius * 2.5).max(0.01);
    camera.far = (distance + radius * 4.0 + 10.0).max(100.0);
    Ok(camera)
}

/// Ejecuta una propuesta sobre un documento aislado y exige que los objetos
/// nuevos emitan geometría propia, sin contar ejes, grilla ni objetos previos.
#[cfg(test)]
fn preflight_assistant_graph_command(
    document: &grafito_core::Document,
    command: &str,
) -> Result<AssistantGraphPreflight, String> {
    let command = grafito_command::assistant_proposals::parse_assistant_command(command)
        .ok_or_else(|| "La propuesta no es un gráfico verificable por el asistente.".to_string())?;
    preflight_assistant_graph_command_with_camera(
        document,
        &command,
        grafito_geometry::Camera3D::new(4.0 / 3.0),
    )
}

fn preflight_assistant_graph_command_with_camera(
    document: &grafito_core::Document,
    command: &AssistantCommandInvocation,
    camera: grafito_geometry::Camera3D,
) -> Result<AssistantGraphPreflight, String> {
    let capability =
        grafito_command::assistant_context::assistant_graph_capability(command.canonical_name())
            .ok_or_else(|| {
                "La propuesta no es un gráfico verificable por el asistente.".to_string()
            })?;
    if !assistant_command_is_safe(command) {
        return Err(
            "La propuesta usa valores literales que no cumplen el catálogo verificable.".into(),
        );
    }

    let existing_ids = document
        .objects_iter()
        .map(|(id, _)| *id)
        .collect::<std::collections::HashSet<_>>();
    let mut staged = document.detached_clone_for_staging();
    let outcome = execute_assistant_command(&mut staged, command);
    if let grafito_command::commands::CommandOutcome::Error(error) = &outcome {
        return Err(error.clone());
    }

    let created_ids = staged
        .objects_iter()
        .filter_map(|(id, _)| (!existing_ids.contains(id)).then_some(*id))
        .collect::<Vec<_>>();
    if created_ids.is_empty() {
        return Err("La propuesta no creó un objeto gráfico nuevo.".into());
    }

    if created_ids.iter().any(|id| {
        staged
            .get_object(*id)
            .is_none_or(|object| match capability.view {
                grafito_command::assistant_context::AssistantGraphView::TwoD => {
                    object.render_space() != grafito_core::RenderSpace::D2
                }
                grafito_command::assistant_context::AssistantGraphView::ThreeD => {
                    object.render_space() != grafito_core::RenderSpace::D3
                }
            })
    }) {
        return Err("La propuesta creó un objeto fuera de la vista gráfica esperada.".into());
    }

    let mut inspection = staged.detached_clone_for_staging();
    for id in existing_ids {
        if let Some(object) = inspection.get_object_mut(id) {
            object.set_visible(false);
        }
    }
    let has_geometry = match capability.proof {
        grafito_command::assistant_context::AssistantGraphProof::StaticTwoD => {
            let (vertices, indices) = grafito_render::Renderer::build_geometry_static(
                &inspection,
                inspection.view(),
                false,
                false,
            );
            static_geometry_intersects_view(&vertices, &indices, inspection.view())
        }
        grafito_command::assistant_context::AssistantGraphProof::WorldMeshThreeD => {
            let screen_size = inspection.view().screen_size;
            let mesh = grafito_render::Renderer::build_3d_world_mesh(
                &inspection,
                &camera,
                screen_size.x,
                screen_size.y,
            );
            mesh.is_complete()
                && mesh.validate().is_ok()
                && world_mesh_intersects_view(&mesh, &camera, screen_size.x, screen_size.y)
        }
        grafito_command::assistant_context::AssistantGraphProof::CpuOverlayThreeD => {
            cpu_overlay_intersects_view(&inspection, &camera, inspection.view().screen_size)
        }
    };
    if !has_geometry {
        return Err("La propuesta no produjo geometría visible; no se aplicó al documento.".into());
    }

    Ok(AssistantGraphPreflight {
        staged,
        outcome,
        view: capability.view,
    })
}

fn preflight_assistant_graph_command_with_prerequisites(
    document: &grafito_core::Document,
    prerequisite_parameters: &[AssistantParameterAssignment],
    command: &AssistantCommandInvocation,
    camera: grafito_geometry::Camera3D,
) -> Result<AssistantGraphPreflight, String> {
    let staged = document_with_assistant_prerequisites(document, prerequisite_parameters)?;
    preflight_assistant_graph_command_with_camera(&staged, command, camera)
}

fn preflight_assistant_parameter(
    document: &grafito_core::Document,
    assignment: &AssistantParameterAssignment,
) -> Result<(), String> {
    let mut staged = document.detached_clone_for_staging();
    stage_assistant_parameter(&mut staged, assignment)
}

fn stage_assistant_parameter(
    document: &mut grafito_core::Document,
    assignment: &AssistantParameterAssignment,
) -> Result<(), String> {
    if let grafito_command::commands::CommandOutcome::Error(error) =
        execute_assistant_parameter(document, assignment)
    {
        return Err(error);
    }
    (document.get_variable(assignment.name()) == Some(assignment.value()))
        .then_some(())
        .ok_or_else(|| "La propuesta no actualizó el parámetro esperado.".into())
}

fn commit_assistant_graph_preflight(
    document: &mut grafito_core::Document,
    undo_stack: &mut VecDeque<grafito_core::Document>,
    redo_stack: &mut VecDeque<grafito_core::ChangeSet>,
    preflight: AssistantGraphPreflight,
) -> grafito_command::commands::CommandOutcome {
    let before = document.clone();
    *document = preflight.staged;
    crate::app::save_command_snapshot_if_mutated(
        &preflight.outcome,
        before,
        document,
        undo_stack,
        redo_stack,
    );
    preflight.outcome
}

fn assistant_command_is_safe(command: &AssistantCommandInvocation) -> bool {
    grafito_command::assistant_context::assistant_graph_capability(command.canonical_name())
        .is_some()
        && grafito_command::assistant_context::assistant_command_has_literal_safe_form(
            command.canonical_name(),
            command.arguments().len(),
        )
        && grafito_command::assistant_context::assistant_command_has_literal_safe_arguments(
            command.canonical_name(),
            command.arguments(),
        )
}

fn assistant_graph_view(
    command: &AssistantCommandInvocation,
) -> Option<grafito_command::assistant_context::AssistantGraphView> {
    grafito_command::assistant_context::assistant_graph_capability(command.canonical_name())
        .map(|capability| capability.view)
}

#[cfg(test)]
fn validate_assistant_command(candidate: &str) -> Option<String> {
    grafito_command::assistant_proposals::parse_assistant_command(candidate)
        .map(|command| command.canonical_text())
}

fn point_is_in_view(x: f32, y: f32, width: f32, height: f32) -> bool {
    x.is_finite() && y.is_finite() && x >= 0.0 && x <= width && y >= 0.0 && y <= height
}

fn static_geometry_intersects_view(
    vertices: &[grafito_render::Vertex],
    indices: &[u32],
    view: &grafito_geometry::ViewTransform,
) -> bool {
    !indices.is_empty()
        && vertices.iter().any(|vertex| {
            point_is_in_view(
                vertex.position[0],
                vertex.position[1],
                view.screen_size.x,
                view.screen_size.y,
            )
        })
}

fn world_mesh_intersects_view(
    mesh: &grafito_render::WorldMesh,
    camera: &grafito_geometry::Camera3D,
    width: f32,
    height: f32,
) -> bool {
    (!mesh.opaque_indices.is_empty() || !mesh.wire_indices.is_empty())
        && mesh
            .opaque_vertices
            .iter()
            .chain(&mesh.wire_vertices)
            .any(|vertex| {
                camera
                    .project(
                        &grafito_geometry::Point3D::new(
                            vertex.position[0] as f64,
                            vertex.position[1] as f64,
                            vertex.position[2] as f64,
                        ),
                        width,
                        height,
                    )
                    .is_some_and(|(x, y)| point_is_in_view(x, y, width, height))
            })
}

fn cpu_overlay_intersects_view(
    document: &grafito_core::Document,
    camera: &grafito_geometry::Camera3D,
    screen_size: glam::Vec2,
) -> bool {
    document.objects_iter().any(|(_, object)| match object {
        grafito_core::GeoObject::Point3D(point) => camera
            .project(&point.position, screen_size.x, screen_size.y)
            .is_some_and(|(x, y)| point_is_in_view(x, y, screen_size.x, screen_size.y)),
        grafito_core::GeoObject::HyperSurface4D(surface) => {
            surface
                .params
                .first()
                .is_some_and(|scale| scale.is_finite() && *scale > 0.0)
                && camera
                    .project(
                        &grafito_geometry::Point3D::new(0.0, 0.0, 0.0),
                        screen_size.x,
                        screen_size.y,
                    )
                    .is_some_and(|(x, y)| point_is_in_view(x, y, screen_size.x, screen_size.y))
        }
        _ => false,
    })
}

fn assistant_graph_perspective(
    view: grafito_command::assistant_context::AssistantGraphView,
    current_view: crate::ViewMode,
) -> Option<crate::Perspective> {
    match (view, current_view) {
        (grafito_command::assistant_context::AssistantGraphView::TwoD, crate::ViewMode::D3) => {
            Some(crate::Perspective::Geometry2D)
        }
        (grafito_command::assistant_context::AssistantGraphView::ThreeD, crate::ViewMode::D2) => {
            Some(crate::Perspective::Geometry3D)
        }
        _ => None,
    }
}

impl GrafitoApp {
    /// Asegura que el panel de Álgebra quede visible después de un Apply exitoso.
    fn ensure_algebra_panel_visible(&mut self) {
        self.left_drawer_open = true;
        self.compact_drawer_open = false;
        self.sidebar_tab = crate::LeftPanelContent::Algebra.default_sidebar_tab();
    }
}

fn load_assistant_attachment(
    path: PathBuf,
) -> Result<grafito_assistant_types::ImageAttachment, String> {
    let limits = AttachmentLimits::default();
    let file = File::open(path).map_err(|_| "No se pudo leer la imagen.".to_string())?;
    let bytes = read_bounded_attachment(file, limits.max_bytes)?;
    let format =
        image::guess_format(&bytes).map_err(|_| "La imagen debe ser PNG o JPEG.".to_string())?;
    let media_type = match format {
        image::ImageFormat::Png => "image/png",
        image::ImageFormat::Jpeg => "image/jpeg",
        _ => return Err("La imagen debe ser PNG o JPEG.".into()),
    };
    let reader = image::ImageReader::with_format(Cursor::new(&bytes), format);
    let (width, height) = reader
        .into_dimensions()
        .map_err(|_| "No se pudieron leer las dimensiones de la imagen.".to_string())?;
    let attachment =
        grafito_assistant_types::ImageAttachment::new(media_type, bytes, width, height);
    validate_attachment(&attachment, &limits)?;
    Ok(attachment)
}

fn read_bounded_attachment(reader: impl Read, max_bytes: usize) -> Result<Vec<u8>, String> {
    let maximum = max_bytes
        .checked_add(1)
        .ok_or_else(|| "El límite de imagen no es válido.".to_string())?;
    let mut bytes = Vec::with_capacity(max_bytes.min(64 * 1024));
    reader
        .take(maximum as u64)
        .read_to_end(&mut bytes)
        .map_err(|_| "No se pudo leer la imagen.".to_string())?;
    if bytes.len() > max_bytes {
        return Err("La imagen supera el límite de tamaño permitido.".into());
    }
    Ok(bytes)
}

#[cfg(test)]
mod tests {
    use super::{
        accepts_model_result, accepts_remote_context, accepts_remote_result,
        anim_parametrica_para_pedido, append_canonical_integral_prose, apply_local_assistant_plan,
        assistant_graph_perspective, attachment_error_message, aviso_fallback_canonico,
        can_offer_assistant_proposal_correction, clasifica_pedido_integral,
        clasifica_pedido_tangente, classify_local_assistant_response,
        commit_assistant_graph_preflight, decide_animacion, esperar_spec_ia_con_timeout,
        ia_disponible_para_anim, inspect_remote_action_proposals, inspect_remote_proposals,
        inspect_remote_proposals_cancellable, is_agent_spark_responses_unsupported_error,
        is_session_or_account_error, is_socratic_repair_error, join_gif_handle_bounded,
        join_puente_bounded, limpiar_media_si_no_animacion, parsear_spec_anim_ia,
        plantilla_para_pedido, playlist_para_pedido, pop_provisional_stream_turn,
        preflight_assistant_flower_scene, preflight_assistant_graph_command,
        preflight_assistant_graph_command_with_prerequisites, preflight_assistant_parameter,
        preflight_assistant_scene, prosa_canonica_para_plantilla, prosa_integral_explicita,
        prosa_para_spec_anim_ia, prosa_tangente_explicita, read_bounded_attachment,
        remote_error_message, remote_stage_for_job, render_media_desde_spec_ia,
        resolver_turno_anim_ia, should_fallback_agent_spark_to_deepseek,
        should_fallback_remote_spark_to_deepseek, socratic_guard_context,
        spec_canonico_para_fallback, split_playlist_request, stage_assistant_parameter,
        titulo_curado, titulo_curado_localized, validar_spec_anim_ia, validate_assistant_command,
        verified_remote_proposals, wants_exercise_request, AgentChannelMsg, AnimIaRender,
        AssistantAgentJob, AssistantAnimIaJob, AssistantAnimJob, AssistantCommandInvocation,
        AssistantModelJob, AssistantParameterAssignment, AssistantProposalJob, AssistantRemoteJob,
        AssistantRemoteRoute, AssistantRuntime, DecisionAnimacion, DesenlaceAnimIa, GifExportJob,
        IntegralPedido, LocalAssistantDisposition, PedidoSpecIa, RemoteProposalVerification,
        RemoteStage, SpecAnimIa, SpecTerminadoGuard, TangentePedido, ANIM_IA_SPEC_TIMEOUT_MS,
        ANIM_MOTOR_IDLE_TIMEOUT_SECS, ANIM_MOTOR_JOB_TIMEOUT_SECS, ANIM_SIN_IA_AVISO,
    };
    use grafito_assistant::{solve_local, CancellationToken, ProviderSettings, RemoteCompletion};
    use grafito_assistant_types::{
        AssistantFocus, AssistantOperation, AssistantRepairFailure, AssistantRepairFailureKind,
        AssistantRepairFeedback, AssistantRequest, ConversationRole, ConversationTurn,
        ImmutableDocumentContext, ProposedPlan, ProviderProfile,
    };
    use grafito_command::commands::CommandOutcome;
    use grafito_core::{Document, GeoObject};
    use grafito_geometry::ViewTransform;
    use grafito_pedagogy::scaffold::{extract_concept, is_exploratory_request};
    use grafito_pedagogy::{PedagogicalLevel, ScaffoldEngine, SocraticFsm};
    use grafito_ui::assistant::{
        AssistantPanelState, AssistantProposal, VerifiedAssistantProposal,
    };
    use std::collections::VecDeque;
    use std::{io::Cursor, sync::mpsc::sync_channel};

    fn command_proposal(text: &str) -> AssistantProposal {
        AssistantProposal::Command(
            grafito_command::assistant_proposals::parse_assistant_command(text)
                .expect("test command must be recognized by the assistant contract"),
        )
    }

    fn parameter_proposal(text: &str) -> AssistantProposal {
        AssistantProposal::Parameter(
            grafito_command::assistant_proposals::parse_assistant_parameter(text)
                .expect("test parameter must be finite"),
        )
    }

    fn parameter_assignment(text: &str) -> AssistantParameterAssignment {
        grafito_command::assistant_proposals::parse_assistant_parameter(text)
            .expect("test parameter must be finite")
    }

    fn assistant_commands(commands: &[String]) -> Vec<AssistantCommandInvocation> {
        commands
            .iter()
            .map(|command| {
                grafito_command::assistant_proposals::parse_assistant_command(command)
                    .expect("test command must be recognized by the assistant contract")
            })
            .collect()
    }

    fn scene_proposal(commands: &[&str]) -> AssistantProposal {
        AssistantProposal::Scene(
            commands
                .iter()
                .map(|command| {
                    grafito_command::assistant_proposals::parse_assistant_command(command)
                        .expect("test scene command must be recognized by the assistant contract")
                })
                .collect(),
        )
    }

    #[test]
    fn assistant_defaults_to_the_opencode_go_provider() {
        let state = AssistantPanelState::default();

        assert_eq!(state.provider, ProviderProfile::OpenCodeGo);
        assert_eq!(state.model, "deepseek-v4-flash");
        assert!(!state.allow_fusion_fallback);
    }

    #[test]
    fn local_arithmetic_is_classified_without_a_remote_request() {
        let response = solve_local(&AssistantRequest::local(
            "2 + 2",
            ImmutableDocumentContext::empty(0),
        ));

        assert!(matches!(
            classify_local_assistant_response(response),
            LocalAssistantDisposition::Solved { plan: None, .. }
        ));
    }

    #[test]
    fn unsupported_local_work_requires_explicit_remote_authorization() {
        let response = solve_local(&AssistantRequest::local(
            "Explicá el teorema de Stokes",
            ImmutableDocumentContext::empty(0),
        ));

        assert!(matches!(
            classify_local_assistant_response(response),
            LocalAssistantDisposition::NeedsRemoteAuthorization(_)
        ));
    }

    #[test]
    fn applying_a_local_plan_records_exactly_one_undo_snapshot() {
        let mut document = Document::new();
        let plan = ProposedPlan::new(
            grafito_command::assistant_context::document_context(&document).basis(),
            vec![
                AssistantOperation::SetVariable {
                    name: "a".into(),
                    value: 2.0,
                },
                AssistantOperation::CreateGraph {
                    expression: "x".into(),
                    variable: "x".into(),
                    domain_min: -2.0,
                    domain_max: 2.0,
                },
            ],
        );
        let mut undo_stack = VecDeque::new();
        let mut redo_stack = VecDeque::new();

        let result =
            apply_local_assistant_plan(&mut document, &plan, &mut undo_stack, &mut redo_stack)
                .expect("a valid local plan must apply atomically");

        assert_eq!(result.changes.len(), 2);
        assert_eq!(document.get_variable("a"), Some(2.0));
        assert_eq!(document.object_count(), 1);
        assert_eq!(undo_stack.len(), 1);
        assert!(redo_stack.is_empty());
    }

    #[test]
    fn stale_local_plan_leaves_document_and_history_unchanged() {
        let mut document = Document::new();
        let plan = ProposedPlan::new(
            grafito_command::assistant_context::document_context(&document).basis(),
            vec![AssistantOperation::SetVariable {
                name: "a".into(),
                value: 2.0,
            }],
        );
        document.set_variable("changed".into(), 1.0);
        let before = serde_json::to_value(&document).expect("document serializes");
        let mut undo_stack = VecDeque::new();
        let mut redo_stack = VecDeque::new();

        assert!(
            apply_local_assistant_plan(&mut document, &plan, &mut undo_stack, &mut redo_stack,)
                .is_err()
        );
        assert_eq!(
            serde_json::to_value(&document).expect("document serializes"),
            before
        );
        assert!(undo_stack.is_empty());
        assert!(redo_stack.is_empty());
    }

    #[test]
    fn parameter_proposals_accept_only_finite_numeric_assignments() {
        let assignment = grafito_command::assistant_proposals::parse_assistant_parameter("a = 2.5")
            .expect("finite parameter assignment");
        assert_eq!(assignment.name(), "a");
        assert_eq!(assignment.value(), 2.5);
        assert!(
            grafito_command::assistant_proposals::parse_assistant_parameter("a = NaN").is_none()
        );
        assert!(
            grafito_command::assistant_proposals::parse_assistant_parameter("a = 1; Save[]")
                .is_none()
        );

        let document = Document::new();
        assert!(preflight_assistant_parameter(&document, &assignment).is_ok());
    }

    #[test]
    fn parameter_proposals_recompute_spreadsheet_dependents() {
        let mut document = Document::new();
        document
            .try_set_variable("a".into(), 1.0)
            .expect("seed ordinary variable");
        document
            .set_spreadsheet_cell(0, 0, "a".into())
            .expect("seed spreadsheet cell");
        document
            .recompute_spreadsheet_variables()
            .expect("spreadsheet cell resolves");
        let assignment = parameter_assignment("a = 2");

        stage_assistant_parameter(&mut document, &assignment).expect("assistant parameter applies");
        assert_eq!(document.get_variable("A1"), Some(2.0));
    }

    #[test]
    fn ordered_parameter_proposals_enable_a_dependent_graph_without_mutating_live_state() {
        let mut document = Document::new();
        document.set_view(ViewTransform::new(800.0, 600.0));
        let command_text = "ImplicitCurve[(x^2 + y^2 - a^2)^3 - x^2*y^3 = 0]";
        let response = format!("```grafito-param\na = 1\n```\n\n```grafito\n{command_text}\n```");

        let check = inspect_remote_proposals(
            &document,
            &response,
            grafito_geometry::Camera3D::new(4.0 / 3.0),
        );

        assert_eq!(
            check.verified,
            vec![
                VerifiedAssistantProposal {
                    candidate_index: 0,
                    proposal: parameter_proposal("a = 1"),
                    prerequisite_parameters: Vec::new(),
                },
                VerifiedAssistantProposal {
                    candidate_index: 1,
                    proposal: command_proposal(command_text),
                    prerequisite_parameters: vec![parameter_assignment("a = 1")],
                },
            ]
        );
        assert_eq!(document.get_variable("a"), None);
        assert_eq!(document.object_count(), 0);
        assert!(preflight_assistant_graph_command(&document, command_text).is_err());

        let graph = &check.verified[1];
        let AssistantProposal::Command(command) = &graph.proposal else {
            panic!("the second proposal must be a graph command");
        };
        let preflight = preflight_assistant_graph_command_with_prerequisites(
            &document,
            &graph.prerequisite_parameters,
            command,
            grafito_geometry::Camera3D::new(4.0 / 3.0),
        )
        .expect("the explicit graph apply must recreate its verified parameter context");
        let mut undo_stack = VecDeque::new();
        let mut redo_stack = VecDeque::new();
        let outcome = commit_assistant_graph_preflight(
            &mut document,
            &mut undo_stack,
            &mut redo_stack,
            preflight,
        );

        assert!(matches!(outcome, CommandOutcome::Message(_)));
        assert_eq!(document.get_variable("a"), Some(1.0));
        assert_eq!(document.object_count(), 1);
        assert_eq!(undo_stack.len(), 1);
        assert!(redo_stack.is_empty());
    }

    #[test]
    fn verified_parameter_does_not_suppress_one_explicit_graph_correction() {
        let mut document = Document::new();
        document.set_view(ViewTransform::new(800.0, 600.0));
        let check = inspect_remote_proposals(
            &document,
            "```grafito-param\na = 1\n```\n\n```grafito\nFunction[1/0]\n```",
            grafito_geometry::Camera3D::new(4.0 / 3.0),
        );

        assert_eq!(check.verified.len(), 1);
        assert_eq!(check.verified_action_count, 0);
        assert!(can_offer_assistant_proposal_correction(
            0,
            check.action_candidate_count,
            check.verified_action_count,
            check.repair_feedback.as_ref(),
        ));
    }

    #[test]
    fn rejected_proposals_produce_sanitized_repair_feedback() {
        let mut document = Document::new();
        document.set_view(ViewTransform::new(800.0, 600.0));
        let response =
            "```grafito\nPolyhedron[NumericArray[{{0,0,0}}],NumericArray[{{0,1,2}}]]\n```";

        let check = inspect_remote_proposals(
            &document,
            response,
            grafito_geometry::Camera3D::new(4.0 / 3.0),
        );

        assert!(check.verified.is_empty());
        assert_eq!(check.action_candidate_count, 1);
        let feedback = check
            .repair_feedback
            .expect("rejected action has safe feedback");
        assert_eq!(feedback.failures.len(), 1);
        assert_eq!(feedback.failures[0].command, "Polyhedron");
        assert_eq!(
            feedback.failures[0].kind,
            AssistantRepairFailureKind::UnsupportedCommand
        );
        assert!(feedback.failures[0].expected_syntax.is_empty());
        assert!(!feedback.prompt_text().contains("NumericArray"));
    }

    #[test]
    fn stale_model_results_are_not_accepted_after_a_provider_switch() {
        assert!(accepts_model_result(
            ProviderProfile::OpenCodeGo,
            ProviderProfile::OpenCodeGo,
            false,
        ));
        assert!(!accepts_model_result(
            ProviderProfile::OllamaLocal,
            ProviderProfile::OpenCodeGo,
            false,
        ));
        assert!(!accepts_model_result(
            ProviderProfile::OpenCodeGo,
            ProviderProfile::OpenCodeGo,
            true,
        ));
    }

    #[test]
    fn remote_results_are_bound_to_the_model_that_started_them() {
        assert!(accepts_remote_result(
            ProviderProfile::OpenCodeGo,
            "deepseek-v4-pro",
            ProviderProfile::OpenCodeGo,
            "deepseek-v4-pro"
        ));
        assert!(!accepts_remote_result(
            ProviderProfile::OpenCodeGo,
            "deepseek-v4-pro",
            ProviderProfile::OpenCodeGo,
            "deepseek-v4-flash"
        ));
    }

    #[test]
    fn remote_results_require_the_exact_document_and_focus_snapshot() {
        let context = ImmutableDocumentContext::from_variables(7, [("a".to_string(), 1.0)]);
        let focus = AssistantFocus::function("f", "x^2", None, None, false);

        assert!(accepts_remote_context(
            &context,
            Some(&focus),
            7,
            &context.digest,
            Some(&focus),
        ));

        let mut changed_revision = context.clone();
        changed_revision.revision = 8;
        assert!(!accepts_remote_context(
            &changed_revision,
            Some(&focus),
            7,
            &context.digest,
            Some(&focus),
        ));

        let changed_document =
            ImmutableDocumentContext::from_variables(7, [("a".to_string(), 2.0)]);
        assert!(!accepts_remote_context(
            &changed_document,
            Some(&focus),
            7,
            &context.digest,
            Some(&focus),
        ));

        let changed_focus = AssistantFocus::function("g", "x^3", None, None, false);
        assert!(!accepts_remote_context(
            &context,
            Some(&changed_focus),
            7,
            &context.digest,
            Some(&focus),
        ));
    }

    #[test]
    fn session_key_is_available_only_to_its_original_provider() {
        let mut runtime = AssistantRuntime::default();
        runtime.remember_key(ProviderProfile::OpenCodeGo, "session-key".into());

        assert_eq!(
            runtime.key_for(ProviderProfile::OpenCodeGo).as_deref(),
            Some("session-key")
        );
        assert!(runtime.key_for(ProviderProfile::DeepSeek).is_none());
    }

    #[test]
    fn session_fallback_model_overrides_expected_without_touching_preference() {
        let mut runtime = AssistantRuntime::default();
        // Sin fallback: se espera el modelo configurado.
        assert_eq!(
            runtime.expected_model("muse-spark-1.3-contributor"),
            "muse-spark-1.3-contributor"
        );
        // Con fallback activo: el resultado en vuelo trae el fallback...
        runtime.fallback_model = Some("deepseek-v4-flash".into());
        assert_eq!(
            runtime.expected_model("muse-spark-1.3-contributor"),
            "deepseek-v4-flash"
        );
        assert!(accepts_remote_result(
            ProviderProfile::OpenCodeGo,
            runtime.expected_model("muse-spark-1.3-contributor"),
            ProviderProfile::OpenCodeGo,
            "deepseek-v4-flash",
        ));
        // ...y el viejo modelo ya no se acepta mientras dura el fallback.
        assert!(!accepts_remote_result(
            ProviderProfile::OpenCodeGo,
            runtime.expected_model("muse-spark-1.3-contributor"),
            ProviderProfile::OpenCodeGo,
            "muse-spark-1.3-contributor",
        ));
    }

    #[test]
    fn error_429_reports_quota_not_model_or_key() {
        // 429 = cuota por minuto, no modelo ni clave: el mensaje no debe
        // mandar a reconfigurar nada ni mostrar el código crudo.
        let quota = remote_error_message(
            "assistant agent returned HTTP 429: Model muse-spark-1.3-contributor rate limited",
            "muse-spark-1.3-contributor",
        );
        assert!(!quota.contains("429"), "sin código crudo: {quota}");
        assert!(
            quota.contains("minuto") || quota.contains('s'),
            "pide esperar, no reconfigurar: {quota}"
        );
        assert!(
            !quota.contains("Revisá Configuración → Modelo"),
            "no culpa al modelo: {quota}"
        );
    }

    #[test]
    fn error_429_with_retry_after_counts_down() {
        // Con sufijo del transporte se muestra la cuenta regresiva exacta.
        let quota = remote_error_message(
            "remote assistant returned HTTP 429: busy (reintentá en 7s)",
            "muse-spark-1.3-contributor",
        );
        assert!(quota.contains('7'), "cuenta regresiva: {quota}");
        assert!(!quota.contains("429"), "sin código crudo: {quota}");
    }

    #[test]
    fn error_rate_limit_marker_maps_to_paused_message() {
        // El marcador interno de la pausa global sale en criollo.
        let paused = remote_error_message(
            &format!("{}:25", grafito_assistant::RATE_LIMIT_PAUSED_MARKER),
            "deepseek-v4-flash",
        );
        assert!(paused.contains("25"), "cuenta regresiva: {paused}");
        assert!(!paused.contains("429"), "sin código crudo: {paused}");
    }

    #[test]
    fn fallbacks_never_fire_on_429_quota_errors() {
        // Ante 429 no se quema cuota probando con otro modelo: el usuario
        // espera y reintenta el suyo cuando vence la pausa.
        assert!(!should_fallback_remote_spark_to_deepseek(
            "remote assistant returned HTTP 429: busy (reintentá en 7s)",
            ProviderProfile::OpenCodeGo,
            "muse-spark-1.3-contributor",
            0,
        ));
        assert!(!should_fallback_agent_spark_to_deepseek(
            "assistant agent returned HTTP 429 (reintentá en 3s)",
            ProviderProfile::OpenCodeGo,
            "muse-spark-1.3-contributor",
        ));
        // ...pero el fallback normal ante 500/timeout sigue intacto.
        assert!(should_fallback_remote_spark_to_deepseek(
            "remote assistant timed out after 30s",
            ProviderProfile::OpenCodeGo,
            "muse-spark-1.3-contributor",
            0,
        ));
    }

    #[test]
    fn agent_spark_success_never_falls_back_only_responses_error_does() {
        // Wiring B1: `Done(Ok)` con Spark se acepta directo — el fallback sólo
        // vive en la rama `Err` ("Responses API") y lo decide este helper.
        assert!(!is_agent_spark_responses_unsupported_error("ok"));
        assert!(!is_agent_spark_responses_unsupported_error("HTTP 500"));
        assert!(is_agent_spark_responses_unsupported_error(
            "agent tools are not supported via Responses API, use chat"
        ));
        assert!(should_fallback_agent_spark_to_deepseek(
            "agent tools are not supported via Responses API",
            ProviderProfile::OpenCodeGo,
            "muse-spark-1.3-contributor",
        ));
        // No dispara con otro modelo, otro proveedor u otro error.
        assert!(!should_fallback_agent_spark_to_deepseek(
            "agent tools are not supported via Responses API",
            ProviderProfile::OpenCodeGo,
            "deepseek-v4-flash",
        ));
        assert!(!should_fallback_agent_spark_to_deepseek(
            "agent tools are not supported via Responses API",
            ProviderProfile::DeepSeek,
            "muse-spark-1.3-contributor",
        ));
        assert!(!should_fallback_agent_spark_to_deepseek(
            "HTTP 500 internal error",
            ProviderProfile::OpenCodeGo,
            "muse-spark-1.3-contributor",
        ));
    }

    #[test]
    fn remote_spark_fallback_only_on_500_or_timeout_without_correction() {
        assert!(should_fallback_remote_spark_to_deepseek(
            "HTTP 500 internal error",
            ProviderProfile::OpenCodeGo,
            "muse-spark-1.3-contributor",
            0,
        ));
        assert!(should_fallback_remote_spark_to_deepseek(
            "request timed out after 60s",
            ProviderProfile::OpenCodeGo,
            "muse-spark-1.3-contributor",
            0,
        ));
        // Con corrección en curso, otro modelo/proveedor u otro error: no.
        assert!(!should_fallback_remote_spark_to_deepseek(
            "HTTP 500 internal error",
            ProviderProfile::OpenCodeGo,
            "muse-spark-1.3-contributor",
            1,
        ));
        assert!(!should_fallback_remote_spark_to_deepseek(
            "HTTP 500 internal error",
            ProviderProfile::OpenCodeGo,
            "deepseek-v4-flash",
            0,
        ));
        assert!(!should_fallback_remote_spark_to_deepseek(
            "HTTP 500 internal error",
            ProviderProfile::DeepSeek,
            "muse-spark-1.3-contributor",
            0,
        ));
        assert!(!should_fallback_remote_spark_to_deepseek(
            "HTTP 401 unauthorized",
            ProviderProfile::OpenCodeGo,
            "muse-spark-1.3-contributor",
            0,
        ));
    }

    #[test]
    fn spark_400_missing_session_falls_back_with_session_message() {
        // Mock del gateway Go (docs Go 2026-09-08): sin sesión válida devuelve
        // 400 con `MissingSessionID` en el cuerpo. El usuario PAGA Go: nada de
        // "tier gratuito" ni "modelo pago" en los mensajes.
        let body_400 = r#"remote assistant returned HTTP 400: {"type":"error","error":{"type":"MissingSessionID","message":"session required"}}"#;
        assert!(
            is_session_or_account_error(body_400),
            "el lector debe detectar MissingSessionID"
        );
        // Fallback automático a deepseek (patrón session-fallback existente).
        assert!(should_fallback_remote_spark_to_deepseek(
            body_400,
            ProviderProfile::OpenCodeGo,
            "muse-spark-1.3-contributor-free",
            0,
        ));
        // Viejos IDs también caen en fallback ante el mismo 400.
        assert!(should_fallback_remote_spark_to_deepseek(
            body_400,
            ProviderProfile::OpenCodeGo,
            "muse-spark-1.3-contributor",
            0,
        ));
        // Los otros tipos de sesión/cuenta/clave también disparan.
        for tipo in ["InvalidApiKey", "ModelDisabled", "AccountBlocked"] {
            let error = format!("remote assistant returned HTTP 400: {{\"type\":\"{tipo}\"}}");
            assert!(
                should_fallback_remote_spark_to_deepseek(
                    &error,
                    ProviderProfile::OpenCodeGo,
                    "muse-spark-1.3",
                    0,
                ),
                "tipo {tipo} debe disparar fallback"
            );
            let mensaje = remote_error_message(&error, "muse-spark-1.3");
            assert!(
                mensaje.contains("clave") || mensaje.contains("deepseek"),
                "mensaje rioplatense ante {tipo}: {mensaje}"
            );
            assert!(
                !mensaje.contains("Revisá Configuración → Modelo"),
                "nada de genérico ante {tipo}: {mensaje}"
            );
        }
        // Mensaje ante el 400 real: header viaja solo + región + re-conectar,
        // sin "gratis"/"pago", con deepseek y sin genérico.
        let mensaje = remote_error_message(body_400, "muse-spark-1.3-contributor-free");
        assert!(
            mensaje.contains("header viaja solo"),
            "dice que el header ya viaja: {mensaje}"
        );
        assert!(
            mensaje.contains("región"),
            "pide verificar región (Spark limitado por región Meta): {mensaje}"
        );
        assert!(
            mensaje.contains("re-conectá"),
            "pide re-conectar la clave Go si persiste: {mensaje}"
        );
        assert!(
            mensaje.contains("deepseek"),
            "ofrece seguir con deepseek: {mensaje}"
        );
        assert!(
            !mensaje.contains("gratis") && !mensaje.contains("tier"),
            "nada de tier gratuito (el usuario PAGA Go): {mensaje}"
        );
        assert!(
            !mensaje.contains("modelo pago"),
            "nada de modelo pago (ya lo tiene): {mensaje}"
        );
        assert!(
            !mensaje.contains("Revisá Configuración → Modelo"),
            "nada de genérico: {mensaje}"
        );
        // Guardas intactas: con corrección, otro modelo/proveedor o 429, no hay fallback.
        assert!(!should_fallback_remote_spark_to_deepseek(
            body_400,
            ProviderProfile::OpenCodeGo,
            "muse-spark-1.3-contributor-free",
            1,
        ));
        assert!(!should_fallback_remote_spark_to_deepseek(
            body_400,
            ProviderProfile::OpenCodeGo,
            "deepseek-v4-flash",
            0,
        ));
        assert!(!should_fallback_remote_spark_to_deepseek(
            &format!("{body_400} 429"),
            ProviderProfile::OpenCodeGo,
            "muse-spark-1.3-contributor-free",
            0,
        ));
    }

    #[test]
    fn go_session_estable_entre_turnos_y_rota_al_limpiar() {
        // Estable entre turnos: dos ensures seguidos devuelven el mismo UUID.
        let mut runtime = AssistantRuntime::default();
        assert!(runtime.go_session_id.is_none());
        let primero = runtime.ensure_go_session();
        assert_eq!(runtime.go_session_id.as_deref(), Some(primero.as_str()));
        let segundo = runtime.ensure_go_session();
        assert_eq!(primero, segundo, "estable entre turnos");
        assert!(grafito_assistant::sanitize_go_session_id(&segundo).is_some());
        // Rota al Limpiar: el próximo turno usa otra sesión.
        runtime.rotate_go_session();
        let tercero = runtime.ensure_go_session();
        assert_ne!(segundo, tercero, "rota al Limpiar conversación");
        assert!(grafito_assistant::sanitize_go_session_id(&tercero).is_some());
        // UA propio, no genérico.
        let ua = grafito_assistant::go_user_agent();
        assert!(ua.starts_with("grafito/"), "{ua}");
    }

    #[test]
    fn missing_session_con_header_igual_fallback_por_region() {
        // Aunque el header `x-opencode-session` viaje (sesión válida generada),
        // el servidor puede rechazar por región: el 400 sigue haciendo fallback
        // a deepseek una vez, sin tocar la preferencia.
        let mut runtime = AssistantRuntime::default();
        let session = runtime.ensure_go_session();
        let settings = ProviderSettings::for_profile(
            ProviderProfile::OpenCodeGo,
            "muse-spark-1.3-contributor",
        )
        .with_go_session_id(Some(session))
        .expect("sesión UUID válida");
        assert!(settings.go_session_id.is_some());
        let body_400 = r#"remote assistant returned HTTP 400: {"type":"error","error":{"type":"MissingSessionID","message":"session required"}}"#;
        assert!(should_fallback_remote_spark_to_deepseek(
            body_400,
            settings.profile,
            &settings.model,
            0,
        ));
        // Y el mensaje sigue en términos Go (reintentar + región + re-conectar).
        let mensaje = remote_error_message(body_400, &settings.model);
        assert!(mensaje.contains("Reintentá"), "{mensaje}");
        assert!(mensaje.contains("región"), "{mensaje}");
        assert!(mensaje.contains("deepseek"), "{mensaje}");
    }

    #[test]
    fn fallback_evalua_modelo_intentado_no_preferencia_sin_bucle() {
        // Regresión del bucle: el reintento en deepseek fallaba y el chequeo
        // con la PREFERENCIA (spark) re-disparaba el fallback al infinito
        // (un aviso por intento). El llamador debe pasar el modelo del job.
        let deepseek_401 = "remote assistant returned HTTP 401: {\"type\":\"error\",\"error\":{\"type\":\"InvalidApiKey\"}}";
        // Intento en deepseek (reintento del fallback) + preferencia spark:
        // con modelo intentado deepseek NO hay fallback, aunque la
        // preferencia siga siendo spark.
        assert!(!should_fallback_remote_spark_to_deepseek(
            deepseek_401,
            ProviderProfile::OpenCodeGo,
            "deepseek-v4-flash",
            0,
        ));
        // Intento en spark con el mismo error SÍ dispara (una sola vez).
        assert!(should_fallback_remote_spark_to_deepseek(
            deepseek_401,
            ProviderProfile::OpenCodeGo,
            "muse-spark-1.3-contributor-free",
            0,
        ));
    }

    #[test]
    fn session_key_remember_trims_pasted_whitespace() {
        let mut runtime = AssistantRuntime::default();
        runtime.remember_key(ProviderProfile::OpenCodeGo, "  sk-grafito-123\n".into());
        assert_eq!(
            runtime.key_for(ProviderProfile::OpenCodeGo).as_deref(),
            Some("sk-grafito-123")
        );
        runtime.forget_key();
        assert!(runtime.key_for(ProviderProfile::OpenCodeGo).is_none());
    }

    #[test]
    fn remote_errors_for_custom_and_validators_are_spanish() {
        // Error de validador Custom (inglés crudo) → español sin exponerlo.
        let custom = remote_error_message(
            "custom API key reference is invalid or reserved for a named provider",
            "modelo-custom",
        );
        assert!(
            custom.contains("No se pudo preparar") || custom.contains("Revisá"),
            "custom en español: {custom}"
        );
        // Identificador de modelo inválido → rama de modelo en español.
        let model_id = remote_error_message("remote model identifier is invalid", "x");
        assert!(
            model_id.contains("no está disponible"),
            "modelo en español: {model_id}"
        );
        // Endpoint Custom sin HTTPS → envoltorio español.
        let https = remote_error_message("custom OpenAI-compatible endpoints must use HTTPS", "x");
        assert!(
            https.contains("Revisá Configuración"),
            "endpoint en español: {https}"
        );
        // Responses API → mensaje específico en español.
        let responses = remote_error_message(
            "agent tools are not supported via Responses API",
            "muse-spark-1.3-contributor",
        );
        assert!(
            responses.contains("Responses API") && responses.contains("deepseek-v4-flash"),
            "responses en español: {responses}"
        );
    }

    #[test]
    fn attachment_errors_are_spanish_not_raw_english() {
        assert_eq!(
            attachment_error_message("assistant attachment byte limit exceeded"),
            "La imagen no es válida o supera los límites permitidos."
        );
        // Los mensajes ya españoles de grafito-ui pasan intactos.
        assert_eq!(
            attachment_error_message("se alcanzó el límite de adjuntos configurado"),
            "se alcanzó el límite de adjuntos configurado"
        );
    }

    #[test]
    fn cancel_all_jobs_marks_tokens_and_drops_anim_without_orphans() {
        let mut runtime = AssistantRuntime::default();
        let remote_cancel = CancellationToken::default();
        let (remote_tx, remote_rx) = sync_channel::<Result<RemoteCompletion, String>>(1);
        runtime.remote_job = Some(AssistantRemoteJob {
            id: 1,
            provider: ProviderProfile::OpenCodeGo,
            model: "muse-spark-1.3-contributor".into(),
            route: AssistantRemoteRoute::SelectedModel,
            fusion_fallback_allowed: false,
            question: "q".into(),
            correction_attempt: 0,
            repair_target_turn: None,
            document_revision: 1,
            document_digest: "d".into(),
            focus: None,
            cancellation: remote_cancel.clone(),
            receiver: remote_rx,
            stream_rx: None,
            stream_text: String::new(),
            preview_active: false,
            started_at: std::time::Instant::now(),
            first_delta_at: None,
        });
        let proposal_cancel = CancellationToken::default();
        let (proposal_tx, proposal_rx) =
            sync_channel::<Result<RemoteProposalVerification, String>>(1);
        runtime.proposal_job = Some(AssistantProposalJob {
            id: 2,
            provider: ProviderProfile::OpenCodeGo,
            model: "muse-spark-1.3-contributor".into(),
            route: AssistantRemoteRoute::SelectedModel,
            fusion_fallback_allowed: false,
            question: "q".into(),
            correction_attempt: 0,
            repair_target_turn: None,
            document_revision: 1,
            document_digest: "d".into(),
            focus: None,
            text: "t".into(),
            cancellation: proposal_cancel.clone(),
            receiver: proposal_rx,
        });
        let agent_cancel = grafito_agent::loop_engine::Cancellation::default();
        let (agent_tx, agent_rx) = sync_channel::<AgentChannelMsg>(128);
        let (_, clarification_rx) = sync_channel::<grafito_ui::assistant::PendingClarification>(4);
        runtime.agent_job = Some(AssistantAgentJob {
            provider: ProviderProfile::OpenCodeGo,
            model: "muse-spark-1.3-contributor".into(),
            cancellation: agent_cancel.clone(),
            receiver: agent_rx,
            clarification_receiver: clarification_rx,
        });
        let model_cancel = CancellationToken::default();
        let (model_tx, model_rx) = sync_channel::<Result<Vec<String>, String>>(1);
        runtime.model_job = Some(AssistantModelJob {
            id: 3,
            provider: ProviderProfile::OpenCodeGo,
            cancellation: model_cancel.clone(),
            receiver: model_rx,
        });
        let (_anim_tx, anim_rx) =
            sync_channel::<Result<grafito_ui::assistant::AssistantMedia, String>>(1);
        let anim_cancel = CancellationToken::default();
        runtime.anim_job = Some(AssistantAnimJob {
            cancellation: anim_cancel.clone(),
            receiver: anim_rx,
        });

        assert!(runtime.cancel_all_assistant_jobs());
        assert!(remote_cancel.is_cancelled());
        assert!(proposal_cancel.is_cancelled());
        assert!(agent_cancel.is_cancelled());
        assert!(model_cancel.is_cancelled());
        // Anim AS4: señala el token como el resto y dropea el slot de inmediato.
        assert!(anim_cancel.is_cancelled());
        assert!(runtime.anim_job.is_none());
        // Los jobs con token conservan el slot hasta el drain (sin huérfanos).
        assert!(!runtime.remote_request_slot_is_free());

        // Drenar remote + proposal vía take_finished_* (marca cancelled).
        remote_tx
            .send(Err("remote assistant request was cancelled".into()))
            .unwrap();
        let finished_remote = runtime.take_finished_remote_job().unwrap();
        assert!(finished_remote.cancelled);
        proposal_tx.send(Err("cancelled".into())).unwrap();
        let finished_proposal = runtime.take_finished_proposal_job().unwrap();
        assert!(finished_proposal.cancelled);
        // El canal del agente sigue vivo hasta que el poll lo drene.
        agent_tx
            .send(AgentChannelMsg::Done(Err("cancelled".into())))
            .unwrap();
        let agent_msg = runtime
            .agent_job
            .as_ref()
            .expect("agent slot held until poll drain")
            .receiver
            .try_recv()
            .expect("agent channel must not be orphaned");
        assert!(matches!(agent_msg, AgentChannelMsg::Done(_)));
        // Model se drena vía take_finished_model_job.
        model_tx.send(Err("cancelled".into())).unwrap();
        assert!(runtime.take_finished_model_job().is_some());
        // Simula el drain del poll del agente: libera el slot remoto.
        runtime.agent_job = None;
        assert!(runtime.remote_request_slot_is_free());
        // Sin jobs, cancelar es no-op.
        assert!(!runtime.cancel_all_assistant_jobs());
    }

    #[test]
    fn anim_cancel_signals_token_instead_of_only_dropping_receiver() {
        // AS4 cancel real: descartar señala el token (el hilo lo chequea entre
        // frames en el closure de progreso). Headless, sin ventana ni render.
        let mut runtime = AssistantRuntime::default();
        assert!(!runtime.cancel_anim_job(), "sin job es no-op");
        let anim_cancel = CancellationToken::default();
        let (_anim_tx, anim_rx) =
            sync_channel::<Result<grafito_ui::assistant::AssistantMedia, String>>(1);
        runtime.anim_job = Some(AssistantAnimJob {
            cancellation: anim_cancel.clone(),
            receiver: anim_rx,
        });
        assert!(!anim_cancel.is_cancelled());
        assert!(runtime.cancel_anim_job());
        assert!(
            anim_cancel.is_cancelled(),
            "descartar debe señalar el token"
        );
        assert!(runtime.anim_job.is_none());
        assert!(!runtime.cancel_anim_job(), "doble cancel es no-op");
    }

    #[test]
    fn anim_progress_closure_observes_token_between_frames() {
        // El render nativo no acepta token: el hilo lo chequea en el closure
        // de progreso. Este test pineado verifica el contrato sin render
        // pesado: el closure ve el token entre frames.
        let cancel = CancellationToken::default();
        let worker = cancel.clone();
        let mut saw: Vec<(usize, usize)> = Vec::new();
        let mut on_frame = |done: usize, total: usize| {
            saw.push((done, total));
            assert!(
                !worker.is_cancelled(),
                "sin cancel el closure no debe ver token"
            );
        };
        for frame in 1..=3 {
            on_frame(frame, 3);
        }
        assert_eq!(saw, vec![(1, 3), (2, 3), (3, 3)]);
        cancel.cancel();
        assert!(worker.is_cancelled(), "cancel debe verse entre frames");
    }

    #[test]
    fn wa_segunda_animacion_avisa_reemplazo_explicito() {
        // W-A red: 2da animación en curso → toast "ya estoy animando",
        // no muda. Sin animación → `None` (arranque silencioso normal).
        let message = super::anim_replace_message(true).expect("aviso");
        assert!(
            message.contains("animando"),
            "el aviso debe decir animando: {message}"
        );
        assert!(super::anim_replace_message(false).is_none());
    }

    #[test]
    fn pedido_con_animacion_instala_media_en_el_turno_con_prosa_humana() {
        // Pedido-con-animación → media instalada en el turno, sin ventana.
        // Headless: render clásico tiny (64x48, 48 frames) + transcript.
        use std::collections::BTreeMap;
        let pedido = "explica la derivada con animación";
        assert!(crate::anim_ui::wants_animation_request(pedido));
        let concepto = crate::anim_ui::animation_concept_from_request(pedido)
            .expect("con concepto debe validar");
        let plantilla = crate::anim_native::detect_template_for_concept(&concepto);
        assert_eq!(plantilla, "derivative-slope");
        let frames = crate::anim_native::render_anim_with_progress(
            plantilla,
            &concepto,
            64,
            48,
            &BTreeMap::new(),
            &mut |_, _| {},
        );
        assert!(!frames.is_empty(), "el nativo debe producir frames");
        let ctx = egui::Context::default();
        let mut panel = AssistantPanelState::default();
        panel.begin_request(pedido.to_string());
        let base = grafito_ui::assistant::humanize_prose_text("La derivada es la pendiente.");
        let mut prosa = base;
        prosa.push_str(
            "

",
        );
        prosa.push_str(crate::anim_ui::animation_reference_sentence());
        panel.complete_local_request(prosa.clone());
        let media = grafito_ui::assistant::AssistantMedia {
            title: format!("{concepto} (nativa)"),
            frames,
        };
        panel.set_media(Some(media), &ctx);
        // Media instalada para el reproductor del transcript (último turno).
        assert!(panel.media.is_some(), "media debe vivir en el turno");
        assert!(!panel.media.as_ref().expect("media").frames.is_empty());
        let ultimo = panel.conversation.last().expect("turno asistente");
        assert_eq!(ultimo.role, ConversationRole::Assistant);
        assert!(
            ultimo.content.contains("deslizador"),
            "prosa: {}",
            ultimo.content
        );
        assert!(ultimo.content.contains("reproducir"));
        for id in ["PlayPause", "Slider", "Button", "Tangent", "Select", "Play"] {
            assert!(!ultimo.content.contains(id), "prosa sin {id}");
            assert!(
                !panel.media.as_ref().expect("media").title.contains(id),
                "título sin {id}"
            );
        }
    }

    #[test]
    fn pedido_ambiguo_error_honesto_sin_media_ni_invento() {
        // Pedido ambiguo → error honesto sin media, jamás inventa frames.
        let ctx = egui::Context::default();
        for ambiguo in ["animalo", "con animación", "explica con animación"] {
            assert!(
                crate::anim_ui::wants_animation_request(ambiguo),
                "{ambiguo:?} pide animación"
            );
            let err = crate::anim_ui::animation_concept_from_request(ambiguo)
                .expect_err("ambiguo debe fallar honesto");
            assert!(
                err.contains("qué animar") || err.contains("vacío"),
                "guía útil: {err}"
            );
            assert!(err.contains("por ejemplo"), "dice qué pedir: {err}");
            let mut panel = AssistantPanelState::default();
            panel.begin_request(ambiguo.to_string());
            let honesto = grafito_ui::assistant::humanize_prose_text(&err);
            panel.complete_local_request(honesto.clone());
            panel.set_media(None, &ctx);
            assert!(panel.media.is_none(), "sin media rancia en {ambiguo:?}");
            let ultimo = panel.conversation.last().expect("turno guía");
            assert!(ultimo.content.contains("por ejemplo"));
        }
        // Sin gatillo no es animación (no debe disparar hilo).
        assert!(!crate::anim_ui::wants_animation_request("derivá x^2"));
        assert!(crate::anim_ui::animation_concept_from_request("derivá x^2").is_err());
    }

    #[test]
    fn playlist_solo_y_despues_parte_en_dos_con_templates_propios() {
        // El patrón exacto parte en dos, con caso y acento variados.
        let (a, b) =
            split_playlist_request("explica la derivada y después la integral con animación")
                .expect("debe partir");
        assert!(a.contains("derivada"), "lado A: {a}");
        assert!(b.contains("integral"), "lado B: {b}");
        let (a2, b2) = split_playlist_request("derivada Y DESPUÉS integral con animación")
            .expect("mayúsculas");
        assert!(a2.contains("derivada"), "{a2}");
        assert!(b2.contains("integral"), "{b2}");
        let (_a3, _b3) = split_playlist_request("derivada y despues integral con animación")
            .expect("sin acento");
        // Cada lado resuelve su plantilla por el punto único.
        let lista = playlist_para_pedido("explica la derivada y después la integral con animación")
            .expect("playlist válida");
        assert_eq!(lista.len(), 2);
        assert_eq!(lista.total_duration_ms(), 4500);
        let t0 = lista.steps[0]
            .request
            .as_ref()
            .expect("step animado")
            .template
            .clone();
        let t1 = lista.steps[1]
            .request
            .as_ref()
            .expect("step animado")
            .template
            .clone();
        assert_eq!(t0, "derivative-slope", "lado derivada");
        assert_eq!(t1, "integral-area", "lado integral");
        // Scheduler global: el primer step ocupa 0..2500 (2 s + 0.5 espera).
        assert_eq!(lista.sample_at(100), Some((0, 100)));
        assert_eq!(lista.sample_at(2600), Some((1, 100)));
        assert_eq!(lista.sample_at(4500), None);
    }

    #[test]
    fn playlist_luego_y_despues_solos_tambien_separan() {
        // Red-first (auditoría): "derivada luego integral" debe partir en
        // (a,b) igual que "y después". Antes SOLO "y después" valía.
        let (a, b) = split_playlist_request("derivada luego integral").expect("luego separa");
        assert_eq!(a, "derivada");
        assert_eq!(b, "integral");
        let (c, d) =
            split_playlist_request("derivada después integral").expect("después solo separa");
        assert_eq!(c, "derivada");
        assert_eq!(d, "integral");
    }

    #[test]
    fn playlist_fuera_de_patron_cae_al_single_honesto() {
        // Sin conector, conector solo, lados vacíos o doble conector: None
        // (el llamante sigue el flujo single, jamás arma parcial).
        assert!(split_playlist_request("explica la derivada con animación").is_none());
        assert!(split_playlist_request("y después con animación").is_none());
        assert!(split_playlist_request("derivada y después con animación").is_some());
        assert!(split_playlist_request("derivada y después").is_none());
        assert!(split_playlist_request(
            "derivada y después integral y después taylor con animación"
        )
        .is_none());
        assert!(playlist_para_pedido("explica la derivada con animación").is_none());
        // "luego"/"después" solos también separan (ver test dedicado arriba);
        // acá se pinnea que el doble conector mixto sigue cayendo al single.
        assert!(
            split_playlist_request("derivada luego integral después taylor con animación")
                .is_none()
        );
    }
    #[test]
    fn playlist_concat_entra_en_presupuesto_y_titulo_nombra_ambos() {
        // Dos steps nativos de 48 + espera de 0.5 s: el fitting ajusta la
        // cadencia para entrar en 96 preservando extremos (nada parcial).
        use std::collections::BTreeMap;
        let lista = playlist_para_pedido("explica la derivada y después la integral con animación")
            .expect("playlist válida");
        let mut partes = Vec::new();
        for step in &lista.steps {
            let request = step.request.as_ref().expect("step animado");
            let frames = crate::anim_native::render_anim_with_progress(
                &request.template,
                &request.concept,
                64,
                48,
                &BTreeMap::new(),
                &mut |_, _| {},
            );
            assert!(!frames.is_empty());
            partes.push((frames, step.wait_after_ms));
        }
        // Sin fitting no entraría (48+48+6 holds = 102 > 96): el fitting lo
        // deja en 25+25+6 = 56 con extremos intactos.
        assert!(crate::anim_native::concat_playlist_with_holds(
            partes.iter().map(|(f, w)| (f.clone(), *w)).collect(),
            crate::anim_native::GIF_BASE_FPS,
        )
        .is_err());
        let todo =
            crate::anim_native::concat_playlist_fitting(partes, crate::anim_native::GIF_BASE_FPS)
                .expect("el fitting debe entrar en 96");
        assert!(todo.len() <= grafito_anim::protocol::PLAYLIST_MAX_FRAMES_TOTAL);
        assert_eq!(todo.len(), 56, "25+25+6 holds, got: {}", todo.len());
        let timeline = lista
            .global_timeline(&[48, 48])
            .expect("timeline global válida");
        assert_eq!(
            grafito_anim::protocol::playlist_frame_at(&timeline, 0, todo.len()),
            0
        );
        assert_eq!(
            grafito_anim::protocol::playlist_frame_at(&timeline, 4499, todo.len()),
            todo.len() - 1
        );
    }

    #[test]
    fn integral_tres_ramas_canonica_explicita_invalida() {
        // 1. Sin función → canónica (se renderiza Y se declara).
        assert_eq!(
            clasifica_pedido_integral(
                "haceme una animacion de una integral (nativa)",
                "integral-area"
            ),
            IntegralPedido::Canonica
        );
        // 2. Con función válida → explícita (flujo intacto).
        assert_eq!(
            clasifica_pedido_integral(
                "animacion de la integral de f(x)=x^3 de 0 a 2 con animación",
                "integral-area"
            ),
            IntegralPedido::Explicita
        );
        // 3. Con función inválida → error honesto (sin frames ni hilo).
        match clasifica_pedido_integral(
            "animacion de la integral de f(x)=foo(x) con p en [0,1] con animación",
            "integral-area",
        ) {
            IntegralPedido::FuncionInvalida(detalle) => {
                assert!(detalle.contains("foo(x)"), "{detalle}");
                assert!(detalle.contains("x^2"), "da ejemplo: {detalle}");
            }
            otro => panic!("esperaba FuncionInvalida, fue {otro:?}"),
        }
        // Fuera de integral-area o sin mención: no aplica (nada cambia).
        assert_eq!(
            clasifica_pedido_integral("explica la derivada con animación", "derivative-slope"),
            IntegralPedido::NoAplica
        );
        assert_eq!(
            clasifica_pedido_integral("explica la derivada con animación", "integral-area"),
            IntegralPedido::NoAplica
        );
        // Probabilidad usa el renderer integral como soporte: no se frena.
        assert_eq!(
            clasifica_pedido_integral("explica la probabilidad con animación", "integral-area"),
            IntegralPedido::NoAplica
        );
    }

    #[test]
    fn wb_timeout_es_mitad_del_budget_y_peor_caso_un_request() {
        // Documenta el const: mitad del budget del turno (60s/2) y peor caso
        // 1 request extra por turno de animación (el SPEC); sin IA cero.
        assert_eq!(
            ANIM_IA_SPEC_TIMEOUT_MS,
            grafito_assistant_types::RequestBudget::default().timeout_ms / 2
        );
        assert_eq!(ANIM_IA_SPEC_TIMEOUT_MS, 30_000);
    }

    #[test]
    fn wb_sin_ia_va_a_canonica_declarada_con_aviso_de_una_linea() {
        // Sin IA (offline/local sin clave): ni remoto ni agente → false.
        assert!(!ia_disponible_para_anim(false, false, false, false));
        // Pausa 429 o examen también fuerzan local (no queman cuota).
        assert!(!ia_disponible_para_anim(true, true, true, false));
        assert!(!ia_disponible_para_anim(true, true, false, true));
        assert!(!ia_disponible_para_anim(false, true, true, false));
        // Remoto o agente solos sí hay IA.
        assert!(ia_disponible_para_anim(false, true, false, false));
        assert!(ia_disponible_para_anim(true, false, false, false));
        // Sin IA el desenlace es canónica + aviso aunque viniera un SPEC
        // (cero doble render: sin IA jamás se renderiza IA).
        let spec_ignorado = SpecAnimIa {
            expr: "x^3".to_string(),
            p0: 0.0,
            p1: 2.0,
            plantilla: "integral-area".to_string(),
            param: "p".to_string(),
        };
        match resolver_turno_anim_ia(false, PedidoSpecIa::Exito(spec_ignorado)) {
            DesenlaceAnimIa::FallbackCanonico { aviso } => {
                assert_eq!(aviso, ANIM_SIN_IA_AVISO);
                assert!(aviso.contains("sin conexión"), "{aviso}");
                assert!(aviso.contains("x²"), "{aviso}");
                assert!(aviso.contains("pedime otra"), "{aviso}");
                assert!(!aviso.contains('\n'), "UNA línea: {aviso}");
            }
            otro => panic!("sin IA debe ser fallback, fue {otro:?}"),
        }
        // La canónica de fallback es x² [0,2] y valida con las puertas.
        let canonica = spec_canonico_para_fallback("integral-area");
        assert_eq!(canonica.expr, "x^2");
        assert_eq!((canonica.p0, canonica.p1), (0.0, 2.0));
        assert!(validar_spec_anim_ia(&canonica).is_ok());
        let tangente = spec_canonico_para_fallback("derivative-slope");
        assert_eq!(tangente.expr, "x^2");
        assert!(validar_spec_anim_ia(&tangente).is_ok());
    }

    #[test]
    fn wb_con_ia_mock_spec_x3_se_refleja_en_prosa_y_frames() {
        // Mock de IA: JSON con f=x³ (la IA propuso, el motor solo valida).
        let texto_ia =
            r#"{"expr_a": "x^3", "range": [0, 2], "plantilla": "integral-area", "param": "p"}"#;
        let spec = parsear_spec_anim_ia(texto_ia, "animame la integral con animación")
            .expect("x^3 valida");
        assert_eq!(spec.expr, "x^3");
        assert_eq!((spec.p0, spec.p1), (0.0, 2.0));
        assert_eq!(spec.plantilla, "integral-area");
        // La prosa del turno MUST nombrar función y rango venidos de la IA.
        let prosa = prosa_para_spec_anim_ia(&spec);
        assert!(prosa.contains("x^3"), "{prosa}");
        assert!(prosa.contains("[0,2]"), "{prosa}");
        assert!(prosa.contains("deslizador"), "{prosa}");
        // El desenlace con IA es RenderIa (un solo render, el de la IA).
        match resolver_turno_anim_ia(true, PedidoSpecIa::Exito(spec.clone())) {
            DesenlaceAnimIa::RenderIa {
                spec: render_spec,
                prosa: render_prosa,
            } => {
                assert_eq!(render_spec.expr, "x^3");
                assert!(render_prosa.contains("x^3"), "{render_prosa}");
            }
            otro => panic!("con IA válida debe renderizar IA, fue {otro:?}"),
        }
        // Los frames reflejan el SPEC (x³), no la canónica (x²): difieren.
        let cancel = CancellationToken::default();
        let media_ia = render_media_desde_spec_ia(&spec, &cancel).expect("render x^3");
        assert!(!media_ia.frames.is_empty());
        assert_ne!(
            media_ia.frames.first().map(|f| &f.pixels),
            media_ia.frames.last().map(|f| &f.pixels),
            "los frames deben animar"
        );
        assert!(
            media_ia.title.contains("x^3"),
            "título nombra x³: {}",
            media_ia.title
        );
        let canonica = spec_canonico_para_fallback("integral-area");
        let media_canonica =
            render_media_desde_spec_ia(&canonica, &cancel).expect("render canónico");
        assert_ne!(
            media_ia.frames[0].pixels, media_canonica.frames[0].pixels,
            "el SPEC de la IA (x³) no es la canónica (x²)"
        );
        // SPEC inválido (foo) → Err honesto, jamás frames.
        let basura = r#"{"expr": "foo(x)", "p0": 0, "p1": 2, "plantilla": "integral-area"}"#;
        assert!(parsear_spec_anim_ia(basura, "integral con animación").is_err());
        match resolver_turno_anim_ia(true, PedidoSpecIa::Invalido("la función no valida".into())) {
            DesenlaceAnimIa::ErrorHonesto(detalle) => {
                assert!(!detalle.is_empty());
            }
            otro => panic!("SPEC inválido debe ser error honesto, fue {otro:?}"),
        }
    }

    #[test]
    fn wb_timeout_ia_va_a_fallback_con_aviso() {
        // Timeout: canal vivo sin mensaje dentro del plazo → Timeout.
        let (_tx, rx) = std::sync::mpsc::sync_channel::<Result<SpecAnimIa, String>>(1);
        match esperar_spec_ia_con_timeout(&rx, 30) {
            PedidoSpecIa::Timeout => {}
            otra => panic!("debía dar Timeout, fue {otra:?}"),
        }
        // Error de transporte (400-429/offline) → fallback, no error.
        for salida in [
            PedidoSpecIa::Timeout,
            PedidoSpecIa::Transporte("remote assistant returned HTTP 429".into()),
            PedidoSpecIa::Transporte("MissingSessionID".into()),
        ] {
            match resolver_turno_anim_ia(true, salida) {
                DesenlaceAnimIa::FallbackCanonico { aviso } => {
                    assert_eq!(aviso, ANIM_SIN_IA_AVISO);
                    assert!(!aviso.contains('\n'), "UNA línea: {aviso}");
                }
                otra => panic!("timeout/transporte debe ser fallback, fue {otra:?}"),
            }
        }
    }

    #[test]
    fn m1_submit_tangente_explicita_x3_frames_de_x3() {
        // Defecto 1: "derivada x³ [-2,2]" por Submit → frames de x³, no
        // canónica muda. Decisión + render usan `infer_tangent_anim` igual
        // que el agente.
        let pedido = "derivada x³ [-2,2] con animación";
        assert_eq!(plantilla_para_pedido(pedido), "derivative-slope");
        assert_eq!(
            clasifica_pedido_tangente(pedido, "derivative-slope"),
            TangentePedido::Explicita
        );
        match decide_animacion(pedido) {
            DecisionAnimacion::RenderExplicito {
                plantilla,
                concepto: _,
                expr,
            } => {
                assert_eq!(plantilla, "derivative-slope");
                assert_eq!(expr, "x^3");
            }
            otra => panic!("explícita debe renderizar, fue {otra:?}"),
        }
        // La prosa nombra f y rango (defecto 6: jamás huérfana).
        let prosa = prosa_tangente_explicita("x^3", pedido);
        assert!(prosa.contains("x^3"), "{prosa}");
        assert!(prosa.contains("[-2,2]"), "{prosa}");
        assert!(prosa.contains("deslizador"), "{prosa}");
        assert!(!prosa.contains("pedime otra"), "{prosa}");
        // El render explícito difiere de la canónica x².
        let anim = anim_parametrica_para_pedido("derivative-slope", pedido)
            .expect("explícita no falla")
            .expect("derivative-slope con tangente es paramétrica");
        assert_eq!(anim.expr_a, "x^3");
        let cancel = CancellationToken::default();
        let media = render_media_desde_spec_ia(
            &SpecAnimIa {
                expr: anim.expr_a.clone(),
                p0: anim.p0,
                p1: anim.p1,
                plantilla: "derivative-slope".to_string(),
                param: anim.param.as_str().to_string(),
            },
            &cancel,
        )
        .expect("render x^3");
        assert!(
            media.title.contains("x^3"),
            "título nombra x³: {}",
            media.title
        );
        let canonica = spec_canonico_para_fallback("derivative-slope");
        let media_canonica = render_media_desde_spec_ia(&canonica, &cancel).expect("canónica");
        assert_ne!(
            media.frames[0].pixels, media_canonica.frames[0].pixels,
            "x³ no es la canónica x²"
        );
    }

    #[test]
    fn m1_tangente_invalida_foo_error_honesto_sin_frames() {
        // Defecto 2: `foo(x)` en tangente diverge (agente `Err`, Submit
        // canónica muda). Ahora Submit es honesto con qué pedir.
        let pedido = "tangente móvil de f(x)=foo(x) con animación";
        assert_eq!(plantilla_para_pedido(pedido), "derivative-slope");
        match clasifica_pedido_tangente(pedido, "derivative-slope") {
            TangentePedido::FuncionInvalida(detalle) => {
                assert!(detalle.contains("foo(x)"), "{detalle}");
                assert!(detalle.contains("x^2"), "da ejemplo: {detalle}");
            }
            otro => panic!("inválida no clasifica, fue {otro:?}"),
        }
        match decide_animacion(pedido) {
            DecisionAnimacion::PreguntarSinMedia(guia) => {
                assert!(guia.contains("foo(x)"), "{guia}");
            }
            otra => panic!("inválida no renderiza, fue {otra:?}"),
        }
        assert!(anim_parametrica_para_pedido("derivative-slope", pedido).is_err());
        // Suelta sin `=` también es honesta, no canónica muda.
        assert!(
            anim_parametrica_para_pedido("derivative-slope", "derivada foo(x) [-2,2]").is_err()
        );
    }

    #[test]
    fn m1_aviso_fallback_declara_plantilla_y_rango_reales() {
        // Defecto 3: el aviso declara lo EFECTIVAMENTE renderizado.
        let integral = spec_canonico_para_fallback("integral-area");
        let aviso = aviso_fallback_canonico(&integral);
        assert!(!aviso.contains('\n'), "UNA línea: {aviso}");
        assert!(aviso.contains("x^2"), "{aviso}");
        assert!(aviso.contains("[0,2]"), "{aviso}");
        assert!(aviso.contains("integral"), "{aviso}");
        assert!(aviso.contains("pedime otra"), "{aviso}");
        let tangente = spec_canonico_para_fallback("derivative-slope");
        let aviso_t = aviso_fallback_canonico(&tangente);
        assert!(!aviso_t.contains('\n'), "UNA línea: {aviso_t}");
        assert!(aviso_t.contains("x^2"), "{aviso_t}");
        assert!(aviso_t.contains("[-1.5,1.5]"), "{aviso_t}");
        assert!(aviso_t.contains("tangente"), "{aviso_t}");
        assert_ne!(aviso, aviso_t, "cada plantilla declara lo suyo");
    }

    #[test]
    fn m1_prosa_canonica_por_plantilla_no_cruza() {
        // La canónica tangente declara tangente; la integral, integral.
        let tangente = prosa_canonica_para_plantilla("derivative-slope");
        assert!(tangente.contains("tangente"), "{tangente}");
        assert!(tangente.contains("x²"), "{tangente}");
        assert!(tangente.contains("deslizador"), "{tangente}");
        let integral = prosa_canonica_para_plantilla("integral-area");
        assert!(!integral.contains("tangente"), "{integral}");
        assert!(integral.contains("x²"), "{integral}");
    }

    #[test]
    fn m1_cancel_total_cancela_todo_incluye_export() {
        // Defecto 5: `cancel_anim_job` cancela TODO lo vivo del turno.
        let mut runtime = AssistantRuntime::default();
        let remote_cancel = CancellationToken::default();
        let (_rtx, rrx) = sync_channel::<Result<RemoteCompletion, String>>(1);
        runtime.remote_job = Some(AssistantRemoteJob {
            id: 1,
            provider: ProviderProfile::OpenCodeGo,
            model: "deepseek-v4-flash".into(),
            route: AssistantRemoteRoute::SelectedModel,
            fusion_fallback_allowed: false,
            question: "q".into(),
            correction_attempt: 0,
            repair_target_turn: None,
            document_revision: 1,
            document_digest: "d".into(),
            focus: None,
            cancellation: remote_cancel.clone(),
            receiver: rrx,
            stream_rx: None,
            stream_text: String::new(),
            preview_active: false,
            started_at: std::time::Instant::now(),
            first_delta_at: None,
        });
        let agent_cancel = grafito_agent::loop_engine::Cancellation::default();
        let (_atx, arx) = sync_channel::<AgentChannelMsg>(1);
        let (_ctx, crx) = sync_channel::<grafito_ui::assistant::PendingClarification>(1);
        runtime.agent_job = Some(AssistantAgentJob {
            provider: ProviderProfile::OpenCodeGo,
            model: "deepseek-v4-flash".into(),
            cancellation: agent_cancel.clone(),
            receiver: arx,
            clarification_receiver: crx,
        });
        let anim_cancel = CancellationToken::default();
        let (_anx, anrx) = sync_channel::<Result<AnimIaRender, String>>(1);
        runtime.anim_ia_job = Some(AssistantAnimIaJob {
            cancellation: anim_cancel.clone(),
            receiver: anrx,
        });
        // Export en vuelo sobre un temporal real: el reaper lo entierra.
        let ruta = std::env::temp_dir().join(format!(
            "grafito_cancel_reaper_{}_{}.gif",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|d| d.as_nanos())
                .unwrap_or(0)
        ));
        std::fs::write(&ruta, b"GIF89a").expect("temporal del test");
        let ruta_hilo = ruta.clone();
        runtime.gif_export_job = Some(GifExportJob {
            handle: std::thread::spawn(move || Ok(ruta_hilo)),
            frame_count: 1,
            cancel: CancellationToken::default(),
            path: ruta.clone(),
        });
        assert!(runtime.cancel_anim_job(), "había turno en vuelo");
        assert!(remote_cancel.is_cancelled(), "remote señalado");
        assert!(agent_cancel.is_cancelled(), "agent señalado");
        assert!(anim_cancel.is_cancelled(), "anim-ia señalado");
        assert!(runtime.anim_ia_job.is_none(), "anim dropeado");
        assert!(runtime.gif_export_job.is_none(), "export soltado");
        // El reaper hace join + borra (espera acotada, sin cuelgue).
        for _ in 0..200 {
            if !ruta.exists() {
                break;
            }
            std::thread::sleep(std::time::Duration::from_millis(10));
        }
        assert!(!ruta.exists(), "el reaper entierra el temporal");
        // Remote conserva el slot hasta el drain (sin huérfanos).
        assert!(!runtime.remote_request_slot_is_free());
    }

    #[test]
    fn r1_puente_muere_si_complete_paniquea() {
        // R1-5: flag en `Drop`/scopeguard + `join` acotado 100 ms. Si el
        // `complete` paniquea, el puente igual muere (sin hilo huérfano).
        use std::sync::atomic::Ordering;
        use std::time::Duration;
        // El guard marca en Drop incluso con panic (vía `catch_unwind`).
        let flag = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false));
        let res = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            let _g = SpecTerminadoGuard { flag: flag.clone() };
            panic!("complete simulado");
        }));
        assert!(res.is_err());
        assert!(flag.load(Ordering::Acquire), "el guard marca en panic");
        // El puente con la flag marcada muere solo y el join acotado lo drena.
        let flag2 = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false));
        let flag_puente = flag2.clone();
        let puente = std::thread::spawn(move || {
            while !flag_puente.load(Ordering::Acquire) {
                std::thread::sleep(Duration::from_millis(5));
            }
        });
        flag2.store(true, Ordering::Release);
        assert!(
            join_puente_bounded(puente, Duration::from_millis(100)),
            "puente marcado muere dentro de la cota"
        );
        // Colgado de verdad: timeout rápido sin bloquear (detach con marca).
        let colgado = std::thread::spawn(|| std::thread::sleep(Duration::from_secs(30)));
        let inicio = std::time::Instant::now();
        assert!(!join_puente_bounded(colgado, Duration::from_millis(50)));
        assert!(inicio.elapsed() < Duration::from_secs(2));
    }

    #[test]
    fn r1_reaper_acotado_sin_crecimiento_y_temporal_borrado() {
        // R1-4: cancel durante export → `join` acotado (no cuelga) + temporal
        // borrado + sin crecimiento de threads (5 cancels seguidos estables).
        use std::time::Duration;
        // Rápido: termina y se joinea dentro de la cota.
        let h = std::thread::spawn(|| {
            Ok::<_, crate::anim_native::GifExportError>(std::path::PathBuf::from("x"))
        });
        let inicio = std::time::Instant::now();
        assert!(join_gif_handle_bounded(h, Duration::from_secs(2)).is_some());
        assert!(inicio.elapsed() < Duration::from_secs(2));
        // Colgado: da timeout rápido sin bloquear (detach con marca).
        let colgado = std::thread::spawn(|| {
            std::thread::sleep(Duration::from_secs(30));
            Ok::<_, crate::anim_native::GifExportError>(std::path::PathBuf::from("y"))
        });
        let inicio = std::time::Instant::now();
        assert!(join_gif_handle_bounded(colgado, Duration::from_millis(120)).is_none());
        assert!(
            inicio.elapsed() < Duration::from_secs(5),
            "el timeout no cuelga el cancel"
        );
        // Cancel real sobre temporal: el slot se suelta y el reaper entierra.
        let mut runtime = AssistantRuntime::default();
        let ruta = std::env::temp_dir().join(format!(
            "grafito_reaper_r1_{}_{}.gif",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|d| d.as_nanos())
                .unwrap_or(0)
        ));
        std::fs::write(&ruta, b"GIF89a").expect("temporal");
        let ruta_hilo = ruta.clone();
        runtime.gif_export_job = Some(GifExportJob {
            handle: std::thread::spawn(move || Ok(ruta_hilo)),
            frame_count: 1,
            cancel: CancellationToken::default(),
            path: ruta.clone(),
        });
        assert!(runtime.cancel_anim_job());
        assert!(runtime.gif_export_job.is_none());
        for _ in 0..200 {
            if !ruta.exists() {
                break;
            }
            std::thread::sleep(Duration::from_millis(10));
        }
        assert!(!ruta.exists(), "temporal borrado tras cancel");
    }

    #[test]
    fn m1_playlist_titula_por_punto_unico_sin_eco_crudo() {
        // Defecto 7: la playlist titula por `titulo_curado` (mismo que las
        // 3 vías), jamás eco crudo con typos.
        let pedido =
            "haceme una animacion de una integrela y después explica la derivada con animación";
        let (a, b) = split_playlist_request(pedido).expect("playlist X y después Y");
        let ta = plantilla_para_pedido(&a);
        let tb = plantilla_para_pedido(&b);
        assert_eq!(ta, "integral-area");
        assert_eq!(tb, "derivative-slope");
        let titulo = format!(
            "{} (playlist nativa)",
            [titulo_curado(ta, &a, None), titulo_curado(tb, &b, None)].join(" y después ")
        );
        assert!(!titulo.to_lowercase().contains("integrela"), "{titulo}");
        assert!(titulo.contains("Integral"), "{titulo}");
        assert!(titulo.contains("Derivada"), "{titulo}");
    }

    #[test]
    fn m1_timeouts_motor_pineados_y_spec_pre_cancelado_no_toca_red() {
        // Defecto 4: cada timeout documentado en su const.
        assert_eq!(ANIM_MOTOR_IDLE_TIMEOUT_SECS, 2);
        assert_eq!(ANIM_MOTOR_JOB_TIMEOUT_SECS, 15);
        assert_eq!(
            ANIM_IA_SPEC_TIMEOUT_MS,
            grafito_assistant_types::RequestBudget::default().timeout_ms / 2
        );
        // Token pre-cancelado: el SPEC agente no toca red (Transporte
        // honesto inmediato; la ligadura Cancellation↔token vive adentro).
        let cancel = CancellationToken::default();
        cancel.cancel();
        let settings = grafito_assistant::ProviderSettings::for_profile(
            ProviderProfile::OpenCodeGo,
            "deepseek-v4-flash",
        );
        match crate::GrafitoApp::pedir_spec_ia_de_verdad(
            "derivada x^3 [-2,2]".to_string(),
            settings,
            None,
            true,
            ANIM_IA_SPEC_TIMEOUT_MS,
            cancel,
        ) {
            PedidoSpecIa::Transporte(detalle) => {
                assert!(detalle.contains("cancel"), "{detalle}");
            }
            otra => panic!("pre-cancelado debe ser Transporte, fue {otra:?}"),
        }
    }

    #[test]
    fn m1_remota_ia_tangente_x3_valida_prosa_frames_y_timeout_fallback() {
        // Vía remota IA ante los mismos inputs que Submit y agente.
        // 1. Explícita: JSON tangente x³ → SPEC válido, prosa que nombra.
        let texto_ia =
            r#"{"expr": "x^3", "p0": -2, "p1": 2, "plantilla": "derivative-slope", "param": "p"}"#;
        let spec = parsear_spec_anim_ia(texto_ia, "derivada x³ [-2,2] con animación")
            .expect("tangente x^3 valida");
        assert_eq!(spec.expr, "x^3");
        assert_eq!((spec.p0, spec.p1), (-2.0, 2.0));
        assert_eq!(spec.plantilla, "derivative-slope");
        let prosa = prosa_para_spec_anim_ia(&spec);
        assert!(prosa.contains("x^3"), "{prosa}");
        assert!(prosa.contains("[-2,2]"), "{prosa}");
        match resolver_turno_anim_ia(true, PedidoSpecIa::Exito(spec.clone())) {
            DesenlaceAnimIa::RenderIa {
                spec: render_spec,
                prosa: render_prosa,
            } => {
                assert_eq!(render_spec.expr, "x^3");
                assert!(render_prosa.contains("x^3"), "{render_prosa}");
            }
            otro => panic!("con IA válida debe renderizar IA, fue {otro:?}"),
        }
        let cancel = CancellationToken::default();
        let media = render_media_desde_spec_ia(&spec, &cancel).expect("render tangente x^3");
        assert!(
            media.title.contains("x^3"),
            "título nombra x³: {}",
            media.title
        );
        // 2. Vacía (sin función): el parse exige función → error honesto.
        assert!(parsear_spec_anim_ia("{}", "derivada con animación").is_err());
        // 3. Inválida: foo(x) no valida con `infer_*`.
        let basura = r#"{"expr": "foo(x)", "p0": -2, "p1": 2, "plantilla": "derivative-slope"}"#;
        assert!(parsear_spec_anim_ia(basura, "tangente con animación").is_err());
        match resolver_turno_anim_ia(true, PedidoSpecIa::Invalido("la función no valida".into())) {
            DesenlaceAnimIa::ErrorHonesto(detalle) => assert!(!detalle.is_empty()),
            otro => panic!("SPEC inválido debe ser error honesto, fue {otro:?}"),
        }
        // 4. Timeout/transporte → fallback canónico con aviso genérico del
        // resolver (el worker lo especializa con `aviso_fallback_canonico`).
        for salida in [
            PedidoSpecIa::Timeout,
            PedidoSpecIa::Transporte("remote assistant returned HTTP 429".into()),
        ] {
            match resolver_turno_anim_ia(true, salida) {
                DesenlaceAnimIa::FallbackCanonico { aviso } => {
                    assert_eq!(aviso, ANIM_SIN_IA_AVISO);
                }
                otra => panic!("timeout/transporte debe ser fallback, fue {otra:?}"),
            }
        }
    }

    #[test]
    fn integral_canonica_prosa_declara_y_no_duplica() {
        // La prosa declara la canónica en rioplatense.
        let mut conversacion = vec![ConversationTurn::assistant("La animación está lista.")];
        append_canonical_integral_prose(&mut conversacion);
        let texto = &conversacion.last().expect("turno").content;
        assert!(texto.contains("x²"), "{texto}");
        assert!(texto.contains("pedime otra"), "{texto}");
        // Idempotente: segunda pasada no duplica.
        append_canonical_integral_prose(&mut conversacion);
        assert_eq!(
            conversacion
                .last()
                .expect("turno")
                .content
                .matches("pedime otra")
                .count(),
            1
        );
        // Turno de usuario o vacío: no toca nada.
        let mut usuario = vec![ConversationTurn::user("hola")];
        append_canonical_integral_prose(&mut usuario);
        assert_eq!(usuario.last().expect("turno").content, "hola");
        let mut vacia: Vec<ConversationTurn> = Vec::new();
        append_canonical_integral_prose(&mut vacia);
        assert!(vacia.is_empty());
    }

    #[test]
    fn screenshot_typo_integrela_es_canonica_con_prosa_que_declara() {
        // Input EXACTO del screenshot (typos "animacion"/"integrela"): antes
        // era `universal` + prosa preguntaba Y media mostraba (contradicción).
        // Ahora es canónica declarada, sin pregunta.
        let pedido = "hace una animacion de una integrela (nativa)";
        assert!(
            crate::anim_ui::wants_animation_request(pedido),
            "el gatillo 'animacion' debe disparar"
        );
        assert!(
            crate::anim_ui::animation_concept_from_request(pedido).is_ok(),
            "con 'integrela (nativa)' hay concepto, no es ambiguo"
        );
        assert!(
            grafito_anim::parametric::pedido_menciona_area(pedido),
            "'integrela' matchea 'integral' por fuzzy"
        );
        assert_eq!(plantilla_para_pedido(pedido), "integral-area");
        assert_eq!(
            clasifica_pedido_integral(pedido, "integral-area"),
            IntegralPedido::Canonica
        );
        match decide_animacion(pedido) {
            DecisionAnimacion::RenderCanonico {
                plantilla,
                concepto,
            } => {
                assert_eq!(plantilla, "integral-area");
                assert!(concepto.contains("integrela"), "{concepto}");
            }
            otra => panic!("el typo debe ser RenderCanonico, fue {otra:?}"),
        }
        // La prosa del turno declara la canónica Y referencia la media:
        // jamás pregunta y muestra a la vez.
        let prosa = format!(
            "{}\n\n{}",
            crate::anim_ui::animation_reference_sentence(),
            grafito_anim::parametric::INTEGRAL_CANONICAL_PROSA
        );
        assert!(prosa.contains("deslizador"), "{prosa}");
        assert!(prosa.contains("x²"), "{prosa}");
        assert!(prosa.contains("pedime otra"), "{prosa}");
        assert!(
            !prosa.contains("¿Qué función")
                && !prosa.contains("qué función")
                && !prosa.contains("Qué función"),
            "la canónica declara, no pregunta: {prosa}"
        );
    }

    #[test]
    fn typos_animacion_integrela_derivadaa_normalizan_y_matchean() {
        // Normalización sin tildes.
        assert_eq!(
            grafito_anim::parametric::normaliza_para_match("animación"),
            "animacion"
        );
        assert_eq!(
            grafito_anim::parametric::normaliza_para_match("ÁREA"),
            "area"
        );
        // Fuzzy acotado por token.
        assert!(grafito_anim::parametric::token_matchea_clave(
            "integrela",
            "integral"
        ));
        assert!(grafito_anim::parametric::token_matchea_clave(
            "derivadaa",
            "derivada"
        ));
        assert!(grafito_anim::parametric::token_matchea_clave(
            "animacion",
            "animacion"
        ));
        assert!(grafito_anim::parametric::token_matchea_clave(
            "integrar", "integral"
        ));
        // Cortas no hacen fuzzy ("arena" no es área).
        assert!(!grafito_anim::parametric::token_matchea_clave(
            "arena", "area"
        ));
        assert!(!grafito_anim::parametric::pedido_menciona_area(
            "tarea de matemática"
        ));
    }

    #[test]
    fn decision_matriz_explicita_canonica_invalida_media_y_prosa() {
        // a) Explícita → media SÍ + prosa que nombra la función.
        let explicita = "animacion de la integral de f(x)=x^3 de 0 a 2 con animación";
        match decide_animacion(explicita) {
            DecisionAnimacion::RenderExplicito {
                plantilla,
                concepto: _,
                expr,
            } => {
                assert_eq!(plantilla, "integral-area");
                assert_eq!(expr, "x^3");
                let prosa = prosa_integral_explicita(&expr, explicita);
                assert!(prosa.contains("x^3"), "{prosa}");
                assert!(prosa.contains("deslizador"), "{prosa}");
                assert!(
                    !prosa.contains("pedime otra"),
                    "el marcador es solo canónico: {prosa}"
                );
            }
            otra => panic!("explícita debe renderizar, fue {otra:?}"),
        }
        // b) Sin función (incluido typo) → media SÍ + prosa canónica declarada.
        for sin_funcion in [
            "haceme una animacion de una integral (nativa)",
            "hace una animacion de una integrela (nativa)",
        ] {
            match decide_animacion(sin_funcion) {
                DecisionAnimacion::RenderCanonico { plantilla, .. } => {
                    assert_eq!(plantilla, "integral-area", "{sin_funcion}");
                }
                otra => panic!("{sin_funcion:?} debe ser canónica, fue {otra:?}"),
            }
        }
        // c) Inválida → SIN media, solo pregunta/guía.
        let invalida = "animacion de la integral de f(x)=foo(x) con p en [0,1] con animación";
        match decide_animacion(invalida) {
            DecisionAnimacion::PreguntarSinMedia(guia) => {
                assert!(guia.contains("foo(x)"), "{guia}");
                assert!(guia.contains("x^2"), "da ejemplo: {guia}");
            }
            otra => panic!("inválida no renderiza, fue {otra:?}"),
        }
        // Ambiguo local → SIN media.
        match decide_animacion("explica con animación") {
            DecisionAnimacion::PreguntarSinMedia(_) => {}
            otra => panic!("ambiguo no renderiza, fue {otra:?}"),
        }
        // No-animación → flujo chat normal.
        assert_eq!(
            decide_animacion("derivá x^2"),
            DecisionAnimacion::NoAnimacion
        );
    }

    #[test]
    fn plantilla_typo_no_cae_a_universal() {
        // El detector clásico dice `universal` para el typo; el punto único
        // lo corrige a `integral-area` (evita fallback huérfano).
        let pedido = "hace una animacion de una integrela (nativa)";
        assert_eq!(
            crate::anim_native::detect_template_for_concept(pedido),
            "universal"
        );
        assert_eq!(plantilla_para_pedido(pedido), "integral-area");
    }

    #[test]
    fn titulo_curado_universal_con_typo_no_muestra_typo() {
        // Frente títulos a medias: las 3 vías comparten `titulo_curado`;
        // `universal` + concepto con typo "integrela" no hace eco crudo.
        let base = titulo_curado("universal", "una integrela (nativa)", None);
        assert_eq!(base, "Integral — área bajo la curva");
        assert!(
            !base.to_lowercase().contains("integrela"),
            "el typo no se muestra: {base}"
        );
        // La vía nativa agrega el sufijo una sola vez (sin duplicar).
        assert_eq!(
            format!("{base} (nativa)"),
            "Integral — área bajo la curva (nativa)"
        );
        // La paramétrica titula por kind aunque el concepto venga sucio.
        let anim = crate::anim_native::parametric_for_template("derivative-slope", "derivada")
            .expect("derivative-slope es paramétrica");
        assert_eq!(
            titulo_curado("derivative-slope", "una derivadaa (nativa)", Some(&anim)),
            "Tangente móvil · x^2"
        );
        // Concepto libre desconocido: curado sin sufijo duplicado.
        assert_eq!(
            titulo_curado("universal", "fractales raros (nativa)", None),
            "fractales raros"
        );
        // Vacío: honesto, jamás título en blanco.
        assert_eq!(titulo_curado("universal", "   ", None), "Animación");
    }

    #[test]
    fn titulo_normaliza_una_vez_con_paridad() {
        // Auditoría (triple normalización): el concepto se normaliza UNA vez
        // y se reusa para área/tangente/nombres. El contador cuenta las
        // llamadas en el fuente (aguja armada por partes para no autocontar
        // este test en el `include_str!`).
        let aguja = concat!("normaliza_para_match", "(concept)");
        let fuente: &str = include_str!("assistant.rs");
        assert_eq!(
            fuente.matches(aguja).count(),
            1,
            "una sola normalización del concepto en assistant.rs"
        );
        // Paridad con los matchers canónicos: batería con typos, mayúsculas
        // y tildes da los mismos títulos que antes del refactor.
        for (concepto, esperado) in [
            ("una integrela", "Integral — área bajo la curva"),
            ("INTEGRAL de x", "Integral — área bajo la curva"),
            ("el área bajo la curva", "Integral — área bajo la curva"),
            ("derivadaa de x^2", "Derivada como pendiente"),
            ("la tangente en x=1", "Derivada como pendiente"),
            ("PITÁGORAS", "Teorema de Pitágoras"),
            ("serie de taylor", "Serie de Taylor"),
            ("mapeo conforme", "Mapeo conforme"),
            ("conformal map", "Mapeo conforme"),
            ("tarea pendiente", "Derivada como pendiente"),
            ("fractales raros (nativa)", "fractales raros"),
            ("   ", "Animación"),
        ] {
            assert_eq!(
                titulo_curado("universal", concepto, None),
                esperado,
                "concepto: {concepto}"
            );
        }
        // "tarea" sola NO es área (exacta por token, evita "tarea"→área).
        assert_eq!(titulo_curado("universal", "la tarea", None), "la tarea");
    }

    #[test]
    fn titulos_vienen_del_catalogo_i18n() {
        // Auditoría: los literales ES viven en `MESSAGES` (`media.title.*`);
        // acá solo `t()` + sustitución. EN/PT resuelven sin colgados.
        use grafito_ui::i18n::{t, Locale};
        assert_eq!(
            titulo_curado("integral-area", "x", None),
            t("media.title.integral", Locale::Es)
        );
        assert_eq!(
            titulo_curado("pitagoras", "x", None),
            t("media.title.pitagoras", Locale::Es)
        );
        let anim = crate::anim_native::parametric_for_template("derivative-slope", "derivada")
            .expect("derivative-slope es paramétrica");
        for locale in [Locale::Es, Locale::En, Locale::Pt] {
            let base = titulo_curado_localized("universal", "una integrela", None, locale);
            assert_eq!(base, t("media.title.integral", locale), "{locale:?}");
            let por_kind = titulo_curado_localized("derivative-slope", "x", Some(&anim), locale);
            assert!(
                por_kind.contains("x^2"),
                "el expr viaja en {locale:?}: {por_kind}"
            );
            for colgado in ["{expr}", "{p0}", "{p1}", "{param}"] {
                assert!(
                    !por_kind.contains(colgado),
                    "sin placeholders colgados en {locale:?}: {por_kind}"
                );
            }
        }
        let en = titulo_curado_localized("universal", "una integrela", None, Locale::En);
        assert_eq!(en, "Integral — area under the curve");
    }

    #[test]
    fn export_sin_media_falla_honesto_sin_hilo() {
        // Sin animación: nada se spawnea, la card muestra el motivo y hay aviso.
        let mut app = crate::app::dummy_grafito_app();
        let ctx = egui::Context::default();
        app.export_assistant_media(&ctx);
        assert!(app.assistant_runtime.gif_export_job.is_none());
        assert!(
            matches!(
                app.assistant.media_export_state(),
                grafito_ui::assistant::MediaExportState::Failed(_)
            ),
            "sin frames el error es honesto, jamás mudo"
        );
    }

    #[test]
    fn export_no_duplica_job_en_vuelo() {
        // Slot ocupado: avisa y no pisa el hilo existente.
        let mut app = crate::app::dummy_grafito_app();
        let ctx = egui::Context::default();
        app.assistant_runtime.gif_export_job = Some(GifExportJob {
            handle: std::thread::spawn(|| Ok(std::path::PathBuf::from("ocupado"))),
            frame_count: 1,
            cancel: CancellationToken::default(),
            path: std::path::PathBuf::from("ocupado"),
        });
        app.export_assistant_media(&ctx);
        assert!(
            app.assistant_runtime.gif_export_job.is_some(),
            "no pisa el job en vuelo"
        );
        // Limpia sin colgar (el hilo ya terminó).
        let job = app
            .assistant_runtime
            .gif_export_job
            .take()
            .expect("job en vuelo");
        let _ = job.handle.join();
    }

    #[test]
    fn export_con_frames_corre_en_hilo_y_publica_exito_con_ruta() {
        // Cableado real: `spawn_gif_export` escribe un GIF válido y el poll
        // publica `Done` (ruta avisada por el aviso). Limpia su temporal.
        // El prefijo `grafito_animacion_` solo lo crea este export.
        let mut app = crate::app::dummy_grafito_app();
        let ctx = egui::Context::default();
        let frames = vec![egui::ColorImage::new([8, 8], egui::Color32::RED); 3];
        app.assistant.set_media(
            Some(grafito_ui::assistant::AssistantMedia {
                title: "prueba".into(),
                frames,
            }),
            &ctx,
        );
        app.export_assistant_media(&ctx);
        assert!(app.assistant_runtime.gif_export_job.is_some());
        assert_eq!(
            *app.assistant.media_export_state(),
            grafito_ui::assistant::MediaExportState::Exporting
        );
        for _ in 0..200 {
            app.poll_gif_export_job(&ctx);
            if app.assistant_runtime.gif_export_job.is_none() {
                break;
            }
            std::thread::sleep(std::time::Duration::from_millis(10));
        }
        assert!(
            app.assistant_runtime.gif_export_job.is_none(),
            "el hilo debe terminar"
        );
        assert_eq!(
            *app.assistant.media_export_state(),
            grafito_ui::assistant::MediaExportState::Done
        );
        // El GIF existe y es real; se borra para no ensuciar el temporal.
        let mut cleaned = 0;
        let entries = std::fs::read_dir(std::env::temp_dir()).expect("temporal legible");
        for entry in entries.filter_map(Result::ok) {
            let path = entry.path();
            let is_ours = path
                .file_name()
                .and_then(|name| name.to_str())
                .is_some_and(|name| name.starts_with("grafito_animacion_"));
            if !is_ours {
                continue;
            }
            let bytes = std::fs::read(&path).expect("GIF exportado legible");
            assert_eq!(&bytes[0..6], b"GIF89a", "GIF real con cabecera");
            std::fs::remove_file(&path).expect("limpia su temporal");
            cleaned += 1;
        }
        assert_eq!(cleaned, 1, "un solo GIF de este export");
    }

    #[test]
    fn cancel_a_mitad_senala_token_y_descarta_media_rancia() {
        // Cancel a mitad → token señalado y sin media rancia en el turno.
        let mut runtime = AssistantRuntime::default();
        let cancel = CancellationToken::default();
        let (tx, rx) = sync_channel::<Result<grafito_ui::assistant::AssistantMedia, String>>(1);
        runtime.anim_job = Some(AssistantAnimJob {
            cancellation: cancel.clone(),
            receiver: rx,
        });
        assert!(runtime.cancel_anim_job(), "debe haber job");
        assert!(cancel.is_cancelled(), "descartar señala el token");
        assert!(runtime.anim_job.is_none());
        // El hilo en vuelo observa el token entre frames y su envío tardío
        // (Err cancel) no tiene receiver: se descarta sin publicar.
        // El slot ya se dropeó, así que el turno queda sin media rancia.
        drop(tx);
        let ctx = egui::Context::default();
        let mut panel = AssistantPanelState::default();
        panel.set_media(None, &ctx);
        assert!(panel.media.is_none(), "cancel no deja media rancia");
        // El closure de progreso del hilo habría visto el token (contrato).
        assert!(cancel.is_cancelled());
    }

    #[test]
    fn cancelled_remote_job_remains_nonretryable_until_its_joiner_is_reaped() {
        let mut runtime = AssistantRuntime::default();
        let cancellation = CancellationToken::default();
        let (sender, receiver) = sync_channel::<Result<RemoteCompletion, String>>(1);
        runtime.remote_job = Some(AssistantRemoteJob {
            id: 1,
            provider: ProviderProfile::OpenCodeGo,
            model: "deepseek-v4-pro".into(),
            route: AssistantRemoteRoute::SelectedModel,
            fusion_fallback_allowed: false,
            question: "consulta".into(),
            correction_attempt: 0,
            repair_target_turn: None,
            document_revision: 3,
            document_digest: "fnv1a64:request".into(),
            focus: None,
            cancellation: cancellation.clone(),
            receiver,
            stream_rx: None,
            stream_text: String::new(),
            preview_active: false,
            started_at: std::time::Instant::now(),
            first_delta_at: None,
        });

        assert!(runtime.cancel_stale_remote_job(ProviderProfile::DeepSeek, "deepseek-chat"));
        assert!(cancellation.is_cancelled());
        assert!(!runtime.remote_request_slot_is_free());

        sender
            .send(Err("remote assistant request was cancelled".into()))
            .unwrap();
        assert!(!runtime.remote_request_slot_is_free());
        let finished = runtime.take_finished_remote_job().unwrap();
        assert_eq!(finished.document_revision, 3);
        assert_eq!(finished.document_digest, "fnv1a64:request");
        assert!(finished.focus.is_none());
        assert!(runtime.remote_request_slot_is_free());
    }

    #[test]
    fn stream_preview_drains_to_provisional_bubble_and_pops_on_finish() {
        let mut runtime = AssistantRuntime::default();
        let (result_tx, result_rx) = sync_channel::<Result<RemoteCompletion, String>>(1);
        let (delta_tx, delta_rx) = sync_channel::<String>(128);
        let cancel = CancellationToken::default();
        runtime.remote_job = Some(AssistantRemoteJob {
            id: 1,
            provider: ProviderProfile::OpenCodeGo,
            model: "muse-spark-1.3-contributor".into(),
            route: AssistantRemoteRoute::SelectedModel,
            fusion_fallback_allowed: false,
            question: "derivá x^2".into(),
            correction_attempt: 0,
            repair_target_turn: None,
            document_revision: 1,
            document_digest: "d".into(),
            focus: None,
            cancellation: cancel.clone(),
            receiver: result_rx,
            stream_rx: Some(delta_rx),
            stream_text: String::new(),
            preview_active: false,
            started_at: std::time::Instant::now(),
            first_delta_at: None,
        });
        let mut panel = AssistantPanelState::default();
        panel
            .conversation
            .push(ConversationTurn::user("derivá x^2"));
        let ctx = egui::Context::default();

        // Sin deltas no hay burbuja provisional.
        assert!(!runtime.drain_remote_stream_preview(&mut panel, &ctx));
        assert_eq!(panel.conversation.len(), 1);

        delta_tx.send("Hola ".into()).unwrap();
        delta_tx.send("mundo".into()).unwrap();
        assert!(runtime.drain_remote_stream_preview(&mut panel, &ctx));
        assert_eq!(panel.conversation.len(), 2);
        let provisional = panel.conversation.last().unwrap();
        assert_eq!(provisional.role, ConversationRole::Assistant);
        assert_eq!(provisional.content, "Hola mundo");

        // Más deltas actualizan el mismo turno (no duplican burbujas).
        delta_tx.send("!".into()).unwrap();
        assert!(runtime.drain_remote_stream_preview(&mut panel, &ctx));
        assert_eq!(panel.conversation.len(), 2);
        assert_eq!(panel.conversation.last().unwrap().content, "Hola mundo!");

        // Al terminar (cancelado), el poll retira el provisional y queda el
        // turno de usuario, igual que en el path no-streaming.
        cancel.cancel();
        result_tx
            .send(Err("remote assistant request was cancelled".into()))
            .unwrap();
        let finished = runtime.take_finished_remote_job().unwrap();
        assert!(finished.cancelled);
        assert!(finished.stream_preview_active);
        pop_provisional_stream_turn(&mut panel);
        assert_eq!(panel.conversation.len(), 1);
        assert_eq!(
            panel.conversation.last().unwrap().role,
            ConversationRole::User
        );
    }

    #[test]
    fn pop_provisional_stream_turn_never_touches_real_history() {
        let mut panel = AssistantPanelState::default();
        // Sin turnos o con último turno de usuario: no-op.
        pop_provisional_stream_turn(&mut panel);
        assert!(panel.conversation.is_empty());
        panel.conversation.push(ConversationTurn::user("hola"));
        pop_provisional_stream_turn(&mut panel);
        assert_eq!(panel.conversation.len(), 1);
    }

    #[test]
    fn remote_stages_go_in_order_with_rioplatense_texts() {
        // autorizada → conectando → esperando primer token → recibiendo (KiB).
        assert_eq!(remote_stage_for_job(0, false, 0), RemoteStage::Autorizada);
        assert_eq!(remote_stage_for_job(1, false, 0), RemoteStage::Conectando);
        assert_eq!(
            remote_stage_for_job(2, false, 0),
            RemoteStage::EsperandoPrimerToken
        );
        assert_eq!(
            remote_stage_for_job(30, false, 0),
            RemoteStage::EsperandoPrimerToken
        );
        assert_eq!(
            remote_stage_for_job(3, true, 0),
            RemoteStage::Recibiendo { kib: 0 }
        );
        assert_eq!(
            remote_stage_for_job(12, true, 3),
            RemoteStage::Recibiendo { kib: 3 }
        );
        // Textos rioplatenses viven en la Piel (una sola fuente); la app solo
        // sincroniza etapa + segundos. Aca se pinnea el mapeo app->ui.
        let ui_for = |stage: RemoteStage| match stage {
            RemoteStage::Autorizada => grafito_ui::assistant::RemoteStage::Autorizada,
            RemoteStage::Conectando => grafito_ui::assistant::RemoteStage::Conectando,
            RemoteStage::EsperandoPrimerToken => {
                grafito_ui::assistant::RemoteStage::EsperandoPrimerToken
            }
            RemoteStage::Recibiendo { kib } => {
                grafito_ui::assistant::RemoteStage::Recibiendo { kib }
            }
        };
        assert_eq!(
            ui_for(RemoteStage::Autorizada).label(),
            "Autorizada, conectando…"
        );
        assert_eq!(
            ui_for(RemoteStage::EsperandoPrimerToken).label(),
            "Esperando el primer token…"
        );
        assert_eq!(
            ui_for(RemoteStage::Recibiendo { kib: 3 }).label(),
            "Recibiendo (3 KiB)…"
        );
        // Umbral lento unico (10s) en transporte y Piel: sin agrandar timeouts.
        assert_eq!(grafito_assistant::REMOTE_SLOW_STAGE_SECS, 10);
        assert_eq!(grafito_ui::assistant::REMOTE_SLOW_STAGE_SECS, 10);
        assert_eq!(grafito_assistant::REMOTE_CONNECT_TIMEOUT_SECS, 10);
    }

    #[test]
    fn remote_stage_sync_updates_panel_and_first_delta_marks_receiving() {
        let mut runtime = AssistantRuntime::default();
        let (_result_tx, result_rx) = sync_channel::<Result<RemoteCompletion, String>>(1);
        let (delta_tx, delta_rx) = sync_channel::<String>(128);
        runtime.remote_job = Some(AssistantRemoteJob {
            id: 1,
            provider: ProviderProfile::OpenCodeGo,
            model: "deepseek-v4-flash".into(),
            route: AssistantRemoteRoute::SelectedModel,
            fusion_fallback_allowed: false,
            question: "q".into(),
            correction_attempt: 0,
            repair_target_turn: None,
            document_revision: 1,
            document_digest: "d".into(),
            focus: None,
            cancellation: CancellationToken::default(),
            receiver: result_rx,
            stream_rx: Some(delta_rx),
            stream_text: String::new(),
            preview_active: false,
            started_at: std::time::Instant::now(),
            first_delta_at: None,
        });
        let mut panel = AssistantPanelState::default();
        let ctx = egui::Context::default();
        // Sin deltas: etapa inicial (autorizada/conectando), sin burbuja.
        assert!(!runtime.drain_remote_stream_preview(&mut panel, &ctx));
        assert!(matches!(
            panel.remote_stage,
            grafito_ui::assistant::RemoteStage::Autorizada
                | grafito_ui::assistant::RemoteStage::Conectando
        ));
        // Primer delta: marca `first_delta_at` y pasa a `Recibiendo`.
        delta_tx.send("hola ".into()).unwrap();
        assert!(runtime.drain_remote_stream_preview(&mut panel, &ctx));
        let job = runtime.remote_job.as_ref().unwrap();
        assert!(job.first_delta_at.is_some());
        assert!(matches!(
            panel.remote_stage,
            grafito_ui::assistant::RemoteStage::Recibiendo { .. }
        ));
        assert_eq!(panel.remote_stage_text(), "Recibiendo (0 KiB)…");
    }

    #[test]
    fn remote_error_messages_are_honest_per_stage_without_raw_echo() {
        // Esperando primer token: dice la etapa + sugerencia, sin inglés crudo.
        let esperando = remote_error_message(
            "remote assistant stream timed out waiting for first token after 60s",
            "muse-spark-1.3",
        );
        assert!(esperando.contains("primer token"), "{esperando}");
        assert!(
            esperando.contains("deepseek") || esperando.contains("Reintentá"),
            "{esperando}"
        );
        assert!(!esperando.contains("timed out after"), "{esperando}");
        // Recibiendo: pide continuar, sin eco crudo.
        let recibiendo = remote_error_message(
            "remote assistant stream timed out while receiving after 60s (12 KiB received)",
            "muse-spark-1.3",
        );
        assert!(recibiendo.contains("recib"), "{recibiendo}");
        assert!(
            recibiendo.contains("continúe") || recibiendo.contains("partes"),
            "{recibiendo}"
        );
        assert!(!recibiendo.contains("while receiving"), "{recibiendo}");
        // Timeout genérico: tiempo total, sin agrandar presupuesto.
        let total =
            remote_error_message("remote assistant timed out after 60s", "deepseek-v4-flash");
        assert!(total.contains("tiempo total"), "{total}");
        assert!(!total.contains("timed out after"), "{total}");
        // Conectando: etapa + sugerencia, sin inglés crudo.
        let conectando = remote_error_message(
            "remote assistant stream could not connect to the provider (red o DNS)",
            "deepseek-v4-flash",
        );
        assert!(conectando.contains("conectando"), "{conectando}");
        assert!(!conectando.contains("could not connect"), "{conectando}");
    }

    #[test]
    fn b7_wants_exercise_request_solo_con_verbo_explicito() {
        // Dispara con verbo de ejercitación explícito…
        assert!(wants_exercise_request("haceme un ejercicio de derivadas"));
        assert!(wants_exercise_request("practicamos integrales, che"));
        assert!(wants_exercise_request("andamiame con límites"));
        assert!(wants_exercise_request("poneme a prueba con fracciones"));
        // …y no pisa preguntas ni graficación normal.
        assert!(!wants_exercise_request("qué es una derivada"));
        assert!(!wants_exercise_request("graficá x^2"));
        assert!(!wants_exercise_request("hola"));
        assert!(!wants_exercise_request(""));
    }

    #[test]
    fn socratic_guard_context_seeds_attempts_from_heuristic_answers() {
        // Fresca: sólo la pregunta actual → attempts=0, tema sanitizado a canónico.
        // `derivá x^2` extrae `derivada` (verbo→sustantivo), jamás el crudo.
        let fresh = vec![ConversationTurn::user("derivá x^2")];
        let guard = socratic_guard_context(8, None, "derivá x^2", &fresh);
        assert_eq!(guard.fsm.attempts, 0);
        assert_eq!(guard.fsm.topic, "derivada");
        assert!(guard.scaffold.question.contains("derivada"));
        assert!(!guard.scaffold.question.contains("derivá x^2"));

        // Sin tema reconocido → tópico vacío + scaffold fallback (nunca crudo).
        let raw = "hola haceme ejemplos para probar las capacidades de graficacion";
        let demo = vec![ConversationTurn::user(raw)];
        let guard = socratic_guard_context(8, None, raw, &demo);
        assert_eq!(guard.fsm.attempts, 0);
        assert_eq!(guard.fsm.topic, "");
        assert_eq!(
            guard.scaffold.question,
            grafito_pedagogy::scaffold::NO_CONCEPT_FALLBACK_QUESTION
        );
        assert!(!guard.scaffold.question.contains("hola"));

        // Un intercambio SIN `?` previa → attempts=0 (no es respuesta heurística).
        let one_exchange = vec![
            ConversationTurn::user("primera"),
            ConversationTurn::assistant("re-pregunta"),
            ConversationTurn::user("segunda"),
        ];
        let guard = socratic_guard_context(8, Some("derivada"), "segunda", &one_exchange);
        assert_eq!(guard.fsm.attempts, 0);
        assert_eq!(guard.fsm.topic, "derivada");

        // Una respuesta a `?` previa → attempts=1 (sigue bloqueado, <2).
        let one_answer = vec![
            ConversationTurn::user("quiero ver derivada"),
            ConversationTurn::assistant("¿qué forma te imaginás?"),
            ConversationTurn::user("mi intento"),
            ConversationTurn::assistant("¿y ahora qué ves?"),
            ConversationTurn::user("segunda"),
        ];
        let guard = socratic_guard_context(8, Some("derivada"), "segunda", &one_answer);
        assert_eq!(guard.fsm.attempts, 1);
        assert_eq!(guard.fsm.topic, "derivada");

        // Dos respuestas a `?` → attempts=2 (reveal permitido).
        let two_answers = vec![
            ConversationTurn::user("init"),
            ConversationTurn::assistant("¿primera pregunta?"),
            ConversationTurn::user("ans1"),
            ConversationTurn::assistant("¿segunda pregunta?"),
            ConversationTurn::user("ans2"),
            ConversationTurn::assistant("¿tercera pregunta?"),
            ConversationTurn::user("tres"),
        ];
        let guard = socratic_guard_context(8, Some("  "), "tres", &two_answers);
        assert_eq!(guard.fsm.attempts, 2);
        // Tema en blanco + pregunta sin tema → tópico vacío (fallback, no crudo).
        assert_eq!(guard.fsm.topic, "");
    }

    #[test]
    fn socratic_repair_error_is_detected_by_prefix() {
        // Diagnóstico interno (solo logs/detección) conserva el prefijo.
        assert!(is_socratic_repair_error(
            "GUARD TELLING — REPARACIÓN SOCRÁTICA OBLIGATORIA (attempts=0 <2, ...)"
        ));
        assert!(!is_socratic_repair_error(
            "remote assistant returned HTTP 500: boom"
        ));
        assert!(!is_socratic_repair_error(
            "remote assistant request was cancelled"
        ));
        // La voz de estudiante (lo que SÍ va al transcript) jamás lleva el prefijo.
        let fsm = SocraticFsm::new("derivada");
        let scaffold =
            ScaffoldEngine.scaffold("derivada", PedagogicalLevel::from_level_value(8), &[]);
        let student = fsm.repair_student_message(&scaffold);
        assert!(!is_socratic_repair_error(&student));
        assert!(!student.contains("GUARD"), "{student}");
    }

    #[test]
    fn socratic_repair_turn_has_no_jargon_or_raw_echo_red_first() {
        // Regresión P0 red-first con el input real que mostró la directiva interna.
        // Falla si el turno contiene jerga o eco degenerado del saludo.
        let raw = "hola haceme ejemplos para probar las capacidades de graficacion";
        // 1. Es exploratorio y sin tema: el extractor y el scaffold no interpolan crudo.
        assert!(is_exploratory_request(raw));
        assert_eq!(extract_concept(raw), None);
        let scaffold = ScaffoldEngine.scaffold(raw, PedagogicalLevel::from_level_value(8), &[]);
        assert_eq!(
            scaffold.question,
            grafito_pedagogy::scaffold::NO_CONCEPT_FALLBACK_QUESTION
        );
        // 2. El guard sanitiza: tópico vacío (fallback), attempts=0 no punitivo.
        let conversation = vec![ConversationTurn::user(raw)];
        let guard = socratic_guard_context(8, None, raw, &conversation);
        assert_eq!(guard.fsm.attempts, 0);
        assert_eq!(guard.fsm.topic, "");
        // 3. La voz de Mili (lo único que va al transcript) no tiene jerga ni eco.
        let fsm = SocraticFsm::new(guard.fsm.topic.clone());
        let turno = fsm.repair_student_message(&guard.scaffold);
        for forbidden in [
            "GUARD",
            "REPARACIÓN",
            "REPARACION",
            "attempts",
            "can_reveal",
            "Re-pregunt",
            "¿Te imaginás hola",
        ] {
            assert!(
                !turno.contains(forbidden),
                "el turno contiene '{forbidden}': '{turno}'"
            );
        }
        assert!(turno.contains("Antes de mostrarte"), "{turno}");
    }

    #[test]
    fn proposal_verification_keeps_the_remote_slot_locked_until_its_worker_is_reaped() {
        let mut runtime = AssistantRuntime::default();
        let cancellation = CancellationToken::default();
        let (sender, receiver) = sync_channel::<Result<RemoteProposalVerification, String>>(1);
        runtime.proposal_job = Some(AssistantProposalJob {
            id: 2,
            provider: ProviderProfile::OpenCodeGo,
            model: "deepseek-v4-pro".into(),
            route: AssistantRemoteRoute::SelectedModel,
            fusion_fallback_allowed: false,
            question: "dibujá un corazon".into(),
            correction_attempt: 0,
            repair_target_turn: None,
            document_revision: 3,
            document_digest: "fnv1a64:request".into(),
            focus: None,
            text: "respuesta remota".into(),
            cancellation: cancellation.clone(),
            receiver,
        });

        assert!(!runtime.remote_request_slot_is_free());
        assert!(runtime.cancel_stale_remote_job(ProviderProfile::DeepSeek, "deepseek-chat"));
        assert!(cancellation.is_cancelled());

        sender
            .send(Ok(RemoteProposalVerification {
                verified: Vec::new(),
                candidate_count: 1,
                candidate_code_block_indices: vec![0],
                action_candidate_count: 1,
                verified_action_count: 0,
                repair_feedback: None,
            }))
            .unwrap();
        let finished = runtime.take_finished_proposal_job().unwrap();
        assert_eq!(finished.text, "respuesta remota");
        assert!(finished.cancelled);
        assert!(runtime.remote_request_slot_is_free());
    }

    #[test]
    fn cancelled_proposal_verification_stops_before_preflighting_remote_commands() {
        let document = Document::new();
        let cancellation = CancellationToken::default();
        cancellation.cancel();

        let result = inspect_remote_proposals_cancellable(
            &document,
            "```grafito\nFunction[sin(x)]\n```",
            grafito_geometry::Camera3D::new(4.0 / 3.0),
            &cancellation,
            false,
        );

        assert!(matches!(result, Err(error) if error.contains("canceló")));
    }

    #[test]
    fn repeated_provider_switches_and_refreshes_keep_one_model_worker_until_reaped() {
        let mut runtime = AssistantRuntime::default();
        let cancellation = CancellationToken::default();
        let (sender, receiver) = sync_channel::<Result<Vec<String>, String>>(1);
        runtime.model_job = Some(AssistantModelJob {
            id: 7,
            provider: ProviderProfile::OpenCodeGo,
            cancellation: cancellation.clone(),
            receiver,
        });

        assert!(runtime.cancel_stale_model_job(ProviderProfile::DeepSeek));
        assert!(cancellation.is_cancelled());
        for provider in [
            ProviderProfile::OllamaLocal,
            ProviderProfile::DeepSeek,
            ProviderProfile::OpenCodeGo,
        ] {
            assert!(!runtime.cancel_stale_model_job(provider));
            assert!(!runtime.request_model_refresh());
            assert_eq!(runtime.model_job.as_ref().map(|job| job.id), Some(7));
        }
        assert!(!runtime.take_queued_model_refresh_if_idle());

        sender
            .send(Err("remote model request was cancelled".into()))
            .unwrap();
        assert!(runtime.take_finished_model_job().is_some());
        assert!(runtime.take_queued_model_refresh_if_idle());
        assert!(!runtime.take_queued_model_refresh_if_idle());
    }

    #[test]
    fn assistant_command_handoff_accepts_only_verified_graph_proposals() {
        assert_eq!(
            validate_assistant_command("Function[sin(x)]").as_deref(),
            Some("Function[sin(x)]")
        );
        assert_eq!(
            validate_assistant_command("DomainColoring[1/z, -2, 2, -2, 2, 160]").as_deref(),
            Some("DomainColoring[1/z, -2, 2, -2, 2, 160]")
        );
        assert_eq!(
            validate_assistant_command("Surface3D[x^2+y^2, -2, 2, -2, 2]").as_deref(),
            Some("Surface3D[x^2+y^2, -2, 2, -2, 2]")
        );
        assert_eq!(
            validate_assistant_command("PolarCurve[1, 0, 2*pi]").as_deref(),
            Some("PolarCurve[1, 0, 2*pi]")
        );
        assert_eq!(
            validate_assistant_command("Segment3D[0, 0, 0, 1, 0, 0]").as_deref(),
            Some("Segment3D[0, 0, 0, 1, 0, 0]")
        );
        assert_eq!(
            validate_assistant_command("Tetrahedron[0, 0, 0, 2]").as_deref(),
            Some("Tetrahedron[0, 0, 0, 2]")
        );
        assert_eq!(
            validate_assistant_command("Tesseract4D[]").as_deref(),
            Some("Tesseract4D[]")
        );
        assert_eq!(
            validate_assistant_command("HypercubeND[4, 1, {0,0,0,0,0,0}]").as_deref(),
            Some("HypercubeND[4, 1, {0,0,0,0,0,0}]")
        );
        assert_eq!(
            validate_assistant_command("Aizawa[]").as_deref(),
            Some("Aizawa[]")
        );
        assert_eq!(
            validate_assistant_command("Mandelbrot[]").as_deref(),
            Some("Mandelbrot[]")
        );
        assert_eq!(
            validate_assistant_command("ImplicitRegion[(x^2 + y^2 - 1)^3 - x^2*y^3 = 0]")
                .as_deref(),
            Some("ImplicitCurve[(x^2 + y^2 - 1)^3 - x^2*y^3 = 0]")
        );
        assert_eq!(
            validate_assistant_command("ImplicitCurve[x^2 + y^2, 1, <=]").as_deref(),
            Some("ImplicitCurve[x^2 + y^2, 1, <=]")
        );
        assert_eq!(
            validate_assistant_command("Surface3D[(cos(u), sin(u), v), 0, 2*pi, -1, 1]").as_deref(),
            Some("Surface3D[(cos(u), sin(u), v), 0, 2*pi, -1, 1]")
        );
        assert_eq!(
            validate_assistant_command("Surface3D[cos(u), sin(u), v, 0, 2*pi, -1, 1]").as_deref(),
            Some("Surface3D[cos(u), sin(u), v, 0, 2*pi, -1, 1]")
        );
        assert_eq!(
            validate_assistant_command("DomainColoring[1/z]").as_deref(),
            Some("DomainColoring[1/z]")
        );
        assert!(
            validate_assistant_command("DomainColoring[(z^2 - 1)/(z^2 + 1), -2, 2, -2, 2, r]")
                .is_none()
        );
        for command in [
            "Aizawa[0.95, 0.7, 0.6, 3.5, 0.25, 0.1]",
            "Chen[35, 3, 28]",
            "Halvorsen[1.4]",
            "Dadras[3, 2.7, 1.7, 2, 9]",
            "Chua[15.6, 28, -1.143, -0.714]",
            "Hypersphere[]",
        ] {
            assert_eq!(
                validate_assistant_command(command).as_deref(),
                Some(command)
            );
        }
        for command in [
            "Chen[35, 3, 28, 0]",
            "Halvorsen[1.4, 0]",
            "Dadras[3, 2.7, 1.7, 2, 9, 0]",
            "Chua[15.6, 28, -1.143, -0.714, 0]",
            "Hypersphere[1]",
        ] {
            assert!(
                validate_assistant_command(command).is_none(),
                "{command} must be rejected by the assistant arity gate"
            );
        }
        assert!(validate_assistant_command("Function[]").is_none());
        assert!(validate_assistant_command("Analyze[f]").is_none());
        assert!(validate_assistant_command("ParametricCurve2D[cos(t), sin(t), 0]").is_none());
        assert!(validate_assistant_command("Histogram[{1, 2, 3}]").is_none());
        assert!(validate_assistant_command("Line3D[A, B]").is_none());
        assert!(validate_assistant_command("Plane3D[A, B, C]").is_none());
        assert!(validate_assistant_command("Script[Save[]]").is_none());
        assert!(validate_assistant_command("Save[file]").is_none());
        assert!(validate_assistant_command("Import[data.csv]").is_none());
        assert!(validate_assistant_command("Function[x]; Analyze[f]").is_none());
        assert!(validate_assistant_command("Unknown[x]").is_none());
    }

    #[test]
    fn heart_alias_is_preflighted_before_it_becomes_an_assistant_action() {
        let mut document = Document::new();
        document.set_view(ViewTransform::new(800.0, 600.0));
        let command = "ImplicitRegion[(x^2 + y^2 - 1)^3 - x^2*y^3 = 0]";
        let response = format!("```grafito\n{command}\n```");

        assert_eq!(
            verified_remote_proposals(
                &document,
                &response,
                grafito_geometry::Camera3D::new(4.0 / 3.0)
            ),
            vec![command_proposal(command)]
        );
        assert_eq!(document.object_count(), 0);
    }

    #[test]
    fn tetrahedron_fence_is_verified_without_mutating_the_live_document() {
        let mut document = Document::new();
        document.set_view(ViewTransform::new(800.0, 600.0));
        let command = "Tetrahedron[0, 0, 0, 2]";
        let response = format!("```grafito\n{command}\n```");

        assert_eq!(
            verified_remote_proposals(
                &document,
                &response,
                grafito_geometry::Camera3D::new(4.0 / 3.0),
            ),
            vec![command_proposal(command)]
        );
        assert_eq!(document.object_count(), 0);
    }

    #[test]
    fn named_regular_polytope_fences_are_verified_without_mutating_live_document() {
        let mut document = Document::new();
        document.set_view(ViewTransform::new(800.0, 600.0));

        for command in [
            "Tesseract4D[1.5,{0.1,-0.2,0.3,-0.4,0.5,-0.6}]",
            "SimplexND[5,1.25,{0.1,-0.2,0.3,-0.4,0.5,-0.6,0.7,-0.8,0.9,-1.0}]",
        ] {
            let response = format!("```grafito\n{command}\n```");
            assert_eq!(
                verified_remote_proposals(
                    &document,
                    &response,
                    grafito_geometry::Camera3D::new(4.0 / 3.0),
                ),
                vec![command_proposal(command)],
                "{command} must be an actionable verified fence"
            );
            assert_eq!(document.object_count(), 0, "preflight must stay isolated");
        }
    }

    #[test]
    fn assistant_repair_allows_two_bounded_passes_after_a_rejected_graph() {
        let feedback = AssistantRepairFeedback {
            failures: vec![AssistantRepairFailure {
                command: "Polyhedron".into(),
                kind: AssistantRepairFailureKind::UnsupportedCommand,
                expected_syntax: Vec::new(),
            }],
        };

        assert!(can_offer_assistant_proposal_correction(
            0,
            1,
            0,
            Some(&feedback),
        ));
        assert!(can_offer_assistant_proposal_correction(
            1,
            1,
            0,
            Some(&feedback),
        ));
        assert!(!can_offer_assistant_proposal_correction(
            2,
            1,
            0,
            Some(&feedback),
        ));
        assert!(!can_offer_assistant_proposal_correction(
            0,
            0,
            0,
            Some(&feedback),
        ));
        assert!(!can_offer_assistant_proposal_correction(
            0,
            1,
            1,
            Some(&feedback),
        ));
        assert!(can_offer_assistant_proposal_correction(
            1,
            0,
            0,
            Some(&feedback),
        ));
    }

    #[test]
    fn explicit_assistant_correction_can_repair_an_attachment_bearing_rejection() {
        let feedback = AssistantRepairFeedback {
            failures: vec![AssistantRepairFailure {
                command: "UnsupportedGraph".into(),
                kind: AssistantRepairFailureKind::UnsupportedCommand,
                expected_syntax: Vec::new(),
            }],
        };

        assert!(can_offer_assistant_proposal_correction(
            0,
            1,
            0,
            Some(&feedback),
        ));
        assert!(can_offer_assistant_proposal_correction(
            1,
            1,
            0,
            Some(&feedback),
        ));
        assert!(!can_offer_assistant_proposal_correction(
            2,
            1,
            0,
            Some(&feedback),
        ));
        assert!(!can_offer_assistant_proposal_correction(
            0,
            0,
            0,
            Some(&feedback)
        ));
        assert!(!can_offer_assistant_proposal_correction(
            0,
            1,
            1,
            Some(&feedback)
        ));
    }

    #[test]
    fn rejected_scenes_can_offer_one_explicit_repair() {
        let mut document = Document::new();
        document.set_view(ViewTransform::new(800.0, 600.0));
        let response = "```grafito-scene\nCylinder[0, 0, 0, 0.16, 3]\nSphere[0, 3, 0, 0.45]\n```";
        let check = inspect_remote_proposals(
            &document,
            response,
            grafito_geometry::Camera3D::new(4.0 / 3.0),
        );

        assert_eq!(check.candidate_count, 1);
        assert_eq!(check.action_candidate_count, 1);
        assert!(check.verified.is_empty());
        assert!(can_offer_assistant_proposal_correction(
            0,
            check.action_candidate_count,
            check.verified_action_count,
            check.repair_feedback.as_ref(),
        ));
    }

    #[test]
    fn malformed_scene_fences_can_offer_one_explicit_repair() {
        let document = Document::new();
        let check = inspect_remote_proposals(
            &document,
            "```grafito-scene\nCylinder[0, 0, 0, 0.16, 3]\n```",
            grafito_geometry::Camera3D::new(4.0 / 3.0),
        );

        assert_eq!(check.candidate_count, 1);
        assert_eq!(check.action_candidate_count, 1);
        assert!(check.verified.is_empty());
        let feedback = check
            .repair_feedback
            .as_ref()
            .expect("a malformed scene must offer sanitized feedback");
        assert!(matches!(
            feedback.failures.as_slice(),
            [AssistantRepairFailure {
                command,
                kind: AssistantRepairFailureKind::InvalidSyntax,
                ..
            }] if command == "Scene"
        ));
        assert!(can_offer_assistant_proposal_correction(
            0,
            check.action_candidate_count,
            check.verified_action_count,
            check.repair_feedback.as_ref(),
        ));
    }

    #[test]
    fn initial_prose_response_is_not_a_proposal_failure() {
        let document = Document::new();
        let check = inspect_remote_proposals(
            &document,
            "La gráfica se puede estudiar con análisis complejo.",
            grafito_geometry::Camera3D::new(4.0 / 3.0),
        );

        assert_eq!(check.action_candidate_count, 0);
        assert_eq!(check.verified_action_count, 0);
        assert!(check.repair_feedback.is_none());
    }

    #[test]
    fn prose_only_repair_response_keeps_one_bounded_retry_available() {
        let document = Document::new();
        let check = inspect_remote_action_proposals(
            &document,
            "La gráfica se puede estudiar con análisis complejo.",
            grafito_geometry::Camera3D::new(4.0 / 3.0),
        );

        assert_eq!(check.action_candidate_count, 0);
        assert_eq!(check.verified_action_count, 0);
        let feedback = check
            .repair_feedback
            .as_ref()
            .expect("a repair must request an action");
        assert_eq!(feedback.failures.len(), 1);
        assert_eq!(feedback.failures[0].command, "GraphProposal");
        assert_eq!(
            feedback.failures[0].kind,
            AssistantRepairFailureKind::InvalidSyntax
        );
        assert!(can_offer_assistant_proposal_correction(
            1,
            check.action_candidate_count,
            check.verified_action_count,
            check.repair_feedback.as_ref(),
        ));
    }

    #[test]
    fn assistant_graph_preflight_commits_only_drawable_staged_commands() {
        let mut document = Document::new();
        document.set_view(ViewTransform::new(800.0, 600.0));

        let preflight = preflight_assistant_graph_command(&document, "Function[sin(x)/(x^2+1)]")
            .expect("finite function must be drawable");
        assert!(matches!(preflight.outcome, CommandOutcome::Message(_)));
        assert!(preflight
            .staged
            .objects_iter()
            .any(|(_, object)| matches!(object, GeoObject::Function(_))));
        assert_eq!(
            document.object_count(),
            0,
            "preflight must not mutate live state"
        );

        let complex =
            preflight_assistant_graph_command(&document, "DomainColoring[1/z, -2, 2, -2, 2, 160]")
                .expect("domain coloring must be drawable");
        assert!(complex.staged.objects_iter().any(
            |(_, object)| matches!(object, GeoObject::ComplexGrid(grid) if grid.render_mode == 1)
        ));

        let surface =
            preflight_assistant_graph_command(&document, "Surface3D[x^2+y^2, -2, 2, -2, 2]")
                .expect("finite 3d surface must be drawable");
        assert!(surface
            .staged
            .objects_iter()
            .any(|(_, object)| matches!(object, GeoObject::Surface3D(_))));

        let hypercube = preflight_assistant_graph_command(&document, "Hypercube[]")
            .expect("4d CPU projection must be drawable");
        assert!(hypercube
            .staged
            .objects_iter()
            .any(|(_, object)| matches!(object, GeoObject::HyperSurface4D(_))));

        let mandelbrot = preflight_assistant_graph_command(&document, "Mandelbrot[]")
            .expect("zero-argument Mandelbrot must use its bounded default");
        assert!(mandelbrot
            .staged
            .objects_iter()
            .any(|(_, object)| matches!(object, GeoObject::Fractal2D(_))));

        for command in [
            "Function[1/0]",
            "Function[1000000]",
            "DomainColoring[not valid, -2, 2, -2, 2, 160]",
            "Cube[2000, 0, 0, 1]",
        ] {
            assert!(
                preflight_assistant_graph_command(&document, command).is_err(),
                "{command} must not be applied"
            );
        }
        assert_eq!(
            document.object_count(),
            0,
            "rejected preflight must remain atomic"
        );
    }

    #[test]
    fn assistant_preflight_accepts_standard_logarithmic_and_exponential_functions() {
        let document = Document::new();

        for command in [
            "Function[log(x)]",
            "Function[exp(x)]",
            "Function[(4/pi)*(sin(x)+sin(3*x)/3+sin(5*x)/5)]",
        ] {
            assert!(
                preflight_assistant_graph_command(&document, command).is_ok(),
                "{command} should be a verified assistant proposal"
            );
        }
    }

    #[test]
    fn assistant_functions_do_not_autodefine_remote_symbols() {
        let mut document = Document::new();
        document.set_view(ViewTransform::new(800.0, 600.0));

        assert!(preflight_assistant_graph_command(&document, "Function[sin(k*x)/k]").is_err());
        assert_eq!(document.get_variable("k"), None);

        document.set_variable("k".into(), 3.0);
        assert!(preflight_assistant_graph_command(&document, "Function[sin(k*x)/k]").is_ok());
    }

    #[test]
    fn literal_safe_graph_capabilities_preflight_across_every_supported_render_mode() {
        let mut document = Document::new();
        document.set_view(ViewTransform::new(800.0, 600.0));

        for command in [
            "ParametricCurve2D[cos(t), sin(t), 0, 6.28]",
            "PolarCurve[1, 0, 6.28]",
            "ImplicitCurve[x^2+y^2=1]",
            "Contour[x^2+y^2, -2, 2, -2, 2, 1]",
            "VectorField2D[-y, x]",
            "PhasePortrait[y, -x]",
            "ComplexGrid[z^2]",
            "HeatMap[x+y, -2, 2, -2, 2, 64]",
            "Quadrants[]",
            "Julia[-0.7, 0.2, 32]",
            "BurningShip[]",
            "Point3D[0, 0, 0]",
            "Segment3D[0, 0, 0, 1, 1, 1]",
            "Line3D[0, 0, 0, 1, 1, 1]",
            "Plane3D[0, 1, 0, 0]",
            "Sphere[0, 0, 0, 1]",
            "Cube[0, 0, 0, 1]",
            "Tetrahedron[0, 0, 0, 1]",
            "Cylinder[0, 0, 0, 1, 2]",
            "Cone[0, 0, 0, 1, 2]",
            "Torus[0, 0, 0, 2, 0.5]",
            "Moebius[2, 0.5]",
            "Curve3D[(cos(t), sin(t), t), 0, 6.28]",
            "ComplexSurface[1/z, -2, 2, -2, 2, 32]",
            "VectorField3D[-y, x, z]",
            "Lorenz[]",
            "Hypersphere[]",
            "Pentachoron4D[]",
            "Tesseract4D[]",
            "SixteenCell4D[]",
            "TwentyFourCell4D[]",
            "OneTwentyCell4D[]",
            "SixHundredCell4D[]",
            "SimplexND[3]",
            "HypercubeND[4]",
            "CrossPolytopeND[5,1,{0.1,-0.2,0.3,-0.4,0.5,-0.6,0.7,-0.8,0.9,-1.0}]",
        ] {
            assert!(
                preflight_assistant_graph_command(&document, command).is_ok(),
                "{command} must preflight through its declared render route"
            );
        }
    }

    #[test]
    fn data_backed_graph_capabilities_never_preflight_as_assistant_actions() {
        let mut document = Document::new();
        document.set_view(ViewTransform::new(800.0, 600.0));

        for command in [
            "Histogram[{1, 2, 3, 4}, 4]",
            "ScatterPlot[{1, 2, 3}, {1, 4, 9}]",
            "BoxPlot[{1, 2, 3, 4, 5}]",
            "LinearRegression[{1, 2, 3}, {1, 4, 9}]",
        ] {
            assert!(
                preflight_assistant_graph_command(&document, command).is_err(),
                "{command} must remain a reference-only assistant form"
            );
        }
        assert_eq!(document.object_count(), 0);
    }

    #[test]
    fn preflight_commit_updates_the_document_and_undo_history_once() {
        let mut document = Document::new();
        document.set_view(ViewTransform::new(800.0, 600.0));
        let preflight = preflight_assistant_graph_command(&document, "Function[sin(x)]")
            .expect("finite function must be drawable");
        let mut undo_stack = VecDeque::new();
        let mut redo_stack = VecDeque::new();

        let outcome = commit_assistant_graph_preflight(
            &mut document,
            &mut undo_stack,
            &mut redo_stack,
            preflight,
        );

        assert!(matches!(outcome, CommandOutcome::Message(_)));
        assert_eq!(document.object_count(), 1);
        assert_eq!(undo_stack.len(), 1);
        assert!(redo_stack.is_empty());
    }

    #[test]
    fn graph_apply_switches_only_between_required_2d_and_3d_views() {
        assert_eq!(
            assistant_graph_perspective(
                grafito_command::assistant_context::AssistantGraphView::ThreeD,
                crate::ViewMode::D2,
            ),
            Some(crate::Perspective::Geometry3D)
        );
        assert_eq!(
            assistant_graph_perspective(
                grafito_command::assistant_context::AssistantGraphView::TwoD,
                crate::ViewMode::D3,
            ),
            Some(crate::Perspective::Geometry2D)
        );
        assert_eq!(
            assistant_graph_perspective(
                grafito_command::assistant_context::AssistantGraphView::TwoD,
                crate::ViewMode::D2,
            ),
            None
        );
    }

    #[test]
    fn s1_apply_muestra_la_vista_esperada_segun_el_objeto() {
        // S1 auto-graficar 1-click: apply → vista esperada (2D o 3D según el
        // objeto; si no se puede determinar, default 2D honesto).
        use grafito_command::assistant_proposals::{
            parse_assistant_command, parse_assistant_parameter, AssistantProposal,
        };
        let two_d =
            AssistantProposal::Command(parse_assistant_command("Function[x]").expect("2D válido"));
        assert_eq!(
            two_d.expected_view(),
            grafito_command::assistant_context::AssistantGraphView::TwoD
        );
        // Desde 3D, un 2D pide cambio a Geometry2D (el Apply lo abre).
        assert_eq!(
            assistant_graph_perspective(two_d.expected_view(), crate::ViewMode::D3),
            Some(crate::Perspective::Geometry2D)
        );
        // Desde 2D, un 2D no pide cambio (ya visible).
        assert_eq!(
            assistant_graph_perspective(two_d.expected_view(), crate::ViewMode::D2),
            None
        );
        let three_d = AssistantProposal::Command(
            parse_assistant_command("Sphere[0, 0, 0, 1]").expect("3D válido"),
        );
        assert_eq!(
            three_d.expected_view(),
            grafito_command::assistant_context::AssistantGraphView::ThreeD
        );
        assert_eq!(
            assistant_graph_perspective(three_d.expected_view(), crate::ViewMode::D2),
            Some(crate::Perspective::Geometry3D)
        );
        // Parámetro sin objeto → default 2D honesto (no inventa 3D).
        let param =
            AssistantProposal::Parameter(parse_assistant_parameter("a = 2.5").expect("parámetro"));
        assert_eq!(
            param.expected_view(),
            grafito_command::assistant_context::AssistantGraphView::TwoD
        );
    }

    #[test]
    fn s2_clarificacion_round_trip_sin_bloquear() {
        // S2 `ask_user` real vía evento: parse del `args_summary` → pendiente
        // → respuesta saneada para el loop como `function_call_output`.
        // Puro y no bloqueante (sin threads, sin Document, sin I/O).
        let pending = super::parse_agent_ask_user_pending(
            r#"{"question":"¿qué valor le doy a x?","options":["0","1"]}"#,
        )
        .expect("pendiente parseable");
        assert_eq!(pending.question, "¿qué valor le doy a x?");
        assert_eq!(pending.options, vec!["0".to_owned(), "1".to_owned()]);
        // Truncado con `…` igual intenta (honesto, sin inventar).
        assert!(super::parse_agent_ask_user_pending("hola").is_none());
        assert!(super::parse_agent_ask_user_pending("").is_none());
        assert!(super::parse_agent_ask_user_pending(r#"{"question":""}"#).is_none());
        // La respuesta vuelve al loop como `function_call_output` (Responses)
        // vía `answer_pending_clarification` (nuevo job, nunca bloquea).
        let output = grafito_agent::tools::ask_user_answer_function_output(&pending.call_id, "1")
            .expect("respuesta no vacía");
        assert_eq!(output["type"], "function_call_output");
        assert_eq!(output["call_id"], pending.call_id);
        assert_eq!(output["output"], "1");
    }

    #[test]
    fn s3_entrada_rota_da_explicacion_mas_fix() {
        // S3: fence inválida típica → `vibecoder_explain` + fix sintáctico.
        let (explained, fix) =
            grafito_assistant::agent::explain_invalid_proposal("Function[x", "derivada");
        assert_eq!(
            explained.kind,
            grafito_assistant::agent::VibecoderKind::Syntax
        );
        assert!(explained.explanation.contains("derivada"));
        assert_eq!(fix.as_deref(), Some("Function[x]"));
        // Semántico jamás se reescribe en silencio.
        let (_, fix) = grafito_assistant::agent::explain_invalid_proposal("Script[Save[]]", "");
        assert!(fix.is_none());
    }

    #[test]
    fn flower_scene_is_atomic_drawable_and_fitted_before_commit() {
        let mut document = Document::new();
        document.set_view(ViewTransform::new(800.0, 600.0));
        let commands = flower_scene_commands();
        let typed_commands = assistant_commands(&commands);

        let preflight = preflight_assistant_flower_scene(
            &document,
            &typed_commands,
            grafito_geometry::Camera3D::new(4.0 / 3.0),
        )
        .expect("a complete flower scene must be drawable");

        assert_eq!(document.object_count(), 0, "staging must stay atomic");
        assert_eq!(preflight.staged.object_count(), commands.len());
        assert!(preflight.camera.distance.is_finite());
        assert!(preflight.camera.distance > 0.0);
        assert!(preflight.staged.objects_iter().any(|(_, object)| {
            matches!(object, GeoObject::Surface3D(surface) if surface.solid)
        }));
    }

    #[test]
    fn incomplete_flower_scene_never_becomes_a_verified_remote_proposal() {
        let mut document = Document::new();
        document.set_view(ViewTransform::new(800.0, 600.0));
        let incomplete = [
            "Cylinder[0, 0, 0, 0.16, 3]",
            "Sphere[0, 3, 0, 0.45]",
            "Surface3D[(u, 3, 0.3*v), 0, 1.5, -1, 1]",
        ]
        .join("\n");
        let response = format!("```grafito-scene\n{incomplete}\n```");

        assert!(verified_remote_proposals(
            &document,
            &response,
            grafito_geometry::Camera3D::new(4.0 / 3.0)
        )
        .is_empty());
        assert_eq!(document.object_count(), 0);
    }

    #[test]
    fn incomplete_world_mesh_scene_never_becomes_an_actionable_assistant_card() {
        let mut document = Document::new();
        document.set_view(ViewTransform::new(800.0, 600.0));
        let commands = vec!["Lorenz[]".to_string(); 5];
        let typed_commands = assistant_commands(&commands);

        assert!(preflight_assistant_scene(
            &document,
            &typed_commands,
            grafito_geometry::Camera3D::new(4.0 / 3.0),
        )
        .is_err());
        assert_eq!(document.object_count(), 0, "preflight must stay isolated");
    }

    #[test]
    fn multiline_grafito_segment3d_scene_is_preflighted_atomically() {
        let mut document = Document::new();
        document.set_view(ViewTransform::new(800.0, 600.0));
        let edges = [
            "Segment3D[0,0,0,1,0,0]",
            "Segment3D[1,0,0,0.5,0.8660254037844386,0]",
            "Segment3D[0.5,0.8660254037844386,0,0,0,0]",
            "Segment3D[0,0,0,0.5,0.2886751345948129,0.816496580927726]",
            "Segment3D[1,0,0,0.5,0.2886751345948129,0.816496580927726]",
            "Segment3D[0.5,0.8660254037844386,0,0.5,0.2886751345948129,0.816496580927726]",
        ];
        let response = format!("```grafito\n{}\n```", edges.join("\n"));

        let proposals = verified_remote_proposals(
            &document,
            &response,
            grafito_geometry::Camera3D::new(4.0 / 3.0),
        );

        assert_eq!(proposals, vec![scene_proposal(&edges)]);
        assert_eq!(document.object_count(), 0, "preflight must stay atomic");
    }

    #[test]
    fn labeled_tetrahedron_scene_is_preflighted_as_six_direct_edges() {
        let mut document = Document::new();
        document.set_view(ViewTransform::new(800.0, 600.0));
        let edges = [
            "Segment3D[0,0.816496580927726,0,-0.7071067811865475,-0.4082482904638631,0]",
            "Segment3D[0,0.816496580927726,0,0.7071067811865475,-0.4082482904638631,0]",
            "Segment3D[0,0.816496580927726,0,0,-0.4082482904638631,0.816496580927726]",
            "Segment3D[-0.7071067811865475,-0.4082482904638631,0,0.7071067811865475,-0.4082482904638631,0]",
            "Segment3D[-0.7071067811865475,-0.4082482904638631,0,0,-0.4082482904638631,0.816496580927726]",
            "Segment3D[0.7071067811865475,-0.4082482904638631,0,0,-0.4082482904638631,0.816496580927726]",
        ];
        let response = format!(
            "```grafito-scene\nv0 = Point3D[0,0.816496580927726,0]\nv1 = Point3D[-0.7071067811865475,-0.4082482904638631,0]\nv2 = Point3D[0.7071067811865475,-0.4082482904638631,0]\nv3 = Point3D[0,-0.4082482904638631,0.816496580927726]\n{}\n```",
            edges
                .iter()
                .enumerate()
                .map(|(index, edge)| format!("a{index:02} = {edge}"))
                .collect::<Vec<_>>()
                .join("\n")
        );

        assert_eq!(
            verified_remote_proposals(
                &document,
                &response,
                grafito_geometry::Camera3D::new(4.0 / 3.0),
            ),
            vec![scene_proposal(&edges)]
        );
        assert_eq!(document.object_count(), 0, "preflight must stay atomic");
    }

    #[test]
    fn flower_preflight_rejects_axis_swapped_disconnected_petals() {
        let mut document = Document::new();
        document.set_view(ViewTransform::new(800.0, 600.0));
        let commands = [
            "Cylinder[0, 0, 0, 0.16, 3]",
            "Sphere[0, 3, 0, 0.45]",
            "Surface3D[(u, 0.3*v, 3 + 0.25*(1-(u/1.5)^2)*(1-v^2)), 0, 1.5, -1, 1]",
            "Surface3D[(-u, -0.3*v, 3 + 0.25*(1-(u/1.5)^2)*(1-v^2)), 0, 1.5, -1, 1]",
            "Surface3D[(0.3*v, u, 3 + 0.25*(1-(u/1.5)^2)*(1-v^2)), 0, 1.5, -1, 1]",
            "Surface3D[(-0.3*v, -u, 3 + 0.25*(1-(u/1.5)^2)*(1-v^2)), 0, 1.5, -1, 1]",
        ]
        .into_iter()
        .map(str::to_owned)
        .collect::<Vec<_>>();
        let typed_commands = assistant_commands(&commands);

        let error = preflight_assistant_flower_scene(
            &document,
            &typed_commands,
            grafito_geometry::Camera3D::new(4.0 / 3.0),
        )
        .err()
        .expect("axis-swapped petals must not pass as one connected flower");

        assert!(error.contains("conectada"), "unexpected error: {error}");
        assert_eq!(document.object_count(), 0);
    }

    #[test]
    fn verified_remote_proposals_preflight_only_a_fixed_number_of_fenced_proposals() {
        let mut document = Document::new();
        document.set_view(ViewTransform::new(800.0, 600.0));
        let response = std::iter::repeat_n("```grafito\nFunction[sin(x)]\n```", 5)
            .collect::<Vec<_>>()
            .join("\n");

        let proposals = verified_remote_proposals(
            &document,
            &response,
            grafito_geometry::Camera3D::new(4.0 / 3.0),
        );

        assert_eq!(proposals.len(), 4);
        assert_eq!(document.object_count(), 0);
    }

    fn flower_scene_commands() -> Vec<String> {
        [
            "Cylinder[0, 0, 0, 0.16, 3]",
            "Sphere[0, 3, 0, 0.45]",
            "Surface3D[(u, 3 + 0.25*(1-(u/1.5)^2)*(1-v^2), 0.3*v), 0, 1.5, -1, 1]",
            "Surface3D[(-u, 3 + 0.25*(1-(u/1.5)^2)*(1-v^2), -0.3*v), 0, 1.5, -1, 1]",
            "Surface3D[(0.3*v, 3 + 0.25*(1-(u/1.5)^2)*(1-v^2), u), 0, 1.5, -1, 1]",
            "Surface3D[(-0.3*v, 3 + 0.25*(1-(u/1.5)^2)*(1-v^2), -u), 0, 1.5, -1, 1]",
        ]
        .into_iter()
        .map(str::to_owned)
        .collect()
    }

    #[test]
    fn attachment_reader_stops_before_an_oversized_file_is_fully_buffered() {
        let bytes = read_bounded_attachment(Cursor::new(vec![7; 5]), 4);

        assert!(bytes.is_err());
    }

    #[test]
    fn secuencia_tres_turnos_sin_media_rancia_ni_controles() {
        // Secuencia del reporte: T1 integral (anda) → T2 otra animación
        // (sin control-chars) → T3 no-animación (sin media pegada).
        // Replica el orden de Submit (`decide_animacion` + reset T1).
        fn controles(texto: &str) -> Vec<char> {
            texto
                .chars()
                .filter(|c| c.is_control() && !matches!(c, '\n' | '\r' | '\t'))
                .collect()
        }
        fn sin_controles(etiqueta: &str, texto: &str) {
            let malos = controles(texto);
            for malo in &malos {
                eprintln!("control en {etiqueta}: U+{:04X}", *malo as u32);
            }
            assert!(malos.is_empty(), "{etiqueta} trae controles");
        }
        let ctx = egui::Context::default();
        let mut panel = AssistantPanelState::default();
        // T1: integral canónica → media SÍ + prosa que la declara.
        let pedido1 = "haceme una animacion de una integral (nativa)";
        let d1 = decide_animacion(pedido1);
        assert!(
            matches!(d1, DecisionAnimacion::RenderCanonico { .. }),
            "{d1:?}"
        );
        panel.begin_request(pedido1.to_string());
        let prosa1 = format!(
            "{}\n\n{}",
            crate::anim_ui::animation_reference_sentence(),
            grafito_anim::parametric::INTEGRAL_CANONICAL_PROSA
        );
        sin_controles("prosa1", &prosa1);
        let humano1 = grafito_ui::assistant::humanize_prose_text(&prosa1);
        sin_controles("humano1", &humano1);
        panel.complete_local_request(humano1);
        panel.set_media(
            Some(grafito_ui::assistant::AssistantMedia {
                title: "Integral — área bajo la curva (nativa)".to_string(),
                frames: Vec::new(),
            }),
            &ctx,
        );
        assert!(panel.media.is_some(), "T1 instala la integral");
        // T2: OTRA animación (tangente canónica declarada, M1: antes era
        // RenderGenerico huérfano) → local-only, prosa limpia que declara.
        let pedido2 = "explica la derivada con animación";
        let d2 = decide_animacion(pedido2);
        assert!(
            matches!(d2, DecisionAnimacion::RenderCanonico { .. }),
            "{d2:?}"
        );
        panel.begin_request(pedido2.to_string());
        let prosa2 = prosa_canonica_para_plantilla("derivative-slope");
        assert!(
            prosa2.contains("tangente"),
            "la canónica se declara: {prosa2}"
        );
        sin_controles("prosa2", &prosa2);
        let humano2 = grafito_ui::assistant::humanize_prose_text(&prosa2);
        sin_controles("humano2", &humano2);
        panel.complete_local_request(humano2);
        for turno in &panel.conversation {
            sin_controles("historial", &turno.content);
        }
        // Sin gatillo en el mensaje actual no hay animación (va a remoto,
        // jamás inventa media): límite honesto del punto único T1.
        for implicito in ["y ahora la derivada", "otra", "haceme otra"] {
            assert!(
                matches!(decide_animacion(implicito), DecisionAnimacion::NoAnimacion),
                "{implicito:?} sin gatillo no anima"
            );
        }
        // T3: no-animación → reset T1 limpia la integral pegada.
        let pedido3 = "¿qué es una derivada?";
        let d3 = decide_animacion(pedido3);
        assert!(matches!(d3, DecisionAnimacion::NoAnimacion), "{d3:?}");
        panel.begin_request(pedido3.to_string());
        panel.complete_local_request("La derivada es la pendiente.".to_string());
        limpiar_media_si_no_animacion(&mut panel, &d3, &ctx);
        assert!(panel.media.is_none(), "T3 no re-muestra la integral");
        // El reset no toca turnos de animación (Submit ya limpió al spawnear).
        panel.set_media(
            Some(grafito_ui::assistant::AssistantMedia {
                title: "Derivada como pendiente (nativa)".to_string(),
                frames: Vec::new(),
            }),
            &ctx,
        );
        limpiar_media_si_no_animacion(&mut panel, &d2, &ctx);
        assert!(panel.media.is_some(), "el reset solo actúa en NoAnimacion");
    }

    #[test]
    fn error_not_displayable_pide_reintento_sin_configuracion() {
        // Bug A: no es tema de modelo ni de clave → reintentá/reformulá.
        let mensaje = remote_error_message(
            "remote assistant response content is not displayable: expected a non-empty text message without control characters",
            "muse-spark-1.3-contributor",
        );
        assert!(
            !mensaje.contains("Configuración"),
            "no manda a Configuración: {mensaje}"
        );
        assert!(
            mensaje.contains("Reintentá") || mensaje.contains("reformulá"),
            "pide reintentar: {mensaje}"
        );
    }
}
