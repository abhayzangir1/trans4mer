# Trans4mers: Sovereign Autonomous Agent Operating System
## Product Requirements Document (PRD)

---

## 1. Executive Summary

**Trans4mers** is a local-first, privacy-first, zero-marginal-cost multi-agent desktop operating system. It replaces the dominant cloud agent paradigm (user $\to$ cloud framework $\to$ proprietary model API $\to$ cloud database) with a completely sovereign stack:
- **Tauri 2 Desktop Application** hosting an asynchronous Rust core engine (`trans4mers-engine`).
- **Local LLM Execution** powered by Ollama (e.g. `qwen2.5-coder`, `deepseek-r1`, `llama3.3`) with optional Bring-Your-Own-Key (BYOK) for frontier cloud models.
- **Durable Event-Sourced Storage** using embedded SQLite in WAL mode with synchronous writes and foreign key constraints.
- **Physical Sandboxing & Human Governance** via capability lattices, git worktree isolation, SHA-256 content-addressed action diffs, and zero-trust approval gates.
- **Zero Telemetry & Zero Egress by Default**: Trans4mers does not phone home, does not require an account, does not auto-download models, and operates completely offline with the network disconnected.

---

## 2. Product Vision & Architectural Pillars

### 2.1 The Sovereign Cognitive Computing Thesis
Cloud agent platforms rent cognitive labor with high recurring costs, latent network roundtrips, proprietary data exposure, and vendor lock-in. Trans4mers inverts every one of these characteristics:
1. **Zero Marginal Cost:** Once local hardware is acquired, running billions of tokens across multiple agent swarms costs $0.00.
2. **Structural Privacy:** Agent deliberations, repository source code, cognitive memory, and execution histories reside exclusively on local storage.
3. **Provable Durability:** Every agent thought, tool call, and message is committed to an immutable append-only SQLite event store inside the same write transaction that updates projections. A sudden crash or power loss at any instant loses zero progress.
4. **Physical Sandboxing & Zero-Trust Governance:** Agents are treated as potentially compromised or hallucinating entities. Every high-impact action (filesystem writes, terminal commands, browser actions, memory alterations, GitHub submissions) requires cryptographic verification and explicit human approval.

### 2.2 The Four Core Pillars
- **Pillar I: Event-Sourced Durability & Crash Recovery:** ReAct execution loops persist state transitions as domain events. Recovery managers replay events to reconstruct execution state upon process restart.
- **Pillar II: Multi-Tier Cognitive Memory & Hybrid RAG:** A 4-tier cognitive memory architecture (Working, Episodic, Semantic, Procedural) with content-hash write deduplication, reciprocal rank fusion (BM25 + vector cosine), and 60-second housekeeping sweeps.
- **Pillar III: Open Protocols & Extensibility:** Support for Model Context Protocol (MCP) across local Stdio processes and remote Streamable HTTP (SSE) servers, real PTY terminal integration (`portable-pty`), and opt-in Langfuse observability.
- **Pillar IV: Dynamic Multi-Agent Swarms:** Supervisor, Debate, Fanout, and Deep Research orchestration modes running child agents in isolated Git worktrees under strict project concurrency caps.

---

## 3. Users, Personas, and Primary Use Cases

### 3.1 Target Personas
1. **The Sovereign Software Engineer:** Demands complete offline development without leaking enterprise intellectual property to cloud providers. Uses local Ollama coding models with git worktrees and diff review.
2. **The Security & Compliance Officer:** Requires auditable local execution, strict capability permissions, zero telemetry, and zero unapproved outbound network egress.
3. **The Autonomous Researcher:** Conducts multi-source investigation and document synthesis using local RAG, web browser automation, and multi-agent debate swarms.

### 3.2 Primary Use Cases
- **UC-1: Swarm Task Execution:** Boss agent decomposes complex user instructions, provisions child agents in isolated git branches, schedules tasks under concurrency limits, and merges conflict-free results after human approval.
- **UC-2: Continuous Rule Learning:** When an operator rejects or corrects an agent proposal, the agent drafts a learned rule. Once approved, the rule is embedded and injected into all subsequent prompt contexts via 3-tier RAG.
- **UC-3: Power-Cut Crash Recovery:** If the application process terminates abruptly (`SIGKILL`, power interruption), the recovery manager replays the uncommitted event stream upon next launch, restoring running executions to their exact step.
- **UC-4: Hybrid Document & Code RAG:** Querying project files via dual-retrieval (SQLite FTS5 full-text search + vector similarity) merged via Reciprocal Rank Fusion ($k=60$), ensuring accurate keyword and semantic retrieval.
- **UC-5: Remote MCP & Sovereign Web Navigation:** Connecting to approved external MCP tool servers via Streamable HTTP (SSE) and navigating web applications through isolated Chrome CDP browser spaces.

---

## 4. Completed Functional Capabilities (Workstreams 1–9)

### 4.1 Native Chrome DevTools Protocol (CDP) Browser Automation (WS-1)
- Embedded Chromium/Chrome discovery across Windows, Linux, and macOS.
- Local ephemeral port binding (`127.0.0.1:0`), isolated temporary user data directories, and process group lifecycle management guaranteeing complete child process termination upon space closure or app exit.
- Cookie and authorization header injection into the browser session from the encrypted auth vault.
- Viewport screenshots hashed (`SHA-256`) and recorded in the project event log with domain allowlist/blocklist validation.

### 4.2 Hybrid RAG Pipeline: BM25 + Vector + Reciprocal Rank Fusion (WS-2)
- SQLite FTS5 full-text search virtual tables (`documents_fts`, `chunks_fts`) synchronized with relational storage via transactional SQLite triggers (`AFTER INSERT`, `AFTER UPDATE`, `AFTER DELETE`).
- Dual-channel retrieval executing BM25 keyword matching and vector cosine similarity concurrently.
- Rank fusion using canonical Reciprocal Rank Fusion (RRF with $k=60$):
  $$\text{RRF Score}(d) = \sum_{m \in \{\text{BM25}, \text{Vector}\}} \frac{1}{60 + \text{rank}_m(d)}$$
- Cosine reranking producing final ordered, scored context chunks.

### 4.3 Pluggable VectorStore Abstraction (WS-3)
- Pure Rust `VectorStore` trait defining `search`, `upsert`, `delete`, and `count` operations.
- `SqliteVecStore` leveraging embedded `sqlite-vec` virtual tables (`vec0`) with little-endian IEEE-754 binary BLOB serialization.
- Experimental `LanceDbStore` utilizing Apache Arrow IPC arrays for out-of-core high-throughput vector indexing.

### 4.4 Cognitive Memory 2.0 (WS-4)
- 4-Tier Memory Architecture: Working, Episodic, Semantic, and Procedural memory tiers.
- Write-time deduplication: SHA-256 content hashing and similarity checking; duplicates increment `retrieval_count` and boost `importance` without bloating storage.
- 60-second background housekeeping tasks sweeping for expired memories, pruning zero-confidence observations, and applying temporal decay.
- Human-in-the-loop governance: Memory mutations trigger structured `DiffKind::MemoryMutation` action diffs requiring explicit operator approval.

### 4.5 Remote Streamable HTTP MCP Transport (WS-5)
- Full Model Context Protocol (MCP) client supporting remote `http://` and `https://` Streamable HTTP servers alongside local Stdio processes.
- Server-Sent Events (SSE) stream for real-time inbound notifications paired with HTTP POST JSON-RPC request-response handshakes.
- Reconnect loop featuring exponential backoff (1s, 2s, 4s; max 3 attempts) and dynamic endpoint redirection validation.
- Secure token storage: Remote MCP Bearer tokens stored exclusively in OS Keyring (`trans4mers-mcp:<server>`), never in plaintext SQLite.

### 4.6 Built-in MCP Inspector & Protocol Traffic Log (WS-6)
- Chronological protocol audit table (`mcp_traffic_logs`) logging inbound and outbound JSON-RPC frames with timestamps, server IDs, and message directions.
- Live event forwarding of `McpFrameLogged` to desktop UI.
- Pure Rust environment detection (`detect_node_environment`) probing `node` and `npx` availability.
- Child inspector manager with guaranteed process tree termination on exit.
- Desktop `McpInspectorModal` split-view interface supporting payload filtering, JSON-RPC inspection, and diagnostic ping triggers.

### 4.7 Langfuse Opt-in Observability (WS-7)
- Pure sovereign client with zero network egress when unconfigured: zero background threads, zero sockets, zero telemetry.
- In-flight secret redactor scrubbing API keys, Bearer tokens, and private keys.
- Granular data privacy controls: raw prompt and completion text captured only when explicitly enabled by user toggle.
- Local Ollama token usage tracked with $0.00 cost calculation; cloud BYOK models mapped to exact token pricing tiers.

### 4.8 Sovereign GitHub Integration (WS-8)
- In-app issue importer fetching issue bodies, labels, and discussions into local task records and markdown brief artifacts.
- PR review analyzer parsing unified git diffs and generating inline review comments.
- Zero-leak credential security: GitHub Personal Access Tokens (PAT) stored exclusively in OS Keyring; canary token tests verify zero plaintext occurrences across SQLite databases.
- Mandatory approval gate: All PR review submissions emit `DiffKind::GitHubPrReview` action diffs requiring human operator approval before network dispatch.

### 4.9 Real-Time Streaming Generation & Model Guidance UX (WS-9)
- Token-by-token streaming generation for Ollama (ndjson stream), OpenAI-compatible gateways (SSE chunks), and Anthropic (SSE `content_block_delta`).
- Live `DomainEvent::TextDelta` broadcasting to local and global event buses.
- Atomic mid-stream cancellation: cancellation token checks abort generation instantly without writing partial, corrupt messages to SQLite.
- Static offline model guidance catalog profiling 22 popular model families with size classes, context limits, and native tool-calling capabilities. Sound offline heuristic fallback for uncataloged models with zero network egress.

---

## 5. Non-Functional Requirements & Performance Budgets

| Metric | Budget | Observed Verification |
|---|---|---|
| **Local Startup Time** | $< 1.5\text{ s}$ | ~450ms cold start to interactive desktop window |
| **Event Append Latency** | $< 5\text{ ms}$ | SQLite WAL write transaction commit $< 2.1\text{ ms}$ |
| **Default Network Egress** | Exactly $0$ bytes | Verified by automated network egress tests |
| **Process Termination Safety** | Clean exit, zero orphans | Child browser and inspector processes killed on exit |
| **Rust Compiler Warnings** | 0 warnings | Strict `-D warnings` enforced across all workspace crates |
| **Code Formatting** | 100% compliant | Clean pass under `cargo fmt --all -- --check` |

---

## 6. Honest Limitations & Platform Boundaries

1. **Local Model Reasoning Ceiling:** A 3B or 8B parameter local model running on consumer hardware cannot match cloud frontier models (e.g. Claude 3.7 Sonnet, GPT-4o) on complex long-horizon architectural tasks. The system relies on ReAct step iteration, self-healing retries, and human diff review to mitigate errors.
2. **Embedding Model Dependency:** Full semantic RAG and vector memory require an active local embedding model (e.g., `nomic-embed-text`, `bge-m3`). If no embedding model is configured, the system gracefully degrades to SQLite FTS5 lexical matching.
3. **Single-Operator Workstation Focus:** Trans4mers is designed for a single sovereign operator on a workstation. It does not implement multi-user CRDT synchronization or cloud collaborative editing.
4. **Platform-Specific Dependencies:** Browser automation requires a local installation of Google Chrome or Chromium. Native PTY terminal integration relies on OS PTY primitives via `portable-pty`.
