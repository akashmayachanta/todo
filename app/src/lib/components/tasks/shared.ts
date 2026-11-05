import type { Task, TaskCategory, TaskPriority, TaskProject } from '$lib/api';
import { localInstant } from '$lib/taskParse';

export type TaskView = 'inbox' | 'today' | 'upcoming' | 'karma' | 'project' | 'filter';

/** How a view is laid out, chosen per view (like Todoist's View menu). */
export type TaskLayout = 'list' | 'board' | 'calendar';

const LAYOUT_KEY = 'tasks.layout';

/** The saved layout of each view: `inbox`, `today`, `upcoming` or `project:<id>`. */
export function loadLayouts(): Record<string, TaskLayout> {
	try {
		const raw = JSON.parse(localStorage.getItem(LAYOUT_KEY) ?? '{}');
		return raw && typeof raw === 'object' ? raw : {};
	} catch {
		return {};
	}
}

export function saveLayouts(layouts: Record<string, TaskLayout>) {
	try { localStorage.setItem(LAYOUT_KEY, JSON.stringify(layouts)); } catch { /* the choice just isn't remembered */ }
}

/** Display settings from the pane's Settings section, remembered in this browser. */
export type TaskSettings = {
	/** Open subtasks listed under their task (Inbox and projects, list and board). */
	showSubtasks: boolean;
	/** Done subtasks listed under their task too, ticked. */
	showDoneSubtasks: boolean;
	/** The first line of a task's description under its title. */
	showNotes: boolean;
	/** Where a task lives, in Today and Upcoming. */
	showPlace: boolean;
	/** Open-task counts in the pane. */
	showCounts: boolean;
	/** Done tasks listed under the open ones (Inbox and projects, list and board). */
	showCompleted: boolean;
	/** The view `/tasks` opens on. */
	startView: 'today' | 'inbox' | 'upcoming';
	/** Karma: tasks to complete a day, and a week. */
	dailyGoal: number;
	weeklyGoal: number;
};

/** A goal is a whole number of tasks, 1 to 999. */
export const clampGoal = (n: number, fallback: number) =>
	Number.isFinite(n) ? Math.min(999, Math.max(1, Math.round(n))) : fallback;

export const DEFAULT_TASK_SETTINGS: TaskSettings = {
	showSubtasks: true,
	showDoneSubtasks: false,
	showNotes: true,
	showPlace: true,
	showCounts: true,
	showCompleted: false,
	startView: 'today',
	dailyGoal: 5,
	weeklyGoal: 25,
};

const SETTINGS_KEY = 'tasks.settings';

export function loadTaskSettings(): TaskSettings {
	const out = { ...DEFAULT_TASK_SETTINGS };
	try {
		const raw = JSON.parse(localStorage.getItem(SETTINGS_KEY) ?? '{}');
		if (!raw || typeof raw !== 'object') return out;
		for (const k of ['showSubtasks', 'showDoneSubtasks', 'showNotes', 'showPlace', 'showCounts', 'showCompleted'] as const) {
			if (typeof raw[k] === 'boolean') out[k] = raw[k];
		}
		if (['today', 'inbox', 'upcoming'].includes(raw.startView)) out.startView = raw.startView;
		for (const k of ['dailyGoal', 'weeklyGoal'] as const) {
			if (typeof raw[k] === 'number') out[k] = clampGoal(raw[k], out[k]);
		}
	} catch {
		/* storage blocked or a bad value: the defaults */
	}
	return out;
}

export function saveTaskSettings(settings: TaskSettings) {
	try { localStorage.setItem(SETTINGS_KEY, JSON.stringify(settings)); } catch { /* not remembered after a reload */ }
}

/**
 * The reminder a task gets by default: at its due time, when it has one that is
 * still ahead. A date with no time gets none.
 */
export function defaultReminder(due_date: string | null | undefined, due_time: string | null | undefined, now = new Date()): string | null {
	if (!due_date || !due_time) return null;
	const at = localInstant(due_date, due_time);
	return new Date(at).getTime() > now.getTime() ? at : null;
}

/** Project colours; the key is what is stored in task_projects.color. */
export const PROJECT_COLORS = [
	{ key: 'grey',   label: 'Grey',   value: 'oklch(62% 0.012 245)' },
	{ key: 'red',    label: 'Red',    value: 'oklch(62% 0.19 25)' },
	{ key: 'orange', label: 'Orange', value: 'oklch(70% 0.16 55)' },
	{ key: 'yellow', label: 'Yellow', value: 'oklch(78% 0.15 90)' },
	{ key: 'green',  label: 'Green',  value: 'oklch(64% 0.15 150)' },
	{ key: 'teal',   label: 'Teal',   value: 'oklch(64% 0.10 195)' },
	{ key: 'blue',   label: 'Blue',   value: 'oklch(60% 0.15 250)' },
	{ key: 'violet', label: 'Violet', value: 'oklch(58% 0.17 295)' },
	{ key: 'pink',   label: 'Pink',   value: 'oklch(66% 0.17 350)' },
] as const;

export function projectColor(key: string | null | undefined): string {
	return (PROJECT_COLORS.find((c) => c.key === key) ?? PROJECT_COLORS[0]).value;
}

export const PRIORITY_OPTIONS: { value: TaskPriority; label: string }[] = [
	{ value: 1, label: 'Priority 1' },
	{ value: 2, label: 'Priority 2' },
	{ value: 3, label: 'Priority 3' },
	{ value: 4, label: 'Priority 4' },
];

/** Where a task sits, for a label: "Work / Backend", or "Inbox". */
export function placeName(task: Pick<Task, 'project_id' | 'category_id'>, projects: TaskProject[], categories: TaskCategory[]): string {
	const project = projects.find((p) => p.id === task.project_id);
	if (!project) return 'Inbox';
	const category = categories.find((c) => c.id === task.category_id);
	return category ? `${project.name} / ${category.name}` : project.name;
}

/** Due date, then time (untimed last), then priority, then manual order. */
export function byDue(a: Task, b: Task): number {
	return (a.due_date ?? '9999').localeCompare(b.due_date ?? '9999')
		|| (a.due_time ?? '99').localeCompare(b.due_time ?? '99')
		|| a.priority - b.priority
		|| a.position - b.position
		|| a.id.localeCompare(b.id);
}

export function byPosition(a: Task, b: Task): number {
	return a.position - b.position || a.id.localeCompare(b.id);
}

/** An ISO instant moved by whole local days (so 9:00 stays 9:00 across DST). */
export function shiftInstant(iso: string, days: number): string {
	const d = new Date(iso);
	d.setDate(d.getDate() + days);
	return d.toISOString();
}
