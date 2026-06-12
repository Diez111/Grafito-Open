//! FFI command processor adapter.
//!
//! The full command implementation lives in `grafito-command` so desktop and
//! Android execute the same mathematical language. This adapter only translates
//! the shared result into FFI-friendly metadata.

use grafito_core::{Document, ObjectId};
use std::collections::HashSet;

#[derive(Debug, Clone)]
pub struct CommandOutcome {
    pub success: bool,
    pub message: Option<String>,
    pub new_object_id: Option<ObjectId>,
}

pub fn process_input(document: &mut Document, input_text: &mut String) -> CommandOutcome {
    let before: HashSet<ObjectId> = document.objects_iter().map(|(id, _)| *id).collect();

    let shared_message = grafito_command::process_input(document, input_text);

    let new_object_id = document
        .objects_iter()
        .map(|(id, _)| *id)
        .find(|id| !before.contains(id));

    let success = shared_message.is_some() || new_object_id.is_some();

    CommandOutcome {
        success,
        message: shared_message.or_else(|| {
            new_object_id
                .map(|id| format!("Created object {}", crate::converters::id_to_string(id)))
        }),
        new_object_id,
    }
}

#[cfg(test)]
mod tests {
    use super::process_input;
    use grafito_core::{Document, GeoObject};

    #[test]
    fn ffi_adapter_uses_shared_advanced_commands() {
        let mut doc = Document::new();
        let mut input = "Lorenz[]".to_string();

        let outcome = process_input(&mut doc, &mut input);

        assert!(outcome.success);
        assert!(outcome.message.unwrap_or_default().contains("Lorenz"));
        assert!(doc
            .objects_iter()
            .any(|(_, obj)| matches!(obj, GeoObject::Attractor3D(_))));
    }

    #[test]
    fn ffi_adapter_treats_text_only_commands_as_success() {
        let mut doc = Document::new();
        let mut input = "Mean[{1,2,3}]".to_string();

        let outcome = process_input(&mut doc, &mut input);

        assert!(outcome.success);
        assert!(outcome.new_object_id.is_none());
        assert!(outcome.message.unwrap_or_default().contains("Mean"));
    }
}
