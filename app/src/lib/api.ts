// The Tasks data types, and `api.tasks`: the data source the page and its
// components call. In the app that is the local DB, through Tauri commands
// (src/app/tauriClient.ts); see $lib/tasks/client.

import { tasksClient, type TasksClient } from '$lib/tasks/client';

/** The `detail` message from an error a TasksClient call threw, or the raw message.
 *  Errors arrive as "<status>: {"detail": ...}"; a validation error (422) can
 *  have a list of { loc, msg } as its detail. */
export function errorDetail(e: unknown): string {
	const message = e instanceof Error ? e.message : String(e);
	const body = message.replace(/^\d{3}: /, '');
	try {
		const detail = JSON.parse(body).detail;
		if (typeof detail === 'string') return detail;
		if (Array.isArray(detail) && detail.length) {
			return detail.map((d) => (d?.loc ? `${d.loc.at(-1)}: ${d.msg}` : String(d?.msg ?? d))).join('; ');
		}
		return message;
	} catch {
		return message;
	}
}

/** 1 is the highest. */
export type TaskPriority = 1 | 2 | 3 | 4;
export interface TaskProject {
	id: string; name: string; color: string | null; position: number;
	created_at: string; updated_at: string;
}
/** A section of a project. */
export interface TaskCategory {
	id: string; project_id: string; name: string; position: number;
	created_at: string; updated_at: string;
}
/** No project_id means the Inbox. A subtask (parent_id) shares its parent's project and category. */
export interface Task {
	id: string; project_id: string | null; category_id: string | null; parent_id: string | null;
	title: string; description: string | null; priority: TaskPriority;
	/** `YYYY-MM-DD` and `HH:MM`, the user's local date and time. */
	due_date: string | null; due_time: string | null;
	/** How long it takes, in minutes. */
	duration_minutes: number | null;
	/** A place in free text; arrival reminders come later. */
	location: string | null;
	/** ISO instant. */
	remind_at: string | null;
	/** e.g. "every monday, thursday"; see $lib/taskParse. */
	recurrence: string | null;
	position: number; completed_at: string | null; created_at: string; updated_at: string;
}
export interface TaskIn {
	title: string; description?: string | null;
	project_id?: string | null; category_id?: string | null; parent_id?: string | null;
	priority?: TaskPriority; due_date?: string | null; due_time?: string | null;
	duration_minutes?: number | null; location?: string | null; remind_at?: string | null; recurrence?: string | null; position?: number;
}
/** Only the fields sent change; null clears one. */
export type TaskPatch = Partial<Omit<Task, 'id' | 'created_at' | 'updated_at'>> & {
	/** With the next due_date, when a repeating task's occurrence is ticked: logged for Karma. */
	completes_occurrence?: boolean;
};
/** One tick in the Karma log. */
export interface TaskCompletion { task_id: string | null; completed_at: string }
/** total is every tick ever logged; items are those since the instant asked, oldest first. */
export interface TaskCompletions { total: number; items: TaskCompletion[] }

/** A due window a filter can match; see $lib/components/tasks/taskFilters. */
export type TaskDueWindow = 'overdue' | 'today' | 'tomorrow' | 'next_7' | 'next_30' | 'no_date';
/** A detail a task row can show. */
export type TaskRowField = 'due' | 'place' | 'description' | 'duration' | 'repeat' | 'reminder' | 'subtasks';
export type TaskGroupBy = 'none' | 'priority' | 'project' | 'category' | 'due';
/** Each list is "any of"; an empty one means any. null in projects is the Inbox. */
export interface TaskFilterCriteria {
	projects: (string | null)[]; categories: string[]; priorities: TaskPriority[]; due: TaskDueWindow[];
}
/** A saved view of the open tasks; the browser does the matching. */
export interface TaskFilter {
	id: string; name: string; color: string | null; criteria: TaskFilterCriteria;
	fields: TaskRowField[]; group_by: TaskGroupBy; position: number;
	created_at: string; updated_at: string;
}
export type TaskFilterIn = Omit<TaskFilter, 'id' | 'position' | 'created_at' | 'updated_at'> & { position?: number };

export const api = {
	/** The local tasks DB; see $lib/tasks/client. */
	get tasks(): TasksClient {
		return tasksClient();
	},
};
