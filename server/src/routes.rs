//! The REST API for tasks, device sign-in and the sync endpoints. Every route
//! is under TASKS_API_BASE_PATH (/tasks-api by default).

use std::sync::Arc;

use axum::body::Bytes;
use axum::extract::{FromRequest, FromRequestParts, Path, Query, Request, State};
use axum::http::request::Parts;
use axum::http::{header, HeaderValue, StatusCode};
use axum::response::{IntoResponse, Redirect, Response};
use axum::routing::{get, post};
use axum::{Json, Router};
use serde::de::DeserializeOwned;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use tasks_core::sync::{self, PullResponse, PushRequest, PushResponse};
use tower_http::services::{ServeDir, ServeFile};
use tasks_core::{ops, Section, SectionIn, Completions, Ctx, Filter, FilterIn, Project, ProjectIn, ProjectOrder, Profile, ProfilePatch, Task, TaskIn, TaskOrder, TaskPatch, TaskQuery};

use crate::auth::{AuthUser, Device};
use crate::error::{ApiError, ApiResult};
use crate::releases::{Platform, Release};
use crate::AppState;

type St = State<Arc<AppState>>;

/// A JSON body; a bad one is a 422 with a readable detail.
pub struct Body<T>(pub T);

impl<S: Send + Sync, T: DeserializeOwned> FromRequest<S> for Body<T> {
    type Rejection = ApiError;

    async fn from_request(req: Request, state: &S) -> Result<Self, Self::Rejection> {
        let bytes = Bytes::from_request(req, state)
            .await
            .map_err(|e| ApiError::new(StatusCode::BAD_REQUEST, e.body_text()))?;
        let bytes = if bytes.is_empty() { Bytes::from_static(b"{}") } else { bytes };
        serde_json::from_slice(&bytes).map(Body).map_err(|e| ApiError::unprocessable(e.to_string()))
    }
}

/// Query parameters; bad ones are a 422.
pub struct Q<T>(pub T);

impl<S: Send + Sync, T: DeserializeOwned> FromRequestParts<S> for Q<T> {
    type Rejection = ApiError;

    async fn from_request_parts(parts: &mut Parts, _: &S) -> Result<Self, Self::Rejection> {
        Query::<T>::try_from_uri(&parts.uri).map(|q| Q(q.0)).map_err(|e| ApiError::unprocessable(e.body_text()))
    }
}

fn ok() -> Json<Value> {
    Json(json!({ "ok": true }))
}

/// Runs a write on the user's DB, then tells their other devices to sync.
async fn write<T, F>(s: &AppState, u: &AuthUser, f: F) -> ApiResult<T>
where
    T: Send + 'static,
    F: FnOnce(&rusqlite::Connection) -> tasks_core::Result<T> + Send + 'static,
{
    let out = s.store.run(u.user_id, f).await?;
    s.hub.notify(u.user_id, &u.device_id);
    Ok(out)
}

pub fn router(state: Arc<AppState>) -> Router {
    let base = state.config.base_path.clone();
    let p = |s: &str| format!("{base}{s}");
    let mut router = Router::new();
    if !base.is_empty() {
        router = router.route(&p(""), get(list_tasks).post(create_task));
    }
    router = web_routes(router, &state.config.web_dir, base.is_empty());
    router
        .route(&p("/"), get(list_tasks).post(create_task))
        .route(&p("/health"), get(health))
        .route(&p("/projects"), get(list_projects).post(create_project))
        .route(&p("/projects/reorder"), post(reorder_projects))
        .route(&p("/projects/{id}"), post(update_project))
        .route(&p("/projects/{id}/delete"), post(delete_project))
        .route(&p("/sections"), get(list_sections).post(create_section))
        .route(&p("/sections/reorder"), post(reorder_sections))
        .route(&p("/sections/{id}"), post(update_section))
        .route(&p("/sections/{id}/delete"), post(delete_section))
        .route(&p("/filters"), get(list_filters).post(create_filter))
        .route(&p("/filters/{id}"), post(update_filter))
        .route(&p("/filters/{id}/delete"), post(delete_filter))
        .route(&p("/profile"), get(get_profile).post(update_profile))
        .route(&p("/completions"), get(list_completions))
        .route(&p("/reorder"), post(reorder_tasks))
        .route(&p("/delete-completed"), post(delete_completed))
        .route(&p("/auth/login"), post(login))
        .route(&p("/auth/register"), post(register))
        .route(&p("/auth/me"), get(me))
        .route(&p("/auth/devices"), get(list_devices))
        .route(&p("/auth/devices/{id}/revoke"), post(revoke_device))
        .route(&p("/sync/push"), post(push))
        .route(&p("/sync/pull"), get(pull))
        .route(&p("/sync/events"), get(events))
        .route(&p("/app/latest"), get(latest_release))
        .route(&p("/app/download"), get(download_release))
        .route(&p("/{id}"), get(get_task).post(update_task))
        .route(&p("/{id}/delete"), post(delete_task))
        .route(&p("/{id}/complete"), post(complete_task))
        .route(&p("/{id}/reopen"), post(reopen_task))
        .layer(axum::extract::DefaultBodyLimit::max(32 * 1024 * 1024))
        .with_state(state)
}

async fn health() -> Json<Value> {
    Json(json!({ "ok": true, "service": "tasks-server", "version": env!("CARGO_PKG_VERSION") }))
}

// ── Projects ─────────────────────────────────────────────────────────────────

async fn list_projects(State(s): St, u: AuthUser) -> ApiResult<Json<Vec<Project>>> {
    Ok(Json(s.store.run(u.user_id, ops::list_projects).await?))
}

async fn create_project(State(s): St, u: AuthUser, Body(input): Body<ProjectIn>) -> ApiResult<(StatusCode, Json<Project>)> {
    let item = write(&s, &u, move |c| ops::create_project(c, &Ctx::server(), input)).await?;
    Ok((StatusCode::CREATED, Json(item)))
}

async fn reorder_tasks(State(s): St, u: AuthUser, Body(order): Body<TaskOrder>) -> ApiResult<Json<Vec<Task>>> {
    Ok(Json(write(&s, &u, move |c| ops::reorder_tasks(c, &Ctx::server(), &order.ids)).await?))
}

async fn delete_completed(State(s): St, u: AuthUser, Body(order): Body<TaskOrder>) -> ApiResult<Json<Value>> {
    let deleted = write(&s, &u, move |c| ops::delete_completed(c, &Ctx::server(), &order.ids)).await?;
    Ok(Json(json!({ "deleted": deleted })))
}

async fn reorder_projects(State(s): St, u: AuthUser, Body(order): Body<ProjectOrder>) -> ApiResult<Json<Vec<Project>>> {
    Ok(Json(write(&s, &u, move |c| ops::reorder_projects(c, &Ctx::server(), &order.ids)).await?))
}

async fn update_project(State(s): St, u: AuthUser, Path(id): Path<String>, Body(input): Body<ProjectIn>) -> ApiResult<Json<Project>> {
    Ok(Json(write(&s, &u, move |c| ops::update_project(c, &Ctx::server(), &id, input)).await?))
}

async fn delete_project(State(s): St, u: AuthUser, Path(id): Path<String>) -> ApiResult<Json<Value>> {
    write(&s, &u, move |c| ops::delete_project(c, &Ctx::server(), &id)).await?;
    Ok(ok())
}

// ── Sections ───────────────────────────────────────────────────────────────

#[derive(Deserialize)]
struct SectionQuery {
    project_id: Option<String>,
}

async fn list_sections(State(s): St, u: AuthUser, Q(q): Q<SectionQuery>) -> ApiResult<Json<Vec<Section>>> {
    Ok(Json(s.store.run(u.user_id, move |c| ops::list_sections(c, q.project_id.as_deref())).await?))
}

async fn create_section(State(s): St, u: AuthUser, Body(input): Body<SectionIn>) -> ApiResult<(StatusCode, Json<Section>)> {
    let item = write(&s, &u, move |c| ops::create_section(c, &Ctx::server(), input)).await?;
    Ok((StatusCode::CREATED, Json(item)))
}

#[derive(Deserialize)]
struct SectionOrder {
    project_id: String,
    ids: Vec<String>,
}

async fn reorder_sections(State(s): St, u: AuthUser, Body(order): Body<SectionOrder>) -> ApiResult<Json<Vec<Section>>> {
    Ok(Json(write(&s, &u, move |c| ops::reorder_sections(c, &Ctx::server(), &order.project_id, &order.ids)).await?))
}

async fn update_section(State(s): St, u: AuthUser, Path(id): Path<String>, Body(input): Body<SectionIn>) -> ApiResult<Json<Section>> {
    Ok(Json(write(&s, &u, move |c| ops::update_section(c, &Ctx::server(), &id, input)).await?))
}

async fn delete_section(State(s): St, u: AuthUser, Path(id): Path<String>) -> ApiResult<Json<Value>> {
    write(&s, &u, move |c| ops::delete_section(c, &Ctx::server(), &id)).await?;
    Ok(ok())
}

// ── Filters ──────────────────────────────────────────────────────────────────

async fn list_filters(State(s): St, u: AuthUser) -> ApiResult<Json<Vec<Filter>>> {
    Ok(Json(s.store.run(u.user_id, ops::list_filters).await?))
}

async fn create_filter(State(s): St, u: AuthUser, Body(input): Body<FilterIn>) -> ApiResult<(StatusCode, Json<Filter>)> {
    let item = write(&s, &u, move |c| ops::create_filter(c, &Ctx::server(), input)).await?;
    Ok((StatusCode::CREATED, Json(item)))
}

async fn update_filter(State(s): St, u: AuthUser, Path(id): Path<String>, Body(input): Body<FilterIn>) -> ApiResult<Json<Filter>> {
    Ok(Json(write(&s, &u, move |c| ops::update_filter(c, &Ctx::server(), &id, input)).await?))
}

async fn delete_filter(State(s): St, u: AuthUser, Path(id): Path<String>) -> ApiResult<Json<Value>> {
    write(&s, &u, move |c| ops::delete_filter(c, &Ctx::server(), &id)).await?;
    Ok(ok())
}

// ── Profile ──────────────────────────────────────────────────────────────────

/// The account's display name and photo: the synced `profile` row, for the browser
/// (the apps read their own copy).
async fn get_profile(State(s): St, u: AuthUser) -> ApiResult<Json<Profile>> {
    Ok(Json(s.store.run(u.user_id, ops::get_profile).await?))
}

async fn update_profile(State(s): St, u: AuthUser, Body(patch): Body<ProfilePatch>) -> ApiResult<Json<Profile>> {
    Ok(Json(write(&s, &u, move |c| ops::update_profile(c, &Ctx::server(), patch)).await?))
}

// ── Tasks ────────────────────────────────────────────────────────────────────

async fn list_tasks(State(s): St, u: AuthUser, Q(q): Q<TaskQuery>) -> ApiResult<Json<Vec<Task>>> {
    Ok(Json(s.store.run(u.user_id, move |c| ops::list_tasks(c, &q)).await?))
}

#[derive(Deserialize)]
struct SinceQuery {
    since: String,
}

async fn list_completions(State(s): St, u: AuthUser, Q(q): Q<SinceQuery>) -> ApiResult<Json<Completions>> {
    Ok(Json(s.store.run(u.user_id, move |c| ops::list_completions(c, &q.since)).await?))
}

async fn get_task(State(s): St, u: AuthUser, Path(id): Path<String>) -> ApiResult<Json<Task>> {
    Ok(Json(s.store.run(u.user_id, move |c| ops::get_task(c, &id)).await?))
}

async fn create_task(State(s): St, u: AuthUser, Body(input): Body<TaskIn>) -> ApiResult<(StatusCode, Json<Task>)> {
    let item = write(&s, &u, move |c| ops::create_task(c, &Ctx::server(), input)).await?;
    Ok((StatusCode::CREATED, Json(item)))
}

async fn update_task(State(s): St, u: AuthUser, Path(id): Path<String>, Body(patch): Body<TaskPatch>) -> ApiResult<Json<Task>> {
    Ok(Json(write(&s, &u, move |c| ops::update_task(c, &Ctx::server(), &id, patch)).await?))
}

#[derive(Deserialize)]
struct CompleteIn {
    /// The caller's local date, `2026-11-05`: a repeating task moves to its next
    /// date after it. The server's own date when left out.
    today: Option<String>,
}

/// Ticks a task: core decides what that changes (ops::complete_task). A repeating
/// task's reminder moves by whole days in the server's time zone.
async fn complete_task(State(s): St, u: AuthUser, Path(id): Path<String>, Body(input): Body<CompleteIn>) -> ApiResult<Json<Task>> {
    let today = match input.today.as_deref() {
        Some(day) => chrono::NaiveDate::parse_from_str(day, "%Y-%m-%d")
            .map_err(|_| ApiError::unprocessable("today must be a date like 2026-11-05".to_string()))?,
        None => chrono::Local::now().date_naive(),
    };
    Ok(Json(write(&s, &u, move |c| ops::complete_task(c, &Ctx::server(), &id, today)).await?))
}

async fn reopen_task(State(s): St, u: AuthUser, Path(id): Path<String>) -> ApiResult<Json<Task>> {
    Ok(Json(write(&s, &u, move |c| ops::reopen_task(c, &Ctx::server(), &id)).await?))
}

async fn delete_task(State(s): St, u: AuthUser, Path(id): Path<String>) -> ApiResult<Json<Value>> {
    write(&s, &u, move |c| ops::delete_task(c, &Ctx::server(), &id)).await?;
    Ok(ok())
}

// ── Devices ──────────────────────────────────────────────────────────────────

async fn me(u: AuthUser) -> Json<Value> {
    Json(json!({ "user_id": u.user_id, "username": u.username, "device_id": u.device_id }))
}

#[derive(Deserialize)]
struct LoginIn {
    username: String,
    password: String,
    /// What the device is called in the device list, e.g. "Windows (DESKTOP-1)".
    #[serde(default)]
    device_name: String,
}

fn device_name(name: &str) -> String {
    let name = name.trim();
    if name.is_empty() { "Device".to_string() } else { name.chars().take(100).collect() }
}

#[derive(Serialize)]
struct LoginOut {
    /// Shown once; only its hash is kept.
    token: String,
    device: Device,
    user_id: i64,
    username: String,
}

/// Signs a device in: a username and password for a long-lived device token.
async fn login(State(s): St, Body(input): Body<LoginIn>) -> ApiResult<(StatusCode, Json<LoginOut>)> {
    let name = device_name(&input.device_name);
    let accounts = s.accounts.clone();
    let (user, device, token) = tokio::task::spawn_blocking(move || accounts.login(&input.username, &input.password, &name))
        .await
        .map_err(|e| ApiError::internal(e.to_string()))??;
    tracing::info!("{} signed in a new device, {}", user.username, device.name);
    Ok((StatusCode::CREATED, Json(LoginOut { token, device, user_id: user.id, username: user.username })))
}

/// Signs up: a new account, and a device token for the device that made it.
/// TASKS_ALLOW_SIGNUP=false turns it off.
async fn register(State(s): St, Body(input): Body<LoginIn>) -> ApiResult<(StatusCode, Json<LoginOut>)> {
    if !s.config.allow_signup {
        return Err(ApiError::forbidden("Sign-up is turned off on this server"));
    }
    let name = device_name(&input.device_name);
    let accounts = s.accounts.clone();
    let (user, device, token) = tokio::task::spawn_blocking(move || accounts.register(&input.username, &input.password, &name))
        .await
        .map_err(|e| ApiError::internal(e.to_string()))??;
    tracing::info!("{} signed up on {}", user.username, device.name);
    Ok((StatusCode::CREATED, Json(LoginOut { token, device, user_id: user.id, username: user.username })))
}

async fn list_devices(State(s): St, u: AuthUser) -> ApiResult<Json<Vec<Device>>> {
    let accounts = s.accounts.clone();
    let list = tokio::task::spawn_blocking(move || accounts.list_devices(u.user_id))
        .await
        .map_err(|e| ApiError::internal(e.to_string()))??;
    Ok(Json(list))
}

async fn revoke_device(State(s): St, u: AuthUser, Path(id): Path<String>) -> ApiResult<Json<Value>> {
    let accounts = s.accounts.clone();
    let found = tokio::task::spawn_blocking(move || accounts.revoke(u.user_id, &id))
        .await
        .map_err(|e| ApiError::internal(e.to_string()))??;
    if !found {
        return Err(ApiError::new(StatusCode::NOT_FOUND, "Device not found"));
    }
    Ok(ok())
}

// ── Sync ─────────────────────────────────────────────────────────────────────

async fn push(State(s): St, u: AuthUser, Body(req): Body<PushRequest>) -> ApiResult<Json<PushResponse>> {
    let out = s
        .store
        .run(u.user_id, move |c| {
            let applied = sync::apply(c, &Ctx::server(), &req.rows)?;
            Ok(PushResponse { accepted: applied.accepted, ignored: applied.ignored, seq: sync::current_seq(c)? })
        })
        .await?;
    if out.accepted > 0 {
        s.hub.notify(u.user_id, &u.device_id);
    }
    Ok(Json(out))
}

/// The live notices (events.rs): `changed` when another device wrote something.
async fn events(State(s): St, u: AuthUser) -> impl axum::response::IntoResponse {
    // x-accel-buffering: nginx mustn't hold the events in its buffer.
    // no-transform: Cloudflare mustn't compress the stream, which holds events back too.
    ([("x-accel-buffering", "no"), ("cache-control", "no-cache, no-transform")], s.hub.stream(u.user_id, u.device_id))
}

#[derive(Deserialize)]
struct PullQuery {
    #[serde(default)]
    since: i64,
    limit: Option<usize>,
}

async fn pull(State(s): St, u: AuthUser, Q(q): Q<PullQuery>) -> ApiResult<Json<PullResponse>> {
    let limit = q.limit.unwrap_or(500).clamp(1, 2000);
    Ok(Json(s.store.run(u.user_id, move |c| sync::changes_since(c, q.since, limit)).await?))
}

// ── App updates (releases.rs) ────────────────────────────────────────────────

#[derive(Deserialize)]
struct PlatformQuery {
    platform: Platform,
}

/// The newest build of the app for a platform: its version, SHA-256 and size.
async fn latest_release(State(s): St, _u: AuthUser, Q(q): Q<PlatformQuery>) -> ApiResult<Json<Release>> {
    let releases = s.releases.clone();
    let release = tokio::task::spawn_blocking(move || releases.latest(q.platform))
        .await
        .map_err(|e| ApiError::internal(e.to_string()))??;
    Ok(Json(release))
}

async fn download_release(State(s): St, _u: AuthUser, Q(q): Q<PlatformQuery>) -> ApiResult<axum::response::Response> {
    let releases = s.releases.clone();
    tokio::task::spawn_blocking(move || releases.download(q.platform)).await.map_err(|e| ApiError::internal(e.to_string()))?
}

// ── The web UI ───────────────────────────────────────────────────────────────

/// Where the browser copy of the app lives (app/build-web, built with base path /tasks-app).
const WEB_PATH: &str = "/tasks-app";

/// Serves the web UI at /tasks-app/ (and sends / there, unless the API sits at the root).
/// Skipped, with a log line, when the folder isn't there.
fn web_routes(router: Router<Arc<AppState>>, dir: &std::path::Path, api_at_root: bool) -> Router<Arc<AppState>> {
    if !dir.join("index.html").is_file() {
        tracing::info!("No web UI at {} (TASKS_WEB_DIR); {WEB_PATH}/ is off", dir.display());
        return router;
    }
    tracing::info!("Web UI at {WEB_PATH}/ from {}", dir.display());
    let to_app = || async { Redirect::to(&format!("{WEB_PATH}/")) };
    let mut router = router
        .route(WEB_PATH, get(to_app))
        .route(&format!("{WEB_PATH}/"), get(web_app))
        .route(&format!("{WEB_PATH}/{{*path}}"), get(web_app));
    if !api_at_root {
        router = router.route("/", get(to_app));
    }
    router
}

/// A file from the web UI's folder; any other path is the single-page app's index.html,
/// so deep links like /tasks-app/tasks?project=… load the app.
async fn web_app(State(s): St, req: Request) -> Response {
    let dir = s.config.web_dir.clone();
    let (mut parts, body) = req.into_parts();
    let rest = parts.uri.path().strip_prefix(WEB_PATH).unwrap_or("/");
    let rest = if rest.is_empty() { "/" } else { rest };
    let uri = match parts.uri.query() {
        Some(q) => format!("{rest}?{q}"),
        None => rest.to_string(),
    };
    parts.uri = match uri.parse() {
        Ok(u) => u,
        Err(_) => return StatusCode::BAD_REQUEST.into_response(),
    };
    let req = Request::from_parts(parts, body);
    let index = dir.join("index.html");
    let res = match ServeDir::new(&dir).fallback(ServeFile::new(&index)).try_call(req).await {
        Ok(res) => res,
        Err(e) => return ApiError::internal(e.to_string()).into_response(),
    };
    let mut res = res.map(axum::body::Body::new);
    // index.html names the current build's files, so it is always checked again.
    if res.headers().get(header::CONTENT_TYPE).is_some_and(|t| t.as_bytes().starts_with(b"text/html")) {
        res.headers_mut().insert(header::CACHE_CONTROL, HeaderValue::from_static("no-cache"));
    }
    res
}
