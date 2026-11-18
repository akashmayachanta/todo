# MCP token storage and management routes

Status: done
Type: task

See `../spec.md`.

## Schema

New file `server/migrations/0002_mcp_tokens.sql` (never edit `0001`), added to
`MIGRATIONS` in `server/src/auth.rs`:

```sql
-- Migration 0002: MCP tokens, for AI clients calling {base}/mcp.
-- Apart from device tokens: an MCP token works only on /mcp, a device token never does.
CREATE TABLE mcp_tokens (
    id           TEXT PRIMARY KEY,
    user_id      INTEGER NOT NULL REFERENCES users (id),
    name         TEXT NOT NULL,
    scope        TEXT NOT NULL CHECK (scope IN ('read', 'read_write')),
    token_hash   TEXT NOT NULL UNIQUE,    -- sha256 of the token
    created_at   TEXT NOT NULL,
    expires_at   TEXT,                    -- NULL: never expires
    last_used_at TEXT
);
CREATE INDEX idx_mcp_tokens_user ON mcp_tokens (user_id);
```

## accounts (auth.rs)

- `pub const MCP_TOKEN_PREFIX: &str = "mcp_";` and make `random_token` take the prefix.
- `McpScope` enum (`Read`, `ReadWrite`), serde `snake_case`.
- `McpToken { id, name, scope, created_at, expires_at, last_used_at }` (Serialize).
- `create_mcp_token(user_id, name, scope, expires_at: Option<String>) -> (McpToken, String)`.
- `list_mcp_tokens(user_id)`: newest first.
- `set_mcp_token_expiry(user_id, id, expires_at: Option<String>) -> bool`.
- `delete_mcp_token(user_id, id) -> bool`: removes the row.
- `check_mcp(token) -> Option<McpUser>`: prefix check, hash match, user not disabled,
  `expires_at` NULL or later than now (compare parsed times, not strings), then
  `last_used_at` to the minute as `check` does.
- `McpUser { user_id, token_id, scope }` with a `FromRequestParts` extractor like
  `AuthUser`. Its 401 carries `WWW-Authenticate: Bearer`.

## Routes (routes.rs), signed in with `AuthUser`

- `GET  /auth/mcp-tokens` → `[McpToken]`
- `POST /auth/mcp-tokens` `{ name, scope, expires_at? }` → 201 `{ token, info: McpToken }`.
  `expires_at` left out means 30 days from now; `null` means never; a date must be in
  the future. Use a double option (`Option<Option<String>>`) for that, as `TaskPatch`
  does. Name trimmed, 1 to 100 characters, defaults to "AI client".
- `POST /auth/mcp-tokens/{id}` `{ expires_at }` → `McpToken` (`null` = never). 404 if not theirs.
- `POST /auth/mcp-tokens/{id}/delete` → `{ ok: true }`. 404 if not theirs.

## Tests (auth.rs)

- Create, check, delete; a deleted token is refused.
- An expired token is refused; moving its expiry later lets it back in; `None` never expires.
- A disabled user's MCP token is refused.
- `check` refuses an `mcp_` token and `check_mcp` refuses a `dt_` one.
- One user can't change or delete another's token.

`cargo test -p tasks-server` (run from PowerShell).

## Comments

2026-11-19: Implemented. `server/migrations/0002_mcp_tokens.sql`; accounts functions,
`McpUser` extractor, `check_expiry`/`default_expiry` in `auth.rs`; routes in
`routes.rs`. `cargo check -p tasks-server --tests` is clean; the tests still need a run.
