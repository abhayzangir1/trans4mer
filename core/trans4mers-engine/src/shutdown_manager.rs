use std::sync::atomic::{AtomicBool, Ordering};
use tokio::sync::broadcast;
use tracing::info;

pub struct ShutdownManager {
    shutdown_tx: broadcast::Sender<()>,
    is_shutting_down: AtomicBool,
}

impl Default for ShutdownManager {
    fn default() -> Self {
        Self::new()
    }
}

impl ShutdownManager {
    pub fn new() -> Self {
        let (shutdown_tx, _) = broadcast::channel(1);
        Self {
            shutdown_tx,
            is_shutting_down: AtomicBool::new(false),
        }
    }

    /// Subscribes a worker thread to the shutdown signal.
    pub fn subscribe(&self) -> broadcast::Receiver<()> {
        self.shutdown_tx.subscribe()
    }

    /// Checks if a shutdown has been requested.
    pub fn is_shutting_down(&self) -> bool {
        self.is_shutting_down.load(Ordering::SeqCst)
    }

    /// Triggers a graceful shutdown across the entire system.
    pub fn trigger_shutdown(&self) {
        info!("Initiating graceful system shutdown...");
        self.is_shutting_down.store(true, Ordering::SeqCst);
        let _ = self.shutdown_tx.send(());
    }
}
