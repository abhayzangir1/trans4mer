use tauri::State;
use trans4mers_domain::ids::ExecutionId;
use trans4mers_domain::langfuse::LangfuseConfigStatus;
use trans4mers_engine::app_state::AppState;
use trans4mers_providers::langfuse::LangfuseClient;
use trans4mers_storage::repos::settings_repo;

use crate::langfuse_observer::LangfuseObserver;
use crate::settings::KeyringManager;

#[tauri::command]
pub async fn get_langfuse_config(
    state: State<'_, AppState>,
) -> Result<LangfuseConfigStatus, String> {
    let config = LangfuseObserver::load_config(&state);
    Ok(LangfuseConfigStatus::from_config(&config))
}

#[tauri::command]
pub async fn save_langfuse_config(
    host: String,
    enabled: bool,
    capture_prompts: bool,
    public_key: Option<String>,
    secret_key: Option<String>,
    state: State<'_, AppState>,
) -> Result<(), String> {
    let clean_host = host.trim().to_string();

    state
        .global_db
        .with_write_tx(|conn| {
            settings_repo::set_app_setting(conn, "langfuse_host", &clean_host)?;
            settings_repo::set_app_setting(
                conn,
                "langfuse_enabled",
                if enabled { "true" } else { "false" },
            )?;
            settings_repo::set_app_setting(
                conn,
                "langfuse_capture_prompts",
                if capture_prompts { "true" } else { "false" },
            )?;
            Ok(())
        })
        .map_err(|e| format!("Failed to save Langfuse settings: {}", e))?;

    if let Some(pk) = public_key {
        let trimmed = pk.trim();
        if !trimmed.is_empty() && trimmed != "********" {
            KeyringManager::set_api_key("langfuse:public_key", trimmed)
                .map_err(|e| format!("Failed to store public key in keyring: {}", e))?;
        }
    }

    if let Some(sk) = secret_key {
        let trimmed = sk.trim();
        if !trimmed.is_empty() && trimmed != "********" {
            KeyringManager::set_api_key("langfuse:secret_key", trimmed)
                .map_err(|e| format!("Failed to store secret key in keyring: {}", e))?;
        }
    }

    Ok(())
}

#[tauri::command]
pub async fn test_langfuse_connection(
    host: String,
    public_key: Option<String>,
    secret_key: Option<String>,
) -> Result<String, String> {
    let clean_host = host.trim();
    if clean_host.is_empty() {
        return Err("Host endpoint cannot be empty".to_string());
    }

    let resolved_pub = public_key
        .filter(|k| !k.trim().is_empty() && k != "********")
        .or_else(|| KeyringManager::get_api_key("langfuse:public_key").ok())
        .or_else(|| KeyringManager::get_api_key("langfuse_public_key").ok());

    let resolved_sec = secret_key
        .filter(|k| !k.trim().is_empty() && k != "********")
        .or_else(|| KeyringManager::get_api_key("langfuse:secret_key").ok())
        .or_else(|| KeyringManager::get_api_key("langfuse_secret_key").ok());

    let client = LangfuseClient::new();
    client
        .test_connection(clean_host, resolved_pub.as_deref(), resolved_sec.as_deref())
        .await
        .map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn sync_execution_to_langfuse(
    execution_id: String,
    state: State<'_, AppState>,
) -> Result<(), String> {
    let exec_id = ExecutionId::from_str(&execution_id).map_err(|e| e.to_string())?;
    LangfuseObserver::process_execution(&state, &exec_id)
        .await
        .map_err(|e| e.to_string())
}
