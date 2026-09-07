# Architecture Decisions

This document explains the WHY behind the non-obvious engineering choices. For the full multi-subsystem architecture blueprints and diagrams, see [**System Architecture Reference (docs/ARCHITECTURE.md)**](ARCHITECTURE.md).

## Locked decisions (recap)

- **SQLite + WAL + sqlite-vec** — zero-setup, embedded; single-writer correctness via `Mutex<Connection>`, concurrent readers via fresh WAL connections.
- **Event sourcing lite** — immutable `events` table is the source of truth; state tables are projections written in the SAME transaction as the event insert (`commit_and_emit`), then broadcast on the in-process bus after commit.
- **Stateless providers** — the providers crate has no storage dependency; `TokenMetrics` ride the response and the engine persists them.
- **Tauri 2 zero-scope filesystem** — the frontend FS plugin scope is EMPTY; all file access goes through `WorkspaceFileSystem` (traversal-proof, canonicalized containment).

## Deviations from the plan (with reasons)

1. **`async-trait` in domain** — the plan's `Tool` trait uses `#[async_trait]` but the domain Cargo.toml omitted the dependency. Added (dyn-safe async traits require it).
2. **Manual Display/FromStr/Serde for `EffectClass`/`Capability`** — strum cannot derive on enums with data fields (`IdempotentMutation { requires_idempotency_key }`, `Custom(String)`). Hand-written impls keep the plan's SQL string forms (`"ReadOnly"`, `"IdempotentMutation"`, `"custom:<name>"`).
3. **engine → providers dependency** — the plan's `AppState` (engine) holds `ProviderRegistry`, `EmbeddingProvider`, and `TerminalManager`, which are providers-crate types. The dependency graph therefore has engine depending on providers (no cycle; providers only depends on domain).
4. **Full-payload create events** — the plan's create events (e.g. `MessageCreated`) carried thin payloads that could not populate the state tables the same plan specifies. Create events now carry the full aggregate struct.
5. **Scheduler owns the cancellation map** — the plan sketched cancellation tokens in both the Scheduler and AppState; a single shared `Arc<DashMap>` (Scheduler-owned, AppState-referenced) eliminates the dual-source state inconsistency.
6. **Project registry in global.db (M003)** — the plan's global schema had no projects table, yet `create_project`/`list_projects` need a durable registry. Added migration M003 (gap-fill).
7. **Root `[features]` moved** — a virtual workspace manifest cannot declare `[features]`; the `e2e-tests` feature lives on `tests/integration`.
8. **`IdempotentMutationKeyed`** — `EffectClass::IdempotentMutation { requires_idempotency_key: true }` serializes to this string so the flag survives the SQL round-trip; the recovery check's `starts_with("IdempotentMutation")` still matches.

## Race-condition engineering (the interesting part)

- **Scheduler wake latch**: a wake that arrives while an execution is still finishing is latched and applied when the flow reports `Suspended` — never lost, never duplicated. `FlowOutcome::{Finished, Suspended}` distinguishes "yielding" from "done" so finished work is never restarted.
- **Permit ownership**: the permit is moved into the scheduler-spawned task wrapping the launcher future; it is released exactly when the execution (or suspension) completes.
- **Approval resume**: on re-run, the runtime checks for an existing Approved approval for (execution, tool) before re-evaluating policy — otherwise a resumed execution would ask forever.
- **Git merges**: SAFE checkout runs BEFORE the branch pointer moves (after the move, HEAD equals the target and checkout becomes a no-op). Force checkout is never used — conflicts surface as `ConflictDetected`.
- **Inbox at-least-once**: CLAIMED-but-unacked messages revert to QUEUED on crash; claim, ack, and revert are all single-statement transactions through the single-writer mutex.

## Event-sourcing invariant enforcement

`apply_projection` matches every `DomainEvent` variant with NO catch-all arm — adding an event variant without a projection is a compile error. The only direct state-table writes outside `commit_and_emit` are the sanctioned recovery/shutdown corrections (documented in the plan's own recovery code) and operational tables (checkpoints, steps, token usage, inbox transitions, policies, settings, learned rules, project registry).

## Vision-alignment pass (session 2) — fixes and gap-fills

The second build session cross-checked the full codebase against the product vision (Slack-like team chat, DMs, default-agent replies, delegation, browser/MCP/plugin/skill tooling, agent-driven UI customization). Everything below was implemented with integration coverage in `tests/integration/src/vision_features_test.rs`.

9. **Reply-channel plumbing** — agent replies previously always landed in the conversation's FIRST channel (a DM answer would appear in the blackboard). Inbox payloads now carry `channel_id`; the runtime, `message.send`, `delegate_task`, and `trigger_agent` all resolve the reply channel from the most recent claimed inbox message (DM stays a DM), falling back to the first channel.
10. **Default-agent routing** — a human message in the shared blackboard with no mentions routes to the conversation's default agent (earliest-joined active member). Agent senders never trigger default routing (prevents agent ping-pong loops); explicit mentions and DM routing apply to all senders.
11. **`complete_task` as a tool call** — the loop only recognized the plain-text `TASK_COMPLETE:` sentinel. A proper tool call to `complete_task` now terminates the execution, posts the summary, ACKs the inbox (both paths tested).
12. **`message.send` decoupled from the runtime** — the tool persisted nothing (a sentinel string + a name-based special case in the runtime did the posting). The tool now commits the message itself (event-sourced) into the resolved reply channel; the runtime special case is gone.
13. **Self-healing feedback loop actually wired** — tool failures (success=false, or Err after backoff retries) previously hard-failed the execution. They are now posted as ToolResult messages and the loop continues, so the LLM self-corrects (step-limited). F4 (stuck) escalates to the human. `E::ToolExecution` classifies as F2, not F4.
14. **Recovery re-queue** — `RecoveryManager::recover_all` marked executions Queued but nothing ever handed them back to the scheduler (durable-resume was broken at the wiring level). `requeue_pending` runs at startup; the integration test proves a crashed execution RESUMES and completes.
15. **Shutdown drain counter** — `ShutdownManager`'s running counter was never incremented; the drain wait was vacuous. The scheduler now maintains the counter per launched execution and shares it with the shutdown path. MCP servers and plugin processes are stopped on window close.
16. **MCP/plugin tool registration** — `McpToolAdapter`/`PluginToolAdapter` existed but were never instantiated (agents could not use MCP/plugin tools). Launch/start commands now register them (`mcp:<server>:<tool>`, `plugin:<id>:<tool>`) with family capabilities `custom:mcp` / `custom:plugin`; the context engine surfaces registered Mcp/Plugin tools to agents holding the family capability.
17. **Skills wired** — `skills/*.toml` were dead assets. A TOML skill loader + catalog (engine), startup loading (app), `list_skills`/`reload_skills` IPC, and context-engine injection via `default_skills` are all live. The UI shows the catalog in Settings.
18. **`delegate_task` tool (plan Phase 8.1)** — agent-callable delegation with bounded capability inheritance, membership, child execution + durable inbox routing, isolated git worktree, model-config inheritance (children run on the parent's provider), and a visible `DelegationRequest` message. Refusals (escalation attempts, unknown definitions, depth overflow) return as tool results so the agent can adapt.
19. **`shell.execute` tool** — real workspace-rooted command execution (timeout + output caps, `sh -c` / `cmd /C`), `NonIdempotentMutation` (never auto-retried), `ShellExecute` capability (policy default Ask).
20. **Browser tooling** — `BrowserManager` trait (plan decision #24) with a real reqwest-based implementation: HTTP(S) only, redirect-following, size caps, HTML text extraction (script/style skipped, entities decoded) and absolute link extraction. The `browser.fetch` agent tool wraps it (BrowserNavigate capability). CDP click/type automation is deliberately NOT faked — deferred until a real browser backend exists.
21. **Agent-driven UI preferences** — `ui.set_preference` tool (custom `UiControl` capability, `#RRGGBB`-validated) updates the SHARED in-memory `AppConfig` Arc, persists config.toml, and broadcasts `UiPreferenceChanged`; the app bridge emits `ui_preference_changed`; the frontend theme store applies CSS variables live. The Frontend Agent definition ships with the capability + tool.
22. **Tauri context ownership** — `generate_context!()` lived in the app LIBRARY crate, which has no tauri.conf.json (the config belongs to the desktop binary crate). `run()` now takes the context from the entry point: one config file, one owner. The capabilities file referenced `os:default`/`dialog:default` while the plugin crates were not dependencies of the desktop crate (build-time permission resolution) — fixed; the never-initialized `tauri-plugin-fs` and its dead config scope were removed.
23. **App-layer compile fixes** — the app crate had never been compiled (sandbox lacked GUI dev libs): broken `crate::parse_id` import paths (13 modules), a raw-SQL block referencing `rusqlite` without the dependency (moved into `TokenUsageRepo::list_by_agent`), a borrow-then-move of the app handle, and `AppState`/`Arc` construction mismatches. All fixed; the whole workspace now type-checks via a stub-pkg-config harness (metadata-only `.pc` files; no linking) and clippy is at ZERO warnings workspace-wide.

## Session 3 — user guarantees (local-LLM, conversation mutex, dynamic swarm)

The third pass turned the user's explicit runtime guarantees into enforced behavior, each with end-to-end coverage (`tests/integration/src/local_llm_test.rs` + `scheduler_test.rs`).

24. **No model auto-install (hard guarantee)** — the Ollama provider only ever calls `GET /api/tags`, `POST /api/chat`, and `POST /api/embeddings`. There is no `/api/pull` (or any install/copy endpoint) anywhere in the codebase; the mock-Ollama test asserts the full request log contains zero install calls. Model installation happens in the user's terminal only; the wizard and Settings merely LIST what is already installed.
25. **No hardcoded model names** — `ModelConfig::inherit()` is an empty provider/model marker meaning "resolve at runtime". Resolution chain: agent instance override → definition's user-chosen model (the legacy `llama3.1:8b` scaffold default is treated as NOT chosen) → app default (`general.default_model` from config.toml) → read-only auto-detect from the installed list (embedding-only models skipped, negative results expire after 60s so a model installed later is picked up without restart) → a clear, actionable error. The Boss Agent and every dynamically spawned child carry the inherit marker, so a user running `qwen2.5-coder:3b` gets exactly that model on every agent.
26. **One running conversation per project (conversation mutex)** — all agents in a project share one workspace, so concurrent conversations would overwrite each other's files. The scheduler (single-owner loop state, race-free) tracks `active_conversation` per project: entries from other conversations stay PENDING and launch automatically when the running conversation's executions all finish (nothing is lost — deferral, not rejection). Conversations in different projects still run in parallel. The `queue`/`wake`/completion channels now carry `conversation_id`; `complete()` is idempotent (a duplicate completion report can never double-decrement the mutex).
27. **Dynamic swarming, no template agents** — `seed_defaults_if_empty` now seeds EXACTLY ONE definition: the Boss Agent (orchestrator prompt, broad bounded-inheritance capability set, inherit model marker). The 3 previous template seeds (General/Frontend/Researcher) were removed. `delegate_task` was redesigned: the parent invents the specialist at call time (`agent_name`, `role`, `task_description`, optional `capabilities`/`system_instructions`); a NEW AgentDefinition is created and persisted per spawn (metadata `spawned_dynamically`), default tools are derived from the granted capabilities, and the child's system prompt is composed from the brief (self-contained — the child does not see the parent's conversation). An optional `definition` argument still allows instantiating user-authored definitions explicitly.
28. **Startup model warm-up** — the app spawns a background task after setup that resolves the default model (user setting or auto-detect). Failure is non-fatal: resolution retries lazily at the first agent run, so starting the app without Ollama running is safe.
29. **Settings cache invalidation** — `update_settings` clears the engine's default-model cache so a model change takes effect on the very next generation (no stale cached model).
30. **Test harness honesty** — the integration harness now mirrors the real app: `general.default_model` set by default (the wizard equivalent), and `new_agent(..., "inherit")` creates agents WITHOUT a model override so tests exercise the full resolution chain. `local_llm_test.rs` runs the ENTIRE stack over a real TCP mock-Ollama HTTP server: auto-detect → boss (default agent, inherit model) → dynamic `delegate_task` spawn → child writes a real file → both executions complete → token usage recorded from real HTTP responses → zero pull/install calls; plus the two failure paths (missing model 404 → execution fails cleanly; no Ollama at all → actionable error, never a hardcoded fallback).

---

## Remediation session decisions (31–50)

31. **wire_launcher is called in `build_app_state`** — the scheduler's execution launcher is installed in production code (the audit's C1: the shipped app silently ran zero agents). Test harness parity: the harness already wired it, so the app now matches the tested behavior exactly.

32. **Argument-scoped approvals (migration M004)** — `approvals.arguments_hash` (SHA-256 of canonical key-sorted JSON) makes an approval cover the EXACT arguments it was requested for. Legacy NULL-hash rows match any arguments (back-compat). Canonical serialization makes a re-issued identical call after resume hash identically.

33. **Approvals are fully event-sourced** — `resolve`/`expire` now go through `commit_and_emit` (ApprovalResolved carries feedback via serde-default), so replay reconstructs resolutions. Duplicate pending requests are REUSED, not re-created (dedup on execution+tool+args hash).

34. **Escalations are resumable** — the EscalateToHuman and Planning-failure paths persist `WaitingForMessage` + a checkpoint before suspending; the mention router WAKES WaitingForMessage executions on new human messages (WaitingForApproval stays with the approval flow). No zombie "Running" rows.

35. **Error-path integrity** — the outer flow handler no longer clobbers terminal statuses (Cancelled→Failed(0,0) fixed; step/generation preserved); `resolve_approval` wakes only genuinely-waiting executions; `cancel_execution` durably marks non-launched executions Cancelled AND removes them from the scheduler queue (new `Command::Cancel`).

36. **Panic-safe scheduler** — the launcher wrapper catches unwinds (AssertUnwindSafe) so a runtime panic releases the permit, project slot, and conversation mutex instead of deadlocking them forever; duplicate Queue commands are ignored (dedup like Wake). Enforced by tests.

37. **Per-project tool registry + WorkspaceRouter** — workspace-scoped tools are registered per project (the last-opened project can never rewire another project's tools); a new router maps agent → isolated worktree root so delegated children genuinely operate inside their worktrees. Filesystem/shell tools resolve the root per requesting agent at dispatch time.

38. **Worktree lifecycle at terminal state** — completed children merge + prune automatically; failed/cancelled children discard; conflict cases keep the worktree for manual resolution. All transitions post a SystemNotification to the channel.

39. **Real file locks** — `filesystem.write` acquires a durable lock_repo lease keyed `file:{project}:{path}` (60s TTL, brief retry, then an honest conflict error). Cross-agent last-write-wins corruption is gone.

40. **LOCAL-MODEL compaction** — `compact_with_local_llm` asks the same LLM that runs the agent to summarize the dropped window; every failure path degrades to boundary-safe extractive truncation (logged). All truncation is char-boundary-safe (the compactor/shell/browser byte-slice panics are fixed and regression-tested).

41. **Real vector memory** — EmbeddingService wires `/api/embeddings` end-to-end: memorize_rule + create_memory embed+upsert (dimension-guarded); context assembly embeds the current directives and runs scope-respecting KNN (semantic hits filtered to the visible set). Degradation without an embedding model is logged, never silent. `num_ctx` is sent to Ollama to match the compaction threshold.

42. **Workflow engine executes** — topological batch walk, parallel fan-out, real agent executions per AgentTask, HumanReview pauses (resum via IPC), Condition edges (ok/fail/node-name) with branch skipping, event-sourced run status + channel notifications.

43. **Hardened process management** — shell.execute kills timed-out processes (kill_on_drop); MCP requests are timeout-bounded, stderr is piped to the log, the handshake sends `notifications/initialized`, response ids match numeric AND string forms, and `shutdown_mcp_server` really kills the process. Plugin roundtrips verify response ids and skip non-JSON lines. The dead `generate_stream` path was REMOVED (latent tool-args bug) rather than left as a trap.

44. **Sandbox hardening** — writes refuse to go through symlinks (dangling-link escape closed), re-canonicalize the parent after mkdir (TOCTOU narrowing), reads are 10MiB-capped; browser fetch streams the body with a hard cap (no full buffering); Ollama list/embedding clients have bounded timeouts.

45. **Frontend contract fixes** — get_messages passes the required conversationId; `Actor::System`'s bare-string wire form is handled (the chat-pane TypeError is gone); terminal input/resize route through refs (no stale closures), close-then-reopen is explicit, the PTY starts in the project workspace.

46. **Live UI** — useEvent keeps the handler in a ref (stale-closure wipe fixed); agent_created events make dynamic spawns appear instantly; the swarm map pulses from live executions, node clicks open the step-tail + steering drawer; TaskList rows select executions; settings policies load the persisted rules; empty mention arrays pass null (backend parsing active); channels auto-select on conversation change; every invoke has an error path.

47. **New surfaces** — MCP / Plugins / Workflow / Git / Artifacts panels (real IPC, error states); BYOK key management (store/list/delete via OS keyring); 75 commands with exact TS parity.

48. **Config honesty** — ModelConfig::default() is the inherit marker (llama3.1:8b hardcoding removed everywhere; legacy persisted values are still treated as "not chosen" so old DBs keep working); provider model inventories are empty by default; ProjectSettings.default_model is None.

49. **Ops fixes** — release.yml installs frontend deps; ci.yml excludes the GTK-dependent crates from the logic test job (the desktop-build job compiles them WITH system deps); nightly CI pulls qwen2.5-coder:3b; skills + the example plugin are BUNDLED as Tauri resources and the loaders find them in packaged installs; the CSP no longer whitelists external endpoints; installers ask before installing Ollama and never install models; engine git2 uses default-features=false.

50. **Housekeeping is real** — a 60s background task expires stale approvals, purges expired ephemeral memories, prunes checkpoints (keep 3 per execution), and cleans orphaned worktrees; one corrupted project DB is skipped at startup instead of aborting the app; the writer-mutex poisoning is recovered; project DB open is race-free (DashMap entry API).

51. **LIVE concurrency settings (F06 closed)** — ProjectSettings.max_concurrent_agents, ConversationSettings.max_concurrent_agents, and GeneralConfig.max_global_concurrent_agents are now ENFORCED, not decorative: the scheduler takes SetProjectCap / SetConversationCap / SetGlobalLimit commands; open_project, create_project (via open_project), update_project_settings, and update_settings push the caps at runtime. The global limit grows instantly and shrinks by stopping NEW launches only (in-flight executions drain; permits are never yanked). start_execution also propagates the conversation's cap before queueing. All covered by scheduler tests (live caps + shrink/grow).

52. **Full model-resolution chain (F06 model side)** — instance override > conversation settings override > definition (explicit, legacy-scaffold excluded) > PROJECT default (ProjectSettings.default_provider/default_model — the project settings UI's model input is now live) > app default (wizard/auto-detect). Every resolved config whose provider_endpoint is absent inherits the endpoint from the app config's provider entry (a custom Ollama URL in config.toml now applies to every level, not just auto-detect). E2E-proven by mock-Ollama request-log assertions (project model wins over auto-detect; conversation override wins over project).

53. **Step-counter fidelity (F14)** — ExecutionStepCompleted now persists `current_step` (new ExecutionRepo::update_step_row); Completed rows no longer lag the runtime by one step. Asserted in the e2e project-model test (completed row carries current_step == 1).

54. **Delegate honesty (F11)** — delegate_task rejects unparseable capability strings as a tool ERROR naming every bad string and listing the valid names (previously silently dropped: the parent believed a capability was granted when it wasn't). The child's step budget honors the project's max_execution_steps instead of a hardcoded 30.

55. **Starvation guard** — the pending queue's effective priority includes a bounded age bonus (+1 per 1024 epochs, cap +64), so a continuous stream of newer wake-boosted entries can never permanently starve a deferred conversation's entries (deferral under the one-conversation-per-project mutex is by design; starvation was not).

56. **Gemini function calling (F13)** — the Google provider maps the unified tool schema to `tools: [{functionDeclarations}]` (previously tools were silently dropped for Google BYOK agents); response parsing already handled functionCall parts. Unit-tested on the mapping shape.

57. **Ollama request hygiene** — `keep_alive: "5m"` keeps the model warm between swarm steps; `num_predict` is omitted when no max_output_tokens is set (explicit null removed); num_ctx unchanged.

58. **Terminal dependency migration** — xterm/xterm-addon-fit (deprecated, unmaintained) replaced by @xterm/xterm 6 + @xterm/addon-fit (same API, maintained). tsc/vitest/vite all green on the new packages.
