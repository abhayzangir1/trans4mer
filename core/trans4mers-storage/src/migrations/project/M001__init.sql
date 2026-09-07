CREATE TABLE IF NOT EXISTS _schema_migrations (
    version TEXT PRIMARY KEY,
    applied_at DATETIME NOT NULL DEFAULT CURRENT_TIMESTAMP
);

CREATE TABLE IF NOT EXISTS conversations (
    id TEXT PRIMARY KEY,
    project_id TEXT NOT NULL,
    title TEXT NOT NULL,
    status TEXT NOT NULL,
    settings TEXT NOT NULL,
    created_at DATETIME NOT NULL DEFAULT CURRENT_TIMESTAMP,
    updated_at DATETIME NOT NULL DEFAULT CURRENT_TIMESTAMP
);

CREATE TABLE IF NOT EXISTS messages (
    id TEXT PRIMARY KEY,
    conversation_id TEXT NOT NULL,
    channel_id TEXT NOT NULL,
    thread_id TEXT,
    sender_actor TEXT NOT NULL,
    content TEXT NOT NULL,
    message_kind TEXT NOT NULL,
    mentions TEXT NOT NULL,
    attachments TEXT NOT NULL,
    requires_approval BOOLEAN NOT NULL DEFAULT 0,
    approval_id TEXT,
    created_at DATETIME NOT NULL DEFAULT CURRENT_TIMESTAMP,
    updated_at DATETIME NOT NULL DEFAULT CURRENT_TIMESTAMP,
    FOREIGN KEY(conversation_id) REFERENCES conversations(id)
);

CREATE TABLE IF NOT EXISTS agent_instances (
    id TEXT PRIMARY KEY,
    project_id TEXT NOT NULL,
    definition_id TEXT NOT NULL,
    parent_instance_id TEXT,
    status TEXT NOT NULL,
    capabilities TEXT NOT NULL,
    model_config_override TEXT,
    depth_level INTEGER NOT NULL,
    created_at DATETIME NOT NULL DEFAULT CURRENT_TIMESTAMP,
    updated_at DATETIME NOT NULL DEFAULT CURRENT_TIMESTAMP
);

CREATE TABLE IF NOT EXISTS inbox_messages (
    id TEXT PRIMARY KEY,
    recipient_agent_id TEXT NOT NULL,
    sender_actor_id TEXT NOT NULL,
    payload TEXT NOT NULL,
    delivery_state TEXT NOT NULL DEFAULT 'QUEUED',
    execution_id TEXT,
    created_at DATETIME NOT NULL DEFAULT CURRENT_TIMESTAMP,
    claimed_at DATETIME,
    FOREIGN KEY(recipient_agent_id) REFERENCES agent_instances(id)
);

CREATE TABLE IF NOT EXISTS agent_executions (
    id TEXT PRIMARY KEY,
    agent_instance_id TEXT NOT NULL,
    task_id TEXT,
    conversation_id TEXT NOT NULL,
    status TEXT NOT NULL,
    generation INTEGER NOT NULL DEFAULT 0,
    current_step INTEGER NOT NULL DEFAULT 0,
    max_steps INTEGER NOT NULL DEFAULT 50,
    trigger_message_id TEXT,
    started_at DATETIME,
    updated_at DATETIME NOT NULL DEFAULT CURRENT_TIMESTAMP,
    completed_at DATETIME,
    FOREIGN KEY(agent_instance_id) REFERENCES agent_instances(id)
);

CREATE TABLE IF NOT EXISTS execution_steps (
    id TEXT PRIMARY KEY,
    execution_id TEXT NOT NULL,
    step_number INTEGER NOT NULL,
    step_type TEXT NOT NULL,
    action_intent TEXT NOT NULL,
    tool_call_request TEXT,
    tool_result_summary TEXT NOT NULL,
    decision_rationale TEXT NOT NULL,
    token_usage TEXT,
    duration_ms INTEGER NOT NULL,
    created_at DATETIME NOT NULL DEFAULT CURRENT_TIMESTAMP,
    FOREIGN KEY(execution_id) REFERENCES agent_executions(id)
);

CREATE TABLE IF NOT EXISTS execution_checkpoints (
    id TEXT PRIMARY KEY,
    execution_id TEXT NOT NULL UNIQUE,
    generation INTEGER NOT NULL,
    step_number INTEGER NOT NULL,
    execution_status TEXT NOT NULL,
    execution_phase TEXT NOT NULL,
    inbox_cursor TEXT,
    pending_tool_state TEXT,
    last_event_sequence INTEGER NOT NULL,
    context_snapshot TEXT,
    created_at DATETIME NOT NULL DEFAULT CURRENT_TIMESTAMP,
    FOREIGN KEY(execution_id) REFERENCES agent_executions(id)
);

CREATE TABLE IF NOT EXISTS agent_execution_locks (
    agent_instance_id TEXT PRIMARY KEY,
    execution_id TEXT NOT NULL,
    locked_at DATETIME NOT NULL DEFAULT CURRENT_TIMESTAMP,
    expires_at DATETIME NOT NULL,
    FOREIGN KEY(agent_instance_id) REFERENCES agent_instances(id),
    FOREIGN KEY(execution_id) REFERENCES agent_executions(id)
);

CREATE TABLE IF NOT EXISTS domain_events (
    sequence_id INTEGER PRIMARY KEY AUTOINCREMENT,
    event_id TEXT NOT NULL UNIQUE,
    event_type TEXT NOT NULL,
    payload TEXT NOT NULL,
    actor_id TEXT NOT NULL,
    created_at DATETIME NOT NULL DEFAULT CURRENT_TIMESTAMP
);

CREATE TABLE IF NOT EXISTS project_memories (
    id TEXT PRIMARY KEY,
    project_id TEXT NOT NULL,
    conversation_id TEXT,
    agent_instance_id TEXT,
    scope TEXT NOT NULL,
    lifecycle TEXT NOT NULL,
    content TEXT NOT NULL,
    importance REAL NOT NULL,
    confidence REAL NOT NULL,
    provenance TEXT NOT NULL,
    depth_level INTEGER NOT NULL,
    expires_at DATETIME,
    visibility_overrides TEXT,
    created_at DATETIME NOT NULL DEFAULT CURRENT_TIMESTAMP,
    updated_at DATETIME NOT NULL DEFAULT CURRENT_TIMESTAMP
);

CREATE TABLE IF NOT EXISTS learned_rules (
    id TEXT PRIMARY KEY,
    project_id TEXT, -- NULL for Global rules
    rule_text TEXT NOT NULL,
    confidence TEXT NOT NULL,
    metadata TEXT NOT NULL,
    created_at DATETIME NOT NULL DEFAULT CURRENT_TIMESTAMP
);

CREATE TABLE IF NOT EXISTS artifacts (
    id TEXT PRIMARY KEY,
    project_id TEXT NOT NULL,
    producer_execution_id TEXT,
    relative_path TEXT NOT NULL,
    content_hash TEXT NOT NULL,
    mime_type TEXT NOT NULL,
    size_bytes INTEGER NOT NULL,
    created_at DATETIME NOT NULL DEFAULT CURRENT_TIMESTAMP
);

CREATE TABLE IF NOT EXISTS tasks (
    id TEXT PRIMARY KEY,
    conversation_id TEXT NOT NULL,
    assigned_agent_id TEXT,
    parent_task_id TEXT,
    title TEXT NOT NULL,
    description TEXT NOT NULL,
    status TEXT NOT NULL,
    priority INTEGER NOT NULL,
    created_at DATETIME NOT NULL DEFAULT CURRENT_TIMESTAMP,
    updated_at DATETIME NOT NULL DEFAULT CURRENT_TIMESTAMP,
    completed_at DATETIME
);

CREATE TABLE IF NOT EXISTS approvals (
    id TEXT PRIMARY KEY,
    execution_id TEXT NOT NULL,
    agent_instance_id TEXT NOT NULL,
    conversation_id TEXT NOT NULL,
    capability TEXT NOT NULL,
    tool_name TEXT,
    action_description TEXT NOT NULL,
    arguments_summary TEXT NOT NULL,
    risk_level TEXT NOT NULL,
    status TEXT NOT NULL,
    human_feedback TEXT,
    requested_at DATETIME NOT NULL DEFAULT CURRENT_TIMESTAMP,
    expires_at DATETIME NOT NULL,
    resolved_at DATETIME
);

CREATE TABLE IF NOT EXISTS workflows (
    id TEXT PRIMARY KEY,
    conversation_id TEXT NOT NULL,
    name TEXT NOT NULL,
    description TEXT NOT NULL,
    nodes TEXT NOT NULL,
    edges TEXT NOT NULL,
    created_at DATETIME NOT NULL DEFAULT CURRENT_TIMESTAMP,
    updated_at DATETIME NOT NULL DEFAULT CURRENT_TIMESTAMP
);

CREATE TABLE IF NOT EXISTS workflow_runs (
    id TEXT PRIMARY KEY,
    workflow_id TEXT NOT NULL,
    status TEXT NOT NULL,
    current_node_id TEXT,
    started_at DATETIME NOT NULL DEFAULT CURRENT_TIMESTAMP,
    completed_at DATETIME,
    FOREIGN KEY(workflow_id) REFERENCES workflows(id)
);

CREATE TABLE IF NOT EXISTS token_usages (
    id TEXT PRIMARY KEY,
    execution_id TEXT NOT NULL,
    agent_instance_id TEXT NOT NULL,
    conversation_id TEXT NOT NULL,
    provider TEXT NOT NULL,
    model TEXT NOT NULL,
    prompt_tokens INTEGER NOT NULL,
    completion_tokens INTEGER NOT NULL,
    total_tokens INTEGER NOT NULL,
    estimated_cost_usd REAL,
    compute_time_ms INTEGER NOT NULL,
    created_at DATETIME NOT NULL DEFAULT CURRENT_TIMESTAMP
);

CREATE TABLE IF NOT EXISTS tool_executions (
    id TEXT PRIMARY KEY,
    execution_id TEXT,
    tool_name TEXT NOT NULL,
    arguments TEXT,
    effect_class TEXT,
    idempotency_key TEXT,
    success INTEGER NOT NULL DEFAULT 1,
    created_at DATETIME NOT NULL DEFAULT CURRENT_TIMESTAMP
);

CREATE TABLE IF NOT EXISTS policies (id INTEGER PRIMARY KEY AUTOINCREMENT, capability TEXT NOT NULL, outcome TEXT NOT NULL, scope TEXT NOT NULL, entity_id TEXT);

CREATE TABLE IF NOT EXISTS channels (
    id TEXT PRIMARY KEY,
    conversation_id TEXT NOT NULL,
    name TEXT NOT NULL,
    kind TEXT NOT NULL,
    is_read_only INTEGER NOT NULL DEFAULT 0,
    member_actors TEXT NOT NULL,
    created_at DATETIME NOT NULL DEFAULT CURRENT_TIMESTAMP,
    updated_at DATETIME NOT NULL DEFAULT CURRENT_TIMESTAMP,
    FOREIGN KEY(conversation_id) REFERENCES conversations(id)
);

CREATE TABLE IF NOT EXISTS agent_memberships (
    agent_instance_id TEXT NOT NULL,
    conversation_id TEXT NOT NULL,
    role_in_conversation TEXT NOT NULL,
    joined_at DATETIME NOT NULL DEFAULT CURRENT_TIMESTAMP,
    left_at DATETIME,
    PRIMARY KEY (agent_instance_id, conversation_id),
    FOREIGN KEY(agent_instance_id) REFERENCES agent_instances(id),
    FOREIGN KEY(conversation_id) REFERENCES conversations(id)
);

