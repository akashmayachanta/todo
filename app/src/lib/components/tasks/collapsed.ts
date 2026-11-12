// Tasks whose subtasks are hidden under them (the subtask count's toggle),
// remembered in this browser. Per device, like the sidebar's expanded projects.

import { writable } from 'svelte/store';
import { browser } from '$app/environment';

const KEY = 'tasks-collapsed-subtasks';

function load(): string[] {
	try {
		const list = browser ? JSON.parse(localStorage.getItem(KEY) ?? '[]') : [];
		return Array.isArray(list) ? list.filter((x) => typeof x === 'string') : [];
	} catch {
		return [];
	}
}

/** Ids of the tasks with their subtasks hidden. */
export const collapsedSubtasks = writable<string[]>(load());

export function toggleSubtasks(id: string) {
	collapsedSubtasks.update((list) => {
		const next = list.includes(id) ? list.filter((x) => x !== id) : [...list, id];
		try {
			localStorage.setItem(KEY, JSON.stringify(next));
		} catch {
			/* not remembered after a reload */
		}
		return next;
	});
}
