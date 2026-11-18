// The browser copy of the UI that tasks-server serves at /tasks-app (server/src/routes.rs):
// the same build with base path /tasks-app, into build-web/. Then mirrored to
// target/release/web, beside tasks-server.exe, where copy-release.bat takes it from.
import { spawnSync } from 'node:child_process';
import { cpSync, rmSync } from 'node:fs';
import { fileURLToPath } from 'node:url';

const result = spawnSync('npx', ['vite', 'build'], {
	stdio: 'inherit',
	shell: true,
	env: { ...process.env, TASKS_WEB_BASE: process.env.TASKS_WEB_BASE || '/tasks-app' },
});
if (result.status !== 0) process.exit(result.status ?? 1);

const from = fileURLToPath(new URL('../build-web', import.meta.url));
const to = fileURLToPath(new URL('../../target/release/web', import.meta.url));
rmSync(to, { recursive: true, force: true });
cpSync(from, to, { recursive: true });
console.log(`Web UI: ${to}`);
