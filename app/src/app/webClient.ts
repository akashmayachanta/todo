// The TasksClient for a plain browser, on tasks-server's /tasks-app page: every call
// goes straight to the server's REST API (server/src/routes.rs) with this browser's
// device token. Online only: no local copy, reminders, pins or updates.
// Errors read "<status>: <body>", like tauriClient's, so errorDetail() reads both.

import { writable, get } from 'svelte/store';
import type { TasksClient } from '$lib/tasks/client';
import { tasksChanged } from '$lib/tasks/client';
import { ymd } from '$lib/taskParse';

/** The server's TASKS_API_BASE_PATH. The page is served by the same server, so same origin. */
export const API_BASE: string = (import.meta.env.VITE_TASKS_API_BASE as string | undefined) || '/tasks-api';

const TOKEN_KEY = 'tasks.webToken';
const USER_KEY = 'tasks.webUser';

export interface WebSession {
	token: string;
	username: string;
}

function load(): WebSession | null {
	try {
		const token = localStorage.getItem(TOKEN_KEY);
		const username = localStorage.getItem(USER_KEY);
		return token ? { token, username: username ?? '' } : null;
	} catch {
		return null;
	}
}

function save(s: WebSession | null) {
	try {
		if (s) {
			localStorage.setItem(TOKEN_KEY, s.token);
			localStorage.setItem(USER_KEY, s.username);
		} else {
			localStorage.removeItem(TOKEN_KEY);
			localStorage.removeItem(USER_KEY);
		}
	} catch {
		/* private window: signed in for this page only */
	}
}

/** Who is signed in in this browser; null shows the sign-in page. */
export const webSession = writable<WebSession | null>(typeof window === 'undefined' ? null : load());
webSession.subscribe(save);

async function failure(res: Response): Promise<Error> {
	const body = await res.text().catch(() => '');
	return new Error(`${res.status}: ${body || res.statusText}`);
}

async function request<T>(method: 'GET' | 'POST', path: string, body?: unknown): Promise<T> {
	const session = get(webSession);
	const headers: Record<string, string> = {};
	if (session) headers.Authorization = `Bearer ${session.token}`;
	if (body !== undefined) headers['Content-Type'] = 'application/json';
	let res: Response;
	try {
		res = await fetch(`${API_BASE}${path}`, { method, headers, body: body === undefined ? undefined : JSON.stringify(body) });
	} catch {
		throw new Error("0: Can't reach the server");
	}
	if (res.status === 401 && session) webSession.set(null);
	if (!res.ok) throw await failure(res);
	return (await res.json()) as T;
}

const getJson = <T>(path: string) => request<T>('GET', path);
const post = <T>(path: string, body: unknown = {}) => request<T>('POST', path, body);
const enc = encodeURIComponent;
const done = () => undefined;

/** Signs this browser in as a device called "Browser". */
export async function webLogin(username: string, password: string): Promise<void> {
	const out = await request<{ token: string; username: string }>('POST', '/auth/login', {
		username,
		password,
		device_name: 'Browser',
	});
	webSession.set({ token: out.token, username: out.username });
}

/** Forgets the token here. The device stays in the account's list until revoked from an app. */
export function webSignOut() {
	webSession.set(null);
}

export const webTasksClient: TasksClient = {
	projects: {
		list: () => getJson('/projects'),
		create: (data) => post('/projects', data),
		update: (id, data) => post(`/projects/${enc(id)}`, data),
		delete: (id) => post(`/projects/${enc(id)}/delete`).then(done),
		reorder: (ids) => post('/projects/reorder', { ids }),
	},
	sections: {
		list: (projectId) => getJson(projectId ? `/sections?project_id=${enc(projectId)}` : '/sections'),
		create: (data) => post('/sections', data),
		update: (id, data) => post(`/sections/${enc(id)}`, data),
		delete: (id) => post(`/sections/${enc(id)}/delete`).then(done),
		reorder: (projectId, ids) => post('/sections/reorder', { project_id: projectId, ids }),
	},
	filters: {
		list: () => getJson('/filters'),
		create: (data) => post('/filters', data),
		update: (id, data) => post(`/filters/${enc(id)}`, data),
		delete: (id) => post(`/filters/${enc(id)}/delete`).then(done),
	},
	list: (opts = {}) => {
		const q = new URLSearchParams();
		if (opts.completed) q.set('completed', 'true');
		if (opts.project_id) q.set('project_id', opts.project_id);
		if (opts.inbox) q.set('inbox', 'true');
		if (opts.limit != null) q.set('limit', String(opts.limit));
		if (opts.q) q.set('q', opts.q);
		const s = q.toString();
		return getJson(`/${s ? `?${s}` : ''}`);
	},
	get: (id) => getJson(`/${enc(id)}`),
	completions: (since) => getJson(`/completions?since=${enc(since)}`),
	create: (data) => post('/', data),
	update: (id, patch) => post(`/${enc(id)}`, patch),
	// The browser's own date: a repeating task moves to the day after it.
	complete: (id) => post(`/${enc(id)}/complete`, { today: ymd(new Date()) }),
	reopen: (id) => post(`/${enc(id)}/reopen`),
	delete: (id) => post(`/${enc(id)}/delete`).then(done),
	deleteCompleted: (ids) => post<{ deleted: number }>('/delete-completed', { ids }).then((r) => r.deleted),
	reorder: (ids) => post('/reorder', { ids }),
};

/**
 * Listens to GET /sync/events (with fetch: EventSource can't send the token) and
 * reloads the page's tasks on each `changed`. It reconnects after a drop, and
 * reloads then too, since a notice may have been missed. Returns the function that stops it.
 */
export function followServerChanges(): () => void {
	const stop = new AbortController();
	let retry: ReturnType<typeof setTimeout> | null = null;

	async function connect(first: boolean) {
		const session = get(webSession);
		if (!session || stop.signal.aborted) return;
		try {
			const res = await fetch(`${API_BASE}/sync/events`, {
				headers: { Authorization: `Bearer ${session.token}`, Accept: 'text/event-stream' },
				signal: stop.signal,
			});
			if (res.status === 401) {
				webSession.set(null);
				return;
			}
			if (!res.ok || !res.body) throw new Error(String(res.status));
			if (!first) tasksChanged();
			const reader = res.body.pipeThrough(new TextDecoderStream()).getReader();
			let buffer = '';
			for (;;) {
				const { value, done } = await reader.read();
				if (done) break;
				buffer += value.replace(/\r\n/g, '\n');
				let end: number;
				while ((end = buffer.indexOf('\n\n')) >= 0) {
					const block = buffer.slice(0, end);
					buffer = buffer.slice(end + 2);
					if (block.split('\n').some((line) => line.trim() === 'event: changed' || line.trim() === 'event:changed')) {
						tasksChanged();
					}
				}
			}
		} catch {
			/* dropped: try again below */
		}
		if (!stop.signal.aborted) retry = setTimeout(() => connect(false), 5000);
	}

	connect(true);
	return () => {
		stop.abort();
		if (retry) clearTimeout(retry);
	};
}
