use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::RwLock;
use trans4mers_domain::error::Trans4mersError;
use trans4mers_domain::tool::{Tool, ToolRequest, ToolResult};

pub struct ToolExecutor {
    pub registry: RwLock<HashMap<String, Arc<dyn Tool>>>,
}

impl Default for ToolExecutor {
    fn default() -> Self {
        Self::new()
    }
}

impl ToolExecutor {
    pub fn new() -> Self {
        Self {
            registry: RwLock::new(HashMap::new()),
        }
    }

    /// Dynamically mounts an extensible tool into the registry
    pub async fn register_tool(&self, tool: Arc<dyn Tool>) {
        let manifest = tool.manifest();
        let mut registry = self.registry.write().await;
        registry.insert(manifest.name.clone(), tool);
    }

    /// Asynchronously executes a tool dynamically looking it up by name.
    /// MCP/JSON-RPC/Native tools are abstracted entirely.
    pub async fn execute_tool(&self, request: &ToolRequest) -> Result<ToolResult, Trans4mersError> {
        let tool_name = request.tool_name.as_str();

        // Ensure we clone the Arc to release the read lock before executing (prevents deadlocks if tools register dynamically)
        let tool = {
            let registry = self.registry.read().await;
            registry.get(tool_name).cloned()
        };

        match tool {
            Some(t) => t.execute(request).await,
            None => Err(Trans4mersError::ToolNotFound(tool_name.to_string())),
        }
    }
}
