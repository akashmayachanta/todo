# Docs for the MCP server

Status: done
Type: task
Blocked by: 02

- `README.md`: an "MCP" section covering what it does, the URL, making a token in
  Settings → AI, scopes and expiry, and the `claude mcp add` line. Mention that
  reverse proxies must pass `Authorization` and allow POST on `/mcp`.
- `GLOSSARY.md`: "MCP token" (versus device token), "scope".
- Add a line about `/mcp` and its tokens to `CLAUDE.md`'s "Things that are easy to get wrong".

## Comments

2026-11-19: Done. README: "AI clients (MCP)" under Server, token routes and `/mcp`
in the API list, notes in Accounts, the browser paragraph and Checking it.
`GLOSSARY.md` didn't exist; started it with device token, MCP token, scope and MCP
endpoint. CLAUDE.md: an **MCP** group in "Things that are easy to get wrong" and
`/mcp` in the layout table.
