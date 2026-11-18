// The Tasks data source. The app sets it to its Tauri client before anything
// loads (src/routes/+layout.svelte); components reach it through api.tasks.

import type {
	Task, TaskSection, TaskCompletions, TaskFilter, TaskFilterIn, TaskIn, TaskPatch, TaskProject,
	TaskReport, TaskReportQuery,
} from '$lib/api';

export interface TaskListOptions {
	completed?: boolean;
	project_id?: string | null;
	inbox?: boolean;
	/** Search titles and descriptions (global search): open tasks, or done ones with `completed`. */
	q?: string;
	limit?: number;
}

export interface TasksClient {
	projects: {
		list(): Promise<TaskProject[]>;
		create(data: { name: string; color?: string | null }): Promise<TaskProject>;
		update(id: string, data: { name: string; color?: string | null; favorite?: boolean; description?: string | null }): Promise<TaskProject>;
		/** Its sections and tasks go with it. */
		delete(id: string): Promise<void>;
		/** Every project id in the new order; returns the projects in that order. */
		reorder(ids: string[]): Promise<TaskProject[]>;
	};
	sections: {
		list(projectId?: string): Promise<TaskSection[]>;
		create(data: { project_id: string; name: string; description?: string | null }): Promise<TaskSection>;
		update(id: string, data: { project_id: string; name: string; description?: string | null }): Promise<TaskSection>;
		/** Its tasks stay in the project, without a section. */
		delete(id: string): Promise<void>;
		/** A project's section ids in the new order; returns its sections in that order. */
		reorder(projectId: string, ids: string[]): Promise<TaskSection[]>;
	};
	filters: {
		list(): Promise<TaskFilter[]>;
		create(data: TaskFilterIn): Promise<TaskFilter>;
		/** A full update; leaving out position keeps its place. */
		update(id: string, data: TaskFilterIn): Promise<TaskFilter>;
		delete(id: string): Promise<void>;
	};
	/** Open tasks (with all their subtasks) by default; `completed` lists done ones, newest first; `q` searches. */
	list(opts?: TaskListOptions): Promise<Task[]>;
	get(id: string): Promise<Task>;
	/** The Karma log since an ISO instant. */
	completions(since: string): Promise<TaskCompletions>;
	/** A page of Reporting: ticks newest first, each with its task. */
	report(query: TaskReportQuery): Promise<TaskReport>;
	create(data: TaskIn): Promise<Task>;
	update(id: string, patch: TaskPatch): Promise<Task>;
	/**
	 * Ticks a task. Core decides what that changes: a repeating task stays open and moves to
	 * its next date (with its reminder), a subtask's time leaves its task's total. Returns the
	 * task as it is now.
	 */
	complete(id: string): Promise<Task>;
	/** Unticks a done task; a subtask's time returns to its task's total. */
	reopen(id: string): Promise<Task>;
	/** Its subtasks go with it. */
	delete(id: string): Promise<void>;
	/** Deletes the done ones among these ids (with their subtasks); returns how many. */
	deleteCompleted(ids: string[]): Promise<number>;
	/** Sibling tasks in the new order; returns them in that order. */
	reorder(ids: string[]): Promise<Task[]>;
}

let current: TasksClient | null = null;

/** Sets the data source; the app's layout calls this before any page loads. */
export function setTasksClient(client: TasksClient) {
	current = client;
}

export function tasksClient(): TasksClient {
	if (!current) throw new Error('No tasks client');
	return current;
}

const changeListeners = new Set<() => void>();

/** Calls `fn` when the tasks changed somewhere else (a sync pulled another
 *  device's edits), so a page can load them again. Returns the function that stops it. */
export function onTasksChanged(fn: () => void): () => void {
	changeListeners.add(fn);
	return () => {
		changeListeners.delete(fn);
	};
}

export function tasksChanged() {
	changeListeners.forEach((fn) => fn());
}
