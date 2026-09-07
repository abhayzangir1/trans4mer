use chrono::Utc;
use std::sync::Arc;
use tauri::State;
use trans4mers_domain::artifact::Artifact;
use trans4mers_domain::event::DomainEvent;
use trans4mers_domain::ids::{AgentInstanceId, ExecutionId, ProjectId};
use trans4mers_engine::app_state::AppState;
use trans4mers_storage::repos::artifact_comment_repo::{self, ArtifactComment};
use uuid::Uuid;

#[tauri::command]
pub async fn get_artifacts(
    project_id: String,
    state: State<'_, AppState>,
) -> Result<Vec<Artifact>, String> {
    let proj_id = ProjectId::from_str(&project_id).map_err(|e| e.to_string())?;
    let db = state
        .get_project_db(&proj_id)
        .ok_or_else(|| "Project DB not found".to_string())?;
    let arts = db
        .with_read_conn(|conn| {
            trans4mers_storage::repos::artifact_repo::list_artifacts(conn, &project_id)
        })
        .map_err(|e| e.to_string())?;
    Ok(arts)
}

#[tauri::command]
pub async fn list_artifact_comments(
    project_id: String,
    artifact_id: String,
    state: State<'_, AppState>,
) -> Result<Vec<ArtifactComment>, String> {
    let proj_id = ProjectId::from_str(&project_id).map_err(|e| e.to_string())?;
    let db = state
        .get_project_db(&proj_id)
        .ok_or_else(|| "Project DB not found".to_string())?;
    let comments = db
        .with_read_conn(|conn| {
            artifact_comment_repo::list_comments_for_artifact(conn, &artifact_id)
        })
        .map_err(|e| e.to_string())?;
    Ok(comments)
}

#[tauri::command]
#[allow(clippy::too_many_arguments)]
pub async fn add_artifact_comment(
    project_id: String,
    artifact_id: String,
    line_start: Option<i64>,
    line_end: Option<i64>,
    selected_text: Option<String>,
    comment: String,
    user_id: Option<String>,
    state: State<'_, AppState>,
) -> Result<ArtifactComment, String> {
    let proj_id = ProjectId::from_str(&project_id).map_err(|e| e.to_string())?;
    let db = state
        .get_project_db(&proj_id)
        .ok_or_else(|| "Project DB not found".to_string())?;

    let comment_id = Uuid::new_v4().to_string();
    let author = user_id.unwrap_or_else(|| "Human Developer".to_string());
    let now = Utc::now();

    let record = ArtifactComment {
        id: comment_id.clone(),
        artifact_id: artifact_id.clone(),
        user_id: author.clone(),
        line_start,
        line_end,
        selected_text: selected_text.clone(),
        comment: comment.clone(),
        status: "open".to_string(),
        created_at: now,
    };

    db.with_write_tx(|conn| artifact_comment_repo::create_comment(conn, &record))
        .map_err(|e| e.to_string())?;

    // Broadcast domain event
    let event = DomainEvent::ArtifactCommentAdded {
        project_id: proj_id,
        artifact_id,
        comment_id,
        author,
        content: comment,
    };

    let envelope = db
        .with_write_tx(|conn| {
            trans4mers_engine::cqrs::commit_event(conn, event, state.human_actor_id)
        })
        .map_err(|e| e.to_string())?;

    let env_arc = Arc::new(envelope);
    let bus = state.get_event_bus(&proj_id);
    let _ = bus.publish(env_arc.clone());
    let _ = state.global_event_bus.publish(env_arc);

    Ok(record)
}

#[tauri::command]
pub async fn send_artifact_comment_to_agent(
    project_id: String,
    artifact_id: String,
    comment_id: String,
    target_agent_id: Option<String>,
    state: State<'_, AppState>,
) -> Result<(), String> {
    let proj_id = ProjectId::from_str(&project_id).map_err(|e| e.to_string())?;
    let db = state
        .get_project_db(&proj_id)
        .ok_or_else(|| "Project DB not found".to_string())?;

    // 1. Fetch comments and find the target
    let comments = db
        .with_read_conn(|conn| {
            artifact_comment_repo::list_comments_for_artifact(conn, &artifact_id)
        })
        .map_err(|e| e.to_string())?;

    let target_comment = comments
        .into_iter()
        .find(|c| c.id == comment_id)
        .ok_or_else(|| format!("Comment '{}' not found", comment_id))?;

    // 2. Resolve target agent instance
    let agent_id = if let Some(ref aid_str) = target_agent_id {
        AgentInstanceId::from_str(aid_str).map_err(|e| e.to_string())?
    } else {
        // Fallback to active agent in project
        let id_str: String = db.with_read_conn(|conn| {
            conn.query_row(
                "SELECT id FROM agent_instances WHERE status != 'TERMINATED' ORDER BY created_at ASC LIMIT 1",
                [],
                |row| row.get(0)
            ).map_err(|e| trans4mers_domain::error::Trans4mersError::Database(e.to_string()))
        }).map_err(|_| "No active agent found to receive comment feedback.".to_string())?;
        AgentInstanceId::from_str(&id_str).map_err(|e| e.to_string())?
    };

    // 3. Format message prompt
    let prompt_text = format!(
        "[Interactive Artifact Review Feedback]\n\
         Artifact ID: {}\n\
         Comment from {}:\n\
         Line Range: {:?} - {:?}\n\
         Code context:\n```\n{}\n```\n\
         Reviewer Comment:\n{}\n\n\
         Please inspect the code and implement or address this feedback directly.",
        target_comment.artifact_id,
        target_comment.user_id,
        target_comment.line_start,
        target_comment.line_end,
        target_comment
            .selected_text
            .as_deref()
            .unwrap_or("<No specific snippet selected>"),
        target_comment.comment
    );

    // 4. Resolve or create execution
    let mut exec_id_opt: Option<ExecutionId> = None;
    let _ = db.with_read_conn(|conn| {
        if let Ok(exec_id_str) = conn.query_row(
            "SELECT id FROM agent_executions WHERE agent_instance_id = ?1 AND status IN ('Pending', 'Running', 'Checkpointing', 'WaitingForApproval', 'WaitingForMessage')",
            rusqlite::params![agent_id.as_str()],
            |row| row.get::<_, String>(0)
        ) {
            exec_id_opt = ExecutionId::from_str(&exec_id_str).ok();
        }
        Ok(())
    });

    let exec_id = if let Some(id) = exec_id_opt {
        id
    } else {
        let new_id = ExecutionId::new();
        let _ = db.with_write_tx(|conn| {
            conn.execute(
                &format!("INSERT INTO agent_executions (id, agent_instance_id, conversation_id, status, generation, current_step, max_steps, updated_at) VALUES (?1, ?2, '{}', 'Pending', 0, 0, 100, ?3)", trans4mers_domain::constants::DEFAULT_CHANNEL_NAME),
                rusqlite::params![new_id.as_str(), agent_id.as_str(), Utc::now().to_rfc3339()]
            )?;
            Ok(())
        });
        new_id
    };

    // 5. Enqueue into target agent's durable inbox
    let inbox_event = DomainEvent::InboxMessageQueued {
        message_id: Uuid::new_v4().to_string(),
        recipient_agent_id: agent_id,
        sender_actor_id: "user".to_string(),
        payload: prompt_text,
    };

    let envelope = db
        .with_write_tx(|conn| {
            trans4mers_engine::cqrs::commit_event(conn, inbox_event, state.human_actor_id)
        })
        .map_err(|e| e.to_string())?;

    let env_arc = Arc::new(envelope);
    let bus = state.get_event_bus(&proj_id);
    let _ = bus.publish(env_arc.clone());
    let _ = state.global_event_bus.publish(env_arc);

    // 6. Wake up scheduler
    state.scheduler.queue(exec_id, proj_id);

    // 7. Update comment status to sent
    let _ = db.with_write_tx(|conn| {
        artifact_comment_repo::update_comment_status(conn, &comment_id, "dispatched_to_agent")
    });

    Ok(())
}
