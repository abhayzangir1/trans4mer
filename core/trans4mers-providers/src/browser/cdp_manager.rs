use async_trait::async_trait;
use dashmap::DashMap;
use futures::StreamExt;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::Mutex;
use tracing::{info, warn};
use url::Url;

use chromiumoxide::Page;
use chromiumoxide::browser::{Browser, BrowserConfig};
use trans4mers_domain::browser::{
    BrowserActionResult, BrowserAuthEntry, BrowserManager, BrowserNavigationResult, BrowserSpace,
};
use trans4mers_domain::error::Trans4mersError;

use crate::browser::auth_injector::AuthInjector;
use crate::browser::binary_detector::BrowserBinaryDetector;
use crate::browser::dom_eval::{DomActionResult, DomEval};
use crate::browser::markdown_extractor::{DEFAULT_MAX_CHARS, MarkdownExtractor};

struct SpaceSession {
    browser: Mutex<Browser>,
    page: Mutex<Page>,
    _handler_task: tokio::task::JoinHandle<()>,
}

pub struct CdpBrowserManager {
    sessions: DashMap<String, Arc<SpaceSession>>,
}

impl CdpBrowserManager {
    pub fn new() -> Self {
        Self {
            sessions: DashMap::new(),
        }
    }
}

impl Default for CdpBrowserManager {
    fn default() -> Self {
        Self::new()
    }
}

impl CdpBrowserManager {
    /// Resolves or launches a dedicated browser session for the given space.
    async fn get_or_launch_session(
        &self,
        space: &BrowserSpace,
    ) -> Result<Arc<SpaceSession>, Trans4mersError> {
        if let Some(existing) = self.sessions.get(&space.id) {
            return Ok(existing.clone());
        }

        let binary_path = BrowserBinaryDetector::find_binary(space.browser_binary.as_deref())?;
        let profile_dir = PathBuf::from(&space.profile_path);
        if let Err(e) = tokio::fs::create_dir_all(&profile_dir).await {
            return Err(Trans4mersError::Internal(format!(
                "Failed to create browser profile directory {}: {}",
                profile_dir.display(),
                e
            )));
        }

        info!(
            space_id = %space.id,
            binary = %binary_path.display(),
            profile = %profile_dir.display(),
            "Launching dedicated Chrome instance for space"
        );

        let config = BrowserConfig::builder()
            .chrome_executable(&binary_path)
            .user_data_dir(&profile_dir)
            .arg("--no-first-run")
            .arg("--no-default-browser-check")
            .arg("--remote-debugging-port=0")
            .arg("--remote-debugging-address=127.0.0.1")
            .arg("--disable-background-networking")
            .arg("--disable-sync")
            .arg("--disable-extensions")
            .arg("--disable-default-apps")
            .arg("--headless=new")
            .build()
            .map_err(|e| {
                Trans4mersError::Browser(format!("Failed to build browser config: {}", e))
            })?;

        let (browser, mut handler) = Browser::launch(config).await.map_err(|e| {
            Trans4mersError::Browser(format!("Failed to launch Chrome process: {}", e))
        })?;

        let handler_task = tokio::spawn(async move {
            while let Some(h) = handler.next().await {
                if let Err(e) = h {
                    tracing::trace!(error = %e, "CDP handler event error");
                }
            }
        });

        let page = browser.new_page("about:blank").await.map_err(|e| {
            Trans4mersError::Browser(format!("Failed to create initial page: {}", e))
        })?;

        let session = Arc::new(SpaceSession {
            browser: Mutex::new(browser),
            page: Mutex::new(page),
            _handler_task: handler_task,
        });

        self.sessions.insert(space.id.clone(), session.clone());
        Ok(session)
    }

    /// Evaluates if the URL matches allowed or blocked domains
    fn check_domain_policy(space: &BrowserSpace, url: &str) -> Result<(), Trans4mersError> {
        let parsed = Url::parse(url)
            .map_err(|e| Trans4mersError::Internal(format!("Invalid URL: {}", e)))?;
        let host = parsed.host_str().unwrap_or("").to_lowercase();

        // 1. Check blocklist
        for blocked in &space.permissions.domain_blocklist {
            let b = blocked.trim().to_lowercase();
            if let Some(suffix) = b.strip_prefix("*.") {
                if host == suffix || host.ends_with(&format!(".{}", suffix)) {
                    warn!(host = %host, pattern = %b, "Domain blocked by space policy");
                    return Err(Trans4mersError::PolicyDenied {
                        capability: format!("Domain '{}' is blocked by security policy", host),
                    });
                }
            } else if host == b {
                warn!(host = %host, "Domain blocked by space policy");
                return Err(Trans4mersError::PolicyDenied {
                    capability: format!("Domain '{}' is blocked by security policy", host),
                });
            }
        }

        // 2. Check allowlist (if not wildcard)
        if !space.permissions.domain_allowlist.is_empty()
            && !space.permissions.domain_allowlist.iter().any(|d| d == "*")
        {
            let allowed = space.permissions.domain_allowlist.iter().any(|pattern| {
                let p = pattern.trim().to_lowercase();
                if let Some(suffix) = p.strip_prefix("*.") {
                    host == suffix || host.ends_with(&format!(".{}", suffix))
                } else {
                    host == p
                }
            });

            if !allowed {
                warn!(host = %host, "Domain not in space allowlist");
                return Err(Trans4mersError::PolicyDenied {
                    capability: format!("Domain '{}' is not allowed by security policy", host),
                });
            }
        }

        Ok(())
    }
}

#[async_trait]
impl BrowserManager for CdpBrowserManager {
    async fn navigate(
        &self,
        space: &BrowserSpace,
        url: &str,
        auth_entries: &[BrowserAuthEntry],
    ) -> Result<BrowserNavigationResult, Trans4mersError> {
        let clean_url = if url.starts_with("http://") || url.starts_with("https://") {
            url.to_string()
        } else {
            format!("https://{}", url)
        };

        Self::check_domain_policy(space, &clean_url)?;

        let session = self.get_or_launch_session(space).await?;
        let page_guard = session.page.lock().await;

        // 1. Inject auth headers / cookies before navigation
        AuthInjector::inject_auth_into_page(&page_guard, &clean_url, auth_entries).await?;

        // 2. Navigate with timeout-capped auto-wait
        let nav_res =
            tokio::time::timeout(Duration::from_secs(20), page_guard.goto(&clean_url)).await;

        match nav_res {
            Ok(Ok(_)) => {}
            Ok(Err(e)) => {
                return Err(Trans4mersError::Browser(format!(
                    "Navigation failed: {}",
                    e
                )));
            }
            Err(_) => {
                return Err(Trans4mersError::BrowserTimeout {
                    condition: format!("navigation to {}", clean_url),
                    timeout_ms: 20_000,
                });
            }
        }

        // Bounded wait for page readiness (load event / DOM ready)
        let _ =
            tokio::time::timeout(Duration::from_secs(5), page_guard.wait_for_navigation()).await;

        // Extract metadata
        let title = page_guard
            .evaluate("document.title")
            .await
            .ok()
            .and_then(|v| v.into_value::<String>().ok())
            .unwrap_or_else(|| clean_url.clone());

        let current_url = page_guard
            .evaluate("window.location.href")
            .await
            .ok()
            .and_then(|v| v.into_value::<String>().ok())
            .unwrap_or_else(|| clean_url.clone());

        let is_secure = current_url.starts_with("https://");

        // Extract brief readable summary for navigation result
        let summary_raw = page_guard
            .evaluate(MarkdownExtractor::build_markdown_extract_script())
            .await
            .ok()
            .and_then(|v| v.into_value::<String>().ok())
            .unwrap_or_default();

        let content_summary = MarkdownExtractor::cap_output(&summary_raw, 2_000);

        Ok(BrowserNavigationResult {
            current_url,
            page_title: title,
            status_code: 200,
            is_secure,
            content_summary,
        })
    }

    async fn click(
        &self,
        space: &BrowserSpace,
        selector: &str,
    ) -> Result<BrowserActionResult, Trans4mersError> {
        let session = self.get_or_launch_session(space).await?;
        let page_guard = session.page.lock().await;

        let script = DomEval::build_click_script(selector);

        // Auto-wait loop: retry up to 3000ms in 100ms intervals
        let start = std::time::Instant::now();
        let timeout = Duration::from_millis(3000);

        loop {
            let res = page_guard.evaluate(script.as_str()).await;
            if let Ok(v) = res {
                let parsed = v
                    .into_value::<String>()
                    .ok()
                    .and_then(|json_str| serde_json::from_str::<DomActionResult>(&json_str).ok())
                    .filter(|a| a.success);
                if let Some(action_res) = parsed {
                    return Ok(BrowserActionResult {
                        success: true,
                        message: action_res.message,
                        target_element: action_res.tag,
                    });
                }
            }

            if start.elapsed() >= timeout {
                return Err(Trans4mersError::BrowserElementNotInteractable {
                    selector: selector.to_string(),
                    reason: "Element not found, not visible, or disabled after 3000ms".to_string(),
                });
            }

            tokio::time::sleep(Duration::from_millis(100)).await;
        }
    }

    async fn type_text(
        &self,
        space: &BrowserSpace,
        selector: &str,
        text: &str,
    ) -> Result<BrowserActionResult, Trans4mersError> {
        let session = self.get_or_launch_session(space).await?;
        let page_guard = session.page.lock().await;

        let script = DomEval::build_type_script(selector, text);

        // Auto-wait loop
        let start = std::time::Instant::now();
        let timeout = Duration::from_millis(3000);

        loop {
            let res = page_guard.evaluate(script.as_str()).await;
            if let Ok(v) = res {
                let parsed = v
                    .into_value::<String>()
                    .ok()
                    .and_then(|json_str| serde_json::from_str::<DomActionResult>(&json_str).ok())
                    .filter(|a| a.success);
                if let Some(action_res) = parsed {
                    return Ok(BrowserActionResult {
                        success: true,
                        message: action_res.message,
                        target_element: action_res.tag,
                    });
                }
            }

            if start.elapsed() >= timeout {
                return Err(Trans4mersError::BrowserElementNotInteractable {
                    selector: selector.to_string(),
                    reason: "Input element not found or not editable after 3000ms".to_string(),
                });
            }

            tokio::time::sleep(Duration::from_millis(100)).await;
        }
    }

    async fn extract_text(&self, space: &BrowserSpace) -> Result<String, Trans4mersError> {
        let session = self.get_or_launch_session(space).await?;
        let page_guard = session.page.lock().await;

        let script = MarkdownExtractor::build_markdown_extract_script();
        let raw = page_guard
            .evaluate(script)
            .await
            .map_err(|e| Trans4mersError::Browser(format!("Failed to extract page text: {}", e)))?
            .into_value::<String>()
            .map_err(|e| {
                Trans4mersError::Browser(format!("Failed to deserialize markdown text: {}", e))
            })?;

        Ok(MarkdownExtractor::cap_output(&raw, DEFAULT_MAX_CHARS))
    }

    async fn extract_structured(
        &self,
        space: &BrowserSpace,
        schema_hint: &str,
    ) -> Result<serde_json::Value, Trans4mersError> {
        let session = self.get_or_launch_session(space).await?;
        let page_guard = session.page.lock().await;

        let script = MarkdownExtractor::build_structured_extract_script(schema_hint);
        let raw_json_str = page_guard
            .evaluate(script.as_str())
            .await
            .map_err(|e| {
                Trans4mersError::Browser(format!("Structured extraction evaluation failed: {}", e))
            })?
            .into_value::<String>()
            .map_err(|e| {
                Trans4mersError::Browser(format!(
                    "Failed to deserialize structured JSON string: {}",
                    e
                ))
            })?;

        let val: serde_json::Value = serde_json::from_str(&raw_json_str)
            .map_err(|e| Trans4mersError::Serialization(e.to_string()))?;

        Ok(val)
    }

    async fn screenshot(&self, space: &BrowserSpace) -> Result<Vec<u8>, Trans4mersError> {
        let session = self.get_or_launch_session(space).await?;
        let page_guard = session.page.lock().await;

        let params = chromiumoxide::page::ScreenshotParams::builder()
            .format(chromiumoxide::cdp::browser_protocol::page::CaptureScreenshotFormat::Png)
            .build();

        let bytes = page_guard.screenshot(params).await.map_err(|e| {
            Trans4mersError::Browser(format!("Failed to capture screenshot: {}", e))
        })?;

        Ok(bytes)
    }

    async fn close_space(&self, space_id: &str) -> Result<(), Trans4mersError> {
        if let Some((_, session)) = self.sessions.remove(space_id) {
            info!(space_id = %space_id, "Closing Chrome browser session for space");
            let mut b = session.browser.lock().await;
            let _ = tokio::time::timeout(std::time::Duration::from_secs(2), b.close()).await;
        }
        Ok(())
    }

    async fn close_all(&self) -> Result<(), Trans4mersError> {
        info!("Closing all active browser sessions");
        let keys: Vec<String> = self.sessions.iter().map(|k| k.key().clone()).collect();
        for k in keys {
            let _ = self.close_space(&k).await;
        }
        Ok(())
    }
}
