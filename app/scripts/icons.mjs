// `npm run icons`: makes every icon size. Windows from icons-src/icon.svg (green);
// The web favicon is copied from the Windows one into static/.
// Android from icons-src/icon-android.json (also green, with a smaller check so it
// fits the adaptive icon's mask) into src-tauri/icons/android.
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
// The web copy's favicon (served from static/).
cpSync(join(app, 'icons-src/icon.svg'), join(app, 'static/favicon.svg'));
cpSync(join(app, 'src-tauri/icons/32x32.png'), join(app, 'static/favicon.png'));

const temp = mkdtempSync(join(tmpdir(), 'tasks-icons-'));
try {
	// The web copy's install icons (static/manifest.webmanifest): the rounded one as is,
	// and a full-bleed one with a smaller check for masks and the iPhone home screen.
	const pwa = join(app, 'static/pwa');
	rmSync(pwa, { recursive: true, force: true });
	icon('icons-src/icon.svg', '-o', join(temp, 'any'), '-p', '192,512');
	icon('icons-src/icon-maskable.svg', '-o', join(temp, 'maskable'), '-p', '180,512');
	cpSync(join(temp, 'any/192x192.png'), join(pwa, 'icon-192.png'));
	cpSync(join(temp, 'any/512x512.png'), join(pwa, 'icon-512.png'));
	cpSync(join(temp, 'maskable/512x512.png'), join(pwa, 'maskable-512.png'));
	cpSync(join(temp, 'maskable/180x180.png'), join(pwa, 'apple-touch-icon.png'));


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
