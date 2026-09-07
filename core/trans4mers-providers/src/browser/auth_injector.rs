use chromiumoxide::Page;
use tracing::{info, warn};
use trans4mers_domain::browser::BrowserAuthEntry;
use trans4mers_domain::error::Trans4mersError;

pub struct AuthInjector;

impl AuthInjector {
    /// Injects cookies, auth headers, and localStorage from the space's auth entries.
    pub async fn inject_auth_into_page(
        page: &Page,
        target_url: &str,
        entries: &[BrowserAuthEntry],
    ) -> Result<(), Trans4mersError> {
        for entry in entries {
            // 1. Inject Cookies
            if let Some(cookies_val) = &entry.cookies {
                Self::inject_cookies(page, target_url, cookies_val).await?;
            }

            // 2. Inject Headers
            if let Some(headers_val) = &entry.auth_headers {
                Self::inject_headers(page, headers_val).await?;
            }

            // 3. Inject LocalStorage
            if let Some(storage_val) = &entry.local_storage {
                Self::inject_local_storage(page, storage_val).await?;
            }
        }
        Ok(())
    }

    async fn inject_cookies(
        page: &Page,
        target_url: &str,
        cookies_val: &serde_json::Value,
    ) -> Result<(), Trans4mersError> {
        use chromiumoxide::cdp::browser_protocol::network::SetCookieParams;

        if let Some(arr) = cookies_val.as_array() {
            for item in arr {
                let name = item.get("name").and_then(|v| v.as_str());
                let value = item.get("value").and_then(|v| v.as_str());
                let domain = item.get("domain").and_then(|v| v.as_str());
                let path = item.get("path").and_then(|v| v.as_str());

                if let (Some(n), Some(v)) = (name, value) {
                    let mut builder = SetCookieParams::builder()
                        .name(n.to_string())
                        .value(v.to_string())
                        .url(target_url.to_string());

                    if let Some(d) = domain {
                        builder = builder.domain(d.to_string());
                    }
                    if let Some(p) = path {
                        builder = builder.path(p.to_string());
                    }

                    match builder.build() {
                        Ok(params) => {
                            if let Err(e) = page.execute(params).await {
                                warn!(error = %e, "Failed to set cookie via CDP");
                            }
                        }
                        Err(e) => {
                            warn!(error = %e, "Failed to build SetCookieParams");
                        }
                    }
                }
            }
        } else if let Some(map) = cookies_val.as_object() {
            for (k, v) in map {
                let val_str = match v {
                    serde_json::Value::String(s) => s.clone(),
                    other => other.to_string(),
                };
                let params_res = SetCookieParams::builder()
                    .name(k.clone())
                    .value(val_str)
                    .url(target_url.to_string())
                    .path("/".to_string())
                    .build();

                match params_res {
                    Ok(p) => {
                        if let Err(e) = page.execute(p).await {
                            warn!(error = %e, "Failed to set cookie via CDP");
                        }
                    }
                    Err(e) => {
                        warn!(error = %e, "Failed to build SetCookieParams");
                    }
                }
            }
        }
        Ok(())
    }

    async fn inject_headers(
        page: &Page,
        headers_val: &serde_json::Value,
    ) -> Result<(), Trans4mersError> {
        if let Some(map) = headers_val.as_object() {
            let headers_map = chromiumoxide::cdp::browser_protocol::network::Headers::new(
                serde_json::Value::Object(map.clone()),
            );
            let params =
                chromiumoxide::cdp::browser_protocol::network::SetExtraHttpHeadersParams::new(
                    headers_map,
                );
            if let Err(e) = page.execute(params).await {
                warn!(error = %e, "Failed to set extra HTTP headers via CDP");
            } else {
                info!("Injected custom HTTP auth headers via CDP");
            }
        }
        Ok(())
    }

    async fn inject_local_storage(
        page: &Page,
        storage_val: &serde_json::Value,
    ) -> Result<(), Trans4mersError> {
        if let Some(map) = storage_val.as_object() {
            let escaped_obj = serde_json::to_string(map).unwrap_or_else(|_| "{}".to_string());
            let js = format!(
                r#"(() => {{
                    try {{
                        const obj = {escaped_obj};
                        for (const [k, v] of Object.entries(obj)) {{
                            localStorage.setItem(k, typeof v === 'string' ? v : JSON.stringify(v));
                        }}
                    }} catch(e) {{}}
                }})()"#
            );
            let _ = page.evaluate(js).await;
            info!("Injected custom localStorage items");
        }
        Ok(())
    }
}
