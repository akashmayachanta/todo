// Puts the app's own Android code into the generated project, which lives only in
// the WSL copy (gen/android is never synced from Windows):
// - android/java/*.kt into the app's package (reminders, background sync, the plugin)
// - android/res/** into its resources (the reminder's status bar icon)
// - the receivers, the crash log's provider and the battery permission into
//   AndroidManifest.xml
// - WorkManager into app/build.gradle.kts, and R8 rules into app/proguard-tasks.pro
// It also takes out what earlier versions put in and the app no longer has
// (REMOVED below), since gen/android keeps whatever was copied into it.
// Runs before every Android build. Safe to run again.
import { copyFileSync, existsSync, mkdirSync, readdirSync, readFileSync, rmSync, statSync, writeFileSync } from 'node:fs';
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

// The in-app updater, dropped on Android: REQUEST_INSTALL_PACKAGES made Play
// Protect flag the app as harmful (see src-tauri/src/updates.rs).
const REMOVED = {
	files: ['java/dev/tasks/app/UpdateFileProvider.kt', 'res/xml/update_paths.xml'],
	permissions: ['android.permission.REQUEST_INSTALL_PACKAGES'],
	blocks: ['tasks-native: updates'],
};
for (const file of REMOVED.files) rmSync(join(MAIN, file), { force: true });

// ── Manifest ─────────────────────────────────────────────────────────────────
const manifestPath = join(MAIN, 'AndroidManifest.xml');
let manifest = readFileSync(manifestPath, 'utf8');
for (const permission of REMOVED.permissions) {
	manifest = manifest.replace(new RegExp(`\\n[ \\t]*<uses-permission android:name="${permission.replaceAll('.', '\\.')}"\\s*/>`, 'g'), '');
}
for (const marker of REMOVED.blocks) {
	// The marker comment and the <provider> after it.
	manifest = manifest.replace(new RegExp(`\\n[ \\t]*<!-- ${marker} [^>]*-->\\s*<provider[\\s\\S]*?</provider>`, 'g'), '');
}
// Battery: background sync.
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
// Crash reports to Download/Tasks (CrashLog.kt). A provider starts with the
// process; the high initOrder starts it before the others (WorkManager's), so a
// crash in one of those is caught too.
const CRASH_LOG = `
        <!-- tasks-native: crash log (scripts/android-native.mjs) -->
        <provider
            android:name=".CrashLogProvider"
            android:authorities="\${applicationId}.crashlog"
            android:initOrder="1000"
            android:exported="false" />
`;
// Replaced each run, so an older entry (without initOrder) is updated.
manifest = manifest.replace(/\n[ \t]*<!-- tasks-native: crash log [^>]*-->\s*<provider[^>]*?\/>/g, '');
manifest = manifest.replace(/(\s*<\/application>)/, `${CRASH_LOG.trimEnd()}$1`);
writeFileSync(manifestPath, manifest);

// ── R8 ───────────────────────────────────────────────────────────────────────
// The release build shrinks and optimizes the code with every *.pro file in app/.
// R8 removed the constructor WorkManager makes its database with by reflection,
// so the app crashed on every start (NoSuchMethodException
// WorkDatabase_Impl.<init>). Our own classes are kept whole, too: Rust calls
// Native and the plugin by name (JNI), and the manifest names the rest.
writeFileSync(
	join(APP, 'proguard-tasks.pro'),
	`# Written by scripts/android-native.mjs. Edit it there, not here.
-keep class * extends androidx.room.RoomDatabase { <init>(); }
-keep class androidx.work.impl.WorkDatabase_Impl { <init>(); }
-keep class dev.tasks.app.** { *; }
`,
);

// ── Gradle ───────────────────────────────────────────────────────────────────
const gradlePath = join(APP, 'build.gradle.kts');
let gradle = readFileSync(gradlePath, 'utf8');
const WORK = 'implementation("androidx.work:work-runtime-ktx:2.10.0")';
if (!gradle.includes('androidx.work:work-runtime')) {
	gradle = gradle.replace(/(dependencies\s*\{)/, `$1\n    ${WORK}`);
	writeFileSync(gradlePath, gradle);
}

console.log('Android native code, manifest entries and WorkManager are in place.');
