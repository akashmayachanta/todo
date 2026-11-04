// After `tauri android init`: adds the permissions reminders need to the
// generated AndroidManifest.xml. POST_NOTIFICATIONS comes with the notification
// plugin; exact alarms let a reminder fire on time while the phone dozes.
// Safe to run again.
import { readFileSync, writeFileSync, existsSync } from 'node:fs';
import { fileURLToPath } from 'node:url';

const manifest = fileURLToPath(new URL('../src-tauri/gen/android/app/src/main/AndroidManifest.xml', import.meta.url));
if (!existsSync(manifest)) {
	console.error(`Not found: ${manifest}\nRun \`npm run tauri android init\` first.`);
	process.exit(1);
}

const PERMISSIONS = [
	'android.permission.POST_NOTIFICATIONS',
	'android.permission.SCHEDULE_EXACT_ALARM',
	'android.permission.USE_EXACT_ALARM',
	'android.permission.RECEIVE_BOOT_COMPLETED',
];

let xml = readFileSync(manifest, 'utf8');
const missing = PERMISSIONS.filter((p) => !xml.includes(`"${p}"`));
if (missing.length) {
	const lines = missing.map((p) => `    <uses-permission android:name="${p}" />`).join('\n');
	xml = xml.replace(/(<manifest[^>]*>)/, `$1\n${lines}`);
	writeFileSync(manifest, xml);
	console.log(`Added to AndroidManifest.xml: ${missing.join(', ')}`);
} else {
	console.log('AndroidManifest.xml already has the reminder permissions.');
}
