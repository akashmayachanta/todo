// Picks the version for a build and prints it. build-windows.bat and
// build-android.sh run it before every build, each passing the file where its own
// platform's last successful build wrote its version:
//   node scripts/bump-version.mjs <path to tasks-app.exe.version or tasks.apk.version>
//
// The version in tauri.conf.json is raised by one patch (0.1.4 -> 0.1.5) only when
// that platform's last build already used it. So the Windows and Android builds of
// one round share a version, whichever runs first and even when they run at once,
// while a platform built again gets a higher one. The app offers an update when the
// server's version is higher (src-tauri/src/updates.rs), and Android's versionCode
// comes from it too, so it must keep rising for an update to install. A build that
// fails writes no version file, so its retry keeps the same number.
// Only tauri.conf.json changes; Tauri takes the app's version from there.
import { existsSync, readFileSync, writeFileSync } from 'node:fs';
import { fileURLToPath } from 'node:url';

const lastFile = process.argv[2];
if (!lastFile) {
	console.error('usage: node scripts/bump-version.mjs <last build .version file>');
	process.exit(1);
}

const path = fileURLToPath(new URL('../src-tauri/tauri.conf.json', import.meta.url));
const text = readFileSync(path, 'utf8');
const VERSION = /^(\s*"version":\s*")(\d+)\.(\d+)\.(\d+)(")/m;
const match = text.match(VERSION);
if (!match) {
	console.error(`No "version": "x.y.z" in ${path}`);
	process.exit(1);
}
const [, , major, minor, patch] = match;
const current = `${major}.${minor}.${patch}`;

// Android's versionCode is major * 1000000 + minor * 1000 + patch, so patch stops at 999.
const code = (v) => {
	const m = /^(\d+)\.(\d+)\.(\d+)$/.exec(v.trim());
	return m ? Number(m[1]) * 1_000_000 + Number(m[2]) * 1_000 + Number(m[3]) : 0;
};
const last = existsSync(lastFile) ? readFileSync(lastFile, 'utf8') : '';

if (code(last) < code(current)) {
	// This platform hasn't shipped the current version yet: build it as it is.
	console.log(current);
} else {
	if (Number(patch) >= 999) {
		console.error(`Version ${current}: raise the minor version in ${path} by hand.`);
		process.exit(1);
	}
	const version = `${major}.${minor}.${Number(patch) + 1}`;
	writeFileSync(path, text.replace(VERSION, `$1${version}$5`));
	console.log(version);
}
