-- Global schema M006 — Store raw vectors as BLOBs in global SQLite relational tables
-- Enables zero-cost instant index rebuilds for global knowledge

ALTER TABLE global_memories ADD COLUMN embedding BLOB;
ALTER TABLE learned_rules ADD COLUMN embedding BLOB;
