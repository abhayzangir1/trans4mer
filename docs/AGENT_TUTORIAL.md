# Agent Tutorial

## Your first agent

1. **Launch the app** — the setup wizard verifies Ollama, creates a project, and seeds the **Boss Agent** as the conversation's default orchestrator. There are NO other pre-made agents: every specialist is spawned dynamically.
2. **Say something** — type "read README.md and summarize it" in the chat. A message without an @mention is answered by the boss.
3. **Watch the ReAct loop** — the Agent panel shows the semantic step trace (reasoning, tool calls, results) with durations; the Swarm Map node pulses while the agent runs and its drawer tails the live steps.
4. **Approve carefully** — the first file write triggers an approval card. Approvals are ARGUMENT-SCOPED: approving a write to one file does not authorize writes to other files. Set a capability to Allow in Settings → Policies for trusted agents (the selects show the PERSISTED rules).

## The completion protocol

Agents declare completion by calling the `complete_task` tool with a summary. Small local models sometimes wrap this in plain text; the runtime also accepts the `TASK_COMPLETE:` sentinel prefix, so degenerate outputs cannot loop forever (empty responses count as completion too).

## Mentions

Address a specific agent with `@<name>` (typeahead) or `@<agent-id-prefix>` in the chat. The mention router delivers the message to that agent's durable inbox and wakes it if idle; running agents pick the message up on their next loop iteration; agents waiting on a human (escalations) are RESUMED by your message.

## Delegation (worktrees)

When a parent agent spawns a child (bounded to the parent's capabilities), the child's filesystem and shell tools operate INSIDE its own git worktree (`.trans4mers/worktrees/<agent-id>`). When the child's execution finishes, the engine merges its branch back automatically (explicit outcomes: `Success`, `UpToDate`, or `ConflictDetected` — never force-resolved) and prunes the worktree. Children that fail or are cancelled have their worktrees discarded; conflict cases keep the worktree for manual resolution.

## Memory and learning

- Agents see memories scoped by the 6-level hierarchy (Global/Project/Conversation/Agent/Ephemeral/Artifact).
- When you correct an agent, it may call `memorize_rule` (requires approval by default). Approved rules are injected into future prompts via 3-tier RAG: global rules, structural filters (language/file pattern), then vector similarity.

## Pausing, resuming, retiring

- **Pause** — the agent finishes its current step and stops picking up new executions (identity stays Active-in-app but no runs start while paused).
- **Retire** — the agent is archived; its history, steps, and artifacts remain queryable.
