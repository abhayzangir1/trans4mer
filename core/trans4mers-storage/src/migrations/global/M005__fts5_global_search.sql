-- Global schema M005 — SQLite FTS5 External Content Virtual Tables for Global Memories & Rules

-- 1. Global Memories FTS5 Virtual Table & Sync Triggers
CREATE VIRTUAL TABLE IF NOT EXISTS global_memories_fts USING fts5(
    content,
    content='global_memories',
    content_rowid='rowid'
);

CREATE TRIGGER IF NOT EXISTS global_memories_ai AFTER INSERT ON global_memories BEGIN
    INSERT INTO global_memories_fts(rowid, content) VALUES (new.rowid, new.content);
END;

CREATE TRIGGER IF NOT EXISTS global_memories_ad AFTER DELETE ON global_memories BEGIN
    INSERT INTO global_memories_fts(global_memories_fts, rowid, content) VALUES('delete', old.rowid, old.content);
END;

CREATE TRIGGER IF NOT EXISTS global_memories_au AFTER UPDATE ON global_memories BEGIN
    INSERT INTO global_memories_fts(global_memories_fts, rowid, content) VALUES('delete', old.rowid, old.content);
    INSERT INTO global_memories_fts(rowid, content) VALUES (new.rowid, new.content);
END;

INSERT INTO global_memories_fts(global_memories_fts) VALUES('rebuild');

-- 2. Learned Rules FTS5 Virtual Table & Sync Triggers (Global)
CREATE VIRTUAL TABLE IF NOT EXISTS learned_rules_fts USING fts5(
    rule_text,
    content='learned_rules',
    content_rowid='rowid'
);

CREATE TRIGGER IF NOT EXISTS global_learned_rules_ai AFTER INSERT ON learned_rules BEGIN
    INSERT INTO learned_rules_fts(rowid, rule_text) VALUES (new.rowid, new.rule_text);
END;

CREATE TRIGGER IF NOT EXISTS global_learned_rules_ad AFTER DELETE ON learned_rules BEGIN
    INSERT INTO learned_rules_fts(learned_rules_fts, rowid, rule_text) VALUES('delete', old.rowid, old.rule_text);
END;

CREATE TRIGGER IF NOT EXISTS global_learned_rules_au AFTER UPDATE ON learned_rules BEGIN
    INSERT INTO learned_rules_fts(learned_rules_fts, rowid, rule_text) VALUES('delete', old.rowid, old.rule_text);
    INSERT INTO learned_rules_fts(rowid, rule_text) VALUES (new.rowid, new.rule_text);
END;

INSERT INTO learned_rules_fts(learned_rules_fts) VALUES('rebuild');
