use serde::{Deserialize, Serialize};
use std::sync::Arc;
use tokio::sync::Mutex;
use tracing::info;
use trans4mers_domain::error::Trans4mersError;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NodeEnvironment {
    pub available: bool,
    pub node_path: Option<String>,
    pub node_version: Option<String>,
    pub npx_path: Option<String>,
    pub guidance: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct McpInspectorLaunchResult {
    pub success: bool,
    pub pid: Option<u32>,
    pub port: u16,
    pub message: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct McpInspectorStatus {
    pub running: bool,
    pub pid: Option<u32>,
    pub server_name: Option<String>,
    pub port: Option<u16>,
}

/// Probes the system for Node.js and npx without throwing errors if absent.
pub fn detect_node_environment() -> NodeEnvironment {
    let node_bin = which::which("node");
    let npx_bin = which::which("npx");

    match (node_bin, npx_bin) {
        (Ok(node_p), Ok(npx_p)) => {
            let version_output = std::process::Command::new(&node_p)
                .arg("--version")
                .output()
                .ok()
                .and_then(|o| String::from_utf8(o.stdout).ok())
                .map(|s| s.trim().to_string());

            NodeEnvironment {
                available: true,
                node_path: Some(node_p.to_string_lossy().to_string()),
                node_version: version_output,
                npx_path: Some(npx_p.to_string_lossy().to_string()),
                guidance: None,
            }
        }
        _ => NodeEnvironment {
            available: false,
            node_path: None,
            node_version: None,
            npx_path: None,
            guidance: Some(
                "Node.js runtime was not detected on this machine. To launch the official MCP Inspector CLI, please install Node.js from https://nodejs.org. Alternatively, you can use Trans4mers' built-in zero-dependency protocol traffic log directly in this panel.".to_string(),
            ),
        },
    }
}

struct ActiveInspector {
    pid: u32,
    server_name: String,
    port: u16,
    child: tokio::process::Child,
}

#[derive(Clone, Default)]
pub struct McpInspectorManager {
    active: Arc<Mutex<Option<ActiveInspector>>>,
}

impl McpInspectorManager {
    pub fn new() -> Self {
        Self {
            active: Arc::new(Mutex::new(None)),
        }
    }

    /// Queries the current inspector status, updating if child has exited.
    pub async fn status(&self) -> McpInspectorStatus {
        let mut guard = self.active.lock().await;
        if let Some(active) = guard.as_mut() {
            match active.child.try_wait() {
                Ok(Some(_exit_status)) => {
                    *guard = None;
                    McpInspectorStatus {
                        running: false,
                        pid: None,
                        server_name: None,
                        port: None,
                    }
                }
                Ok(None) => McpInspectorStatus {
                    running: true,
                    pid: Some(active.pid),
                    server_name: Some(active.server_name.clone()),
                    port: Some(active.port),
                },
                Err(_) => {
                    *guard = None;
                    McpInspectorStatus {
                        running: false,
                        pid: None,
                        server_name: None,
                        port: None,
                    }
                }
            }
        } else {
            McpInspectorStatus {
                running: false,
                pid: None,
                server_name: None,
                port: None,
            }
        }
    }

    /// Stops any running inspector process.
    pub async fn stop(&self) -> Result<(), Trans4mersError> {
        let mut guard = self.active.lock().await;
        if let Some(mut active) = guard.take() {
            info!(pid = active.pid, server = %active.server_name, "Terminating MCP Inspector process");
            let _ = active.child.kill().await;

            #[cfg(windows)]
            {
                // Ensure the full process tree is killed on Windows
                let _ = std::process::Command::new("taskkill")
                    .args(["/F", "/T", "/PID", &active.pid.to_string()])
                    .output();
            }
        }
        Ok(())
    }

    /// Spawns `@modelcontextprotocol/inspector` pre-connected to the specified server.
    pub async fn launch(
        &self,
        server_name: &str,
        command_or_url: &str,
        args: &[String],
    ) -> Result<McpInspectorLaunchResult, Trans4mersError> {
        let node_env = detect_node_environment();
        if !node_env.available {
            return Err(Trans4mersError::Internal(
                node_env
                    .guidance
                    .unwrap_or_else(|| "Node.js is not installed".to_string()),
            ));
        }

        // Clean up any previously active inspector
        self.stop().await?;

        let port: u16 = 5173;
        let is_http =
            command_or_url.starts_with("http://") || command_or_url.starts_with("https://");

        let mut cmd = if cfg!(windows) {
            let mut c = tokio::process::Command::new("cmd");
            c.arg("/C")
                .arg("npx")
                .arg("-y")
                .arg("@modelcontextprotocol/inspector");
            c
        } else {
            let mut c = tokio::process::Command::new("npx");
            c.arg("-y").arg("@modelcontextprotocol/inspector");
            c
        };

        if is_http {
            cmd.arg("--transport").arg("sse").arg(command_or_url);
        } else {
            cmd.arg(command_or_url);
            for arg in args {
                cmd.arg(arg);
            }
        }

        cmd.stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null());

        let child = cmd.spawn().map_err(|e| {
            Trans4mersError::Internal(format!("Failed to spawn MCP Inspector: {}", e))
        })?;

        let pid = child.id().unwrap_or(0);
        info!(pid = pid, server = %server_name, "MCP Inspector spawned");

        let mut guard = self.active.lock().await;
        *guard = Some(ActiveInspector {
            pid,
            server_name: server_name.to_string(),
            port,
            child,
        });

        Ok(McpInspectorLaunchResult {
            success: true,
            pid: Some(pid),
            port,
            message: format!(
                "MCP Inspector started for '{}'. Open http://localhost:{} to view inspector interface.",
                server_name, port
            ),
        })
    }
}
