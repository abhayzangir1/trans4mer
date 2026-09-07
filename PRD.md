# Trans4mers Product Requirements Document

## 1. Overview

Trans4mers is a desktop application for running local, multi-agent AI workflows on developer workstations. It replaces hosted cloud agent services with a local runtime built on Tauri v2 and Rust, backed by local Ollama models and an embedded SQLite database.

Key product components:
- Tauri v2 application wrapper hosting a multi-threaded Rust engine and a React 18 frontend.
- Local model execution through Ollama daemons, with optional API keys for cloud providers stored in the OS credential manager.
- Event-sourced persistence using SQLite in WAL mode with synchronous writes.
- Action approval gates with unified diff inspection, argument hashing, and isolated git worktrees.
- Integrated desktop interface providing team chat, a code editor, terminal, browser mirror, swarm visualizer, memory browser, and MCP protocol inspector.
- Offline by default with zero network egress unless explicitly configured.

---

## 2. Goals and Design Principles

1. Private by default: Source code, conversation logs, and cognitive memory stay on local disk. Network calls only happen when an operator configures an external provider, connects a remote MCP server, or starts a browser task.
2. Crash recovery: The engine writes every agent step and tool invocation to an append-only event log before mutating database tables. If the app closes abruptly, it reconstructs in-flight work on startup.
3. Operator oversight: Autonomous agents can make mistakes. Operations that touch disk outside temporary folders, execute terminal commands, or alter memory rules pause for human approval.
4. Independent execution: Tasks run locally without ongoing per-token subscription costs when using open-weight models.

---

## 3. Users and Workflows

### Personas
- Software engineers who want automated coding assistance, multi-file refactors, and test runs without sending proprietary code to third-party servers.
- Security auditors who need reproducible local agent runs with strict permission boundaries, secret scanning, and full audit logs.
- Researchers analyzing local documents, searching technical literature, and running multi-perspective debate swarms.

### Workflows
- Multi-agent feature development: A supervisor agent breaks down a technical goal into milestones, launches worker agents in isolated git worktrees (`agent/{sub_agent_id}`), and requests review before merging code.
- Adversarial debate: Two agents run a structured multi-round debate on an architectural choice, followed by an executive agent summarizing actionable conclusions.
- Refactoring with diff review: An agent drafts file changes, and the operator reviews the proposed diff hunk-by-hunk in a side-by-side Monaco editor before accepting.
- Rule distillation: When an operator rejects an action with feedback, the system saves the correction as a learned constraint in procedural memory and injects it into future prompts.
- Codebase RAG: Developers search code repositories using a combination of BM25 full-text queries and dense vector similarity.
- Browser automation: Agents interact with web applications in sandboxed Chromium profiles while streaming viewport updates to the user interface.

---

## 4. Functional Specifications

### Desktop Layout
The application uses a 3-column resizable layout:
- Left panel: Team sidebar with channels (`#general`), direct message agent avatars, and a file explorer.
- Center panel: Multi-tab workspace housing:
  - Chat: Channel feed supporting markdown formatting, `@mention` agent dispatches, and `/schedule` cron triggers.
  - Code: Monaco editor with file tree navigation and keyboard save shortcuts.
  - Swarm Map: ReactFlow canvas showing active agent relationships, delegation links, and execution status.
  - Fleet Dashboard: Grid view of all active agents with step counters, token metrics, and cancel buttons.
  - Deep Research: Multi-stage research view that breaks questions into search facets, indexes sources, and outputs cited summaries.
  - Browser Mirror: Viewport rendering sandboxed Chromium sessions with controls to roll back to earlier snapshots.
  - Memory Inspector: Browser for the 4-tier memory pyramid showing retention levels and temporal validity ranges.
  - Document RAG: Indexing panel reporting chunk counts, file hashes, and hybrid search results.
  - Artifacts: Markdown viewer for plans, diagrams, and reports, supporting line-level comments that dispatch tasks back to agents.
- Right panel: Inspector displaying selected agent details, active ReAct steps, and an embedded terminal (`xterm.js`).

### Multi-Agent Runtime
- The ReAct loop runs up to 25 steps per execution, handling message claiming, context compaction, hybrid memory retrieval, streaming generation, and tool execution.
- Model resolution checks six levels: agent instance override, conversation override, definition default, project setting, app config, and local Ollama auto-detection.
- Supervisor-worker swarms decompose goals into milestones and track progress against deadlines.
- Adversarial debates alternate between proponent and critic prompts for 1 to 10 rounds before reaching a consensus judgment.
- Fan-out swarms distribute independent subtasks across a pool of pre-allocated workers.
- Child agents run inside dedicated git worktrees located at `.trans4mers/worktrees/{agent_id}`.

### Memory and Retrieval
- Four tiers categorize memory by permanence: working memory for in-flight context, episodic memory for task logs, semantic memory for distilled facts, and procedural memory for permanent constraints.
- Hybrid search runs SQLite FTS5 BM25 queries alongside dense vector similarity, combining results with Reciprocal Rank Fusion ($k = 60$).
- Background jobs run a 60-second distillation sweep over inactive conversations and an optional 3:00 AM dreaming worker with local-model credit safeguards.

### Security and Governance
- Every tool declares an effect class (`ReadOnly`, `IdempotentMutation`, `NonIdempotentMutation`, `Unknown`), a risk level (`Safe`, `Low`, `Medium`, `High`, `Critical`), and required capabilities.
- Policies follow a deny-wins rule across global, project, and agent scopes. If any layer denies, the request is blocked.
- DiffReviewer generates unified diffs using longest common subsequence dynamic programming and scans for sensitive patterns (passwords, private keys, API tokens).
- Argument hashing serializes tool arguments into sorted canonical JSON and checks a SHA-256 digest before running approved actions.
- Advisory file locks prevent agents from making conflicting edits to the same file.

### Protocols and External Tools
- Includes 25 built-in native tools covering file I/O, git commands, terminal interaction, browser tasks, memory queries, and sub-agent delegation.
- The Model Context Protocol client handles child processes over stdio and remote servers over SSE, logging all traffic frames to SQLite.
- Interactive terminal sessions spawn real shell processes (`powershell.exe` on Windows, `bash` on Unix) through `portable-pty`.
- An offline guidance catalog includes context limits, parameter sizes, and capability flags for 22 model families without making web requests.

---

## 5. Non-Functional Requirements

- Cold start to an interactive desktop window takes under 1.5 seconds on solid-state storage.
- Synchronous SQLite WAL transactions commit in under 5 milliseconds.
- Zero outbound network traffic when operating in local mode.
- Child processes (Chromium and PTY sessions) terminate cleanly when browser spaces close or the application exits.
- Rust codebase compiles cleanly under `-D warnings`.

---

## 6. Known Constraints and Operational Limits

1. Model reasoning limits: Small models (3B to 8B parameters) can struggle on multi-step architectural refactors. The runtime uses step limits, self-healing retries, and manual diff approvals to catch missteps.
2. Embedding requirement: Full semantic vector search requires an active embedding model (such as `nomic-embed-text` in Ollama). When none is present, retrieval falls back to lexical FTS5 BM25 search.
3. Single workstation design: Trans4mers runs as a local single-user desktop program. It does not provide multi-tenant user accounts or real-time cloud document sync.
4. Binary prerequisites: Browser spaces require a local Google Chrome or Chromium executable. The MCP Inspector requires Node.js and npx installed on the host.
