-- Migration 0002: MCP tokens, for AI clients calling {base}/mcp.
--
-- Apart from device tokens: an MCP token works only on /mcp, and a device
-- token never does. A user makes them in the app's Settings → AI, and can have
-- any number. Deleting one removes its row. Only a hash of the token is kept.

CREATE TABLE mcp_tokens (
    id           TEXT PRIMARY KEY,
    user_id      INTEGER NOT NULL REFERENCES users (id),
    name         TEXT NOT NULL,
    scope        TEXT NOT NULL CHECK (scope IN ('read', 'read_write')),
    token_hash   TEXT NOT NULL UNIQUE,    -- sha256 of the token
    created_at   TEXT NOT NULL,
    expires_at   TEXT,                    -- canonical ISO instant; NULL: never expires
    last_used_at TEXT
);
CREATE INDEX idx_mcp_tokens_user ON mcp_tokens (user_id);
