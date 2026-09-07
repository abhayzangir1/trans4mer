use chrono::Utc;
use std::sync::Arc;
use tauri::State;
use trans4mers_domain::ids::ProjectId;
use trans4mers_domain::token_usage::CostBudget;
use trans4mers_domain::tool::McpServerEntry;
use trans4mers_engine::app_state::AppState;
use trans4mers_storage::repos::{cost_repo, mcp_registry_repo};

#[tauri::command]
pub async fn get_token_usage(
    project_id: String,
    agent_id: Option<String>,
    state: State<'_, AppState>,
) -> Result<i64, String> {
    let proj_id = ProjectId::from_str(&project_id).map_err(|e| e.to_string())?;
    let db = state
        .get_project_db(&proj_id)
        .ok_or_else(|| "Project DB not found".to_string())?;
    let count: i64 = db.with_read_conn(|conn| {
        let mut stmt = conn.prepare(
            "
            SELECT COALESCE(SUM(u.total_tokens), 0)
            FROM token_usages u
            JOIN agent_instances a ON u.agent_instance_id = a.id
            WHERE a.project_id = ?1 AND (?2 IS NULL OR a.id = ?2)
        ",
        )?;
        let sum = stmt.query_row(rusqlite::params![proj_id.as_str(), agent_id], |row| {
            row.get(0)
        })?;
        Ok(sum)
    })?;
    Ok(count)
}

#[tauri::command]
pub async fn replay_events(
    project_id: String,
    from_sequence: Option<i64>,
    state: State<'_, AppState>,
) -> Result<u32, String> {
    let proj_id = ProjectId::from_str(&project_id).map_err(|e| e.to_string())?;
    let db = state
        .get_project_db(&proj_id)
        .ok_or_else(|| "Project DB not found".to_string())?;

    let events = db.with_read_conn(|conn| {
        let evs = trans4mers_storage::repos::event_repo::EventRepo::get_events_since(
            conn,
            from_sequence.unwrap_or(0),
        )?;
        Ok(evs)
    })?;

    let count = events.len() as u32;

    let event_bus = state.get_event_bus(&proj_id);
    let global_bus = &state.global_event_bus;

    for envelope in events {
        let env_arc = std::sync::Arc::new(envelope);
        let _ = event_bus.publish(env_arc.clone());
        let _ = global_bus.publish(env_arc);
    }

    Ok(count)
}

#[tauri::command]
pub async fn load_plugin(
    project_id: String,
    plugin_path: String,
    state: State<'_, AppState>,
) -> Result<usize, String> {
    let proj_id = ProjectId::from_str(&project_id).map_err(|e| e.to_string())?;
    let path = std::path::PathBuf::from(plugin_path);

    let runner = trans4mers_engine::PluginRunner::load(path)
        .await
        .map_err(|e| e.to_string())?;

    let tools = runner
        .initialize_plugin()
        .await
        .map_err(|e| e.to_string())?;

    let count = tools.len();

    let provider = state
        .provider_registry
        .get("ollama")
        .or_else(|_| state.provider_registry.get("openai"))
        .or_else(|_| state.provider_registry.get_default())
        .map_err(|e| e.to_string())?;
    let executor = state.get_or_create_tool_executor(&proj_id, provider).await;
    for tool in tools {
        executor.register_tool(tool).await;
    }
    Ok(count)
}

#[tauri::command]
pub async fn connect_mcp_server(
    project_id: String,
    command: String,
    state: State<'_, AppState>,
) -> Result<usize, String> {
    let proj_id = ProjectId::from_str(&project_id).map_err(|e| e.to_string())?;

    // Invariant: Verify MCP server is registered and explicitly approved before launch
    let maybe_entry = state
        .global_db
        .with_read_conn(|conn| {
            if let Ok(Some(entry)) = mcp_registry_repo::get_server(conn, &command) {
                return Ok(Some(entry));
            }
            // If not found by name, try looking up by command
            let mut stmt = conn.prepare(
                "SELECT name, command, args, env, auto_launch, approved, notes, created_at, updated_at
                 FROM mcp_server_registry WHERE command = ?1 LIMIT 1",
            )?;
            let mut rows = stmt.query([&command])?;
            if let Some(row) = rows.next()? {
                let name: String = row.get(0)?;
                let command: String = row.get(1)?;
                let args_json: String = row.get(2)?;
                let env_json: String = row.get(3)?;
                let auto_launch_int: i32 = row.get(4)?;
                let approved_int: i32 = row.get(5)?;
                let notes: Option<String> = row.get(6)?;
                let created_str: String = row.get(7)?;
                let updated_str: String = row.get(8)?;

                let args: Vec<String> = serde_json::from_str(&args_json).unwrap_or_default();
                let env: std::collections::HashMap<String, String> =
                    serde_json::from_str(&env_json).unwrap_or_default();

                Ok(Some(trans4mers_domain::tool::McpServerEntry {
                    name,
                    command,
                    args,
                    env,
                    auto_launch: auto_launch_int != 0,
                    approved: approved_int != 0,
                    notes,
                    created_at: trans4mers_storage::datetime_util::parse_db_datetime(&created_str),
                    updated_at: trans4mers_storage::datetime_util::parse_db_datetime(&updated_str),
                }))
            } else {
                Ok(None)
            }
        })
        .map_err(|e: trans4mers_domain::error::Trans4mersError| e.to_string())?;

    let (target_cmd, token, headers, s_name) = if let Some(entry) = maybe_entry {
        if !entry.approved {
            return Err(format!(
                "Policy Denied: MCP server '{}' is not approved. Enable and approve it in Settings > MCP Registry first.",
                entry.name
            ));
        }
        let auth_token =
            crate::settings::KeyringManager::get_api_key(&format!("mcp:{}", entry.name)).ok();
        let custom_headers = if entry.env.is_empty() {
            None
        } else {
            Some(entry.env)
        };
        (entry.command, auth_token, custom_headers, entry.name)
    } else {
        return Err(format!(
            "Policy Denied: MCP server '{}' is not registered. Register and approve it in Settings > MCP Registry first.",
            command
        ));
    };

    let db_clone = state.global_db.clone();
    let event_bus_clone = state.global_event_bus.clone();

    let traffic_sink: trans4mers_providers::mcp_client::McpTrafficSink = Arc::new(move |frame| {
        let _ = db_clone.with_write_tx(|conn| {
            trans4mers_storage::repos::mcp_traffic_repo::McpTrafficRepo::insert(conn, &frame)
        });
        let envelope = trans4mers_domain::event::EventEnvelope {
            sequence_id: 0,
            event_id: trans4mers_domain::ids::EventId::new(),
            event: trans4mers_domain::event::DomainEvent::McpFrameLogged { frame },
            actor_id: trans4mers_domain::ids::ActorId::new(),
            signature: None,
            created_at: chrono::Utc::now(),
        };
        let _ = event_bus_clone.publish(Arc::new(envelope));
    });

    let client = trans4mers_providers::mcp_client::McpClient::connect_named(
        &s_name,
        &target_cmd,
        token,
        headers,
        Some(traffic_sink),
    )
    .await
    .map_err(|e| e.to_string())?;

    let _ = client.initialize().await;
    let tools = client.list_tools().await.map_err(|e| e.to_string())?;

    let count = tools.len();

    let provider = state
        .provider_registry
        .get("ollama")
        .or_else(|_| state.provider_registry.get("openai"))
        .or_else(|_| state.provider_registry.get_default())
        .map_err(|e| e.to_string())?;
    let executor = state.get_or_create_tool_executor(&proj_id, provider).await;
    for tool in tools {
        executor.register_tool(tool).await;
    }
    Ok(count)
}

#[tauri::command]
pub async fn cmd_get_system_status(
    state: tauri::State<'_, trans4mers_engine::app_state::AppState>,
) -> Result<serde_json::Value, String> {
    let db_status = match state.global_db.with_read_conn(|conn| {
        conn.query_row("SELECT 1", [], |_| Ok(()))?;
        Ok(())
    }) {
        Ok(_) => "Connected",
        Err(_) => "Error",
    };

    let endpoint = {
        let cfg = state.config.read().await;
        cfg.providers
            .iter()
            .find(|p| p.name == "ollama")
            .map(|p| p.endpoint.clone())
            .unwrap_or_else(|| "http://localhost:11434".to_string())
    };
    let tags_url = format!("{}/api/tags", endpoint.trim_end_matches('/'));

    let client = reqwest::Client::new();
    let ollama_status = match client
        .get(&tags_url)
        .timeout(std::time::Duration::from_millis(1500))
        .send()
        .await
    {
        Ok(r) if r.status().is_success() => "Online",
        _ => "Offline",
    };

    let active_executions = state.execution_cancellation_tokens.len();
    let active_projects = state.project_dbs.len();

    let mut active_terminals = 0;
    for tm in state.terminal_managers.iter() {
        active_terminals += tm.value().active_sessions_count();
    }

    Ok(serde_json::json!({
        "database_status": db_status,
        "ollama_status": ollama_status,
        "active_executions": active_executions,
        "active_projects": active_projects,
        "active_terminals": active_terminals,
        "scheduler_status": "Running"
    }))
}

#[tauri::command]
pub async fn get_cost_summary(
    provider: Option<String>,
    project_id: Option<String>,
    state: State<'_, AppState>,
) -> Result<serde_json::Value, String> {
    let prov = provider.as_deref().unwrap_or("openai");

    state
        .global_db
        .with_read_conn(|conn| {
            let daily = cost_repo::get_daily_cost(conn, prov, project_id.as_deref())?;
            let monthly = cost_repo::get_monthly_cost(conn, prov, project_id.as_deref())?;
            let budget = cost_repo::get_budget(conn, prov, project_id.as_deref())?;

            Ok(serde_json::json!({
                "provider": prov,
                "daily_usd": daily,
                "monthly_usd": monthly,
                "budget": budget,
            }))
        })
        .map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn set_cost_budget(
    provider: String,
    project_id: Option<String>,
    monthly_ceiling_usd: Option<f32>,
    daily_ceiling_usd: Option<f32>,
    hard_block: bool,
    state: State<'_, AppState>,
) -> Result<(), String> {
    let budget_id = format!("{}:{}", provider, project_id.as_deref().unwrap_or("global"));
    let budget = CostBudget {
        id: budget_id,
        project_id,
        provider,
        monthly_ceiling_usd,
        daily_ceiling_usd,
        hard_block,
        alert_thresholds: vec![0.5, 0.8, 1.0],
        created_at: Utc::now(),
        updated_at: Utc::now(),
    };

    state
        .global_db
        .with_write_tx(|conn| cost_repo::upsert_budget(conn, &budget))
        .map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn list_mcp_servers(state: State<'_, AppState>) -> Result<Vec<McpServerEntry>, String> {
    state
        .global_db
        .with_read_conn(mcp_registry_repo::list_servers)
        .map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn register_mcp_server(
    name: String,
    command: String,
    args: Option<Vec<String>>,
    env: Option<std::collections::HashMap<String, String>>,
    auto_launch: bool,
    notes: Option<String>,
    state: State<'_, AppState>,
) -> Result<(), String> {
    let entry = McpServerEntry {
        name,
        command,
        args: args.unwrap_or_default(),
        env: env.unwrap_or_default(),
        auto_launch,
        approved: false,
        notes,
        created_at: Utc::now(),
        updated_at: Utc::now(),
    };

    state
        .global_db
        .with_write_tx(|conn| mcp_registry_repo::register_server(conn, &entry))
        .map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn toggle_mcp_approval(
    name: String,
    approved: bool,
    state: State<'_, AppState>,
) -> Result<(), String> {
    state
        .global_db
        .with_write_tx(|conn| mcp_registry_repo::set_approval(conn, &name, approved))
        .map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn delete_mcp_server(name: String, state: State<'_, AppState>) -> Result<(), String> {
    let _ = crate::settings::KeyringManager::delete_api_key(&format!("mcp:{}", name));
    state
        .global_db
        .with_write_tx(|conn| mcp_registry_repo::delete_server(conn, &name))
        .map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn set_mcp_auth_token(name: String, token: String) -> Result<(), String> {
    let trimmed = token.trim();
    if trimmed.is_empty() {
        let _ = crate::settings::KeyringManager::delete_api_key(&format!("mcp:{}", name));
    } else {
        crate::settings::KeyringManager::set_api_key(&format!("mcp:{}", name), trimmed)?;
    }
    Ok(())
}

#[tauri::command]
pub async fn get_mcp_auth_status(name: String) -> Result<bool, String> {
    Ok(crate::settings::KeyringManager::get_api_key(&format!("mcp:{}", name)).is_ok())
}

fn inspector_manager() -> &'static trans4mers_providers::mcp_inspector::McpInspectorManager {
    static MGR: std::sync::OnceLock<trans4mers_providers::mcp_inspector::McpInspectorManager> =
        std::sync::OnceLock::new();
    MGR.get_or_init(trans4mers_providers::mcp_inspector::McpInspectorManager::new)
}

#[tauri::command]
pub async fn get_mcp_traffic_logs(
    server_name: String,
    limit: Option<u32>,
    state: State<'_, AppState>,
) -> Result<Vec<trans4mers_domain::event::McpTrafficFrame>, String> {
    state
        .global_db
        .with_read_conn(|conn| {
            trans4mers_storage::repos::mcp_traffic_repo::McpTrafficRepo::list_by_server(
                conn,
                &server_name,
                limit.unwrap_or(100),
            )
        })
        .map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn clear_mcp_traffic_logs(
    server_name: String,
    state: State<'_, AppState>,
) -> Result<(), String> {
    state
        .global_db
        .with_write_tx(|conn| {
            trans4mers_storage::repos::mcp_traffic_repo::McpTrafficRepo::clear_by_server(
                conn,
                &server_name,
            )
        })
        .map_err(|e| e.to_string())
}

#[tauri::command]
pub fn check_node_environment()
-> Result<trans4mers_providers::mcp_inspector::NodeEnvironment, String> {
    Ok(trans4mers_providers::mcp_inspector::detect_node_environment())
}

#[tauri::command]
pub async fn launch_mcp_inspector(
    server_name: String,
    state: State<'_, AppState>,
) -> Result<trans4mers_providers::mcp_inspector::McpInspectorLaunchResult, String> {
    let entry = state
        .global_db
        .with_read_conn(|conn| mcp_registry_repo::get_server(conn, &server_name))
        .map_err(|e| e.to_string())?
        .ok_or_else(|| format!("Server '{}' is not registered", server_name))?;

    inspector_manager()
        .launch(&entry.name, &entry.command, &entry.args)
        .await
        .map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn stop_mcp_inspector() -> Result<(), String> {
    inspector_manager().stop().await.map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn get_mcp_inspector_status()
-> Result<trans4mers_providers::mcp_inspector::McpInspectorStatus, String> {
    Ok(inspector_manager().status().await)
}

#[tauri::command]
pub async fn send_mcp_ping(
    server_name: String,
    state: State<'_, AppState>,
) -> Result<String, String> {
    let entry = state
        .global_db
        .with_read_conn(|conn| mcp_registry_repo::get_server(conn, &server_name))
        .map_err(|e| e.to_string())?
        .ok_or_else(|| format!("Server '{}' is not registered", server_name))?;

    if !entry.approved {
        return Err(format!("Server '{}' is not approved", server_name));
    }

    let auth_token =
        crate::settings::KeyringManager::get_api_key(&format!("mcp:{}", entry.name)).ok();
    let custom_headers = if entry.env.is_empty() {
        None
    } else {
        Some(entry.env)
    };

    let db_clone = state.global_db.clone();
    let event_bus_clone = state.global_event_bus.clone();
    let s_name = entry.name.clone();

    let traffic_sink: trans4mers_providers::mcp_client::McpTrafficSink = Arc::new(move |frame| {
        let _ = db_clone.with_write_tx(|conn| {
            trans4mers_storage::repos::mcp_traffic_repo::McpTrafficRepo::insert(conn, &frame)
        });
        let envelope = trans4mers_domain::event::EventEnvelope {
            sequence_id: 0,
            event_id: trans4mers_domain::ids::EventId::new(),
            event: trans4mers_domain::event::DomainEvent::McpFrameLogged { frame },
            actor_id: trans4mers_domain::ids::ActorId::new(),
            signature: None,
            created_at: chrono::Utc::now(),
        };
        let _ = event_bus_clone.publish(Arc::new(envelope));
    });

    let client = trans4mers_providers::mcp_client::McpClient::connect_named(
        &s_name,
        &entry.command,
        auth_token,
        custom_headers,
        Some(traffic_sink),
    )
    .await
    .map_err(|e| e.to_string())?;

    let _ = client.initialize().await;
    client.ping().await.map_err(|e| e.to_string())?;
    Ok("pong".to_string())
}
