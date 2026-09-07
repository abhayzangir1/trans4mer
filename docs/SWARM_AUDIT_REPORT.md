# Trans4mers: Final Swarm Audit Report

This document compiles the strict, line-by-line audit findings from the 5 specialized autonomous agents deployed across the Trans4mers codebase. Per the "Brutal Honesty" mandate, the following architectural flaws, mocked components, and fake implementations have been exposed.

## 1. Frontend Inconsistencies & Fake UI (Agent 1 & Agent 2)

### A. Fake UI Wiring & Disconnected Layout
* **File Explorer & Code Editor:** The `FileExplorer` component renders a visual list of files but lacks an `onClick` handler. The `CodeEditor` component is rendered inside `WorkspaceShell` with no props. They are completely disconnected visual stubs, meaning the user cannot click a file to view or edit it.
* **Fake State Management (Facade):** `apps/desktop/src/store/swarmStore.ts` manages agent states (e.g., `updateAgentStatus`) purely in local frontend memory. It *never* invokes Tauri IPC commands (`create_agent`, `pause_agent`), meaning the UI is displaying a fake simulation of agent state entirely decoupled from the Rust backend.
* **State Duplication:** The `activeProjectId` state is dangerously split across both `useUiStore` and `useProjectStore`, guaranteeing eventual race conditions and state desynchronization.

### B. Broken IPC Contracts & Settings
* **Name Mismatch:** `useProject.ts` attempts to `invoke('cmd_get_project')`, but the backend only exposes `get_project`. The frontend will permanently fail to fetch project data.
* **Settings Modal Broken Logic:** `SetupWizard` configures `openai` and `ollama`, but `SettingsModal` only exposes an input field for `anthropic`. There is no way for a user to update their OpenAI API key after initial setup.

## 2. Backend Domain & Storage Failures (Agent 3)

### A. SQL Schema Conflicts & Syntax Errors
* **Missing UNIQUE Constraints:** `execution_repo.rs` attempts to `INSERT ... ON CONFLICT(execution_id)`, but `execution_id` is NOT defined as a `UNIQUE` index in `M001__init.sql`. This will cause a fatal SQLite crash on execution recovery/checkpointing.
* **Invalid sqlite-vec Syntax:** `vector_store.rs` attempts to use `MATCH ?1` for vector similarity. This is FTS5 syntax, not `sqlite-vec` syntax (which requires distance functions like `vec_distance_L2`), meaning semantic memory retrieval will crash.
* **Non-existent Columns:** `message_repo.rs` queries for a `sender_actor` column, but the schema only defines `sender_id` and `sender_type`. This SQL query is fundamentally broken.

### B. Masking & Hardcoded Mocks
* **Hardcoded Sequences:** `message_repo.rs` hardcodes the `sequence_id: 0` for all returned messages, destroying the event-sourced ordering logic.
* **Masking Corruption:** `agent_inst_repo.rs` uses `.unwrap_or(AgentStatus::Idle)` when deserializing state from the database. Instead of failing fast on corrupted data, it silently masks database corruption.
* **Incomplete CRUD Stubs:** `memory_repo.rs`, `approval_repo.rs`, and `workflow_repo.rs` are facades. They implement `insert` methods but lack the basic `get` or `list` methods required for the system to actually function.

## 3. Backend Engine & Architectural Fatalities (Agent 4)

### A. Phantom Events (Severe Race Condition)
* **File:** `src/cqrs.rs` and `src/agent_runtime.rs`
* **Issue:** `commit_and_emit` publishes CQRS DomainEvents to the `EventBus` *before* the SQLite transaction is successfully committed. If the disk write fails, the UI and other agents have already reacted to an event that does not legally exist in the system, shattering state consistency.

### B. Broken Logic & Dead Code
* **Un-emitted Events:** The ReAct loop attempts to emit a `ToolExecuted` event but checks for the JSON key `"tool"`. However, the execution layer outputs the key as `"tool_name"`. The event is never emitted.
* **Workflow Engine Facade:** `workflow_engine.rs` is a facade. It does not yield or wait for `AgentTask` nodes to complete. Furthermore, the `HumanReview` node is hardcoded to throw a fatal error because asynchronous DAG suspension was never actually implemented.

### C. IPC Plugin Sandbox Collapse
* **File:** `src/plugin_runner.rs`
* **Issue:** The plugin subprocess reader initializes by reading exactly 4KB of stdout. If a plugin's manifest exceeds 4KB, it truncates and crashes. During tool execution, it loops `stdout.read` until EOF. Since JSON-RPC plugins are persistent processes, they do not emit EOF, meaning every plugin call will hang indefinitely until the 30-second failsafe triggers.

### D. Unbounded Memory Growth
* **File:** `src/scheduler.rs`
* **Issue:** The task scheduler does not use a fixed worker pool. It calls `tokio::spawn` for every single pending queue item. If a swarm generates thousands of tasks, it will spawn thousands of blocked threads, leading to unbounded memory exhaustion.

## 4. Backend Commands & Mocked Providers (Agent 5)

### A. Blatant Mock Data
* **Provider Connections:** `test_provider_connection` completely ignores its arguments and hardcodes a dummy HTTP request directly to OpenAI. `list_available_models` returns hardcoded fake arrays like `vec!["llama3", "mistral"]` if anything fails, providing false confidence to the user.
* **Settings:** `get_settings` ignores the requested provider and unconditionally returns the status of the `"openai"` key.
* **Security:** `main.rs` falls back to `dummy_key` for OpenAI if the OS Keyring or Environment Variable fails, masking authentication errors and ensuring HTTP requests silently fail downstream.

### B. Incomplete Wiring & Placeholders
* **Terminal Bypass:** Because the frontend fails to send `project_id`, `terminal_resize` and `terminal_write` resort to blindly iterating over every active terminal manager in the global state to find a matching session ID.
* **Missing Features:** The Workflow UI implies users can build graphs, but there are zero Tauri endpoints exported to add, remove, or link workflow nodes. The workflow API is a facade.
