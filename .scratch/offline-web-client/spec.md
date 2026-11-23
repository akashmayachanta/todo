# Offline web client

Status: needs-triage

The browser copy (`/tasks-app/`, installable as a PWA) is online only: `webClient.ts` calls
the REST API directly, so with no connection the shell opens (service worker) but no tasks
load and nothing can be saved. The Windows and Android apps keep a local copy through
`tasks-core` and sync it. This brings the browser copy closer to that.

## Sketch

- **Local store**: the tasks, projects and sections in IndexedDB, per signed-in account.
  Cleared on sign-out.
- **Edits**: written locally and marked dirty, as `tasks-core` does, then pushed. Same
  protocol as the apps: `/sync` push and pull, last write wins per row, deletes as
  tombstones. The rules stay in Rust, so options are:
  1. compile `tasks-core`'s ops and merge to WASM with SQLite-in-WASM (wa-sqlite / sqlite
     wasm + OPFS), so the browser runs the same code as the apps, or
  2. reimplement the client side in TypeScript. Smaller, but it duplicates the rules
     and can drift from `ops.rs` / `sync.rs` / `recurrence.rs` / `reminder.rs`.
  Option 1 is preferred; it needs a spike to check size and OPFS support on iOS Safari.
- **Sync loop**: push and pull on start, on `online`, on `visibilitychange`, every 30 s
  while open, and when `/sync/events` says to (as `syncer.rs` does).
- **Sync status**: the account menu shows synced / offline / pending like the apps.
- **Background sync**: only while the page is open. Periodic Background Sync is
  Chromium-only, installed-PWA-only and throttled; iOS has none. Not worth relying on.
- **Several tabs**: one writer (Web Locks or a SharedWorker), others reload on a
  BroadcastChannel message, like `tasks://local-changed` between the desktop windows.

## Out of scope

- Reminders: `.scratch/web-push-reminders/`.
- Home-screen widgets: a web app can't add them on Android or iOS.
- Desktop and Server settings tabs: they're for Windows and the apps' own server address;
  the browser copy is always served by its own server.
