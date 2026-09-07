use async_trait::async_trait;
use std::collections::HashMap;
use std::path::PathBuf;
use std::process::Stdio;
use std::sync::Arc;
use tokio::io::AsyncWriteExt;
use tokio::process::Command;
use trans4mers_domain::error::Trans4mersError;
use trans4mers_domain::tool::{
    EffectClass, RiskLevel, Tool, ToolManifest, ToolRequest, ToolResult, ToolSource,
};

use std::sync::atomic::{AtomicU32, Ordering};

pub struct PluginSandboxConfig {
    pub max_execution_time_secs: u64,
    pub max_output_size_bytes: u64,
    pub max_concurrent_calls: u32,
}

impl Default for PluginSandboxConfig {
    fn default() -> Self {
        Self {
            max_execution_time_secs: 30,
            max_output_size_bytes: 10_485_760, // 10 MB
            max_concurrent_calls: 3,
        }
    }
}

/// Manages isolated JSON-RPC plugin subprocesses (Wasm or native binaries).
pub struct PluginRunner {
    plugin_path: PathBuf,
}

impl PluginRunner {
    pub async fn load(path: PathBuf) -> Result<Self, Trans4mersError> {
        Ok(Self { plugin_path: path })
    }

    /// Queries the plugin binary for its manifest and exposes it as a `Tool`.
    pub async fn initialize_plugin(&self) -> Result<Vec<Arc<dyn Tool>>, Trans4mersError> {
        let mut child = Command::new(&self.plugin_path)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .spawn()
            .map_err(|e| Trans4mersError::Internal(e.to_string()))?;

        let mut stdin = child
            .stdin
            .take()
            .ok_or_else(|| Trans4mersError::Internal("Failed to capture plugin stdin".to_string()))?;
        let stdout = child
            .stdout
            .take()
            .ok_or_else(|| Trans4mersError::Internal("Failed to capture plugin stdout".to_string()))?;

        let req = serde_json::json!({
            "jsonrpc": "2.0",
            "id": 1,
            "method": "manifest"
        });

        let req_bytes = serde_json::to_vec(&req)
            .map_err(|e| Trans4mersError::Internal(format!("Failed to serialize plugin request: {}", e)))?;
        stdin
            .write_all(&req_bytes)
            .await
            .map_err(|e| Trans4mersError::Internal(e.to_string()))?;
        stdin
            .write_all(b"\n")
            .await
            .map_err(|e| Trans4mersError::Internal(e.to_string()))?;

        let mut reader = tokio::io::BufReader::new(stdout);
        let mut line = String::new();
        use tokio::io::AsyncBufReadExt;
        reader
            .read_line(&mut line)
            .await
            .map_err(|e| Trans4mersError::Internal(e.to_string()))?;
        let resp: serde_json::Value =
            serde_json::from_str(&line).unwrap_or_else(|_| serde_json::json!({}));

        let mut tools: Vec<Arc<dyn Tool>> = Vec::new();
        if let Some(t) = resp.get("result") {
            let manifest = ToolManifest {
                name: t
                    .get("name")
                    .and_then(|v| v.as_str())
                    .unwrap_or("unknown_plugin")
                    .to_string(),
                description: t
                    .get("description")
                    .and_then(|v| v.as_str())
                    .unwrap_or("")
                    .to_string(),
                input_schema: t
                    .get("inputSchema")
                    .cloned()
                    .unwrap_or(serde_json::json!({})),
                output_schema: None,
                effect_class: EffectClass::NonIdempotentMutation,
                baseline_risk: RiskLevel::Medium,
                required_capabilities: vec![],
                source: ToolSource::Plugin,
                verification_command: None,
            };

            tools.push(Arc::new(PluginToolWrapper {
                plugin_path: self.plugin_path.clone(),
                manifest,
                config: Arc::new(PluginSandboxConfig::default()),
                active_calls: Arc::new(AtomicU32::new(0)),
            }));
        }

        let _ = child.kill().await;
        Ok(tools)
    }
}

pub struct PluginToolWrapper {
    plugin_path: PathBuf,
    manifest: ToolManifest,
    config: Arc<PluginSandboxConfig>,
    active_calls: Arc<AtomicU32>,
}

#[async_trait]
impl Tool for PluginToolWrapper {
    fn manifest(&self) -> &ToolManifest {
        &self.manifest
    }

    async fn execute(&self, request: &ToolRequest) -> Result<ToolResult, Trans4mersError> {
        let current_calls = self.active_calls.load(Ordering::SeqCst);
        if current_calls >= self.config.max_concurrent_calls {
            return Err(Trans4mersError::Internal(format!(
                "Plugin concurrency limit reached ({})",
                self.config.max_concurrent_calls
            )));
        }

        self.active_calls.fetch_add(1, Ordering::SeqCst);

        let execution_future = async {
            let mut child = Command::new(&self.plugin_path)
                .stdin(Stdio::piped())
                .stdout(Stdio::piped())
                .spawn()
                .map_err(|e| Trans4mersError::Internal(e.to_string()))?;

            let mut stdin = child
                .stdin
                .take()
                .ok_or_else(|| Trans4mersError::Internal("Failed to capture plugin stdin".to_string()))?;
            let stdout = child
                .stdout
                .take()
                .ok_or_else(|| Trans4mersError::Internal("Failed to capture plugin stdout".to_string()))?;

            let req = serde_json::json!({
                "jsonrpc": "2.0",
                "id": 1,
                "method": "execute",
                "params": request.arguments
            });

            let req_bytes = serde_json::to_vec(&req)
                .map_err(|e| Trans4mersError::Internal(format!("Failed to serialize plugin request: {}", e)))?;
            stdin
                .write_all(&req_bytes)
                .await
                .map_err(|e| Trans4mersError::Internal(e.to_string()))?;
            stdin
                .write_all(b"\n")
                .await
                .map_err(|e| Trans4mersError::Internal(e.to_string()))?;

            let mut reader = tokio::io::BufReader::new(stdout);
            let mut line = String::new();
            use tokio::io::AsyncBufReadExt;
            reader
                .read_line(&mut line)
                .await
                .map_err(|e| Trans4mersError::Internal(e.to_string()))?;
            if line.len() as u64 > self.config.max_output_size_bytes {
                let _ = child.kill().await;
                return Err(Trans4mersError::Internal(format!(
                    "Plugin output too large (max {})",
                    self.config.max_output_size_bytes
                )));
            }

            let resp: serde_json::Value =
                serde_json::from_str(&line).unwrap_or_else(|_| serde_json::json!({}));
            let content = resp
                .get("result")
                .and_then(|r| r.get("content"))
                .map(|c| c.to_string())
                .unwrap_or_else(|| "No content".to_string());
            let _ = child.kill().await;

            Ok(ToolResult {
                success: !resp.get("error").is_some(),
                content,
                error: resp.get("error").map(|e| e.to_string()),
                artifacts: vec![],
                metadata: HashMap::new(),
            })
        };

        let result = tokio::time::timeout(
            std::time::Duration::from_secs(self.config.max_execution_time_secs),
            execution_future,
        )
        .await;

        self.active_calls.fetch_sub(1, Ordering::SeqCst);

        match result {
            Ok(res) => res,
            Err(_) => Err(Trans4mersError::Internal(format!(
                "Plugin timed out after {}s",
                self.config.max_execution_time_secs
            ))),
        }
    }
}
