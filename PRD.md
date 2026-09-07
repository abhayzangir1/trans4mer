# Trans4mers: Sovereign Autonomous Multi-Agent Desktop Operating System
## Product Requirements Document (PRD)

---

## 1. Executive Summary

**Trans4mers** is a sovereign, local-first, zero-marginal-cost multi-agent desktop operating system designed for software engineers, security professionals, and researchers. It replaces centralized cloud agent frameworks with a fully autonomous desktop application running on local hardware:

- **Desktop Substrate**: Tauri v2 application hosting a multi-threaded Rust engine (`trans4mers-engine`) and a reactive React 18 / TypeScript frontend.
- **Local-First Model Execution**: Native integration with local Ollama daemons (e.g. `qwen2.5-coder:32b`, `deepseek-r1:32b`, `llama3.3:70b`) with Bring-Your-Own-Key (BYOK) support for frontier cloud models (Claude 3.7, GPT-4.5/o3-mini, Gemini 2.0 Pro) secured via OS Keyring vaulting.
- **Durable Event-Sourced Storage**: Pure CQRS/event-sourcing with SQLite in WAL mode, ensuring zero data loss across abrupt power termination.
- **Zero-Trust Governance**: 3D capability lattice, per-hunk unified diff review, anti-TOCTOU canonical argument hashing, and isolated git worktrees.
- **Comprehensive Workspace Shell**: Integrated 3-column desktop environment featuring Team Chat, Monaco Code Editor, interactive PTY Terminal (`portable-pty`), Chrome DevTools Protocol (CDP) Browser Live Mirror, Visual Swarm Designer, Fleet Dashboard, 4-Tier Memory Inspector, Document RAG, and MCP Protocol Inspector.
- **Zero Telemetry & Zero Egress by Default**: Trans4mers does not phone home, does not require an account, does not auto-download unauthorized assets, and can operate entirely severed from the public internet.

---

## 2. Product Vision & Sovereign Computing Principles

### 2.1 The Sovereign Agent Thesis
Cloud-based AI developer tools suffer from structural limitations: high ongoing operational expenses, data exfiltration risks, vendor lock-in, and unpredictable API latency. Trans4mers establishes a new standard for sovereign computing:

1. **Zero Marginal Cost Execution**: Running billions of tokens across local multi-agent swarms incurs zero recurring fees.
2. **Absolute Data Privacy**: Codebases, execution traces, cognitive memories, and agent debates remain strictly contained within local workstation SQLite databases and git repositories.
3. **Provable Durability**: State transitions occur through an append-only event ledger. Every agent step, tool call, and observation is committed inside a transactional write boundary before side-effects occur.
4. **Zero-Trust Human Oversight**: Agents are treated as untrusted actors. Destructive operations (file writes outside scratch, terminal commands, remote git pushes, memory mutations, credential reads) are intercepted by the `PolicyEngine` and require explicit human sign-off.

---

## 3. Target User Personas & Core Use Cases

### 3.1 User Personas
- **The Sovereign Systems Engineer**: Demands full offline autonomy to build, test, and refactor code without exposing enterprise intellectual property to third-party cloud servers.
- **The Security & Compliance Officer**: Requires complete auditability, explicit capability gating, secret scanning, and cryptographic anti-tamper assurances.
- **The Autonomous AI Researcher**: Leverages multi-agent debates, deep research iterative web crawling, and 4-tier cognitive memory to synthesize large volumes of technical data.

### 3.2 Core Use Cases
- **UC-1: Multi-Agent Feature Implementation**: Supervisor agents decompose macro engineering goals into sequential milestones, dispatching worker agents into isolated git worktrees (`agent/{sub_agent_id}`) to implement, test, and merge code.
- **UC-2: Adversarial Multi-Agent Debate**: Proponent and Adversarial Critic agents conduct multi-turn formal debates on architecture and security, synthesizing vetted consensus through an Executive Judge agent.
- **UC-3: Zero-Trust Code Refactoring with Diff Review**: An agent proposes code refactors; the operator inspects changes line-by-line in a side-by-side Monaco diff viewer, accepting or rejecting individual hunks.
- **UC-4: Continuous Heuristic Learning ("Teach Rule")**: When an operator rejects an agent action, the operator's feedback is distilled into a permanent constraint stored in procedural memory and injected into all future agent contexts.
- **UC-5: Deep Workspace Document RAG**: Querying thousands of workspace code files and markdown documents using hybrid retrieval (SQLite FTS5 BM25 + dense vector KNN) fused via Reciprocal Rank Fusion ($k=60$).
- **UC-6: Sovereign Web Automation & Live Mirroring**: Autonomous web interaction through isolated Chrome DevTools Protocol (CDP) spaces with real-time viewport streaming to the desktop UI.

---

## 4. Product Surface & Functional Specification

### 4.1 Workspace Shell & Navigation Architecture
The Trans4mers desktop interface is organized as a 3-column resizable layout:
- **Left Column**: Team Sidebar with conversation channels (`#general`) and direct-message agent avatars, or the Workspace File Explorer.
- **Center Column**: Multi-Tab Workspace hosting:
  - **Chat (`activeTab: 'chat'`)**: Channel stream with markdown formatting, code syntax highlighting, `@mention` agent dispatching, and `/schedule` cron automation.
  - **Code Editor (`activeTab: 'code'`)**: Embedded Monaco Editor with file tree navigation and immediate save synchronization.
  - **Swarm Map (`activeTab: 'swarm'`)**: Interactive ReactFlow canvas with Dagre auto-layout visualizing agent relationships, delegation trees, and live execution states.
  - **Fleet Dashboard (`activeTab: 'fleet'`)**: Live supervisory grid displaying active agent instances, step counters, token velocity, and immediate execution kill switches.
  - **Deep Research (`activeTab: 'research'`)**: Autonomous multi-query web research dashboard with iterative question decomposition and markdown report generation.
  - **Browser Live Mirror (`activeTab: 'browser'`)**: Dedicated viewport rendering isolated CDP browser sessions with point-in-time snapshot rollback controls.
  - **Memory Inspector (`activeTab: 'memory'`)**: Cognitive pyramid browser displaying Working, Episodic, Semantic, and Procedural memory tiers with bi-temporal validity tracking.
  - **Document RAG (`activeTab: 'docs'`)**: Workspace indexing panel showing chunk counts, SHA-256 file hashes, and hybrid search testing.
  - **Interactive Artifacts (`activeTab: 'artifacts'`)**: Review pane for generated plans, architecture diagrams, and briefs, featuring line-level comments that dispatch follow-up tasks to agent inboxes.
- **Right Column**: Live Inspector displaying selected agent profiles, capability matrices, and ReAct step execution logs, paired with the embedded PTY Terminal (`xterm.js`).

### 4.2 Multi-Agent Runtime & Swarm Topologies
- **ReAct State Machine**: 25-step execution loop executing thought extraction, dynamic tool manifest assembly, grammar constraints, streaming LLM inference, and self-healing recovery.
- **Supervisor-Worker Topology**: Hierarchical decomposition where a lead agent provisions worker agents, monitors milestones, and compiles a unified execution report.
- **Adversarial Debate Topology**: Structured 1-to-10 round debates between Proponent and Critic agents, concluded by an Executive Consensus Judge.
- **Parallel Fan-Out Topology**: High-throughput distributed task execution distributing batches across worker pools under concurrency caps.
- **Git Worktree Isolation**: Sub-agents execute within dedicated git worktrees (`.trans4mers/worktrees/{agent_id}`), merging changes back to the main branch upon task completion.

### 4.3 4-Tier Cognitive Memory & Hybrid RAG
- **Working Memory**: Real-time scratchpad holding transient observations.
- **Episodic Memory**: Checkpointed task execution logs and tool results.
- **Semantic Memory**: Distilled facts and domain knowledge indexed with dense embeddings.
- **Procedural Memory**: Permanent operational heuristics and learned constraint rules.
- **Hybrid Retrieval**: Parallel execution of SQLite FTS5 BM25 search and dense vector KNN (via `sqlite-vec` or `LanceDB`), merged using Reciprocal Rank Fusion ($k=60$).
- **Background Consolidation**: Hourly evaluation triggering Nightly Dreaming at 3:00 AM with credit protection guards, alongside 60-second conversation distillation sweeps.

### 4.4 Zero-Trust Governance & Security Trust Layer
- **3D Capability Lattice**: Strict classification of every tool by `EffectClass` (`ReadOnly`, `IdempotentMutation`, `NonIdempotentMutation`, `Unknown`), `RiskLevel` (`Safe`, `Low`, `Medium`, `High`, `Critical`), and fine-grained `Capability`.
- **4-Layer Policy Precedence**: Absolute DENY-wins evaluation across Agent, Project, and Global scopes.
- **DiffReviewer Trust Layer**: Longest Common Subsequence (LCS) dynamic programming generating unified diff hunks, coupled with mandatory secret scanning.
- **Anti-TOCTOU Canonical Argument Hashing**: SHA-256 digests over key-sorted canonical JSON arguments verified before tool dispatch.
- **Advisory File Lease Locks**: SQLite-backed 60-second TTL leases preventing concurrent file clobbering.

### 4.5 External Protocols & Hardware Tools
- **25 Native Built-in Tools**: Comprehensive primitives spanning filesystem I/O, git manipulation, terminal execution, CDP browser interaction, cognitive memory, MCP invocation, and dynamic delegation.
- **Model Context Protocol (MCP)**: Sovereign host supporting Stdio child processes and Streamable HTTP (SSE) remote servers, complete with live JSON-RPC frame logging and official MCP Inspector integration.
- **Native PTY Terminal**: High-performance pseudo-terminal sessions powered by `portable-pty` spawning native shells (`powershell.exe` on Windows, `bash` on Unix) streamed directly to `@xterm/xterm`.
- **Offline Model Guidance Catalog**: Zero-egress static directory profiling 22 model families with size classes, context limits, and tool-calling capabilities.

---

## 5. Non-Functional Requirements & Operating Constraints

| Requirement | Metric / Specification | Verification Target |
| :--- | :--- | :--- |
| **Cold Start Latency** | $< 1.5\text{ s}$ from executable launch to interactive UI | Verified on standard workstation SSD |
| **Event Append Latency** | $< 5\text{ ms}$ per committed CQRS event envelope | Synchronous SQLite WAL write transaction |
| **Default Network Egress** | Exactly $0$ bytes outbound | Loopback-only enforcement (`127.0.0.1`) |
| **Process Hygiene** | Zero orphaned background processes on exit | Guaranteed cleanup of Chromium and PTY processes |
| **Code Quality** | Zero compiler warnings under `-D warnings` | 100% compliant across all workspace crates |

---

## 6. Known Limitations & Technical Boundaries

1. **Local Parameter Ceilings**: Small local models (3B to 8B parameters) exhibit reasoning degradation on long-horizon engineering tasks. Trans4mers utilizes ReAct step bounding, self-healing classification, and human-in-the-loop diff review to catch deviations.
2. **Embedding Model Prerequisite**: Semantic RAG and vector memory require an active local embedding model (e.g. `nomic-embed-text` via Ollama). In its absence, the engine degrades gracefully to SQLite FTS5 lexical matching.
3. **Single-Operator Architecture**: Designed for single-operator workstations without multi-tenant cloud authentication or multi-user document synchronization.
4. **External Binary Prerequisites**: Browser automation requires local installation of Google Chrome or Chromium; MCP Inspector requires local Node.js and npx binaries.
