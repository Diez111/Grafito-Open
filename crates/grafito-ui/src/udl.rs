//! UDL ligero — F5 Scandinavian progressive disclosure sin laberinto.
//!
//! Módulo liviano que expone el filtrado de toolbar por `level_value: u32`
//! sin acoplar `grafito-ui` a `grafito-pedagogy` vía tipos. La toolbar ya
//! provee `toolbar_groups_for_level_value(u32)` como single source of truth;
//! este módulo re-exporta y añade helpers de etiqueta UDL para UI.
//!
//! Si `grafito-ui` no pudiera depender de `pedagogy`, este archivo sería el
//! único punto de contacto (feature-gated). Actualmente `grafito-ui` sí
//! depende de `pedagogy`, pero se mantiene el helper ligero por compat.

use crate::toolbar::{
    filter_groups_by_level, toolbar_groups_for_level_value, ToolGroupId, TOOLBAR_LEVEL_PRIMARY_MAX,
    TOOLBAR_LEVEL_SECONDARY_MAX,
};

/// Etiqueta UDL para `level_value` sin necesidad de `PedagogicalLevel`.
pub fn udl_label_for_level_value(level_value: u32) -> &'static str {
    if level_value <= TOOLBAR_LEVEL_PRIMARY_MAX {
        "Primaria"
    } else if level_value <= TOOLBAR_LEVEL_SECONDARY_MAX {
        "Secundaria"
    } else if level_value <= 12 {
        "UTN AM1"
    } else if level_value == 13 {
        "UTN Álgebra"
    } else if level_value == 14 {
        "UTN AM2/Prob."
    } else {
        "Universidad"
    }
}

/// Grupos permitidos para `level_value` — delega a `toolbar` single source.
pub fn udl_groups_for_level_value(level_value: u32) -> &'static [ToolGroupId] {
    toolbar_groups_for_level_value(level_value)
}

/// Filtra grupos de perspectiva por `level_value` (versión UDL ligera).
pub fn udl_filter_groups(groups: &[ToolGroupId], level_value: u32) -> Vec<ToolGroupId> {
    filter_groups_by_level(groups, level_value)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::toolbar::{PRIMARY_TOOL_GROUPS, SECONDARY_TOOL_GROUPS, UNIVERSITY_TOOL_GROUPS};

    #[test]
    fn udl_primary_is_five() {
        assert_eq!(udl_groups_for_level_value(2).len(), 5);
        assert_eq!(udl_groups_for_level_value(4), PRIMARY_TOOL_GROUPS);
    }

    #[test]
    fn udl_secondary_is_eight() {
        assert_eq!(udl_groups_for_level_value(8).len(), 8);
        assert_eq!(udl_groups_for_level_value(10), SECONDARY_TOOL_GROUPS);
    }

    #[test]
    fn udl_university_is_all() {
        assert_eq!(udl_groups_for_level_value(15), UNIVERSITY_TOOL_GROUPS);
        assert!(udl_groups_for_level_value(15).len() >= SECONDARY_TOOL_GROUPS.len());
    }

    #[test]
    fn udl_filter_respects_perspective() {
        let perspective = [
            crate::toolbar::ToolGroupId::Move,
            crate::toolbar::ToolGroupId::Point,
            crate::toolbar::ToolGroupId::Advanced,
        ];
        // Primary solo permite Move/Point, filtra Advanced
        let filtered = udl_filter_groups(&perspective, 2);
        assert_eq!(
            filtered,
            vec![
                crate::toolbar::ToolGroupId::Move,
                crate::toolbar::ToolGroupId::Point
            ]
        );
    }
}
