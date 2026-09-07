use serde::{Deserialize, Serialize};

/// Configuration for Langfuse opt-in observability.
/// Public and secret keys are stored exclusively in the OS Keyring.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct LangfuseConfig {
    pub host: String,
    pub enabled: bool,
    pub capture_prompts: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub public_key: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub secret_key: Option<String>,
}

impl Default for LangfuseConfig {
    fn default() -> Self {
        Self {
            host: "http://localhost:3000".to_string(),
            enabled: false,
            capture_prompts: false,
            public_key: None,
            secret_key: None,
        }
    }
}

/// Safe status representation for UI reporting without exposing secrets.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct LangfuseConfigStatus {
    pub host: String,
    pub enabled: bool,
    pub capture_prompts: bool,
    pub has_public_key: bool,
    pub has_secret_key: bool,
    pub public_key_masked: Option<String>,
}

impl LangfuseConfigStatus {
    pub fn from_config(config: &LangfuseConfig) -> Self {
        let has_pub = config
            .public_key
            .as_ref()
            .map(|k| !k.trim().is_empty())
            .unwrap_or(false);
        let has_sec = config
            .secret_key
            .as_ref()
            .map(|k| !k.trim().is_empty())
            .unwrap_or(false);

        let masked = config.public_key.as_ref().and_then(|k| {
            let trimmed = k.trim();
            if trimmed.is_empty() {
                None
            } else if trimmed.len() <= 8 {
                Some("********".to_string())
            } else {
                Some(format!(
                    "{}...{}",
                    &trimmed[..4],
                    &trimmed[trimmed.len() - 4..]
                ))
            }
        });

        Self {
            host: config.host.clone(),
            enabled: config.enabled,
            capture_prompts: config.capture_prompts,
            has_public_key: has_pub,
            has_secret_key: has_sec,
            public_key_masked: masked,
        }
    }
}
