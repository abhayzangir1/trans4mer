use chrono::Utc;
use serde_json::json;
use std::sync::Arc;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpListener;

use trans4mers_domain::browser::{
    BrowserAuthEntry, BrowserManager, BrowserPermissions, BrowserSpace,
};
use trans4mers_domain::error::Trans4mersError;
use trans4mers_domain::ids::ProjectId;
use trans4mers_providers::browser::CdpBrowserManager;
use trans4mers_storage::repos::browser_repo;

/// Starts a minimal local HTTP server on an ephemeral port.
async fn start_test_server() -> (String, tokio::task::JoinHandle<()>) {
    let listener = TcpListener::bind("127.0.0.1:0")
        .await
        .expect("bind ephemeral port");
    let addr = listener.local_addr().expect("local addr");
    let base_url = format!("http://{}", addr);

    let handle = tokio::spawn(async move {
        loop {
            let Ok((mut socket, _)) = listener.accept().await else {
                break;
            };
            tokio::spawn(async move {
                let mut buf = [0u8; 4096];
                let Ok(n) = socket.read(&mut buf).await else {
                    return;
                };
                let req_str = String::from_utf8_lossy(&buf[..n]);

                let (status, body) = if req_str.contains("GET /auth_test") {
                    if req_str.contains("auth_token=super_secret_token_123") {
                        (
                            "200 OK",
                            "<html><head><title>Auth Area</title></head><body><h1>Welcome Authenticated User</h1><p id=\"secret\">Secret Dashboard Data</p></body></html>",
                        )
                    } else {
                        (
                            "200 OK",
                            "<html><head><title>Guest Area</title></head><body><h1>Please Login</h1><p>Anonymous visitor</p></body></html>",
                        )
                    }
                } else if req_str.contains("GET /slow_element") {
                    (
                        "200 OK",
                        r#"<html><head><title>Slow Page</title></head><body>
                        <h1>Slow Loading Container</h1>
                        <div id="container"></div>
                        <script>
                            setTimeout(() => {
                                const btn = document.createElement('button');
                                btn.id = 'dynamic-btn';
                                btn.innerText = 'Click Me Ready';
                                document.getElementById('container').appendChild(btn);
                            }, 150);
                        </script>
                    </body></html>"#,
                    )
                } else if req_str.contains("GET /form_test") {
                    (
                        "200 OK",
                        r#"<html><head><title>Form Page</title></head><body>
                        <h1>Form Testing</h1>
                        <input id="username-field" name="username" placeholder="Enter username" />
                        <button id="submit-btn" onclick="document.body.innerHTML = '<h1>Form Submitted for ' + document.getElementById('username-field').value + '</h1>'">Submit</button>
                    </body></html>"#,
                    )
                } else {
                    (
                        "200 OK",
                        "<html><head><title>Default</title></head><body><h1>Hello World</h1></body></html>",
                    )
                };

                let response = format!(
                    "HTTP/1.1 {}\r\nContent-Type: text/html\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
                    status,
                    body.len(),
                    body
                );
                let _ = socket.write_all(response.as_bytes()).await;
                let _ = socket.flush().await;
            });
        }
    });

    (base_url, handle)
}

#[tokio::test]
async fn test_cdp_browser_automation_full_suite() {
    let (base_url, server_handle) = start_test_server().await;
    let browser_mgr = Arc::new(CdpBrowserManager::new());

    let test_dir = tempfile::tempdir().expect("tempdir");
    let profile_a = test_dir.path().join("profile_a");
    let profile_b = test_dir.path().join("profile_b");

    let space_a = BrowserSpace {
        id: "space_alpha".to_string(),
        project_id: ProjectId::new(),
        name: "Space Alpha".to_string(),
        profile_path: profile_a.to_string_lossy().to_string(),
        browser_binary: None,
        permissions: BrowserPermissions {
            agent_allowlist: vec!["*".to_string()],
            domain_allowlist: vec!["*".to_string()],
            domain_blocklist: vec!["*.blocked.example".to_string(), "evil.com".to_string()],
            snapshot_every_action: false,
            max_snapshots: 10,
        },
        is_active: true,
        created_at: Utc::now(),
        updated_at: Utc::now(),
    };

    let space_b = BrowserSpace {
        id: "space_beta".to_string(),
        project_id: ProjectId::new(),
        name: "Space Beta".to_string(),
        profile_path: profile_b.to_string_lossy().to_string(),
        browser_binary: None,
        permissions: BrowserPermissions {
            agent_allowlist: vec!["*".to_string()],
            domain_allowlist: vec!["*".to_string()],
            domain_blocklist: vec!["*.blocked.example".to_string()],
            snapshot_every_action: false,
            max_snapshots: 10,
        },
        is_active: true,
        created_at: Utc::now(),
        updated_at: Utc::now(),
    };

    // 1. Acceptance Criterion: Domain blocklist denied at network layer
    let blocked_res = browser_mgr
        .navigate(&space_a, "http://sub.blocked.example/admin", &[])
        .await;
    assert!(
        blocked_res.is_err(),
        "Navigation to blocked domain must fail"
    );
    match blocked_res.err().unwrap() {
        Trans4mersError::PolicyDenied { capability } => {
            assert!(capability.contains("blocked by security policy"));
        }
        other => panic!("Expected PolicyDenied, got {:?}", other),
    }

    // 2. Acceptance Criterion: Two spaces run in parallel with separate profiles; zero cookie cross-talk
    let auth_entry_a = BrowserAuthEntry {
        id: "auth_a".to_string(),
        space_id: space_a.id.clone(),
        service: "test_service".to_string(),
        cookies: Some(json!([
            { "name": "auth_token", "value": "super_secret_token_123", "path": "/" }
        ])),
        local_storage: None,
        auth_headers: None,
        encrypted_blob: None,
        created_at: Utc::now(),
        updated_at: Utc::now(),
    };

    let url_auth = format!("{}/auth_test", base_url);

    // Navigate Space A (with cookie)
    let _nav_a = browser_mgr
        .navigate(&space_a, &url_auth, &[auth_entry_a])
        .await
        .expect("Nav A");
    let text_a = browser_mgr.extract_text(&space_a).await.expect("Extract A");
    assert!(
        text_a.contains("Welcome Authenticated User"),
        "Space A must have injected cookie auth. Got: {}",
        text_a
    );

    // Navigate Space B (WITHOUT cookie)
    let _nav_b = browser_mgr
        .navigate(&space_b, &url_auth, &[])
        .await
        .expect("Nav B");
    let text_b = browser_mgr.extract_text(&space_b).await.expect("Extract B");
    assert!(
        text_b.contains("Please Login"),
        "Space B must NOT have Space A's cookies. Got: {}",
        text_b
    );
    assert!(
        !text_b.contains("Welcome Authenticated User"),
        "Zero cookie cross-talk verified"
    );

    // 3. Acceptance Criterion: Auto-wait succeeds on a slow-loading local page without sleeps
    let url_slow = format!("{}/slow_element", base_url);
    let _ = browser_mgr
        .navigate(&space_a, &url_slow, &[])
        .await
        .expect("Nav slow");
    // Click on element that appears after 150ms
    let click_res = browser_mgr
        .click(&space_a, "#dynamic-btn")
        .await
        .expect("Auto-wait click on dynamic button");
    assert!(click_res.success);

    // Timeout produces a typed error for non-existent element
    let timeout_click = browser_mgr
        .click(&space_a, "#non-existent-button-xyz")
        .await;
    assert!(timeout_click.is_err());
    match timeout_click.err().unwrap() {
        Trans4mersError::BrowserElementNotInteractable { selector, reason } => {
            assert_eq!(selector, "#non-existent-button-xyz");
            assert!(reason.contains("Element not found"));
        }
        other => panic!("Expected BrowserElementNotInteractable, got {:?}", other),
    }

    // 4. Type text & form interaction
    let url_form = format!("{}/form_test", base_url);
    let _ = browser_mgr
        .navigate(&space_a, &url_form, &[])
        .await
        .expect("Nav form");
    let type_res = browser_mgr
        .type_text(&space_a, "#username-field", "Alice Sovereign")
        .await
        .expect("Type text");
    assert!(type_res.success);
    let submit_res = browser_mgr
        .click(&space_a, "#submit-btn")
        .await
        .expect("Submit click");
    assert!(submit_res.success);
    let text_form = browser_mgr
        .extract_text(&space_a)
        .await
        .expect("Extract form text");
    assert!(
        text_form.contains("Form Submitted for Alice Sovereign"),
        "Form interaction verified: {}",
        text_form
    );

    // 5. Screenshot capture
    let screenshot_bytes = browser_mgr.screenshot(&space_a).await.expect("Screenshot");
    assert!(!screenshot_bytes.is_empty(), "Screenshot captured");
    assert_eq!(
        &screenshot_bytes[..8],
        &[0x89, b'P', b'N', b'G', 0x0D, 0x0A, 0x1A, 0x0A],
        "Valid PNG header"
    );

    // 6. Structured extraction with provenance
    let structured = browser_mgr
        .extract_structured(&space_a, "headings and links")
        .await
        .expect("Structured extraction");
    assert!(structured.get("url").is_some());
    assert!(structured.get("timestamp").is_some());

    // 7. Acceptance Criterion: Clean process cleanup on close_all
    browser_mgr.close_all().await.expect("Clean close all");

    // 8. Acceptance Criterion: Space snapshot & rollback verification
    let snap_source = test_dir.path().join("profile_snap_src");
    let snap_target = test_dir.path().join("profile_snap_dest");
    tokio::fs::create_dir_all(&snap_source).await.unwrap();
    tokio::fs::write(snap_source.join("cookie_store.db"), b"initial_state_123")
        .await
        .unwrap();

    // Take physical snapshot
    let (file_count, bytes, tree_hash) =
        browser_repo::create_physical_snapshot(&snap_source, &snap_target).unwrap();
    assert_eq!(file_count, 1);
    assert_eq!(bytes, 17);
    assert!(!tree_hash.is_empty());

    // Modify state (simulating rejected mutation)
    tokio::fs::write(snap_source.join("cookie_store.db"), b"corrupted_bad_state")
        .await
        .unwrap();
    tokio::fs::write(snap_source.join("malicious_file.exe"), b"bad_file")
        .await
        .unwrap();

    // Rollback
    browser_repo::restore_physical_snapshot(&snap_target, &snap_source).unwrap();
    let restored_data = tokio::fs::read(snap_source.join("cookie_store.db"))
        .await
        .unwrap();
    assert_eq!(
        restored_data, b"initial_state_123",
        "Profile reverted to snapshot data"
    );
    assert!(
        !snap_source.join("malicious_file.exe").exists(),
        "Rejected additions wiped on rollback"
    );

    server_handle.abort();
}
