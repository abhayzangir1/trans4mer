use keyring::Entry;
use serde::Serialize;

#[derive(Serialize)]
pub struct ApiKeyStatus {
    pub provider: String,
    pub is_set: bool,
}

pub struct KeyringManager;

impl KeyringManager {
    fn get_entry(provider: &str) -> Result<Entry, keyring::Error> {
        Entry::new("trans4mers", provider)
    }

    pub fn set_api_key(provider: &str, api_key: &str) -> Result<(), String> {
        let entry =
            Self::get_entry(provider).map_err(|e| format!("Failed to access keyring: {}", e))?;
        entry
            .set_password(api_key)
            .map_err(|e| format!("Failed to set API key: {}", e))?;
        Ok(())
    }

    pub fn get_api_key(provider: &str) -> Result<String, String> {
        let entry =
            Self::get_entry(provider).map_err(|e| format!("Failed to access keyring: {}", e))?;
        entry
            .get_password()
            .map_err(|e| format!("Failed to get API key: {}", e))
    }

    pub fn delete_api_key(provider: &str) -> Result<(), String> {
        let entry =
            Self::get_entry(provider).map_err(|e| format!("Failed to access keyring: {}", e))?;
        entry
            .delete_credential()
            .map_err(|e| format!("Failed to delete API key: {}", e))
    }
}
