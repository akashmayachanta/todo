# Building and testing

This file covers three builds:

1. **The server.** The apps need one to sign in and sync.
2. **The Windows app.** Builds natively on Windows.
3. **The Android APK.** Builds inside WSL2 (Ubuntu), so Windows admin rights aren't needed.

The README explains how the app works. This file is just the steps.

**After the one-time setup**, two scripts handle each rebuild:

| Build | Run | Output |
|---|---|---|
| Try the app (dev mode) | `run-app.bat`, or `run-app.bat --server` to start the server too | Opens the app with live UI reload; nothing to install |
| Server | `build-server.bat` | `target\release\tasks-server.exe` |
| Windows exe | `build-windows.bat` (double-click, or run it from PowerShell) | `target\release\tasks-app.exe`, `target\release\bundle\nsis\*-setup.exe` |
| Android APK | In Ubuntu: `bash /mnt/c/Drive/Vault/todo/build-android.sh` | `target\apk\tasks.apk` |

`build-android.sh` copies the latest code into `~/todo`, builds there, and copies the APK back into the Windows repo.

---

## 1. Server (Windows, PowerShell)

```powershell
cd C:\Drive\Vault\todo
cargo build --release -p tasks-server
copy .env.example .env                 # edit if needed; the defaults work
.\target\release\tasks-server.exe      # listens on 0.0.0.0:8091/tasks-api
```

Check it from another terminal:

```powershell
curl http://localhost:8091/tasks-api/health
```

The phone needs HTTPS to reach the server, so run a tunnel too:

```powershell
ngrok http 8091                        # note the https://xxxx.ngrok.app address
```

---

## 2. Windows app

### Prerequisites

This machine already has these, as of 2026-11-04:

- Rust, with the `x86_64-pc-windows-msvc` host (cargo 1.99.1)
- Node.js v26.8.1 and npm 11.19.0
- Visual Studio Build Tools, in `C:\Program Files (x86)\Microsoft Visual Studio\18`
- WebView2, which ships with Windows 11

### Build

```powershell
cd C:\Drive\Vault\todo\app
npm install
npm run tauri dev        # dev window with hot reload (Vite on :1420)
npm run tauri build -- --bundles nsis   # release build + the NSIS installer
```

The build writes:

- `target\release\tasks-app.exe`
- `target\release\bundle\nsis\Tasks_0.1.0_x64-setup.exe`. It installs for the current user, with no admin rights needed.

The MSI is skipped because WiX is slow and MSI installs usually need admin. Run a plain
`npm run tauri build` if you ever want it.

### Test on Windows

- [ ] Install with the NSIS setup.exe and open **Tasks**.
- [ ] Enter the server address `http://localhost:8091`, then **Register** a user (password of 8+ characters).
- [ ] "Getting your tasks…" finishes, and the sync chip shows *synced*.
- [ ] Add a task, then edit it and complete it. The chip goes to *syncing*, then back to *synced*.
- [ ] Close the window. It hides to the tray. Tray → Open brings it back.
- [ ] **Alt+N** anywhere opens Quick add.
- [ ] Set a reminder 1–2 minutes ahead. A Windows toast appears within about 30 s of that time.
- [ ] Stop the server. The chip shows *server down*. Start it again and the chip returns to *synced*.
- [ ] Tray → **Start at login**: sign out of Windows and back in. The app starts in the tray.

The app keeps its data in `%APPDATA%\dev.tasks.app\` and its device token in Credential Manager, under `tasks-app`.

---

## 3. Android APK (WSL2 Ubuntu)

Building on Windows needs admin rights or Developer Mode, because Tauri symlinks the `.so` libraries into `jniLibs`. WSL2 has no such limit. Every step below runs inside Ubuntu, and the `sudo` prompts ask for your **Linux** password, not Windows admin.

### 3.1 One-time setup

Open Ubuntu (`wsl -d Ubuntu`) and run:

```bash
# System packages: a host C toolchain for build scripts, a JDK, and helpers
sudo apt update
sudo apt install -y build-essential unzip curl rsync openjdk-17-jdk

# Rust and the Android targets
curl https://sh.rustup.rs -sSf | sh -s -- -y
source ~/.cargo/env
rustup target add aarch64-linux-android armv7-linux-androideabi i686-linux-android x86_64-linux-android

# Node.js through nvm
curl -o- https://raw.githubusercontent.com/nvm-sh/nvm/v0.40.1/install.sh | bash
source ~/.bashrc
nvm install --lts

# Android command-line tools (Android Studio isn't needed)
# If this zip has moved, take the current "Command line tools only" Linux link
# from https://developer.android.com/studio#command-tools
mkdir -p ~/android/cmdline-tools && cd ~/android/cmdline-tools
curl -LO https://dl.google.com/android/repository/commandlinetools-linux-11076708_latest.zip
unzip commandlinetools-linux-*.zip && mv cmdline-tools latest && rm commandlinetools-linux-*.zip

# Environment variables
cat >> ~/.bashrc <<'EOF'
export ANDROID_HOME=$HOME/android
export NDK_HOME=$ANDROID_HOME/ndk/27.2.12479018
export JAVA_HOME=/usr/lib/jvm/java-17-openjdk-amd64
export PATH=$ANDROID_HOME/cmdline-tools/latest/bin:$ANDROID_HOME/platform-tools:$PATH
EOF
source ~/.bashrc

# SDK packages: download the zips by hand, because sdkmanager is slow and shows little progress.
# If a download stops, run its curl line again to resume it.
mkdir -p $ANDROID_HOME/downloads && cd $ANDROID_HOME/downloads
curl -L -C - --progress-bar -O https://dl.google.com/android/repository/android-ndk-r27c-linux.zip        # 664 MB
curl -L -C - --progress-bar -O https://dl.google.com/android/repository/platform-37.0_r02.zip             # 67 MB
curl -L -C - --progress-bar -O https://dl.google.com/android/repository/build-tools_r36_linux.zip         # 64 MB
curl -L -C - --progress-bar -O https://dl.google.com/android/repository/platform-tools_r37.0.2-linux.zip  # 10 MB

# Compare the zips with Google's checksums. Every line should end in OK.
sha1sum -c <<'EOF'
090e8083a715fdb1a3e402d0763c388abb03fb4e  android-ndk-r27c-linux.zip
ed8ebf7f8822a4de5686d427f237d2fa30ff7410  platform-37.0_r02.zip
b0b6376977657e8ad9b969bacf4093601da2c6fb  build-tools_r36_linux.zip
736a08af10515674e40d8b754fe87390d698da2b  platform-tools_r37.0.2-linux.zip
EOF

# Unzip each one to the folder the SDK expects. This prints nothing, and the NDK may take a few minutes.
install_zip() {
  rm -rf "$2" unpacked
  mkdir -p "$(dirname "$2")"
  unzip -q "$1" -d unpacked && mv unpacked/* "$2" && rmdir unpacked
}
install_zip android-ndk-r27c-linux.zip $NDK_HOME
install_zip platform-37.0_r02.zip $ANDROID_HOME/platforms/android-37.0
install_zip build-tools_r36_linux.zip $ANDROID_HOME/build-tools/36.0.0
install_zip platform-tools_r37.0.2-linux.zip $ANDROID_HOME/platform-tools
cd ~ && rm -rf $ANDROID_HOME/downloads

yes | sdkmanager --licenses
```

Check the setup:

```bash
java -version          # 17.x
sdkmanager --list_installed   # should list all four packages
ls $NDK_HOME           # should exist
node -v && cargo -V
```

If a package is missing from that list, install it with `sdkmanager` instead, for example
`sdkmanager "platforms;android-37.0" "build-tools;36.0.0"`.

The generated Android project sets these versions: `compileSdk = 37`, and its Android Gradle
Plugin (9.3.1) expects build-tools 36.0.0. They can change when the Tauri CLI is upgraded and
`android:init` is run again.

### 3.2 Copy the project into Linux

Build from a copy on the Linux filesystem, not from `/mnt/c/...`:

- **`node_modules` here holds Windows binaries,** such as `lightningcss-win32-x64-msvc`. Linux needs its own install.
- **Cargo and Gradle run very slowly across `/mnt/c`.**

```bash
rsync -a \
  --exclude node_modules --exclude target --exclude app/build \
  --exclude .svelte-kit --exclude app/src-tauri/gen/android \
  /mnt/c/Drive/Vault/todo/ ~/todo/
```

Run the same command again whenever you change code on Windows. It leaves out
`gen/android`, so the Android project generated in WSL is never overwritten.

### 3.3 Generate the Android project (once)

```bash
cd ~/todo/app
npm ci
npm run android:init     # writes src-tauri/gen/android and adds the reminder permissions
```

### 3.4 Signing key (once)

```bash
keytool -genkey -v -keystore ~/tasks-release.jks -keyalg RSA -keysize 2048 -validity 10000 -alias tasks
```

Create `~/todo/app/src-tauri/gen/android/keystore.properties`. It's git-ignored.

```
password=<keystore password>
keyAlias=tasks
storeFile=/home/<your-linux-user>/tasks-release.jks
```

You don't need to edit `build.gradle.kts`. `npm run android:build` runs
`app/scripts/android-signing.mjs`, which adds the signing config to that generated file.
It also replaces any signing config you added by hand.

Back up `~/tasks-release.jks` somewhere safe. Updates to an installed APK have to be signed with the same key.

### 3.5 Build the APK

```bash
cd ~/todo/app
npm run android:build
```

The APK is at:

```
~/todo/app/src-tauri/gen/android/app/build/outputs/apk/universal/release/app-universal-release.apk
```

From Windows, the same file is at
`\\wsl$\Ubuntu\home\<your-linux-user>\todo\app\src-tauri\gen\android\app\build\outputs\apk\universal\release\app-universal-release.apk`.

### 3.6 Install it on the phone

WSL2 can't see USB devices without `usbipd-win`, and installing that needs admin. Use one of these instead.

**A: adb from Windows (simplest).** Download *SDK Platform-Tools for Windows*
from https://developer.android.com/tools/releases/platform-tools and unzip it,
for example to `C:\Users\akash.achanta\platform-tools`. No admin is needed.
Turn on USB debugging on the phone and plug it in. Then:

```powershell
cd C:\Users\akash.achanta\platform-tools
.\adb devices
.\adb install -r "\\wsl$\Ubuntu\home\<your-linux-user>\todo\app\src-tauri\gen\android\app\build\outputs\apk\universal\release\app-universal-release.apk"
```

**B: copy the file.** Copy the APK to the phone and open it there. This needs
*Install unknown apps* allowed for your file manager.

**C: wireless debugging from WSL (Android 11+).** On the phone, open Developer
options → Wireless debugging → *Pair device with pairing code*. Then, in WSL:

```bash
adb pair <phone-ip>:<pair-port>      # enter the code
adb connect <phone-ip>:<port>
adb install -r ~/todo/app/src-tauri/gen/android/app/build/outputs/apk/universal/release/app-universal-release.apk
```

If WSL can't reach the phone, put this in `%USERPROFILE%\.wslconfig`, then run `wsl --shutdown`:

```ini
[wsl2]
networkingMode=mirrored
```

`npm run android:dev`, the live-reload mode, also needs option C: the phone has to
reach the Vite server in WSL, and Windows Firewall may still block it. If it does,
rebuild and reinstall the APK after each change.

Don't use the Android emulator under WSL; it runs poorly there. Use a real phone.

### Test on Android

- [ ] Open **Tasks** and allow notifications when asked.
- [ ] Enter the ngrok `https://…` address as the server, then **Sign in** with the user you registered on Windows.
- [ ] "Getting your tasks…" finishes, and the tasks from Windows appear.
- [ ] Add a task on the phone. It appears on Windows after the next sync (about 2 s after the edit, or click the chip).
- [ ] Turn on airplane mode and add a task. The chip shows *offline*. Turn airplane mode off and the task syncs.
- [ ] Set a reminder 2 minutes ahead and close the app. The notification still arrives. Tapping it opens the task.
- [ ] If reminders arrive late on Android 12+, go to Settings → Apps → Tasks → **Alarms & reminders** → allow.

---

## Optional: commit the generated Android project

`src-tauri/gen/android` should be committed, but never `keystore.properties` or
the `.jks`. To move it back to the Windows repo:

```bash
rsync -a --exclude build --exclude .gradle --exclude keystore.properties \
  ~/todo/app/src-tauri/gen/android/ /mnt/c/Drive/Vault/todo/app/src-tauri/gen/android/
```

## Troubleshooting

| Symptom | Fix |
|---|---|
| `NDK_HOME` / "NDK not found" | `ls ~/android/ndk` and point `NDK_HOME` at the version that is installed |
| `linker 'cc' not found` | `sudo apt install build-essential` |
| Gradle: "Unsupported class file major version" | `JAVA_HOME` isn't JDK 17. Check `java -version` and `echo $JAVA_HOME` |
| `npm ci` fails on lightningcss / rolldown | `node_modules` was copied from Windows. `rm -rf node_modules && npm ci` |
| APK won't install over an older one | It was signed with a different key. Uninstall the old app first |
| `adb devices` is empty in WSL | Expected for USB. Use option A or C above |
