//! Reconocimiento estricto de propuestas remotas del asistente.
//!
//! Este módulo convierte bloques fenced no confiables en invocaciones canónicas
//! ligadas al registro. Los frontends sólo reciben estas invocaciones tipadas;
//! el texto remoto original nunca llega al dispatcher como una acción directa.

use crate::{
    assistant_context::{
        assistant_command_has_literal_safe_arguments, assistant_command_has_literal_safe_form,
        assistant_graph_capability,
    },
    command_registry::{self, CommandSpec},
    commands::{self, CommandOutcome},
};
use grafito_core::Document;

const MAX_ASSISTANT_COMMAND_BYTES: usize = 1_024;
const MAX_ASSISTANT_PARAMETER_BYTES: usize = 128;
const MAX_ASSISTANT_SCENE_COMPONENTS: usize = 8;

/// Invocación gráfica validada contra una especificación registrada.
///
/// Sus campos permanecen privados para que sólo el reconocedor pueda crearla
/// desde contenido remoto. `canonical_text` se genera desde `CommandSpec`, no
/// reutiliza el encabezado recibido del proveedor.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AssistantCommandInvocation {
    spec: &'static CommandSpec,
    arguments: Vec<String>,
}

impl AssistantCommandInvocation {
    /// Especificación registrada que autorizó la invocación.
    pub fn spec(&self) -> &'static CommandSpec {
        self.spec
    }

    /// Nombre canónico del dispatcher para esta invocación.
    pub fn canonical_name(&self) -> &'static str {
        self.spec.canonical
    }

    /// Argumentos ya delimitados y acotados por el reconocedor.
    pub fn arguments(&self) -> &[String] {
        &self.arguments
    }

    /// Serialización canónica que puede entrar al dispatcher local.
    pub fn canonical_text(&self) -> String {
        format!("{}[{}]", self.spec.canonical, self.arguments.join(", "))
    }
}

/// Asignación escalar finita que precede opcionalmente a una propuesta gráfica.
#[derive(Debug, Clone, PartialEq)]
pub struct AssistantParameterAssignment {
    name: String,
    value: f64,
}

// `value` sólo se construye tras comprobar `is_finite`, por lo que no puede
// contener NaN y conserva una relación de igualdad reflexiva.
impl Eq for AssistantParameterAssignment {}

impl AssistantParameterAssignment {
    /// Nombre válido de la variable que se actualizará localmente.
    pub fn name(&self) -> &str {
        &self.name
    }

    /// Valor numérico finito que se actualizará localmente.
    pub fn value(&self) -> f64 {
        self.value
    }

    /// Serialización canónica para el único dispatcher local.
    pub fn canonical_text(&self) -> String {
        format!("{} = {}", self.name, self.value)
    }
}

/// Acción remota reconocida, pero aún no preflighted ni aplicada.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AssistantProposal {
    /// Un comando gráfico independiente.
    Command(AssistantCommandInvocation),
    /// Una escena 3D cuyos componentes se verifican y aplican atómicamente.
    Scene(Vec<AssistantCommandInvocation>),
    /// Una asignación finita para un parámetro local.
    Parameter(AssistantParameterAssignment),
}

impl AssistantProposal {
    /// Texto canónico para mostrar en una tarjeta de propuesta.
    pub fn canonical_text(&self) -> String {
        match self {
            Self::Command(command) => command.canonical_text(),
            Self::Scene(commands) => commands
                .iter()
                .map(AssistantCommandInvocation::canonical_text)
                .collect::<Vec<_>>()
                .join("\n"),
            Self::Parameter(assignment) => assignment.canonical_text(),
        }
    }

    /// Vista que debe mostrarse al aplicar (S1 auto-graficar 1-click).
    ///
    /// Pura y total: `Command` usa su capacidad verificada, `Scene` usa la
    /// vista del primer componente si todos comparten vista, y `Parameter`
    /// (sin objeto gráfico) cae a `TwoD` honesto (no cambia a 3D). Si no se
    /// puede determinar, default `TwoD` honesto.
    #[must_use]
    pub fn expected_view(&self) -> crate::assistant_context::AssistantGraphView {
        use crate::assistant_context::{assistant_graph_capability, AssistantGraphView};
        match self {
            Self::Command(command) => assistant_graph_capability(command.canonical_name())
                .map(|capability| capability.view)
                .unwrap_or(AssistantGraphView::TwoD),
            Self::Scene(commands) => {
                let mut view = None;
                for command in commands {
                    let current = assistant_graph_capability(command.canonical_name())
                        .map(|capability| capability.view);
                    match (view, current) {
                        (None, Some(current)) => view = Some(current),
                        (Some(expected), Some(current)) if expected == current => {}
                        _ => return AssistantGraphView::TwoD,
                    }
                }
                view.unwrap_or(AssistantGraphView::TwoD)
            }
            Self::Parameter(_) => AssistantGraphView::TwoD,
        }
    }

    /// Etiqueta corta de la vista (`2D`/`3D`) para la tarjeta (rioplatense, sin jerga).
    #[must_use]
    pub fn expected_view_label(&self) -> &'static str {
        use crate::assistant_context::AssistantGraphView;
        match self.expected_view() {
            AssistantGraphView::TwoD => "2D",
            AssistantGraphView::ThreeD => "3D",
        }
    }
}

/// Propuesta reconocida y su ubicación estable entre los bloques de código.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AssistantProposalCandidate {
    /// Índice entre todos los bloques fenced completos de la respuesta.
    pub code_block_index: usize,
    /// Acción tipada pendiente de preflight local, si superó el reconocimiento.
    pub proposal: Option<AssistantProposal>,
    /// Diagnóstico seguro de una acción fenced que no puede ejecutarse.
    pub rejection: Option<AssistantProposalRejection>,
}

impl AssistantProposalCandidate {
    /// Indica si el bloque representa una acción gráfica, incluso si fue rechazada.
    pub fn is_action_candidate(&self) -> bool {
        self.proposal.as_ref().is_some_and(|proposal| {
            matches!(
                proposal,
                AssistantProposal::Command(_) | AssistantProposal::Scene(_)
            )
        }) || self.rejection.is_some()
    }
}

/// Motivo saneado por el que un bloque fenced no puede producir una acción.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AssistantProposalRejection {
    /// Nombre del comando, nunca argumentos o contenido remoto completo.
    pub command: String,
    /// Clasificación local de la propuesta descartada.
    pub kind: AssistantProposalRejectionKind,
}

/// Clase estable de rechazo producida antes del preflight de la aplicación.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AssistantProposalRejectionKind {
    InvalidSyntax,
    UnsupportedCommand,
}

/// Reconoce una única invocación gráfica y la liga al registro y a la política.
pub fn parse_assistant_command(candidate: &str) -> Option<AssistantCommandInvocation> {
    let candidate = candidate.trim();
    if candidate.is_empty()
        || candidate.len() > MAX_ASSISTANT_COMMAND_BYTES
        || candidate.contains(['\n', '\r', ';'])
        || !is_complete_assistant_command(candidate)
    {
        return None;
    }

    let parsed = commands::parse_cas_command(candidate)?;
    let spec = command_registry::resolve(&parsed.command)?;
    if assistant_graph_capability(spec.canonical).is_none()
        || !assistant_command_has_literal_safe_form(spec.canonical, parsed.args.len())
        || !assistant_command_has_literal_safe_arguments(spec.canonical, &parsed.args)
        || !has_explicit_assistant_function_expression(spec.canonical, &parsed.args)
    {
        return None;
    }

    Some(AssistantCommandInvocation {
        spec,
        arguments: parsed.args,
    })
}

/// Reconoce una asignación escalar finita sin permitir expresiones o scripts.
pub fn parse_assistant_parameter(candidate: &str) -> Option<AssistantParameterAssignment> {
    let candidate = candidate.trim();
    if candidate.is_empty()
        || candidate.len() > MAX_ASSISTANT_PARAMETER_BYTES
        || candidate.contains(['\n', '\r', ';'])
    {
        return None;
    }
    let (name, value) = candidate.split_once('=')?;
    let name = name.trim();
    let value = value.trim().parse::<f64>().ok()?;
    (is_assistant_identifier(name) && value.is_finite()).then(|| AssistantParameterAssignment {
        name: name.into(),
        value,
    })
}

/// Extrae propuestas tipadas de los fenced blocks completos de una respuesta.
pub fn assistant_fenced_proposals(content: &str) -> Vec<AssistantProposalCandidate> {
    let lines = content.lines().collect::<Vec<_>>();
    let mut proposals = Vec::new();
    let mut index = 0;
    let mut code_block_index = 0;

    while index < lines.len() {
        let line = lines[index].trim();
        if let Some(language) = line.strip_prefix("```") {
            if let Some(end) = lines[index + 1..]
                .iter()
                .position(|candidate| candidate.trim_start().starts_with("```"))
            {
                let end = index + end + 1;
                let text = lines[index + 1..end].join("\n");
                if let Some(candidate) =
                    assistant_proposal_from_fence(&language.trim().to_ascii_lowercase(), &text)
                {
                    let (proposal, rejection) = match candidate {
                        RecognizedAssistantFence::Proposal(proposal) => (Some(proposal), None),
                        RecognizedAssistantFence::Rejected(rejection) => (None, Some(rejection)),
                    };
                    proposals.push(AssistantProposalCandidate {
                        code_block_index,
                        proposal,
                        rejection,
                    });
                }
                code_block_index += 1;
                index = end + 1;
                continue;
            }
        }
        index += 1;
    }

    proposals
}

/// Ejecuta sólo la serialización canónica de una invocación ya reconocida.
pub fn execute_assistant_command(
    document: &mut Document,
    invocation: &AssistantCommandInvocation,
) -> CommandOutcome {
    let mut input = invocation.canonical_text();
    commands::process_input(document, &mut input)
}

/// Ejecuta sólo la serialización canónica de un parámetro ya reconocido.
pub fn execute_assistant_parameter(
    document: &mut Document,
    assignment: &AssistantParameterAssignment,
) -> CommandOutcome {
    let mut input = assignment.canonical_text();
    commands::process_input(document, &mut input)
}

enum RecognizedAssistantFence {
    Proposal(AssistantProposal),
    Rejected(AssistantProposalRejection),
}

fn assistant_proposal_from_fence(language: &str, text: &str) -> Option<RecognizedAssistantFence> {
    match language {
        "grafito" => parse_assistant_command(text)
            .map(AssistantProposal::Command)
            .or_else(|| assistant_scene_proposal(text))
            .map(RecognizedAssistantFence::Proposal)
            .or_else(|| rejected_assistant_command(text))
            .or_else(|| rejected_assistant_scene(text)),
        "grafito-scene" => assistant_scene_proposal(text)
            .or_else(|| assistant_labeled_tetrahedron_scene_proposal(text))
            .map(RecognizedAssistantFence::Proposal)
            .or_else(|| rejected_assistant_scene(text)),
        "grafito-param" => parse_assistant_parameter(text)
            .map(AssistantProposal::Parameter)
            .map(RecognizedAssistantFence::Proposal),
        _ => None,
    }
}

fn rejected_assistant_command(text: &str) -> Option<RecognizedAssistantFence> {
    let command = text.trim();
    if command.lines().count() != 1 || !is_complete_assistant_command(command) {
        return None;
    }
    let name = command.split_once('[')?.0.trim();
    let kind = command_registry::resolve(name)
        .filter(|spec| assistant_graph_capability(spec.canonical).is_some())
        .map_or(AssistantProposalRejectionKind::UnsupportedCommand, |_| {
            AssistantProposalRejectionKind::InvalidSyntax
        });
    Some(RecognizedAssistantFence::Rejected(
        AssistantProposalRejection {
            command: name.into(),
            kind,
        },
    ))
}

fn rejected_assistant_scene(text: &str) -> Option<RecognizedAssistantFence> {
    (!text.trim().is_empty()).then(|| {
        RecognizedAssistantFence::Rejected(AssistantProposalRejection {
            command: "Scene".into(),
            kind: AssistantProposalRejectionKind::InvalidSyntax,
        })
    })
}

fn assistant_scene_proposal(text: &str) -> Option<AssistantProposal> {
    let commands = text
        .lines()
        .map(str::trim)
        .filter(|line| !line.is_empty())
        .map(parse_assistant_command)
        .collect::<Option<Vec<_>>>()?;
    ((2..=MAX_ASSISTANT_SCENE_COMPONENTS).contains(&commands.len()))
        .then_some(AssistantProposal::Scene(commands))
}

fn assistant_labeled_tetrahedron_scene_proposal(text: &str) -> Option<AssistantProposal> {
    let lines = text
        .lines()
        .map(str::trim)
        .filter(|line| !line.is_empty())
        .collect::<Vec<_>>();
    if lines.len() != 10 {
        return None;
    }

    let mut labels = std::collections::HashSet::new();
    let mut point_count = 0;
    let mut edges = Vec::new();
    for line in lines {
        let (label, command) = line.split_once('=')?;
        let label = label.trim();
        let command = parse_assistant_command(command.trim())?;
        if !is_assistant_identifier(label) || !labels.insert(label) {
            return None;
        }
        match command.canonical_name() {
            "Point3D"
                if command.arguments().len() == 3
                    && command
                        .arguments()
                        .iter()
                        .all(|argument| is_finite_coordinate_literal(argument)) =>
            {
                point_count += 1;
            }
            "Segment3D"
                if command.arguments().len() == 6
                    && command
                        .arguments()
                        .iter()
                        .all(|argument| is_finite_coordinate_literal(argument)) =>
            {
                edges.push(command);
            }
            _ => return None,
        }
    }

    (point_count == 4 && edges.len() == 6).then_some(AssistantProposal::Scene(edges))
}

fn is_complete_assistant_command(command: &str) -> bool {
    let Some(open) = command.find('[') else {
        return false;
    };
    if open == 0
        || !command.ends_with(']')
        || !is_assistant_identifier(&command[..open])
        || has_empty_top_level_argument(&command[open + 1..command.len() - 1])
    {
        return false;
    }

    let mut expected_closings = Vec::new();
    for character in command[open..].chars() {
        match character {
            '(' => expected_closings.push(')'),
            '[' => expected_closings.push(']'),
            '{' => expected_closings.push('}'),
            ')' | ']' | '}' if expected_closings.pop() != Some(character) => return false,
            _ => {}
        }
    }
    expected_closings.is_empty()
}

fn is_assistant_identifier(value: &str) -> bool {
    value.len() <= 64
        && value
            .chars()
            .next()
            .is_some_and(|character| character.is_ascii_alphabetic() || character == '_')
        && value
            .chars()
            .all(|character| character.is_ascii_alphanumeric() || character == '_')
}

fn has_empty_top_level_argument(arguments: &str) -> bool {
    if arguments.trim().is_empty() {
        return false;
    }

    let mut depth = 0usize;
    let mut start = 0;
    for (index, character) in arguments.char_indices() {
        match character {
            '(' | '[' | '{' => depth += 1,
            ')' | ']' | '}' => depth = depth.saturating_sub(1),
            ',' if depth == 0 => {
                if arguments[start..index].trim().is_empty() {
                    return true;
                }
                start = index + 1;
            }
            _ => {}
        }
    }
    arguments[start..].trim().is_empty()
}

fn has_explicit_assistant_function_expression(command: &str, arguments: &[String]) -> bool {
    if command != "Function" {
        return true;
    }
    let Some(expression) = arguments.first() else {
        return true;
    };
    !expression
        .split(|character: char| !(character.is_ascii_alphanumeric() || character == '_'))
        .map(str::to_ascii_lowercase)
        .any(|identifier| {
            matches!(
                identifier.as_str(),
                "sum" | "a_n" | "b_n" | "n" | "fourier" | "fouriertransform" | "fourier_transform"
            )
        })
}

fn is_finite_coordinate_literal(argument: &str) -> bool {
    argument.parse::<f64>().is_ok_and(f64::is_finite)
}

#[cfg(test)]
mod tests {
    use super::{
        assistant_fenced_proposals, parse_assistant_command, parse_assistant_parameter,
        AssistantProposal, AssistantProposalCandidate, AssistantProposalRejection,
        AssistantProposalRejectionKind,
    };

    #[test]
    fn registered_aliases_become_canonical_typed_invocations() {
        let invocation = parse_assistant_command("ImplicitRegion[(x^2 + y^2 - 1)^3 - x^2*y^3 = 0]")
            .expect("the registered alias must be recognized");

        assert_eq!(invocation.spec().id, "geometry.implicit-curve");
        assert_eq!(invocation.canonical_name(), "ImplicitCurve");
        assert_eq!(
            invocation.canonical_text(),
            "ImplicitCurve[(x^2 + y^2 - 1)^3 - x^2*y^3 = 0]"
        );
    }

    #[test]
    fn unregistered_or_non_actionable_text_never_becomes_an_invocation() {
        for candidate in [
            "Script[Save[]]",
            "Save[file]",
            "Import[data.csv]",
            "Function[x]; Analyze[f]",
            "Unknown[x]",
            "Function[x\n]",
            "Function[, x]",
            "Function[Sum[sin(n*x), {n, 1, 10}]]",
        ] {
            assert!(
                parse_assistant_command(candidate).is_none(),
                "{candidate} must not become an assistant action"
            );
        }
    }

    #[test]
    fn rejected_fences_keep_only_a_sanitized_repair_target() {
        let proposals = assistant_fenced_proposals(
            "```grafito\nPolyhedron[NumericArray[{{0,0,0}}], NumericArray[{{0,1,2}}]]\n```\n\n```grafito\nScript[Save[]]\n```",
        );

        assert_eq!(proposals.len(), 2);
        assert!(proposals
            .iter()
            .all(|candidate| candidate.proposal.is_none()));
        assert!(proposals
            .iter()
            .all(AssistantProposalCandidate::is_action_candidate));
        assert!(matches!(
            proposals[0].rejection.as_ref(),
            Some(AssistantProposalRejection {
                command,
                kind: AssistantProposalRejectionKind::UnsupportedCommand,
            }) if command == "Polyhedron"
        ));
        assert!(matches!(
            proposals[1].rejection.as_ref(),
            Some(AssistantProposalRejection {
                command,
                kind: AssistantProposalRejectionKind::UnsupportedCommand,
            }) if command == "Script"
        ));
    }

    #[test]
    fn malformed_scene_fences_keep_a_sanitized_correction_path() {
        for fence in [
            "```grafito-scene\nCylinder[0,0,0,1,2]\n```",
            "```grafito-scene\nFunction[x]\nFunction[x]\nFunction[x]\nFunction[x]\nFunction[x]\nFunction[x]\nFunction[x]\nFunction[x]\nFunction[x]\n```",
        ] {
            let proposals = assistant_fenced_proposals(fence);
            assert!(matches!(
                proposals.as_slice(),
                [AssistantProposalCandidate {
                    proposal: None,
                    rejection: Some(AssistantProposalRejection {
                        command,
                        kind: AssistantProposalRejectionKind::InvalidSyntax,
                    }),
                    ..
                }] if command == "Scene"
            ));
        }
    }

    #[test]
    fn fenced_proposals_keep_code_block_identity_and_only_emit_typed_actions() {
        let proposals = assistant_fenced_proposals(
            "```rust\nlet ignored = true;\n```\n\n```grafito-param\na = 2.5\n```\n\n```grafito\nImplicitRegion[x^2 + y^2 = a^2]\n```\n\n```grafito-scene\nSegment3D[0,0,0,1,0,0]\nSegment3D[1,0,0,0,1,0]\n```",
        );

        assert_eq!(proposals.len(), 3);
        assert_eq!(proposals[0].code_block_index, 1);
        assert_eq!(proposals[1].code_block_index, 2);
        assert_eq!(proposals[2].code_block_index, 3);
        assert!(matches!(
            &proposals[0].proposal,
            Some(AssistantProposal::Parameter(_))
        ));
        assert!(matches!(
            &proposals[1].proposal,
            Some(AssistantProposal::Command(_))
        ));
        assert!(matches!(
            &proposals[2].proposal,
            Some(AssistantProposal::Scene(ref scene)) if scene.len() == 2
        ));
        assert_eq!(
            proposals[1]
                .proposal
                .as_ref()
                .expect("typed proposal")
                .canonical_text(),
            "ImplicitCurve[x^2 + y^2 = a^2]"
        );
    }

    #[test]
    fn labeled_tetrahedron_becomes_six_typed_edges() {
        let proposals = assistant_fenced_proposals(
            "```grafito-scene\nv0 = Point3D[0,0,0]\nv1 = Point3D[1,0,0]\nv2 = Point3D[0,1,0]\nv3 = Point3D[0,0,1]\na00 = Segment3D[0,0,0,1,0,0]\na01 = Segment3D[0,0,0,0,1,0]\na02 = Segment3D[0,0,0,0,0,1]\na03 = Segment3D[1,0,0,0,1,0]\na04 = Segment3D[1,0,0,0,0,1]\na05 = Segment3D[0,1,0,0,0,1]\n```",
        );

        assert!(matches!(
            proposals.as_slice(),
            [proposal] if matches!(&proposal.proposal, Some(AssistantProposal::Scene(scene)) if scene.len() == 6)
        ));
    }

    #[test]
    fn parameter_assignments_are_finite_and_canonical() {
        let assignment = parse_assistant_parameter("a = 2.5").expect("finite scalar assignment");
        assert_eq!(assignment.name(), "a");
        assert_eq!(assignment.value(), 2.5);
        assert_eq!(assignment.canonical_text(), "a = 2.5");
        assert!(parse_assistant_parameter("a = NaN").is_none());
        assert!(parse_assistant_parameter("a = sin(1)").is_none());
        assert!(parse_assistant_parameter("a = 1; Save[]").is_none());
    }

    #[test]
    fn expected_view_apunta_a_la_vista_correcta_con_default_2d_honesto() {
        use crate::assistant_context::AssistantGraphView;
        // S1: apply → vista esperada (2D o 3D según el objeto).
        let two_d = parse_assistant_command("Function[x]").expect("2D válido");
        assert_eq!(
            AssistantProposal::Command(two_d).expected_view(),
            AssistantGraphView::TwoD
        );
        assert_eq!(
            AssistantProposal::Command(parse_assistant_command("Function[x]").expect("2D válido"))
                .expected_view_label(),
            "2D"
        );
        let three_d = parse_assistant_command("Sphere[0, 0, 0, 1]").expect("3D válido");
        assert_eq!(
            AssistantProposal::Command(three_d).expected_view(),
            AssistantGraphView::ThreeD
        );
        // Parámetro sin objeto → default 2D honesto (no inventa 3D).
        let param = parse_assistant_parameter("a = 2.5").expect("parámetro");
        assert_eq!(
            AssistantProposal::Parameter(param).expected_view(),
            AssistantGraphView::TwoD
        );
    }
}
