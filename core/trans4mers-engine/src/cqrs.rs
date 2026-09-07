use crate::event_projector::EventProjector;
use chrono::Utc;
use rusqlite::Transaction;
use trans4mers_domain::error::Trans4mersError;
use trans4mers_domain::event::{DomainEvent, EventEnvelope};
use trans4mers_domain::ids::{ActorId, EventId};
use trans4mers_storage::repos::event_repo::EventRepo;

/// Saves the event to the immutable log and projects it synchronously to relational tables.
/// DOES NOT broadcast. Broadcasting must happen AFTER the transaction commits.
pub fn commit_event(
    tx: &Transaction,
    event: DomainEvent,
    actor_id: ActorId,
) -> Result<EventEnvelope, Trans4mersError> {
    let mut envelope = EventEnvelope {
        sequence_id: 0, // Assigned by DB
        event_id: EventId::new(),
        event,
        actor_id,
        signature: None,
        created_at: Utc::now(),
    };

    // 1. Serialize and Append to Event Store (Immutable Log)
    let sequence_id = EventRepo::insert(tx, &envelope)?;
    envelope.sequence_id = sequence_id;

    // 2. Apply Projections to Relational Models (Synchronous within transaction)
    EventProjector::project_event(tx, &envelope)?;

    Ok(envelope)
}
