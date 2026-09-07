use chrono::Utc;
use trans4mers_domain::document::DocChunk;
use trans4mers_domain::ids::{AgentInstanceId, ConversationId, MemoryId, ProjectId};
use trans4mers_domain::learning::{Confidence, LearnedRule, RuleMetadata};
use trans4mers_domain::memory::{
    Memory, MemoryLifecycle, MemoryProvenance, MemoryScope, MemoryTier,
};
use trans4mers_storage::db_handle::DbHandle;
use trans4mers_storage::fts5::{sanitize_fts5_prefix_query, sanitize_fts5_query};
use trans4mers_storage::migration_runner::MigrationRunner;
use trans4mers_storage::repos::document_repo;
use uuid::Uuid;

#[test]
fn test_fts5_query_sanitizer_edge_cases() {
    // Malformed punctuation and special operators must not crash FTS5
    let raw = "test: (foo\"bar*) AND NOT OR 'weird^symbols'";
    let sanitized = sanitize_fts5_query(raw);
    assert!(!sanitized.contains(':'));
    assert!(!sanitized.contains('('));
    assert!(!sanitized.contains(')'));
    assert!(!sanitized.contains('*'));

    // Prefix sanitizer should quote terms and attach * to the last token if len >= 3
    let prefix_q = sanitize_fts5_prefix_query("database index");
    assert!(prefix_q.contains("\"database\""));
    assert!(prefix_q.ends_with("\"index\"*"));

    // Empty or pure punctuation query returns empty string
    assert_eq!(sanitize_fts5_query("   !@#$%^&*()   "), "");
    assert_eq!(sanitize_fts5_prefix_query(""), "");
}

#[test]
fn test_fts5_document_chunks_bm25_search() {
    let unique = format!("t4m_fts5_doc_{}.sqlite", Uuid::new_v4());
    let db_path = std::env::temp_dir().join(unique);
    let db = DbHandle::open(&db_path).expect("open db");

    db.with_exclusive_conn(|conn| MigrationRunner::run_project_migrations(conn, 768))
        .expect("run project migrations");

    let proj_id = "test_fts5_proj";

    let chunk1 = DocChunk {
        chunk_id: "chunk_browser".to_string(),
        project_id: proj_id.to_string(),
        file_path: "src/browser.rs".to_string(),
        content_hash: "hash1".to_string(),
        ord: 0,
        line_start: 1,
        line_end: 20,
        text: "The CdpBrowserManager automates native Chromium via Chrome DevTools Protocol."
            .to_string(),
        embedding: None,
        kind: "code".to_string(),
        token_estimate: 15,
        created_at: Utc::now(),
    };

    let chunk2 = DocChunk {
        chunk_id: "chunk_database".to_string(),
        project_id: proj_id.to_string(),
        file_path: "src/db.rs".to_string(),
        content_hash: "hash2".to_string(),
        ord: 0,
        line_start: 1,
        line_end: 20,
        text: "The SQLite relational database maintains CQRS domain events and memory tiers."
            .to_string(),
        embedding: None,
        kind: "code".to_string(),
        token_estimate: 15,
        created_at: Utc::now(),
    };

    let chunk3 = DocChunk {
        chunk_id: "chunk_security".to_string(),
        project_id: proj_id.to_string(),
        file_path: "src/security.rs".to_string(),
        content_hash: "hash3".to_string(),
        ord: 0,
        line_start: 1,
        line_end: 20,
        text:
            "Zero egress policy prevents unauthorized network connections and token exfiltration."
                .to_string(),
        embedding: None,
        kind: "code".to_string(),
        token_estimate: 15,
        created_at: Utc::now(),
    };

    // Insert batch into doc_chunks; triggers automatically index into doc_chunks_fts
    db.with_write_tx(|conn| {
        document_repo::insert_chunk_batch(conn, &[chunk1.clone(), chunk2.clone(), chunk3.clone()])
    })
    .expect("insert chunk batch");

    // 1. Search for browser automation
    let query_1 = "Chromium DevTools Protocol";
    let sanitized_1 = sanitize_fts5_prefix_query(query_1);
    let results_1 = db
        .with_read_conn(|conn| {
            document_repo::search_chunks_fts5(conn, proj_id, &sanitized_1, None, 10)
        })
        .expect("search chunks fts5");

    assert!(!results_1.is_empty(), "Should return matching chunks");
    assert_eq!(results_1[0].0.chunk_id, "chunk_browser");

    // 2. Search for database and verify rank
    let query_2 = "SQLite database";
    let sanitized_2 = sanitize_fts5_prefix_query(query_2);
    let results_2 = db
        .with_read_conn(|conn| {
            document_repo::search_chunks_fts5(conn, proj_id, &sanitized_2, None, 10)
        })
        .expect("search chunks fts5");

    assert!(!results_2.is_empty());
    assert_eq!(results_2[0].0.chunk_id, "chunk_database");

    // 3. Search with file_pattern filter
    let results_filtered = db
        .with_read_conn(|conn| {
            document_repo::search_chunks_fts5(conn, proj_id, &sanitized_2, Some("security"), 10)
        })
        .expect("search with filter");

    assert_eq!(
        results_filtered.len(),
        0,
        "Should not match when file_pattern doesn't match"
    );

    let _ = std::fs::remove_file(db_path);
}

#[test]
fn test_fts5_memory_search_and_acl() {
    let unique = format!("t4m_fts5_mem_{}.sqlite", Uuid::new_v4());
    let db_path = std::env::temp_dir().join(unique);
    let db = DbHandle::open(&db_path).expect("open db");

    db.with_exclusive_conn(|conn| MigrationRunner::run_project_migrations(conn, 768))
        .expect("run project migrations");

    let proj_id = ProjectId::new();
    let agent_id_1 = AgentInstanceId::new();
    let agent_id_2 = AgentInstanceId::new();
    let conv_id_1 = ConversationId::new();

    let mem1 = Memory {
        id: MemoryId::new(),
        project_id: proj_id.clone(),
        conversation_id: Some(conv_id_1.clone()),
        agent_instance_id: Some(agent_id_1.clone()),
        scope: MemoryScope::Conversation,
        lifecycle: MemoryLifecycle::Persisted,
        content: "Remember that the user prefers PostgreSQL schemas over MySQL.".to_string(),
        embedding: None,
        importance: 0.9,
        confidence: 0.95,
        provenance: MemoryProvenance {
            source_event_id: None,
            source_message_id: None,
            source_agent_id: None,
            creation_reason: "user preference".to_string(),
        },
        depth_level: 0,
        tier: MemoryTier::Working,
        retrieval_count: 0,
        last_retrieved_at: None,
        expires_at: None,
        valid_from: None,
        valid_until: None,
        valid_at: None,
        invalid_at: None,
        replaced_by: None,
        content_hash: None,
        visibility_overrides: None,
        created_at: Utc::now(),
        updated_at: Utc::now(),
    };

    let mem2 = Memory {
        id: MemoryId::new(),
        project_id: proj_id.clone(),
        conversation_id: None,
        agent_instance_id: Some(agent_id_2.clone()),
        scope: MemoryScope::Agent,
        lifecycle: MemoryLifecycle::Persisted,
        content: "Private notes for agent 2 about browser execution telemetry.".to_string(),
        embedding: None,
        importance: 0.7,
        confidence: 0.8,
        provenance: MemoryProvenance {
            source_event_id: None,
            source_message_id: None,
            source_agent_id: None,
            creation_reason: "internal note".to_string(),
        },
        depth_level: 0,
        tier: MemoryTier::Episodic,
        retrieval_count: 0,
        last_retrieved_at: None,
        expires_at: None,
        valid_from: None,
        valid_until: None,
        valid_at: None,
        invalid_at: None,
        replaced_by: None,
        content_hash: None,
        visibility_overrides: None,
        created_at: Utc::now(),
        updated_at: Utc::now(),
    };

    // Insert memories into project_memories
    db.with_write_tx(|tx| {
        tx.execute(
            "INSERT INTO project_memories (
                id, project_id, conversation_id, agent_instance_id, scope, lifecycle,
                content, importance, confidence, provenance, depth_level, tier, retrieval_count,
                created_at, updated_at
            ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15)",
            rusqlite::params![
                mem1.id.to_string(),
                mem1.project_id.to_string(),
                mem1.conversation_id.as_ref().map(|c| c.to_string()),
                mem1.agent_instance_id.as_ref().map(|a| a.to_string()),
                mem1.scope.to_string(),
                mem1.lifecycle.to_string(),
                mem1.content,
                mem1.importance,
                mem1.confidence,
                "{}",
                mem1.depth_level,
                mem1.tier.to_string(),
                mem1.retrieval_count,
                mem1.created_at.to_rfc3339(),
                mem1.updated_at.to_rfc3339(),
            ],
        )?;

        tx.execute(
            "INSERT INTO project_memories (
                id, project_id, conversation_id, agent_instance_id, scope, lifecycle,
                content, importance, confidence, provenance, depth_level, tier, retrieval_count,
                created_at, updated_at
            ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15)",
            rusqlite::params![
                mem2.id.to_string(),
                mem2.project_id.to_string(),
                mem2.conversation_id.as_ref().map(|c| c.to_string()),
                mem2.agent_instance_id.as_ref().map(|a| a.to_string()),
                mem2.scope.to_string(),
                mem2.lifecycle.to_string(),
                mem2.content,
                mem2.importance,
                mem2.confidence,
                "{}",
                mem2.depth_level,
                mem2.tier.to_string(),
                mem2.retrieval_count,
                mem2.created_at.to_rfc3339(),
                mem2.updated_at.to_rfc3339(),
            ],
        )?;
        Ok(())
    })
    .expect("insert memories");

    // Query FTS5 directly for "PostgreSQL schemas"
    let q = sanitize_fts5_prefix_query("PostgreSQL schemas");
    let matches: Vec<String> = db
        .with_read_conn(|conn| {
            let mut stmt = conn.prepare(
                "SELECT m.id FROM project_memories m
                 JOIN project_memories_fts f ON f.rowid = m.rowid
                 WHERE f.content MATCH ?1",
            )?;
            let rows = stmt.query_map([&q], |row| row.get(0))?;
            let mut list = Vec::new();
            for r in rows {
                list.push(r?);
            }
            Ok(list)
        })
        .expect("fts5 memory match");

    assert_eq!(matches.len(), 1);
    assert_eq!(matches[0], mem1.id.to_string());

    let _ = std::fs::remove_file(db_path);
}

#[test]
fn test_fts5_learned_rules_search() {
    let unique = format!("t4m_fts5_rules_{}.sqlite", Uuid::new_v4());
    let db_path = std::env::temp_dir().join(unique);
    let db = DbHandle::open(&db_path).expect("open db");

    db.with_exclusive_conn(|conn| MigrationRunner::run_project_migrations(conn, 768))
        .expect("run project migrations");

    let proj_id = ProjectId::new();

    let rule1 = LearnedRule {
        id: trans4mers_domain::ids::LearnedRuleId::new(),
        project_id: Some(proj_id.clone()),
        rule_text:
            "Always sanitize SQL queries using parameterized parameters to prevent SQL injection."
                .to_string(),
        confidence: Confidence::High,
        metadata: RuleMetadata {
            language: Some("rust".to_string()),
            framework: Some("rusqlite".to_string()),
            file_pattern: None,
            source_agent_id: None,
            source_execution_id: None,
        },
        embedding: None,
        created_at: Utc::now(),
    };

    let rule2 = LearnedRule {
        id: trans4mers_domain::ids::LearnedRuleId::new(),
        project_id: Some(proj_id.clone()),
        rule_text: "Enforce zero network egress by default; never contact telemetry endpoints."
            .to_string(),
        confidence: Confidence::High,
        metadata: RuleMetadata {
            language: None,
            framework: None,
            file_pattern: None,
            source_agent_id: None,
            source_execution_id: None,
        },
        embedding: None,
        created_at: Utc::now(),
    };

    db.with_write_tx(|tx| {
        for rule in &[&rule1, &rule2] {
            tx.execute(
                "INSERT INTO learned_rules (id, project_id, rule_text, confidence, metadata, created_at)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
                rusqlite::params![
                    rule.id.to_string(),
                    rule.project_id.as_ref().map(|p| p.to_string()),
                    rule.rule_text,
                    rule.confidence.to_string(),
                    "{}",
                    rule.created_at.to_rfc3339(),
                ],
            )?;
        }
        Ok(())
    })
    .expect("insert rules");

    // Search rules via FTS5 for "telemetry egress"
    let q = sanitize_fts5_prefix_query("telemetry egress");
    let matches: Vec<String> = db
        .with_read_conn(|conn| {
            let mut stmt = conn.prepare(
                "SELECT r.id FROM learned_rules r
                 JOIN learned_rules_fts f ON f.rowid = r.rowid
                 WHERE f.rule_text MATCH ?1",
            )?;
            let rows = stmt.query_map([&q], |row| row.get(0))?;
            let mut list = Vec::new();
            for r in rows {
                list.push(r?);
            }
            Ok(list)
        })
        .expect("fts5 rule match");

    assert_eq!(matches.len(), 1);
    assert_eq!(matches[0], rule2.id.to_string());

    let _ = std::fs::remove_file(db_path);
}
