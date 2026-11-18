# Desktop: manage MCP tokens through Tauri commands

Status: done
Type: task
Blocked by: 01

On Windows the device token is in the Credential Manager and only Rust reads it, so
the AI tab goes through Tauri commands.

## Work

- `core/src/client.rs`: an `McpToken` type (or reuse one moved to core's model) and
  `Client::mcp_tokens(token)`, `create_mcp_token(token, name, scope, expires_at)`,
  `set_mcp_token_expiry(token, id, expires_at)`, `delete_mcp_token(token, id)`.
  `expires_at` must be able to send "left out", `null` and a date apart.
- `app/src-tauri/src/commands.rs`: async commands `app_mcp_tokens`,
  `app_mcp_token_create`, `app_mcp_token_expiry`, `app_mcp_token_delete`, and
  `app_mcp_url` (`ServerInfo::tasks_url("/mcp")`). Register them in `lib.rs`.
  Desktop only; on Android they return an error.
- Errors go through `client::diagnose`, so offline shows as offline.

`cargo check -p tasks-core` and `cargo check -p <app crate>` from PowerShell.

## Comments

2026-11-19: Implemented. `McpToken`/`NewMcpToken` and four methods in
`core/src/client.rs`; `app_mcp_url`, `app_mcp_tokens`, `app_mcp_token_create`,
`app_mcp_token_expiry`, `app_mcp_token_delete` in `commands.rs`, registered in
`lib.rs`. `updates::client` is now `pub(crate)` and shared. Change from the plan:
create always sends `expires_at` (a date or null); the AI tab works out the
30-day default itself, so the "left out" case isn't used by the app.
`cargo check -p tasks-core --all-features` and `cargo check -p tasks-app` are clean.
