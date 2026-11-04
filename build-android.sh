#!/usr/bin/env bash
# Builds the signed release APK inside WSL2 Ubuntu.
# Run from Ubuntu:  bash /mnt/c/Drive/Vault/todo/build-android.sh
# One-time setup (SDK, NDK, JDK, keystore, android:init): see BUILD.md, section 3.
set -euo pipefail

SRC="$(cd "$(dirname "$0")" && pwd)"   # the Windows repo, seen from WSL
DEST="${DEST:-$HOME/todo}"             # the Linux copy that is built

# A script doesn't read ~/.bashrc, so load the toolchain here.
source "$HOME/.cargo/env"
export NVM_DIR="$HOME/.nvm"; source "$NVM_DIR/nvm.sh"
export ANDROID_HOME="${ANDROID_HOME:-$HOME/android}"
export NDK_HOME="${NDK_HOME:-$(ls -d "$ANDROID_HOME"/ndk/* | sort -V | tail -n 1)}"
export JAVA_HOME="${JAVA_HOME:-/usr/lib/jvm/java-17-openjdk-amd64}"
export PATH="$ANDROID_HOME/platform-tools:$PATH"

# Copy only code into Linux. Excluded paths are never copied or deleted, and
# there is no --delete, so the Linux node_modules, build output and Android
# project survive every run. Later runs copy only files changed since the last.
rsync -a \
  --exclude node_modules --exclude target --exclude app/build \
  --exclude .svelte-kit --exclude app/src-tauri/gen/android \
  "$SRC/" "$DEST/"

cd "$DEST/app"
npm install
npm run android:build

# Put the APK where Windows can reach it (target/ is git-ignored).
APK="$DEST/app/src-tauri/gen/android/app/build/outputs/apk/universal/release/app-universal-release.apk"
mkdir -p "$SRC/target/apk"
cp "$APK" "$SRC/target/apk/tasks.apk"

echo
echo "Done: $SRC/target/apk/tasks.apk"
echo "Install from Windows:  adb install -r $(wslpath -w "$SRC/target/apk/tasks.apk")"
