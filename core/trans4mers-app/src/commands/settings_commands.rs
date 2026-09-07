use crate::settings::{ApiKeyStatus, KeyringManager};
use serde::Serialize;
use std::sync::Arc;
use tauri::State;
use trans4mers_domain::config::{
    CompactionConfig, DiffReviewConfig, FeatureToggles, PerformanceProfile,
};
use trans4mers_engine::app_state::AppState;
use trans4mers_providers::llm::{
    AnthropicProvider, GoogleProvider, OllamaProvider, OpenAiCompatProvider,
};

#[derive(Serialize)]
pub struct AppSettingsResponse {
    pub provider: String,
    pub keys: Vec<ApiKeyStatus>,
    pub compaction: CompactionConfig,
    pub diff_review: DiffReviewConfig,
    pub performance_profile: PerformanceProfile,
    pub features: FeatureToggles,
    pub ollama_endpoint: String,
}

#[tauri::command]
pub async fn get_settings(state: State<'_, AppState>) -> Result<AppSettingsResponse, String> {
    let providers = vec!["ollama", "openai", "anthropic", "google", "openrouter"];
    let mut keys = Vec::new();
    for p in providers {
        let is_set = KeyringManager::get_api_key(p).is_ok()
            || (p == "google" && KeyringManager::get_api_key("gemini").is_ok());
        keys.push(ApiKeyStatus {
            provider: p.to_string(),
            is_set,
        });
    }

    let config = state.config.read().await;
    let provider = config
        .providers
        .first()
        .map(|p| p.name.clone())
        .unwrap_or_else(|| "ollama".to_string());
    let compaction = config.compaction.clone();
    let diff_review = config.diff_review.clone();
    let performance_profile = config.performance_profile.clone();
    let features = config.features.clone();
    let ollama_endpoint = config
        .providers
        .iter()
        .find(|p| p.name == "ollama")
        .map(|p| p.endpoint.clone())
        .unwrap_or_else(|| "http://127.0.0.1:11434".to_string());

    Ok(AppSettingsResponse {
        provider,
        keys,
        compaction,
        diff_review,
        performance_profile,
        features,
        ollama_endpoint,
    })
}

#[tauri::command]
pub async fn update_settings(
    provider: String,
    api_key: String,
    state: State<'_, AppState>,
) -> Result<(), String> {
    let lower = provider.to_lowercase();
    if !api_key.is_empty() {
        KeyringManager::set_api_key(&lower, &api_key).map_err(|e| e.to_string())?;

        // Dynamically update the running AppState provider_registry
        match lower.as_str() {
            "openai" => {
                state
                    .provider_registry
                    .register(Arc::new(OpenAiCompatProvider::new(
                        "openai".to_string(),
                        "https://api.openai.com/v1".to_string(),
                        api_key,
                    )));
            }
            "openrouter" => {
                state
                    .provider_registry
                    .register(Arc::new(OpenAiCompatProvider::new(
                        "openrouter".to_string(),
                        "https://openrouter.ai/api/v1".to_string(),
                        api_key,
                    )));
            }
            "anthropic" => {
                state
                    .provider_registry
                    .register(Arc::new(AnthropicProvider::new(api_key, None)));
            }
            "google" | "gemini" => {
                state
                    .provider_registry
                    .register(Arc::new(GoogleProvider::new(api_key, None)));
            }
            _ => {}
        }
        Ok(())
    } else {
        let _ = KeyringManager::delete_api_key(&lower);
        Ok(())
    }
}

#[tauri::command]
pub async fn update_feature_toggles(
    features: FeatureToggles,
    state: State<'_, AppState>,
) -> Result<(), String> {
    state.config.write().await.features = features;
    Ok(())
}

#[tauri::command]
pub async fn update_provider_endpoint(
    provider: String,
    endpoint: String,
    state: State<'_, AppState>,
) -> Result<(), String> {
    if provider.to_lowercase() == "ollama" && !endpoint.is_empty() {
        state
            .provider_registry
            .register(Arc::new(OllamaProvider::new(endpoint.clone())));
        let mut config = state.config.write().await;
        if let Some(entry) = config.providers.iter_mut().find(|p| p.name == "ollama") {
            entry.endpoint = endpoint;
        }
    }
    Ok(())
}

#[tauri::command]
pub async fn update_compaction_config(
    config: CompactionConfig,
    state: State<'_, AppState>,
) -> Result<(), String> {
    state.config.write().await.compaction = config;
    Ok(())
}

#[tauri::command]
pub async fn update_diff_review_config(
    config: DiffReviewConfig,
    state: State<'_, AppState>,
) -> Result<(), String> {
    state.config.write().await.diff_review = config;
    Ok(())
}

#[tauri::command]
pub async fn update_performance_profile(
    profile: PerformanceProfile,
    state: State<'_, AppState>,
) -> Result<(), String> {
    state.config.write().await.performance_profile = profile;
    Ok(())
}

#[tauri::command]
pub async fn set_default_provider(
    provider: String,
    state: State<'_, AppState>,
) -> Result<(), String> {
    let lower = provider.to_lowercase();
    let mut config = state.config.write().await;
    if let Some(pos) = config
        .providers
        .iter()
        .position(|p| p.name.to_lowercase() == lower)
    {
        let p = config.providers.remove(pos);
        config.providers.insert(0, p);
    } else {
        let is_local = lower == "ollama";
        let endpoint = match lower.as_str() {
            "openai" => "https://api.openai.com/v1".to_string(),
            "anthropic" => "https://api.anthropic.com/v1".to_string(),
            "openrouter" => "https://openrouter.ai/api/v1".to_string(),
            "google" | "gemini" => "https://generativelanguage.googleapis.com/v1beta".to_string(),
            _ => "http://127.0.0.1:11434".to_string(),
        };
        config.providers.insert(
            0,
            trans4mers_domain::config::ProviderEntry {
                name: lower.clone(),
                endpoint,
                is_local,
                is_free: is_local,
                requires_api_key: !is_local,
                enabled: true,
                models: Vec::new(),
            },
        );
    }
    Ok(())
}
