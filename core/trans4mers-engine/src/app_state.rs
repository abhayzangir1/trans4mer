use crate::event_bus::EventBus;
use crate::scheduler::Scheduler;
use crate::shutdown_manager::ShutdownManager;
use dashmap::DashMap;
use std::sync::Arc;
use tokio_util::sync::CancellationToken;
use trans4mers_domain::ids::{ActorId, ExecutionId, ProjectId};
use trans4mers_domain::provider::{LlmProvider, ProviderRegistry};
use trans4mers_storage::db_handle::DbHandle;

/// The authoritative Dependency Injection container for the Trans4mers engine.
#[derive(Clone)]
pub struct AppState {
    /// The global database for cross-project state (settings, global memory)
    pub global_db: Arc<DbHandle>,

    /// Partitioned databases mapped per project
    pub project_dbs: Arc<DashMap<ProjectId, Arc<DbHandle>>>,

    /// Global CQRS Event Broadcaster for the UI
    pub global_event_bus: Arc<EventBus>,

    /// Partitioned CQRS Event Broadcasters per project for agent isolation
    pub project_event_buses: Arc<DashMap<ProjectId, Arc<EventBus>>>,

    /// Permit controller for execution concurrency
    pub scheduler: Arc<Scheduler>,

    /// Agnostic LLM Gateway definitions
    pub provider_registry: Arc<ProviderRegistry>,

    /// Global graceful shutdown token
    pub shutdown_manager: Arc<ShutdownManager>,

    /// Dedicated cancellation tokens for running autonomous agent execution loops
    pub execution_cancellation_tokens: Arc<DashMap<ExecutionId, CancellationToken>>,

    /// Partitioned PTY shell backends per project
    pub terminal_managers: Arc<DashMap<ProjectId, Arc<crate::terminal_manager::TerminalManager>>>,

    /// Direct session ID to ProjectId lookup for O(1) terminal dispatch
    pub terminal_sessions: Arc<DashMap<String, ProjectId>>,

    /// Project-partitioned Tool Registries
    pub tool_executors: Arc<DashMap<ProjectId, Arc<crate::tool_executor::ToolExecutor>>>,

    /// Stable identity for the local human user across all messages
    pub human_actor_id: ActorId,

    /// Global application configuration
    pub config: Arc<tokio::sync::RwLock<trans4mers_domain::config::AppConfig>>,

    /// Loaded skills catalog
    pub skill_catalog: Arc<crate::skill_loader::SkillCatalog>,

    /// Sovereign Browser Use Manager
    pub browser_manager: Arc<dyn trans4mers_domain::browser::BrowserManager>,
}

impl AppState {
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        global_db: DbHandle,
        global_event_bus: Arc<EventBus>,
        scheduler: Arc<Scheduler>,
        provider_registry: Arc<ProviderRegistry>,
        shutdown_manager: Arc<ShutdownManager>,
        config: trans4mers_domain::config::AppConfig,
        skill_catalog: Arc<crate::skill_loader::SkillCatalog>,
        browser_manager: Arc<dyn trans4mers_domain::browser::BrowserManager>,
    ) -> Self {
        let bm_shutdown = browser_manager.clone();
        let mut shut_rx = shutdown_manager.subscribe();
        tokio::spawn(async move {
            let _ = shut_rx.recv().await;
            let _ = bm_shutdown.close_all().await;
        });

        Self {
            global_db: Arc::new(global_db),
            project_dbs: Arc::new(DashMap::new()),
            global_event_bus,
            project_event_buses: Arc::new(DashMap::new()),
            scheduler,
            provider_registry,
            shutdown_manager,
            execution_cancellation_tokens: Arc::new(DashMap::new()),
            terminal_managers: Arc::new(DashMap::new()),
            terminal_sessions: Arc::new(DashMap::new()),
            tool_executors: Arc::new(DashMap::new()),
            human_actor_id: ActorId::new(),
            config: Arc::new(tokio::sync::RwLock::new(config)),
            skill_catalog,
            browser_manager,
        }
    }

    /// Gets a multiplexed event bus that broadcasts to BOTH the isolated project loop and the global UI
    pub fn get_event_bus(&self, project_id: &ProjectId) -> Arc<EventBus> {
        self.project_event_buses
            .entry(*project_id)
            .or_insert_with(|| Arc::new(EventBus::new(100)))
            .value()
            .clone()
    }

    pub fn get_terminal_manager(
        &self,
        project_id: &ProjectId,
    ) -> Arc<crate::terminal_manager::TerminalManager> {
        self.terminal_managers
            .entry(*project_id)
            .or_insert_with(|| Arc::new(crate::terminal_manager::TerminalManager::new()))
            .value()
            .clone()
    }

    /// Helper to dynamically resolve or mount a project database
    pub fn get_project_db(&self, project_id: &ProjectId) -> Option<Arc<DbHandle>> {
        if let Some(db) = self.project_dbs.get(project_id) {
            return Some(db.clone());
        }

        // Lazy load the project database by resolving the actual workspace path
        let mut workspace_path = None;
        let mut dimensions = 768;
        let _ = self.global_db.with_read_conn(|conn| {
            if let Ok(Some(proj)) =
                trans4mers_storage::repos::project_repo::get_project(conn, project_id)
            {
                workspace_path = Some(std::path::PathBuf::from(proj.workspace_path));
                dimensions = proj.embedding_dimensions;
            }
            Ok(())
        });

        let w_path = workspace_path?;
        if !w_path.exists() {
            tracing::warn!(
                "Workspace path does not exist on disk for project {}: {:?}",
                project_id,
                w_path
            );
            return None;
        }

        // Enforce Phase 0.6 isolation logic
        let t4_dir = w_path.join(".trans4mers");
        if let Err(e) = std::fs::create_dir_all(&t4_dir) {
            tracing::error!("Failed to create .trans4mers directory: {}", e);
            return None;
        }

        let db_path = t4_dir.join("project.sqlite");

        if let Ok(db) = DbHandle::open(&db_path) {
            // Apply required migrations to the new project database
            let _ = db.with_exclusive_conn(|conn_guard| {
                let _ =
                    trans4mers_storage::migration_runner::MigrationRunner::run_project_migrations(
                        conn_guard, dimensions,
                    );
                Ok(())
            });

            let db_arc = Arc::new(db);
            self.project_dbs.insert(*project_id, db_arc.clone());
            Some(db_arc)
        } else {
            None
        }
    }

    /// Helper to get or lazily initialize the tool executor for a project
    pub async fn get_or_create_tool_executor(
        &self,
        project_id: &ProjectId,
        provider: Arc<dyn LlmProvider>,
    ) -> Arc<crate::tool_executor::ToolExecutor> {
        if let Some(exec) = self.tool_executors.get(project_id) {
            exec.clone()
        } else {
            let exec = Arc::new(crate::tool_executor::ToolExecutor::new());
            crate::native_tools::register_native_tools(&exec, Arc::new(self.clone()), provider)
                .await;
            self.tool_executors.insert(*project_id, exec.clone());
            exec
        }
    }
}
