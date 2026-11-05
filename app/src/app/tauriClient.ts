// The app's TasksClient: every call is a Tauri command on the local tasks DB
// (src-tauri/src/commands.rs), which runs the same rules as tasks-server and
// syncs in the background. Errors arrive as "<status>: {"detail": ...}", like
// the server's, so errorDetail() reads both the same way.

import { invoke } from '@tauri-apps/api/core';
import { emit } from '@tauri-apps/api/event';
import { getCurrentWindow } from '@tauri-apps/api/window';
import type { TasksClient } from '$lib/tasks/client';

export interface ServerInfo {
	url: string;
	/** The server's TASKS_API_BASE_PATH; empty for the root. */
	tasks_base: string;
}

export interface SyncStatus {
	/** offline: this device has no internet. server_down: the internet works, the server doesn't answer. */
	state: 'setup' | 'syncing' | 'synced' | 'offline' | 'server_down' | 'error' | 'signed_out';
	message: string | null;
	last_synced_at: string | null;
	pending: number;
	/** The first full sync for this account has finished; until then the tasks aren't on this device. */
	ready: boolean;
}

export interface AppStatus {
	configured: boolean;
	server: ServerInfo | null;
	username: string | null;
	sync: SyncStatus;
	platform: string;
}

export async function call<T>(cmd: string, args?: Record<string, unknown>): Promise<T> {
	try {
		return await invoke<T>(cmd, args);
	} catch (e) {
		throw new Error(typeof e === 'string' ? e : JSON.stringify(e));
	}
}

/** Told to the other windows when this one writes, with this window's label. */
export const EVENT_LOCAL_CHANGED = 'tasks://local-changed';

/** A write. The pins and the main window each keep their own copy of the tasks, and
 *  `tasks://changed` only fires after a sync, so a write tells the others itself. */
async function write<T>(cmd: string, args?: Record<string, unknown>): Promise<T> {
	const out = await call<T>(cmd, args);
	emit(EVENT_LOCAL_CHANGED, getCurrentWindow().label).catch(() => { /* the others catch up on the next sync */ });
	return out;
}

const done = () => undefined;

export const tauriTasksClient: TasksClient = {
	projects: {
		list: () => call('tasks_projects_list'),
		create: (data) => write('tasks_projects_create', { data }),
		update: (id, data) => write('tasks_projects_update', { id, data }),
		delete: (id) => write('tasks_projects_delete', { id }).then(done),
		reorder: (ids) => write('tasks_projects_reorder', { ids }),
	},
	categories: {
		list: (projectId) => call('tasks_categories_list', { project: projectId ?? null }),
		create: (data) => write('tasks_categories_create', { data }),
		update: (id, data) => write('tasks_categories_update', { id, data }),
		delete: (id) => write('tasks_categories_delete', { id }).then(done),
	},
	filters: {
		list: () => call('tasks_filters_list'),
		create: (data) => write('tasks_filters_create', { data }),
		update: (id, data) => write('tasks_filters_update', { id, data }),
		delete: (id) => write('tasks_filters_delete', { id }).then(done),
	},
	list: (opts = {}) =>
		call('tasks_list', {
			opts: {
				completed: !!opts.completed,
				project_id: opts.project_id ?? null,
				inbox: !!opts.inbox,
				limit: opts.limit ?? null,
				q: opts.q ?? null,
			},
		}),
	get: (id) => call('tasks_get', { id }),
	completions: (since) => call('tasks_completions', { since }),
	create: (data) => write('tasks_create', { data }),
	update: (id, patch) => write('tasks_update', { id, patch }),
	delete: (id) => write('tasks_delete', { id }).then(done),
	reorder: (ids) => write('tasks_reorder', { ids }),
};
