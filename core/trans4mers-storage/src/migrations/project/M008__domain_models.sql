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

CREATE TABLE IF NOT EXISTS learned_rules (
    id TEXT PRIMARY KEY,
    project_id TEXT, -- NULL for Global rules
    rule_text TEXT NOT NULL,
    confidence TEXT NOT NULL,
    metadata TEXT NOT NULL,
    created_at DATETIME NOT NULL DEFAULT CURRENT_TIMESTAMP
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
