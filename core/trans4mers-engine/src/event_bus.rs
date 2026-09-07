use std::sync::Arc;
use tokio::sync::broadcast;
use trans4mers_domain::error::Trans4mersError;
use trans4mers_domain::event::EventEnvelope;

pub struct EventBus {
    sender: broadcast::Sender<Arc<EventEnvelope>>,
}

impl EventBus {
    pub fn new(capacity: usize) -> Self {
        let (sender, _) = broadcast::channel(capacity);
        Self { sender }
    }

    /// Publishes an event to all active subscribers.
    /// Returns the number of receivers that received the message.
    pub fn publish(&self, event: Arc<EventEnvelope>) -> Result<(), Trans4mersError> {
        // Ignore the SendError if there are no active subscribers.
        // It's perfectly normal for events to fire in the background.
        let _ = self.sender.send(event);
        Ok(())
    }

    /// Subscribes to the event stream.
    pub fn subscribe(&self) -> broadcast::Receiver<Arc<EventEnvelope>> {
        self.sender.subscribe()
    }
}
