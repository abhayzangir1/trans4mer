use chrono::{DateTime, Utc};
use std::str::FromStr;
use tauri::State;
use trans4mers_domain::ids::{AgentInstanceId, ConversationId, MemoryId, ProjectId};
use trans4mers_domain::memory::{
    Memory, MemoryLifecycle, MemoryProvenance, MemoryScope, MemoryTier,
};
use trans4mers_engine::app_state::AppState;
use trans4mers_storage::repos::memory_repo;

#[tauri::command]
pub async fn get_memories(
    project_id: String,
    scope: Option<String>,
    tier: Option<String>,
    agent_id: Option<String>,
    limit: Option<u32>,
    state: State<'_, AppState>,
) -> Result<Vec<Memory>, String> {
    let proj_id = ProjectId::from_str(&project_id).map_err(|e| e.to_string())?;
    let db = state
        .get_project_db(&proj_id)
        .ok_or_else(|| "Project DB not found".to_string())?;

    let memories: Vec<Memory> = db.with_read_conn(|conn| {
        let mut query = "SELECT id, project_id, conversation_id, agent_instance_id, scope, lifecycle,
                                content, importance, confidence, provenance, depth_level, tier,
                                retrieval_count, last_retrieved_at, expires_at, visibility_overrides,
                                created_at, updated_at, valid_at, invalid_at, replaced_by, content_hash
                         FROM project_memories
                         WHERE project_id = ?1 AND (invalid_at IS NULL OR invalid_at > datetime('now'))".to_string();

        let mut params: Vec<rusqlite::types::Value> = vec![rusqlite::types::Value::Text(project_id.clone())];
        let mut p_idx = 2;

        if let Some(s) = scope {
            query.push_str(&format!(" AND scope = ?{}", p_idx));
            params.push(rusqlite::types::Value::Text(s));
            p_idx += 1;
        }

        if let Some(t) = tier {
            query.push_str(&format!(" AND tier = ?{}", p_idx));
            params.push(rusqlite::types::Value::Text(t));
            p_idx += 1;
        }

        if let Some(a_id) = agent_id {
            query.push_str(&format!(" AND agent_instance_id = ?{}", p_idx));
            params.push(rusqlite::types::Value::Text(a_id));
            p_idx += 1;
        }

        query.push_str(&format!(" ORDER BY created_at DESC LIMIT ?{}", p_idx));
        params.push(rusqlite::types::Value::Integer(limit.unwrap_or(50) as i64));

        let mut stmt = conn.prepare(&query)?;
        let rows = stmt.query_map(rusqlite::params_from_iter(params), |row| {
            let id_str: String = row.get(0)?;
            let proj_str: String = row.get(1)?;
            let conv_str: Option<String> = row.get(2)?;
            let agent_str: Option<String> = row.get(3)?;
            let scope_str: String = row.get(4)?;
            let lc_str: String = row.get(5)?;
            let content: String = row.get(6)?;
            let importance: f32 = row.get(7)?;
            let confidence: f32 = row.get(8)?;
            let prov_str: String = row.get(9)?;
            let depth_level: u32 = row.get(10)?;
            let tier_str: String = row.get(11)?;
            let retrieval_count: u32 = row.get(12)?;
            let last_retrieved_str: Option<String> = row.get(13)?;
            let exp_str: Option<String> = row.get(14)?;
            let vis_str: Option<String> = row.get(15)?;
            let created_str: String = row.get(16)?;
            let updated_str: String = row.get(17)?;
            let valid_at_str: Option<String> = row.get(18).ok().flatten();
            let invalid_at_str: Option<String> = row.get(19).ok().flatten();
            let replaced_by: Option<String> = row.get(20).ok().flatten();
            let content_hash: Option<String> = row.get(21).ok().flatten();

            Ok(Memory {
                id: MemoryId::from_str(&id_str).unwrap_or_default(),
                project_id: ProjectId::from_str(&proj_str).unwrap_or_default(),
                conversation_id: conv_str.and_then(|s| ConversationId::from_str(&s).ok()),
                agent_instance_id: agent_str.and_then(|s| AgentInstanceId::from_str(&s).ok()),
                scope: MemoryScope::from_str(&scope_str).unwrap_or(MemoryScope::Project),
                lifecycle: MemoryLifecycle::from_str(&lc_str).unwrap_or(MemoryLifecycle::Observation),
                content,
                embedding: None,
                importance,
                confidence,
                provenance: serde_json::from_str(&prov_str).unwrap_or_else(|_| MemoryProvenance {
                    source_event_id: None,
                    source_message_id: None,
                    source_agent_id: None,
                    creation_reason: "Unknown".to_string(),
                }),
                depth_level,
                tier: MemoryTier::from_str(&tier_str).unwrap_or(MemoryTier::Working),
                retrieval_count,
                last_retrieved_at: last_retrieved_str.and_then(|s| DateTime::parse_from_rfc3339(&s).ok().map(|d| d.with_timezone(&Utc))),
                expires_at: exp_str.and_then(|s| DateTime::parse_from_rfc3339(&s).ok().map(|d| d.with_timezone(&Utc))),
                valid_from: None,
                valid_until: None,
                visibility_overrides: vis_str.and_then(|s| serde_json::from_str(&s).ok()),
                created_at: DateTime::parse_from_rfc3339(&created_str).map(|d| d.with_timezone(&Utc)).unwrap_or_else(|_| Utc::now()),
                updated_at: DateTime::parse_from_rfc3339(&updated_str).map(|d| d.with_timezone(&Utc)).unwrap_or_else(|_| Utc::now()),
                valid_at: valid_at_str.and_then(|s| DateTime::parse_from_rfc3339(&s).ok().map(|d| d.with_timezone(&Utc))),
                invalid_at: invalid_at_str.and_then(|s| DateTime::parse_from_rfc3339(&s).ok().map(|d| d.with_timezone(&Utc))),
                replaced_by,
                content_hash,
            })
        })?;

        let mut results = Vec::new();
        for r in rows {
            results.push(r?);
        }
        Ok(results)
    }).map_err(|e| e.to_string())?;

    Ok(memories)
}

#[tauri::command]
pub async fn search_memories(
    project_id: String,
    agent_id: Option<String>,
    query: Option<String>,
    limit: Option<u32>,
    state: State<'_, AppState>,
) -> Result<Vec<Memory>, String> {
    let proj_id = ProjectId::from_str(&project_id).map_err(|e| e.to_string())?;
    let ag_id = agent_id
        .and_then(|a| AgentInstanceId::from_str(&a).ok())
        .unwrap_or_default();
    let memory_engine =
        trans4mers_engine::MemoryEngine::new(std::sync::Arc::new(state.inner().clone()));

    let embedding = if let Some(ref q) = query {
        if !q.trim().is_empty() {
            if let Ok(provider) = state.provider_registry.get_default() {
                let model_cfg = trans4mers_domain::config::ModelConfig::default();
                provider.embed(q, &model_cfg).await.ok()
            } else {
                None
            }
        } else {
            None
        }
    } else {
        None
    };

    let memories = memory_engine
        .retrieve_memories(
            &proj_id,
            &ag_id,
            None,
            query.as_deref(),
            embedding.as_deref(),
            limit.unwrap_or(20),
        )
        .map_err(|e| e.to_string())?;
    Ok(memories)
}

#[tauri::command]
pub async fn update_memory_tier(
    project_id: String,
    memory_id: String,
    new_tier: String,
    state: State<'_, AppState>,
) -> Result<(), String> {
    let proj_id = ProjectId::from_str(&project_id).map_err(|e| e.to_string())?;
    let tier = MemoryTier::from_str(&new_tier).map_err(|e| e.to_string())?;
    let db = state
        .get_project_db(&proj_id)
        .ok_or_else(|| "Project DB not found".to_string())?;

    db.with_write_tx(|conn| memory_repo::update_tier(conn, &memory_id, tier))
        .map_err(|e| e.to_string())?;

    // Publish CQRS event
    let event = trans4mers_domain::event::DomainEvent::MemoryTierPromoted {
        project_id: proj_id,
        memory_id,
        old_tier: "manual_edit".to_string(),
        new_tier,
    };
    let envelope = trans4mers_domain::event::EventEnvelope {
        sequence_id: 0,
        event_id: trans4mers_domain::ids::EventId::new(),
        event,
        actor_id: state.human_actor_id,
        signature: None,
        created_at: Utc::now(),
    };
    let env_arc = std::sync::Arc::new(envelope);
    let _ = state.get_event_bus(&proj_id).publish(env_arc.clone());
    let _ = state.global_event_bus.publish(env_arc);

    Ok(())
}

#[tauri::command]
pub async fn delete_memory(
    project_id: String,
    memory_id: String,
    state: State<'_, AppState>,
) -> Result<(), String> {
    let proj_id = ProjectId::from_str(&project_id).map_err(|e| e.to_string())?;
    let db = state
        .get_project_db(&proj_id)
        .ok_or_else(|| "Project DB not found".to_string())?;

    db.with_write_tx(|conn| memory_repo::delete_memory(conn, &memory_id))
        .map_err(|e| e.to_string())
}

#[derive(serde::Serialize)]
pub struct MemoryTierStat {
    pub tier: String,
    pub count: usize,
    pub avg_importance: f32,
}

#[tauri::command]
pub async fn get_memory_pyramid_stats(
    project_id: String,
    state: State<'_, AppState>,
) -> Result<Vec<MemoryTierStat>, String> {
    let proj_id = ProjectId::from_str(&project_id).map_err(|e| e.to_string())?;
    let db = state
        .get_project_db(&proj_id)
        .ok_or_else(|| "Project DB not found".to_string())?;

    db.with_read_conn(|conn| {
        let stats = memory_repo::count_by_tier(conn, &project_id)?;
        Ok(stats
            .into_iter()
            .map(|(tier, count, avg_importance)| MemoryTierStat {
                tier,
                count,
                avg_importance,
            })
            .collect())
    })
    .map_err(|e| e.to_string())
}
