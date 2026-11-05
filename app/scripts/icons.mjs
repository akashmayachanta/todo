// `npm run icons`: makes every icon size. Windows from icons-src/icon.svg (green);
// Android from icons-src/icon-android.json (black) into src-tauri/icons/android.
// `tauri icon` writes all platforms at once, so Android's go to a temp folder and
// only that part is kept. Then copies them into the Android project, if there is one.
import { execFileSync } from 'node:child_process';
import { cpSync, mkdtempSync, rmSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import { fileURLToPath } from 'node:url';

const app = fileURLToPath(new URL('..', import.meta.url));
const icon = (...args) => execFileSync(`npx tauri icon ${args.map((a) => `"${a}"`).join(' ')}`, { cwd: app, stdio: 'inherit', shell: true });

icon('icons-src/icon.svg');

const temp = mkdtempSync(join(tmpdir(), 'tasks-icons-'));
try {
	icon('icons-src/icon-android.json', '-o', temp);
	const android = join(app, 'src-tauri/icons/android');
	rmSync(android, { recursive: true, force: true });
	cpSync(join(temp, 'android'), android, { recursive: true });
} finally {
	rmSync(temp, { recursive: true, force: true });
}
// `tauri icon` always writes these too; this app doesn't build for iOS.
rmSync(join(app, 'src-tauri/icons/ios'), { recursive: true, force: true });

await import('./android-icons.mjs');
