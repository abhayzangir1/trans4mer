use async_trait::async_trait;
use dashmap::DashMap;
use futures::StreamExt;
use std::collections::HashMap;
use std::process::Stdio;
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::process::Command;
use tokio::sync::{RwLock, mpsc, oneshot};
use tracing::{error, info, warn};
use url::Url;

use trans4mers_domain::error::Trans4mersError;
use trans4mers_domain::event::{McpTrafficDirection, McpTrafficFrame};
use trans4mers_domain::tool::{
    EffectClass, RiskLevel, Tool, ToolManifest, ToolRequest, ToolResult, ToolSource,
};

pub type McpTrafficSink = Arc<dyn Fn(McpTrafficFrame) + Send + Sync>;

/// Trait abstracting the underlying MCP transport mechanism (stdio child process vs remote streamable HTTP).
#[async_trait]
pub trait McpTransport: Send + Sync {
    /// Dispatches a JSON-RPC request. Returns `Ok(Some(body))` if an immediate HTTP response was received,
    /// or `Ok(None)` if the response will be streamed asynchronously over stdout / SSE.
    async fn send_request(
        &self,
        req_id: u64,
        payload: serde_json::Value,
    ) -> Result<Option<serde_json::Value>, Trans4mersError>;

    /// Human-readable transport identifier ("stdio" or "http/sse").
    fn transport_type(&self) -> &'static str;

    /// Gracefully closes background listener tasks and child processes.
    async fn close(&self) -> Result<(), Trans4mersError>;
}

// ---------------------------------------------------------------------------
// STDIO TRANSPORT
// ---------------------------------------------------------------------------

pub struct StdioTransport {
    request_tx: mpsc::Sender<serde_json::Value>,
    abort_handles: Vec<tokio::task::AbortHandle>,
}

impl StdioTransport {
    pub fn spawn(
        cmd_line: &str,
        pending_requests: Arc<DashMap<u64, oneshot::Sender<serde_json::Value>>>,
    ) -> Result<Self, Trans4mersError> {
        Self::spawn_with_env(cmd_line, None, pending_requests)
    }

    pub fn spawn_with_env(
        cmd_line: &str,
        env: Option<&HashMap<String, String>>,
        pending_requests: Arc<DashMap<u64, oneshot::Sender<serde_json::Value>>>,
    ) -> Result<Self, Trans4mersError> {
        let parts: Vec<&str> = cmd_line.split_whitespace().collect();
        if parts.is_empty() {
            return Err(Trans4mersError::Internal(
                "Empty MCP command line".to_string(),
            ));
        }

        let server_cmd = parts[0].to_string();
        let args: Vec<String> = parts[1..].iter().map(|s| s.to_string()).collect();

        let mut cmd = Command::new(&server_cmd);
        cmd.args(&args)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::inherit());

        if let Some(env_map) = env {
            for (k, v) in env_map {
                cmd.env(k, v);
            }
        }

        let mut child = cmd.spawn().map_err(|e| {
            Trans4mersError::Internal(format!(
                "Failed to spawn MCP process '{}': {}",
                server_cmd, e
            ))
        })?;

        let mut stdin = child
            .stdin
            .take()
            .ok_or_else(|| Trans4mersError::Internal("Failed to open child stdin".to_string()))?;
        let stdout = child
            .stdout
            .take()
            .ok_or_else(|| Trans4mersError::Internal("Failed to open child stdout".to_string()))?;

        let (tx, mut rx) = mpsc::channel::<serde_json::Value>(128);

        // Writer task
        let writer_handle = tokio::spawn(async move {
            while let Some(req) = rx.recv().await {
                if let Ok(req_bytes) = serde_json::to_vec(&req) {
                    if stdin.write_all(&req_bytes).await.is_err() {
                        break;
                    }
                    if stdin.write_all(b"\n").await.is_err() {
                        break;
                    }
                }
            }
            let _ = child.kill().await;
        })
        .abort_handle();

        // Reader task
        let reader_pending = pending_requests.clone();
        let reader_handle = tokio::spawn(async move {
            let mut reader = BufReader::new(stdout).lines();
            while let Ok(Some(line)) = reader.next_line().await {
                if let Ok(resp) = serde_json::from_str::<serde_json::Value>(&line) {
                    let maybe_id = resp.get("id").and_then(|v| v.as_u64());
                    if let Some((_, sender)) = maybe_id.and_then(|id| reader_pending.remove(&id)) {
                        let _ = sender.send(resp);
                    }
                }
            }
        })
        .abort_handle();

        Ok(Self {
            request_tx: tx,
            abort_handles: vec![writer_handle, reader_handle],
        })
    }
}

#[async_trait]
impl McpTransport for StdioTransport {
    async fn send_request(
        &self,
        _req_id: u64,
        payload: serde_json::Value,
    ) -> Result<Option<serde_json::Value>, Trans4mersError> {
        self.request_tx.send(payload).await.map_err(|_| {
            Trans4mersError::Internal("MCP stdio background task terminated".to_string())
        })?;
        Ok(None)
    }

    fn transport_type(&self) -> &'static str {
        "stdio"
    }

    async fn close(&self) -> Result<(), Trans4mersError> {
        for handle in &self.abort_handles {
            handle.abort();
        }
        Ok(())
    }
}

impl Drop for StdioTransport {
    fn drop(&mut self) {
        for handle in &self.abort_handles {
            handle.abort();
        }
    }
}

// ---------------------------------------------------------------------------
// STREAMABLE HTTP (SSE + POST) TRANSPORT
// ---------------------------------------------------------------------------

pub struct StreamableHttpTransport {
    base_url: Url,
    post_endpoint: Arc<RwLock<Url>>,
    http_client: reqwest::Client,
    auth_token: Option<String>,
    custom_headers: HashMap<String, String>,
    abort_handle: tokio::task::AbortHandle,
}

impl StreamableHttpTransport {
    pub async fn connect(
        url_str: &str,
        auth_token: Option<String>,
        custom_headers: HashMap<String, String>,
        pending_requests: Arc<DashMap<u64, oneshot::Sender<serde_json::Value>>>,
    ) -> Result<Self, Trans4mersError> {
        let base_url = Url::parse(url_str).map_err(|e| {
            Trans4mersError::Internal(format!("Invalid remote MCP URL '{}': {}", url_str, e))
        })?;

        let scheme = base_url.scheme();
        if scheme != "http" && scheme != "https" {
            return Err(Trans4mersError::Internal(format!(
                "Unsupported remote MCP URL scheme '{}'. Must be http or https.",
                scheme
            )));
        }

        let http_client = reqwest::Client::builder()
            .connect_timeout(std::time::Duration::from_secs(10))
            .build()
            .map_err(|e| {
                Trans4mersError::Internal(format!("Failed to build HTTP client: {}", e))
            })?;

        // Verify initial SSE connectivity to fail clean if unreachable
        let mut test_req = http_client
            .get(base_url.as_str())
            .header(reqwest::header::ACCEPT, "text/event-stream");
        if let Some(token) = &auth_token {
            test_req = test_req.header(
                reqwest::header::AUTHORIZATION,
                format!("Bearer {}", token.trim()),
            );
        }
        for (k, v) in &custom_headers {
            test_req = test_req.header(k, v);
        }

        let initial_resp = test_req.send().await.map_err(|e| {
            Trans4mersError::Internal(format!(
                "Failed to connect to remote MCP server at '{}': {}",
                base_url, e
            ))
        })?;

        if !initial_resp.status().is_success() {
            let status = initial_resp.status();
            let body = initial_resp.text().await.unwrap_or_default();
            return Err(Trans4mersError::Internal(format!(
                "Remote MCP server returned error status {}: {}",
                status, body
            )));
        }

        let post_endpoint = Arc::new(RwLock::new(base_url.clone()));
        let post_endpoint_writer = post_endpoint.clone();
        let base_url_clone = base_url.clone();
        let auth_token_clone = auth_token.clone();
        let custom_headers_clone = custom_headers.clone();
        let http_client_clone = http_client.clone();
        let pending_requests_clone = pending_requests.clone();

        // Spawn background SSE event listener task with exponential backoff reconnection
        let abort_handle = tokio::spawn(async move {
            let mut retry_count = 0u32;
            let mut active_response = Some(initial_resp);

            loop {
                let resp = match active_response.take() {
                    Some(r) => r,
                    None => {
                        // Reconnect attempt
                        let backoff_secs = match retry_count {
                            0 => 1,
                            1 => 2,
                            _ => 4,
                        };
                        warn!(
                            url = %base_url_clone,
                            attempt = retry_count + 1,
                            backoff_secs = backoff_secs,
                            "Remote MCP SSE disconnected; attempting reconnection"
                        );
                        tokio::time::sleep(std::time::Duration::from_secs(backoff_secs)).await;

                        let mut req = http_client_clone
                            .get(base_url_clone.as_str())
                            .header(reqwest::header::ACCEPT, "text/event-stream");
                        if let Some(token) = &auth_token_clone {
                            req = req.header(
                                reqwest::header::AUTHORIZATION,
                                format!("Bearer {}", token.trim()),
                            );
                        }
                        for (k, v) in &custom_headers_clone {
                            req = req.header(k, v);
                        }

                        match req.send().await {
                            Ok(r) if r.status().is_success() => {
                                info!(url = %base_url_clone, "Remote MCP SSE reconnected successfully");
                                retry_count = 0;
                                r
                            }
                            Ok(r) => {
                                warn!(url = %base_url_clone, status = %r.status(), "Remote MCP SSE reconnect returned error");
                                retry_count += 1;
                                if retry_count >= 3 {
                                    error!(url = %base_url_clone, "Max reconnect attempts (3) exceeded for remote MCP server");
                                    break;
                                }
                                continue;
                            }
                            Err(e) => {
                                warn!(url = %base_url_clone, error = %e, "Remote MCP SSE reconnect failed");
                                retry_count += 1;
                                if retry_count >= 3 {
                                    error!(url = %base_url_clone, "Max reconnect attempts (3) exceeded for remote MCP server");
                                    break;
                                }
                                continue;
                            }
                        }
                    }
                };

                // Stream and parse incoming SSE chunks
                let mut stream = resp.bytes_stream();
                let mut buffer = String::new();

                while let Some(chunk_res) = stream.next().await {
                    match chunk_res {
                        Ok(chunk) => {
                            if let Ok(text) = std::str::from_utf8(&chunk) {
                                buffer.push_str(text);

                                while let Some((pos, delim_len)) = buffer
                                    .find("\r\n\r\n")
                                    .map(|pos| (pos, 4))
                                    .or_else(|| buffer.find("\n\n").map(|pos| (pos, 2)))
                                {
                                    let frame = buffer[..pos].to_string();
                                    buffer = buffer[pos + delim_len..].to_string();

                                    Self::process_sse_frame(
                                        &frame,
                                        &base_url_clone,
                                        &post_endpoint_writer,
                                        &pending_requests_clone,
                                    )
                                    .await;
                                }
                            }
                        }
                        Err(e) => {
                            warn!(url = %base_url_clone, error = %e, "Error reading remote MCP SSE stream");
                            break;
                        }
                    }
                }
            }
        })
        .abort_handle();

        Ok(Self {
            base_url,
            post_endpoint,
            http_client,
            auth_token,
            custom_headers,
            abort_handle,
        })
    }

    async fn process_sse_frame(
        frame: &str,
        base_url: &Url,
        post_endpoint: &Arc<RwLock<Url>>,
        pending_requests: &Arc<DashMap<u64, oneshot::Sender<serde_json::Value>>>,
    ) {
        let mut event_type = String::new();
        let mut data_lines = Vec::new();

        for raw_line in frame.lines() {
            let line = raw_line.trim_end_matches('\r');
            if line.is_empty() || line.starts_with(':') {
                // Comment / keepalive line
                continue;
            }

            if let Some(rest) = line.strip_prefix("event:") {
                event_type = rest.trim().to_string();
            } else if let Some(rest) = line.strip_prefix("data:") {
                data_lines.push(rest.trim().to_string());
            }
        }

        let combined_data = data_lines.join("\n");

        if event_type == "endpoint" {
            // MCP 2024-11-05 spec: Endpoint event indicates the URL where client POST requests should be sent
            let target_str = combined_data.trim();
            if let Ok(resolved_url) = base_url.join(target_str) {
                // Security invariant: same-origin check to prevent off-host SSRF
                if resolved_url.origin() == base_url.origin() {
                    info!(
                        base_url = %base_url,
                        new_endpoint = %resolved_url,
                        "Updated remote MCP JSON-RPC POST endpoint"
                    );
                    *post_endpoint.write().await = resolved_url;
                } else {
                    warn!(
                        base_origin = %base_url.origin().ascii_serialization(),
                        redirect_origin = %resolved_url.origin().ascii_serialization(),
                        "Rejected off-origin MCP endpoint redirect"
                    );
                }
            }
        } else if (event_type == "message" || event_type.is_empty()) && !combined_data.is_empty() {
            // Standard JSON-RPC response or notification frame
            if let Ok(resp) = serde_json::from_str::<serde_json::Value>(&combined_data) {
                let maybe_id = resp.get("id").and_then(|v| v.as_u64());
                if let Some((_, sender)) = maybe_id.and_then(|id| pending_requests.remove(&id)) {
                    let _ = sender.send(resp);
                }
            }
        }
    }

    /// Returns the base URL configured for this remote MCP transport.
    pub fn base_url(&self) -> &Url {
        &self.base_url
    }
}

#[async_trait]
impl McpTransport for StreamableHttpTransport {
    async fn send_request(
        &self,
        _req_id: u64,
        payload: serde_json::Value,
    ) -> Result<Option<serde_json::Value>, Trans4mersError> {
        let endpoint_url = (*self.post_endpoint.read().await).clone();

        let mut req = self
            .http_client
            .post(endpoint_url.as_str())
            .header(reqwest::header::CONTENT_TYPE, "application/json");

        if let Some(token) = &self.auth_token {
            req = req.header(
                reqwest::header::AUTHORIZATION,
                format!("Bearer {}", token.trim()),
            );
        }
        for (k, v) in &self.custom_headers {
            req = req.header(k, v);
        }

        let resp = req
            .json(&payload)
            .send()
            .await
            .map_err(|e| Trans4mersError::Internal(format!("MCP HTTP POST failed: {}", e)))?;

        if resp.status().is_success() {
            let body_text = resp.text().await.unwrap_or_default();
            if let Ok(json_body) = serde_json::from_str::<serde_json::Value>(&body_text)
                && json_body.is_object()
                && (json_body.get("result").is_some() || json_body.get("error").is_some())
            {
                // Server replied directly in HTTP response
                return Ok(Some(json_body));
            }
            // 202 Accepted or empty body: response will arrive via SSE message event
            Ok(None)
        } else {
            let status = resp.status();
            let err_text = resp.text().await.unwrap_or_default();
            Err(Trans4mersError::Internal(format!(
                "MCP server returned HTTP error status {}: {}",
                status, err_text
            )))
        }
    }

    fn transport_type(&self) -> &'static str {
        "http/sse"
    }

    async fn close(&self) -> Result<(), Trans4mersError> {
        self.abort_handle.abort();
        Ok(())
    }
}

impl Drop for StreamableHttpTransport {
    fn drop(&mut self) {
        self.abort_handle.abort();
    }
}

// ---------------------------------------------------------------------------
// UNIFIED MCP CLIENT & TOOL WRAPPER
// ---------------------------------------------------------------------------

pub struct McpClient {
    transport: Arc<dyn McpTransport>,
    pending_requests: Arc<DashMap<u64, oneshot::Sender<serde_json::Value>>>,
    next_id: AtomicU64,
    server_name: String,
    traffic_sink: Option<McpTrafficSink>,
}

impl McpClient {
    /// Connects to an MCP server with server name and optional traffic logging sink.
    pub async fn connect_named(
        server_name: &str,
        target: &str,
        auth_token: Option<String>,
        custom_headers: Option<HashMap<String, String>>,
        traffic_sink: Option<McpTrafficSink>,
    ) -> Result<Arc<Self>, Trans4mersError> {
        let trimmed = target.trim();
        if trimmed.is_empty() {
            return Err(Trans4mersError::Internal(
                "Empty MCP URL/Command".to_string(),
            ));
        }

        if let Ok(parsed_url) = Url::parse(trimmed) {
            let s = parsed_url.scheme();
            if s != "http" && s != "https" && s != "stdio" {
                return Err(Trans4mersError::Internal(format!(
                    "Unsupported remote MCP URL scheme '{}'. Must be http or https.",
                    s
                )));
            }
        }

        let pending_requests: Arc<DashMap<u64, oneshot::Sender<serde_json::Value>>> =
            Arc::new(DashMap::new());

        let transport: Arc<dyn McpTransport> =
            if trimmed.starts_with("http://") || trimmed.starts_with("https://") {
                Arc::new(
                    StreamableHttpTransport::connect(
                        trimmed,
                        auth_token,
                        custom_headers.unwrap_or_default(),
                        pending_requests.clone(),
                    )
                    .await?,
                )
            } else {
                let cmd_str = trimmed.strip_prefix("stdio://").unwrap_or(trimmed);
                let mut env = HashMap::new();
                if server_name == "github" || server_name.contains("github") {
                    let clean_tok = auth_token.as_deref().map(str::trim).unwrap_or("");
                    if !clean_tok.is_empty() {
                        env.insert(
                            "GITHUB_PERSONAL_ACCESS_TOKEN".to_string(),
                            clean_tok.to_string(),
                        );
                    }
                }
                Arc::new(StdioTransport::spawn_with_env(
                    cmd_str,
                    if env.is_empty() { None } else { Some(&env) },
                    pending_requests.clone(),
                )?)
            };

        Ok(Arc::new(Self {
            transport,
            pending_requests,
            next_id: AtomicU64::new(1),
            server_name: server_name.to_string(),
            traffic_sink,
        }))
    }

    /// Connects to an MCP server, auto-detecting transport from URL scheme (`http://` / `https://` -> SSE+POST, else stdio).
    pub async fn connect(url: &str) -> Result<Arc<Self>, Trans4mersError> {
        Self::connect_named("mcp", url, None, None, None).await
    }

    /// Connects to an MCP server with optional Bearer auth token (loaded from OS keyring) and custom headers.
    pub async fn connect_with_auth(
        target: &str,
        auth_token: Option<String>,
        custom_headers: Option<HashMap<String, String>>,
    ) -> Result<Arc<Self>, Trans4mersError> {
        Self::connect_named("mcp", target, auth_token, custom_headers, None).await
    }

    /// Returns the active transport identifier ("stdio" or "http/sse").
    pub fn transport_type(&self) -> &'static str {
        self.transport.transport_type()
    }

    /// Returns the configured server name.
    pub fn server_name(&self) -> &str {
        &self.server_name
    }

    /// Closes the transport and stops any background tasks.
    pub async fn close(&self) -> Result<(), Trans4mersError> {
        self.transport.close().await
    }

    /// Dispatches a JSON-RPC 2.0 call with guaranteed 30s timeout, automatic response resolution, and protocol traffic logging.
    pub async fn rpc_call(
        &self,
        method: &str,
        params: Option<serde_json::Value>,
    ) -> Result<serde_json::Value, Trans4mersError> {
        let req_id = self.next_id.fetch_add(1, Ordering::SeqCst);
        let req = serde_json::json!({
            "jsonrpc": "2.0",
            "id": req_id,
            "method": method,
            "params": params
        });

        if let Some(sink) = &self.traffic_sink {
            sink(McpTrafficFrame {
                id: format!("frame-{}-{}", chrono::Utc::now().timestamp_micros(), req_id),
                server_name: self.server_name.clone(),
                direction: McpTrafficDirection::Outbound,
                method: Some(method.to_string()),
                payload: req.to_string(),
                timestamp: chrono::Utc::now(),
            });
        }

        let (tx, rx) = oneshot::channel();
        self.pending_requests.insert(req_id, tx);

        let res = match self.transport.send_request(req_id, req).await {
            Ok(Some(direct_resp)) => {
                self.pending_requests.remove(&req_id);
                Ok(direct_resp)
            }
            Ok(None) => match tokio::time::timeout(std::time::Duration::from_secs(30), rx).await {
                Ok(Ok(val)) => Ok(val),
                Ok(Err(_)) => {
                    self.pending_requests.remove(&req_id);
                    Err(Trans4mersError::Internal(
                        "Failed to read MCP response".to_string(),
                    ))
                }
                Err(_) => {
                    self.pending_requests.remove(&req_id);
                    Err(Trans4mersError::Internal(
                        "MCP request timed out after 30s".to_string(),
                    ))
                }
            },
            Err(e) => {
                self.pending_requests.remove(&req_id);
                Err(e)
            }
        };

        if let Some(sink) = &self.traffic_sink {
            match &res {
                Ok(resp_val) => {
                    sink(McpTrafficFrame {
                        id: format!(
                            "frame-{}-{}-res",
                            chrono::Utc::now().timestamp_micros(),
                            req_id
                        ),
                        server_name: self.server_name.clone(),
                        direction: McpTrafficDirection::Inbound,
                        method: Some(method.to_string()),
                        payload: resp_val.to_string(),
                        timestamp: chrono::Utc::now(),
                    });
                }
                Err(err) => {
                    sink(McpTrafficFrame {
                        id: format!(
                            "frame-{}-{}-err",
                            chrono::Utc::now().timestamp_micros(),
                            req_id
                        ),
                        server_name: self.server_name.clone(),
                        direction: McpTrafficDirection::Inbound,
                        method: Some(method.to_string()),
                        payload: serde_json::json!({
                            "jsonrpc": "2.0",
                            "id": req_id,
                            "error": {
                                "code": -32603,
                                "message": err.to_string()
                            }
                        })
                        .to_string(),
                        timestamp: chrono::Utc::now(),
                    });
                }
            }
        }

        res
    }

    /// Performs standard MCP initialization handshake and logs protocol traffic.
    pub async fn initialize(&self) -> Result<serde_json::Value, Trans4mersError> {
        let init_params = serde_json::json!({
            "protocolVersion": "2024-11-05",
            "capabilities": {},
            "clientInfo": {
                "name": "trans4mers-sovereign-agent-os",
                "version": "0.1.0"
            }
        });
        let resp = self.rpc_call("initialize", Some(init_params)).await?;

        // Send notifications/initialized
        let notify_req = serde_json::json!({
            "jsonrpc": "2.0",
            "method": "notifications/initialized"
        });
        if let Some(sink) = &self.traffic_sink {
            sink(McpTrafficFrame {
                id: format!(
                    "frame-{}-init-notify",
                    chrono::Utc::now().timestamp_micros()
                ),
                server_name: self.server_name.clone(),
                direction: McpTrafficDirection::Outbound,
                method: Some("notifications/initialized".to_string()),
                payload: notify_req.to_string(),
                timestamp: chrono::Utc::now(),
            });
        }
        let _ = self.transport.send_request(0, notify_req).await;

        Ok(resp)
    }

    /// Performs ping call.
    pub async fn ping(&self) -> Result<(), Trans4mersError> {
        self.rpc_call("ping", None).await.map(|_| ())
    }

    pub async fn list_tools(self: &Arc<Self>) -> Result<Vec<Arc<dyn Tool>>, Trans4mersError> {
        let resp = self.rpc_call("tools/list", None).await?;

        let mut tools: Vec<Arc<dyn Tool>> = Vec::new();
        if let Some(tools_arr) = resp
            .get("result")
            .and_then(|r| r.get("tools"))
            .and_then(|t| t.as_array())
        {
            for t in tools_arr {
                let manifest = ToolManifest {
                    name: t
                        .get("name")
                        .and_then(|v| v.as_str())
                        .unwrap_or("unknown")
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
                    source: ToolSource::Mcp,
                    verification_command: None,
                };

                tools.push(Arc::new(McpToolWrapper {
                    client: self.clone(),
                    manifest,
                }));
            }
        }

        Ok(tools)
    }
}

pub struct McpToolWrapper {
    client: Arc<McpClient>,
    manifest: ToolManifest,
}

#[async_trait]
impl Tool for McpToolWrapper {
    fn manifest(&self) -> &ToolManifest {
        &self.manifest
    }

    async fn execute(&self, request: &ToolRequest) -> Result<ToolResult, Trans4mersError> {
        let params = serde_json::json!({
            "name": &self.manifest.name,
            "arguments": &request.arguments
        });

        let resp = self.client.rpc_call("tools/call", Some(params)).await?;

        let content = resp
            .get("result")
            .and_then(|r| r.get("content"))
            .map(|c| c.to_string())
            .unwrap_or_else(|| "No content".to_string());

        Ok(ToolResult {
            success: !resp.get("error").is_some(),
            content,
            error: resp.get("error").map(|e| e.to_string()),
            artifacts: vec![],
            metadata: HashMap::new(),
        })
    }
}
