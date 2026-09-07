use crate::app_state::AppState;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::sync::Arc;
use trans4mers_domain::error::Trans4mersError;
use trans4mers_domain::ids::ProjectId;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct McpRequest {
    pub jsonrpc: String,
    pub id: Option<Value>,
    pub method: String,
    pub params: Option<Value>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct McpResponse {
    pub jsonrpc: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub id: Option<Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub result: Option<Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<McpErrorDetail>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct McpErrorDetail {
    pub code: i32,
    pub message: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub data: Option<Value>,
}

pub struct McpServerBridge;

impl McpServerBridge {
    /// Handles incoming MCP JSON-RPC 2.0 requests
    pub async fn handle_request(
        app_state: Arc<AppState>,
        project_id: ProjectId,
        raw_json: &str,
    ) -> Result<String, Trans4mersError> {
        let req: McpRequest = serde_json::from_str(raw_json).map_err(|e| {
            Trans4mersError::Serialization(format!("Invalid JSON-RPC format: {}", e))
        })?;

        let id = req.id.clone();

        let res = match req.method.as_str() {
            "initialize" => Self::handle_initialize(),
            "notifications/initialized" => Ok(json!({})),
            "ping" => Ok(json!({})),
            "tools/list" => Self::handle_tools_list(),
            "tools/call" => Self::handle_tools_call(app_state, project_id, req.params).await,
            "resources/list" => Self::handle_resources_list(app_state, project_id),
            "resources/read" => Self::handle_resources_read(app_state, project_id, req.params),
            other => Err(McpErrorDetail {
                code: -32601,
                message: format!("Method not found: {}", other),
                data: None,
            }),
        };

        let response = match res {
            Ok(val) => McpResponse {
                jsonrpc: "2.0".to_string(),
                id,
                result: Some(val),
                error: None,
            },
            Err(err) => McpResponse {
                jsonrpc: "2.0".to_string(),
                id,
                result: None,
                error: Some(err),
            },
        };

        serde_json::to_string(&response).map_err(|e| Trans4mersError::Serialization(e.to_string()))
    }

    fn handle_initialize() -> Result<Value, McpErrorDetail> {
        Ok(json!({
            "protocolVersion": "2024-11-05",
            "capabilities": {
                "tools": {
                    "listChanged": false
                },
                "resources": {
                    "subscribe": false,
                    "listChanged": false
                }
            },
            "serverInfo": {
                "name": "trans4mers-sovereign-bridge",
                "version": "0.1.0"
            }
        }))
    }

    fn handle_tools_list() -> Result<Value, McpErrorDetail> {
        Ok(json!({
            "tools": [
                {
                    "name": "memory_search",
                    "description": "Search 4-tier memory substrate (Working, Episodic, Semantic, Procedural) for past facts and lessons.",
                    "inputSchema": {
                        "type": "object",
                        "properties": {
                            "query": { "type": "string", "description": "Semantic or keyword search query" },
                            "tier": { "type": "string", "description": "Optional tier filter (Working, Episodic, Semantic, Procedural)" }
                        },
                        "required": ["query"]
                    }
                },
                {
                    "name": "deep_research",
                    "description": "Trigger a 5-stage cited deep research workflow over workspace substrate and external references.",
                    "inputSchema": {
                        "type": "object",
                        "properties": {
                            "topic": { "type": "string", "description": "Research subject or query" }
                        },
                        "required": ["topic"]
                    }
                },
                {
                    "name": "browser_navigate",
                    "description": "Navigate an isolated browser space to a URL and capture real-time live mirror frame.",
                    "inputSchema": {
                        "type": "object",
                        "properties": {
                            "space_id": { "type": "string", "description": "Browser isolation space ID" },
                            "url": { "type": "string", "description": "Destination URL" }
                        },
                        "required": ["space_id", "url"]
                    }
                }
            ]
        }))
    }

    async fn handle_tools_call(
        app_state: Arc<AppState>,
        project_id: ProjectId,
        params: Option<Value>,
    ) -> Result<Value, McpErrorDetail> {
        let p = params.ok_or_else(|| McpErrorDetail {
            code: -32602,
            message: "Missing params object".to_string(),
            data: None,
        })?;

        let tool_name = p
            .get("name")
            .and_then(|v| v.as_str())
            .ok_or_else(|| McpErrorDetail {
                code: -32602,
                message: "Missing tool name in params".to_string(),
                data: None,
            })?;

        let args = p.get("arguments").cloned().unwrap_or(json!({}));

        match tool_name {
            "memory_search" => {
                let query = args
                    .get("query")
                    .and_then(|v| v.as_str())
                    .unwrap_or_default();
                let tier = args.get("tier").and_then(|v| v.as_str());

                let memories = if let Some(db) = app_state.get_project_db(&project_id) {
                    db.with_read_conn(|conn| {
                        let query_pattern = format!("%{}%", query);
                        let mut sql = "SELECT id, tier, content, importance FROM project_memories WHERE project_id = ?1 AND content LIKE ?2".to_string();
                        if tier.is_some() {
                            sql.push_str(" AND tier = ?3");
                        }
                        sql.push_str(" ORDER BY importance DESC LIMIT 5");

                        let mut stmt = conn.prepare(&sql)?;
                        let mut results = Vec::new();
                        if let Some(t) = tier {
                            let rows = stmt.query_map(rusqlite::params![project_id.as_str(), query_pattern, t], |row| {
                                let id: String = row.get(0)?;
                                let t: String = row.get(1)?;
                                let c: String = row.get(2)?;
                                let imp: f64 = row.get(3)?;
                                Ok(format!("[{}] (Tier: {}, Imp: {:.2}) {}", id, t, imp, c))
                            })?;
                            for r in rows.flatten() {
                                results.push(r);
                            }
                        } else {
                            let rows = stmt.query_map(rusqlite::params![project_id.as_str(), query_pattern], |row| {
                                let id: String = row.get(0)?;
                                let t: String = row.get(1)?;
                                let c: String = row.get(2)?;
                                let imp: f64 = row.get(3)?;
                                Ok(format!("[{}] (Tier: {}, Imp: {:.2}) {}", id, t, imp, c))
                            })?;
                            for r in rows.flatten() {
                                results.push(r);
                            }
                        }
                        Ok(results)
                    }).unwrap_or_default()
                } else {
                    Vec::new()
                };

                let output = if memories.is_empty() {
                    format!("No memories found matching '{}'", query)
                } else {
                    memories.join("\n")
                };

                Ok(json!({
                    "content": [
                        { "type": "text", "text": output }
                    ]
                }))
            }
            "deep_research" => {
                let topic = args
                    .get("topic")
                    .and_then(|v| v.as_str())
                    .unwrap_or("Untitled Investigation");
                let report = crate::deep_research::DeepResearchEngine::conduct_research(
                    app_state,
                    project_id,
                    trans4mers_domain::ids::ConversationId::new(),
                    topic.to_string(),
                )
                .await
                .map_err(|e| McpErrorDetail {
                    code: -32000,
                    message: format!("Deep research failed: {}", e),
                    data: None,
                })?;

                Ok(json!({
                    "content": [
                        { "type": "text", "text": report.markdown_content }
                    ]
                }))
            }
            "browser_navigate" => {
                let space_id = args
                    .get("space_id")
                    .and_then(|v| v.as_str())
                    .unwrap_or("bspace_default");
                let url = args
                    .get("url")
                    .and_then(|v| v.as_str())
                    .unwrap_or("https://trans4mers.ai");

                let frame = crate::browser_space_manager::BrowserSpaceManager::navigate(
                    app_state,
                    &project_id,
                    space_id,
                    url,
                )
                .await
                .map_err(|e| McpErrorDetail {
                    code: -32000,
                    message: format!("Browser navigation failed: {}", e),
                    data: None,
                })?;

                Ok(json!({
                    "content": [
                        { "type": "text", "text": format!("Navigated to {}\nTitle: {}\nStatus: {}", frame.current_url, frame.page_title, frame.status_code) }
                    ]
                }))
            }
            unknown => Err(McpErrorDetail {
                code: -32601,
                message: format!("Tool not found: {}", unknown),
                data: None,
            }),
        }
    }

    fn handle_resources_list(
        app_state: Arc<AppState>,
        project_id: ProjectId,
    ) -> Result<Value, McpErrorDetail> {
        let mut resources = Vec::new();

        if let Some(db) = app_state.get_project_db(&project_id) {
            let _ = db.with_read_conn(|conn| {
                if let Ok(mut stmt) = conn.prepare("SELECT id, tier, importance FROM project_memories WHERE project_id = ?1 LIMIT 20")
                    && let Ok(rows) = stmt.query_map([project_id.as_str()], |row| {
                        let id: String = row.get(0)?;
                        let tier: String = row.get(1)?;
                        Ok((id, tier))
                    }) {
                        for (id, tier) in rows.flatten() {
                            resources.push(json!({
                                "uri": format!("trans4mers://memories/{}", id),
                                "name": format!("Memory: {} ({})", id, tier),
                                "mimeType": "text/plain"
                            }));
                        }
                    }
                Ok(())
            });
        }

        Ok(json!({ "resources": resources }))
    }

    fn handle_resources_read(
        app_state: Arc<AppState>,
        project_id: ProjectId,
        params: Option<Value>,
    ) -> Result<Value, McpErrorDetail> {
        let p = params.ok_or_else(|| McpErrorDetail {
            code: -32602,
            message: "Missing params object".to_string(),
            data: None,
        })?;

        let uri = p
            .get("uri")
            .and_then(|v| v.as_str())
            .ok_or_else(|| McpErrorDetail {
                code: -32602,
                message: "Missing uri parameter".to_string(),
                data: None,
            })?;

        if let Some(mem_id) = uri.strip_prefix("trans4mers://memories/")
            && let Some(db) = app_state.get_project_db(&project_id)
        {
            let content = db
                .with_read_conn(|conn| {
                    let text: String = conn.query_row(
                        "SELECT content FROM project_memories WHERE id = ?1",
                        [mem_id],
                        |row| row.get(0),
                    )?;
                    Ok(text)
                })
                .unwrap_or_else(|_| "Memory not found".to_string());

            return Ok(json!({
                "contents": [
                    {
                        "uri": uri,
                        "mimeType": "text/plain",
                        "text": content
                    }
                ]
            }));
        }

        Err(McpErrorDetail {
            code: -32002,
            message: format!("Resource not found: {}", uri),
            data: None,
        })
    }
}
