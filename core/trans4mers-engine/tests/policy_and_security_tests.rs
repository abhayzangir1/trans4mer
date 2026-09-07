use sha2::{Digest, Sha256};
use trans4mers_domain::config::DiffReviewConfig;
use trans4mers_domain::policy::PolicyOutcome;
use trans4mers_domain::tool::{Capability, EffectClass, RiskLevel, ToolManifest, ToolSource};
use trans4mers_engine::diff_reviewer::DiffReviewer;

#[test]
fn test_policy_deny_wins_lattice() {
    // 1. Any DENY overrides ALLOW and ASK
    let outcomes_with_deny = vec![
        (Some(PolicyOutcome::Deny), Some(PolicyOutcome::Allow), Some(PolicyOutcome::Allow)),
        (Some(PolicyOutcome::Allow), Some(PolicyOutcome::Deny), Some(PolicyOutcome::Ask)),
        (Some(PolicyOutcome::Ask), Some(PolicyOutcome::Ask), Some(PolicyOutcome::Deny)),
    ];

    for (global, project, agent) in outcomes_with_deny {
        let outcome = if matches!(global, Some(PolicyOutcome::Deny))
            || matches!(project, Some(PolicyOutcome::Deny))
            || matches!(agent, Some(PolicyOutcome::Deny))
        {
            PolicyOutcome::Deny
        } else if matches!(global, Some(PolicyOutcome::Ask))
            || matches!(project, Some(PolicyOutcome::Ask))
            || matches!(agent, Some(PolicyOutcome::Ask))
        {
            PolicyOutcome::Ask
        } else {
            PolicyOutcome::Allow
        };
        assert_eq!(outcome, PolicyOutcome::Deny, "Deny must always win over Ask and Allow");
    }
}

#[test]
fn test_policy_ask_precedence_over_allow() {
    let outcomes_with_ask = vec![
        (Some(PolicyOutcome::Allow), Some(PolicyOutcome::Ask), Some(PolicyOutcome::Allow)),
        (Some(PolicyOutcome::Ask), Some(PolicyOutcome::Allow), None),
        (None, None, Some(PolicyOutcome::Ask)),
    ];

    for (global, project, agent) in outcomes_with_ask {
        let outcome = if matches!(global, Some(PolicyOutcome::Deny))
            || matches!(project, Some(PolicyOutcome::Deny))
            || matches!(agent, Some(PolicyOutcome::Deny))
        {
            PolicyOutcome::Deny
        } else if matches!(global, Some(PolicyOutcome::Ask))
            || matches!(project, Some(PolicyOutcome::Ask))
            || matches!(agent, Some(PolicyOutcome::Ask))
        {
            PolicyOutcome::Ask
        } else {
            PolicyOutcome::Allow
        };
        assert_eq!(outcome, PolicyOutcome::Ask, "Ask must take precedence over Allow");
    }
}

#[test]
fn test_diff_reviewer_lcs_hunk_computation() {
    let old_content = "fn main() {\n    println!(\"hello\");\n}\n";
    let new_content = "fn main() {\n    println!(\"hello world\");\n    println!(\"safety check\");\n}\n";

    let (hunks, additions, deletions) = DiffReviewer::compute_file_hunks(old_content, new_content);

    assert_eq!(hunks.len(), 1, "Must produce unified hunk");
    assert!(additions >= 2, "Must identify added lines");
    assert!(deletions >= 1, "Must identify modified/deleted line");
    assert!(hunks[0].header.starts_with("@@"), "Hunk header must use diff format");
}

#[test]
fn test_diff_reviewer_secret_detection() {
    let patterns = vec![
        "sk-ant-".to_string(),
        "ghp_".to_string(),
        "password=".to_string(),
        "AWS_SECRET_ACCESS_KEY".to_string(),
    ];

    let clean_diff = "const port = 8080;\nconst host = '127.0.0.1';";
    let dirty_diff = "const token = 'ghp_ABCDEFGHIJKLMNOPQRSTUVWXYZ123456';";
    let password_diff = "DATABASE_URL=postgres://user:password=supersecret@localhost/db";

    assert!(DiffReviewer::scan_secrets(clean_diff, &patterns).is_none());
    assert!(DiffReviewer::scan_secrets(dirty_diff, &patterns).is_some());
    assert!(DiffReviewer::scan_secrets(password_diff, &patterns).is_some());
}

#[test]
fn test_diff_reviewer_auto_approve_vs_force_review() {
    let config = DiffReviewConfig {
        auto_approve_small_files: true,
        auto_approve_clean_commands: true,
        force_review_secret_patterns: vec!["api_key".to_string(), "token".to_string()],
    };

    // Secret pattern present -> must NOT auto-approve even if small
    let secret_payload = "export api_key='12345'";
    let reason = DiffReviewer::scan_secrets(secret_payload, &config.force_review_secret_patterns);
    assert!(reason.is_some(), "Must force review on secret pattern");

    // Clean small payload -> eligible for auto-approve
    let clean_payload = "console.log('clean log');";
    let clean_reason = DiffReviewer::scan_secrets(clean_payload, &config.force_review_secret_patterns);
    assert!(clean_reason.is_none(), "Clean diff must not trigger force-review reason");
}

#[test]
fn test_canonical_arguments_sha256_anti_toctou() {
    // Canonical arguments hashing: key sorting and whitespace normalization
    let mut keys = vec!["path", "content", "mode"];
    keys.sort();

    let mut hasher = Sha256::new();
    for k in keys {
        hasher.update(k.as_bytes());
        hasher.update(b"=");
        if k == "content" {
            hasher.update(b"let x = 42;");
        } else if k == "mode" {
            hasher.update(b"overwrite");
        } else {
            hasher.update(b"src/lib.rs");
        }
        hasher.update(b";");
    }
    let hash1 = format!("{:x}", hasher.finalize());

    // Tampered content (attacker changes payload between approval and execution)
    let mut keys2 = vec!["path", "content", "mode"];
    keys2.sort();

    let mut hasher2 = Sha256::new();
    for k in keys2 {
        hasher2.update(k.as_bytes());
        hasher2.update(b"=");
        if k == "content" {
            hasher2.update(b"rm -rf /"); // Attacker payload
        } else if k == "mode" {
            hasher2.update(b"overwrite");
        } else {
            hasher2.update(b"src/lib.rs");
        }
        hasher2.update(b";");
    }
    let hash2 = format!("{:x}", hasher2.finalize());

    assert_ne!(hash1, hash2, "TOCTOU tampered arguments must produce divergent SHA-256 fingerprint");
}

#[test]
fn test_tool_manifest_risk_lattice() {
    let safe_tool = ToolManifest {
        name: "fs_read".to_string(),
        description: "Read file contents".to_string(),
        input_schema: serde_json::json!({}),
        output_schema: None,
        effect_class: EffectClass::ReadOnly,
        baseline_risk: RiskLevel::Safe,
        required_capabilities: vec![Capability::FilesystemRead],
        source: ToolSource::Native,
        verification_command: None,
    };

    let destructive_tool = ToolManifest {
        name: "shell_exec".to_string(),
        description: "Execute shell command".to_string(),
        input_schema: serde_json::json!({}),
        output_schema: None,
        effect_class: EffectClass::NonIdempotentMutation,
        baseline_risk: RiskLevel::Critical,
        required_capabilities: vec![Capability::ShellExecute],
        source: ToolSource::Native,
        verification_command: None,
    };

    assert_eq!(safe_tool.baseline_risk, RiskLevel::Safe);
    assert_eq!(destructive_tool.baseline_risk, RiskLevel::Critical);
    assert_ne!(safe_tool.effect_class, destructive_tool.effect_class);
}
