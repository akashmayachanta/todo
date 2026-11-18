# The /mcp endpoint and its tools

Status: done
Type: task
Blocked by: 01

See `../spec.md` for the protocol choices and the tool table.

## Work

- New `server/src/mcp.rs`; `mod mcp;` in `main.rs`.
- In `routes.rs`, `.route(&p("/mcp"), post(mcp::handle).get(mcp::not_allowed))`
  placed before the `/{id}` catch-all, with its own small body limit (about 1 MB).
- `handle(State, McpUser, headers, body)`:
  - `Origin` present and not matching `Host` → 403.
  - Parse a JSON-RPC 2.0 message. A batch (array) → JSON-RPC error `-32600`.
  - A notification (no `id`) → 202, empty body.
  - `initialize` → `{ protocolVersion, capabilities: { tools: {} }, serverInfo: { name: "tasks", version } , instructions }`.
    Short `instructions`: what the server is and the read → do → `complete_task` flow.
  - `ping` → `{}`.
  - `tools/list` → the tools the token's scope allows.
  - `tools/call` → run the tool. A write tool with a read token → `isError: true`
    "This token is read-only". Unknown tool → `-32602`.
  - Unknown method → `-32601`.
- Tool arguments deserialize straight into the core types (`TaskIn`, `TaskPatch`,
  `TaskQuery`) plus an `id` where needed; serde errors become `isError: true`.
- `list_tasks`: default `limit` 50, cap 200.
- `complete_task`: optional `today` (`YYYY-MM-DD`), else the server's date, as the
  REST route does. Share that parsing with `routes.rs` instead of copying it.
- Writes: `store.run` then `hub.notify(user_id, &format!("mcp:{token_id}"))`. Make
  the `write()` helper in routes.rs take `(user_id, sender)` so both use it.
- Input schemas are `json!` literals with descriptions taken from the rules in
  `core/src/model.rs` (date/time formats, priority range, title length) and
  `core/src/recurrence.rs` (recurrence syntax).

## Tests (mcp.rs, temp data dir)

- `initialize` answers with a protocol version and the tools capability.
- `tools/list` with a read token has no write tools; with read_write it has all nine.
- `create_task` then `list_tasks` finds it; `complete_task` then `list_tasks
  { completed: true }` finds it; `delete_task` removes it from the list.
- Bad input (empty title) → `isError: true` with core's message.
- A write tool with a read token → `isError: true`.
- No token → 401 with `WWW-Authenticate`.

Bump `server/Cargo.toml` to `0.3.0` with a line in its version note.

## Comments

2026-11-19: Implemented in `server/src/mcp.rs`; `routes.rs` gained `write_as` and
`parse_today`, shared with the REST routes. Changes from the plan: `list_tasks`
defaults to 100 and caps at 500 (the open list isn't limited in core, so it's cut
in mcp.rs, with `more: true` when cut). `update_task` refuses `completed_at`,
`completes_occurrence` and `position`, pointing to complete_task / reopen_task.
The 401 + `WWW-Authenticate` path isn't covered by a test (no HTTP harness here).
`cargo check -p tasks-server --tests` is clean; tests still need a run.

2026-11-19: Two follow-ups for the tab group exporter. `list_tasks` takes `offset`
(pages; `more` means more after this page). The Origin check now applies only to
http(s) origins, so a browser extension's `chrome-extension://` Origin is let
through (it still needs an MCP token).
