# Tasks

A tasks app with its own sync server: a Rust server, a Windows app and an
Android APK. Each device keeps its own copy of your tasks and works offline;
the server keeps them in step across devices.

```
.
├── core/                tasks-core: the rules, the sync merge, and (feature
│   │                    "client") the device side of sync over HTTP
│   └── migrations/      the tasks DB's numbered SQL migrations
├── server/              tasks-server: accounts, the REST API, /sync for devices
│   └── migrations/      accounts.db's numbered SQL migrations
└── app/                 Tauri 2 app (Windows .exe/.msi, Android .apk)
    ├── src/             SvelteKit: the Tasks UI (lib/, routes/tasks) and the shell
    │                    (app/: setup, first sync, sync chip, Tauri TasksClient)
    ├── icons-src/       the icon's source; `npm run icons` makes every size from it
    └── src-tauri/       Rust: local tasks.db, commands, sync loop, reminders, tray
```

## How it fits together

- **Server.** It keeps `accounts.db` (users and device tokens) and one SQLite DB
  per user, `user_<id>.db`, in `TASKS_DATA_DIR`. It listens on `TASKS_PORT`
  (8091) under `TASKS_API_BASE_PATH` (`/tasks-api`). Put it behind ngrok, or any
  HTTPS proxy, to reach it from your phone.
- **Apps.** Each device has its own `tasks.db`. The UI calls Tauri commands that
  run the same `tasks-core` rules locally. A background loop syncs with the
  server: it pushes the local changes, then pulls everything since the last seq
  it saw.

### Accounts

- **Register** in the app: server address, a username and a password (8+
  characters). That makes the account and signs this device in.
- **Sign in** on each other device with the same username and password.
- Each device gets its own long-lived token, kept in the Windows Credential
  Manager, or the app's private storage on Android. Passwords are stored as
  Argon2id hashes; tokens as SHA-256 hashes.
- `TASKS_ALLOW_SIGNUP=false` closes registration once your accounts exist.
- On the server, `tasks-server user add|passwd|disable|enable|list` manages
  accounts. A disabled user can't sign in, and all their devices are refused.
- `GET /auth/devices` lists a user's devices, and
  `POST /auth/devices/<id>/revoke` signs one out.

### Online the first time, offline after that

- **First sign-in needs the internet.** After signing in or registering, the app
  shows "Getting your tasks…" until the first full sync has brought the
  account's tasks to the device. If that can't happen it says why, keeps
  retrying, and offers Try again.
- **After that it works offline.** Every edit goes to the local DB first and is
  marked to sync. Nothing waits on the network.
- **Two kinds of "can't sync".** When a sync fails, the app checks whether the
  device can reach the internet at all (a quick connection to 1.1.1.1, 8.8.8.8
  or 9.9.9.9):
  - **Offline**: this device has no internet. It retries every 30 s.
  - **Server down**: the internet works, but the server doesn't answer, or
    ngrok or a proxy answers for it with an error (`ngrok-error-code`, 502, 503
    or 504). It retries every minute.
- **It syncs as soon as it can.** That's when the network comes back, when the
  app returns to the front (Android resumes it), and about 2 s after an edit.
  Otherwise it syncs every 2 minutes. The chip in the app bar shows synced,
  syncing, offline, server down, error or signed out, plus how many changes are
  waiting. Click it to sync now.
- **Either side can drop mid-sync.** A push is safe to repeat: a row the server
  already has is ignored. A pull resumes from the last page that was saved. So
  a sync cut short by either side just finishes next time.

A server reachable only on your LAN, with no internet, shows as "offline" when
it's down. The check can only tell the two apart when the internet works.

### Sync

Every row has these columns:

| Column | What it is |
|---|---|
| `id` | A UUIDv7 made on the device that created the row |
| `updated_at` | The time of the latest version, in ms |
| `updated_by` | The device id that made that version |
| `deleted_at` | Set on a tombstone |
| `server_seq` | The server's change counter when it accepted the version |
| `dirty` | Device only: the change hasn't been pushed yet |

- **Merge rule.** Last write wins per row, by `(updated_at, updated_by)`.
  A delete is a version too. A project's delete writes tombstones for its
  categories and tasks, so it reaches every device.
- **Repairs.** After every merge, `sync::repair` fixes rows that edits on
  different devices left inconsistent. A task added offline to a project that
  was deleted elsewhere gets deleted too. A task in a category that another
  device moved to a different project follows its category.
- **Push.** `POST /sync/push {device_id, rows}` returns `{accepted, ignored, seq}`.
- **Pull.** `GET /sync/pull?since=<seq>&limit=` returns `{rows, next, more, epoch, seq}`.
- **A replaced or restored server DB.** `epoch` names the server DB, and `seq`
  is the server's counter. If a device sees a new epoch, or a counter below its
  own last seq (the DB was copied back from an older backup), it pulls from 0
  and pushes every local row again. That way nothing it had is lost.

### Migrations

Both databases are migrated the same way, like Alembic: numbered SQL files run
in order, and `PRAGMA user_version` records how many a DB has had. They run when
the DB is opened.

- The tasks DB (the server's `user_<id>.db` and each device's `tasks.db`):
  `core/migrations/`, listed in `core/src/db.rs`.
- The server's `accounts.db`: `server/migrations/`, listed in `server/src/auth.rs`.

To change a schema, add the next file (`0002_<what>.sql`) and add it to the
list. Never edit or reorder a migration that has shipped.

## Server

Building needs Rust (stable, MSVC target) and the Visual Studio C++ Build
Tools. A portable MSVC install works too, once its `setup_x64.bat` environment
is loaded.

```powershell
cargo build --release -p tasks-server
copy .env.example .env                        # then edit it
.\target\release\tasks-server.exe             # serve (the default command)
.\target\release\tasks-server.exe help
```

Settings come from the environment, or a `.env` in the working folder or next
to the exe. `TASKS_ENV_FILE` names a different file. See `.env.example`:

| Key | Default | Use |
|---|---|---|
| `TASKS_HOST` | `0.0.0.0` | Address |
| `TASKS_PORT` | 8091 | Port |
| `TASKS_API_BASE_PATH` | `/tasks-api` | Route prefix; empty for the root |
| `TASKS_DATA_DIR` | `data` | accounts.db and the user DBs |
| `TASKS_ALLOW_SIGNUP` | `true` | Whether the app's Register works |

`RUST_LOG=debug` gives more logging.

To reach it from a phone, expose it over HTTPS, e.g. `ngrok http 8091`, and
use the ngrok address as the server address in the app.

### Moving existing tasks DBs in

A tasks DB from the old setup (`user_<n>.db`) works as is. Copy it into
`TASKS_DATA_DIR`, then make its owner with that id:
`tasks-server user add <name> --id <n>`. Delete each app's `tasks.db` first if
it synced with a different server, so it doesn't push old rows back.

### API

Every route is under `TASKS_API_BASE_PATH`. Errors come back as
`{"detail": "..."}`.

- `POST /auth/register`, `POST /auth/login`: `{username, password, device_name}`
  returns `{token, device, user_id, username}`
- `GET /auth/me`, `GET /auth/devices`, `POST /auth/devices/{id}/revoke`
- `GET /?completed=&project_id=&inbox=&limit=`, plus `q=` to search
- `POST /`, `GET`/`POST /{id}`, `POST /{id}/delete`
- `/projects`, `/projects/reorder`, `/projects/{id}`, `/projects/{id}/delete`
- `/categories`, `/filters`, with the same `{id}` and `{id}/delete` forms
- `GET /completions?since=`
- `/sync/push`, `/sync/pull`, `/health`

```bash
B=http://localhost:8091/tasks-api
T=$(curl -s -X POST $B/auth/register -H "Content-Type: application/json" \
     -d '{"username":"me","password":"a long password","device_name":"curl"}' | jq -r .token)
curl -H "Authorization: Bearer $T" $B/projects
curl -H "Authorization: Bearer $T" -H "Content-Type: application/json" \
     -X POST $B/ -d '{"title":"Try the API","due_date":"2026-11-05"}'
```

## Windows app

You need Node.js and Rust with the MSVC Build Tools. WebView2 ships with Windows 11.

```powershell
cd app
npm install
npm run tauri dev                 # dev window with hot reload (Vite on :1420)
npm run tauri build               # release: target\release\tasks-app.exe and the installers
```

The installers land in `target/release/bundle/` (`nsis/*.exe`, `msi/*.msi`).
The NSIS installer installs for the current user, without admin rights.

On first run, the app asks for the server address (e.g. `https://xyz.ngrok.app`),
then sign in or register. **Path…** changes `/tasks-api` if the server uses
another prefix. **Account** in the app bar signs out, or in as someone else.

On Windows the app also has:

- **A tray icon.** Its menu has Open, Quick add, Sync now, Start at login and
  Quit. Closing the window hides it to the tray.
- **Alt+N anywhere.** It brings the app up with the quick add open.
- **Start at login** (`tauri-plugin-autostart`). It starts with `--minimized`,
  straight to the tray.
- **Reminders.** A Rust loop checks `remind_at` every 30 s and shows a Windows
  toast. Each reminder fires once. One missed while the app was closed still
  shows if it is under a day old.

The app id is `dev.tasks.app`. Its data folder is `%APPDATA%\dev.tasks.app\`,
which holds `tasks.db` and `config.json`. The device token is in the
Credential Manager, under `tasks-app`.

## Android APK

The same project builds for Android with `tauri android`. You need:

- **Android Studio**, with the SDK, NDK and command-line tools. Set
  `ANDROID_HOME` and `NDK_HOME`.
- **A JDK 17+**: Android Studio's bundled one, via `JAVA_HOME`.
- **The Rust targets:**
  `rustup target add aarch64-linux-android armv7-linux-androideabi i686-linux-android x86_64-linux-android`.

```powershell
cd app
npm run android:init      # tauri android init, then adds the reminder permissions to the manifest
npm run android:dev       # on a USB-connected phone or an emulator
npm run android:build     # tauri android build --apk
```

`android:init` writes `src-tauri/gen/android`. Commit that folder, but never the
keystore. `scripts/android-permissions.mjs` adds `POST_NOTIFICATIONS`,
`SCHEDULE_EXACT_ALARM`, `USE_EXACT_ALARM` and `RECEIVE_BOOT_COMPLETED`.

**Reminders on Android are scheduled notifications.** After every sync or edit,
the app cancels its scheduled notifications. It then schedules one for each open
task with a future `remind_at`: `Schedule::At` with `allow_while_idle`, through
AlarmManager.

- They fire while the app is closed, and while the phone is offline.
- After a reboot they come back the next time the app is opened. The plugin
  only restores alarms it scheduled in a batch, and its Rust API has no batch.
- Tapping one opens the task.
- The app asks for notification permission on first start. On Android 12+, if
  reminders come late, allow **Alarms & reminders** for Tasks in the system
  settings.
- A reminder added on the laptop reaches the phone only after the phone next
  syncs. That happens when the app is opened or resumed, after an edit, when
  the network comes back, or every 2 minutes while it is open. An FCM "sync now" nudge from the server
  would fix this. It is left out until it's needed.

### Signing a release APK

1. Make a keystore once, and keep it out of git (`*.jks` is ignored):
   ```powershell
   keytool -genkey -v -keystore $HOME\tasks-release.jks -keyalg RSA -keysize 2048 -validity 10000 -alias tasks
   ```
2. Create `src-tauri/gen/android/keystore.properties`. It is git-ignored.
   ```
   password=<keystore password>
   keyAlias=tasks
   storeFile=C:\\Users\\<you>\\tasks-release.jks
   ```
3. In `src-tauri/gen/android/app/build.gradle.kts`, add a release signing config
   that reads it:
   ```kotlin
   import java.io.FileInputStream
   import java.util.Properties
   // inside android { … }
   signingConfigs {
       create("release") {
           val props = Properties().apply { load(FileInputStream(rootProject.file("keystore.properties"))) }
           keyAlias = props["keyAlias"] as String
           keyPassword = props["password"] as String
           storeFile = file(props["storeFile"] as String)
           storePassword = props["password"] as String
       }
   }
   buildTypes { getByName("release") { signingConfig = signingConfigs.getByName("release") } }
   ```
4. Build the APK and sideload it:
   ```powershell
   npm run android:build
   adb install -r src-tauri\gen\android\app\build\outputs\apk\universal\release\app-universal-release.apk
   ```
   You can also copy the APK to the phone and open it. That needs "install
   unknown apps" allowed for your file manager.

## Checking it

- `cargo test --workspace`: the migrations and accounts (sign-up, sign-in,
  disabling, revoking).
- `cargo clippy --workspace --all-targets`, and `npm run check` in `app/`.
