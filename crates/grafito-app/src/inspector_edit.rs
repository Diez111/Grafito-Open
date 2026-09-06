//! Estado de edición de la ecuación del Inspector (frente D3, cableado D1-bis
//! en `panels.rs::draw_right_properties_contents`): estado puro y testeable
//! headless sin egui ni I/O. El panel dibuja el `TextEdit` y llama a
//! [`begin_edit`] / [`validate_draft`] / [`commit_draft_with_previous`]
//! empujando el previo al undo vía `snapshot.capture_successful_replacement`.
//!
//! Todo es puro salvo `commit_*` que muta el `Document` de forma atómica
//! (vía `grafito-command::inspector_equation`, que usa `try_replace` sobre
//! copia: jamás aplica a medias). Errores en rioplatense, jamás pánico.

use grafito_command::inspector_equation;
use grafito_core::{Document, GeoObject, ObjectId};

/// Borrador de edición de la ecuación. `editing` distingue "mirando" de
/// "editando" para que D1-bis no pise el texto mientras se escribe.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct InspectorEditState {
    /// Texto que ve y edita el usuario (arranca en la canónica).
    pub draft: String,
    /// Último error de validación (rioplatense) o `None` si el borrador va bien.
    pub error: Option<String>,
    /// `true` una vez que el usuario abrió el campo de edición.
    pub editing: bool,
}

impl InspectorEditState {
    fn new(draft: String) -> Self {
        Self {
            draft,
            error: None,
            editing: false,
        }
    }
}

/// `true` si el objeto tiene ecuación editable (canónica `Some`).
pub fn is_editable(obj: &GeoObject) -> bool {
    inspector_equation::is_equation_editable(obj)
}

/// Arranca la edición: `Some(estado)` con el borrador en la canónica,
/// `None` honesto si el tipo no se edita por ecuación.
pub fn begin_edit(obj: &GeoObject) -> Option<InspectorEditState> {
    obj.canonical_equation_text().map(InspectorEditState::new)
}

/// Pista rioplatense por tipo para el placeholder del campo.
pub fn hint_for(obj: &GeoObject) -> &'static str {
    match obj {
        GeoObject::Function(_) => "Escribí `y = …` en x, p. ej. `y = x^2 + 1`",
        GeoObject::ParametricCurve2D(_) => "Escribí `(x(t), y(t))`, p. ej. `(cos(t), sin(t))`",
        GeoObject::ParametricCurve3D(_) => "Escribí `(x(t), y(t), z(t))`",
        GeoObject::ImplicitCurve(_) => {
            "Escribí `lado = lado`, p. ej. `x^2 + y^2 = 1` (sin operador va `= 0`)"
        }
        GeoObject::Line(_) => {
            "Escribí `ax + by = c`, p. ej. `2x - 3y = 5` (segmento: agregá `[(x1,y1) - (x2,y2)]`)"
        }
        GeoObject::Circle(_) => {
            "Escribí `(x - h)^2 + (y - k)^2 = R`, p. ej. `(x - 1)^2 + (y - 2)^2 = 9`"
        }
        GeoObject::Ellipse(_) => "Escribí `elipse centro=(h, k) rx=… ry=… rot=…`",
        GeoObject::Parabola(_) => "Escribí `parabola vertice=(h, k) p=… vertical=… rot=…`",
        GeoObject::Hyperbola(_) => "Escribí `hiperbola centro=(h, k) a=… b=… horizontal=… rot=…`",
        GeoObject::Point(_) => "Escribí `(x, y)`, p. ej. `(1, 2)`",
        GeoObject::Polygon(_) => "Escribí `[(x1,y1), (x2,y2), …]` con al menos 3 vértices",
        GeoObject::Text(_) => "Escribí `\"texto\" @ (x, y)`",
        GeoObject::PolarCurve(_) => "Escribí `r = …` en t, p. ej. `r = 1 + cos(t)`",
        GeoObject::Surface3D(s) if s.is_parametric => "Escribí `(x(u,v), y(u,v), z(u,v))`",
        GeoObject::Surface3D(s) if s.is_complex => "Escribí `|f(z)|`, p. ej. `|z^2 + 1|`",
        GeoObject::Surface3D(_) => "Escribí `z = …` en x e y",
        _ => "Este tipo no se edita por ecuación todavía",
    }
}

/// Valida el borrador sin tocar el documento. `Ok(objeto)` trae la misma `id`.
pub fn validate_draft(original: &GeoObject, draft: &str) -> Result<GeoObject, String> {
    inspector_equation::parse_inspector_equation(original, draft).map_err(|e| e.message)
}

/// Aplica el borrador al documento (misma `id`). `Ok(true)` = cambió,
/// `Ok(false)` = no-op. Error → documento intacto.
pub fn commit_draft(doc: &mut Document, id: ObjectId, draft: &str) -> Result<bool, String> {
    inspector_equation::apply_inspector_equation(doc, id, draft).map_err(|e| e.message)
}

/// Como [`commit_draft`] pero devuelve el previo para el undo de D1-bis.
/// D1-bis hace `push_history_snapshot(undo, redo, previo)` si `Some`.
pub fn commit_draft_with_previous(
    doc: &mut Document,
    id: ObjectId,
    draft: &str,
) -> Result<Option<Document>, String> {
    inspector_equation::apply_inspector_equation_with_previous(doc, id, draft)
        .map_err(|e| e.message)
}

/// Actualiza el borrador del estado (puro): setea texto y limpia el error.
/// D1-bis llama a esto en cada cambio del `TextEdit` y a [`validate_draft`]
/// para mostrar el error en vivo sin aplicar.
pub fn update_draft(state: &mut InspectorEditState, new_draft: &str) {
    state.draft = new_draft.to_string();
    state.editing = true;
    state.error = None;
}

/// Revalida el estado contra el objeto original y guarda el mensaje en
/// `state.error`. Devuelve `true` si el borrador es válido.
pub fn revalidate_state(state: &mut InspectorEditState, original: &GeoObject) -> bool {
    match validate_draft(original, &state.draft) {
        Ok(_) => {
            state.error = None;
            true
        }
        Err(msg) => {
            state.error = Some(msg);
            false
        }
    }
}

/// Cancela la edición: vuelve al texto canónico y limpia el error.
/// Si no hay canónica, deja el borrador como está pero sin error.
pub fn cancel_edit(state: &mut InspectorEditState, obj: &GeoObject) {
    if let Some(canonical) = obj.canonical_equation_text() {
        state.draft = canonical;
    }
    state.error = None;
    state.editing = false;
}

#[cfg(test)]
mod tests {
    use super::*;
    use grafito_core::{FunctionObj, PointObj};
    use grafito_geometry::Point2;

    #[test]
    fn begin_edit_trae_canonica_y_hint() {
        let obj = GeoObject::Function(FunctionObj::new("x^2"));
        let st = begin_edit(&obj).expect("editable");
        assert_eq!(st.draft, "y = x^2");
        assert!(!hint_for(&obj).is_empty());
        assert!(is_editable(&obj));
    }

    #[test]
    fn begin_edit_none_honesto_para_no_editable() {
        use grafito_core::{Cube3DObj, GeoObject};
        use grafito_geometry::Point3D;
        let obj = GeoObject::Cube3D(Cube3DObj::new(Point3D::new(0.0, 0.0, 0.0), 1.0));
        assert_eq!(begin_edit(&obj), None);
        assert!(!is_editable(&obj));
    }

    #[test]
    fn update_y_revalidate_muestran_error_en_vivo() {
        let obj = GeoObject::Function(FunctionObj::new("x"));
        let mut st = begin_edit(&obj).expect("editable");
        update_draft(&mut st, "y = x + *");
        assert!(st.editing);
        assert!(!revalidate_state(&mut st, &obj));
        let err = st.error.clone().expect("error");
        assert!(err.contains("columna") || err.contains("entender"));
        update_draft(&mut st, "y = x^2");
        assert!(revalidate_state(&mut st, &obj));
        assert_eq!(st.error, None);
    }

    #[test]
    fn commit_preserva_id_y_permite_undo_manual() {
        let obj = GeoObject::Function(FunctionObj::new("x"));
        let mut doc = Document::new();
        let id = doc.try_add_object(obj).expect("alta");
        let ok = commit_draft(&mut doc, id, "y = x^2").expect("commit");
        assert!(ok);
        assert_eq!(
            doc.get_object(id)
                .and_then(|o| o.canonical_equation_text())
                .as_deref(),
            Some("y = x^2")
        );
        // Segundo commit con previo para el undo.
        let prev = commit_draft_with_previous(&mut doc, id, "y = x^3").expect("commit2");
        assert!(prev.is_some());
        let snapshot = prev.expect("previo");
        // El previo trae la ecuación intermedia: undo honesto.
        assert_eq!(
            snapshot
                .get_object(id)
                .and_then(|o| o.canonical_equation_text())
                .as_deref(),
            Some("y = x^2")
        );
    }

    #[test]
    fn commit_fallido_deja_intacto_y_mensaje_rioplatense() {
        let obj = GeoObject::Point(PointObj::new(Point2::new(1.0, 2.0)));
        let mut doc = Document::new();
        let id = doc.try_add_object(obj).expect("alta");
        let before = doc.clone();
        let err = commit_draft(&mut doc, id, "(1,)").expect_err("sintaxis");
        assert!(!err.is_empty());
        let a = serde_json::to_value(&doc).expect("json");
        let b = serde_json::to_value(&before).expect("json");
        assert_eq!(a, b);
    }

    #[test]
    fn cancel_vuelve_a_canonica() {
        let obj = GeoObject::Function(FunctionObj::new("x"));
        let mut st = begin_edit(&obj).expect("editable");
        update_draft(&mut st, "cualquier cosa");
        st.error = Some("error viejo".to_string());
        cancel_edit(&mut st, &obj);
        assert_eq!(st.draft, "y = x");
        assert_eq!(st.error, None);
        assert!(!st.editing);
    }
}
