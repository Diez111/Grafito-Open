//! Persistencia — Auto-save/load del documento

use grafito_core::Document;
use std::fs;

/// Guarda documento a archivo JSON
pub fn save_document(doc: &Document, path: &str) -> bool {
    match serde_json::to_string_pretty(doc) {
        Ok(json) => {
            match fs::write(path, json) {
                Ok(_) => {
                    log::info!("Document saved to {}", path);
                    true
                }
                Err(e) => {
                    log::error!("Failed to write file {}: {}", path, e);
                    false
                }
            }
        }
        Err(e) => {
            log::error!("Failed to serialize document: {}", e);
            false
        }
    }
}

/// Carga documento desde archivo JSON
pub fn load_document(path: &str) -> Option<Document> {
    match fs::read_to_string(path) {
        Ok(json) => {
            if let Err(e) = grafito_core::validation::validate_document_json(&json) {
                log::error!("Document validation failed for {}: {}", path, e);
                return None;
            }
            match serde_json::from_str::<Document>(&json) {
                Ok(doc) => {
                    if let Err(e) = grafito_core::validation::validate_document(&doc) {
                        log::error!("Document validation failed for {}: {}", path, e);
                        return None;
                    }
                    log::info!("Document loaded from {}", path);
                    Some(doc)
                }
                Err(e) => {
                    log::error!("Failed to deserialize document from {}: {}", path, e);
                    None
                }
            }
        }
        Err(e) => {
            log::error!("Failed to read file {}: {}", path, e);
            None
        }
    }
}
