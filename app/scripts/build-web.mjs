// The browser copy of the UI that tasks-server serves at /tasks-app (server/src/routes.rs):
// the same build with base path /tasks-app, into build-web/.
import { spawnSync } from 'node:child_process';

const result = spawnSync('npx', ['vite', 'build'], {
	stdio: 'inherit',
	shell: true,
	env: { ...process.env, TASKS_WEB_BASE: process.env.TASKS_WEB_BASE || '/tasks-app' },
});
process.exit(result.status ?? 1);
