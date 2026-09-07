#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

use std::sync::Arc;
use tauri::Manager;
use tracing::info;

use trans4mers_app::{AppState, EventForwarder};
use trans4mers_engine::{EventBus, Scheduler, ShutdownManager};
use trans4mers_providers::llm::ProviderRegistry;
use trans4mers_storage::DbHandle;

fn main() {
    tracing_subscriber::fmt::init();
    info!("Starting Trans4mers Desktop...");

    // Initialize the multi-threaded Tokio runtime for background orchestration & event forwarding
    let runtime = tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()
        .expect("Failed to create Tokio runtime");
    let _runtime_guard = runtime.enter();

    // Create the global app state strictly as planned
    let db_path = if let Ok(custom_path) = std::env::var("TRANS4MERS_DB_PATH") {
        std::path::PathBuf::from(custom_path)
    } else {
        let cur = std::env::current_dir().unwrap_or_else(|_| std::path::PathBuf::from("."));
        if cur.join("trans4mers.sqlite").exists() {
            cur.join("trans4mers.sqlite")
        } else if let Some(parent) = cur.parent().filter(|p| p.join("trans4mers.sqlite").exists()) {
            parent.join("trans4mers.sqlite")
        } else if let Some(grandparent) = cur.parent().and_then(|p| p.parent()).filter(|p| p.join("trans4mers.sqlite").exists()) {
            grandparent.join("trans4mers.sqlite")
        } else {
            cur.join("trans4mers.sqlite")
        }
    };
    info!("Opening database at: {}", db_path.display());
    let db = DbHandle::open(&db_path).expect("Failed to open SQLite database");

    // Genuine Phase 1 Migration
    db.with_exclusive_conn(|conn| {
        trans4mers_storage::migration_runner::MigrationRunner::run_global_migrations(conn)
    })
    .expect("Failed to run global migrations");

    // Genuine Phase 2/4 Recovery
    let recovered_executions =
        trans4mers_engine::recovery_manager::RecoveryManager::recover_crashed_executions(&db)
            .unwrap_or_default();

    let event_bus = Arc::new(EventBus::new(1000));
    let (scheduler, new_rx, wake_rx) = Scheduler::new(8);
    let scheduler = Arc::new(scheduler); // Max 8 concurrent agents

    // Register Genuine LLM Providers
    let registry = ProviderRegistry::new();
    let ollama_host =
        std::env::var("OLLAMA_HOST").unwrap_or_else(|_| "http://127.0.0.1:11434".to_string());
    registry.register(Arc::new(trans4mers_providers::llm::OllamaProvider::new(
        ollama_host,
    )));

    // Genuine sovereign-first provider registration: no fake dummy_key fallback
    if let Ok(openai_key) = trans4mers_app::settings::KeyringManager::get_api_key("openai")
        .or_else(|_| std::env::var("OPENAI_API_KEY"))
    {
        if !openai_key.trim().is_empty() {
            registry.register(Arc::new(
                trans4mers_providers::llm::OpenAiCompatProvider::new(
                    "openai".to_string(),
                    "https://api.openai.com/v1".to_string(),
                    openai_key,
                ),
            ));
        }
    }

    if let Ok(openrouter_key) = trans4mers_app::settings::KeyringManager::get_api_key("openrouter")
        .or_else(|_| std::env::var("OPENROUTER_API_KEY"))
    {
        if !openrouter_key.trim().is_empty() {
            registry.register(Arc::new(
                trans4mers_providers::llm::OpenAiCompatProvider::new(
                    "openrouter".to_string(),
                    "https://openrouter.ai/api/v1".to_string(),
                    openrouter_key,
                ),
            ));
        }
    }

    if let Ok(anthropic_key) = trans4mers_app::settings::KeyringManager::get_api_key("anthropic")
        .or_else(|_| std::env::var("ANTHROPIC_API_KEY"))
    {
        if !anthropic_key.trim().is_empty() {
            registry.register(Arc::new(trans4mers_providers::llm::AnthropicProvider::new(
                anthropic_key,
                None,
            )));
        }
    }

    if let Ok(gemini_key) = trans4mers_app::settings::KeyringManager::get_api_key("google")
        .or_else(|_| trans4mers_app::settings::KeyringManager::get_api_key("gemini"))
        .or_else(|_| std::env::var("GEMINI_API_KEY"))
    {
        if !gemini_key.trim().is_empty() {
            registry.register(Arc::new(trans4mers_providers::llm::GoogleProvider::new(
                gemini_key, None,
            )));
        }
    }

    let provider_registry = Arc::new(registry);

    let shutdown_manager = Arc::new(ShutdownManager::new());

    let config_dir = dirs::config_dir()
        .unwrap_or_else(|| std::path::PathBuf::from("."))
        .join("trans4mers");
    let _ = std::fs::create_dir_all(&config_dir);
    let config_path = config_dir.join("config.toml");
    let app_config = trans4mers_domain::config::AppConfig::load_from_toml(&config_path)
        .unwrap_or_else(|_| trans4mers_domain::config::AppConfig::default());

    let skill_catalog = trans4mers_engine::skill_loader::SkillCatalog::load_from_dir("skills")
        .unwrap_or_else(|_| trans4mers_engine::skill_loader::SkillCatalog::default());

    let browser_manager = Arc::new(trans4mers_providers::CdpBrowserManager::new());

    let state = AppState::new(
        db,
        event_bus.clone(),
        scheduler.clone(),
        provider_registry,
        shutdown_manager.clone(),
        app_config,
        Arc::new(skill_catalog),
        browser_manager,
    );

    let state_arc = Arc::new(state.clone());
    scheduler.start_background_loop(state_arc.clone(), new_rx, wake_rx);

    // Queue recovered executions from crash
    for (exec_id, proj_id) in recovered_executions {
        info!(
            "Enqueuing recovered execution {} for project {}",
            exec_id, proj_id
        );
        scheduler.queue(exec_id, proj_id);
    }

    trans4mers_engine::housekeeping::Housekeeper::spawn(state_arc.clone());
    trans4mers_engine::scheduled_automation::ScheduledAutomationManager::spawn_heartbeat_loop(
        state_arc.clone(),
    );
    trans4mers_engine::nightly_dreaming::NightlyDreamingWorker::spawn_dreaming_loop(
        state_arc.clone(),
    );
    let telegram_cancel = tokio_util::sync::CancellationToken::new();
    let telegram_gw = Arc::new(trans4mers_engine::telegram_gateway::TelegramGateway::new(
        state_arc.clone(),
    ));
    telegram_gw.start(telegram_cancel);
    trans4mers_app::LangfuseObserver::spawn(state_arc);

    tauri::Builder::default()
        .plugin(tauri_plugin_shell::init())
        .manage(state)
        .invoke_handler(tauri::generate_handler![
            // project_commands
            trans4mers_app::commands::project_commands::create_project,
            trans4mers_app::commands::project_commands::list_projects,
            trans4mers_app::commands::project_commands::get_project,
            trans4mers_app::commands::project_commands::delete_project,
            trans4mers_app::commands::project_commands::vector_migrate,
            trans4mers_app::commands::project_commands::get_vector_backend,
            // conversation_commands
            trans4mers_app::commands::conversation_commands::create_conversation,
            trans4mers_app::commands::conversation_commands::list_conversations,
            trans4mers_app::commands::conversation_commands::get_conversation,
            // message_commands
            trans4mers_app::commands::message_commands::send_message,
            trans4mers_app::commands::message_commands::get_messages,
            // agent_commands
            trans4mers_app::commands::agent_commands::create_agent,
            trans4mers_app::commands::agent_commands::list_agents,
            trans4mers_app::commands::agent_commands::pause_agent,
            trans4mers_app::commands::agent_commands::resume_agent,
            trans4mers_app::commands::agent_commands::list_agent_definitions,
            trans4mers_app::commands::agent_commands::get_agent_definition,
            trans4mers_app::commands::agent_commands::update_agent_definition,
            trans4mers_app::commands::agent_commands::create_agent_definition,
            trans4mers_app::commands::agent_commands::start_swarm_debate,
            trans4mers_app::commands::agent_commands::start_supervisor_task,
            trans4mers_app::commands::agent_commands::start_swarm_fanout,
            trans4mers_app::commands::agent_commands::start_deep_research,
            trans4mers_app::commands::agent_commands::kill_agent_execution,
            // execution_commands
            trans4mers_app::commands::execution_commands::list_active_executions,
            trans4mers_app::commands::execution_commands::get_agent_execution_details,
            trans4mers_app::commands::execution_commands::cancel_execution,
            // approval_commands
            trans4mers_app::commands::approval_commands::get_pending_approvals,
            trans4mers_app::commands::approval_commands::resolve_approval,
            trans4mers_app::commands::approval_commands::get_action_diff_by_approval,
            trans4mers_app::commands::approval_commands::get_pending_action_diffs,
            trans4mers_app::commands::approval_commands::resolve_action_diff,
            // memory_commands
            trans4mers_app::commands::memory_commands::get_memories,
            trans4mers_app::commands::memory_commands::search_memories,
            trans4mers_app::commands::memory_commands::update_memory_tier,
            trans4mers_app::commands::memory_commands::delete_memory,
            trans4mers_app::commands::memory_commands::get_memory_pyramid_stats,
            // settings_commands
            trans4mers_app::commands::settings_commands::get_settings,
            trans4mers_app::commands::settings_commands::update_settings,
            trans4mers_app::commands::settings_commands::update_feature_toggles,
            trans4mers_app::commands::settings_commands::update_provider_endpoint,
            trans4mers_app::commands::settings_commands::update_compaction_config,
            trans4mers_app::commands::settings_commands::update_diff_review_config,
            trans4mers_app::commands::settings_commands::update_performance_profile,
            trans4mers_app::commands::settings_commands::set_default_provider,
            // provider_commands
            trans4mers_app::commands::provider_commands::test_provider_connection,
            trans4mers_app::commands::provider_commands::list_available_models,
            trans4mers_app::commands::provider_commands::get_model_guidance,
            trans4mers_app::commands::provider_commands::list_model_guidance_catalog,
            // terminal_commands
            trans4mers_app::commands::terminal_commands::create_terminal_session,
            trans4mers_app::commands::terminal_commands::terminal_write,
            trans4mers_app::commands::terminal_commands::terminal_resize,
            trans4mers_app::commands::terminal_commands::destroy_terminal_session,
            // filesystem_commands
            trans4mers_app::commands::filesystem_commands::read_file,
            trans4mers_app::commands::filesystem_commands::write_file,
            trans4mers_app::commands::filesystem_commands::list_directory,
            trans4mers_app::commands::filesystem_commands::pick_directory,
            // git_commands
            trans4mers_app::commands::git_commands::get_git_status,
            // workflow_commands
            trans4mers_app::commands::workflow_commands::create_workflow,
            trans4mers_app::commands::workflow_commands::start_workflow_run,
            // artifact_commands
            trans4mers_app::commands::artifact_commands::get_artifacts,
            trans4mers_app::commands::artifact_commands::list_artifact_comments,
            trans4mers_app::commands::artifact_commands::add_artifact_comment,
            trans4mers_app::commands::artifact_commands::send_artifact_comment_to_agent,
            // automation_commands
            trans4mers_app::commands::automation_commands::list_scheduled_tasks,
            trans4mers_app::commands::automation_commands::create_scheduled_task,
            trans4mers_app::commands::automation_commands::toggle_scheduled_task,
            trans4mers_app::commands::automation_commands::delete_scheduled_task,
            trans4mers_app::commands::automation_commands::trigger_nightly_dreaming,
            // gateway_commands
            trans4mers_app::commands::gateway_commands::get_gateway_status,
            trans4mers_app::commands::gateway_commands::update_telegram_config,
            trans4mers_app::commands::gateway_commands::test_telegram_connection,
            // system_commands
            trans4mers_app::commands::system_commands::get_token_usage,
            trans4mers_app::commands::system_commands::replay_events,
            trans4mers_app::commands::system_commands::load_plugin,
            trans4mers_app::commands::system_commands::connect_mcp_server,
            trans4mers_app::commands::system_commands::cmd_get_system_status,
            trans4mers_app::commands::system_commands::get_cost_summary,
            trans4mers_app::commands::system_commands::set_cost_budget,
            trans4mers_app::commands::system_commands::list_mcp_servers,
            trans4mers_app::commands::system_commands::register_mcp_server,
            trans4mers_app::commands::system_commands::toggle_mcp_approval,
            trans4mers_app::commands::system_commands::delete_mcp_server,
            trans4mers_app::commands::system_commands::set_mcp_auth_token,
            trans4mers_app::commands::system_commands::get_mcp_auth_status,
            trans4mers_app::commands::system_commands::get_mcp_traffic_logs,
            trans4mers_app::commands::system_commands::clear_mcp_traffic_logs,
            trans4mers_app::commands::system_commands::check_node_environment,
            trans4mers_app::commands::system_commands::launch_mcp_inspector,
            trans4mers_app::commands::system_commands::stop_mcp_inspector,
            trans4mers_app::commands::system_commands::get_mcp_inspector_status,
            trans4mers_app::commands::system_commands::send_mcp_ping,
            // browser_commands
            trans4mers_app::commands::browser_commands::get_browser_spaces,
            trans4mers_app::commands::browser_commands::create_browser_space,
            trans4mers_app::commands::browser_commands::browser_navigate,
            trans4mers_app::commands::browser_commands::list_browser_snapshots,
            trans4mers_app::commands::browser_commands::browser_rollback_snapshot,
            // mcp_commands
            trans4mers_app::commands::mcp_commands::handle_mcp_request,
            // document_commands
            trans4mers_app::commands::document_commands::ingest_documents,
            trans4mers_app::commands::document_commands::search_documents,
            trans4mers_app::commands::document_commands::list_ingested_documents,
            // langfuse_commands
            trans4mers_app::commands::langfuse_commands::get_langfuse_config,
            trans4mers_app::commands::langfuse_commands::save_langfuse_config,
            trans4mers_app::commands::langfuse_commands::test_langfuse_connection,
            trans4mers_app::commands::langfuse_commands::sync_execution_to_langfuse,
            // github_commands
            trans4mers_app::commands::github_commands::get_github_status,
            trans4mers_app::commands::github_commands::save_github_pat,
            trans4mers_app::commands::github_commands::delete_github_pat,
            trans4mers_app::commands::github_commands::import_github_issue,
            trans4mers_app::commands::github_commands::create_github_pr_review,
            trans4mers_app::commands::github_commands::submit_approved_github_pr_review
        ])
        .setup(move |app| {
            let app_handle = app.handle().clone();
            if let Some(window) = app.get_webview_window("main") {
                info!("Main window obtained, showing and focusing...");
                let _ = window.show();
                let _ = window.set_focus();
            } else {
                tracing::warn!(
                    "Main window 'main' not found automatically by Tauri; creating window..."
                );
                let _ = tauri::WebviewWindowBuilder::new(app, "main", tauri::WebviewUrl::default())
                    .title("Trans4mers")
                    .inner_size(1280.0, 800.0)
                    .center()
                    .build();
            }
            // Start the event bridge to send DomainEvents to React
            EventForwarder::spawn_forwarder(app_handle, event_bus);
            Ok(())
        })
        .on_window_event(move |_window, event| {
            if let tauri::WindowEvent::CloseRequested { .. } = event {
                info!("Window close requested. Triggering graceful shutdown.");
                shutdown_manager.trigger_shutdown();
                // We let the OS kill it shortly after, or we wait for tokio tasks in a real desktop app.
            }
        })
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
