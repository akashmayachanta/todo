# Instructions for Claude

Tasks is a to-do app with its own sync server: a Rust server, a Windows app and an
Android APK. Each device keeps its own copy of the tasks and works offline; the
server keeps them in step. See `README.md` for how it works and `BUILD.md` for setup.

## Don't build

Never build anything in this repo. That includes `build-android.sh`,
`build-windows.bat`, `build-server.bat`, `npm run build`, `npm run android:build`,
`tauri build` and `cargo build`.

The user builds manually and reports any errors. Stop at the code changes and
list the build commands they need to run.

## Working style

- Keep turns short. Report back rather than running long checks or test setups.
- Quick checks are fine: `npm run check` in `app/`, and `cargo check -p <crate>` or
  `cargo test -p tasks-core` from the repo root. Say what they don't cover.
- Don't commit unless asked. The working tree often holds the user's own unfinished changes.

## Layout

| Path | What it is |
|---|---|
| `core/` | `tasks-core`: the rules (`ops.rs`), sync merge (`sync.rs`), device sync client (`client.rs`) |
| `core/migrations/` | The tasks DB schema. Never edit a shipped migration; add the next numbered file |
| `server/` | `tasks-server`: Axum REST API, accounts, `/sync`, tray icon |
| `app/src/` | SvelteKit UI (Svelte 5 runes, Tailwind 4, static adapter) |
| `app/src/lib/` | Tasks UI: `components/tasks/`, smart parsing in `taskParse.ts`, links in `links.ts` |
| `app/src/app/` | App shell, imported as `$shell`: setup, settings, sync status |
| `app/src/routes/tasks/` | The main page and `tasks.css` |
| `app/src/routes/widget/` | The desktop's small windows: the quick add popup and pins (`src-tauri/src/widgets.rs`) |
| `app/src-tauri/` | Tauri 2 Rust side: local DB commands, sync loop (`syncer.rs`), reminders, tray |
| `app/scripts/` | Node helpers for icons and the Android project |

## Things that are easy to get wrong

**Data**
- All edits go to the device's local DB through `tasks-core`, marked `dirty`, and
  the sync loop pushes them. Merging is last write wins per row, and deletes are
  tombstones, so rows are never removed.
- Rules belong in `core/`, so the apps and the server share them.
- User data: `%APPDATA%\dev.tasks.app\tasks.db` on the desktop, `data/` on the server.
  Back up before changing either, and prefer the app's own paths. Device tokens are
  in the Credential Manager; don't read them.

**Task text**
- Titles and descriptions are stored as typed, including `[label](https://…)`.
  Show them with `LinkedText.svelte`, or `plainText()` where a link can't go
  (aria labels, toasts, calendar chips). Rust uses `tasks_core::plain_title`.
- An `<a>` can't sit inside a `<button>`, so task rows are clickable divs with a
  `.task-open` button for the keyboard.
- Smart parsing skips links. `parseQuickAdd` token offsets are UTF-16 units.

**Sync UI**
- The page reloads on the `tasks://changed` event. "Synced" only means this device's
  last round worked; another device's edits can take up to 2 minutes to arrive.

**Icons**
- Both are green `#14532d`: Windows from `icons-src/icon.svg`, Android from `icons-src/icon-android*`.
  `npm run icons` regenerates both, and re-encodes `icon.icns` even when nothing changed.
- Android builds use `scripts/android-icons.mjs` to copy the icons in.

**Windows' small windows**
- Each window has its own copy of the tasks. Writes in `tauriClient.ts` emit
  `tasks://local-changed` so the others reload; `tasks://changed` only fires after a sync.
- A pin's label says what it shows (`pin-project-<id>`). Closing one unpins it, so
  anything else that removes the window must use `destroy()`.
- Creating a window from a sync command deadlocks on Windows, so those commands are async.

**Android**
- It is built in WSL from a Linux copy at `~/todo`. `build-android.sh` syncs code there
  but skips `app/src-tauri/gen/android`, which exists only in that copy, so a change
  meant for that folder must be applied by a script in `app/scripts/`.

**Tools on Windows**
- Run `cargo` from PowerShell. In Git Bash, its `link.exe` shadows the MSVC linker.
- Don't call `wsl` with a `/mnt/...` path from Git Bash, which rewrites the path.
- Backslashes in Bash heredocs can get mangled. Use the Write tool for scripts and
  anything containing regexes.
