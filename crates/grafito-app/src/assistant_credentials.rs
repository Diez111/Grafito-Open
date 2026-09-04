//! Credenciales del asistente en el almacén seguro del sistema operativo.
//!
//! Las claves no se incluyen en `AppConfig`, documentos de Grafito ni mensajes
//! de error. Si el almacén no está disponible, el controlador puede usar una
//! clave de sesión que desaparece al cerrar la aplicación.

use grafito_assistant_types::ProviderProfile;

const KEYRING_SERVICE: &str = "Grafito";

/// Devuelve la cuenta fija del llavero para perfiles que requieren credencial.
///
/// Nota: `CustomOpenAiCompatible` requiere clave propia (`assistant-custom`) y
/// nunca reutiliza la de `OpenCodeGo`. `OllamaLocal` no usa clave.
pub(crate) const fn account_for(profile: ProviderProfile) -> Option<&'static str> {
    match profile {
        ProviderProfile::OpenCodeGo => Some("assistant-opencode-go"),
        ProviderProfile::DeepSeek => Some("assistant-deepseek"),
        ProviderProfile::CustomOpenAiCompatible => Some("assistant-custom"),
        ProviderProfile::OllamaLocal => None,
    }
}

fn entry(profile: ProviderProfile) -> Result<keyring::Entry, String> {
    let account = account_for(profile)
        .ok_or_else(|| "the selected assistant provider does not use an API key".to_string())?;
    keyring::Entry::new(KEYRING_SERVICE, account)
        .map_err(|_| "the system credential store is unavailable".to_string())
}

/// Punto único de saneado (puro y testeable): recorta espacios/saltos pegados
/// al copiar y rechaza claves vacías. Vale para OpenCodeGo, DeepSeek y Custom
/// (`assistant-custom`); `OllamaLocal` nunca llega aquí (`account_for` es
/// `None` y `assistant_api_key` retorna `Ok(None)` antes del llavero).
pub(crate) fn sanitize_api_key(key: &str) -> Option<String> {
    let trimmed = key.trim();
    if trimmed.is_empty() {
        None
    } else {
        Some(trimmed.to_owned())
    }
}

/// Lee una clave existente sin exponer detalles del llavero al usuario.
/// Recorta espacios/saltos pegados al copiar para que claves viejas sucias
/// sigan funcionando sin reingreso.
pub(crate) fn load(profile: ProviderProfile) -> Result<Option<String>, String> {
    let entry = entry(profile)?;
    match entry.get_password() {
        Ok(key) => Ok(sanitize_api_key(&key)),
        Err(keyring::Error::NoEntry) => Ok(None),
        Err(_) => Err("the system credential store is unavailable".into()),
    }
}

/// Guarda una clave en el llavero del sistema, nunca en configuración plana.
/// Recorta antes de guardar para que nunca persista con `\n` final.
pub(crate) fn store(profile: ProviderProfile, key: &str) -> Result<(), String> {
    let Some(trimmed) = sanitize_api_key(key) else {
        return Err("the API key cannot be empty".into());
    };
    entry(profile)?
        .set_password(&trimmed)
        .map_err(|_| "the system credential store could not save the API key".to_string())
}

/// Elimina una clave guardada para que el perfil vuelva a requerir credenciales.
pub(crate) fn clear(profile: ProviderProfile) -> Result<(), String> {
    let entry = entry(profile)?;
    match entry.delete_credential() {
        Ok(()) | Err(keyring::Error::NoEntry) => Ok(()),
        Err(_) => Err("the system credential store could not remove the API key".into()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn known_provider_accounts_are_fixed_and_ollama_needs_no_key() {
        assert_eq!(
            account_for(ProviderProfile::OpenCodeGo),
            Some("assistant-opencode-go")
        );
        assert_eq!(
            account_for(ProviderProfile::CustomOpenAiCompatible),
            Some("assistant-custom")
        );
        assert_eq!(account_for(ProviderProfile::OllamaLocal), None);
    }

    #[test]
    fn sanitize_api_key_trims_pasted_whitespace_and_rejects_empty() {
        assert_eq!(
            sanitize_api_key("  sk-grafito-123  \n").as_deref(),
            Some("sk-grafito-123")
        );
        assert_eq!(
            sanitize_api_key("\tkey-con-espacios\t").as_deref(),
            Some("key-con-espacios")
        );
        assert_eq!(sanitize_api_key(""), None);
        assert_eq!(sanitize_api_key("   \n\t  "), None);
    }

    #[cfg(target_os = "linux")]
    #[test]
    fn linux_credential_backend_persists_beyond_a_single_entry() {
        use keyring::{credential::CredentialPersistence, default};

        assert!(matches!(
            default::default_credential_builder().persistence(),
            CredentialPersistence::UntilDelete
        ));
    }

    #[cfg(target_os = "linux")]
    #[test]
    #[ignore = "requires an unlocked desktop Secret Service"]
    fn linux_secret_service_round_trips_a_temporary_credential() {
        let account = format!("assistant-credential-probe-{}", std::process::id());
        let entry = keyring::Entry::new(KEYRING_SERVICE, &account).unwrap();
        entry.set_password("grafito-credential-probe").unwrap();
        drop(entry);
        let reloaded_entry = keyring::Entry::new(KEYRING_SERVICE, &account).unwrap();
        let result = reloaded_entry.get_password();
        let cleanup = reloaded_entry.delete_credential();

        assert_eq!(result.unwrap(), "grafito-credential-probe");
        cleanup.unwrap();
    }
}
