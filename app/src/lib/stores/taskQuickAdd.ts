/**
 * The app-wide quick add: a dialog in the root layout that adds a task from any
 * page. Alt+N (while the main window has focus) or the Tasks pane's "Add task" opens
 * it. From any other app, Alt+N shows the popup instead (routes/widget/quick).
 * The Tasks page tells it where a new task should go by default (the project or
 * day being viewed) and hears about each task it adds, so its list stays current.
 */
import { writable } from 'svelte/store';
import type { Task, TaskPriority } from '$lib/api';

export type QuickAddDefaults = {
	project_id?: string | null;
	category_id?: string | null;
	due_date?: string | null;
	priority?: TaskPriority;
};

/** The defaults the open dialog uses, or null while it is closed. */
export const quickAdd = writable<QuickAddDefaults | null>(null);

let context: (() => QuickAddDefaults) | null = null;
const listeners = new Set<(task: Task) => void>();

/** Opens the dialog, with the given defaults or else those of the page being viewed. */
export function openQuickAdd(defaults?: QuickAddDefaults) {
	quickAdd.set(defaults ?? context?.() ?? {});
}

export function closeQuickAdd() {
	quickAdd.set(null);
}

/** Lets a page supply the defaults; returns the function that withdraws them. */
export function setQuickAddContext(fn: () => QuickAddDefaults): () => void {
	context = fn;
	return () => {
		if (context === fn) context = null;
	};
}

/** Calls `fn` with each task the dialog adds; returns the function that stops it. */
export function onTaskAdded(fn: (task: Task) => void): () => void {
	listeners.add(fn);
	return () => listeners.delete(fn);
}

export function taskAdded(task: Task) {
	listeners.forEach((fn) => fn(task));
}

/** Alt+N, with no other modifier. Matched on the key's position so macOS Option+N works too. */
export function isQuickAddShortcut(e: KeyboardEvent): boolean {
	return e.altKey && !e.ctrlKey && !e.metaKey && !e.shiftKey && e.code === 'KeyN';
}
