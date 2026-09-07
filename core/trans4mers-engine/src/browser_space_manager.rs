use crate::app_state::AppState;
use chrono::Utc;
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use tracing::info;
use trans4mers_domain::browser::{
    BrowserActionResult, BrowserPermissions, BrowserSnapshot, BrowserSpace,
};
use trans4mers_domain::error::Trans4mersError;
use trans4mers_domain::ids::ProjectId;
use trans4mers_storage::repos::browser_repo;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BrowserLiveFrame {
    pub space_id: String,
    pub current_url: String,
    pub page_title: String,
    pub status_code: u16,
    pub is_secure: bool,
    pub html_snippet: String,
    pub last_action: String,
    pub timestamp: String,
}

pub struct BrowserSpaceManager;

impl BrowserSpaceManager {
    /// Create a new isolated browser space with dedicated profile and security policies
    pub async fn create_browser_space(
        app_state: Arc<AppState>,
        project_id: ProjectId,
        name: String,
    ) -> Result<BrowserSpace, Trans4mersError> {
        let space_id = format!("bspace_{}", uuid::Uuid::new_v4().simple());
        let profile_path = format!("data/browser_profiles/{}", space_id);

        let space = BrowserSpace {
            id: space_id.clone(),
            project_id,
            name,
            profile_path,
            browser_binary: None,
            permissions: BrowserPermissions {
                agent_allowlist: vec!["*".to_string()],
                domain_allowlist: vec!["*".to_string()],
                domain_blocklist: vec!["*.malicious.example".to_string()],
                snapshot_every_action: true,
                max_snapshots: 50,
            },
            is_active: true,
            created_at: Utc::now(),
            updated_at: Utc::now(),
        };

        if let Some(db) = app_state.get_project_db(&project_id) {
            db.with_write_tx(|conn| browser_repo::create_space(conn, &space))?;
        }

        info!(space_id = %space_id, "Created isolated browser space");
        Ok(space)
    }

    /// List all browser spaces for a project
    pub fn list_browser_spaces(
        app_state: Arc<AppState>,
        project_id: &ProjectId,
    ) -> Result<Vec<BrowserSpace>, Trans4mersError> {
        if let Some(db) = app_state.get_project_db(project_id) {
            let proj_str = project_id.to_string();
            db.with_read_conn(|conn| browser_repo::list_spaces(conn, &proj_str))
        } else {
            Ok(Vec::new())
        }
    }

    /// Resolves space definition from database or defaults
    pub fn resolve_space(
        app_state: &Arc<AppState>,
        project_id: &ProjectId,
        space_id: &str,
    ) -> BrowserSpace {
        if let Some(db) = app_state.get_project_db(project_id)
            && let Ok(Some(space)) =
                db.with_read_conn(|conn| browser_repo::get_space(conn, space_id))
        {
            return space;
        }

        BrowserSpace {
            id: space_id.to_string(),
            project_id: *project_id,
            name: "Active Browser Space".to_string(),
            profile_path: format!("data/browser_profiles/{}", space_id),
            browser_binary: None,
            permissions: BrowserPermissions {
                agent_allowlist: vec!["*".to_string()],
                domain_allowlist: vec!["*".to_string()],
                domain_blocklist: vec!["*.malicious.example".to_string()],
                snapshot_every_action: true,
                max_snapshots: 50,
            },
            is_active: true,
            created_at: Utc::now(),
            updated_at: Utc::now(),
        }
    }

    /// Executes browser navigation via sovereign CDP BrowserManager with auth injection and auto-waiting.
    pub async fn navigate(
        app_state: Arc<AppState>,
        project_id: &ProjectId,
        space_id: &str,
        target_url: &str,
    ) -> Result<BrowserLiveFrame, Trans4mersError> {
        let space = Self::resolve_space(&app_state, project_id, space_id);

        let auth_entries = if let Some(db) = app_state.get_project_db(project_id) {
            db.with_read_conn(|conn| browser_repo::list_auth_entries(conn, space_id))
                .unwrap_or_default()
        } else {
            Vec::new()
        };

        info!(space_id = %space_id, url = %target_url, "Executing sovereign browser navigation");

        let nav_res = app_state
            .browser_manager
            .navigate(&space, target_url, &auth_entries)
            .await?;

        // Take snapshot if configured
        if space.permissions.snapshot_every_action {
            let _ = Self::take_snapshot(
                &app_state,
                project_id,
                &space,
                Some(&format!("Navigated to {}", nav_res.current_url)),
            );
        }

        let esc_title = Self::html_escape(&nav_res.page_title);
        let esc_url = Self::html_escape(&nav_res.current_url);
        let esc_summary = Self::html_escape(&nav_res.content_summary);

        let snippet = format!(
            "<div class=\"rendered-page\"><div class=\"browser-header\"><h2>{}</h2><div class=\"meta\">URL: <code>{}</code> | Status: {} | SSL: {}</div></div><div class=\"page-content\"><pre>{}</pre></div></div>",
            esc_title, esc_url, nav_res.status_code, nav_res.is_secure, esc_summary
        );

        Ok(BrowserLiveFrame {
            space_id: space_id.to_string(),
            current_url: nav_res.current_url,
            page_title: nav_res.page_title,
            status_code: nav_res.status_code,
            is_secure: nav_res.is_secure,
            html_snippet: snippet,
            last_action: "Navigation (CDP Load + Auto-Wait OK)".to_string(),
            timestamp: Utc::now().to_rfc3339(),
        })
    }

    /// Clicks on an interactable DOM element using accessibility and text heuristics.
    pub async fn click(
        app_state: Arc<AppState>,
        project_id: &ProjectId,
        space_id: &str,
        selector: &str,
    ) -> Result<BrowserActionResult, Trans4mersError> {
        let space = Self::resolve_space(&app_state, project_id, space_id);
        app_state.browser_manager.click(&space, selector).await
    }

    /// Types text into an interactive input element.
    pub async fn type_text(
        app_state: Arc<AppState>,
        project_id: &ProjectId,
        space_id: &str,
        selector: &str,
        text: &str,
    ) -> Result<BrowserActionResult, Trans4mersError> {
        let space = Self::resolve_space(&app_state, project_id, space_id);
        app_state
            .browser_manager
            .type_text(&space, selector, text)
            .await
    }

    /// Extracts clean markdown text from the page.
    pub async fn extract_text(
        app_state: Arc<AppState>,
        project_id: &ProjectId,
        space_id: &str,
    ) -> Result<String, Trans4mersError> {
        let space = Self::resolve_space(&app_state, project_id, space_id);
        app_state.browser_manager.extract_text(&space).await
    }

    /// Extracts structured JSON data from the DOM with provenance.
    pub async fn extract_structured(
        app_state: Arc<AppState>,
        project_id: &ProjectId,
        space_id: &str,
        schema_hint: &str,
    ) -> Result<serde_json::Value, Trans4mersError> {
        let space = Self::resolve_space(&app_state, project_id, space_id);
        app_state
            .browser_manager
            .extract_structured(&space, schema_hint)
            .await
    }

    /// Captures PNG screenshot.
    pub async fn screenshot(
        app_state: Arc<AppState>,
        project_id: &ProjectId,
        space_id: &str,
    ) -> Result<Vec<u8>, Trans4mersError> {
        let space = Self::resolve_space(&app_state, project_id, space_id);
        app_state.browser_manager.screenshot(&space).await
    }

    /// Takes a physical snapshot of the space's profile directory.
    pub fn take_snapshot(
        app_state: &Arc<AppState>,
        project_id: &ProjectId,
        space: &BrowserSpace,
        reason: Option<&str>,
    ) -> Result<BrowserSnapshot, Trans4mersError> {
        let snapshot_id = format!("snap_{}", uuid::Uuid::new_v4().simple());
        let source_profile = std::path::Path::new(&space.profile_path);
        let target_snapshot = std::path::Path::new("data/browser_snapshots").join(&snapshot_id);

        let (file_count, total_size_bytes, tree_hash) =
            browser_repo::create_physical_snapshot(source_profile, &target_snapshot).unwrap_or((
                0,
                0,
                "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855".to_string(),
            ));

        let snapshot = BrowserSnapshot {
            id: snapshot_id,
            space_id: space.id.clone(),
            reason: reason.map(|s| s.to_string()),
            tree_hash,
            file_count,
            total_size_bytes,
            triggered_by: Some("agent_runtime".to_string()),
            created_at: Utc::now(),
        };

        if let Some(db) = app_state.get_project_db(project_id) {
            let _ = db.with_write_tx(|conn| browser_repo::insert_snapshot(conn, &snapshot));
        }

        Ok(snapshot)
    }

    /// Restores the space profile from a physical snapshot on approval rejection or rollback.
    pub async fn rollback_to_snapshot(
        app_state: Arc<AppState>,
        project_id: &ProjectId,
        space_id: &str,
        snapshot_id: &str,
    ) -> Result<(), Trans4mersError> {
        let space = Self::resolve_space(&app_state, project_id, space_id);
        // Close running session first so file locks are released
        let _ = app_state.browser_manager.close_space(space_id).await;

        let snapshot_dir = std::path::Path::new("data/browser_snapshots").join(snapshot_id);
        let profile_dir = std::path::Path::new(&space.profile_path);

        browser_repo::restore_physical_snapshot(&snapshot_dir, profile_dir)?;
        info!(space_id = %space_id, snapshot_id = %snapshot_id, "Rolled back browser profile to snapshot");
        Ok(())
    }

    /// List recorded snapshots for a space
    pub fn list_snapshots(
        app_state: Arc<AppState>,
        project_id: &ProjectId,
        space_id: &str,
    ) -> Result<Vec<BrowserSnapshot>, Trans4mersError> {
        if let Some(db) = app_state.get_project_db(project_id) {
            db.with_read_conn(|conn| browser_repo::list_snapshots(conn, space_id))
        } else {
            Ok(Vec::new())
        }
    }

    pub fn html_escape(s: &str) -> String {
        s.replace('&', "&amp;")
            .replace('<', "&lt;")
            .replace('>', "&gt;")
            .replace('"', "&quot;")
            .replace('\'', "&#x27;")
    }

    pub fn extract_title(html: &str) -> Option<String> {
        let lower = html.to_lowercase();
        let start = lower.find("<title>")? + 7;
        let end = lower[start..].find("</title>")? + start;
        let title = html[start..end].trim();
        if title.is_empty() {
            None
        } else {
            Some(title.to_string())
        }
    }

    pub fn extract_readable_text(html: &str) -> String {
        let mut clean = String::new();
        let mut in_tag = false;
        let mut in_script = false;
        let mut in_style = false;

        let lower = html.to_lowercase();
        let chars: Vec<char> = html.chars().collect();
        let len = chars.len();
        let mut i = 0;

        while i < len {
            if !in_script && !in_style && i + 7 <= len && lower[i..i + 7].starts_with("<script") {
                in_script = true;
                i += 7;
                continue;
            }
            if in_script {
                if i + 9 <= len && lower[i..i + 9].starts_with("</script>") {
                    in_script = false;
                    i += 9;
                } else {
                    i += 1;
                }
                continue;
            }
            if !in_script && !in_style && i + 6 <= len && lower[i..i + 6].starts_with("<style") {
                in_style = true;
                i += 6;
                continue;
            }
            if in_style {
                if i + 8 <= len && lower[i..i + 8].starts_with("</style>") {
                    in_style = false;
                    i += 8;
                } else {
                    i += 1;
                }
                continue;
            }

            if chars[i] == '<' {
                in_tag = true;
                i += 1;
                continue;
            }
            if chars[i] == '>' {
                in_tag = false;
                clean.push(' ');
                i += 1;
                continue;
            }

            if !in_tag {
                clean.push(chars[i]);
            }
            i += 1;
        }

        clean.split_whitespace().collect::<Vec<_>>().join(" ")
    }
}
