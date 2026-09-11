//! Preflight puro del asistente (slice 1 del split de god objects).
//!
//! Funciones libres sin `self` movidas desde `assistant.rs` sin cambios de
//! comportamiento. Las constantes estan duplicadas de `assistant.rs:41-48`
//! (fuente canonica) para evitar imports circulares
//! `assistant` <-> `assistant_preflight`. Mantener sincronizado.

use grafito_assistant::{validate_attachment, CancellationToken};
use grafito_assistant_types::{
    AssistantRepairFailure, AssistantRepairFailureKind, AssistantRepairFeedback, AttachmentLimits,
    ProviderProfile,
};
use grafito_command::assistant_proposals::{
    assistant_fenced_proposals, execute_assistant_command, execute_assistant_parameter,
    AssistantCommandInvocation, AssistantParameterAssignment, AssistantProposal,
    AssistantProposalRejection, AssistantProposalRejectionKind,
};
use grafito_ui::assistant::VerifiedAssistantProposal;
use std::collections::VecDeque;
use std::fs::File;
use std::io::{Cursor, Read};
use std::path::PathBuf;

const MAX_REMOTE_PROPOSAL_PREFLIGHTS: usize = 4;
const MAX_ASSISTANT_PROPOSAL_CORRECTIONS: u8 = 2;
const MAX_ASSISTANT_CORRECTION_SOURCE_BYTES: usize = 2_048;
const OPENCODE_VISION_MODEL: &str = "mimo-2.5-vl";
const ASSISTANT_CORRECTION_INSTRUCTION: &str = "\n\nUna propuesta gráfica anterior no superó la verificación local. Conservá la intención de la solicitud y regenerá una respuesta completa y autocontenida con un bloque grafito o un bloque grafito-scene de 2 a 8 comandos ejecutables. Si necesitás un parámetro escalar nuevo, incluí antes un único bloque grafito-param con una asignación finita. Usá exclusivamente la sintaxis exacta del catálogo; no inventes comandos ni emitas acciones de archivo, red, sistema o Script.";

pub(crate) fn plugin_validation_context() -> grafito_plugins::ValidationContext<'static> {
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

pub(crate) fn assistant_correction_prompt(question: &str) -> String {
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

pub(crate) fn can_offer_assistant_proposal_correction(
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

pub(crate) fn can_use_fusion_fallback(
    fallback_allowed: bool,
    provider: ProviderProfile,
    selected_model: &str,
) -> bool {
    fallback_allowed
        && provider == ProviderProfile::OpenCodeGo
        && selected_model == OPENCODE_VISION_MODEL
}

pub(crate) fn assistant_expected_syntax(command: &str) -> Vec<String> {
    let mut syntaxes = grafito_command::assistant_context::assistant_executable_syntaxes(command);
    if let Some(guidance) =
        grafito_command::assistant_context::assistant_literal_argument_guidance(command)
    {
        syntaxes.push(guidance.into());
    }
    syntaxes
}

pub(crate) fn classify_assistant_preflight_error(error: &str) -> AssistantRepairFailureKind {
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

pub(crate) fn assistant_repair_failure_for_command(
    command: &AssistantCommandInvocation,
    error: &str,
) -> AssistantRepairFailure {
    AssistantRepairFailure {
        command: command.canonical_name().into(),
        kind: classify_assistant_preflight_error(error),
        expected_syntax: assistant_expected_syntax(command.canonical_name()),
    }
}

pub(crate) fn assistant_repair_failure_from_rejection(
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

pub(crate) fn assistant_repair_failure_for_scene(
    _commands: &[AssistantCommandInvocation],
    error: &str,
) -> AssistantRepairFailure {
    AssistantRepairFailure {
        command: "Scene".into(),
        kind: classify_assistant_preflight_error(error),
        expected_syntax: Vec::new(),
    }
}

pub(crate) struct AssistantGraphPreflight {
    pub(crate) staged: grafito_core::Document,
    pub(crate) outcome: grafito_command::commands::CommandOutcome,
    pub(crate) view: grafito_command::assistant_context::AssistantGraphView,
}

pub(crate) struct AssistantScenePreflight {
    pub(crate) staged: grafito_core::Document,
    pub(crate) outcome: grafito_command::commands::CommandOutcome,
    pub(crate) camera: grafito_geometry::Camera3D,
    pub(crate) view: grafito_command::assistant_context::AssistantGraphView,
}

pub(crate) struct RemoteProposalVerification {
    pub(crate) verified: Vec<VerifiedAssistantProposal>,
    pub(crate) candidate_count: usize,
    pub(crate) candidate_code_block_indices: Vec<usize>,
    pub(crate) action_candidate_count: usize,
    pub(crate) verified_action_count: usize,
    pub(crate) repair_feedback: Option<AssistantRepairFeedback>,
}

#[cfg(test)]
pub(crate) fn verified_remote_proposals(
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
pub(crate) fn inspect_remote_proposals(
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
pub(crate) fn inspect_remote_action_proposals(
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

pub(crate) fn inspect_remote_proposals_cancellable(
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

pub(crate) fn document_with_assistant_prerequisites(
    document: &grafito_core::Document,
    prerequisite_parameters: &[AssistantParameterAssignment],
) -> Result<grafito_core::Document, String> {
    let mut staged = document.detached_clone_for_staging();
    for assignment in prerequisite_parameters {
        stage_assistant_parameter(&mut staged, assignment)?;
    }
    Ok(staged)
}

pub(crate) fn preflight_assistant_scene_with_prerequisites(
    document: &grafito_core::Document,
    prerequisite_parameters: &[AssistantParameterAssignment],
    commands: &[AssistantCommandInvocation],
    camera: grafito_geometry::Camera3D,
) -> Result<AssistantScenePreflight, String> {
    let staged = document_with_assistant_prerequisites(document, prerequisite_parameters)?;
    preflight_assistant_scene(&staged, commands, camera)
}

pub(crate) fn preflight_assistant_scene(
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

pub(crate) fn preflight_homogeneous_assistant_scene(
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

pub(crate) fn preflight_assistant_flower_scene(
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

pub(crate) fn flower_scene_components_are_connected(
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

pub(crate) fn point_segment_distance(
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

pub(crate) fn style_flower_component(
    object: &mut grafito_core::GeoObject,
    petal_index: &mut usize,
) {
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

pub(crate) fn fit_camera_to_world_mesh(
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
pub(crate) fn preflight_assistant_graph_command(
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

pub(crate) fn preflight_assistant_graph_command_with_camera(
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

pub(crate) fn preflight_assistant_graph_command_with_prerequisites(
    document: &grafito_core::Document,
    prerequisite_parameters: &[AssistantParameterAssignment],
    command: &AssistantCommandInvocation,
    camera: grafito_geometry::Camera3D,
) -> Result<AssistantGraphPreflight, String> {
    let staged = document_with_assistant_prerequisites(document, prerequisite_parameters)?;
    preflight_assistant_graph_command_with_camera(&staged, command, camera)
}

pub(crate) fn preflight_assistant_parameter(
    document: &grafito_core::Document,
    assignment: &AssistantParameterAssignment,
) -> Result<(), String> {
    let mut staged = document.detached_clone_for_staging();
    stage_assistant_parameter(&mut staged, assignment)
}

pub(crate) fn stage_assistant_parameter(
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

pub(crate) fn commit_assistant_graph_preflight(
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

pub(crate) fn assistant_command_is_safe(command: &AssistantCommandInvocation) -> bool {
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

pub(crate) fn assistant_graph_view(
    command: &AssistantCommandInvocation,
) -> Option<grafito_command::assistant_context::AssistantGraphView> {
    grafito_command::assistant_context::assistant_graph_capability(command.canonical_name())
        .map(|capability| capability.view)
}

#[cfg(test)]
pub(crate) fn validate_assistant_command(candidate: &str) -> Option<String> {
    grafito_command::assistant_proposals::parse_assistant_command(candidate)
        .map(|command| command.canonical_text())
}

pub(crate) fn point_is_in_view(x: f32, y: f32, width: f32, height: f32) -> bool {
    x.is_finite() && y.is_finite() && x >= 0.0 && x <= width && y >= 0.0 && y <= height
}

pub(crate) fn static_geometry_intersects_view(
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

pub(crate) fn world_mesh_intersects_view(
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

pub(crate) fn cpu_overlay_intersects_view(
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

pub(crate) fn assistant_graph_perspective(
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

pub(crate) fn load_assistant_attachment(
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

pub(crate) fn read_bounded_attachment(
    reader: impl Read,
    max_bytes: usize,
) -> Result<Vec<u8>, String> {
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
