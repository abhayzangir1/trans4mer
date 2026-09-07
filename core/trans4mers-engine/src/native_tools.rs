use async_trait::async_trait;
use serde_json::json;
use std::sync::Arc;
use trans4mers_domain::error::Trans4mersError;
use trans4mers_domain::provider::LlmProvider;
use trans4mers_domain::tool::{
    Capability, EffectClass, RiskLevel, Tool, ToolManifest, ToolRequest, ToolResult, ToolSource,
};

pub struct FilesystemReadTool {
    manifest: ToolManifest,
}

impl Default for FilesystemReadTool {
    fn default() -> Self {
        Self::new()
    }
}

impl FilesystemReadTool {
    pub fn new() -> Self {
        Self {
            manifest: ToolManifest {
                name: "filesystem.read".to_string(),
                description: "Reads the content of a file from the workspace.".to_string(),
                input_schema: json!({
                    "type": "object",
                    "properties": {
                        "path": {
                            "type": "string",
                            "description": "Relative path to the file to read."
                        }
                    },
                    "required": ["path"]
                }),
                required_capabilities: vec![Capability::FilesystemRead],
                effect_class: EffectClass::ReadOnly,
                baseline_risk: RiskLevel::Safe,
                source: ToolSource::Native,
                output_schema: None,
                verification_command: None,
            },
        }
    }
}

#[async_trait]
impl Tool for FilesystemReadTool {
    fn manifest(&self) -> &ToolManifest {
        &self.manifest
    }

    async fn execute(&self, request: &ToolRequest) -> Result<ToolResult, Trans4mersError> {
        let path_val = request
            .arguments
            .get("path")
            .ok_or_else(|| Trans4mersError::Internal("Missing 'path' argument".to_string()))?;

        let relative_path = path_val
            .as_str()
            .ok_or_else(|| Trans4mersError::Internal("'path' must be a string".to_string()))?;

        let root = &request.workspace_root;

        let content = tokio::task::block_in_place(|| {
            trans4mers_storage::filesystem::FileSystemGuard::read_workspace_file(
                root,
                relative_path,
            )
        })?;

        Ok(ToolResult {
            success: true,
            content,
            error: None,
            artifacts: vec![],
            metadata: std::collections::HashMap::new(),
        })
    }
}

pub struct FilesystemListTool {
    manifest: ToolManifest,
}

impl Default for FilesystemListTool {
    fn default() -> Self {
        Self::new()
    }
}

impl FilesystemListTool {
    pub fn new() -> Self {
        Self {
            manifest: ToolManifest {
                name: "filesystem.list".to_string(),
                description: "Lists files and subdirectories within a workspace directory."
                    .to_string(),
                input_schema: json!({
                    "type": "object",
                    "properties": {
                        "path": {
                            "type": "string",
                            "description": "Relative path to directory to list. Defaults to '.' (workspace root)."
                        }
                    }
                }),
                required_capabilities: vec![Capability::FilesystemRead],
                effect_class: EffectClass::ReadOnly,
                baseline_risk: RiskLevel::Safe,
                source: ToolSource::Native,
                output_schema: None,
                verification_command: None,
            },
        }
    }
}

#[async_trait]
impl Tool for FilesystemListTool {
    fn manifest(&self) -> &ToolManifest {
        &self.manifest
    }

    async fn execute(&self, request: &ToolRequest) -> Result<ToolResult, Trans4mersError> {
        let relative_path = request
            .arguments
            .get("path")
            .and_then(|v| v.as_str())
            .unwrap_or(".");

        let root = &request.workspace_root;
        let target = if relative_path == "." || relative_path.is_empty() {
            root.clone()
        } else {
            root.join(relative_path)
        };
        let safe = trans4mers_storage::filesystem::FileSystemGuard::validate_path_in_workspace(
            root, &target,
        )?;
        let mut rd = tokio::fs::read_dir(&safe)
            .await
            .map_err(|e| Trans4mersError::Internal(format!("Failed to read directory: {}", e)))?;
        let mut list = Vec::new();
        while let Ok(Some(entry)) = rd.next_entry().await {
            let name = entry.file_name().to_string_lossy().to_string();
            let is_dir = entry.file_type().await.map(|t| t.is_dir()).unwrap_or(false);
            list.push(format!("{}{}", name, if is_dir { "/" } else { "" }));
        }
        list.sort();

        Ok(ToolResult {
            success: true,
            content: list.join("\n"),
            error: None,
            artifacts: vec![],
            metadata: std::collections::HashMap::new(),
        })
    }
}

pub struct FilesystemWriteTool {
    manifest: ToolManifest,
}

impl Default for FilesystemWriteTool {
    fn default() -> Self {
        Self::new()
    }
}

impl FilesystemWriteTool {
    pub fn new() -> Self {
        Self {
            manifest: ToolManifest {
                name: "filesystem.write".to_string(),
                description: "Writes content to a file in the workspace.".to_string(),
                input_schema: json!({
                    "type": "object",
                    "properties": {
                        "path": {
                            "type": "string",
                            "description": "Relative path to the file."
                        },
                        "content": {
                            "type": "string",
                            "description": "Content to write to the file."
                        }
                    },
                    "required": ["path", "content"]
                }),
                required_capabilities: vec![Capability::FilesystemWrite],
                effect_class: EffectClass::IdempotentMutation {
                    requires_idempotency_key: false,
                },
                baseline_risk: RiskLevel::Medium,
                source: ToolSource::Native,
                output_schema: None,
                verification_command: None,
            },
        }
    }
}

#[async_trait]
impl Tool for FilesystemWriteTool {
    fn manifest(&self) -> &ToolManifest {
        &self.manifest
    }

    async fn execute(&self, request: &ToolRequest) -> Result<ToolResult, Trans4mersError> {
        let path_val = request
            .arguments
            .get("path")
            .ok_or_else(|| Trans4mersError::Internal("Missing 'path' argument".to_string()))?;

        let relative_path = path_val
            .as_str()
            .ok_or_else(|| Trans4mersError::Internal("'path' must be a string".to_string()))?;

        let content = request
            .arguments
            .get("content")
            .ok_or_else(|| Trans4mersError::Internal("Missing 'content' argument".to_string()))?
            .as_str()
            .ok_or_else(|| Trans4mersError::Internal("'content' must be a string".to_string()))?;

        let root = &request.workspace_root;

        tokio::task::block_in_place(|| {
            trans4mers_storage::filesystem::FileSystemGuard::write_workspace_file(
                root,
                relative_path,
                content,
            )
        })?;

        Ok(ToolResult {
            success: true,
            content: format!("Wrote to {}", relative_path),
            error: None,
            artifacts: vec![relative_path.to_string()],
            metadata: std::collections::HashMap::new(),
        })
    }
}

pub struct ShellExecuteTool {
    manifest: ToolManifest,
}

impl Default for ShellExecuteTool {
    fn default() -> Self {
        Self::new()
    }
}

impl ShellExecuteTool {
    pub fn new() -> Self {
        Self {
            manifest: ToolManifest {
                name: "shell.execute".to_string(),
                description: "Executes a shell command in the workspace.".to_string(),
                input_schema: json!({
                    "type": "object",
                    "properties": {
                        "command": {
                            "type": "string",
                            "description": "The shell command to execute."
                        }
                    },
                    "required": ["command"]
                }),
                required_capabilities: vec![Capability::ShellExecute],
                effect_class: EffectClass::NonIdempotentMutation,
                baseline_risk: RiskLevel::High,
                source: ToolSource::Native,
                output_schema: None,
                verification_command: None,
            },
        }
    }
}

#[async_trait]
impl Tool for ShellExecuteTool {
    fn manifest(&self) -> &ToolManifest {
        &self.manifest
    }

    async fn execute(&self, request: &ToolRequest) -> Result<ToolResult, Trans4mersError> {
        let cmd_val = request
            .arguments
            .get("command")
            .ok_or_else(|| Trans4mersError::Internal("Missing 'command' argument".to_string()))?;

        let command = cmd_val
            .as_str()
            .ok_or_else(|| Trans4mersError::Internal("'command' must be a string".to_string()))?;

        let root = &request.workspace_root;

        // Use cross-platform shell
        let mut cmd = if cfg!(target_os = "windows") {
            let mut c = tokio::process::Command::new("powershell");
            c.arg("-Command").arg(command);
            c
        } else {
            let mut c = tokio::process::Command::new("sh");
            c.arg("-c").arg(command);
            c
        };

        let output = tokio::time::timeout(
            std::time::Duration::from_secs(60),
            cmd.current_dir(root).output(),
        )
        .await
        .map_err(|_| {
            Trans4mersError::Internal("Command execution timed out after 60 seconds".to_string())
        })?
        .map_err(|e| Trans4mersError::Internal(format!("Failed to execute command: {}", e)))?;

        let max_output_chars = 32_000;
        let truncate_output = |s: String| -> String {
            if s.len() > max_output_chars {
                let tail: String = s
                    .chars()
                    .rev()
                    .take(max_output_chars / 2)
                    .collect::<Vec<_>>()
                    .into_iter()
                    .rev()
                    .collect();
                let head: String = s.chars().take(max_output_chars / 2).collect();
                format!(
                    "{}\n\n... [OUTPUT TRUNCATED ({} total chars)] ...\n\n{}",
                    head,
                    s.len(),
                    tail
                )
            } else {
                s
            }
        };

        let stdout = truncate_output(String::from_utf8_lossy(&output.stdout).to_string());
        let stderr = truncate_output(String::from_utf8_lossy(&output.stderr).to_string());
        let success = output.status.success();

        let content = if success { stdout } else { stderr.clone() };

        Ok(ToolResult {
            success,
            content,
            error: if !success { Some(stderr) } else { None },
            artifacts: vec![],
            metadata: std::collections::HashMap::new(),
        })
    }
}

/// Helper function to register all native tools into a given ToolExecutor
pub async fn register_native_tools(
    executor: &crate::tool_executor::ToolExecutor,
    app_state: Arc<crate::app_state::AppState>,
    provider: Arc<dyn LlmProvider>,
) {
    executor
        .register_tool(Arc::new(FilesystemReadTool::new()))
        .await;
    executor
        .register_tool(Arc::new(FilesystemListTool::new()))
        .await;
    executor
        .register_tool(Arc::new(FilesystemWriteTool::new()))
        .await;
    executor
        .register_tool(Arc::new(ShellExecuteTool::new()))
        .await;
    executor.register_tool(Arc::new(BrowserTool::new())).await;
    executor
        .register_tool(Arc::new(BrowserNavigateTool::new(app_state.clone())))
        .await;
    executor
        .register_tool(Arc::new(BrowserClickTool::new(app_state.clone())))
        .await;
    executor
        .register_tool(Arc::new(BrowserTypeTool::new(app_state.clone())))
        .await;
    executor
        .register_tool(Arc::new(BrowserExtractTool::new(app_state.clone())))
        .await;
    executor
        .register_tool(Arc::new(BrowserScreenshotTool::new(app_state.clone())))
        .await;
    executor
        .register_tool(Arc::new(crate::delegate_task::DelegateTaskTool::new(
            app_state.clone(),
            provider.clone(),
        )))
        .await;
    executor
        .register_tool(Arc::new(crate::complete_task::CompleteTaskTool::new(
            app_state.clone(),
        )))
        .await;
    executor
        .register_tool(Arc::new(MemorizeRuleTool::new(app_state.clone(), provider)))
        .await;
    executor.register_tool(Arc::new(WebSearchTool::new())).await;
    executor
        .register_tool(Arc::new(MessageSendTool::new(app_state.clone())))
        .await;
    executor
        .register_tool(Arc::new(DocumentSearchTool::new(app_state.clone())))
        .await;
    executor
        .register_tool(Arc::new(UiPreferenceTool::new(app_state.clone())))
        .await;
    executor
        .register_tool(Arc::new(SchedulerCreateTool::new(app_state.clone())))
        .await;
    executor.register_tool(Arc::new(GitStatusTool::new())).await;
    executor.register_tool(Arc::new(GitDiffTool::new())).await;
    executor
        .register_tool(Arc::new(MemoryInsertTool::new(app_state.clone())))
        .await;
    executor
        .register_tool(Arc::new(MemoryReplaceTool::new(app_state.clone())))
        .await;
    executor
        .register_tool(Arc::new(MemorySearchTool::new(app_state.clone())))
        .await;
    executor
        .register_tool(Arc::new(MemoryArchiveTool::new(app_state)))
        .await;
}

pub struct BrowserTool {
    manifest: ToolManifest,
    client: reqwest::Client,
}

impl Default for BrowserTool {
    fn default() -> Self {
        Self::new()
    }
}

impl BrowserTool {
    pub fn new() -> Self {
        Self {
            manifest: ToolManifest {
                name: "browser.fetch".to_string(),
                description:
                    "Navigates to a web URL, fetches content, and extracts clean readable text."
                        .to_string(),
                input_schema: json!({
                    "type": "object",
                    "properties": {
                        "url": {
                            "type": "string",
                            "description": "The HTTP or HTTPS URL to read."
                        }
                    },
                    "required": ["url"]
                }),
                required_capabilities: vec![Capability::BrowserNavigate],
                effect_class: EffectClass::ReadOnly,
                baseline_risk: RiskLevel::Safe,
                source: ToolSource::Native,
                output_schema: None,
                verification_command: None,
            },
            client: reqwest::Client::builder()
                .timeout(std::time::Duration::from_secs(15))
                .user_agent("Mozilla/5.0 (Windows NT 10.0; Win64; x64) Trans4mersBrowser/1.0")
                .build()
                .unwrap_or_default(),
        }
    }
}

#[async_trait]
impl Tool for BrowserTool {
    fn manifest(&self) -> &ToolManifest {
        &self.manifest
    }

    async fn execute(&self, request: &ToolRequest) -> Result<ToolResult, Trans4mersError> {
        let url_val = request
            .arguments
            .get("url")
            .and_then(|v| v.as_str())
            .ok_or_else(|| Trans4mersError::Internal("Missing 'url' argument".to_string()))?;

        if !url_val.starts_with("http://") && !url_val.starts_with("https://") {
            return Err(Trans4mersError::Internal(
                "URL must start with http:// or https://".to_string(),
            ));
        }

        let resp = self.client.get(url_val).send().await.map_err(|e| {
            Trans4mersError::Internal(format!("Failed to fetch URL {}: {}", url_val, e))
        })?;

        let status = resp.status();
        if !status.is_success() {
            return Ok(ToolResult {
                success: false,
                content: String::new(),
                error: Some(format!("HTTP error status {}", status)),
                artifacts: vec![],
                metadata: std::collections::HashMap::new(),
            });
        }

        let body = resp.text().await.map_err(|e| {
            Trans4mersError::Internal(format!("Failed to read response body: {}", e))
        })?;

        let clean_text = clean_html(&body);
        let truncated: String = clean_text.chars().take(8000).collect();

        Ok(ToolResult {
            success: true,
            content: truncated,
            error: None,
            artifacts: vec![],
            metadata: std::collections::HashMap::new(),
        })
    }
}

fn clean_html(html: &str) -> String {
    let mut in_tag = false;
    let mut in_script_or_style = false;
    let mut text = String::with_capacity(html.len() / 2);
    let lower = html.to_lowercase();
    let bytes = html.as_bytes();
    let mut idx = 0;

    while idx < bytes.len() {
        if bytes[idx] == b'<' {
            in_tag = true;
            if lower[idx..].starts_with("<script") || lower[idx..].starts_with("<style") {
                in_script_or_style = true;
            } else if lower[idx..].starts_with("</script>") || lower[idx..].starts_with("</style>")
            {
                in_script_or_style = false;
            }
        } else if bytes[idx] == b'>' {
            in_tag = false;
            text.push(' ');
        } else if !in_tag && !in_script_or_style {
            text.push(bytes[idx] as char);
        }
        idx += 1;
    }

    text.split_whitespace().collect::<Vec<&str>>().join(" ")
}

pub struct WebSearchTool {
    manifest: ToolManifest,
}

impl Default for WebSearchTool {
    fn default() -> Self {
        Self::new()
    }
}

impl WebSearchTool {
    pub fn new() -> Self {
        Self {
            manifest: ToolManifest {
                name: "web_search".to_string(),
                description: "Search the web using DuckDuckGo Lite.".to_string(),
                input_schema: json!({
                    "type": "object",
                    "properties": {
                        "query": {
                            "type": "string",
                            "description": "The search query."
                        }
                    },
                    "required": ["query"]
                }),
                required_capabilities: vec![Capability::NetworkRequest],
                effect_class: EffectClass::ReadOnly,
                baseline_risk: RiskLevel::Safe,
                source: ToolSource::Native,
                output_schema: None,
                verification_command: None,
            },
        }
    }
}

#[async_trait]
impl Tool for WebSearchTool {
    fn manifest(&self) -> &ToolManifest {
        &self.manifest
    }

    async fn execute(&self, request: &ToolRequest) -> Result<ToolResult, Trans4mersError> {
        let query_val = request
            .arguments
            .get("query")
            .ok_or_else(|| Trans4mersError::Internal("Missing 'query' argument".to_string()))?;

        let query = query_val
            .as_str()
            .ok_or_else(|| Trans4mersError::Internal("'query' must be a string".to_string()))?;

        let client = reqwest::Client::new();
        let response = client
            .get("https://lite.duckduckgo.com/lite/")
            .query(&[("q", query)])
            .header(
                reqwest::header::USER_AGENT,
                "Mozilla/5.0 (Windows NT 10.0; Win64; x64)",
            )
            .send()
            .await
            .map_err(|e| Trans4mersError::Internal(format!("HTTP request failed: {}", e)))?;

        let success = response.status().is_success();
        let content = response
            .text()
            .await
            .map_err(|e| Trans4mersError::Internal(format!("Failed to read response: {}", e)))?;

        Ok(ToolResult {
            success,
            content,
            error: if !success {
                Some("HTTP request did not return success status".to_string())
            } else {
                None
            },
            artifacts: vec![],
            metadata: std::collections::HashMap::new(),
        })
    }
}

pub struct MemorizeRuleTool {
    manifest: ToolManifest,
    app_state: Arc<crate::app_state::AppState>,
    provider: Arc<dyn LlmProvider>,
}

impl MemorizeRuleTool {
    pub fn new(app_state: Arc<crate::app_state::AppState>, provider: Arc<dyn LlmProvider>) -> Self {
        Self {
            manifest: ToolManifest {
                name: "memorize_rule".to_string(),
                description: "Call this when the human corrects you. Draft a permanent rule so you never make this mistake again.".to_string(),
                input_schema: json!({
                    "type": "object",
                    "properties": {
                        "rule_text": { "type": "string", "description": "The explicit instruction" },
                        "confidence": { "type": "string", "enum": ["Low", "Medium", "High"], "description": "Your confidence in this rule" },
                        "language": { "type": "string", "description": "e.g., rust, typescript" },
                        "framework": { "type": "string", "description": "e.g., react, tauri" },
                        "file_pattern": { "type": "string", "description": "e.g., *.tsx, src/store/*" }
                    },
                    "required": ["rule_text", "confidence"]
                }),
                effect_class: EffectClass::IdempotentMutation { requires_idempotency_key: false },
                baseline_risk: RiskLevel::Medium,
                required_capabilities: vec![Capability::Custom("MemorizeRule".to_string())],
                source: ToolSource::Native,
                output_schema: None,
                verification_command: None,
            },
            app_state,
            provider,
        }
    }
}

#[async_trait]
impl Tool for MemorizeRuleTool {
    fn manifest(&self) -> &ToolManifest {
        &self.manifest
    }

    async fn execute(&self, request: &ToolRequest) -> Result<ToolResult, Trans4mersError> {
        use trans4mers_domain::ids::LearnedRuleId;
        use trans4mers_domain::learning::{Confidence, LearnedRule, RuleMetadata};

        let rule_text = request
            .arguments
            .get("rule_text")
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_string();
        let confidence_str = request
            .arguments
            .get("confidence")
            .and_then(|v| v.as_str())
            .unwrap_or("Medium");
        let confidence = std::str::FromStr::from_str(confidence_str).unwrap_or(Confidence::Medium);

        let metadata = RuleMetadata {
            language: request
                .arguments
                .get("language")
                .and_then(|v| v.as_str())
                .map(|s| s.to_string()),
            framework: request
                .arguments
                .get("framework")
                .and_then(|v| v.as_str())
                .map(|s| s.to_string()),
            file_pattern: request
                .arguments
                .get("file_pattern")
                .and_then(|v| v.as_str())
                .map(|s| s.to_string()),
            source_agent_id: Some(request.requesting_agent_id.to_string()),
            source_execution_id: Some(request.execution_id.to_string()),
        };

        let rule = LearnedRule {
            id: LearnedRuleId::new(),
            project_id: Some(request.project_id),
            rule_text: rule_text.clone(),
            confidence,
            metadata,
            created_at: chrono::Utc::now(),
            embedding: None,
        };

        let embedding = self
            .provider
            .embed(
                &rule.rule_text,
                &trans4mers_domain::config::ModelConfig::default(),
            )
            .await
            .unwrap_or_else(|_| vec![0.0; 768]);

        let mem_engine = crate::memory_engine::MemoryEngine::new(self.app_state.clone());
        mem_engine.learn_rule(&rule, &embedding)?;

        Ok(ToolResult {
            success: true,
            content: format!(
                "Rule memorized with {} confidence: {}",
                confidence_str, rule_text
            ),
            error: None,
            artifacts: vec![],
            metadata: std::collections::HashMap::new(),
        })
    }
}

pub struct MessageSendTool {
    manifest: ToolManifest,
    app_state: Arc<crate::app_state::AppState>,
}

impl MessageSendTool {
    pub fn new(app_state: Arc<crate::app_state::AppState>) -> Self {
        Self {
            manifest: ToolManifest {
                name: "message.send".to_string(),
                description: "Sends a message to an active channel or conversation.".to_string(),
                input_schema: json!({
                    "type": "object",
                    "properties": {
                        "content": { "type": "string", "description": "The message text to send." },
                        "channel_id": { "type": "string", "description": "Optional target channel ID. Defaults to general." }
                    },
                    "required": ["content"]
                }),
                required_capabilities: vec![Capability::Custom("MessageSend".to_string())],
                effect_class: EffectClass::IdempotentMutation {
                    requires_idempotency_key: false,
                },
                baseline_risk: RiskLevel::Safe,
                source: ToolSource::Native,
                output_schema: None,
                verification_command: None,
            },
            app_state,
        }
    }
}

#[async_trait]
impl Tool for MessageSendTool {
    fn manifest(&self) -> &ToolManifest {
        &self.manifest
    }

    async fn execute(&self, request: &ToolRequest) -> Result<ToolResult, Trans4mersError> {
        let content = request
            .arguments
            .get("content")
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_string();

        let mut target_convo =
            trans4mers_domain::ids::ConversationId::from_uuid(*request.project_id.as_uuid());
        let mut target_channel_str = request
            .arguments
            .get("channel_id")
            .and_then(|v| v.as_str())
            .unwrap_or("general")
            .to_string();

        if let Some(db) = self.app_state.get_project_db(&request.project_id) {
            let _ = db.with_read_conn(|conn| {
                if let Ok(c_id_str) = conn.query_row(
                    "SELECT conversation_id FROM agent_executions WHERE id = ?1",
                    rusqlite::params![request.execution_id.as_str()],
                    |row| row.get::<_, String>(0)
                )
                    && let Ok(cid) = trans4mers_domain::ids::ConversationId::from_str(&c_id_str) {
                        target_convo = cid;
                    }

                // If channel was default "general", inherit the triggering message's channel & conversation
                if target_channel_str == "general"
                    && let Ok(payload_str) = conn.query_row(
                        "SELECT payload FROM inbox_messages WHERE recipient_agent_id = ?1 ORDER BY created_at DESC LIMIT 1",
                        rusqlite::params![request.requesting_agent_id.as_str()],
                        |row| row.get::<_, String>(0)
                    )
                        && let Ok(parsed) = serde_json::from_str::<serde_json::Value>(&payload_str) {
                            if let Some(ch) = parsed.get("channel_id").and_then(|v| v.as_str()) {
                                target_channel_str = ch.to_string();
                            }
                            if let Some(cv) = parsed.get("conversation_id").and_then(|v| v.as_str())
                                && let Ok(cid) = trans4mers_domain::ids::ConversationId::from_str(cv) {
                                    target_convo = cid;
                                }
                        }
                Ok(())
            });
        }

        let channel_id = trans4mers_domain::ids::ChannelId::from_str(&target_channel_str)
            .map_err(|e| Trans4mersError::Internal(e.to_string()))?;

        let agent_msg = trans4mers_domain::message::Message {
            id: trans4mers_domain::ids::MessageId::new(),
            conversation_id: target_convo,
            channel_id,
            thread_id: None,
            sender: trans4mers_domain::actor::Actor::Agent(request.requesting_agent_id),
            content: content.clone(),
            message_kind: trans4mers_domain::message::MessageKind::Chat,
            mentions: vec![],
            attachments: vec![],
            requires_approval: false,
            approval_id: None,
            created_at: chrono::Utc::now(),
            updated_at: chrono::Utc::now(),
        };

        if let Some(db) = self.app_state.get_project_db(&request.project_id) {
            let env = db.with_write_tx(|conn| {
                crate::cqrs::commit_event(
                    conn,
                    trans4mers_domain::event::DomainEvent::MessageSent { message: agent_msg },
                    trans4mers_domain::ids::ActorId::from_uuid(
                        *request.requesting_agent_id.as_uuid(),
                    ),
                )
            })?;
            let env_arc = Arc::new(env);
            let _ = self
                .app_state
                .get_event_bus(&request.project_id)
                .publish(env_arc.clone());
            let _ = self.app_state.global_event_bus.publish(env_arc);
        }

        Ok(ToolResult {
            success: true,
            content: format!("Sent message to channel: {}", content),
            error: None,
            artifacts: vec![],
            metadata: std::collections::HashMap::new(),
        })
    }
}

pub struct UiPreferenceTool {
    manifest: ToolManifest,
    app_state: Arc<crate::app_state::AppState>,
}

impl UiPreferenceTool {
    pub fn new(app_state: Arc<crate::app_state::AppState>) -> Self {
        Self {
            manifest: ToolManifest {
                name: "ui.set_preference".to_string(),
                description: "Sets a UI preference (such as theme) for the user interface."
                    .to_string(),
                input_schema: json!({
                    "type": "object",
                    "properties": {
                        "preference_key": { "type": "string", "description": "The setting key, e.g. theme" },
                        "preference_value": { "type": "string", "description": "The setting value, e.g. dark or light" }
                    },
                    "required": ["preference_key", "preference_value"]
                }),
                required_capabilities: vec![Capability::Custom("UiPreference".to_string())],
                effect_class: EffectClass::IdempotentMutation {
                    requires_idempotency_key: false,
                },
                baseline_risk: RiskLevel::Safe,
                source: ToolSource::Native,
                output_schema: None,
                verification_command: None,
            },
            app_state,
        }
    }
}

#[async_trait]
impl Tool for UiPreferenceTool {
    fn manifest(&self) -> &ToolManifest {
        &self.manifest
    }

    async fn execute(&self, request: &ToolRequest) -> Result<ToolResult, Trans4mersError> {
        let key = request
            .arguments
            .get("preference_key")
            .and_then(|v| v.as_str())
            .unwrap_or("");
        let val = request
            .arguments
            .get("preference_value")
            .and_then(|v| v.as_str())
            .unwrap_or("");

        if key == "theme" {
            let mut cfg = self.app_state.config.write().await;
            cfg.general.theme = val.to_string();
        }

        Ok(ToolResult {
            success: true,
            content: format!("Set UI preference '{}' to '{}'", key, val),
            error: None,
            artifacts: vec![],
            metadata: std::collections::HashMap::new(),
        })
    }
}

pub struct DocumentSearchTool {
    manifest: ToolManifest,
    app_state: Arc<crate::app_state::AppState>,
}

impl DocumentSearchTool {
    pub fn new(app_state: Arc<crate::app_state::AppState>) -> Self {
        Self {
            manifest: ToolManifest {
                name: "search_documents".to_string(),
                description: "Performs hybrid semantic and keyword search across indexed project documents and code files using BM25 and vector embeddings.".to_string(),
                input_schema: json!({
                    "type": "object",
                    "properties": {
                        "query": {
                            "type": "string",
                            "description": "The search query, concepts, questions, or code symbols to search for."
                        },
                        "limit": {
                            "type": "integer",
                            "description": "Maximum number of relevant chunks to return (default 5, max 20)."
                        },
                        "file_pattern": {
                            "type": "string",
                            "description": "Optional file path pattern or extension to restrict the search."
                        }
                    },
                    "required": ["query"]
                }),
                required_capabilities: vec![Capability::FilesystemRead],
                effect_class: EffectClass::ReadOnly,
                baseline_risk: RiskLevel::Safe,
                source: ToolSource::Native,
                output_schema: None,
                verification_command: None,
            },
            app_state,
        }
    }
}

#[async_trait]
impl Tool for DocumentSearchTool {
    fn manifest(&self) -> &ToolManifest {
        &self.manifest
    }

    async fn execute(&self, request: &ToolRequest) -> Result<ToolResult, Trans4mersError> {
        let query = request
            .arguments
            .get("query")
            .and_then(|v| v.as_str())
            .ok_or_else(|| Trans4mersError::Internal("Missing 'query' argument".to_string()))?;

        let limit = request
            .arguments
            .get("limit")
            .and_then(|v| v.as_u64())
            .unwrap_or(5) as usize;

        let file_pattern = request
            .arguments
            .get("file_pattern")
            .and_then(|v| v.as_str());

        let results = crate::document_rag::DocumentRagEngine::search(
            &self.app_state,
            &request.project_id,
            query,
            limit,
            file_pattern,
        )
        .await?;

        if results.is_empty() {
            return Ok(ToolResult {
                success: true,
                content: format!("No matching documents found for query: '{}'", query),
                error: None,
                artifacts: vec![],
                metadata: std::collections::HashMap::new(),
            });
        }

        let mut output = format!(
            "Found {} relevant document chunk(s) for '{}':\n\n",
            results.len(),
            query
        );
        for (i, r) in results.iter().enumerate() {
            output.push_str(&format!(
                "[{}] {}:{}-{} (kind: {}, score: {:.4})\n```\n{}\n```\n\n",
                i + 1,
                r.file_path,
                r.line_start,
                r.line_end,
                r.kind,
                r.score,
                r.snippet.trim()
            ));
        }

        Ok(ToolResult {
            success: true,
            content: output,
            error: None,
            artifacts: vec![],
            metadata: std::collections::HashMap::new(),
        })
    }
}

pub struct SchedulerCreateTool {
    manifest: ToolManifest,
    app_state: Arc<crate::app_state::AppState>,
}

impl SchedulerCreateTool {
    pub fn new(app_state: Arc<crate::app_state::AppState>) -> Self {
        Self {
            manifest: ToolManifest {
                name: "scheduler.create_schedule".to_string(),
                description:
                    "Schedules a recurring or timed automation task for an agent in this project."
                        .to_string(),
                input_schema: json!({
                    "type": "object",
                    "properties": {
                        "cron_expression": {
                            "type": "string",
                            "description": "Standard 5-part cron expression (e.g. '0 2 * * *' for 2 AM daily) or interval (e.g. '@every 1h', '@every 30m')."
                        },
                        "human_readable": {
                            "type": "string",
                            "description": "Short human-readable summary of the scheduled action (e.g. 'Daily Security Audit')."
                        },
                        "action_prompt": {
                            "type": "string",
                            "description": "The exact prompt instructions to execute when this scheduled task triggers."
                        },
                        "target_agent_id": {
                            "type": "string",
                            "description": "Optional target agent definition ID to execute the task (defaults to 'boss')."
                        }
                    },
                    "required": ["cron_expression", "human_readable", "action_prompt"]
                }),
                required_capabilities: vec![Capability::Custom("SchedulerAdmin".to_string())],
                effect_class: EffectClass::IdempotentMutation {
                    requires_idempotency_key: false,
                },
                baseline_risk: RiskLevel::Low,
                source: ToolSource::Native,
                output_schema: None,
                verification_command: None,
            },
            app_state,
        }
    }
}

#[async_trait]
impl Tool for SchedulerCreateTool {
    fn manifest(&self) -> &ToolManifest {
        &self.manifest
    }

    async fn execute(&self, request: &ToolRequest) -> Result<ToolResult, Trans4mersError> {
        let cron_expression = request
            .arguments
            .get("cron_expression")
            .and_then(|v| v.as_str())
            .ok_or_else(|| {
                Trans4mersError::Internal("Missing 'cron_expression' argument".to_string())
            })?;

        let human_readable = request
            .arguments
            .get("human_readable")
            .and_then(|v| v.as_str())
            .ok_or_else(|| {
                Trans4mersError::Internal("Missing 'human_readable' argument".to_string())
            })?;

        let action_prompt = request
            .arguments
            .get("action_prompt")
            .and_then(|v| v.as_str())
            .ok_or_else(|| {
                Trans4mersError::Internal("Missing 'action_prompt' argument".to_string())
            })?;

        let target_agent_id = request
            .arguments
            .get("target_agent_id")
            .and_then(|v| v.as_str())
            .unwrap_or("boss");

        let pid = &request.project_id;
        let db = self.app_state.get_project_db(pid).ok_or_else(|| {
            Trans4mersError::Internal(format!("Project DB not found for '{}'", pid))
        })?;

        let task_id = uuid::Uuid::new_v4().to_string();
        let now = chrono::Utc::now();
        let next_run_at = crate::scheduled_automation::ScheduledAutomationManager::compute_next_run(
            cron_expression,
            now,
        );

        let scheduled_task = trans4mers_storage::repos::scheduled_task_repo::ScheduledTask {
            id: task_id.clone(),
            project_id: pid.to_string(),
            conversation_id: "scheduled-automations".to_string(),
            target_agent_id: target_agent_id.to_string(),
            cron_expression: cron_expression.to_string(),
            human_readable: human_readable.to_string(),
            action_prompt: action_prompt.to_string(),
            is_active: true,
            last_run_at: None,
            next_run_at,
            created_at: now,
        };

        db.with_write_tx(|conn| {
            trans4mers_storage::repos::scheduled_task_repo::create_task(conn, &scheduled_task)
        })?;

        Ok(ToolResult {
            success: true,
            content: format!(
                "Scheduled task created successfully. Task ID: '{}', summary: '{}', schedule: '{}'",
                task_id, human_readable, cron_expression
            ),
            error: None,
            artifacts: vec![],
            metadata: std::collections::HashMap::new(),
        })
    }
}

pub struct SchedulerCancelTool {
    manifest: ToolManifest,
    app_state: Arc<crate::app_state::AppState>,
}

impl SchedulerCancelTool {
    pub fn new(app_state: Arc<crate::app_state::AppState>) -> Self {
        Self {
            manifest: ToolManifest {
                name: "scheduler.cancel_schedule".to_string(),
                description: "Cancels or deletes a scheduled task by its task ID.".to_string(),
                input_schema: json!({
                    "type": "object",
                    "properties": {
                        "task_id": {
                            "type": "string",
                            "description": "The UUID of the scheduled task to cancel."
                        }
                    },
                    "required": ["task_id"]
                }),
                required_capabilities: vec![Capability::Custom("SchedulerAdmin".to_string())],
                effect_class: EffectClass::IdempotentMutation {
                    requires_idempotency_key: false,
                },
                baseline_risk: RiskLevel::Safe,
                source: ToolSource::Native,
                output_schema: None,
                verification_command: None,
            },
            app_state,
        }
    }
}

#[async_trait]
impl Tool for SchedulerCancelTool {
    fn manifest(&self) -> &ToolManifest {
        &self.manifest
    }

    async fn execute(&self, request: &ToolRequest) -> Result<ToolResult, Trans4mersError> {
        let task_id = request
            .arguments
            .get("task_id")
            .and_then(|v| v.as_str())
            .ok_or_else(|| Trans4mersError::Internal("Missing 'task_id' argument".to_string()))?;

        let pid = &request.project_id;
        let db = self.app_state.get_project_db(pid).ok_or_else(|| {
            Trans4mersError::Internal(format!("Project DB not found for '{}'", pid))
        })?;

        db.with_write_tx(|conn| {
            trans4mers_storage::repos::scheduled_task_repo::delete_task(conn, task_id)
        })?;

        Ok(ToolResult {
            success: true,
            content: format!(
                "Scheduled task '{}' has been cancelled and removed.",
                task_id
            ),
            error: None,
            artifacts: vec![],
            metadata: std::collections::HashMap::new(),
        })
    }
}

// ---------------------------------------------------------------------------
// Sovereign Browser Native Tools (WS-1)
// ---------------------------------------------------------------------------

pub struct BrowserNavigateTool {
    manifest: ToolManifest,
    app_state: Arc<crate::app_state::AppState>,
}

impl BrowserNavigateTool {
    pub fn new(app_state: Arc<crate::app_state::AppState>) -> Self {
        Self {
            manifest: ToolManifest {
                name: "browser.navigate".to_string(),
                description: "Navigates an isolated browser space to a URL with auto-waiting for load and DOM readiness.".to_string(),
                input_schema: json!({
                    "type": "object",
                    "properties": {
                        "url": {
                            "type": "string",
                            "description": "Target HTTP or HTTPS URL."
                        },
                        "space_id": {
                            "type": "string",
                            "description": "Optional space ID, defaults to project primary space."
                        }
                    },
                    "required": ["url"]
                }),
                required_capabilities: vec![Capability::BrowserNavigate],
                effect_class: EffectClass::NonIdempotentMutation,
                baseline_risk: RiskLevel::Medium,
                source: ToolSource::Native,
                output_schema: None,
                verification_command: None,
            },
            app_state,
        }
    }
}

#[async_trait]
impl Tool for BrowserNavigateTool {
    fn manifest(&self) -> &ToolManifest {
        &self.manifest
    }

    async fn execute(&self, request: &ToolRequest) -> Result<ToolResult, Trans4mersError> {
        let url = request
            .arguments
            .get("url")
            .and_then(|v| v.as_str())
            .ok_or_else(|| Trans4mersError::Internal("Missing 'url' argument".to_string()))?;

        let space_id = request
            .arguments
            .get("space_id")
            .and_then(|v| v.as_str())
            .unwrap_or("bspace_default");

        let frame = crate::browser_space_manager::BrowserSpaceManager::navigate(
            self.app_state.clone(),
            &request.project_id,
            space_id,
            url,
        )
        .await?;

        let summary = format!(
            "Successfully navigated to '{}'. Title: '{}'. Status: {}. SSL: {}.\n\nPage Content Preview:\n{}",
            frame.current_url,
            frame.page_title,
            frame.status_code,
            frame.is_secure,
            frame.html_snippet
        );

        Ok(ToolResult {
            success: true,
            content: summary,
            error: None,
            artifacts: vec![],
            metadata: std::collections::HashMap::new(),
        })
    }
}

pub struct BrowserClickTool {
    manifest: ToolManifest,
    app_state: Arc<crate::app_state::AppState>,
}

impl BrowserClickTool {
    pub fn new(app_state: Arc<crate::app_state::AppState>) -> Self {
        Self {
            manifest: ToolManifest {
                name: "browser.click".to_string(),
                description: "Clicks on an interactive button, link, or element using CSS selector, text, or ARIA label with visibility and enabled checks.".to_string(),
                input_schema: json!({
                    "type": "object",
                    "properties": {
                        "selector": {
                            "type": "string",
                            "description": "CSS selector, button/link text, or aria-label"
                        },
                        "space_id": {
                            "type": "string",
                            "description": "Optional space ID, defaults to project primary space."
                        }
                    },
                    "required": ["selector"]
                }),
                required_capabilities: vec![Capability::BrowserInteract],
                effect_class: EffectClass::NonIdempotentMutation,
                baseline_risk: RiskLevel::Medium,
                source: ToolSource::Native,
                output_schema: None,
                verification_command: None,
            },
            app_state,
        }
    }
}

#[async_trait]
impl Tool for BrowserClickTool {
    fn manifest(&self) -> &ToolManifest {
        &self.manifest
    }

    async fn execute(&self, request: &ToolRequest) -> Result<ToolResult, Trans4mersError> {
        let selector = request
            .arguments
            .get("selector")
            .and_then(|v| v.as_str())
            .ok_or_else(|| Trans4mersError::Internal("Missing 'selector' argument".to_string()))?;

        let space_id = request
            .arguments
            .get("space_id")
            .and_then(|v| v.as_str())
            .unwrap_or("bspace_default");

        let action_res = crate::browser_space_manager::BrowserSpaceManager::click(
            self.app_state.clone(),
            &request.project_id,
            space_id,
            selector,
        )
        .await?;

        Ok(ToolResult {
            success: action_res.success,
            content: action_res.message,
            error: None,
            artifacts: vec![],
            metadata: std::collections::HashMap::new(),
        })
    }
}

pub struct BrowserTypeTool {
    manifest: ToolManifest,
    app_state: Arc<crate::app_state::AppState>,
}

impl BrowserTypeTool {
    pub fn new(app_state: Arc<crate::app_state::AppState>) -> Self {
        Self {
            manifest: ToolManifest {
                name: "browser.type".to_string(),
                description: "Types text into an interactive input or textarea element with synthetic event dispatch.".to_string(),
                input_schema: json!({
                    "type": "object",
                    "properties": {
                        "selector": {
                            "type": "string",
                            "description": "CSS selector, placeholder, name, or label"
                        },
                        "text": {
                            "type": "string",
                            "description": "Text value to enter into the field"
                        },
                        "space_id": {
                            "type": "string",
                            "description": "Optional space ID, defaults to project primary space."
                        }
                    },
                    "required": ["selector", "text"]
                }),
                required_capabilities: vec![Capability::BrowserInteract],
                effect_class: EffectClass::NonIdempotentMutation,
                baseline_risk: RiskLevel::Medium,
                source: ToolSource::Native,
                output_schema: None,
                verification_command: None,
            },
            app_state,
        }
    }
}

#[async_trait]
impl Tool for BrowserTypeTool {
    fn manifest(&self) -> &ToolManifest {
        &self.manifest
    }

    async fn execute(&self, request: &ToolRequest) -> Result<ToolResult, Trans4mersError> {
        let selector = request
            .arguments
            .get("selector")
            .and_then(|v| v.as_str())
            .ok_or_else(|| Trans4mersError::Internal("Missing 'selector' argument".to_string()))?;

        let text = request
            .arguments
            .get("text")
            .and_then(|v| v.as_str())
            .ok_or_else(|| Trans4mersError::Internal("Missing 'text' argument".to_string()))?;

        let space_id = request
            .arguments
            .get("space_id")
            .and_then(|v| v.as_str())
            .unwrap_or("bspace_default");

        let action_res = crate::browser_space_manager::BrowserSpaceManager::type_text(
            self.app_state.clone(),
            &request.project_id,
            space_id,
            selector,
            text,
        )
        .await?;

        Ok(ToolResult {
            success: action_res.success,
            content: action_res.message,
            error: None,
            artifacts: vec![],
            metadata: std::collections::HashMap::new(),
        })
    }
}

pub struct BrowserExtractTool {
    manifest: ToolManifest,
    app_state: Arc<crate::app_state::AppState>,
}

impl BrowserExtractTool {
    pub fn new(app_state: Arc<crate::app_state::AppState>) -> Self {
        Self {
            manifest: ToolManifest {
                name: "browser.extract".to_string(),
                description: "Extracts readable text in clean Markdown format or structured JSON with URL and timestamp provenance.".to_string(),
                input_schema: json!({
                    "type": "object",
                    "properties": {
                        "mode": {
                            "type": "string",
                            "enum": ["text", "structured"],
                            "description": "Extraction mode: 'text' (clean Markdown) or 'structured' (JSON schema extraction)"
                        },
                        "schema_hint": {
                            "type": "string",
                            "description": "Optional schema hint describing desired fields for structured mode."
                        },
                        "space_id": {
                            "type": "string",
                            "description": "Optional space ID, defaults to project primary space."
                        }
                    }
                }),
                required_capabilities: vec![Capability::BrowserInteract],
                effect_class: EffectClass::ReadOnly,
                baseline_risk: RiskLevel::Safe,
                source: ToolSource::Native,
                output_schema: None,
                verification_command: None,
            },
            app_state,
        }
    }
}

#[async_trait]
impl Tool for BrowserExtractTool {
    fn manifest(&self) -> &ToolManifest {
        &self.manifest
    }

    async fn execute(&self, request: &ToolRequest) -> Result<ToolResult, Trans4mersError> {
        let space_id = request
            .arguments
            .get("space_id")
            .and_then(|v| v.as_str())
            .unwrap_or("bspace_default");

        let mode = request
            .arguments
            .get("mode")
            .and_then(|v| v.as_str())
            .unwrap_or("text");

        let content = if mode == "structured" {
            let schema_hint = request
                .arguments
                .get("schema_hint")
                .and_then(|v| v.as_str())
                .unwrap_or("");
            let val = crate::browser_space_manager::BrowserSpaceManager::extract_structured(
                self.app_state.clone(),
                &request.project_id,
                space_id,
                schema_hint,
            )
            .await?;
            serde_json::to_string_pretty(&val).unwrap_or_default()
        } else {
            crate::browser_space_manager::BrowserSpaceManager::extract_text(
                self.app_state.clone(),
                &request.project_id,
                space_id,
            )
            .await?
        };

        Ok(ToolResult {
            success: true,
            content,
            error: None,
            artifacts: vec![],
            metadata: std::collections::HashMap::new(),
        })
    }
}

pub struct BrowserScreenshotTool {
    manifest: ToolManifest,
    app_state: Arc<crate::app_state::AppState>,
}

impl BrowserScreenshotTool {
    pub fn new(app_state: Arc<crate::app_state::AppState>) -> Self {
        Self {
            manifest: ToolManifest {
                name: "browser.screenshot".to_string(),
                description: "Captures a PNG screenshot of the current page and stores it as a content-addressed artifact.".to_string(),
                input_schema: json!({
                    "type": "object",
                    "properties": {
                        "space_id": {
                            "type": "string",
                            "description": "Optional space ID, defaults to project primary space."
                        }
                    }
                }),
                required_capabilities: vec![Capability::BrowserInteract],
                effect_class: EffectClass::ReadOnly,
                baseline_risk: RiskLevel::Safe,
                source: ToolSource::Native,
                output_schema: None,
                verification_command: None,
            },
            app_state,
        }
    }
}

#[async_trait]
impl Tool for BrowserScreenshotTool {
    fn manifest(&self) -> &ToolManifest {
        &self.manifest
    }

    async fn execute(&self, request: &ToolRequest) -> Result<ToolResult, Trans4mersError> {
        let space_id = request
            .arguments
            .get("space_id")
            .and_then(|v| v.as_str())
            .unwrap_or("bspace_default");

        let bytes = crate::browser_space_manager::BrowserSpaceManager::screenshot(
            self.app_state.clone(),
            &request.project_id,
            space_id,
        )
        .await?;

        let hash = trans4mers_storage::filesystem::sha256_hex(&bytes);
        let filename = format!("screenshot_{}.png", &hash[..12]);
        let artifact_path = request.workspace_root.join("artifacts").join(&filename);

        if let Some(parent) = artifact_path.parent() {
            let _ = tokio::fs::create_dir_all(parent).await;
        }
        tokio::fs::write(&artifact_path, &bytes)
            .await
            .map_err(|e| Trans4mersError::Internal(format!("Failed to save screenshot: {}", e)))?;

        let rel_path = format!("artifacts/{}", filename);
        Ok(ToolResult {
            success: true,
            content: format!(
                "Screenshot captured ({} bytes) and stored at {}",
                bytes.len(),
                rel_path
            ),
            error: None,
            artifacts: vec![rel_path],
            metadata: std::collections::HashMap::new(),
        })
    }
}

pub struct GitStatusTool {
    manifest: ToolManifest,
}

impl Default for GitStatusTool {
    fn default() -> Self {
        Self::new()
    }
}

impl GitStatusTool {
    pub fn new() -> Self {
        Self {
            manifest: ToolManifest {
                name: "git.status".to_string(),
                description: "Inspects the Git status of the project workspace, showing modified, untracked, and staged files.".to_string(),
                input_schema: json!({
                    "type": "object",
                    "properties": {}
                }),
                required_capabilities: vec![Capability::GitRead],
                effect_class: EffectClass::ReadOnly,
                baseline_risk: RiskLevel::Safe,
                source: ToolSource::Native,
                output_schema: None,
                verification_command: None,
            },
        }
    }
}

#[async_trait]
impl Tool for GitStatusTool {
    fn manifest(&self) -> &ToolManifest {
        &self.manifest
    }

    async fn execute(&self, request: &ToolRequest) -> Result<ToolResult, Trans4mersError> {
        let repo_root = &request.workspace_root;
        let repo = match git2::Repository::open(repo_root) {
            Ok(r) => r,
            Err(e) => {
                return Ok(ToolResult {
                    success: false,
                    content: String::new(),
                    error: Some(format!(
                        "Not a git repository at {}: {}",
                        repo_root.display(),
                        e
                    )),
                    artifacts: vec![],
                    metadata: std::collections::HashMap::new(),
                });
            }
        };

        let mut opts = git2::StatusOptions::new();
        opts.include_untracked(true);
        opts.recurse_untracked_dirs(true);

        let statuses = repo.statuses(Some(&mut opts)).map_err(|e| {
            Trans4mersError::Internal(format!("Failed to retrieve git status: {}", e))
        })?;

        let mut output = String::new();
        for entry in statuses.iter() {
            let path = entry.path().unwrap_or("unknown");
            let s = entry.status();
            let mut flags = Vec::new();
            if s.is_index_new() {
                flags.push("staged-new");
            }
            if s.is_index_modified() {
                flags.push("staged-modified");
            }
            if s.is_index_deleted() {
                flags.push("staged-deleted");
            }
            if s.is_wt_new() {
                flags.push("untracked");
            }
            if s.is_wt_modified() {
                flags.push("modified");
            }
            if s.is_wt_deleted() {
                flags.push("deleted");
            }
            output.push_str(&format!("{:20} {}\n", flags.join("|"), path));
        }

        if output.is_empty() {
            output = "Working tree clean. No changes.".to_string();
        }

        Ok(ToolResult {
            success: true,
            content: output,
            error: None,
            artifacts: vec![],
            metadata: std::collections::HashMap::new(),
        })
    }
}

pub struct GitDiffTool {
    manifest: ToolManifest,
}

impl Default for GitDiffTool {
    fn default() -> Self {
        Self::new()
    }
}

impl GitDiffTool {
    pub fn new() -> Self {
        Self {
            manifest: ToolManifest {
                name: "git.diff".to_string(),
                description: "Shows uncommitted changes in the repository working tree."
                    .to_string(),
                input_schema: json!({
                    "type": "object",
                    "properties": {
                        "path": {
                            "type": "string",
                            "description": "Optional specific file path to diff."
                        }
                    }
                }),
                required_capabilities: vec![Capability::GitRead],
                effect_class: EffectClass::ReadOnly,
                baseline_risk: RiskLevel::Safe,
                source: ToolSource::Native,
                output_schema: None,
                verification_command: None,
            },
        }
    }
}

#[async_trait]
impl Tool for GitDiffTool {
    fn manifest(&self) -> &ToolManifest {
        &self.manifest
    }

    async fn execute(&self, request: &ToolRequest) -> Result<ToolResult, Trans4mersError> {
        let repo_root = &request.workspace_root;
        let repo = match git2::Repository::open(repo_root) {
            Ok(r) => r,
            Err(e) => {
                return Ok(ToolResult {
                    success: false,
                    content: String::new(),
                    error: Some(format!(
                        "Not a git repository at {}: {}",
                        repo_root.display(),
                        e
                    )),
                    artifacts: vec![],
                    metadata: std::collections::HashMap::new(),
                });
            }
        };

        let mut diff_opts = git2::DiffOptions::new();
        if let Some(p) = request.arguments.get("path").and_then(|v| v.as_str()) {
            diff_opts.pathspec(p);
        }

        let diff = repo
            .diff_index_to_workdir(None, Some(&mut diff_opts))
            .map_err(|e| Trans4mersError::Internal(format!("Failed to compute git diff: {}", e)))?;

        let mut diff_text = String::new();
        diff.print(git2::DiffFormat::Patch, |_delta, _hunk, line| {
            let prefix = match line.origin() {
                '+' => "+",
                '-' => "-",
                ' ' => " ",
                _ => "",
            };
            if let Ok(s) = std::str::from_utf8(line.content()) {
                diff_text.push_str(prefix);
                diff_text.push_str(s);
            }
            true
        })
        .map_err(|e| Trans4mersError::Internal(format!("Failed to format git diff: {}", e)))?;

        if diff_text.is_empty() {
            diff_text = "No diff. Working directory matches index.".to_string();
        }

        Ok(ToolResult {
            success: true,
            content: diff_text,
            error: None,
            artifacts: vec![],
            metadata: std::collections::HashMap::new(),
        })
    }
}

// ---------------------------------------------------------------------------
// Cognitive Memory Upgrade Native Tools (WS-4)
// ---------------------------------------------------------------------------

pub struct MemoryInsertTool {
    manifest: ToolManifest,
    app_state: Arc<crate::app_state::AppState>,
}

impl MemoryInsertTool {
    pub fn new(app_state: Arc<crate::app_state::AppState>) -> Self {
        Self {
            manifest: ToolManifest {
                name: "memory.insert".to_string(),
                description: "Inserts an observation or fact into the project memory system with temporal validity and content deduplication.".to_string(),
                input_schema: json!({
                    "type": "object",
                    "properties": {
                        "content": {
                            "type": "string",
                            "description": "The observation, fact, or learned preference to store."
                        },
                        "tier": {
                            "type": "string",
                            "enum": ["Working", "Episodic", "Semantic", "Procedural"],
                            "description": "Memory tier. Defaults to 'Working'."
                        },
                        "importance": {
                            "type": "number",
                            "minimum": 0.0,
                            "maximum": 1.0,
                            "description": "Importance score from 0.0 to 1.0. Defaults to 0.7."
                        },
                        "scope": {
                            "type": "string",
                            "enum": ["Project", "Conversation", "Agent"],
                            "description": "Visibility scope. Defaults to 'Project'."
                        }
                    },
                    "required": ["content"]
                }),
                required_capabilities: vec![Capability::MemoryWrite],
                effect_class: EffectClass::NonIdempotentMutation,
                baseline_risk: RiskLevel::Medium,
                source: ToolSource::Native,
                output_schema: None,
                verification_command: None,
            },
            app_state,
        }
    }
}

#[async_trait]
impl Tool for MemoryInsertTool {
    fn manifest(&self) -> &ToolManifest {
        &self.manifest
    }

    async fn execute(&self, request: &ToolRequest) -> Result<ToolResult, Trans4mersError> {
        let content = request
            .arguments
            .get("content")
            .and_then(|v| v.as_str())
            .ok_or_else(|| Trans4mersError::Internal("Missing 'content' argument".to_string()))?;

        let tier_str = request
            .arguments
            .get("tier")
            .and_then(|v| v.as_str())
            .unwrap_or("Working");
        let tier = std::str::FromStr::from_str(tier_str)
            .unwrap_or(trans4mers_domain::memory::MemoryTier::Working);

        let importance = request
            .arguments
            .get("importance")
            .and_then(|v| v.as_f64())
            .unwrap_or(0.7) as f32;

        let scope_str = request
            .arguments
            .get("scope")
            .and_then(|v| v.as_str())
            .unwrap_or("Project");
        let scope = match scope_str {
            "Conversation" => trans4mers_domain::memory::MemoryScope::Conversation,
            "Agent" => trans4mers_domain::memory::MemoryScope::Agent,
            _ => trans4mers_domain::memory::MemoryScope::Project,
        };

        let pid = &request.project_id;
        let db = self.app_state.get_project_db(pid).ok_or_else(|| {
            Trans4mersError::Database(format!("Project DB not found for '{}'", pid))
        })?;

        let hash = crate::memory_engine::compute_memory_content_hash(content);

        // Deduplication check: if active memory with same content hash exists, reinforce it
        let existing = db.with_read_conn(|conn| {
            trans4mers_storage::repos::memory_repo::find_by_content_hash(
                conn,
                &pid.to_string(),
                &hash,
            )
        })?;

        if let Some(existing_mem) = existing {
            db.with_write_tx(|conn| {
                trans4mers_storage::repos::memory_repo::reinforce_memory(
                    conn,
                    &existing_mem.id.to_string(),
                    0.05,
                )
            })?;
            return Ok(ToolResult {
                success: true,
                content: format!(
                    "Existing memory '{}' matched content hash. Reinforced retrieval count to {}.",
                    existing_mem.id,
                    existing_mem.retrieval_count + 1
                ),
                error: None,
                artifacts: vec![],
                metadata: std::collections::HashMap::new(),
            });
        }

        let now = chrono::Utc::now();
        let mem_id = trans4mers_domain::ids::MemoryId::new();
        let mem = trans4mers_domain::memory::Memory {
            id: mem_id,
            project_id: *pid,
            conversation_id: None,
            agent_instance_id: None,
            scope,
            lifecycle: trans4mers_domain::memory::MemoryLifecycle::Persisted,
            content: content.to_string(),
            embedding: None,
            importance,
            confidence: 0.9,
            provenance: trans4mers_domain::memory::MemoryProvenance {
                source_event_id: None,
                source_message_id: None,
                source_agent_id: None,
                creation_reason: "Direct memory insert via tool".to_string(),
            },
            depth_level: 0,
            tier,
            retrieval_count: 0,
            last_retrieved_at: None,
            expires_at: None,
            valid_from: Some(now),
            valid_until: None,
            visibility_overrides: None,
            created_at: now,
            updated_at: now,
            valid_at: Some(now),
            invalid_at: None,
            replaced_by: None,
            content_hash: Some(hash),
        };

        let memory_engine = crate::memory_engine::MemoryEngine::new(self.app_state.clone());
        memory_engine.store_memory(mem)?;

        Ok(ToolResult {
            success: true,
            content: format!("Stored new memory '{}' in tier '{}'.", mem_id, tier),
            error: None,
            artifacts: vec![],
            metadata: std::collections::HashMap::new(),
        })
    }
}

pub struct MemoryReplaceTool {
    manifest: ToolManifest,
    app_state: Arc<crate::app_state::AppState>,
}

impl MemoryReplaceTool {
    pub fn new(app_state: Arc<crate::app_state::AppState>) -> Self {
        Self {
            manifest: ToolManifest {
                name: "memory.replace".to_string(),
                description: "Non-destructively invalidates an outdated memory and replaces it with a revised fact, preserving temporal history.".to_string(),
                input_schema: json!({
                    "type": "object",
                    "properties": {
                        "old_memory_id": {
                            "type": "string",
                            "description": "ID of the existing memory to supersede."
                        },
                        "new_content": {
                            "type": "string",
                            "description": "The revised observation or fact."
                        },
                        "tier": {
                            "type": "string",
                            "enum": ["Working", "Episodic", "Semantic", "Procedural"],
                            "description": "Optional tier for new memory (defaults to prior memory's tier)."
                        },
                        "importance": {
                            "type": "number",
                            "minimum": 0.0,
                            "maximum": 1.0,
                            "description": "Optional importance for new memory (defaults to prior memory's importance)."
                        }
                    },
                    "required": ["old_memory_id", "new_content"]
                }),
                required_capabilities: vec![Capability::MemoryWrite],
                effect_class: EffectClass::NonIdempotentMutation,
                baseline_risk: RiskLevel::Medium,
                source: ToolSource::Native,
                output_schema: None,
                verification_command: None,
            },
            app_state,
        }
    }
}

#[async_trait]
impl Tool for MemoryReplaceTool {
    fn manifest(&self) -> &ToolManifest {
        &self.manifest
    }

    async fn execute(&self, request: &ToolRequest) -> Result<ToolResult, Trans4mersError> {
        let old_id_str = request
            .arguments
            .get("old_memory_id")
            .and_then(|v| v.as_str())
            .ok_or_else(|| {
                Trans4mersError::Internal("Missing 'old_memory_id' argument".to_string())
            })?;

        let new_content = request
            .arguments
            .get("new_content")
            .and_then(|v| v.as_str())
            .ok_or_else(|| {
                Trans4mersError::Internal("Missing 'new_content' argument".to_string())
            })?;

        let pid = &request.project_id;
        let db = self.app_state.get_project_db(pid).ok_or_else(|| {
            Trans4mersError::Database(format!("Project DB not found for '{}'", pid))
        })?;

        let prior = db
            .with_read_conn(|conn| {
                trans4mers_storage::repos::memory_repo::get_project_memory(conn, old_id_str)
            })?
            .ok_or_else(|| {
                Trans4mersError::Internal(format!("Memory '{}' not found", old_id_str))
            })?;

        let tier = request
            .arguments
            .get("tier")
            .and_then(|v| v.as_str())
            .and_then(|s| std::str::FromStr::from_str(s).ok())
            .unwrap_or(prior.tier);

        let importance = request
            .arguments
            .get("importance")
            .and_then(|v| v.as_f64())
            .map(|f| f as f32)
            .unwrap_or(prior.importance);

        let now = chrono::Utc::now();
        let new_id = trans4mers_domain::ids::MemoryId::new();
        let hash = crate::memory_engine::compute_memory_content_hash(new_content);

        // 1. Invalidate prior memory non-destructively
        db.with_write_tx(|conn| {
            trans4mers_storage::repos::memory_repo::invalidate_memory(
                conn,
                old_id_str,
                Some(&new_id.to_string()),
                Some(now),
            )
        })?;

        // 2. Insert new superseding memory
        let new_mem = trans4mers_domain::memory::Memory {
            id: new_id,
            project_id: *pid,
            conversation_id: prior.conversation_id,
            agent_instance_id: prior.agent_instance_id,
            scope: prior.scope,
            lifecycle: trans4mers_domain::memory::MemoryLifecycle::Persisted,
            content: new_content.to_string(),
            embedding: None,
            importance,
            confidence: 0.95,
            provenance: trans4mers_domain::memory::MemoryProvenance {
                source_event_id: None,
                source_message_id: None,
                source_agent_id: None,
                creation_reason: format!("Replaced memory '{}'", old_id_str),
            },
            depth_level: prior.depth_level + 1,
            tier,
            retrieval_count: 0,
            last_retrieved_at: None,
            expires_at: None,
            valid_from: Some(now),
            valid_until: None,
            visibility_overrides: None,
            created_at: now,
            updated_at: now,
            valid_at: Some(now),
            invalid_at: None,
            replaced_by: None,
            content_hash: Some(hash),
        };

        let memory_engine = crate::memory_engine::MemoryEngine::new(self.app_state.clone());
        memory_engine.store_memory(new_mem)?;

        Ok(ToolResult {
            success: true,
            content: format!(
                "Successfully replaced memory '{}' with new memory '{}' (temporal validity updated).",
                old_id_str, new_id
            ),
            error: None,
            artifacts: vec![],
            metadata: std::collections::HashMap::new(),
        })
    }
}

pub struct MemorySearchTool {
    manifest: ToolManifest,
    app_state: Arc<crate::app_state::AppState>,
}

impl MemorySearchTool {
    pub fn new(app_state: Arc<crate::app_state::AppState>) -> Self {
        Self {
            manifest: ToolManifest {
                name: "memory.search".to_string(),
                description: "Searches project memories with hybrid BM25 + vector search and optional temporal window inspection.".to_string(),
                input_schema: json!({
                    "type": "object",
                    "properties": {
                        "query": {
                            "type": "string",
                            "description": "Natural language search query."
                        },
                        "limit": {
                            "type": "integer",
                            "minimum": 1,
                            "maximum": 50,
                            "description": "Maximum number of memories to return (defaults to 10)."
                        },
                        "temporal": {
                            "type": "boolean",
                            "description": "If true, includes historically invalidated memories along with validity windows."
                        }
                    },
                    "required": ["query"]
                }),
                required_capabilities: vec![Capability::MemoryRead],
                effect_class: EffectClass::ReadOnly,
                baseline_risk: RiskLevel::Safe,
                source: ToolSource::Native,
                output_schema: None,
                verification_command: None,
            },
            app_state,
        }
    }
}

#[async_trait]
impl Tool for MemorySearchTool {
    fn manifest(&self) -> &ToolManifest {
        &self.manifest
    }

    async fn execute(&self, request: &ToolRequest) -> Result<ToolResult, Trans4mersError> {
        let query = request
            .arguments
            .get("query")
            .and_then(|v| v.as_str())
            .ok_or_else(|| Trans4mersError::Internal("Missing 'query' argument".to_string()))?;

        let limit = request
            .arguments
            .get("limit")
            .and_then(|v| v.as_u64())
            .unwrap_or(10) as usize;

        let temporal = request
            .arguments
            .get("temporal")
            .and_then(|v| v.as_bool())
            .unwrap_or(false);

        let pid = &request.project_id;
        let memory_engine = crate::memory_engine::MemoryEngine::new(self.app_state.clone());
        let results = memory_engine.retrieve_memories_opts(
            pid,
            &request.requesting_agent_id,
            crate::memory_engine::MemoryRetrievalOptions {
                conversation_id: None,
                query_text: Some(query),
                query_embedding: None,
                limit: limit as u32,
                temporal,
            },
        )?;

        if results.is_empty() {
            return Ok(ToolResult {
                success: true,
                content: "No matching memories found.".to_string(),
                error: None,
                artifacts: vec![],
                metadata: std::collections::HashMap::new(),
            });
        }

        let mut out = format!("Found {} memories:\n\n", results.len());
        for (i, m) in results.iter().enumerate() {
            let status = if let Some(inv) = m.invalid_at {
                format!(
                    "INVALIDATED (at {}, replaced_by: {:?})",
                    inv.to_rfc3339(),
                    m.replaced_by
                )
            } else {
                "ACTIVE".to_string()
            };
            out.push_str(&format!(
                "{}. [{}] [Tier: {}] [Status: {}]\n   Content: {}\n   Valid: {} .. {}\n\n",
                i + 1,
                m.id,
                m.tier,
                status,
                m.content,
                m.valid_at
                    .map(|d| d.to_rfc3339())
                    .unwrap_or_else(|| "none".to_string()),
                m.invalid_at
                    .map(|d| d.to_rfc3339())
                    .unwrap_or_else(|| "open".to_string()),
            ));
        }

        Ok(ToolResult {
            success: true,
            content: out,
            error: None,
            artifacts: vec![],
            metadata: std::collections::HashMap::new(),
        })
    }
}

pub struct MemoryArchiveTool {
    manifest: ToolManifest,
    app_state: Arc<crate::app_state::AppState>,
}

impl MemoryArchiveTool {
    pub fn new(app_state: Arc<crate::app_state::AppState>) -> Self {
        Self {
            manifest: ToolManifest {
                name: "memory.archive".to_string(),
                description: "Explicitly archives an obsolete memory by setting invalid_at to current timestamp, preserving provenance without replacement.".to_string(),
                input_schema: json!({
                    "type": "object",
                    "properties": {
                        "memory_id": {
                            "type": "string",
                            "description": "ID of the memory to archive."
                        },
                        "reason": {
                            "type": "string",
                            "description": "Optional reason for archiving."
                        }
                    },
                    "required": ["memory_id"]
                }),
                required_capabilities: vec![Capability::MemoryWrite],
                effect_class: EffectClass::NonIdempotentMutation,
                baseline_risk: RiskLevel::Medium,
                source: ToolSource::Native,
                output_schema: None,
                verification_command: None,
            },
            app_state,
        }
    }
}

#[async_trait]
impl Tool for MemoryArchiveTool {
    fn manifest(&self) -> &ToolManifest {
        &self.manifest
    }

    async fn execute(&self, request: &ToolRequest) -> Result<ToolResult, Trans4mersError> {
        let mem_id = request
            .arguments
            .get("memory_id")
            .and_then(|v| v.as_str())
            .ok_or_else(|| Trans4mersError::Internal("Missing 'memory_id' argument".to_string()))?;

        let pid = &request.project_id;
        let db = self.app_state.get_project_db(pid).ok_or_else(|| {
            Trans4mersError::Database(format!("Project DB not found for '{}'", pid))
        })?;

        db.with_write_tx(|conn| {
            trans4mers_storage::repos::memory_repo::invalidate_memory(
                conn,
                mem_id,
                None,
                Some(chrono::Utc::now()),
            )
        })?;

        Ok(ToolResult {
            success: true,
            content: format!("Memory '{}' successfully archived.", mem_id),
            error: None,
            artifacts: vec![],
            metadata: std::collections::HashMap::new(),
        })
    }
}
