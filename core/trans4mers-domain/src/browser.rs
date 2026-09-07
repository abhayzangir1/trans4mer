use crate::error::Trans4mersError;
use crate::ids::ProjectId;
use async_trait::async_trait;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BrowserSpace {
    pub id: String,
    pub project_id: ProjectId,
    pub name: String,
    pub profile_path: String,
    pub browser_binary: Option<String>,
    pub permissions: BrowserPermissions,
    pub is_active: bool,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct BrowserPermissions {
    pub agent_allowlist: Vec<String>,
    pub domain_allowlist: Vec<String>,
    pub domain_blocklist: Vec<String>,
    pub snapshot_every_action: bool,
    pub max_snapshots: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BrowserAuthEntry {
    pub id: String,
    pub space_id: String,
    pub service: String,
    pub cookies: Option<serde_json::Value>,
    pub local_storage: Option<serde_json::Value>,
    pub auth_headers: Option<serde_json::Value>,
    pub encrypted_blob: Option<Vec<u8>>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BrowserSnapshot {
    pub id: String,
    pub space_id: String,
    pub reason: Option<String>,
    pub tree_hash: String,
    pub file_count: usize,
    pub total_size_bytes: usize,
    pub triggered_by: Option<String>,
    pub created_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BrowserNavigationResult {
    pub current_url: String,
    pub page_title: String,
    pub status_code: u16,
    pub is_secure: bool,
    pub content_summary: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BrowserActionResult {
    pub success: bool,
    pub message: String,
    pub target_element: Option<String>,
}

#[async_trait]
pub trait BrowserManager: Send + Sync {
    async fn navigate(
        &self,
        space: &BrowserSpace,
        url: &str,
        auth_entries: &[BrowserAuthEntry],
    ) -> Result<BrowserNavigationResult, Trans4mersError>;
    async fn click(
        &self,
        space: &BrowserSpace,
        selector: &str,
    ) -> Result<BrowserActionResult, Trans4mersError>;
    async fn type_text(
        &self,
        space: &BrowserSpace,
        selector: &str,
        text: &str,
    ) -> Result<BrowserActionResult, Trans4mersError>;
    async fn extract_text(&self, space: &BrowserSpace) -> Result<String, Trans4mersError>;
    async fn extract_structured(
        &self,
        space: &BrowserSpace,
        schema_hint: &str,
    ) -> Result<serde_json::Value, Trans4mersError>;
    async fn screenshot(&self, space: &BrowserSpace) -> Result<Vec<u8>, Trans4mersError>;
    async fn close_space(&self, space_id: &str) -> Result<(), Trans4mersError>;
    async fn close_all(&self) -> Result<(), Trans4mersError>;
}

/// Fallback offline browser manager for headless environments or unit tests.
#[derive(Debug, Clone, Default)]
pub struct NoopBrowserManager;

#[async_trait]
impl BrowserManager for NoopBrowserManager {
    async fn navigate(
        &self,
        _space: &BrowserSpace,
        url: &str,
        _auth: &[BrowserAuthEntry],
    ) -> Result<BrowserNavigationResult, Trans4mersError> {
        Ok(BrowserNavigationResult {
            current_url: url.to_string(),
            page_title: "Offline Mock Page".to_string(),
            status_code: 200,
            is_secure: url.starts_with("https://"),
            content_summary: format!("Offline test content for {}", url),
        })
    }
    async fn click(
        &self,
        _space: &BrowserSpace,
        selector: &str,
    ) -> Result<BrowserActionResult, Trans4mersError> {
        Ok(BrowserActionResult {
            success: true,
            message: format!("Clicked {}", selector),
            target_element: Some("BUTTON".to_string()),
        })
    }
    async fn type_text(
        &self,
        _space: &BrowserSpace,
        selector: &str,
        text: &str,
    ) -> Result<BrowserActionResult, Trans4mersError> {
        Ok(BrowserActionResult {
            success: true,
            message: format!("Typed '{}' into {}", text, selector),
            target_element: Some("INPUT".to_string()),
        })
    }
    async fn extract_text(&self, _space: &BrowserSpace) -> Result<String, Trans4mersError> {
        Ok("Offline extracted text".to_string())
    }
    async fn extract_structured(
        &self,
        _space: &BrowserSpace,
        _schema_hint: &str,
    ) -> Result<serde_json::Value, Trans4mersError> {
        Ok(serde_json::json!({ "status": "ok", "mode": "noop" }))
    }
    async fn screenshot(&self, _space: &BrowserSpace) -> Result<Vec<u8>, Trans4mersError> {
        Ok(vec![0x89, b'P', b'N', b'G', 0x0D, 0x0A, 0x1A, 0x0A])
    }
    async fn close_space(&self, _space_id: &str) -> Result<(), Trans4mersError> {
        Ok(())
    }
    async fn close_all(&self) -> Result<(), Trans4mersError> {
        Ok(())
    }
}
