# MCP server

Status: done

Let an AI client (Claude Desktop, Claude Code, any MCP client) read a user's tasks,
add them, change them, tick them off and delete them, through an MCP endpoint on
tasks-server. The main use: the AI reads tasks, does them, then marks them completed.

## Decisions

- **Endpoint**: `POST {base}/mcp` (`/tasks-api/mcp` by default), stateless Streamable
  HTTP: one JSON-RPC message in, one `application/json` answer out. No SSE, no
  `Mcp-Session-Id`. `GET {base}/mcp` is 405. Notifications get 202.
- **Hand-written**, not the `rmcp` crate: `initialize`, `ping`, `tools/list`,
  `tools/call`. `initialize` repeats the client's `protocolVersion` when supported,
  else offers ours; `capabilities: { tools: {} }`.
- **MCP tokens** are their own kind: prefix `mcp_`, own table `mcp_tokens` in
  accounts.db, sha256-hashed, shown once. A device token (`dt_`) never works on
  `/mcp`; an MCP token never works anywhere else.
- **Scope** per token: `read` or `read_write`. A read token's `tools/list` shows only
  the read tools, and calling a write tool is refused.
- **Expiry** per token: defaults to 30 days from creation; the user can pick another
  date or no expiry when making it, and change it later. An expired token gets 401.
- **Many tokens per user.** The user can delete a token (the row is removed; this is
  accounts.db, not the synced tasks DB, so no tombstone).
- **Managed from Settings → AI**, on web and Windows only (not Android). The token
  routes are signed in with the device token (or the web session's token).
- **Writes** run through `tasks_core::ops` with `Ctx::server()` via routes.rs's
  `write()` path, then `hub.notify(user_id, "mcp:<token_id>")` so every device syncs
  within about 2 s. Nothing changes in sync.
- **Tasks only**: no tools to create or change projects, sections or filters.
  Projects and sections can be listed, so the AI can file a task.
- **Structured dates**: the AI sends `YYYY-MM-DD` and friends; no smart parsing
  (`taskParse.ts` is TypeScript only).

## Tools

| Tool | Scope | Calls |
|---|---|---|
| `list_projects` | read | `ops::list_projects` |
| `list_sections` | read | `ops::list_sections` (optional `project_id`) |
| `list_tasks` | read | `ops::list_tasks` (`q`, `project_id`, `inbox`, `completed`, `limit`; default 100, cap 500; `more: true` when cut) |
| `get_task` | read | `ops::get_task` |
| `create_task` | read_write | `ops::create_task(TaskIn)` |
| `update_task` | read_write | `ops::update_task(TaskPatch)`; `null` clears a field |
| `complete_task` | read_write | `ops::complete_task` (optional `today`, `YYYY-MM-DD`) |
| `reopen_task` | read_write | `ops::reopen_task` |
| `delete_task` | read_write | `ops::delete_task` |

Read tools carry `annotations: { readOnlyHint: true }`; `delete_task` carries
`destructiveHint: true`. Results: a short `text` part plus `structuredContent` with
the JSON. Core errors (invalid input, not found) are a tool result with
`isError: true`; protocol faults are JSON-RPC errors.

Tool descriptions are what the model reads. They spell out the date/time formats,
the priority range, the recurrence syntax, that no `project_id` means Inbox, that
titles may hold `[label](https://…)` links, and to call `list_projects` before
filing into a project. Copy the exact rules from `core/src/model.rs` and
`core/src/recurrence.rs`.

## Security

- No or bad `Authorization: Bearer mcp_…`: 401 with `WWW-Authenticate: Bearer`.
- An `Origin` header that doesn't match the request's host: 403 (DNS rebinding).
- Body limit about 1 MB on `/mcp`.
- `last_used_at` kept to the minute, like device tokens.

## Out of scope for now

- Project, section and filter writes.
- OAuth (the MCP spec's authorization flow); bearer tokens are enough for personal use.
- Natural-language dates.

## Tickets

1. `issues/01-mcp-token-storage.md`: schema, accounts functions, token routes
2. `issues/02-mcp-endpoint.md`: `/mcp` and the tools
3. `issues/03-desktop-token-commands.md`: core client methods and Tauri commands
4. `issues/04-settings-ai-tab.md`: the AI tab
5. `issues/05-docs.md`: README and glossary

The user builds: `build-server.bat`, `build-windows.bat`, and the web build
(`npm run build` in `app/`).
