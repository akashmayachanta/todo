import type { Task, TaskCategory, TaskPatch, TaskPriority, TaskProject } from '$lib/api';
import { plainText } from '$lib/links';
import { daysBetween, localInstant, nextOccurrence } from '$lib/taskParse';
import { DEFAULT_DATE_PREFS, type DatePrefs } from '$lib/datePrefs.svelte';

export type TaskView = 'inbox' | 'today' | 'upcoming' | 'karma' | 'project' | 'filter' | 'search';

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

/** Display settings from the pane's Settings section, remembered in this browser. Date & time is in DatePrefs. */
export type TaskSettings = DatePrefs & {
	/** Open subtasks listed under their task (Inbox and projects, list and board). */
	showSubtasks: boolean;
	/** The first line of a task's description under its title. */
	showNotes: boolean;
	/** Where a task lives, in Today and Upcoming. */
	showPlace: boolean;
	/** Open-task counts in the pane. */
	showCounts: boolean;
	/**
	 * Done tasks listed under the open ones (Inbox and projects, list and board), and
	 * done subtasks under their task, in the lists and the task window.
	 */
	showCompleted: boolean;
	/** The view `/tasks` opens on. */
	startView: 'today' | 'inbox' | 'upcoming';
	/** The days after today that Upcoming lists by day and counts in the pane. */
	upcomingDays: UpcomingDays;
	/** Quick add files a task in the project and category its words point to ($lib/suggestPlace). */
	autoPlace: boolean;
	/** Karma: tasks to complete a day, and a week. */
	dailyGoal: number;
	weeklyGoal: number;
};

export const UPCOMING_DAY_CHOICES = [1, 7, 14] as const;
export type UpcomingDays = (typeof UPCOMING_DAY_CHOICES)[number];

/** A goal is a whole number of tasks, 1 to 999. */
export const clampGoal = (n: number, fallback: number) =>
	Number.isFinite(n) ? Math.min(999, Math.max(1, Math.round(n))) : fallback;

export const DEFAULT_TASK_SETTINGS: TaskSettings = {
	showSubtasks: true,
	showNotes: true,
	showPlace: true,
	showCounts: true,
	showCompleted: false,
	startView: 'today',
	upcomingDays: 14,
	autoPlace: true,
	dailyGoal: 5,
	weeklyGoal: 25,
	...DEFAULT_DATE_PREFS,
};

export const SETTINGS_KEY = 'tasks.settings';

export function loadTaskSettings(): TaskSettings {
	const out = { ...DEFAULT_TASK_SETTINGS };
	try {
		const raw = JSON.parse(localStorage.getItem(SETTINGS_KEY) ?? '{}');
		if (!raw || typeof raw !== 'object') return out;
		if (raw.timeFormat === '12h' || raw.timeFormat === '24h') out.timeFormat = raw.timeFormat;
		if (raw.dateFormat === 'dmy' || raw.dateFormat === 'mdy') out.dateFormat = raw.dateFormat;
		for (const k of ['weekStart', 'nextWeek'] as const) {
			if (Number.isInteger(raw[k]) && raw[k] >= 0 && raw[k] <= 6) out[k] = raw[k];
		}
		if (raw.weekend === 0 || raw.weekend === 6) out.weekend = raw.weekend;
		for (const k of ['showSubtasks', 'showNotes', 'showPlace', 'showCounts', 'showCompleted', 'autoPlace', 'smartDates'] as const) {
			if (typeof raw[k] === 'boolean') out[k] = raw[k];
		}
		if (['today', 'inbox', 'upcoming'].includes(raw.startView)) out.startView = raw.startView;
		if (UPCOMING_DAY_CHOICES.includes(raw.upcomingDays)) out.upcomingDays = raw.upcomingDays;
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

/** The most characters a subtask's title shows (core's SUBTASK_TITLE_MAX). */
export const SUBTASK_MAX = 255;

/**
 * Where a subtask title stands against the limit: links count as their label, as in
 * core. `saved` is its current title: an older, longer one can stay until it is changed.
 */
export function subtaskLimit(title: string, saved?: string): { count: number; shown: boolean; over: boolean } {
	const count = [...plainText(title)].length; // characters, as core counts them, not UTF-16 units
	return { count, shown: count >= 200, over: count > SUBTASK_MAX && title !== saved };
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

/** Finds a subtask's parent among `tasks`; null for a top-level task. */
export function parentFinder(tasks: Task[]): (task: Task) => Task | null {
	const byId = new Map(tasks.map((t) => [t.id, t]));
	return (task) => (task.parent_id ? byId.get(task.parent_id) ?? null : null);
}

/** The top-level tasks to list so these tasks show in their hierarchy, in the order they're first reached. */
export function taskRoots(list: Task[], parentOf: (task: Task) => Task | null): Task[] {
	const roots = new Map<string, Task>();
	for (const task of list) {
		const root = parentOf(task) ?? task;
		if (!roots.has(root.id)) roots.set(root.id, root);
	}
	return [...roots.values()];
}

/**
 * Which subtasks a date view lists under a task: all of them when the task itself is
 * due, or only the due ones when it's there just for those.
 */
export function shownWhenDue(date: string): (parent: Task, subtask: Task) => boolean {
	const due = (t: Task) => !!t.due_date && t.due_date <= date;
	return (parent, subtask) => due(parent) || due(subtask);
}

/**
 * The duration a task takes from its subtasks: what is left of theirs, the open ones'
 * total (capped at the 7-day limit), 0 when those are all done. Null when none has a
 * duration, and the task keeps its own.
 */
export function subtaskMinutes(subtasks: Task[]): number | null {
	if (!subtasks.some((s) => s.duration_minutes)) return null;
	let total = 0;
	for (const s of subtasks) if (!s.completed_at) total += s.duration_minutes ?? 0;
	return Math.min(total, 10080);
}

/**
 * The duration to save on a task so it stays its subtasks' remaining total, or undefined
 * when it already is. `hadMinutes`: a subtask with a duration lost it or left, so a task
 * with none left drops the old total rather than keep it.
 */
export function parentMinutes(parent: Task, subtasks: Task[], hadMinutes = false): number | null | undefined {
	const left = subtaskMinutes(subtasks);
	const next = left === null ? (hadMinutes ? null : parent.duration_minutes) : left || null;
	return next === parent.duration_minutes ? undefined : next;
}

/**
 * Minutes of open work in each project and category: every open task's duration. A task
 * whose duration is its subtasks' total is left out, as its open subtasks count on their own.
 */
export function durationTotals(
	open: Task[],
	subtasksOf: (id: string) => Task[],
): { projects: Record<string, number>; categories: Record<string, number> } {
	const projects: Record<string, number> = {};
	const categories: Record<string, number> = {};
	for (const t of open) {
		const m = t.duration_minutes ?? 0;
		if (!m || (t.parent_id === null && subtaskMinutes(subtasksOf(t.id)) !== null)) continue;
		if (t.project_id !== null) projects[t.project_id] = (projects[t.project_id] ?? 0) + m;
		if (t.category_id !== null) categories[t.category_id] = (categories[t.category_id] ?? 0) + m;
	}
	return { projects, categories };
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

/**
 * What ticking or unticking a task changes: a repeating task moves to its next date,
 * and its reminder by as many days, with the tick logged for Karma.
 */
export function completionPatch(task: Task, now: Date): TaskPatch {
	if (task.completed_at) return { completed_at: null };
	const next = task.recurrence && task.due_date ? nextOccurrence(task.recurrence, task.due_date, now) : null;
	if (!next) return { completed_at: new Date().toISOString() };
	const shift = daysBetween(task.due_date!, next);
	return {
		due_date: next,
		remind_at: task.remind_at ? shiftInstant(task.remind_at, shift) : null,
		completes_occurrence: true,
	};
}

/** An ISO instant moved by whole local days (so 9:00 stays 9:00 across DST). */
export function shiftInstant(iso: string, days: number): string {
	const d = new Date(iso);
	d.setDate(d.getDate() + days);
	return d.toISOString();
}
