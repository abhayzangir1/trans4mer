# Trans4mers System Architecture Reference

Welcome to the definitive architecture specification for **Trans4mers**, a sovereign multi-agent desktop operating system. This document is written for engineers, contributors, and systems architects who want to understand the raw, unadorned reality of how Trans4mers operates across every layer of its stack.

---

## Table of Contents

1. [Crate Topology & Workspace Dependency Architecture](#1-crate-topology--workspace-dependency-architecture)
2. [Event-Sourced CQRS & Durability Subsystem](#2-event-sourced-cqrs--durability-subsystem)
3. [Agent Runtime & Self-Healing Execution Loop](#3-agent-runtime--self-healing-execution-loop)
4. [Multi-Agent Swarm Orchestration & Concurrency Model](#4-multi-agent-swarm-orchestration--concurrency-model)
5. [4-Tier Cognitive Memory Pyramid & Hybrid RAG](#5-4-tier-cognitive-memory-pyramid--hybrid-rag)
6. [Zero-Trust Policy Engine & Diff Review Subsystem](#6-zero-trust-policy-engine--diff-review-subsystem)
7. [PTY Terminal, CDP Browser & Extensibility Subsystems](#7-pty-terminal-cdp-browser--extensibility-subsystems)
8. [Tauri v2 Desktop Shell & Frontend State Projection](#8-tauri-v2-desktop-shell--frontend-state-projection)

---

## 1. Crate Topology & Workspace Dependency Architecture

Trans4mers is organized as a unified Cargo virtual workspace consisting of five core backend crates, one Tauri v2 native desktop application wrapper, and a TypeScript/React frontend shell.

### Crate Dependency Graph

```mermaid
flowchart TD
    subgraph UI ["Presentation Layer"]
        Frontend["apps/desktop/src (React 18 + TS + Tailwind)"]
        TauriShell["apps/desktop/src-tauri (Tauri v2 Shell)"]
    end

    subgraph AppLayer ["Application & IPC Layer"]
        App["core/trans4mers-app (22 Command Modules + EventForwarder)"]
    end

    subgraph EngineLayer ["Execution & Intelligence Layer"]
        Engine["core/trans4mers-engine (Scheduler, Runtime, Swarms, RAG, Memory)"]
    end

    subgraph ProviderLayer ["Hardware & Inference Abstraction Layer"]
        Providers["core/trans4mers-providers (Ollama, Anthropic, OpenAI, Gemini, CDP, MCP)"]
    end

    subgraph PersistenceLayer ["Storage & Ledger Layer"]
        Storage["core/trans4mers-storage (SQLite, LanceDB, Migrations, 27 Repos)"]
    end

    subgraph DomainLayer ["Pure Domain Core"]
        Domain["core/trans4mers-domain (Entities, IDs, Events, Configs, Errors)"]
    end

    Frontend <-->|"Tauri IPC: Commands and Events"| TauriShell
    TauriShell --> App
    App --> Engine
    Engine --> Storage
    Engine --> Providers
    Providers --> Domain
    Storage --> Domain
    Engine --> Domain
    App --> Domain
```

### Architectural Boundaries & Invariants

1. **`trans4mers-domain`**: The bedrock crate. It has zero internal workspace dependencies and contains only pure data structures, strongly-typed identifiers ([`ProjectId`](../core/trans4mers-domain/src/ids.rs), [`ExecutionId`](../core/trans4mers-domain/src/ids.rs), [`AgentInstanceId`](../core/trans4mers-domain/src/ids.rs)), the canonical [`DomainEvent`](../core/trans4mers-domain/src/event.rs) enum, and capability definitions.
2. **`trans4mers-storage`**: The sole manager of persistence. Contains raw SQL migration scripts ([`global`](../core/trans4mers-storage/src/migrations/global/) and [`project`](../core/trans4mers-storage/src/migrations/project/)), single-writer transaction wrappers, repository query implementations, and vector store adapters (`SqliteVecStore` and `LanceDbStore`).
3. **`trans4mers-providers`**: Stateless inference and external protocol drivers. It provides standard trait implementations for Ollama, Anthropic, OpenAI, Google Gemini, Chromium DevTools Protocol (CDP), and the Model Context Protocol (MCP). It does not hold database connections; token consumption metrics ride back on the response payloads.
4. **`trans4mers-engine`**: The operational brain. Manages the concurrency scheduler, the ReAct agent execution loop, the hybrid RAG retrieval pipeline, memory tier transitions, swarm debates, and the workspace worktree router.
5. **`trans4mers-app`**: Bridges Rust engine services to Tauri IPC handlers. Houses 22 distinct command modules and the [`EventForwarder`](../core/trans4mers-app/src/event_forwarder.rs) background emitter.
6. **`apps/desktop`**: Native cross-platform desktop UI constructed with React 18, Zustand, TailwindCSS, `@xterm/xterm`, and `@monaco-editor/react`.

---

## 2. Event-Sourced CQRS & Durability Subsystem

Trans4mers rejects traditional CRUD architectures in favor of an **Event-Sourced Command Query Responsibility Segregation (CQRS)** pattern. The single source of truth for the entire operating system is the append-only `events` ledger.

### Write Path: Atomic Commit & Projection Loop

```mermaid
sequenceDiagram
    autonumber
    participant Agent as Agent Execution
    participant AppState as AppState
    participant DB as SQLite DB
    participant Projector as EventProjector
    participant Bus as EventBus
    participant Forwarder as EventForwarder
    participant UI as Desktop UI

    Agent->>AppState: commit_event(DomainEvent, ActorId)
    AppState->>DB: Begin Write Transaction (WAL Mode)
    DB->>DB: INSERT INTO events (sequence_id, payload, actor, timestamp)
    AppState->>Projector: project_event(&tx, &envelope)
    Projector->>DB: UPDATE / INSERT state tables (agents, messages, diffs)
    AppState->>DB: COMMIT TRANSACTION
    AppState->>Bus: publish(Arc<EventEnvelope>)
    Bus->>Forwarder: recv()
    Forwarder->>UI: emit("domain_event", json_envelope)
    UI->>UI: React state update (MessageList, SwarmMap, DiffReviewer)
```

### Database Topology

Trans4mers maintains two distinct SQLite database tiers per workstation:

| Database | Location | Scope | Lifecycle & Contents |
| :--- | :--- | :--- | :--- |
| **Global DB** | `~/.trans4mers/global.sqlite` | Workstation-wide | Persists registered projects, global agent templates, MCP server registry, credential keyrings, and system settings. |
| **Project DB** | `<workspace>/.trans4mers/project.sqlite` | Per-Project Workspace | Persists immutable `events`, conversations, channels, execution traces, diff reviews, approvals, 4-tier memory vectors, document chunks, and FTS5 indices. |

### The `commit_and_emit` Invariant

Every state mutation executes inside the single-writer transaction function:
```rust
// core/trans4mers-engine/src/cqrs.rs
pub fn commit_event(
    tx: &Transaction,
    event: DomainEvent,
    actor_id: ActorId,
) -> Result<EventEnvelope, Trans4mersError> {
    // 1. Serialize and Append to Event Store (Immutable Log)
    let sequence_id = EventRepo::insert(tx, &envelope)?;
    envelope.sequence_id = sequence_id;

    // 2. Apply Projections to Relational Models (Synchronous within transaction)
    EventProjector::project_event(tx, &envelope)?;

    Ok(envelope)
}
```

If power fails or the process is killed at any nanosecond, SQLite rollbacks guarantee that partial projections never exist without their underlying event.

### Cold-Boot Crash Recovery

Upon startup, the [`RecoveryManager`](../core/trans4mers-engine/src/recovery_manager.rs) inspects the database before launching any services:
1. Identifies any executions left in `Running` status without an exit event.
2. Replays the event log from the last saved `CheckpointSaved` boundary.
3. Restores working memory scratchpads and re-queues pending executions into the [`Scheduler`](../core/trans4mers-engine/src/scheduler.rs).
4. Prunes dangling git worktrees and releases stale file lease locks.

---

## 3. Agent Runtime & Self-Healing Execution Loop

Each autonomous agent operates as an asynchronous ReAct (Reasoning + Acting) state machine.

### The ReAct Cycle & Decision Flow

```mermaid
flowchart TD
    Start([Task Triggered / Queued]) --> InboxCheck{Check Agent Inbox}
    InboxCheck -->|Message Claimed| AssembleContext[Assemble Context via ContextEngine]
    InboxCheck -->|Empty| WaitSlot[Wait for Next Execution Slot]
    
    AssembleContext --> ContextCompaction{Token Count Near Ceiling?}
    ContextCompaction -->|Yes| RunCompactor[ContextCompactor: Local LLM Summarization]
    ContextCompaction -->|No| LLMInference[Call LLM Provider via Streaming API]
    RunCompactor --> LLMInference

    LLMInference --> ParseThought["Parse Thought and Tool Invocations"]
    ParseThought --> ToolDecision{Tool Call Emitted?}
    
    ToolDecision -->|No / Complete| CompleteTask["Complete Task and Post Summary"]
    ToolDecision -->|Yes| PolicyCheck{Evaluate PolicyEngine}

    PolicyCheck -->|Deny| RejectTool["Inject Policy Violation into Context"]
    PolicyCheck -->|Ask| HumanGate["Generate ActionDiff and Yield Permit"]
    PolicyCheck -->|Allow| ExecuteTool["Acquire File Lock and Execute Tool"]

    HumanGate --> HumanResolution{Operator Decision}
    HumanResolution -->|Approved| ExecuteTool
    HumanResolution -->|Rejected| InjectFeedback["Feed Operator Feedback into Next Turn"]
    HumanResolution -->|Teach Rule| DistillRule["Store Rule in Procedural Memory"]
    DistillRule --> InjectFeedback

    ExecuteTool --> ToolResult{Execution Result}
    ToolResult -->|Success| CommitObs["Commit Observation Event"]
    ToolResult -->|Transient Failure| BackoffRetry["Exponential Backoff + Jitter"]
    ToolResult -->|Logic Failure| SelfHeal["Feed Error Trace to LLM to Self-Correct"]

    BackoffRetry --> ExecuteTool
    SelfHeal --> AssembleContext
    InjectFeedback --> AssembleContext
    RejectTool --> AssembleContext
    CommitObs --> IncrementStep["Increment Step Counter and Check Budget"]
    IncrementStep --> LLMInference
```

### Dynamic Model Resolution Chain

Trans4mers never forces hardcoded models. Models resolve dynamically at runtime through a strict 6-tier hierarchy:

```
[Agent Instance Override]
       │ (if None)
       ▼
[Conversation Settings Override]
       │ (if None)
       ▼
[Agent Definition Explicit Model]
       │ (if inherited / default)
       ▼
[Project Default Model (project_settings)]
       │ (if None)
       ▼
[Workstation App Default (config.toml)]
       │ (if unreachable)
       ▼
[Local Ollama Auto-Discovery Scan]
```

### Self-Healing Failure Classification

The runtime handles errors according to four distinct failure classes:

| Class | Type | Response Strategy |
| :--- | :--- | :--- |
| **F1** | Transient Transport Error (HTTP 429, 503, socket timeout) | Exponential backoff with random jitter (up to 3 retries) without burning step budget. |
| **F2** | Tool Execution Failure (Exit code $\ne 0$, file not found, bad args) | Result is formatted as a structured `ToolResult` observation; LLM receives error output to plan a corrective step. |
| **F3** | Malformed Output / Schema Mismatch | Prompt is augmented with expected JSON schema constraints and immediately re-prompted. |
| **F4** | Loop Stall / Repetitive Thoughts | Cycle detection detects identical consecutively repeated thoughts and escalates to the operator with a `WaitingForMessage` state. |

---

## 4. Multi-Agent Swarm Orchestration & Concurrency Model

Trans4mers supports dynamic, multi-agent teams executing under strict hardware and workspace concurrency guarantees.

### 3-Level Concurrency Hierarchy

```mermaid
graph TD
    subgraph L1 ["Level 1: Workstation Global Limit"]
        GlobalPermits["Global Semaphore (Default: 8 Permits)"]
    end

    subgraph L2 ["Level 2: Project Concurrency Cap"]
        ProjectA["Project A Cap (e.g. 4)"]
        ProjectB["Project B Cap (e.g. 4)"]
    end

    subgraph L3 ["Level 3: Conversation Mutex"]
        Convo1["Conversation 1 (Active)"]
        Convo2["Conversation 2 (Pending Deferred)"]
    end

    GlobalPermits --> ProjectA
    GlobalPermits --> ProjectB
    ProjectA --> Convo1
    ProjectA -.->|Locked out until Convo 1 finishes| Convo2
```

### The Conversation Mutex Rationale

All agents operating within a project share the underlying filesystem workspace. Allowing multiple concurrent conversations to mutate the workspace simultaneously causes git index lock contention and conflicting edits. 

The [`Scheduler`](../core/trans4mers-engine/src/scheduler.rs) enforces a strict **Conversation Mutex**:
- Exactly **one conversation** per project can hold active execution permits.
- Other conversations in the same project are queued as `Pending`.
- To prevent starvation, pending entries accumulate an **age bonus** (+1 effective priority per 1,024 epochs) so old tasks cannot be indefinitely delayed by rapid incoming wakes.
- Different projects execute concurrently without restriction up to the workstation's global permit ceiling.

### Swarm Collaboration Topologies

```mermaid
flowchart LR
    subgraph Sup ["Supervisor-Worker Pattern"]
        Boss["Supervisor Agent"] -->|Delegate Task 1| W1["Worker: Coding"]
        Boss -->|Delegate Task 2| W2["Worker: Research"]
        W1 -->|Result| Boss
        W2 -->|Result| Boss
    end

    subgraph Deb ["Adversarial Debate Pattern"]
        Prop["Proponent Agent"] <-->|"Rounds 1 to N"| Opp["Opponent Agent"]
        Prop --> Synth["Synthesis / Consensus Judge"]
        Opp --> Synth
    end

    subgraph Fan ["Parallel Fan-Out Pattern"]
        Lead["Lead Orchestrator"] --> Split["Task Splitter"]
        Split --> F1["Shard 1"]
        Split --> F2["Shard 2"]
        Split --> F3["Shard 3"]
        F1 --> Merge["Synthesizer"]
        F2 --> Merge
        F3 --> Merge
    end
```

---

## 5. 4-Tier Cognitive Memory Pyramid & Hybrid RAG

Memory in Trans4mers is structured into four distinct cognitive tiers, mirroring human cognitive architecture:

```mermaid
flowchart TB
    subgraph Pyramid ["Cognitive Memory Pyramid"]
        T1["Tier 1: Working Memory (In-Flight Context & Ephemeral Scratchpad)"]
        T2["Tier 2: Episodic Memory (Checkpointed Task Executions & Tool Traces)"]
        T3["Tier 3: Semantic Memory (Extracted Facts, Invariants & 768-dim Vectors)"]
        T4["Tier 4: Procedural Memory (Distilled Rules, Human Corrections & Skills)"]
    end

    T1 -->|Task Completion| T2
    T2 -->|Nightly Dreaming / Distillation| T3
    T2 -->|Operator Teach Rule / Feedback| T4
```

### Hybrid RAG Retrieval (FTS5 + Vector Reciprocal Rank Fusion)

When searching documents or memories, Trans4mers runs lexical and vector queries concurrently, merging the ranking via **Reciprocal Rank Fusion (RRF)**:

$$\text{RRF\_Score}(d) = \frac{1}{60 + \text{Rank}_{\text{BM25}}(d)} + \frac{1}{60 + \text{Rank}_{\text{Vector}}(d)}$$

```mermaid
flowchart LR
    Query["Search Query"] --> FTS["SQLite FTS5 (BM25 Lexical Search)"]
    Query --> Embed["Embedding Model (768-dim)"]
    Embed --> Vec["sqlite-vec (Cosine Similarity)"]
    FTS --> RRF["RRF Fusion Algorithm"]
    Vec --> RRF
    RRF --> Filter["Visibility & Scope Security Filter"]
    Filter --> Results["Final Ranked Context Chunks"]
```

---

## 6. Zero-Trust Policy Engine & Diff Review Subsystem

Agents are treated as untrusted actors. Dangerous operations cannot execute without passing through cryptographic verification and policy inspection.

### Capability Lattice & Action Diff Pipeline

```mermaid
sequenceDiagram
    participant Agent as Agent Runtime
    participant Policy as PolicyEngine
    participant DiffReview as DiffReviewer
    participant DB as Project DB
    participant UI as Diff Review Drawer

    Agent->>Policy: evaluate_policy(agent_id, capability, tool, arguments)
    alt Policy == Allow
        Policy-->>Agent: Proceed to Execution
    else Policy == Deny
        Policy-->>Agent: Execution Aborted (Policy Violation)
    else Policy == Ask
        Policy->>DiffReview: compute_file_hunks(old_text, new_text)
        DiffReview->>DiffReview: Compute SHA-256 arguments_hash
        DiffReview->>DB: INSERT INTO approvals (status="pending", arguments_hash)
        DiffReview->>DB: INSERT INTO action_diffs (hunks, additions, deletions)
        Agent->>Agent: Drop Permit & Suspend (WaitingForApproval)
        DB-->>UI: Live Approval Notification
        Note over UI: Operator inspects unified diff line-by-line
        UI->>DB: resolve_approval(approval_id, approved=true/false)
        DB-->>Agent: Wake Agent & Resume with Permit
    end
```

### SHA-256 Canonical Argument Hashing

To prevent Time-of-Check to Time-of-Use (TOCTOU) attacks, approval records store a canonical hash:
$$\text{arguments\_hash} = \text{SHA-256}(\text{CanonicalKeySortedJSON}(\text{arguments}))$$

When execution resumes, the runtime verifies that the arguments executing are byte-for-byte identical to the arguments the human operator approved.

---

## 7. PTY Terminal, CDP Browser & Extensibility Subsystems

Trans4mers provides native operating system primitives without intermediate web containers.

### Subsystem Capabilities

```mermaid
flowchart TD
    subgraph PTY ["Native PTY Terminal Subsystem"]
        PtySystem["portable-pty Native Substrate"]
        PtySystem -->|Duplex Stream| ShellProcess["powershell.exe / bash"]
        ShellProcess -->|Output Events| EventBus["EventBus: TerminalOutput"]
        EventBus -->|Tauri IPC| Xterm["@xterm/xterm UI Component"]
    end

    subgraph CDP ["Isolated Chrome DevTools Protocol (CDP)"]
        Chromium["chromiumoxide Headless / Headful Controller"]
        Chromium -->|Isolated Profiles| Profiles[".trans4mers/browser_profiles/<space_id>"]
        Chromium -->|DOM Extraction| Markdown["MarkdownExtractor (HTML -> Clean MD)"]
        Chromium -->|Live Viewport| LiveMirror["LiveMirror Canvas Mirror"]
    end

    subgraph Ext ["Extensibility Engine"]
        MCP["Model Context Protocol (MCP) Client"]
        Plugins["External Python / JSON-RPC Runner"]
        Skills["Declarative TOML Skill Loader"]
        MCP -->|stdio / SSE| McpInspector["Live MCP Inspector"]
    end
```

---

## 8. Tauri v2 Desktop Shell & Frontend State Projection

The presentation layer is fully decoupled from engine execution.

### Frontend Architecture Breakdown

```mermaid
flowchart TD
    subgraph TauriApp ["apps/desktop"]
        subgraph Stores ["Zustand Reactive State Stores"]
            ProjectStore["projectStore (Active Project & Workspace)"]
            ChatStore["conversationStore (Channels, Messages, Mentions)"]
            SwarmStore["swarmStore (Nodes, Edges, Execution Status)"]
            ApprovalStore["approvalStore (Pending Diffs & Reviews)"]
            TerminalStore["terminalStore (Active PTY Sessions)"]
        end

        subgraph Components ["UI Surface Components"]
            ChatView["Team Chat & Direct Messages"]
            SwarmCanvas["Visual Swarm Map (ReactFlow Canvas)"]
            DiffDrawer["Side-by-Side Diff Review Drawer"]
            MonacoPane["Monaco Code Editor & File Tree"]
            TerminalPane["PTY Terminal Shell: xterm.js"]
            MirrorPane["CDP Browser Live Mirror"]
        end
    end

    TauriEvents["Tauri Event Stream: domain_event"] --> Stores
    Stores --> Components
```

### Security Boundary: Zero-Scope Filesystem

In `tauri.conf.json`, the standard Tauri filesystem plugin scope is configured to **completely empty**. 

Frontend code cannot read or write arbitrary files on your workstation. All filesystem queries must pass through the `WorkspaceFileSystem` in Rust, which canonicalizes paths and strictly bounds operations to the project's authorized root directory.

---

## Summary Matrix

| Subsystem | Primary Technology | Resilience Guarantee | Sovereign Boundary |
| :--- | :--- | :--- | :--- |
| **Storage** | SQLite + WAL + sqlite-vec | Synchronous write transactions; zero data loss on abrupt termination | Local `.sqlite` files; zero cloud databases |
| **Event Ledger** | Append-only `events` table | Strict event sourcing; projections derived deterministically | Local storage only; immutable audit trail |
| **Concurrency** | Tokio Semaphore + DashMap | Starvation-resistant age-boost queue; Conversation Mutex | Workstation CPU/GPU thread allocation |
| **Memory** | FTS5 BM25 + Vector RRF | Checkpointed turn history; automated nightly consolidation | Local embeddings (`nomic-embed-text`); zero egress |
| **Governance** | SHA-256 Hunk Diff Review | Deterministic AST secret scanning; operator approval gating | Cryptographic verification on every mutation |
| **Terminal** | `portable-pty` + `@xterm/xterm` | Dedicated child process isolation per session | Local workstation shells (`powershell.exe`/`bash`) |
| **Browser** | `chromiumoxide` CDP | Per-space profile directory isolation | Isolated local Chromium processes; no remote sync |
