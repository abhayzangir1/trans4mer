use std::sync::Arc;
use trans4mers_domain::config::AppConfig;
use trans4mers_domain::conversation::{Conversation, ConversationSettings, ConversationStatus};
use trans4mers_domain::ids::{AgentInstanceId, ConversationId, ProjectId};
use trans4mers_domain::provider::ProviderRegistry;
use trans4mers_engine::app_state::AppState;
use trans4mers_engine::browser_space_manager::BrowserSpaceManager;
use trans4mers_engine::deep_research::DeepResearchEngine;
use trans4mers_engine::event_bus::EventBus;
use trans4mers_engine::scheduler::Scheduler;
use trans4mers_engine::shutdown_manager::ShutdownManager;
use trans4mers_engine::swarm_orchestrator::SwarmOrchestrator;
use trans4mers_storage::db_handle::DbHandle;
use trans4mers_storage::migration_runner::MigrationRunner;
use trans4mers_storage::repos::conversation_repo;

struct TestLlmProvider;

#[async_trait::async_trait]
impl trans4mers_domain::provider::LlmProvider for TestLlmProvider {
    fn name(&self) -> &'static str {
        "test-llm"
    }

    async fn generate(
        &self,
        req: &trans4mers_domain::provider::LlmRequest,
    ) -> Result<trans4mers_domain::provider::LlmResponse, trans4mers_domain::error::Trans4mersError>
    {
        let content = if let Some(last_msg) = req.messages.last() {
            if last_msg.content.contains("Decompose this macro goal") {
                "Milestone 1: Analyze codebase\nMilestone 2: Execute tasks\nMilestone 3: Verify outputs".to_string()
            } else if last_msg
                .content
                .contains("Synthesize an actionable resolution")
                || last_msg.content.contains("Synthesize a vetted consensus")
            {
                "### Multi-Agent Debate Synthesis\nConsensus achieved through adversarial analysis."
                    .to_string()
            } else {
                format!("Synthesized response for: {}", last_msg.content)
            }
        } else {
            "Test LLM generated response".to_string()
        };

        Ok(trans4mers_domain::provider::LlmResponse {
            content,
            tool_calls: None,
            metrics: trans4mers_domain::token_usage::TokenMetrics::default(),
        })
    }

    async fn embed(
        &self,
        _text: &str,
        _config: &trans4mers_domain::config::ModelConfig,
    ) -> Result<Vec<f32>, trans4mers_domain::error::Trans4mersError> {
        Ok(vec![0.1; 768])
    }
}

fn create_test_app_state(
    project_id: ProjectId,
    conversation_id: ConversationId,
) -> (Arc<AppState>, std::path::PathBuf) {
    let unique = format!("t4m_subsys_{}.sqlite", uuid::Uuid::new_v4());
    let db_path = std::env::temp_dir().join(unique);
    let db = DbHandle::open(&db_path).expect("open test db");

    // Run project migrations to create conversations, messages, memories, artifacts, etc.
    db.with_exclusive_conn(|conn| MigrationRunner::run_project_migrations(conn, 768))
        .expect("run migrations");

    // Insert test conversation
    let conv = Conversation {
        id: conversation_id,
        project_id,
        title: "Test Swarm Channel".to_string(),
        status: ConversationStatus::Active,
        settings: ConversationSettings::default(),
        created_at: chrono::Utc::now(),
        updated_at: chrono::Utc::now(),
    };
    db.with_write_tx(|conn| conversation_repo::insert_conversation(conn, &conv))
        .expect("insert test conversation");

    let event_bus = Arc::new(EventBus::new(20));
    let (sched, _, _) = Scheduler::new(4);
    let registry = Arc::new(ProviderRegistry::new());
    registry.register(Arc::new(TestLlmProvider));
    let shutdown = Arc::new(ShutdownManager::new());
    let config = AppConfig::default();

    let state = Arc::new(AppState::new(
        db.clone(),
        event_bus.clone(),
        Arc::new(sched),
        registry,
        shutdown,
        config,
        Arc::new(trans4mers_engine::skill_loader::SkillCatalog::default()),
        Arc::new(trans4mers_domain::browser::NoopBrowserManager),
    ));

    state.project_dbs.insert(project_id, Arc::new(db));
    state.project_event_buses.insert(project_id, event_bus);

    (state, db_path)
}

/// Inserts a minimal agent_instance row into the test DB to satisfy FK constraints.
fn insert_test_agent_instance(db: &DbHandle, project_id: &ProjectId, agent_id: &AgentInstanceId) {
    db.with_write_tx(|conn| {
        conn.execute(
            "INSERT INTO agent_instances (id, project_id, definition_id, status, capabilities, depth_level, created_at, updated_at)
             VALUES (?1, ?2, 'test-def', 'Active', '[]', 0, datetime('now'), datetime('now'))",
            rusqlite::params![agent_id.as_str(), project_id.as_str()],
        )?;
        Ok(())
    }).expect("insert test agent instance");
}

#[test]
fn test_browser_html_extraction() {
    let html = r#"
        <!DOCTYPE html>
        <html>
            <head>
                <title>Trans4mers Architecture Spec</title>
                <style>
                    body { background: #000; color: #fff; }
                    .hidden { display: none; }
                </style>
                <script>
                    function track() { console.log("tracking"); }
                </script>
            </head>
            <body>
                <h1>Sovereign Agent OS</h1>
                <p>Trans4mers operates with zero cloud dependencies.</p>
                <script>alert("evil");</script>
                <p>Four-tier cognitive memory ensures infinite recall.</p>
            </body>
        </html>
    "#;

    let title = BrowserSpaceManager::extract_title(html);
    assert_eq!(title, Some("Trans4mers Architecture Spec".to_string()));

    let clean = BrowserSpaceManager::extract_readable_text(html);
    assert!(clean.contains("Sovereign Agent OS"));
    assert!(clean.contains("zero cloud dependencies"));
    assert!(clean.contains("Four-tier cognitive memory"));
    assert!(
        !clean.contains("background: #000"),
        "CSS style content must be stripped"
    );
    assert!(
        !clean.contains("console.log"),
        "Inline JS script content must be stripped"
    );
    assert!(
        !clean.contains("alert(\"evil\")"),
        "Script tags must be completely removed"
    );
}

#[tokio::test]
async fn test_deep_research_workflow() {
    let project_id = ProjectId::new();
    let conversation_id = ConversationId::new();
    let (app_state, db_path) = create_test_app_state(project_id, conversation_id);

    let topic = "Sovereign AI memory architecture".to_string();

    let mem = trans4mers_domain::memory::Memory {
        id: trans4mers_domain::ids::MemoryId::new(),
        project_id,
        conversation_id: Some(conversation_id),
        agent_instance_id: None,
        scope: trans4mers_domain::memory::MemoryScope::Project,
        lifecycle: trans4mers_domain::memory::MemoryLifecycle::Persisted,
        content: "Sovereign AI memory architecture uses a 4-tier cognitive pyramid: Working, Ephemeral, Semantic, and Archival.".to_string(),
        embedding: None,
        importance: 0.9,
        confidence: 1.0,
        provenance: trans4mers_domain::memory::MemoryProvenance {
            source_event_id: None,
            source_message_id: None,
            source_agent_id: None,
            creation_reason: "Test seed".to_string(),
        },
        depth_level: 1,
        tier: trans4mers_domain::memory::MemoryTier::Semantic,
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
        created_at: chrono::Utc::now(),
        updated_at: chrono::Utc::now(),
    };
    app_state
        .get_project_db(&project_id)
        .unwrap()
        .with_write_tx(|conn| {
            trans4mers_storage::repos::memory_repo::insert_project_memory(conn, &mem)
        })
        .expect("insert test memory");

    let report = DeepResearchEngine::conduct_research(
        app_state.clone(),
        project_id,
        conversation_id,
        topic.clone(),
    )
    .await
    .expect("conduct_research must succeed");

    assert_eq!(report.topic, topic);
    assert!(
        !report.citations.is_empty(),
        "Research report must collate citations"
    );
    assert!(report.markdown_content.contains("## Key Findings"));
    assert!(
        report
            .markdown_content
            .contains("## Source Citations & Bibliography")
    );
    assert!(
        report.markdown_content.contains("[^"),
        "Report should include footnote citations"
    );

    let _ = std::fs::remove_file(&db_path);
}

#[tokio::test]
async fn test_swarm_orchestrator_debate() {
    let project_id = ProjectId::new();
    let conversation_id = ConversationId::new();
    let (app_state, db_path) = create_test_app_state(project_id, conversation_id);
    let proponent_id = AgentInstanceId::new();
    let critic_id = AgentInstanceId::new();

    let pdb = app_state.get_project_db(&project_id).expect("pdb");
    insert_test_agent_instance(&pdb, &project_id, &proponent_id);
    insert_test_agent_instance(&pdb, &project_id, &critic_id);

    let result = SwarmOrchestrator::run_debate(
        app_state.clone(),
        project_id,
        conversation_id,
        "Is local-first execution strictly superior to centralized cloud APIs for agentic memory?"
            .to_string(),
        proponent_id,
        critic_id,
        2,
    )
    .await
    .expect("run_debate should execute without errors");

    assert_eq!(result.pattern, "Debate");
    assert_eq!(
        result.transcript.len(),
        4,
        "2 rounds * 2 speakers = 4 debate turns"
    );
    assert!(result.transcript[0].0.contains("Proponent"));
    assert!(result.transcript[1].0.contains("Critic"));
    assert!(
        result.final_output.contains("Debate Synthesis")
            || result.final_output.contains("Consensus")
    );

    let _ = std::fs::remove_file(&db_path);
}

#[tokio::test]
async fn test_swarm_orchestrator_supervisor() {
    let project_id = ProjectId::new();
    let conversation_id = ConversationId::new();
    let (app_state, db_path) = create_test_app_state(project_id, conversation_id);
    let supervisor_id = AgentInstanceId::new();
    let worker1 = AgentInstanceId::new();
    let worker2 = AgentInstanceId::new();

    // Insert agent instances to satisfy FK constraints on agent_executions and inbox_messages
    let pdb = app_state.get_project_db(&project_id).expect("pdb");
    insert_test_agent_instance(&pdb, &project_id, &supervisor_id);
    insert_test_agent_instance(&pdb, &project_id, &worker1);
    insert_test_agent_instance(&pdb, &project_id, &worker2);

    let pdb_worker = pdb.clone();
    tokio::spawn(async move {
        for _ in 0..20 {
            tokio::time::sleep(std::time::Duration::from_millis(50)).await;
            let _ = pdb_worker.with_write_tx(|conn| {
                conn.execute(
                    "UPDATE agent_executions SET status = 'Completed' WHERE status = 'Pending'",
                    [],
                )?;
                Ok(())
            });
        }
    });

    let result = SwarmOrchestrator::run_supervisor(
        app_state.clone(),
        project_id,
        conversation_id,
        "Refactor SQLite storage layer to support transactional rollbacks".to_string(),
        supervisor_id,
        vec![worker1, worker2],
    )
    .await
    .expect("run_supervisor should execute without errors");

    assert_eq!(result.pattern, "Supervisor");
    assert!(
        !result.transcript.is_empty(),
        "Supervisor must generate delegation transcripts"
    );
    assert!(
        result
            .transcript
            .iter()
            .any(|(speaker, _)| speaker.contains("Supervisor"))
    );
    assert!(
        result
            .transcript
            .iter()
            .any(|(speaker, _)| speaker.contains("Worker"))
    );
    assert!(
        result
            .final_output
            .contains("Supervisor Orchestration Completed")
            || result
                .final_output
                .contains("Supervisor Swarm Execution Report")
    );

    let _ = std::fs::remove_file(&db_path);
}

#[tokio::test]
async fn test_swarm_orchestrator_fanout() {
    let project_id = ProjectId::new();
    let conversation_id = ConversationId::new();
    let (app_state, db_path) = create_test_app_state(project_id, conversation_id);
    let coordinator_id = AgentInstanceId::new();
    let worker1 = AgentInstanceId::new();
    let worker2 = AgentInstanceId::new();

    // Insert agent instances to satisfy FK constraints on agent_executions and inbox_messages
    let pdb = app_state.get_project_db(&project_id).expect("pdb");
    insert_test_agent_instance(&pdb, &project_id, &coordinator_id);
    insert_test_agent_instance(&pdb, &project_id, &worker1);
    insert_test_agent_instance(&pdb, &project_id, &worker2);

    let pdb_fanout = pdb.clone();
    tokio::spawn(async move {
        for _ in 0..20 {
            tokio::time::sleep(std::time::Duration::from_millis(50)).await;
            let _ = pdb_fanout.with_write_tx(|conn| {
                conn.execute(
                    "UPDATE agent_executions SET status = 'Completed' WHERE status = 'Pending'",
                    [],
                )?;
                Ok(())
            });
        }
    });

    let subtasks = vec![
        "Audit cryptographic hash verification".to_string(),
        "Benchmark vector similarity latency".to_string(),
    ];

    let result = SwarmOrchestrator::run_fanout(
        app_state.clone(),
        project_id,
        conversation_id,
        subtasks,
        coordinator_id,
        vec![worker1, worker2],
    )
    .await
    .expect("run_fanout should execute without errors");

    assert_eq!(result.pattern, "FanOut");
    assert_eq!(
        result.transcript.len(),
        2,
        "2 subtasks must yield 2 worker transcripts"
    );
    assert!(
        result
            .final_output
            .contains("Swarm Fan-Out Execution Report")
            || result
                .final_output
                .contains("Swarm Fan-Out Execution Completed")
    );

    let _ = std::fs::remove_file(&db_path);
}

#[tokio::test]
async fn test_nightly_dreaming_consolidation_query() {
    let project_id = ProjectId::new();
    let conversation_id = ConversationId::new();
    let (app_state, db_path) = create_test_app_state(project_id.clone(), conversation_id.clone());

    let pdb = app_state.get_project_db(&project_id).expect("pdb exists");
    pdb.with_write_tx(|conn| {
        conn.execute(
            "INSERT INTO messages (id, conversation_id, channel_id, sender_actor, content, message_kind, mentions, attachments, created_at, updated_at)
             VALUES (?1, ?2, 'general', '\"human\"', 'Remember to configure WAL mode on all SQLite handles', 'Chat', '[]', '[]', ?3, ?3)",
            rusqlite::params![
                uuid::Uuid::new_v4().to_string(),
                conversation_id.to_string(),
                chrono::Utc::now().to_rfc3339(),
            ],
        )?;
        Ok(())
    }).expect("insert message");

    let result = trans4mers_engine::nightly_dreaming::NightlyDreamingWorker::run_consolidation(
        &app_state,
        &project_id,
    )
    .await;
    assert!(
        result.is_ok(),
        "run_consolidation should succeed without SQL column errors: {:?}",
        result.err()
    );

    let summary = result.unwrap();
    assert_eq!(summary.project_id, project_id.to_string());

    let _ = std::fs::remove_file(&db_path);
}

#[tokio::test]
async fn test_bi_temporal_memory_filtering() {
    let project_id = ProjectId::new();
    let conversation_id = ConversationId::new();
    let (app_state, db_path) = create_test_app_state(project_id.clone(), conversation_id);
    let pdb = app_state.get_project_db(&project_id).expect("pdb exists");

    let now = chrono::Utc::now();
    let past = now - chrono::Duration::hours(2);
    let expired = now - chrono::Duration::hours(1);
    let future = now + chrono::Duration::hours(2);

    let make_mem = |id: &str,
                    from: Option<chrono::DateTime<chrono::Utc>>,
                    until: Option<chrono::DateTime<chrono::Utc>>| {
        trans4mers_domain::memory::Memory {
            id: trans4mers_domain::ids::MemoryId::from_str(id)
                .unwrap_or_else(|_| trans4mers_domain::ids::MemoryId::new()),
            project_id: project_id.clone(),
            conversation_id: None,
            agent_instance_id: None,
            scope: trans4mers_domain::memory::MemoryScope::Project,
            lifecycle: trans4mers_domain::memory::MemoryLifecycle::Observation,
            content: format!("Memory fact for {}", id),
            embedding: None,
            importance: 0.9,
            confidence: 0.9,
            provenance: trans4mers_domain::memory::MemoryProvenance {
                source_event_id: None,
                source_message_id: None,
                source_agent_id: None,
                creation_reason: "Test".to_string(),
            },
            depth_level: 1,
            tier: trans4mers_domain::memory::MemoryTier::Semantic,
            retrieval_count: 0,
            last_retrieved_at: None,
            expires_at: None,
            valid_from: from,
            valid_until: until,
            valid_at: None,
            invalid_at: None,
            replaced_by: None,
            content_hash: None,
            visibility_overrides: None,
            created_at: now,
            updated_at: now,
        }
    };

    let mem_active = make_mem("mem-active", Some(past), Some(future));
    let mem_expired = make_mem("mem-expired", Some(past), Some(expired));
    let mem_future = make_mem(
        "mem-future",
        Some(future),
        Some(future + chrono::Duration::hours(2)),
    );

    pdb.with_write_tx(|conn| {
        trans4mers_storage::repos::memory_repo::insert_project_memory(conn, &mem_active)?;
        trans4mers_storage::repos::memory_repo::insert_project_memory(conn, &mem_expired)?;
        trans4mers_storage::repos::memory_repo::insert_project_memory(conn, &mem_future)?;
        Ok(())
    })
    .expect("insert test memories");

    let active_mems = pdb
        .with_read_conn(|conn| {
            trans4mers_storage::repos::memory_repo::list_by_tier(
                conn,
                &project_id.to_string(),
                trans4mers_domain::memory::MemoryTier::Semantic,
            )
        })
        .expect("list_by_tier");

    assert_eq!(
        active_mems.len(),
        1,
        "Only 1 memory is currently bi-temporally valid"
    );
    assert_eq!(active_mems[0].id, mem_active.id);

    let _ = std::fs::remove_file(&db_path);
}

#[test]
fn test_utf8_char_safe_truncation_no_panic() {
    let multi_byte_text = "🚀🦀🌟✨🎉".repeat(400);
    let truncated_1500: String = multi_byte_text.chars().take(1500).collect();
    assert_eq!(truncated_1500.chars().count(), 1500);

    let truncated_300: String = multi_byte_text.chars().take(300).collect();
    assert_eq!(truncated_300.chars().count(), 300);
}
