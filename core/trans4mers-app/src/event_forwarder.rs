use std::sync::Arc;
use tauri::{AppHandle, Emitter};
use tracing::info;
use trans4mers_engine::EventBus;

pub struct EventForwarder;

impl EventForwarder {
    /// Spawns a background task that listens to the Rust EventBus
    /// and forwards serialized events to the Tauri WebView frontend.
    pub fn spawn_forwarder(app_handle: AppHandle, event_bus: Arc<EventBus>) {
        let mut rx = event_bus.subscribe();

        tokio::spawn(async move {
            info!("EventForwarder started: bridging Backend to Frontend.");
            while let Ok(envelope) = rx.recv().await {
                // Serialize the entire EventEnvelope, retaining the EventId and Timestamp
                let payload = match serde_json::to_string(&*envelope) {
                    Ok(p) => p,
                    Err(e) => {
                        tracing::error!(
                            "Failed to serialize DomainEvent envelope for Tauri forwarder: {}",
                            e
                        );
                        continue;
                    }
                };

                // Emit to all Tauri windows using the strict `domain_event` channel
                if let Err(e) = app_handle.emit("domain_event", payload) {
                    tracing::error!("Failed to forward event to Tauri: {}", e);
                }
            }
        });
    }
}
