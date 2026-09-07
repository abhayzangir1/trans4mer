-- Add created_at to agent_executions
ALTER TABLE agent_executions ADD COLUMN created_at DATETIME NOT NULL DEFAULT CURRENT_TIMESTAMP;

-- Add updated_at to action_diffs
ALTER TABLE action_diffs ADD COLUMN updated_at DATETIME;
