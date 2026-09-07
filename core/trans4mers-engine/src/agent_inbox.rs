use crate::cqrs::commit_event;
use rusqlite::Transaction;
use trans4mers_domain::error::Trans4mersError;
use trans4mers_domain::event::DomainEvent;
use trans4mers_domain::ids::{ActorId, AgentInstanceId, InboxMessageId};

pub struct AgentInbox;

impl AgentInbox {
    /// Queues a message for an agent.
    pub fn queue_message(
        tx: &Transaction,
        message_id: &InboxMessageId,
        recipient_agent_id: &AgentInstanceId,
        sender_actor_id: &str,
        payload: &serde_json::Value,
    ) -> Result<(), Trans4mersError> {
        let payload_str = serde_json::to_string(payload).unwrap();
        let event = DomainEvent::InboxMessageQueued {
            message_id: message_id.to_string(),
            recipient_agent_id: *recipient_agent_id,
            sender_actor_id: sender_actor_id.to_string(),
            payload: payload_str,
        };
        commit_event(tx, event, ActorId::new())?;
        Ok(())
    }

    /// Claims a message, transitioning it to CLAIMED. Only one worker can claim it.
    pub fn claim_next(
        tx: &Transaction,
        recipient_agent_id: &AgentInstanceId,
    ) -> Result<Option<(InboxMessageId, serde_json::Value)>, Trans4mersError> {
        // Find next queued message
        let row_opt = {
            let mut stmt = tx
                .prepare(
                    "SELECT id, payload FROM inbox_messages
                 WHERE recipient_agent_id = ?1 AND delivery_state = 'QUEUED'
                 ORDER BY created_at ASC LIMIT 1",
                )
                .map_err(|e| Trans4mersError::Database(e.to_string()))?;
            let mut rows = stmt
                .query(rusqlite::params![recipient_agent_id.as_str()])
                .map_err(|e| Trans4mersError::Database(e.to_string()))?;

            if let Some(row) = rows
                .next()
                .map_err(|e| Trans4mersError::Database(e.to_string()))?
            {
                let id_str: String = row
                    .get(0)
                    .map_err(|e| Trans4mersError::Database(e.to_string()))?;
                let payload_str: String = row
                    .get(1)
                    .map_err(|e| Trans4mersError::Database(e.to_string()))?;
                Some((id_str, payload_str))
            } else {
                None
            }
        };

        if let Some((id_str, payload_str)) = row_opt {
            let event = DomainEvent::InboxMessageClaimed {
                message_id: id_str.clone(),
                agent_id: *recipient_agent_id,
            };
            commit_event(tx, event, ActorId::new())?;

            let payload: serde_json::Value = serde_json::from_str(&payload_str).map_err(|e| {
                Trans4mersError::Internal(format!("Invalid inbox payload JSON: {}", e))
            })?;
            let id = std::str::FromStr::from_str(&id_str).map_err(|e| {
                Trans4mersError::Internal(format!("Invalid inbox message id: {}", e))
            })?;
            Ok(Some((id, payload)))
        } else {
            Ok(None)
        }
    }

    /// Acks a message, meaning it was fully processed and logically committed.
    pub fn ack_message(
        tx: &Transaction,
        message_id: &InboxMessageId,
        recipient_agent_id: &AgentInstanceId,
    ) -> Result<(), Trans4mersError> {
        let event = DomainEvent::InboxMessageAcked {
            message_id: message_id.to_string(),
            agent_id: *recipient_agent_id,
        };
        commit_event(tx, event, ActorId::new())?;
        Ok(())
    }
}
