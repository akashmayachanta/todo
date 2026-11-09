// Puts the app's own Android code into the generated project, which lives only in
// the WSL copy (gen/android is never synced from Windows):
// - android/java/*.kt into the app's package (reminders, background sync, the plugin)
// - android/res/** into its resources (the reminder's status bar icon)
// - the receivers and the battery permission into AndroidManifest.xml
// - WorkManager into app/build.gradle.kts
// Runs before every Android build. Safe to run again.
import { copyFileSync, existsSync, mkdirSync, readdirSync, readFileSync, statSync, writeFileSync } from 'node:fs';
import { join } from 'node:path';
import { fileURLToPath } from 'node:url';

const here = (p) => fileURLToPath(new URL(p, import.meta.url));
const SRC = here('../android');
const APP = here('../src-tauri/gen/android/app');
const MAIN = join(APP, 'src/main');

if (!existsSync(MAIN)) {
	console.error(`Not found: ${MAIN}\nRun \`npm run android:init\` first.`);
	process.exit(1);
}

function copyTree(from, to) {
	mkdirSync(to, { recursive: true });
	for (const name of readdirSync(from)) {
		const a = join(from, name);
		const b = join(to, name);
		if (statSync(a).isDirectory()) copyTree(a, b);
		else copyFileSync(a, b);
	}
}

copyTree(join(SRC, 'java'), join(MAIN, 'java/dev/tasks/app'));
copyTree(join(SRC, 'res'), join(MAIN, 'res'));

// ── Manifest ─────────────────────────────────────────────────────────────────
const manifestPath = join(MAIN, 'AndroidManifest.xml');
let manifest = readFileSync(manifestPath, 'utf8');
const PERMISSION = 'android.permission.REQUEST_IGNORE_BATTERY_OPTIMIZATIONS';
if (!manifest.includes(`"${PERMISSION}"`)) {
	manifest = manifest.replace(/(<manifest[^>]*>)/, `$1\n    <uses-permission android:name="${PERMISSION}" />`);
}
const RECEIVERS = `
        <!-- tasks-native: reminders (scripts/android-native.mjs) -->
        <receiver android:name=".ReminderReceiver" android:exported="false" />
        <receiver android:name=".BootReceiver" android:exported="false">
            <intent-filter>
                <action android:name="android.intent.action.BOOT_COMPLETED" />
                <action android:name="android.intent.action.MY_PACKAGE_REPLACED" />
                <action android:name="android.app.action.SCHEDULE_EXACT_ALARM_PERMISSION_STATE_CHANGED" />
            </intent-filter>
        </receiver>
`;
if (!manifest.includes('tasks-native: reminders')) {
	manifest = manifest.replace(/(\s*<\/application>)/, `${RECEIVERS.trimEnd()}$1`);
}
writeFileSync(manifestPath, manifest);

// ── Gradle ───────────────────────────────────────────────────────────────────
const gradlePath = join(APP, 'build.gradle.kts');
let gradle = readFileSync(gradlePath, 'utf8');
const WORK = 'implementation("androidx.work:work-runtime-ktx:2.10.0")';
if (!gradle.includes('androidx.work:work-runtime')) {
	gradle = gradle.replace(/(dependencies\s*\{)/, `$1\n    ${WORK}`);
	writeFileSync(gradlePath, gradle);
}

console.log('Android native code, manifest entries and WorkManager are in place.');
