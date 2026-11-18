# Settings → AI tab

Status: done
Type: task
Blocked by: 01, 03

## Work

- `SettingsDialog.svelte`: add `'ai'` to `SettingsTab` and `TABS` (label "AI"),
  hidden when `status.platform === 'android'`. Update the comment at the top.
- A small client with one shape for both platforms: web functions in `webClient.ts`
  (session token, `API_BASE`), desktop `call()` wrappers in `tauriClient.ts`.
- The tab shows:
  - **MCP URL** with a copy button. Web: `location.origin + API_BASE + '/mcp'`;
    desktop: `app_mcp_url`.
  - **New token**: name, access (Read only / Read and write; default Read and
    write), expiry (7 days, 30 days (default), 90 days, a custom date, No expiry).
    On create, show the token once with a copy button and a note that it won't be
    shown again, plus ready-to-paste setup:
    - `claude mcp add --transport http tasks <url> --header "Authorization: Bearer <token>"`
    - an `mcpServers` JSON block for other clients.
  - **Token list**: name, access, created, expires (or "Never", or "Expired" in the
    warning colour), last used. Per row: change expiry (same choices) and delete
    (through `confirmDialog`).
- A custom date means the end of that day in local time, sent as an ISO timestamp.
- Offline or server down: say "Connect to the server to manage tokens" instead of
  the list.
- Phone width works (web on a phone), following the dialog's existing section layout.

`npm run check` in `app/`.

## Comments

2026-11-19: Implemented. Client and helpers in `app/src/app/mcpTokens.ts` (web via
`webMcp` in `webClient.ts`, Windows via the Tauri commands); the tab's logic and
markup in `SettingsDialog.svelte` (its styles are scoped there). Default access is
Read & write; editing an expiry starts from 30 days rather than the current date.
Copy falls back to `execCommand` where the Clipboard API is missing (plain http).
`npm run check` is clean. Not yet tried in the running app.
