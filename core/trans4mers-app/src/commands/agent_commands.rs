use rusqlite::OptionalExtension;
use tauri::State;
use trans4mers_domain::agent::AgentInstance;
use trans4mers_domain::error::Trans4mersError;
use trans4mers_domain::ids::{
    AgentDefinitionId, AgentInstanceId, ChannelId, ConversationId, ExecutionId, MessageId,
    ProjectId,
};
use trans4mers_domain::state::AgentStatus;
use trans4mers_engine::app_state::AppState;

#[derive(serde::Serialize, serde::Deserialize, Clone, Debug)]
pub struct EnrichedAgentInstance {
    pub id: String,
    pub project_id: String,
    pub definition_id: String,
    pub name: String,
    pub role: String,
    pub description: String,
    pub status: String,
    pub parent_instance_id: Option<String>,
    pub capabilities: Vec<String>,
    pub model: String,
    pub depth_level: u32,
    pub created_at: String,
    pub updated_at: String,
}

#[tauri::command]
#[allow(clippy::type_complexity)]
pub async fn list_agents(
    project_id: String,
    state: State<'_, AppState>,
) -> Result<Vec<EnrichedAgentInstance>, String> {
    let proj_id = ProjectId::from_str(&project_id).map_err(|e| e.to_string())?;
    let db = state
        .get_project_db(&proj_id)
        .ok_or_else(|| "Project DB not found".to_string())?;

    // First, check if any agent instances exist in project_db
    let raw_agents: Vec<(
        String,
        String,
        String,
        Option<String>,
        String,
        String,
        Option<String>,
        u32,
        String,
        String,
    )> = db
        .with_read_conn(|conn| {
            let mut stmt = conn.prepare(
                "SELECT
                id, project_id, definition_id, parent_instance_id, status,
                capabilities, model_config_override, depth_level, created_at, updated_at
             FROM agent_instances WHERE project_id = ?1 ORDER BY created_at ASC",
            )?;

            let iter = stmt.query_map(rusqlite::params![project_id], |row| {
                Ok((
                    row.get::<_, String>(0)?,
                    row.get::<_, String>(1)?,
                    row.get::<_, String>(2)?,
                    row.get::<_, Option<String>>(3)?,
                    row.get::<_, String>(4)?,
                    row.get::<_, String>(5)?,
                    row.get::<_, Option<String>>(6)?,
                    row.get::<_, u32>(7)?,
                    row.get::<_, String>(8)?,
                    row.get::<_, String>(9)?,
                ))
            })?;

            let mut list = Vec::new();
            for a in iter {
                list.push(a?);
            }
            Ok(list)
        })
        .map_err(|e| e.to_string())?;

    // Enrich with definition metadata from global_db
    let mut enriched = Vec::new();
    for (
        id,
        proj,
        def_id,
        parent_id,
        status,
        caps_str,
        model_override_str,
        depth,
        created_at,
        updated_at,
    ) in raw_agents
    {
        let (name, role, description, default_model) = state.global_db.with_read_conn(|conn| {
            let mut stmt = conn.prepare("SELECT name, role, description, default_model_config FROM agent_definitions WHERE id = ?1")?;
            let res = stmt.query_row(rusqlite::params![def_id], |row| {
                let name: String = row.get(0)?;
                let role: String = row.get(1)?;
                let desc: String = row.get(2)?;
                let model_cfg: String = row.get(3)?;
                let model = serde_json::from_str::<serde_json::Value>(&model_cfg)
                    .ok()
                    .and_then(|v| v.get("model").and_then(|m| m.as_str()).map(|s| s.to_string()))
                    .unwrap_or_else(|| "qwen2.5-coder:3b".to_string());
                Ok((name, role, desc, model))
            }).optional()?;
            Ok(res)
        }).map_err(|e| e.to_string())?.unwrap_or_else(|| {
            (def_id.clone(), "Specialist".to_string(), "".to_string(), "qwen2.5-coder:3b".to_string())
        });

        let caps: Vec<String> = serde_json::from_str(&caps_str).unwrap_or_default();
        let model = model_override_str
            .and_then(|s| serde_json::from_str::<serde_json::Value>(&s).ok())
            .and_then(|v| {
                v.get("model")
                    .and_then(|m| m.as_str())
                    .map(|s| s.to_string())
            })
            .unwrap_or(default_model);

        enriched.push(EnrichedAgentInstance {
            id,
            project_id: proj,
            definition_id: def_id,
            name,
            role,
            description,
            status,
            parent_instance_id: parent_id,
            capabilities: caps,
            model,
            depth_level: depth,
            created_at,
            updated_at,
        });
    }

    Ok(enriched)
}

#[tauri::command]
pub async fn create_agent(
    project_id: String,
    definition_id: String,
    prompt: Option<String>,
    state: State<'_, AppState>,
) -> Result<AgentInstance, String> {
    let id = AgentInstanceId::new();
    let proj_id = ProjectId::from_str(&project_id).map_err(|e| e.to_string())?;
    let _def_id = AgentDefinitionId::from_str(&definition_id).map_err(|e| e.to_string())?;

    let event = trans4mers_domain::event::DomainEvent::AgentSpawned {
        agent_id: id,
        project_id: proj_id,
        definition_id: definition_id.clone(),
        parent_id: None,
    };
    let db = state
        .get_project_db(&proj_id)
        .ok_or_else(|| "Project DB not found".to_string())?;
    let envelope = db.with_write_tx(|conn| {
        trans4mers_engine::cqrs::commit_event(conn, event, state.human_actor_id)
    })?;

    let env_arc = std::sync::Arc::new(envelope);
    let bus = state.get_event_bus(&proj_id);
    let _ = bus.publish(env_arc.clone());
    let _ = state.global_event_bus.publish(env_arc);

    // If there's an initial prompt, route it straight to the agent's inbox and wake the scheduler
    if let Some(initial_prompt) = prompt
        && !initial_prompt.trim().is_empty()
    {
        let msg = trans4mers_domain::message::Message {
            id: MessageId::new(),
            conversation_id: ConversationId::from_uuid(*proj_id.as_uuid()),
            channel_id: ChannelId::from_uuid(*id.as_uuid()),
            sender: trans4mers_domain::actor::Actor::Human {
                id: state.human_actor_id,
                display_name: "Human".to_string(),
            },
            content: initial_prompt,
            mentions: vec![],
            message_kind: trans4mers_domain::message::MessageKind::Chat,
            thread_id: None,
            attachments: vec![],
            requires_approval: false,
            approval_id: None,
            created_at: chrono::Utc::now(),
            updated_at: chrono::Utc::now(),
        };

        let new_exec_id = ExecutionId::new();
        let _ = db.with_write_tx(|conn| {
                conn.execute(
                    "INSERT INTO agent_executions (id, agent_instance_id, conversation_id, status, generation, current_step, max_steps, updated_at) VALUES (?1, ?2, ?3, 'Pending', 0, 0, 25, ?4)",
                    rusqlite::params![new_exec_id.as_str(), id.as_str(), proj_id.as_str(), chrono::Utc::now().to_rfc3339()]
                )?;
                Ok(())
            });

        let inbox_event = trans4mers_domain::event::DomainEvent::InboxMessageQueued {
            message_id: MessageId::new().to_string(),
            recipient_agent_id: id,
            sender_actor_id: msg.sender.to_string(),
            payload: serde_json::to_string(&msg).unwrap_or_default(),
        };

        let envelope = db.with_write_tx(|conn| {
            trans4mers_engine::cqrs::commit_event(conn, inbox_event, state.human_actor_id)
        })?;

        let env_arc = std::sync::Arc::new(envelope);
        let bus = state.get_event_bus(&proj_id);
        let _ = bus.publish(env_arc.clone());
        let _ = state.global_event_bus.publish(env_arc);

        state.scheduler.queue(new_exec_id, proj_id);
    }

    // Return the instance that was created by the projector
    let agent_opt = db.with_read_conn(|conn| {
        trans4mers_storage::repos::agent_inst_repo::get_agent_instance(conn, &id)
    })?;

    agent_opt.ok_or_else(|| "Failed to retrieve projected agent".to_string())
}

#[tauri::command]
pub async fn pause_agent(
    project_id: String,
    agent_instance_id: String,
    state: State<'_, AppState>,
) -> Result<(), String> {
    update_agent_status(project_id, agent_instance_id, AgentStatus::Paused, state).await
}

#[tauri::command]
pub async fn resume_agent(
    project_id: String,
    agent_instance_id: String,
    state: State<'_, AppState>,
) -> Result<(), String> {
    update_agent_status(project_id, agent_instance_id, AgentStatus::Active, state).await
}

async fn update_agent_status(
    project_id: String,
    agent_instance_id: String,
    new_status: AgentStatus,
    state: State<'_, AppState>,
) -> Result<(), String> {
    let proj_id = ProjectId::from_str(&project_id).map_err(|e| e.to_string())?;
    let id = AgentInstanceId::from_str(&agent_instance_id).map_err(|e| e.to_string())?;

    let db = state
        .get_project_db(&proj_id)
        .ok_or_else(|| "Project DB not found".to_string())?;

    let current_agent = db
        .with_read_conn(|conn| {
            trans4mers_storage::repos::agent_inst_repo::get_agent_instance(conn, &id)
        })?
        .ok_or_else(|| "Agent not found".to_string())?;

    let event = trans4mers_domain::event::DomainEvent::AgentStatusChanged {
        agent_instance_id: id,
        old_status: current_agent.status,
        new_status,
    };

    let envelope = db.with_write_tx(|conn| {
        trans4mers_engine::cqrs::commit_event(conn, event, state.human_actor_id)
    })?;

    let env_arc = std::sync::Arc::new(envelope);
    let bus = state.get_event_bus(&proj_id);
    let _ = bus.publish(env_arc.clone());
    let _ = state.global_event_bus.publish(env_arc);

    Ok(())
}

#[derive(serde::Serialize, serde::Deserialize, Clone, Debug)]
pub struct AgentDefinitionDto {
    pub id: String,
    pub name: String,
    pub role: String,
    pub description: String,
    pub default_model: String,
}

#[tauri::command]
pub async fn list_agent_definitions(
    state: State<'_, AppState>,
) -> Result<Vec<AgentDefinitionDto>, String> {
    state.global_db.with_read_conn(|conn| {
        let mut stmt = conn.prepare("SELECT id, name, role, description, default_model_config FROM agent_definitions ORDER BY name ASC")?;
        let rows = stmt.query_map([], |row| {
            let id: String = row.get(0)?;
            let name: String = row.get(1)?;
            let role: String = row.get(2)?;
            let description: String = row.get(3)?;
            let model_cfg: String = row.get(4)?;
            let model = serde_json::from_str::<serde_json::Value>(&model_cfg)
                .ok()
                .and_then(|v| v.get("model").and_then(|m| m.as_str()).map(|s| s.to_string()))
                .unwrap_or_else(|| "qwen2.5-coder:3b".to_string());
            Ok(AgentDefinitionDto { id, name, role, description, default_model: model })
        })?;
        let mut defs = Vec::new();
        for r in rows {
            defs.push(r?);
        }
        Ok(defs)
    }).map_err(|e| e.to_string())
}

#[tauri::command]
#[allow(clippy::too_many_arguments)]
pub async fn create_agent_definition(
    id: String,
    name: String,
    role: String,
    description: String,
    system_instructions: String,
    model: Option<String>,
    baseline_capabilities: Option<Vec<String>>,
    default_skills: Option<Vec<String>>,
    default_tools: Option<Vec<String>>,
    state: State<'_, AppState>,
) -> Result<(), String> {
    let clean_id = id.trim().to_lowercase().replace(' ', "_");
    let model_name = model.unwrap_or_else(|| "qwen2.5-coder:3b".to_string());
    let model_config = serde_json::json!({
        "provider": "ollama",
        "model": model_name,
        "temperature": 0.2
    })
    .to_string();

    let caps_str = baseline_capabilities
        .map(|c| serde_json::to_string(&c).unwrap_or_default())
        .unwrap_or_else(|| {
            "[\"FilesystemRead\", \"FilesystemWrite\", \"ShellExecute\", \"BrowserNavigate\"]"
                .to_string()
        });
    let skills_str = default_skills
        .map(|c| serde_json::to_string(&c).unwrap_or_default())
        .unwrap_or_else(|| "[]".to_string());
    let tools_str = default_tools
        .map(|c| serde_json::to_string(&c).unwrap_or_default())
        .unwrap_or_else(|| {
            "[\"filesystem.read\", \"filesystem.write\", \"shell.execute\", \"browser.fetch\"]"
                .to_string()
        });

    state.global_db.with_write_tx(|conn| {
        conn.execute(
            "INSERT INTO agent_definitions (id, name, role, description, system_instructions, default_model_config, baseline_capabilities, default_skills, default_tools, metadata)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, '{}')
             ON CONFLICT(id) DO UPDATE SET name=?2, role=?3, description=?4, system_instructions=?5, default_model_config=?6, baseline_capabilities=?7, default_skills=?8, default_tools=?9",
            rusqlite::params![clean_id, name, role, description, system_instructions, model_config, caps_str, skills_str, tools_str]
        )?;
        Ok(())
    }).map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn start_swarm_debate(
    project_id: String,
    conversation_id: String,
    topic: String,
    proponent_id: String,
    opponent_id: String,
    rounds: Option<usize>,
    state: State<'_, AppState>,
) -> Result<trans4mers_engine::swarm_orchestrator::SwarmResult, String> {
    let proj_id = ProjectId::from_str(&project_id).map_err(|e| e.to_string())?;
    let conv_id = ConversationId::from_str(&conversation_id)
        .unwrap_or_else(|_| ConversationId::from_uuid(*proj_id.as_uuid()));
    let prop_id = AgentInstanceId::from_str(&proponent_id).map_err(|e| e.to_string())?;
    let opp_id = AgentInstanceId::from_str(&opponent_id).map_err(|e| e.to_string())?;

    trans4mers_engine::SwarmOrchestrator::run_debate(
        state.inner().clone().into(),
        proj_id,
        conv_id,
        topic,
        prop_id,
        opp_id,
        rounds.unwrap_or(3),
    )
    .await
    .map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn start_supervisor_task(
    project_id: String,
    conversation_id: String,
    goal: String,
    supervisor_id: String,
    worker_ids: Vec<String>,
    state: State<'_, AppState>,
) -> Result<trans4mers_engine::swarm_orchestrator::SwarmResult, String> {
    let proj_id = ProjectId::from_str(&project_id).map_err(|e| e.to_string())?;
    let conv_id = ConversationId::from_str(&conversation_id)
        .unwrap_or_else(|_| ConversationId::from_uuid(*proj_id.as_uuid()));
    let sup_id = AgentInstanceId::from_str(&supervisor_id).map_err(|e| e.to_string())?;
    let mut workers = Vec::new();
    for w in worker_ids {
        workers.push(AgentInstanceId::from_str(&w).map_err(|e| e.to_string())?);
    }

    trans4mers_engine::SwarmOrchestrator::run_supervisor(
        state.inner().clone().into(),
        proj_id,
        conv_id,
        goal,
        sup_id,
        workers,
    )
    .await
    .map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn start_swarm_fanout(
    project_id: String,
    conversation_id: String,
    subtasks: Vec<String>,
    coordinator_id: String,
    worker_ids: Vec<String>,
    state: State<'_, AppState>,
) -> Result<trans4mers_engine::swarm_orchestrator::SwarmResult, String> {
    let proj_id = ProjectId::from_str(&project_id).map_err(|e| e.to_string())?;
    let conv_id = ConversationId::from_str(&conversation_id)
        .unwrap_or_else(|_| ConversationId::from_uuid(*proj_id.as_uuid()));
    let coord_id = AgentInstanceId::from_str(&coordinator_id).map_err(|e| e.to_string())?;
    let mut workers = Vec::new();
    for w in worker_ids {
        workers.push(AgentInstanceId::from_str(&w).map_err(|e| e.to_string())?);
    }

    trans4mers_engine::SwarmOrchestrator::run_fanout(
        state.inner().clone().into(),
        proj_id,
        conv_id,
        subtasks,
        coord_id,
        workers,
    )
    .await
    .map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn start_deep_research(
    project_id: String,
    conversation_id: String,
    topic: String,
    state: State<'_, AppState>,
) -> Result<trans4mers_engine::deep_research::DeepResearchReport, String> {
    let proj_id = ProjectId::from_str(&project_id).map_err(|e| e.to_string())?;
    let conv_id = ConversationId::from_str(&conversation_id)
        .unwrap_or_else(|_| ConversationId::from_uuid(*proj_id.as_uuid()));

    trans4mers_engine::DeepResearchEngine::conduct_research(
        state.inner().clone().into(),
        proj_id,
        conv_id,
        topic,
    )
    .await
    .map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn kill_agent_execution(
    project_id: String,
    execution_id: String,
    state: State<'_, AppState>,
) -> Result<(), String> {
    let proj_id = ProjectId::from_str(&project_id).map_err(|e| e.to_string())?;
    let exec_id = ExecutionId::from_str(&execution_id).map_err(|e| e.to_string())?;

    state.scheduler.cancel_execution(&exec_id, state.inner());

    if let Some(db) = state.get_project_db(&proj_id) {
        let _ = db.with_write_tx(|conn| {
            conn.execute(
                "UPDATE agent_executions SET status = 'Failed', updated_at = ?2 WHERE id = ?1",
                rusqlite::params![exec_id.as_str(), chrono::Utc::now().to_rfc3339()],
            )
            .map_err(Trans4mersError::from)?;
            Ok(())
        });
    }

    Ok(())
}

#[tauri::command]
pub async fn update_agent_definition(
    definition_id: String,
    system_instructions: String,
    name: Option<String>,
    role: Option<String>,
    description: Option<String>,
    state: State<'_, AppState>,
) -> Result<(), String> {
    let def_id = AgentDefinitionId::from_str(&definition_id).map_err(|e| e.to_string())?;

    state
        .global_db
        .with_write_tx(|conn| {
            let mut existing =
                trans4mers_storage::repos::agent_def_repo::get_agent_definition(conn, &def_id)?
                    .ok_or_else(|| {
                        Trans4mersError::Database(format!(
                            "Agent definition '{}' not found",
                            definition_id
                        ))
                    })?;

            existing.system_instructions = system_instructions;
            if let Some(n) = name {
                existing.name = n;
            }
            if let Some(r) = role {
                existing.role = r;
            }
            if let Some(d) = description {
                existing.description = d;
            }
            existing.updated_at = chrono::Utc::now();

            trans4mers_storage::repos::agent_def_repo::update_agent_definition(conn, &existing)
        })
        .map_err(|e| e.to_string())?;

    Ok(())
}

#[tauri::command]
pub async fn get_agent_definition(
    definition_id: String,
    state: State<'_, AppState>,
) -> Result<trans4mers_domain::agent::AgentDefinition, String> {
    let def_id = AgentDefinitionId::from_str(&definition_id).map_err(|e| e.to_string())?;
    let def = state
        .global_db
        .with_read_conn(|conn| {
            trans4mers_storage::repos::agent_def_repo::get_agent_definition(conn, &def_id)
        })
        .map_err(|e| e.to_string())?
        .ok_or_else(|| format!("Agent definition '{}' not found", definition_id))?;
    Ok(def)
}
