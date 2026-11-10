/**
 * The tasks quick add learns from ($lib/suggestPlace): the open ones and the latest
 * done ones. Loaded once and shared by every quick add in the window, and loaded
 * again after a minute or when the tasks change (a sync, another window, an add).
 */
import type { Task } from '$lib/api';
import { api } from '$lib/api';
import { onTasksChanged } from '$lib/tasks/client';

const FRESH_MS = 60_000;
const DONE_LIMIT = 1000;

let cached: { at: number; tasks: Promise<Task[]> } | null = null;

export function placeExamples(): Promise<Task[]> {
	if (!cached || Date.now() - cached.at > FRESH_MS) {
		const tasks = Promise.all([api.tasks.list(), api.tasks.list({ completed: true, limit: DONE_LIMIT })])
			.then(([open, done]) => [...open, ...done]);
		const entry = { at: Date.now(), tasks };
		cached = entry;
		tasks.catch(() => {
			if (cached === entry) cached = null;
		});
	}
	return cached.tasks;
}

export function forgetPlaceExamples() {
	cached = null;
}

onTasksChanged(forgetPlaceExamples);
