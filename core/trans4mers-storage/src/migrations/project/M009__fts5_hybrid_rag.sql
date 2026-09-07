-- Project schema M009 — SQLite FTS5 External Content Virtual Tables for Hybrid RAG & BM25 Search

-- 1. Document Chunks FTS5 Virtual Table & Sync Triggers
CREATE VIRTUAL TABLE IF NOT EXISTS doc_chunks_fts USING fts5(
    text,
    content='doc_chunks',
    content_rowid='rowid'
);

CREATE TRIGGER IF NOT EXISTS doc_chunks_ai AFTER INSERT ON doc_chunks BEGIN
    INSERT INTO doc_chunks_fts(rowid, text) VALUES (new.rowid, new.text);
END;

CREATE TRIGGER IF NOT EXISTS doc_chunks_ad AFTER DELETE ON doc_chunks BEGIN
    INSERT INTO doc_chunks_fts(doc_chunks_fts, rowid, text) VALUES('delete', old.rowid, old.text);
END;

CREATE TRIGGER IF NOT EXISTS doc_chunks_au AFTER UPDATE ON doc_chunks BEGIN
    INSERT INTO doc_chunks_fts(doc_chunks_fts, rowid, text) VALUES('delete', old.rowid, old.text);
    INSERT INTO doc_chunks_fts(rowid, text) VALUES (new.rowid, new.text);
END;

INSERT INTO doc_chunks_fts(doc_chunks_fts) VALUES('rebuild');

-- 2. Project Memories FTS5 Virtual Table & Sync Triggers
CREATE VIRTUAL TABLE IF NOT EXISTS project_memories_fts USING fts5(
    content,
    content='project_memories',
    content_rowid='rowid'
);

CREATE TRIGGER IF NOT EXISTS project_memories_ai AFTER INSERT ON project_memories BEGIN
    INSERT INTO project_memories_fts(rowid, content) VALUES (new.rowid, new.content);
END;

CREATE TRIGGER IF NOT EXISTS project_memories_ad AFTER DELETE ON project_memories BEGIN
    INSERT INTO project_memories_fts(project_memories_fts, rowid, content) VALUES('delete', old.rowid, old.content);
END;

CREATE TRIGGER IF NOT EXISTS project_memories_au AFTER UPDATE ON project_memories BEGIN
    INSERT INTO project_memories_fts(project_memories_fts, rowid, content) VALUES('delete', old.rowid, old.content);
    INSERT INTO project_memories_fts(rowid, content) VALUES (new.rowid, new.content);
END;

INSERT INTO project_memories_fts(project_memories_fts) VALUES('rebuild');

-- 3. Learned Rules FTS5 Virtual Table & Sync Triggers
CREATE VIRTUAL TABLE IF NOT EXISTS learned_rules_fts USING fts5(
    rule_text,
    content='learned_rules',
    content_rowid='rowid'
);

CREATE TRIGGER IF NOT EXISTS learned_rules_ai AFTER INSERT ON learned_rules BEGIN
    INSERT INTO learned_rules_fts(rowid, rule_text) VALUES (new.rowid, new.rule_text);
END;

CREATE TRIGGER IF NOT EXISTS learned_rules_ad AFTER DELETE ON learned_rules BEGIN
    INSERT INTO learned_rules_fts(learned_rules_fts, rowid, rule_text) VALUES('delete', old.rowid, old.rule_text);
END;

CREATE TRIGGER IF NOT EXISTS learned_rules_au AFTER UPDATE ON learned_rules BEGIN
    INSERT INTO learned_rules_fts(learned_rules_fts, rowid, rule_text) VALUES('delete', old.rowid, old.rule_text);
    INSERT INTO learned_rules_fts(rowid, rule_text) VALUES (new.rowid, new.rule_text);
END;

INSERT INTO learned_rules_fts(learned_rules_fts) VALUES('rebuild');
