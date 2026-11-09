# AGENTS.md

Guidance for agents working in the **Tab Group Exporter** repository.

## What this is

A Chrome **Manifest V3** extension (v1.6.0) that exports your browser's **open and
closed tab groups** as a single list you can copy, save, or share. The extension
has no build step, no dependencies to install, and no test suite.

The key product idea: Chrome does **not** expose closed tab groups to extensions
(its own "saved groups" are unreadable). So this extension maintains its own
**archive** of every group it has ever observed, keeping the last known tab list
even after the group is closed. That archive is the only reason closed groups
appear in exports.

## File map

| File | Role |
| --- | --- |
| `manifest.json` | MV3 config: permissions, background service worker (`type: module`), popup, icons. |
| `background.js` | Service worker. Debounces tab/group events and triggers archive snapshots. |
| `archive.js` | Core data layer. Reads/writes the group archive in `chrome.storage.local`. |
| `durations.js` | Fetches and caches YouTube video lengths; computes group time totals. |
| `popup.html` | Popup markup + all CSS (theme variables, light/dark, pill and share styling). |
| `popup.js` | Popup logic: collect → format → render, copy, save, share, QR, the Tasks panel. |
| `tasks.js` | Tasks server client: sign-in, and syncing groups as tasks (Learning → YT) with videos as subtasks. |
| `vendor/qrcode.js` | Vendored `qrcode-generator` 1.4.4 (MIT), wrapped as an ES module. |
| `icons/` | Extension icons (16/32/48/128 px). |
| `YT-List.md` | A **sample export** (generated output). Not loaded by the extension. |
| `YT-List.html` | A **standalone viewer** for a sample export (chips, search, collapse). Not part of the build. |

`YT-List.md` and `YT-List.html` are demo/companion artifacts. Do not treat them as
source, and do not wire them into the extension.

## No build, no tests

Code runs as-is. To test manually:

1. `chrome://extensions` → enable **Developer mode**.
2. **Load unpacked** → select this folder.
3. After editing any script, click the extension's **Reload** button (the service
   worker and popup modules are cached by Chrome until then).
4. Open the popup and exercise tab groups. Inspect the service worker console and
   popup console for errors.

There is no bundler, linter, or test runner. Keep the source browser-ready:
plain ES modules, no transpilation, no new runtime dependencies without a strong
reason.

## Architecture

### Event flow (archive)

```
chrome.tabGroups.onCreated/onUpdated/onRemoved
chrome.tabs.onCreated/onMoved/onAttached/onDetached/onUpdated/onRemoved
        │
        ▼
scheduleSnapshot()            background.js — debounced 1500 ms
        │
        ▼
snapshot()                    archive.js — writes chrome.storage.local
```

- **Debounce is load-bearing.** Opening a window or closing a group fires a burst
  of events. Saving mid-burst would replace a full tab list with a half-empty one.
- `tabs.onRemoved` **ignores** `info.isWindowClosing`, so closing a window keeps the
  group's last full snapshot instead of recording an emptied group.
- `tabs.onUpdated` only schedules when `url`, `title`, or `groupId` changed.

### Popup flow

```
refresh()                     popup.js — guarded by a monotonically-increasing refreshToken
  collectGroups()             re-snapshots, then queries tabs + tabGroups, orders by window/index
  [attachDurations()]         only when "Include video times" is on and a YouTube tab exists
  state.groups = groups
  renderPills() + formatters[format]()  →  <textarea> preview
```

`refreshToken` guards against stale async work: an older `refresh()` that resolves
after a newer one bails out instead of overwriting the UI.

### Data model

`chrome.storage.local`:

- `groupArchive` — object keyed by **`title|color`** (or `untitled|<id>`):

  ```js
  {
    "Work|blue": {
      title: "Work",
      color: "blue",
      tabs: [{ title, url, video?, seconds? }],
      open: true,
      liveId: 42,            // current Chrome group id, null when closed
      liveSession: "<uuid>", // sessionId captured when live
      firstSeen: 1700000000000,
      lastSeen:  1700000000000
    }
  }
  ```

- `durationCache` — `{ "<videoId>": seconds }`, never invalidated (lengths are stable).
- `tasksConfig` — `{ serverUrl, token, username, deviceId }` for the Tasks server.
  `serverUrl` includes the base path (e.g. `http://host:8050/tasks-api`). The
  password is never stored; signing out revokes the device token on the server.

`chrome.storage.session`:

- `sessionId` — a UUID, cleared when Chrome restarts. Prevents a brand-new group
  from being mistaken for an old one that reused the same Chrome group id.

`localStorage` (popup only):

- `tge-options` — the checkbox + format selections, restored on open.

### Why groups are keyed by `title|color`

Chrome assigns **new group ids every session**, so ids are not stable identity.
Title + color is what a user perceives as "the same group." `snapshot()` handles
renames/recolors by deleting the entry stored under the old key when its `liveId`
and `liveSession` both match the group currently being written.

## Invariants and gotchas

- **Session identity matters.** When comparing "is this the same live group," always
  require `liveSession === sessionId`; ids alone are ambiguous across restarts.
- **`forgetGroup` only deletes closed entries.** It refuses to remove an open group.
- **Duration fetch is best-effort.** Failures (offline, blocked) leave the tab
  without `seconds` and are silently skipped; `groupTime().partial` then flags the
  group total with a `+` in the UI. Never make a missing duration fatal.
- **`attachDurations` mutates the tab objects in place** (sets `video`/`seconds`)
  and runs a fixed worker pool (`CONCURRENCY = 6`). Preserve the pool pattern if you
  touch it — do not fan out unbounded requests.
- **QR codes must stay black-on-white** even in dark mode (`.qr-box canvas` forces
  `#fff`); inverting them breaks scanners.
- **QR size cap.** `MAX_QR_MODULES = 97` (~version 20). Larger payloads are dropped
  to the "too many links" message. Try EC level `M` first, fall back to `L`.
- **`escapeMd` only escapes `[`, `]`, and `\`.** Titles containing other Markdown
  metacharacters are not escaped. Change deliberately.
- **Save-As uses a `data:` URL**, not a blob URL: a blob URL is revoked when the
  popup closes behind the OS dialog. Keep that reasoning.
- **Share trimming is line-aware.** `fitText` drops whole lines so a link is never
  cut in half, and the full text is copied to the clipboard when trimming occurs.
- **Permissions are minimal.** `youtube.com` host permission exists solely for the
  duration fetches in `durations.js`. Do not broaden it without cause. The
  Tasks server's host is an **optional** permission, requested for that one
  origin at sign-in; keep `chrome.permissions.request` the first `await` in the
  click handler, or Chrome drops the user gesture and refuses.
- **Add to Tasks syncs; the browser wins.** A group matches the open top-level
  task in Learning → YT with the same title (trimmed, case-insensitive); groups
  sent together with the same title are merged first so they share one task.
  Videos match existing subtasks by **video id** from the subtask's link,
  falling back to the plain title. After syncing, the open subtasks are exactly
  the group's videos:
  - a video with no subtask is **added**;
  - a ticked-off subtask whose video is in the group is **reopened**
    (`completed_at: null`), even if it was ticked in the app on purpose;
  - an open subtask that links a video no longer in the group is **ticked off**
    (`completed_at: <now>`; the server has no other done flag). Closing a tab
    means "watched."
  This applies to closed (archived) groups too: their last known tabs count as
  still to watch. Subtasks with no video link (typed in the app) are never
  touched, and nothing is ever deleted. A group with no videos left still
  syncs an existing task (ticking the rest) but never creates one. Only the
  groups being sent are synced, so with "current window only" on, groups in
  other windows are left as they are. Subtasks are posted one at a time, in tab
  order, so their positions follow the group. A completed group task is not
  returned by the server's list, so re-adding that group makes a new task.
- **Subtask titles are `[label](https://www.youtube.com/watch?v=<id>)`.** The
  Tasks app's link pattern has no escaped brackets, so `[ ]` in the label
  become `( )`; the label is cut to 255 characters (the server's subtask limit).
- **Each video subtask gets `duration_minutes`** (its length, rounded, 1–10080).
  `addGroups` calls `attachDurations` itself, so times are sent even when
  "Include video times" is off. Re-adding fills in a missing time on an
  existing subtask but never overwrites one that is set.
- **The group task's `duration_minutes` is saved, not derived.** The Tasks
  app stores a parent's time as the sum of its open subtasks' times and
  recomputes it client-side on each subtask change; the server does not. So
  after syncing a group, `addGroups` saves that total on the task itself,
  mirroring `parentMinutes` in the app's `shared.ts`: capped at 10080, `null`
  once every timed subtask is done, left alone when no subtask has a time.
- **The Tasks token sits in `chrome.storage.local`**, readable by anyone with
  the Chrome profile. It shows as "Tab Group Exporter" in the Tasks device list,
  where it can be revoked.
- **Chrome group colors** live in `GROUP_COLORS` in `archive.js`; keep it in sync
  with Chrome's palette. Unknown colors fall back to grey in the UI.

## Style conventions

- Plain, dependency-free ES modules with `export`/`import`.
- 2-space indentation; single quotes in JS; semicolons.
- Comments explain **why** (e.g. debounce rationale, data-URL rationale), not what.
  Preserve existing comments unless the behavior they describe changes.
- Popup UI uses `$ = (id) => document.getElementById(id)`; follow that pattern.
- CSS is inline in `popup.html`, driven by `:root` custom properties with a
  `prefers-color-scheme: dark` block. Reuse the existing variables (`--bg`,
  `--surface`, `--text`, `--muted`, `--line`, `--accent`, `--accent-text`).

## Making changes safely

1. Keep the no-build constraint: any new file must load directly in Chrome.
2. If you change the archive shape, handle existing stored data gracefully
   (`archive[key]?.firstSeen ?? now`-style defensive reads) — users keep their data
   across extension updates.
3. Prefer editing existing modules over adding new ones; the surface is small on
   purpose.
4. After editing, reload the extension in `chrome://extensions` and verify both
   open and closed groups still export correctly.

## Versioning and releases

- The version lives in **one place**: `version` in `manifest.json` (currently
  `1.6.0`). Chrome reads it there.
- Use **semver**, and bump it whenever behavior changes (not for docs/comment-only
  edits). Chrome only applies an update if this number increases, so a behavior
  change without a bump can be silently ignored on reload.
- Also update the version referenced in `README.md`'s badge, and in the
  "What this is" line at the top of this file, to keep them in step.
- `YT-List.md` and `YT-List.html` are **sample artifacts**, not shipped code.
  They are not referenced by `manifest.json` and must not be versioned with the
  extension or included in release packaging.
- There is no CI or release script. Distribution today is "Load unpacked" from
  this folder; a release is just a bumped `manifest.json` committed cleanly.
