use crate::settings::KeyringManager;

#[tauri::command]
pub async fn test_provider_connection(
    name: String,
    endpoint: String,
    model: Option<String>,
    api_key: Option<String>,
) -> Result<bool, String> {
    let _ = model;
    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(10))
        .build()
        .map_err(|e| e.to_string())?;

    let url = if endpoint.is_empty() {
        match name.to_lowercase().as_str() {
            "openai" => "https://api.openai.com/v1/models".to_string(),
            "anthropic" => "https://api.anthropic.com/v1/models".to_string(),
            "google" | "gemini" => {
                let key = api_key.clone().unwrap_or_default();
                format!(
                    "https://generativelanguage.googleapis.com/v1beta/models?key={}",
                    key
                )
            }
            "openrouter" => "https://openrouter.ai/api/v1/models".to_string(),
            "ollama" => "http://localhost:11434/api/tags".to_string(),
            _ => return Err("Endpoint is required for custom provider".to_string()),
        }
    } else {
        let trimmed = endpoint.trim_end_matches('/');
        if name.to_lowercase() == "ollama" && !trimmed.ends_with("/api/tags") {
            format!("{}/api/tags", trimmed)
        } else if !trimmed.ends_with("/models")
            && (name.to_lowercase() == "openai" || name.to_lowercase() == "openrouter")
        {
            format!("{}/models", trimmed)
        } else {
            endpoint.clone()
        }
    };

    let mut req = client.get(&url);
    if let Some(key) = api_key {
        let trimmed_key = key.trim();
        if !trimmed_key.is_empty() {
            if name.to_lowercase() == "anthropic" {
                req = req
                    .header("x-api-key", trimmed_key)
                    .header("anthropic-version", "2023-06-01");
            } else if name.to_lowercase() != "google"
                && name.to_lowercase() != "gemini"
                && name.to_lowercase() != "ollama"
            {
                req = req.bearer_auth(trimmed_key);
            }
        }
    }

    match req.send().await {
        Ok(r) => Ok(r.status().is_success()),
        Err(e) => Err(e.to_string()),
    }
}

fn resolve_key(provider_name: &str, direct_key: Option<String>) -> Option<String> {
    if let Some(k) = direct_key {
        let trimmed = k.trim().to_string();
        if !trimmed.is_empty() {
            return Some(trimmed);
        }
    }

    if let Ok(k) = KeyringManager::get_api_key(provider_name)
        && !k.trim().is_empty()
    {
        return Some(k);
    }

    let env_var = match provider_name.to_lowercase().as_str() {
        "openai" => "OPENAI_API_KEY",
        "anthropic" => "ANTHROPIC_API_KEY",
        "google" | "gemini" => "GEMINI_API_KEY",
        "openrouter" => "OPENROUTER_API_KEY",
        _ => return None,
    };

    std::env::var(env_var).ok().filter(|s| !s.trim().is_empty())
}

#[tauri::command]
pub async fn list_available_models(
    name: String,
    api_key: Option<String>,
    endpoint: Option<String>,
) -> Result<Vec<String>, String> {
    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(12))
        .build()
        .map_err(|e| e.to_string())?;

    let lower_name = name.to_lowercase();
    match lower_name.as_str() {
        "openai" => {
            let key = resolve_key("openai", api_key).ok_or_else(|| {
                "OpenAI API key not provided or found in secure storage.".to_string()
            })?;

            let url = endpoint.unwrap_or_else(|| "https://api.openai.com/v1/models".to_string());
            let resp = client
                .get(&url)
                .bearer_auth(key)
                .send()
                .await
                .map_err(|e| format!("Failed to query OpenAI models: {}", e))?;

            if !resp.status().is_success() {
                return Err(format!("OpenAI API error status: {}", resp.status()));
            }

            let json: serde_json::Value = resp.json().await.map_err(|e| e.to_string())?;
            let mut models = Vec::new();
            if let Some(arr) = json.get("data").and_then(|d| d.as_array()) {
                for item in arr {
                    if let Some(id) = item.get("id").and_then(|i| i.as_str()) {
                        // Include chat-capable or reasoned models
                        if id.starts_with("gpt-")
                            || id.starts_with("o1")
                            || id.starts_with("o3")
                            || id.contains("chat")
                            || id.contains("instruct")
                        {
                            models.push(id.to_string());
                        }
                    }
                }
            }
            models.sort();
            Ok(models)
        }
        "anthropic" => {
            let key = resolve_key("anthropic", api_key).ok_or_else(|| {
                "Anthropic API key not provided or found in secure storage.".to_string()
            })?;

            let url = endpoint.unwrap_or_else(|| "https://api.anthropic.com/v1/models".to_string());
            let resp = client
                .get(&url)
                .header("x-api-key", key)
                .header("anthropic-version", "2023-06-01")
                .send()
                .await
                .map_err(|e| format!("Failed to query Anthropic models: {}", e))?;

            if !resp.status().is_success() {
                return Err(format!(
                    "Anthropic returned error status: {}",
                    resp.status()
                ));
            }

            let json: serde_json::Value = resp.json().await.map_err(|e| e.to_string())?;
            let mut models = Vec::new();
            if let Some(arr) = json.get("data").and_then(|d| d.as_array()) {
                for item in arr {
                    if let Some(id) = item.get("id").and_then(|i| i.as_str()) {
                        models.push(id.to_string());
                    }
                }
            }
            models.sort();
            Ok(models)
        }
        "google" | "gemini" => {
            let key = resolve_key("google", api_key)
                .or_else(|| resolve_key("gemini", None))
                .ok_or_else(|| {
                    "Google Gemini API key not provided or found in secure storage.".to_string()
                })?;

            let url = format!(
                "https://generativelanguage.googleapis.com/v1beta/models?key={}",
                key
            );
            let resp = client
                .get(&url)
                .send()
                .await
                .map_err(|e| format!("Failed to query Google Gemini models: {}", e))?;

            if !resp.status().is_success() {
                return Err(format!(
                    "Google Gemini returned error status: {}",
                    resp.status()
                ));
            }

            let json: serde_json::Value = resp.json().await.map_err(|e| e.to_string())?;
            let mut models = Vec::new();
            if let Some(arr) = json.get("models").and_then(|m| m.as_array()) {
                for item in arr {
                    if let Some(name) = item.get("name").and_then(|n| n.as_str()) {
                        // Strips the "models/" prefix if present e.g. "models/gemini-2.0-flash" -> "gemini-2.0-flash"
                        let clean_name = name.strip_prefix("models/").unwrap_or(name);
                        if clean_name.starts_with("gemini") {
                            models.push(clean_name.to_string());
                        }
                    }
                }
            }
            models.sort();
            Ok(models)
        }
        "openrouter" => {
            let key = resolve_key("openrouter", api_key).ok_or_else(|| {
                "OpenRouter API key not provided or found in secure storage.".to_string()
            })?;

            let resp = client
                .get("https://openrouter.ai/api/v1/models")
                .bearer_auth(key)
                .send()
                .await
                .map_err(|e| format!("Failed to query OpenRouter models: {}", e))?;

            if !resp.status().is_success() {
                return Err(format!("OpenRouter error status: {}", resp.status()));
            }

            let json: serde_json::Value = resp.json().await.map_err(|e| e.to_string())?;
            let mut models = Vec::new();
            if let Some(arr) = json.get("data").and_then(|d| d.as_array()) {
                for item in arr {
                    if let Some(id) = item.get("id").and_then(|i| i.as_str()) {
                        models.push(id.to_string());
                    }
                }
            }
            models.sort();
            Ok(models)
        }
        "ollama" => {
            let base_url = endpoint.unwrap_or_else(|| "http://localhost:11434".to_string());
            let clean_base = base_url.trim_end_matches('/');
            let tags_url = format!("{}/api/tags", clean_base);

            let resp = client
                .get(&tags_url)
                .timeout(std::time::Duration::from_secs(4))
                .send()
                .await
                .map_err(|e| format!("Ollama unreachable at {}: {}", tags_url, e))?;

            if !resp.status().is_success() {
                return Err(format!("Ollama returned error status: {}", resp.status()));
            }

            let json: serde_json::Value = resp.json().await.map_err(|e| e.to_string())?;
            let mut names = Vec::new();
            if let Some(arr) = json.get("models").and_then(|m| m.as_array()) {
                for m in arr {
                    if let Some(name) = m.get("name").and_then(|n| n.as_str()) {
                        names.push(name.to_string());
                    }
                }
            }
            names.sort();
            Ok(names)
        }
        _ => Err(format!("Unsupported provider '{}'", name)),
    }
}

/// Offline Model Guidance IPC: retrieves metadata, capabilities, size class, and hints for a model
/// with strictly ZERO network egress in the picker path.
#[tauri::command]
pub async fn get_model_guidance(
    model: String,
) -> Result<trans4mers_domain::model_guidance::ModelGuidance, String> {
    Ok(trans4mers_domain::model_guidance::get_model_guidance(
        &model,
    ))
}

/// Offline Model Guidance Catalog IPC: retrieves the full static catalog of pre-profiled models.
#[tauri::command]
pub async fn list_model_guidance_catalog()
-> Result<Vec<trans4mers_domain::model_guidance::ModelGuidance>, String> {
    Ok(trans4mers_domain::model_guidance::list_model_guidance_catalog().to_vec())
}
