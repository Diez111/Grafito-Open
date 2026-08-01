//! Credenciales del asistente en el almacén seguro del sistema operativo.
//!
//! Las claves no se incluyen en `AppConfig`, documentos de Grafito ni mensajes
//! de error. Si el almacén no está disponible, el controlador puede usar una
//! clave de sesión que desaparece al cerrar la aplicación.

use grafito_assistant_types::ProviderProfile;

const KEYRING_SERVICE: &str = "Grafito";

/// Devuelve la cuenta fija del llavero para perfiles que requieren credencial.
pub(crate) const fn account_for(profile: ProviderProfile) -> Option<&'static str> {
    match profile {
        ProviderProfile::OpenCodeGo => Some("assistant-opencode-go"),
        ProviderProfile::DeepSeek => Some("assistant-deepseek"),
        ProviderProfile::OllamaLocal | ProviderProfile::CustomOpenAiCompatible => None,
    }
}

fn entry(profile: ProviderProfile) -> Result<keyring::Entry, String> {
    let account = account_for(profile)
        .ok_or_else(|| "the selected assistant provider does not use an API key".to_string())?;
    keyring::Entry::new(KEYRING_SERVICE, account)
        .map_err(|_| "the system credential store is unavailable".to_string())
}

/// Lee una clave existente sin exponer detalles del llavero al usuario.
pub(crate) fn load(profile: ProviderProfile) -> Result<Option<String>, String> {
    let entry = entry(profile)?;
    match entry.get_password() {
        Ok(key) if !key.trim().is_empty() => Ok(Some(key)),
        Ok(_) => Ok(None),
        Err(keyring::Error::NoEntry) => Ok(None),
        Err(_) => Err("the system credential store is unavailable".into()),
    }
}

/// Guarda una clave en el llavero del sistema, nunca en configuración plana.
pub(crate) fn store(profile: ProviderProfile, key: &str) -> Result<(), String> {
    if key.trim().is_empty() {
        return Err("the API key cannot be empty".into());
    }
    entry(profile)?
        .set_password(key)
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
        assert_eq!(account_for(ProviderProfile::OllamaLocal), None);
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
