use crate::app_state::AppState;
use crate::git_workspace::GitWorkspace;
use async_trait::async_trait;
use chrono::Utc;
use std::sync::Arc;
use trans4mers_domain::actor::Actor;
use trans4mers_domain::agent::{AgentDefinition, AgentInstance};
use trans4mers_domain::config::ModelConfig;
use trans4mers_domain::error::Trans4mersError;
use trans4mers_domain::ids::{
    ActorId, AgentInstanceId, ChannelId, ConversationId, ExecutionId, MessageId,
};
use trans4mers_domain::message::{Message, MessageKind};
use trans4mers_domain::provider::LlmProvider;
use trans4mers_domain::state::AgentStatus;
use trans4mers_domain::tool::{
    EffectClass, RiskLevel, Tool, ToolManifest, ToolRequest, ToolResult, ToolSource,
};

pub struct DelegateTaskTool {
    manifest: ToolManifest,
    app_state: Arc<AppState>,
    _provider: Arc<dyn LlmProvider>,
}

impl DelegateTaskTool {
    pub fn new(app_state: Arc<AppState>, provider: Arc<dyn LlmProvider>) -> Self {
        Self {
            manifest: ToolManifest {
                name: "delegate_task".to_string(),
                description: "Spawns a sub-agent to perform a complex task.".to_string(),
                input_schema: serde_json::json!({
                    "type": "object",
                    "properties": {
                        "agent_definition_id": { "type": "string" },
                        "instructions": { "type": "string" }
                    },
                    "required": ["agent_definition_id", "instructions"]
                }),
                output_schema: None,
                effect_class: EffectClass::NonIdempotentMutation,
                baseline_risk: RiskLevel::Low,
                required_capabilities: vec![],
                source: ToolSource::Native,
                verification_command: None,
            },
            app_state,
            _provider: provider,
        }
    }
}

#[async_trait]
impl Tool for DelegateTaskTool {
    fn manifest(&self) -> &ToolManifest {
        &self.manifest
    }

    async fn execute(&self, request: &ToolRequest) -> Result<ToolResult, Trans4mersError> {
        let def_id_str = request
            .arguments
            .get("agent_definition_id")
            .and_then(|v| v.as_str())
            .ok_or_else(|| Trans4mersError::Internal("Missing agent_definition_id".to_string()))?;

        let instructions = request
            .arguments
            .get("instructions")
            .and_then(|v| v.as_str())
            .ok_or_else(|| Trans4mersError::Internal("Missing instructions".to_string()))?;

        // 1. Look up AgentDefinition from global DB by ID, name, or role
        let mut def_opt: Option<AgentDefinition> = None;
        if let Ok(found) = self.app_state.global_db.with_read_conn(|conn| {
            let mut stmt = conn.prepare(
                "SELECT id, name, role, description, system_instructions, default_model_config, baseline_capabilities, default_skills, default_tools, metadata, created_at, updated_at
                 FROM agent_definitions
                 WHERE id = ?1 OR lower(name) = lower(?1) OR lower(role) LIKE '%' || lower(?1) || '%'
                 LIMIT 1"
            )?;
            let row = stmt.query_row([def_id_str], |row| {
                let id: String = row.get(0)?;
                let name: String = row.get(1)?;
                let role: String = row.get(2)?;
                let desc: String = row.get(3)?;
                let sys: String = row.get(4)?;
                let model: String = row.get(5)?;
                let caps: String = row.get(6)?;
                let def_id = std::str::FromStr::from_str(&id)
                    .map_err(|e| rusqlite::Error::FromSqlConversionFailure(0, rusqlite::types::Type::Text, Box::new(e)))?;
                Ok(AgentDefinition {
                    id: def_id,
                    name,
                    role,
                    description: desc,
                    system_instructions: sys,
                    default_model_config: serde_json::from_str(&model).unwrap_or_default(),
                    baseline_capabilities: serde_json::from_str(&caps).unwrap_or_default(),
                    default_skills: vec![],
                    default_tools: vec![],
                    metadata: std::collections::HashMap::new(),
                    created_at: Utc::now(),
                    updated_at: Utc::now(),
                })
            });
            Ok(row.ok())
        }) {
            def_opt = found;
        }

        let definition = match def_opt {
            Some(d) => d,
            None => {
                // Dynamically instantiate a specialized definition for the requested role contract
                let role_name = if def_id_str.eq_ignore_ascii_case("swarm") {
                    "Software Engineer".to_string()
                } else {
                    def_id_str.replace('_', " ")
                };
                let agent_name = format!("{} Specialist", role_name);
                let sys_instructions = format!(
                    "You are an autonomous {}. Your objective is to carry out the following directive with high precision:\n{}",
                    role_name, instructions
                );
                let (default_provider, default_model) = {
                    let cfg = self.app_state.config.read().await;
                    let prov_entry = cfg.providers.iter().find(|p| p.enabled);
                    let prov = prov_entry
                        .map(|p| p.name.clone())
                        .unwrap_or_else(|| "ollama".to_string());
                    let modl = prov_entry
                        .and_then(|p| p.models.first().cloned())
                        .unwrap_or_else(|| "qwen2.5-coder:3b".to_string());
                    (prov, modl)
                };
                let model_config = ModelConfig {
                    provider: default_provider,
                    model: default_model,
                    temperature: Some(0.2),
                    max_output_tokens: Some(4096),
                    context_limit: None,
                    timeout_secs: None,
                    fallback_provider: None,
                    fallback_model: None,
                    provider_endpoint: None,
                };
                let capabilities = serde_json::json!([
                    "react_loop",
                    "fs_read",
                    "fs_write",
                    "sqlite_memory",
                    "tools_call"
                ]);
                let tools = serde_json::json!([
                    "filesystem.read",
                    "filesystem.write",
                    "filesystem.list",
                    "message.send"
                ]);

                let new_def_id = trans4mers_domain::ids::AgentDefinitionId::new();
                let created_def = AgentDefinition {
                    id: new_def_id.clone(),
                    name: agent_name.clone(),
                    role: role_name.clone(),
                    description: format!("Autonomous agent specialized in {}", role_name),
                    system_instructions: sys_instructions.clone(),
                    default_model_config: model_config.clone(),
                    baseline_capabilities: serde_json::from_value(capabilities.clone()).unwrap_or_default(),
                    default_skills: vec![],
                    default_tools: vec![],
                    metadata: std::collections::HashMap::new(),
                    created_at: Utc::now(),
                    updated_at: Utc::now(),
                };

                let _ = self.app_state.global_db.with_write_tx(|conn| {
                    conn.execute(
                        "INSERT OR IGNORE INTO agent_definitions (id, name, role, description, system_instructions, default_model_config, baseline_capabilities, default_skills, default_tools, metadata, created_at, updated_at)
                         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, '[]', ?8, '{}', CURRENT_TIMESTAMP, CURRENT_TIMESTAMP)",
                        rusqlite::params![
                            new_def_id.as_str(),
                            agent_name.as_str(),
                            role_name.as_str(),
                            format!("Autonomous agent specialized in {}", role_name),
                            sys_instructions.as_str(),
                            serde_json::to_string(&model_config).unwrap_or_default(),
                            serde_json::to_string(&capabilities).unwrap_or_default(),
                            serde_json::to_string(&tools).unwrap_or_default(),
                        ]
                    )?;
                    Ok(())
                });

                created_def
            }
        };

        // 2. Discover project_id (from request)
        let project_id = request.project_id;
        let db = self
            .app_state
            .get_project_db(&project_id)
            .ok_or_else(|| Trans4mersError::Database("Project DB not found".to_string()))?;

        let sub_agent_id = AgentInstanceId::new();

        // 3. Create Git Worktree
        let mut workspace_path = std::path::PathBuf::new();
        let _ = self.app_state.global_db.with_read_conn(|conn| {
            if let Ok(Some(proj)) =
                trans4mers_storage::repos::project_repo::get_project(conn, &project_id)
            {
                workspace_path = std::path::PathBuf::from(proj.workspace_path);
            }
            Ok(())
        });

        if workspace_path.as_os_str().is_empty() {
            return Err(Trans4mersError::Internal(
                "Could not resolve project workspace path".to_string(),
            ));
        }

        let branch_name = format!("agent/{}", sub_agent_id);
        GitWorkspace::create_agent_worktree(&workspace_path, &sub_agent_id, &branch_name)?;

        // Query parent depth & capabilities to enforce strict bounded inheritance
        let (parent_depth, parent_caps): (u32, Vec<trans4mers_domain::tool::Capability>) = db
            .with_read_conn(|conn| {
                if let Ok(row) = conn.query_row(
                    "SELECT depth_level, capabilities FROM agent_instances WHERE id = ?1",
                    rusqlite::params![request.requesting_agent_id.as_str()],
                    |row| {
                        let d: u32 = row.get(0)?;
                        let c_str: String = row.get(1)?;
                        let c: Vec<trans4mers_domain::tool::Capability> =
                            serde_json::from_str(&c_str).unwrap_or_default();
                        Ok((d, c))
                    },
                ) {
                    Ok(row)
                } else {
                    Ok((0, definition.baseline_capabilities.clone()))
                }
            })?;

        // Bounded Inheritance: if delegating parent has AgentSpawn, child gets definition baselines; otherwise subset
        let child_capabilities: Vec<trans4mers_domain::tool::Capability> =
            if parent_caps.contains(&trans4mers_domain::tool::Capability::AgentSpawn) {
                definition.baseline_capabilities
            } else {
                definition
                    .baseline_capabilities
                    .into_iter()
                    .filter(|cap| parent_caps.contains(cap))
                    .collect()
            };
        let child_depth = parent_depth + 1;

        // 4. Create the Domain object with real capabilities
        let agent = AgentInstance {
            id: sub_agent_id,
            project_id,
            definition_id: definition.id.clone(),
            parent_instance_id: Some(request.requesting_agent_id),
            status: AgentStatus::Active,
            capabilities: child_capabilities,
            model_config_override: None,
            depth_level: child_depth,
            created_at: Utc::now(),
            updated_at: Utc::now(),
        };

        // 5. Derive parent conversation ID so sub-agent is attached to the parent execution's conversation
        let parent_convo_id = db
            .with_read_conn(|conn| {
                let res = conn
                    .query_row(
                        "SELECT conversation_id FROM agent_executions WHERE id = ?1",
                        rusqlite::params![request.execution_id.as_str()],
                        |row| {
                            let s: String = row.get(0)?;
                            Ok(ConversationId::from_str(&s).unwrap_or_else(|_| {
                                ConversationId::from_uuid(*project_id.as_uuid())
                            }))
                        },
                    )
                    .unwrap_or_else(|_| ConversationId::from_uuid(*project_id.as_uuid()));
                Ok(res)
            })
            .unwrap_or_else(|_| ConversationId::from_uuid(*project_id.as_uuid()));

        let exec_id = ExecutionId::new();
        let convo_id = parent_convo_id;

        let spawn_event = trans4mers_domain::event::DomainEvent::AgentSpawned {
            agent_id: sub_agent_id,
            project_id,
            definition_id: definition.id.to_string(),
            parent_id: Some(request.requesting_agent_id.to_string()),
        };

        let sub_agent_msg = Message {
            id: MessageId::new(),
            conversation_id: convo_id,
            channel_id: ChannelId::from_uuid(*sub_agent_id.as_uuid()),
            sender: Actor::Agent(request.requesting_agent_id),
            content: instructions.to_string(),
            mentions: vec![trans4mers_domain::message::Mention {
                agent_instance_id: sub_agent_id,
                display_name: definition.name.clone(),
                start_offset: 0,
                end_offset: 0,
            }],
            message_kind: MessageKind::Chat,
            thread_id: None,
            attachments: vec![],
            requires_approval: false,
            approval_id: None,
            created_at: Utc::now(),
            updated_at: Utc::now(),
        };

        let inbox_msg_id = MessageId::new().to_string();
        let inbox_event = trans4mers_domain::event::DomainEvent::InboxMessageQueued {
            message_id: inbox_msg_id,
            recipient_agent_id: sub_agent_id,
            sender_actor_id: format!("agent:{}", request.requesting_agent_id),
            payload: serde_json::to_string(&sub_agent_msg).unwrap_or_default(),
        };

        let (spawn_envelope, inbox_envelope) = db.with_write_tx(|tx| {
            tx.execute(
                "INSERT INTO agent_instances (id, project_id, definition_id, parent_instance_id, status, capabilities, depth_level, created_at, updated_at)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)
                 ON CONFLICT(id) DO UPDATE SET
                    capabilities = excluded.capabilities,
                    depth_level = excluded.depth_level,
                    status = excluded.status,
                    updated_at = excluded.updated_at",
                rusqlite::params![
                    agent.id.as_str(),
                    agent.project_id.as_str(),
                    agent.definition_id.as_str(),
                    agent.parent_instance_id.as_ref().map(|id| id.as_str()),
                    "Active",
                    serde_json::to_string(&agent.capabilities).unwrap_or_else(|_| "[]".to_string()),
                    agent.depth_level,
                    agent.created_at.to_rfc3339(),
                    agent.updated_at.to_rfc3339()
                ]
            )?;
            tx.execute(
                "INSERT INTO agent_executions (id, agent_instance_id, conversation_id, status, generation, current_step, max_steps, updated_at)
                 VALUES (?1, ?2, ?3, 'Pending', 0, 0, 25, ?4)",
                rusqlite::params![exec_id.as_str(), agent.id.as_str(), convo_id.as_str(), Utc::now().to_rfc3339()]
            )?;
            let env_spawn = crate::cqrs::commit_event(tx, spawn_event, ActorId::new())?;
            let env_inbox = crate::cqrs::commit_event(tx, inbox_event, ActorId::new())?;
            Ok((env_spawn, env_inbox))
        })?;

        // Broadcast both events to project and global buses so desktop UI updates in real-time
        let bus = self.app_state.get_event_bus(&project_id);
        let env_spawn_arc = Arc::new(spawn_envelope);
        let _ = bus.publish(env_spawn_arc.clone());
        let _ = self.app_state.global_event_bus.publish(env_spawn_arc);

        let env_inbox_arc = Arc::new(inbox_envelope);
        let _ = bus.publish(env_inbox_arc.clone());
        let _ = self.app_state.global_event_bus.publish(env_inbox_arc);

        // 6. Queue the execution via the true orchestrator (Fire and forget!)
        self.app_state.scheduler.queue(exec_id, project_id);

        // We do NOT block. The sub-agent runs independently.
        let msg = format!(
            "Sub-agent '{}' ({}) successfully spawned with role '{}' in worktree '{}'. The specialist agent is active and processing instructions.",
            definition.name, sub_agent_id, definition.role, branch_name
        );

        Ok(ToolResult {
            success: true,
            content: msg,
            error: None,
            artifacts: vec![],
            metadata: std::collections::HashMap::new(),
        })
    }
}
