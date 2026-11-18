//! The MCP endpoint, {base}/mcp: lets an AI client (Claude Desktop, Claude Code,
//! any MCP client) read the user's tasks, add, change, tick off and delete them.
//!
//! Stateless Streamable HTTP: each POST carries one JSON-RPC message and gets one
//! `application/json` answer; no event stream and no session id. A GET is 405,
//! which the spec allows for a server that never speaks first. Callers sign in
//! with an MCP token (auth::McpUser), never a device token; a read-only token
//! sees and runs only the read tools.
//!
//! Writes go through tasks_core::ops on the server's copy, like the REST API's,
//! and tell the user's devices to sync, so they arrive in about 2 s.

use std::sync::Arc;

use axum::body::Bytes;
use axum::extract::State;
use axum::http::{header, HeaderMap, StatusCode};
use axum::response::{IntoResponse, Response};
use axum::Json;
use serde::de::DeserializeOwned;
use serde::Deserialize;
use serde_json::{json, Map, Value};
use tasks_core::{ops, Ctx, TaskIn, TaskPatch, TaskQuery};

use crate::auth::{McpScope, McpUser};
use crate::error::{ApiError, ApiResult};
use crate::routes::{parse_today, write_as};
use crate::AppState;

/// Newest first; `initialize` repeats the client's version when it is here.
const PROTOCOL_VERSIONS: &[&str] = &["2025-11-25", "2025-06-18", "2025-03-26", "2024-11-05"];

/// The largest message taken. Tasks are small; this is far more than one needs.
const MAX_BODY: usize = 1024 * 1024;

/// list_tasks returns this many unless asked for fewer or more, up to MAX_LIMIT.
const DEFAULT_LIMIT: i64 = 100;
const MAX_LIMIT: i64 = 500;
/// The most ops::list_tasks takes as its limit.
const CORE_LIMIT: i64 = 1000;

// JSON-RPC error codes.
const PARSE_ERROR: i64 = -32700;
const INVALID_REQUEST: i64 = -32600;
const METHOD_NOT_FOUND: i64 = -32601;
const INVALID_PARAMS: i64 = -32602;

const INSTRUCTIONS: &str = "Tasks is the user's to-do list. A usual round: list_tasks to see what is open \
(or search it with q), get_task for one task's details, do the work, then complete_task. \
Projects and sections can be listed, to file a task or narrow a list, but not changed. \
Changes reach the user's devices within seconds.";

pub async fn not_allowed() -> Response {
    (StatusCode::METHOD_NOT_ALLOWED, [(header::ALLOW, "POST")]).into_response()
}

pub async fn handle(State(s): State<Arc<AppState>>, caller: McpUser, headers: HeaderMap, body: Bytes) -> Response {
    if !origin_allowed(&headers) {
        return ApiError::forbidden("This Origin may not call the MCP endpoint").into_response();
    }
    if body.len() > MAX_BODY {
        return ApiError::new(StatusCode::PAYLOAD_TOO_LARGE, "The message is too large").into_response();
    }
    let msg: Value = match serde_json::from_slice(&body) {
        Ok(msg) => msg,
        Err(e) => return Json(rpc_error(Value::Null, PARSE_ERROR, &format!("Not JSON: {e}"))).into_response(),
    };
    match dispatch(&s, &caller, msg).await {
        Some(reply) => Json(reply).into_response(),
        None => StatusCode::ACCEPTED.into_response(),
    }
}

/// A web page on another site mustn't reach the server through the user's
/// network (DNS rebinding), so a request from an http(s) `Origin` must come from
/// this host. MCP clients outside a browser send no Origin, and a browser
/// extension (the tab group exporter) sends its own `chrome-extension://` one,
/// which no web page can claim. Behind a reverse proxy, X-Forwarded-Host names
/// the public host.
fn origin_allowed(headers: &HeaderMap) -> bool {
    let Some(origin) = headers.get(header::ORIGIN).and_then(|v| v.to_str().ok()) else { return true };
    let Some((scheme, origin_host)) = origin.split_once("://") else { return false };
    if !scheme.eq_ignore_ascii_case("http") && !scheme.eq_ignore_ascii_case("https") {
        return true;
    }
    let origin_host = origin_host.trim_end_matches('/');
    ["x-forwarded-host", "host"]
        .iter()
        .filter_map(|name| headers.get(*name)?.to_str().ok())
        .any(|hosts| hosts.split(',').any(|h| h.trim().eq_ignore_ascii_case(origin_host)))
}

fn rpc_error(id: Value, code: i64, message: &str) -> Value {
    json!({ "jsonrpc": "2.0", "id": id, "error": { "code": code, "message": message } })
}

/// The answer to one JSON-RPC message; None for a notification (or a response,
/// which this server never asks for), answered with 202.
pub(crate) async fn dispatch(s: &AppState, caller: &McpUser, msg: Value) -> Option<Value> {
    if msg.is_array() {
        return Some(rpc_error(Value::Null, INVALID_REQUEST, "Batches aren't supported"));
    }
    let id = msg.get("id").filter(|id| !id.is_null()).cloned();
    let method = msg.get("method").and_then(Value::as_str);
    let (Some(id), Some(method)) = (id, method) else {
        return match (msg.get("id"), method) {
            (_, Some(_)) | (Some(_), None) => None,
            (None, None) => Some(rpc_error(Value::Null, INVALID_REQUEST, "Not a JSON-RPC message")),
        };
    };
    let params = msg.get("params").cloned().unwrap_or(Value::Null);
    let result = match method {
        "initialize" => Ok(initialize(&params)),
        "ping" => Ok(json!({})),
        "tools/list" => Ok(json!({ "tools": tools(caller.scope) })),
        "tools/call" => call(s, caller, params).await,
        _ => Err((METHOD_NOT_FOUND, format!("Unknown method '{method}'"))),
    };
    Some(match result {
        Ok(result) => json!({ "jsonrpc": "2.0", "id": id, "result": result }),
        Err((code, message)) => rpc_error(id, code, &message),
    })
}

fn initialize(params: &Value) -> Value {
    let asked = params.get("protocolVersion").and_then(Value::as_str);
    let version = asked.filter(|v| PROTOCOL_VERSIONS.contains(v)).unwrap_or(PROTOCOL_VERSIONS[0]);
    json!({
        "protocolVersion": version,
        "capabilities": { "tools": {} },
        "serverInfo": { "name": "tasks", "title": "Tasks", "version": env!("CARGO_PKG_VERSION") },
        "instructions": INSTRUCTIONS,
    })
}

// ── Tools ────────────────────────────────────────────────────────────────────

struct Tool {
    name: &'static str,
    /// Needs a read and write token.
    writes: bool,
    definition: fn() -> Value,
}

const TOOLS: &[Tool] = &[
    Tool { name: "list_projects", writes: false, definition: list_projects_def },
    Tool { name: "list_sections", writes: false, definition: list_sections_def },
    Tool { name: "list_tasks", writes: false, definition: list_tasks_def },
    Tool { name: "get_task", writes: false, definition: get_task_def },
    Tool { name: "create_task", writes: true, definition: create_task_def },
    Tool { name: "update_task", writes: true, definition: update_task_def },
    Tool { name: "complete_task", writes: true, definition: complete_task_def },
    Tool { name: "reopen_task", writes: true, definition: reopen_task_def },
    Tool { name: "delete_task", writes: true, definition: delete_task_def },
];

/// The tools a token of this scope may use.
fn tools(scope: McpScope) -> Vec<Value> {
    TOOLS.iter().filter(|t| scope.can_write() || !t.writes).map(|t| (t.definition)()).collect()
}

async fn call(s: &AppState, caller: &McpUser, params: Value) -> Result<Value, (i64, String)> {
    #[derive(Deserialize)]
    struct CallIn {
        name: String,
        #[serde(default)]
        arguments: Value,
    }
    let input: CallIn = serde_json::from_value(params).map_err(|e| (INVALID_PARAMS, format!("Bad tools/call params: {e}")))?;
    let tool = TOOLS
        .iter()
        .find(|t| t.name == input.name)
        .ok_or_else(|| (INVALID_PARAMS, format!("Unknown tool '{}'", input.name)))?;
    if tool.writes && !caller.scope.can_write() {
        return Ok(tool_error("This token is read-only: it can list and read tasks but not change them."));
    }
    let args = match input.arguments {
        Value::Null => Map::new(),
        Value::Object(map) => map,
        _ => return Ok(tool_error("arguments must be an object")),
    };
    Ok(match run(s, caller, tool.name, args).await {
        Ok(out) => json!({
            "content": [{ "type": "text", "text": out.to_string() }],
            "structuredContent": out,
            "isError": false,
        }),
        Err(e) => tool_error(&e.detail),
    })
}

/// A failed call the model can read and put right, not a protocol error.
fn tool_error(message: &str) -> Value {
    json!({ "content": [{ "type": "text", "text": message }], "isError": true })
}

fn parse<T: DeserializeOwned>(args: Map<String, Value>) -> ApiResult<T> {
    serde_json::from_value(Value::Object(args)).map_err(|e| ApiError::unprocessable(format!("Invalid arguments: {e}")))
}

/// Takes the `id` argument out of `args`.
fn take_id(args: &mut Map<String, Value>) -> ApiResult<String> {
    match args.remove("id") {
        Some(Value::String(id)) if !id.trim().is_empty() => Ok(id),
        _ => Err(ApiError::unprocessable("id: a task id is required")),
    }
}

async fn run(s: &AppState, caller: &McpUser, tool: &str, mut args: Map<String, Value>) -> ApiResult<Value> {
    let user = caller.user_id;
    let sender = format!("mcp:{}", caller.token_id);
    match tool {
        "list_projects" => {
            let projects = s.store.run(user, ops::list_projects).await?;
            Ok(json!({ "projects": to_json(&projects) }))
        }
        "list_sections" => {
            #[derive(Deserialize)]
            struct In {
                #[serde(default)]
                project_id: Option<String>,
            }
            let input: In = parse(args)?;
            let sections = s.store.run(user, move |c| ops::list_sections(c, input.project_id.as_deref())).await?;
            Ok(json!({ "sections": to_json(&sections) }))
        }
        "list_tasks" => {
            let offset = match args.remove("offset") {
                None | Some(Value::Null) => 0,
                Some(v) => v.as_i64().filter(|n| *n >= 0).ok_or_else(|| ApiError::unprocessable("offset: a whole number, 0 or more"))?,
            };
            let mut query: TaskQuery = parse(args)?;
            let limit = query.limit.unwrap_or(DEFAULT_LIMIT);
            if !(1..=MAX_LIMIT).contains(&limit) {
                return Err(ApiError::unprocessable(format!("limit: between 1 and {MAX_LIMIT}")));
            }
            // One more than the page, to tell whether there are more. Core takes
            // at most CORE_LIMIT for a search or the done tasks; the open list
            // isn't limited in core, so it is cut here.
            query.limit = Some((offset + limit + 1).min(CORE_LIMIT));
            let tasks = s.store.run(user, move |c| ops::list_tasks(c, &query)).await?;
            let more = tasks.len() as i64 > offset + limit;
            let page: Vec<_> = tasks.into_iter().skip(offset as usize).take(limit as usize).collect();
            Ok(json!({ "tasks": to_json(&page), "more": more }))
        }
        "get_task" => {
            let id = take_id(&mut args)?;
            let task = s.store.run(user, move |c| ops::get_task(c, &id)).await?;
            Ok(to_json(&task))
        }
        "create_task" => {
            args.remove("position");
            let input: TaskIn = parse(args)?;
            let task = write_as(s, user, &sender, move |c| ops::create_task(c, &Ctx::server(), input)).await?;
            Ok(to_json(&task))
        }
        "update_task" => {
            let id = take_id(&mut args)?;
            // Ticking goes through complete_task and reopen_task, which know about
            // repeating tasks and subtasks; the order stays the user's.
            for key in ["completed_at", "completes_occurrence", "position"] {
                if args.remove(key).is_some() {
                    return Err(ApiError::unprocessable(format!(
                        "{key} can't be changed here; use complete_task or reopen_task to tick a task"
                    )));
                }
            }
            let patch: TaskPatch = parse(args)?;
            let task = write_as(s, user, &sender, move |c| ops::update_task(c, &Ctx::server(), &id, patch)).await?;
            Ok(to_json(&task))
        }
        "complete_task" => {
            let id = take_id(&mut args)?;
            #[derive(Deserialize)]
            struct In {
                #[serde(default)]
                today: Option<String>,
            }
            let input: In = parse(args)?;
            let today = parse_today(input.today.as_deref())?;
            let task = write_as(s, user, &sender, move |c| ops::complete_task(c, &Ctx::server(), &id, today)).await?;
            Ok(to_json(&task))
        }
        "reopen_task" => {
            let id = take_id(&mut args)?;
            let task = write_as(s, user, &sender, move |c| ops::reopen_task(c, &Ctx::server(), &id)).await?;
            Ok(to_json(&task))
        }
        "delete_task" => {
            let id = take_id(&mut args)?;
            let deleted = id.clone();
            write_as(s, user, &sender, move |c| ops::delete_task(c, &Ctx::server(), &id)).await?;
            Ok(json!({ "deleted": deleted }))
        }
        _ => Err(ApiError::unprocessable(format!("Unknown tool '{tool}'"))),
    }
}

fn to_json<T: serde::Serialize>(value: &T) -> Value {
    serde_json::to_value(value).unwrap_or(Value::Null)
}

// ── Tool definitions ─────────────────────────────────────────────────────────
// What the model reads to use the tools: the rules come from core's checks
// (core/src/model.rs) and repeat rules (core/src/recurrence.rs).

fn read_only() -> Value {
    json!({ "readOnlyHint": true, "openWorldHint": false })
}

fn id_prop() -> Value {
    json!({ "type": "string", "description": "The task's id, from list_tasks." })
}

/// The editable fields of a task. `nullable`: each may be null, to clear it.
fn task_props(nullable: bool) -> Map<String, Value> {
    let t = |ty: &str| if nullable { json!([ty, "null"]) } else { json!(ty) };
    let props = json!({
        "title": { "type": "string", "description": "What to do, 1 to 500 characters (a subtask's at most 255). May hold Markdown links like [label](https://…), which the apps show as links." },
        "description": { "type": t("string"), "description": "Notes, plain text. May hold [label](https://…) links." },
        "project_id": { "type": t("string"), "description": "A project's id from list_projects. None (or null): the Inbox." },
        "section_id": { "type": t("string"), "description": "A section's id from list_sections, in the task's project." },
        "parent_id": { "type": t("string"), "description": "Makes it a subtask of that task. Subtasks are one level deep and take their parent's project and section." },
        "priority": { "type": t("integer"), "minimum": 1, "maximum": 4, "description": "1 is the most urgent, 4 is no priority (the default)." },
        "due_date": { "type": t("string"), "description": "YYYY-MM-DD, in the user's own calendar." },
        "due_time": { "type": t("string"), "description": "HH:MM, 24-hour. Needs a due_date." },
        "duration_minutes": { "type": t("integer"), "minimum": 1, "maximum": 10080, "description": "How long it takes, in minutes." },
        "location": { "type": t("string"), "description": "Where, at most 500 characters." },
        "remind_at": { "type": t("string"), "description": "When to remind the user: an ISO 8601 date-time with its offset, like 2026-11-20T09:00:00+05:30. Without an offset it is taken as UTC." },
        "recurrence": { "type": t("string"), "description": "How it repeats; needs a due_date. 'every day', 'every week', 'every month', 'every year', 'every 3 days', 'every 2 weeks', 'every weekday', or days like 'every monday, thursday'. Ticking it off moves it to its next date." },
    });
    match props {
        Value::Object(map) => map,
        _ => Map::new(),
    }
}

fn list_projects_def() -> Value {
    json!({
        "name": "list_projects",
        "title": "List projects",
        "description": "The user's projects, with their ids. Tasks with no project are in the Inbox.",
        "inputSchema": { "type": "object", "properties": {} },
        "annotations": read_only(),
    })
}

fn list_sections_def() -> Value {
    json!({
        "name": "list_sections",
        "title": "List sections",
        "description": "The sections projects are split into, with their ids.",
        "inputSchema": { "type": "object", "properties": {
            "project_id": { "type": "string", "description": "Only this project's sections." },
        } },
        "annotations": read_only(),
    })
}

fn list_tasks_def() -> Value {
    json!({
        "name": "list_tasks",
        "title": "List tasks",
        "description": "The user's tasks. By default the open tasks of every project in the user's order, each open task with its subtasks (done ones too). \
`more` is true when there are more after this page: ask again with offset moved on by limit, or narrow with project_id, inbox or q.",
        "inputSchema": { "type": "object", "properties": {
            "project_id": { "type": "string", "description": "Only this project's tasks." },
            "inbox": { "type": "boolean", "description": "Only the tasks with no project." },
            "completed": { "type": "boolean", "description": "The done tasks instead, newest first." },
            "q": { "type": "string", "description": "Search titles and descriptions for this text (open tasks soonest due first, or done ones with completed). Ignores project_id and inbox." },
            "limit": { "type": "integer", "minimum": 1, "maximum": MAX_LIMIT, "description": format!("At most this many; {DEFAULT_LIMIT} by default.") },
            "offset": { "type": "integer", "minimum": 0, "description": "Skip this many first, to read the next page. 0 by default." },
        } },
        "annotations": read_only(),
    })
}

fn get_task_def() -> Value {
    json!({
        "name": "get_task",
        "title": "Get a task",
        "description": "One task by its id, with all its fields.",
        "inputSchema": { "type": "object", "properties": { "id": id_prop() }, "required": ["id"] },
        "annotations": read_only(),
    })
}

fn create_task_def() -> Value {
    json!({
        "name": "create_task",
        "title": "Add a task",
        "description": "Adds a task (or, with parent_id, a subtask) and returns it. Use list_projects first to file it in a project.",
        "inputSchema": { "type": "object", "properties": task_props(false), "required": ["title"] },
        "annotations": { "readOnlyHint": false, "destructiveHint": false, "idempotentHint": false, "openWorldHint": false },
    })
}

fn update_task_def() -> Value {
    let mut props = task_props(true);
    props.insert("id".into(), id_prop());
    json!({
        "name": "update_task",
        "title": "Change a task",
        "description": "Changes only the fields sent; null clears one. Clearing due_date clears due_time and the reminder too. \
To tick a task off use complete_task; to undo that, reopen_task.",
        "inputSchema": { "type": "object", "properties": props, "required": ["id"] },
        "annotations": { "readOnlyHint": false, "destructiveHint": false, "idempotentHint": true, "openWorldHint": false },
    })
}

fn complete_task_def() -> Value {
    json!({
        "name": "complete_task",
        "title": "Complete a task",
        "description": "Ticks a task off, with its open subtasks. A repeating task stays open and moves to its next date instead.",
        "inputSchema": { "type": "object", "properties": {
            "id": id_prop(),
            "today": { "type": "string", "description": "The user's date today, YYYY-MM-DD, for a repeating task's next date. The server's date when left out." },
        }, "required": ["id"] },
        "annotations": { "readOnlyHint": false, "destructiveHint": false, "idempotentHint": false, "openWorldHint": false },
    })
}

fn reopen_task_def() -> Value {
    json!({
        "name": "reopen_task",
        "title": "Reopen a task",
        "description": "Undoes complete_task: the task is open again.",
        "inputSchema": { "type": "object", "properties": { "id": id_prop() }, "required": ["id"] },
        "annotations": { "readOnlyHint": false, "destructiveHint": false, "idempotentHint": true, "openWorldHint": false },
    })
}

fn delete_task_def() -> Value {
    json!({
        "name": "delete_task",
        "title": "Delete a task",
        "description": "Deletes a task and its subtasks from all the user's devices. Prefer complete_task for work that is done.",
        "inputSchema": { "type": "object", "properties": { "id": id_prop() }, "required": ["id"] },
        "annotations": { "readOnlyHint": false, "destructiveHint": true, "idempotentHint": true, "openWorldHint": false },
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::auth::Accounts;
    use crate::config::Config;
    use crate::{events, releases, store};

    struct Setup {
        dir: std::path::PathBuf,
        state: AppState,
        user_id: i64,
    }

    impl Drop for Setup {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.dir);
        }
    }

    fn setup() -> Setup {
        let dir = std::env::temp_dir().join(format!("tasks-mcp-test-{}", tasks_core::new_id()));
        std::fs::create_dir_all(&dir).unwrap();
        let config = Config {
            host: "127.0.0.1".into(),
            port: 0,
            base_path: "/tasks-api".into(),
            data_dir: dir.clone(),
            allow_signup: true,
            web_dir: dir.join("web"),
            env_file: None,
        };
        let accounts = Accounts::open(&config.accounts_db()).unwrap();
        let user_id = accounts.add_user("akash", "password1").unwrap().id;
        let state = AppState {
            store: Arc::new(store::Store::new(config.clone())),
            accounts: Arc::new(accounts),
            releases: Arc::new(releases::Releases::new()),
            hub: Arc::new(events::Hub::default()),
            config,
        };
        Setup { dir, state, user_id }
    }

    fn caller(t: &Setup, scope: McpScope) -> McpUser {
        McpUser { user_id: t.user_id, token_id: "test".into(), scope }
    }

    async fn rpc(t: &Setup, scope: McpScope, method: &str, params: Value) -> Value {
        let msg = json!({ "jsonrpc": "2.0", "id": 1, "method": method, "params": params });
        dispatch(&t.state, &caller(t, scope), msg).await.unwrap()
    }

    async fn tool(t: &Setup, scope: McpScope, name: &str, args: Value) -> Value {
        rpc(t, scope, "tools/call", json!({ "name": name, "arguments": args })).await["result"].clone()
    }

    fn names(list: &Value) -> Vec<&str> {
        list["result"]["tools"].as_array().unwrap().iter().map(|t| t["name"].as_str().unwrap()).collect()
    }

    fn titles(result: &Value) -> Vec<&str> {
        result["structuredContent"]["tasks"].as_array().unwrap().iter().map(|t| t["title"].as_str().unwrap()).collect()
    }

    #[tokio::test]
    async fn initialize_offers_tools_in_a_version_the_client_speaks() {
        let t = setup();
        let out = rpc(&t, McpScope::Read, "initialize", json!({ "protocolVersion": "2025-06-18" })).await;
        assert_eq!(out["result"]["protocolVersion"], "2025-06-18");
        assert!(out["result"]["capabilities"]["tools"].is_object());
        let out = rpc(&t, McpScope::Read, "initialize", json!({ "protocolVersion": "1999-01-01" })).await;
        assert_eq!(out["result"]["protocolVersion"], PROTOCOL_VERSIONS[0]);

        let note = json!({ "jsonrpc": "2.0", "method": "notifications/initialized" });
        assert!(dispatch(&t.state, &caller(&t, McpScope::Read), note).await.is_none());
        assert_eq!(rpc(&t, McpScope::Read, "nope", json!({})).await["error"]["code"], METHOD_NOT_FOUND);
    }

    #[tokio::test]
    async fn a_read_token_sees_and_runs_only_the_read_tools() {
        let t = setup();
        let read = names(&rpc(&t, McpScope::Read, "tools/list", json!({})).await).len();
        let all = names(&rpc(&t, McpScope::ReadWrite, "tools/list", json!({})).await).len();
        assert_eq!((read, all), (4, 9));

        let out = tool(&t, McpScope::Read, "create_task", json!({ "title": "Nope" })).await;
        assert_eq!(out["isError"], true);
        let out = tool(&t, McpScope::Read, "list_tasks", json!({})).await;
        assert!(titles(&out).is_empty());
    }

    #[tokio::test]
    async fn tasks_can_be_added_changed_completed_and_deleted() {
        let t = setup();
        let rw = McpScope::ReadWrite;
        let made = tool(&t, rw, "create_task", json!({ "title": "Write the report", "priority": 1, "due_date": "2026-12-01" })).await;
        assert_eq!(made["isError"], false);
        let id = made["structuredContent"]["id"].as_str().unwrap().to_string();
        tool(&t, rw, "create_task", json!({ "title": "Second" })).await;

        let changed = tool(&t, rw, "update_task", json!({ "id": id, "title": "Write the big report", "due_date": null })).await;
        assert_eq!(changed["structuredContent"]["title"], "Write the big report");
        assert!(changed["structuredContent"]["due_date"].is_null());
        assert_eq!(tool(&t, rw, "update_task", json!({ "id": id, "completed_at": "2026-11-19T00:00:00Z" })).await["isError"], true);

        let open = tool(&t, McpScope::Read, "list_tasks", json!({ "limit": 1 })).await;
        assert_eq!((titles(&open).len(), open["structuredContent"]["more"].as_bool()), (1, Some(true)));
        let next = tool(&t, McpScope::Read, "list_tasks", json!({ "limit": 1, "offset": 1 })).await;
        assert_eq!(next["structuredContent"]["more"], false);
        assert_ne!(titles(&next), titles(&open));

        tool(&t, rw, "complete_task", json!({ "id": id })).await;
        let done = tool(&t, McpScope::Read, "list_tasks", json!({ "completed": true })).await;
        assert_eq!(titles(&done), ["Write the big report"]);
        tool(&t, rw, "reopen_task", json!({ "id": id })).await;
        assert_eq!(tool(&t, rw, "get_task", json!({ "id": id })).await["structuredContent"]["completed_at"], Value::Null);

        tool(&t, rw, "delete_task", json!({ "id": id })).await;
        assert_eq!(titles(&tool(&t, rw, "list_tasks", json!({})).await), ["Second"]);
        assert_eq!(tool(&t, rw, "get_task", json!({ "id": id })).await["isError"], true);
    }

    #[tokio::test]
    async fn bad_input_is_a_tool_error_the_model_can_read() {
        let t = setup();
        let out = tool(&t, McpScope::ReadWrite, "create_task", json!({ "title": "" })).await;
        assert_eq!(out["isError"], true);
        assert!(out["content"][0]["text"].as_str().unwrap().contains("title"));
        let out = tool(&t, McpScope::ReadWrite, "create_task", json!({ "title": "x", "priority": 9 })).await;
        assert_eq!(out["isError"], true);
        let out = rpc(&t, McpScope::ReadWrite, "tools/call", json!({ "name": "drop_tables" })).await;
        assert_eq!(out["error"]["code"], INVALID_PARAMS);
    }

    #[test]
    fn an_origin_must_match_the_host() {
        let headers = |pairs: &[(&'static str, &'static str)]| {
            let mut h = HeaderMap::new();
            for (k, v) in pairs {
                h.insert(*k, v.parse().unwrap());
            }
            h
        };
        assert!(origin_allowed(&headers(&[("host", "tasks.example.com")])));
        assert!(origin_allowed(&headers(&[("host", "tasks.example.com"), ("origin", "https://tasks.example.com")])));
        assert!(origin_allowed(&headers(&[("host", "127.0.0.1:8050"), ("x-forwarded-host", "tasks.example.com"), ("origin", "https://tasks.example.com")])));
        assert!(!origin_allowed(&headers(&[("host", "tasks.example.com"), ("origin", "https://evil.example")])));
        assert!(!origin_allowed(&headers(&[("host", "tasks.example.com"), ("origin", "null")])));
        assert!(origin_allowed(&headers(&[("host", "tasks.example.com"), ("origin", "chrome-extension://abcdefghijklmnop")])));
    }
}
