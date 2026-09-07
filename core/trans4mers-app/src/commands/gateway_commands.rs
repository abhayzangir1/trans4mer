use serde::{Deserialize, Serialize};
use tauri::State;
use trans4mers_engine::app_state::AppState;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GatewayStatus {
    pub telegram_enabled: bool,
    pub telegram_configured: bool,
    pub allowed_chat_ids_count: usize,
    pub default_project_id: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct TelegramGetMeResponse {
    pub ok: bool,
    pub result: Option<TelegramBotInfo>,
    pub description: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct TelegramBotInfo {
    pub id: i64,
    pub is_bot: bool,
    pub first_name: String,
    pub username: Option<String>,
}

#[tauri::command]
pub async fn get_gateway_status(state: State<'_, AppState>) -> Result<GatewayStatus, String> {
    let cfg = state.config.read().await;
    Ok(GatewayStatus {
        telegram_enabled: cfg.telegram.enabled,
        telegram_configured: cfg
            .telegram
            .bot_token
            .as_ref()
            .map(|t| !t.trim().is_empty())
            .unwrap_or(false),
        allowed_chat_ids_count: cfg.telegram.allowed_chat_ids.len(),
        default_project_id: cfg.telegram.default_project_id.clone(),
    })
}

#[tauri::command]
pub async fn update_telegram_config(
    enabled: bool,
    bot_token: Option<String>,
    allowed_chat_ids: Vec<i64>,
    default_project_id: Option<String>,
    state: State<'_, AppState>,
) -> Result<(), String> {
    let mut cfg = state.config.write().await;
    cfg.telegram.enabled = enabled;
    cfg.telegram.bot_token = bot_token;
    cfg.telegram.allowed_chat_ids = allowed_chat_ids;
    cfg.telegram.default_project_id = default_project_id;
    Ok(())
}

#[tauri::command]
pub async fn test_telegram_connection(bot_token: String) -> Result<String, String> {
    let token = bot_token.trim();
    if token.is_empty() {
        return Err("Telegram bot token cannot be empty".to_string());
    }

    let url = format!("https://api.telegram.org/bot{}/getMe", token);
    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(10))
        .build()
        .map_err(|e| e.to_string())?;

    let resp = client
        .get(&url)
        .send()
        .await
        .map_err(|e| format!("Failed to reach Telegram API: {}", e))?;
    let body: TelegramGetMeResponse = resp
        .json()
        .await
        .map_err(|e| format!("Invalid Telegram response: {}", e))?;

    if body.ok {
        if let Some(bot) = body.result {
            Ok(format!(
                "Connected successfully to Telegram bot: @{} ({})",
                bot.username.unwrap_or_default(),
                bot.first_name
            ))
        } else {
            Ok("Connected successfully to Telegram bot".to_string())
        }
    } else {
        Err(body
            .description
            .unwrap_or_else(|| "Telegram API rejected token".to_string()))
    }
}
