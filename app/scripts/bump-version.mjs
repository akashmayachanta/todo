// Raises the app's version by one patch (0.1.4 -> 0.1.5) in tauri.conf.json and
// prints the new one. build-windows.bat and build-android.sh run it before every
// build, so each build the server hands out has a version of its own: the app
// offers an update when the server's is higher (src-tauri/src/updates.rs).
// Android's versionCode comes from it too, and must keep rising for an update to
// install. Only tauri.conf.json changes; Tauri takes the app's version from there.
import { readFileSync, writeFileSync } from 'node:fs';
import { fileURLToPath } from 'node:url';

const path = fileURLToPath(new URL('../src-tauri/tauri.conf.json', import.meta.url));
const text = readFileSync(path, 'utf8');
const VERSION = /^(\s*"version":\s*")(\d+)\.(\d+)\.(\d+)(")/m;
const match = text.match(VERSION);
if (!match) {
	console.error(`No "version": "x.y.z" in ${path}`);
	process.exit(1);
}
const [, , major, minor, patch] = match;
// Android's versionCode is major * 1000000 + minor * 1000 + patch, so patch stops at 999.
if (Number(patch) >= 999) {
	console.error(`Version ${major}.${minor}.${patch}: raise the minor version in ${path} by hand.`);
	process.exit(1);
}
const version = `${major}.${minor}.${Number(patch) + 1}`;
writeFileSync(path, text.replace(VERSION, `$1${version}$5`));
console.log(version);
