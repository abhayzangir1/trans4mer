use rusqlite::{Connection, Transaction, params};
use trans4mers_domain::error::Trans4mersError;
use trans4mers_domain::event::EventEnvelope;

pub struct EventRepo;

impl EventRepo {
    /// Appends a new event to the immutable append-only event log.
    pub fn insert(tx: &Transaction, envelope: &EventEnvelope) -> Result<i64, Trans4mersError> {
        let payload = serde_json::to_string(&envelope.event)
            .map_err(|e| Trans4mersError::Internal(format!("Failed to serialize event: {}", e)))?;

        let event_val = serde_json::to_value(&envelope.event)
            .map_err(|e| Trans4mersError::Internal(format!("Failed to serialize event value: {}", e)))?;
        let event_type = event_val
            .get("type")
            .and_then(|v| v.as_str())
            .unwrap_or("Unknown")
            .to_string();

        tx.execute(
            "INSERT INTO domain_events (event_id, event_type, payload, actor_id, signature, created_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
            params![
                envelope.event_id.as_str(),
                event_type,
                payload,
                envelope.actor_id.as_str(),
                envelope.signature,
                envelope.created_at.to_rfc3339()
            ],
        ).map_err(|e| Trans4mersError::Database(format!("Failed to append event: {}", e)))?;

        Ok(tx.last_insert_rowid())
    }

    pub fn get_events_since(
        conn: &Connection,
        sequence_id: i64,
    ) -> Result<Vec<EventEnvelope>, Trans4mersError> {
        let mut stmt = conn
            .prepare(
                "SELECT sequence_id, event_id, payload, actor_id, signature, created_at
             FROM domain_events
             WHERE sequence_id > ?1
             ORDER BY sequence_id ASC",
            )
            .map_err(|e| Trans4mersError::Database(e.to_string()))?;

        let rows = stmt
            .query_map(params![sequence_id], |row| {
                let seq: i64 = row.get(0)?;
                let id: String = row.get(1)?;
                let payload: String = row.get(2)?;
                let actor_id: String = row.get(3)?;
                let signature: Option<String> = row.get(4)?;
                let created_at: String = row.get(5)?;

                let event = serde_json::from_str(&payload).map_err(|_| {
                    rusqlite::Error::InvalidColumnType(
                        2,
                        "Invalid JSON".to_string(),
                        rusqlite::types::Type::Text,
                    )
                })?;

                Ok(EventEnvelope {
                    sequence_id: seq,
                    event_id: std::str::FromStr::from_str(&id).unwrap_or_default(),
                    event,
                    actor_id: std::str::FromStr::from_str(&actor_id).unwrap_or_default(),
                    signature,
                    created_at: crate::datetime_util::parse_db_datetime(&created_at),
                })
            })
            .map_err(|e| Trans4mersError::Database(e.to_string()))?;

        let mut envelopes = Vec::new();
        for row in rows {
            envelopes.push(row.map_err(|e| Trans4mersError::Database(e.to_string()))?);
        }

        Ok(envelopes)
    }
}
