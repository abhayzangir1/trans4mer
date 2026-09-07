use rusqlite::Transaction;
use trans4mers_domain::error::Trans4mersError;
use trans4mers_domain::event::{DomainEvent, EventEnvelope};
use trans4mers_domain::ids::{AgentInstanceId, ConversationId, ExecutionId};
use trans4mers_domain::tool::{Capability, RiskLevel};

pub struct ApprovalEngine;

impl ApprovalEngine {
    /// Canonical JSON representation with sorted keys to ensure deterministic hashing
    pub fn canonical_arguments_json(v: &serde_json::Value) -> String {
        match v {
            serde_json::Value::Object(map) => {
                let mut keys: Vec<&String> = map.keys().collect();
                keys.sort();
                let mut out = String::from("{");
                for (i, k) in keys.iter().enumerate() {
                    if i > 0 {
                        out.push(',');
                    }
                    out.push_str(&serde_json::to_string(k).unwrap_or_default());
                    out.push(':');
                    out.push_str(&Self::canonical_arguments_json(&map[*k]));
                }
                out.push('}');
                out
            }
            serde_json::Value::Array(arr) => {
                let mut out = String::from("[");
                for (i, elem) in arr.iter().enumerate() {
                    if i > 0 {
                        out.push(',');
                    }
                    out.push_str(&Self::canonical_arguments_json(elem));
                }
                out.push(']');
                out
            }
            _ => serde_json::to_string(v).unwrap_or_default(),
        }
    }

    /// SHA-256 of canonical JSON arguments for argument-scoped security
    pub fn hash_arguments(v: &serde_json::Value) -> String {
        trans4mers_storage::filesystem::sha256_hex(Self::canonical_arguments_json(v).as_bytes())
    }

    /// Checks if this exact (execution, tool, arguments) was already approved
    pub fn has_approved(
        conn: &rusqlite::Connection,
        execution_id: &ExecutionId,
        tool_name: &str,
        arguments: &serde_json::Value,
    ) -> Result<bool, Trans4mersError> {
        let hash = Self::hash_arguments(arguments);
        trans4mers_storage::repos::approval_repo::has_approved(conn, execution_id, tool_name, &hash)
    }

    /// Checks if this exact (execution, tool, arguments) was rejected
    pub fn was_rejected(
        conn: &rusqlite::Connection,
        execution_id: &ExecutionId,
        tool_name: &str,
        arguments: &serde_json::Value,
    ) -> Result<bool, Trans4mersError> {
        let hash = Self::hash_arguments(arguments);
        trans4mers_storage::repos::approval_repo::was_rejected(conn, execution_id, tool_name, &hash)
    }

    /// Creates a new Pending Approval and commits the event to the log.
    #[allow(clippy::too_many_arguments)]
    pub fn request_approval(
        tx: &Transaction,
        approval_id: String,
        execution_id: ExecutionId,
        agent_id: AgentInstanceId,
        conversation_id: ConversationId,
        capability: Capability,
        tool_name: String,
        payload: String,
        arguments_hash: Option<String>,
        risk_level: RiskLevel,
    ) -> Result<EventEnvelope, Trans4mersError> {
        let event = DomainEvent::PendingApproval {
            approval_id: approval_id.to_string(),
            execution_id,
            conversation_id,
            agent_id,
            capability: capability.to_string(),
            tool_name,
            payload,
            risk_level: risk_level.to_string(),
            arguments_hash,
        };

        crate::cqrs::commit_event(tx, event, trans4mers_domain::ids::ActorId::new())
    }

    /// Resolves an existing approval (Approved or Denied) and commits the event.
    pub fn resolve_approval(
        tx: &Transaction,
        approval_id: String,
        approved: bool,
        feedback: Option<String>,
    ) -> Result<EventEnvelope, Trans4mersError> {
        let event = DomainEvent::ApprovalResolved {
            approval_id,
            approved,
            feedback,
        };

        crate::cqrs::commit_event(tx, event, trans4mers_domain::ids::ActorId::new())
    }
}
