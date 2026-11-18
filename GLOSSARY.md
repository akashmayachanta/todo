# Glossary

Terms used in the code, docs and issues for Tasks.

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
