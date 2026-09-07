use crate::event_bus::EventBus;
use chrono::Utc;
use dashmap::DashMap;
use portable_pty::{CommandBuilder, PtySize, native_pty_system};
use std::io::{Read, Write};
use std::sync::Arc;
use tokio::sync::RwLock;
use trans4mers_domain::error::Trans4mersError;
use trans4mers_domain::event::{DomainEvent, EventEnvelope};
use trans4mers_domain::ids::{ActorId, EventId};

pub struct TerminalSession {
    pub pty_writer: std::sync::Mutex<Box<dyn Write + Send>>,
    pub master_pty: Arc<std::sync::Mutex<Box<dyn portable_pty::MasterPty + Send>>>,
}

pub struct TerminalManager {
    sessions: DashMap<String, Arc<RwLock<TerminalSession>>>,
}

impl Default for TerminalManager {
    fn default() -> Self {
        Self::new()
    }
}

impl TerminalManager {
    pub fn new() -> Self {
        Self {
            sessions: DashMap::new(),
        }
    }

    /// Physically spawns a local PTY shell process with requested dimensions.
    pub fn spawn_terminal(
        &self,
        terminal_id: String,
        workspace: &str,
        bus: Arc<EventBus>,
        cols: u16,
        rows: u16,
    ) -> Result<(), Trans4mersError> {
        let pty_system = native_pty_system();
        let pair = pty_system
            .openpty(PtySize {
                rows: if rows == 0 { 24 } else { rows },
                cols: if cols == 0 { 80 } else { cols },
                pixel_width: 0,
                pixel_height: 0,
            })
            .map_err(|e| Trans4mersError::Internal(e.to_string()))?;

        // Determine shell based on OS (fallback to bash/powershell)
        let shell = if cfg!(target_os = "windows") {
            "powershell.exe"
        } else {
            "bash"
        };

        let mut cmd = CommandBuilder::new(shell);
        cmd.cwd(workspace);

        let _child = pair
            .slave
            .spawn_command(cmd)
            .map_err(|e| Trans4mersError::Internal(e.to_string()))?;

        let writer = pair
            .master
            .take_writer()
            .map_err(|e| Trans4mersError::Internal(e.to_string()))?;
        let mut reader = pair
            .master
            .try_clone_reader()
            .map_err(|e| Trans4mersError::Internal(e.to_string()))?;

        let session = TerminalSession {
            pty_writer: std::sync::Mutex::new(writer),
            master_pty: Arc::new(std::sync::Mutex::new(pair.master)),
        };
        self.sessions
            .insert(terminal_id.clone(), Arc::new(RwLock::new(session)));

        // Background task to read PTY and broadcast to UI
        let term_id_clone = terminal_id.clone();
        std::thread::spawn(move || {
            let mut buf = [0u8; 1024];
            while let Ok(n) = reader.read(&mut buf) {
                if n == 0 {
                    break;
                }
                let output = String::from_utf8_lossy(&buf[..n]).to_string();

                let event = DomainEvent::TerminalOutput {
                    terminal_id: term_id_clone.clone(),
                    data: output,
                };

                let envelope = EventEnvelope {
                    sequence_id: 0,
                    event_id: EventId::new(),
                    event,
                    actor_id: ActorId::new(),
                    signature: None,
                    created_at: Utc::now(),
                };

                let _ = bus.publish(Arc::new(envelope));
            }
        });

        Ok(())
    }

    pub async fn write_to_terminal(
        &self,
        terminal_id: &str,
        data: &[u8],
    ) -> Result<(), Trans4mersError> {
        if let Some(session_ref) = self.sessions.get(terminal_id) {
            let session = session_ref.read().await;
            session
                .pty_writer
                .lock()
                .map_err(|e| Trans4mersError::Internal(e.to_string()))?
                .write_all(data)
                .map_err(|e| Trans4mersError::Internal(e.to_string()))?;
            Ok(())
        } else {
            Err(Trans4mersError::Internal("Terminal not found".to_string()))
        }
    }

    pub async fn resize_terminal(
        &self,
        terminal_id: &str,
        cols: u16,
        rows: u16,
    ) -> Result<(), Trans4mersError> {
        if let Some(session_ref) = self.sessions.get(terminal_id) {
            let session = session_ref.read().await;
            let master = session
                .master_pty
                .lock()
                .map_err(|_| Trans4mersError::Internal("Terminal PTY mutex poisoned".to_string()))?;
            master
                .resize(PtySize {
                    rows,
                    cols,
                    pixel_width: 0,
                    pixel_height: 0,
                })
                .map_err(|e| Trans4mersError::Internal(e.to_string()))?;
            Ok(())
        } else {
            Err(Trans4mersError::Internal("Terminal not found".to_string()))
        }
    }

    pub fn close_terminal(&self, terminal_id: &str) -> bool {
        self.sessions.remove(terminal_id).is_some()
    }

    pub fn active_sessions_count(&self) -> usize {
        self.sessions.len()
    }
}
