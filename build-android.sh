#!/usr/bin/env bash
# Builds the signed release APK inside WSL2 Ubuntu.
# Run from Ubuntu:  bash /mnt/c/Drive/Vault/todo/build-android.sh
# One-time setup (SDK, NDK, JDK, keystore, android:init): see BUILD.md, section 3.
# Each build raises the version (app/scripts/bump-version.mjs) and writes it to
# target/apk/tasks.apk.version, which copy-release.bat puts next to the server
# for app updates.
set -euo pipefail

SRC="$(cd "$(dirname "$0")" && pwd)"   # the Windows repo, seen from WSL
DEST="${DEST:-$HOME/todo}"             # the Linux copy that is built

# Run from the Linux copy, the sync would copy it onto itself: the APK would be
# built from stale code and left in Linux, not in the Windows repo.
if [ "$(realpath -m "$SRC")" = "$(realpath -m "$DEST")" ]; then
  echo "error: this is the Linux copy ($DEST); it is built from the Windows repo." >&2
  echo "Run the Windows one instead:  bash /mnt/c/Drive/Vault/todo/build-android.sh" >&2
  exit 1
fi

# A script doesn't read ~/.bashrc, so load the toolchain here.
source "$HOME/.cargo/env"
export NVM_DIR="$HOME/.nvm"; source "$NVM_DIR/nvm.sh"
export ANDROID_HOME="${ANDROID_HOME:-$HOME/android}"
export NDK_HOME="${NDK_HOME:-$(ls -d "$ANDROID_HOME"/ndk/* | sort -V | tail -n 1)}"
export JAVA_HOME="${JAVA_HOME:-/usr/lib/jvm/java-17-openjdk-amd64}"
export PATH="$ANDROID_HOME/platform-tools:$PATH"

# In the Windows repo, so the new version is synced below and stays there.
VERSION="$(node "$SRC/app/scripts/bump-version.mjs")"

# Copy only code into Linux. Excluded paths are never copied or deleted, and
# there is no --delete, so the Linux node_modules, build output and Android
# project survive every run. Later runs copy only files changed since the last.
rsync -a \
  --exclude node_modules --exclude target --exclude app/build \
  --exclude .svelte-kit --exclude app/src-tauri/gen/android \
  "$SRC/" "$DEST/"

APK="$DEST/app/src-tauri/gen/android/app/build/outputs/apk/universal/release/app-universal-release.apk"

cd "$DEST/app"
npm install
# Delete the old APK, so the copy below can't pick it up if this build writes elsewhere.
rm -f "$APK"
npm run android:build

# Put the APK where Windows can reach it (target/ is git-ignored).
mkdir -p "$SRC/target/apk"
cp "$APK" "$SRC/target/apk/tasks.apk"
echo "$VERSION" > "$SRC/target/apk/tasks.apk.version"

echo
echo "Done: $SRC/target/apk/tasks.apk, version $VERSION"
echo "Install from Windows:  adb install -r $(wslpath -w "$SRC/target/apk/tasks.apk")"
