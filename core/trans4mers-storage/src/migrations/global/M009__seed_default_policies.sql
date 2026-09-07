-- Seed baseline zero-trust policies for safe vs privileged capabilities
INSERT INTO policies (capability, outcome, scope, entity_id)
SELECT 'FilesystemRead', 'Allow', 'Global', NULL
WHERE NOT EXISTS (SELECT 1 FROM policies WHERE capability = 'FilesystemRead' AND scope = 'Global');

INSERT INTO policies (capability, outcome, scope, entity_id)
SELECT 'GitRead', 'Allow', 'Global', NULL
WHERE NOT EXISTS (SELECT 1 FROM policies WHERE capability = 'GitRead' AND scope = 'Global');

INSERT INTO policies (capability, outcome, scope, entity_id)
SELECT 'MemoryRead', 'Allow', 'Global', NULL
WHERE NOT EXISTS (SELECT 1 FROM policies WHERE capability = 'MemoryRead' AND scope = 'Global');

INSERT INTO policies (capability, outcome, scope, entity_id)
SELECT 'MemoryWrite', 'Allow', 'Global', NULL
WHERE NOT EXISTS (SELECT 1 FROM policies WHERE capability = 'MemoryWrite' AND scope = 'Global');

INSERT INTO policies (capability, outcome, scope, entity_id)
SELECT 'AgentMessage', 'Allow', 'Global', NULL
WHERE NOT EXISTS (SELECT 1 FROM policies WHERE capability = 'AgentMessage' AND scope = 'Global');

INSERT INTO policies (capability, outcome, scope, entity_id)
SELECT 'MessageSend', 'Allow', 'Global', NULL
WHERE NOT EXISTS (SELECT 1 FROM policies WHERE capability = 'MessageSend' AND scope = 'Global');

INSERT INTO policies (capability, outcome, scope, entity_id)
SELECT 'Custom(MessageSend)', 'Allow', 'Global', NULL
WHERE NOT EXISTS (SELECT 1 FROM policies WHERE capability = 'Custom(MessageSend)' AND scope = 'Global');

INSERT INTO policies (capability, outcome, scope, entity_id)
SELECT 'BrowserNavigate', 'Allow', 'Global', NULL
WHERE NOT EXISTS (SELECT 1 FROM policies WHERE capability = 'BrowserNavigate' AND scope = 'Global');

INSERT INTO policies (capability, outcome, scope, entity_id)
SELECT 'FilesystemWrite', 'Ask', 'Global', NULL
WHERE NOT EXISTS (SELECT 1 FROM policies WHERE capability = 'FilesystemWrite' AND scope = 'Global');

INSERT INTO policies (capability, outcome, scope, entity_id)
SELECT 'ShellExecute', 'Ask', 'Global', NULL
WHERE NOT EXISTS (SELECT 1 FROM policies WHERE capability = 'ShellExecute' AND scope = 'Global');

INSERT INTO policies (capability, outcome, scope, entity_id)
SELECT 'GitPush', 'Ask', 'Global', NULL
WHERE NOT EXISTS (SELECT 1 FROM policies WHERE capability = 'GitPush' AND scope = 'Global');

INSERT INTO policies (capability, outcome, scope, entity_id)
SELECT 'SecretRead', 'Ask', 'Global', NULL
WHERE NOT EXISTS (SELECT 1 FROM policies WHERE capability = 'SecretRead' AND scope = 'Global');
