import type {
	Task, TaskCategory, TaskDueWindow, TaskFilterCriteria, TaskGroupBy, TaskPatch, TaskPriority, TaskProject, TaskRowField,
} from '$lib/api';
import { addDays, todayHeading } from '$lib/taskParse';
import { byDue, projectColor } from './shared';

/**
 * Saved filters and grouping on the Tasks page. Pure functions: the page passes in
 * the tasks, projects, categories and today's date, so nothing here reads the clock.
 */

export const DUE_WINDOWS: { value: TaskDueWindow; label: string }[] = [
	{ value: 'overdue',  label: 'Overdue' },
	{ value: 'today',    label: 'Today' },
	{ value: 'tomorrow', label: 'Tomorrow' },
	{ value: 'next_7',   label: 'Next 7 days' },
	{ value: 'next_30',  label: 'Next 30 days' },
	{ value: 'no_date',  label: 'No date' },
];

export const ROW_FIELDS: { value: TaskRowField; label: string }[] = [
	{ value: 'due',         label: 'Due date' },
	{ value: 'place',       label: 'Project' },
	{ value: 'description', label: 'Description' },
	{ value: 'duration',    label: 'Duration' },
	{ value: 'repeat',      label: 'Repeat' },
	{ value: 'reminder',    label: 'Reminder' },
	{ value: 'subtasks',    label: 'Subtask count' },
];

export const DEFAULT_FILTER_FIELDS: TaskRowField[] = ['due', 'place', 'description'];

export const FILTER_GROUPS: { value: TaskGroupBy; label: string }[] = [
	{ value: 'none',     label: 'None' },
	{ value: 'priority', label: 'Priority' },
	{ value: 'project',  label: 'Project' },
	{ value: 'category', label: 'Project / category' },
	{ value: 'due',      label: 'Due date' },
];

/** Grouping in Today and Upcoming; 'date' is their own day sections. */
export type DateViewGroup = 'date' | 'priority' | 'project' | 'category';

export const DATE_VIEW_GROUPS: { value: DateViewGroup; label: string }[] = [
	{ value: 'date',     label: 'Date' },
	{ value: 'priority', label: 'Priority' },
	{ value: 'project',  label: 'Project' },
	{ value: 'category', label: 'Project / category' },
];

export function emptyCriteria(): TaskFilterCriteria {
	return { projects: [], categories: [], priorities: [], due: [] };
}

export type GroupCtx = { projects: TaskProject[]; categories: TaskCategory[]; today: string };

/** What a task added from a bucket (or a filter) starts with. */
export type AddDefaults = { project_id?: string | null; category_id?: string | null; due_date?: string | null; priority?: TaskPriority };

// ── Matching ──────────────────────────────────────────────────────────────────

/**
 * The criteria without ids of deleted projects or categories, and without
 * categories whose project isn't picked (they are only offered under one).
 */
export function pruneCriteria(c: TaskFilterCriteria, ctx: GroupCtx): TaskFilterCriteria {
	const projects = c.projects.filter((id) => id === null || ctx.projects.some((p) => p.id === id));
	const categories = c.categories.filter((id) => {
		const cat = ctx.categories.find((x) => x.id === id);
		return !!cat && projects.includes(cat.project_id);
	});
	return { ...c, projects, categories };
}

function inWindow(due: string | null, w: TaskDueWindow, today: string): boolean {
	if (w === 'no_date') return !due;
	if (!due) return false;
	switch (w) {
		case 'overdue':  return due < today;
		case 'today':    return due === today;
		case 'tomorrow': return due === addDays(today, 1);
		case 'next_7':   return due >= today && due <= addDays(today, 6);
		case 'next_30':  return due >= today && due <= addDays(today, 29);
	}
}

/**
 * A test for one filter, built once for a list of tasks. A task passes every
 * non-empty criterion, and any value within one. A filter whose picked places
 * were all deleted matches nothing rather than everything.
 */
export function compileFilter(c: TaskFilterCriteria, ctx: GroupCtx): (task: Task) => boolean {
	const pc = pruneCriteria(c, ctx);
	if (c.projects.length && !pc.projects.length) return () => false;
	const catProject = new Map(ctx.categories.map((x) => [x.id, x.project_id]));
	return (task) => {
		if (pc.projects.length) {
			if (!pc.projects.includes(task.project_id)) return false;
			if (task.project_id !== null) {
				const picked = pc.categories.filter((id) => catProject.get(id) === task.project_id);
				if (picked.length && (task.category_id === null || !picked.includes(task.category_id))) return false;
			}
		}
		if (pc.priorities.length && !pc.priorities.includes(task.priority)) return false;
		if (pc.due.length && !pc.due.some((w) => inWindow(task.due_date, w, ctx.today))) return false;
		return true;
	};
}

export function matchesFilter(task: Task, c: TaskFilterCriteria, ctx: GroupCtx): boolean {
	return compileFilter(c, ctx)(task);
}

/** What a task added in a filter starts with: whatever the filter pins to one value. */
export function filterDefaults(c: TaskFilterCriteria, ctx: GroupCtx): AddDefaults {
	const pc = pruneCriteria(c, ctx);
	const out: AddDefaults = {};
	if (pc.projects.length === 1 && pc.projects[0] !== null) {
		out.project_id = pc.projects[0];
		if (pc.categories.length === 1) out.category_id = pc.categories[0];
	}
	if (pc.priorities.length === 1) out.priority = pc.priorities[0];
	if (pc.due.length === 1 && pc.due[0] === 'today') out.due_date = ctx.today;
	if (pc.due.length === 1 && pc.due[0] === 'tomorrow') out.due_date = addDays(ctx.today, 1);
	return out;
}

// ── Grouping ──────────────────────────────────────────────────────────────────

export type Bucket = {
	key: string;
	title: string;
	sub?: string;
	/** A CSS colour for the heading's dot (a project's); null for the Inbox. */
	color?: string | null;
	overdue?: boolean;
	tasks: Task[];
	/** What dropping a task here changes; null when nothing can be dropped here. */
	drop: TaskPatch | null;
	defaults: AddDefaults;
};

type Spec = Omit<Bucket, 'tasks'>;

const PRIORITY_COLORS: Record<TaskPriority, string | null> = {
	1: 'var(--color-danger)',
	2: 'var(--color-warning)',
	3: 'var(--color-info)',
	4: null,
};

function bucketSpecs(by: Exclude<TaskGroupBy, 'none'>, ctx: GroupCtx): Spec[] {
	const sortedCats = [...ctx.categories].sort((a, b) => a.position - b.position || a.id.localeCompare(b.id));
	switch (by) {
		case 'priority':
			return ([1, 2, 3, 4] as TaskPriority[]).map((p) => ({
				key: `pri:${p}`, title: `Priority ${p}`, color: PRIORITY_COLORS[p], drop: { priority: p }, defaults: { priority: p },
			}));
		case 'project':
			return [
				{ key: 'proj:inbox', title: 'Inbox', color: null, drop: { project_id: null, category_id: null }, defaults: {} },
				...ctx.projects.map((p) => ({
					key: `proj:${p.id}`, title: p.name, color: projectColor(p.color),
					drop: { project_id: p.id, category_id: null }, defaults: { project_id: p.id },
				})),
			];
		case 'category':
			return [
				{ key: 'cat:inbox:none', title: 'Inbox', color: null, drop: { project_id: null, category_id: null }, defaults: {} },
				...ctx.projects.flatMap((p) => [
					{
						key: `cat:${p.id}:none`, title: p.name, sub: 'No category', color: projectColor(p.color),
						drop: { project_id: p.id, category_id: null }, defaults: { project_id: p.id },
					},
					...sortedCats.filter((c) => c.project_id === p.id).map((c) => ({
						key: `cat:${p.id}:${c.id}`, title: `${p.name} / ${c.name}`, color: projectColor(p.color),
						drop: { project_id: p.id, category_id: c.id }, defaults: { project_id: p.id, category_id: c.id },
					})),
				]),
			];
		case 'due': {
			const tomorrow = addDays(ctx.today, 1);
			return [
				{ key: 'due:overdue', title: 'Overdue', overdue: true, drop: null, defaults: {} },
				{ key: 'due:today', title: todayHeading(ctx.today), drop: { due_date: ctx.today }, defaults: { due_date: ctx.today } },
				{ key: 'due:tomorrow', title: 'Tomorrow', drop: { due_date: tomorrow }, defaults: { due_date: tomorrow } },
				{ key: 'due:next_7', title: 'Next 7 days', drop: null, defaults: {} },
				{ key: 'due:later', title: 'Later', drop: null, defaults: {} },
				{ key: 'due:no_date', title: 'No date', drop: { due_date: null, due_time: null }, defaults: { due_date: null } },
			];
		}
	}
}

function bucketKey(task: Task, by: Exclude<TaskGroupBy, 'none'>, today: string): string {
	switch (by) {
		case 'priority': return `pri:${task.priority}`;
		case 'project':  return `proj:${task.project_id ?? 'inbox'}`;
		case 'category': return `cat:${task.project_id ?? 'inbox'}:${task.project_id === null ? 'none' : task.category_id ?? 'none'}`;
		case 'due': {
			const d = task.due_date;
			if (!d) return 'due:no_date';
			if (d < today) return 'due:overdue';
			if (d === today) return 'due:today';
			if (d === addDays(today, 1)) return 'due:tomorrow';
			return d <= addDays(today, 6) ? 'due:next_7' : 'due:later';
		}
	}
}

/**
 * The tasks in buckets, in a fixed order (P1→P4; Inbox then projects; each
 * project's "No category" then its categories; Overdue → No date), each sorted
 * by due date. Empty buckets are left out, except all four priorities with
 * `allPriorities` (the board, so every priority is a drop target).
 */
export function groupTasks(
	tasks: Task[], by: Exclude<TaskGroupBy, 'none'>, ctx: GroupCtx, opts: { allPriorities?: boolean } = {},
): Bucket[] {
	const buckets = new Map<string, Bucket>(bucketSpecs(by, ctx).map((s) => [s.key, { ...s, tasks: [] }]));
	for (const t of tasks) {
		const key = bucketKey(t, by, ctx.today);
		let b = buckets.get(key);
		if (!b) {
			// A project or category this page doesn't know yet (a race with a reload).
			b = { key, title: 'Other', drop: null, defaults: {}, tasks: [] };
			buckets.set(key, b);
		}
		b.tasks.push(t);
	}
	const keepEmpty = by === 'priority' && opts.allPriorities;
	return [...buckets.values()]
		.filter((b) => b.tasks.length || keepEmpty)
		.map((b) => ({ ...b, tasks: b.tasks.sort(byDue) }));
}

// ── The grouping picked for Today and Upcoming, remembered in this browser ────

const GROUPING_KEY = 'tasks.grouping';

export function loadGrouping(): Record<string, DateViewGroup> {
	try {
		const raw = JSON.parse(localStorage.getItem(GROUPING_KEY) ?? '{}');
		if (!raw || typeof raw !== 'object') return {};
		const ok = DATE_VIEW_GROUPS.map((g) => g.value) as string[];
		return Object.fromEntries(Object.entries(raw).filter(([, v]) => ok.includes(v as string))) as Record<string, DateViewGroup>;
	} catch {
		return {};
	}
}

export function saveGrouping(grouping: Record<string, DateViewGroup>) {
	try { localStorage.setItem(GROUPING_KEY, JSON.stringify(grouping)); } catch { /* not remembered after a reload */ }
}
