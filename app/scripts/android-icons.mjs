// Before an Android build: copies src-tauri/icons/android into the generated Android
// project. That project isn't synced from the Windows repo (see build-android.sh) and
// `tauri android init` writes its own default icons, so this is what puts ours in the
// APK. Does nothing before `android:init`. Safe to run again.
import { cpSync, existsSync } from 'node:fs';
import { fileURLToPath } from 'node:url';

const icons = fileURLToPath(new URL('../src-tauri/icons/android', import.meta.url));
const res = fileURLToPath(new URL('../src-tauri/gen/android/app/src/main/res', import.meta.url));

if (existsSync(res)) {
	cpSync(icons, res, { recursive: true });
	console.log('Copied the app icons into the Android project.');
}
