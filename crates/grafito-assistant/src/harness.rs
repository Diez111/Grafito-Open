//! Harness local, sin UI ni transporte, para propuestas del asistente.
//!
//! Las operaciones públicas de este módulo no reciben proveedores, claves ni
//! contextos egui. Un cliente debe pedir explícitamente que un receipt se
//! persista y debe invocar `replay` antes de confiar en evidencia recuperada.

use crate::solve_local;
use grafito_assistant_types::{
    AssistantPlanReceipt, AssistantRequest, AssistantResponse, PrivacyMode, ProposedPlan,
};
use grafito_command::{
    assistant_context::document_context,
    assistant_plan::{
        apply_staged_plan, replay_plan, stage_plan, PlanApplyResult, PlanPreview, PlanReplayResult,
        StagedPlan,
    },
};
use grafito_core::Document;

/// Respuesta local junto al staging opcional de su propuesta allowlisted.
pub struct HeadlessAssistantResult {
    /// Resultado del solver local, apto para cualquier frontend.
    pub response: AssistantResponse,
    /// Staging disponible sólo cuando la respuesta incluyó una propuesta válida.
    pub staged_plan: Option<HeadlessStagedPlan>,
}

/// Token en memoria de un plan ya staged que aún requiere Apply explícito.
pub struct HeadlessStagedPlan {
    inner: StagedPlan,
}

impl HeadlessStagedPlan {
    /// Diferencias generadas contra el documento base exacto.
    pub fn preview(&self) -> &PlanPreview {
        self.inner.preview()
    }

    /// Receipt hash-only que puede conservarse localmente por decisión del cliente.
    pub fn receipt(&self) -> &AssistantPlanReceipt {
        self.inner.receipt()
    }
}

/// Crea una solicitud estrictamente local ligada al contexto actual del documento.
pub fn local_request(document: &Document, problem: impl Into<String>) -> AssistantRequest {
    AssistantRequest::local(problem, document_context(document))
}

/// Resuelve una solicitud local y stagea su propuesta, si existe, sin mutar el documento.
pub fn request(
    document: &Document,
    request: &AssistantRequest,
) -> Result<HeadlessAssistantResult, String> {
    if request.privacy_mode != PrivacyMode::LocalOnly {
        return Err("headless assistant harness only accepts local requests".into());
    }
    if request.context != document_context(document) {
        return Err("headless assistant request context does not match the document".into());
    }

    let response = solve_local(request);
    let staged_plan = response
        .plan
        .as_ref()
        .map(|plan| stage(document, plan))
        .transpose()?;
    Ok(HeadlessAssistantResult {
        response,
        staged_plan,
    })
}

/// Stagea una propuesta allowlisted sin alterar el documento vivo.
pub fn stage(document: &Document, plan: &ProposedPlan) -> Result<HeadlessStagedPlan, String> {
    Ok(HeadlessStagedPlan {
        inner: stage_plan(document, plan)?,
    })
}

/// Aplica un staging previamente aprobado por el cliente.
pub fn apply(
    document: &mut Document,
    staged: HeadlessStagedPlan,
) -> Result<PlanApplyResult, String> {
    apply_staged_plan(document, staged.inner)
}

/// Stagea y aplica una propuesta cuando el cliente no necesita retener el token entre pasos.
pub fn apply_plan(document: &mut Document, plan: &ProposedPlan) -> Result<PlanApplyResult, String> {
    apply(document, stage(document, plan)?)
}

/// Reproduce localmente el staging y valida un receipt sin aplicar cambios.
pub fn replay(
    document: &Document,
    plan: &ProposedPlan,
    receipt: &AssistantPlanReceipt,
) -> Result<PlanReplayResult, String> {
    replay_plan(document, plan, receipt)
}
