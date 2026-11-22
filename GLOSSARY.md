# Glossary

Terms used in the code, docs and issues for Tasks.

**Due moment**: the instant a task is due, in the user's local time: its due date at
its due time, or at 10:00 for a date with no time. A task with no date has none.

**Reminder**: the instant the user is told about a task (`remind_at`). At most one
per task. It can be set apart from the due moment, or left off.

**Default reminder**: the reminder a task gets when its due date or time is set
while it has no reminder and none is asked for: at its due moment, if that is
still ahead. Asking for "no reminder" means none is given; a reminder taken off
earlier isn't remembered.

**Lead**: how long before the due moment a task's reminder fires. When the due
date or time changes and no new reminder is asked for, the reminder moves so the
lead stays the same. Taking the date off takes the reminder off.

**Device token**: the long-lived key a device gets when it signs in (`dt_…`). It
works on every route except `/mcp`. Kept in the Windows Credential Manager, the
Android app's private storage, or a browser's local storage; the server keeps only
its SHA-256 hash in `accounts.db` (`devices`).

**MCP token**: a key for an AI client (`mcp_…`), made in Settings → AI. It works
only on `{base}/mcp` and has a scope and an optional expiry. A user can have any
number; deleting one removes it. Stored hashed in `accounts.db` (`mcp_tokens`).
_Not_ a device token: it never syncs and isn't in the device list.

**Scope**: what an MCP token may do. `read` lists and reads tasks, projects and
sections; `read_write` also adds, changes, completes, reopens and deletes tasks.
Neither can change projects or sections.

**MCP endpoint**: `POST {base}/mcp` (`server/src/mcp.rs`), the server's Model
Context Protocol server: stateless Streamable HTTP, one JSON-RPC message per
request. Its writes run on the server's copy and reach devices through sync.
