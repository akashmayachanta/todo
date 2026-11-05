import type { Task, TaskCategory, TaskFilter, TaskIn, TaskProject } from '$lib/api';
import { plainText } from '$lib/links';
import { addDays } from '$lib/taskParse';
import { byDue, byPosition, parentFinder, projectColor, shownWhenDue, taskRoots } from './shared';
import { compileFilter, filterDefaults, type AddDefaults, type GroupCtx } from './taskFilters';

/**
 * What a pinned window shows. The window's label names its list
 * (`pin-today`, `pin-project-<id>`; src-tauri/src/state.rs has the same kinds).
 */
export type PinKind = 'inbox' | 'today' | 'upcoming' | 'project' | 'category' | 'filter' | 'task';

export interface PinTarget {
	kind: PinKind;
	id: string | null;
}

export function pinFromLabel(label: string): PinTarget | null {
	const m = /^pin-(inbox|today|upcoming|project|category|filter|task)(?:-(.+))?$/.exec(label);
	return m ? { kind: m[1] as PinKind, id: m[2] ?? null } : null;
}

export interface PinData {
	/** Open tasks, each with all its subtasks, as the main page loads them. */
	tasks: Task[];
	/** A pinned task when it isn't among those (done). */
	pinnedTask: Task | null;
	projects: TaskProject[];
	categories: TaskCategory[];
	filters: TaskFilter[];
	today: string;
}

export interface PinView {
	/** Its project, category or task was deleted. */
	missing: boolean;
	title: string;
	/** A CSS colour for the dot before the title. */
	color: string | null;
	tasks: Task[];
	/** Their subtasks are listed under them. */
	nested: boolean;
	/** Narrows the subtasks listed under a task. */
	subtaskFilter?: (parent: Task, subtask: Task) => boolean;
	/** The number in the title bar, when it isn't the open tasks listed. */
	count?: number;
	/** What a task added here starts with. */
	defaults: AddDefaults;
	/** New tasks are subtasks of this one. */
	parentId: string | null;
}

const MISSING: PinView = { missing: true, title: 'Removed', color: null, tasks: [], nested: false, defaults: {}, parentId: null };

/** The tasks of a pin, in the order the main page lists them. */
export function pinView(target: PinTarget, data: PinData): PinView {
	const { tasks, projects, categories, filters, today } = data;
	const open = tasks.filter((t) => !t.completed_at);
	const top = open.filter((t) => t.parent_id === null);
	const dated = open.filter((t) => t.due_date);
	const base = { missing: false, color: null, nested: false, defaults: {}, parentId: null };
	switch (target.kind) {
		case 'inbox':
			return { ...base, title: 'Inbox', tasks: top.filter((t) => t.project_id === null).sort(byPosition), nested: true };
		case 'today': {
			const due = dated.filter((t) => t.due_date! <= today).sort(byDue);
			return {
				...base,
				title: 'Today',
				tasks: taskRoots(due, parentFinder(tasks)),
				nested: true,
				subtaskFilter: shownWhenDue(today),
				count: due.length,
				defaults: { due_date: today },
			};
		}
		case 'upcoming':
			return { ...base, title: 'Upcoming', tasks: dated.filter((t) => t.due_date! > today).sort(byDue), defaults: { due_date: addDays(today, 1) } };
		case 'project': {
			const project = projects.find((p) => p.id === target.id);
			if (!project) return MISSING;
			const order = new Map(categories.map((c) => [c.id, c.position]));
			const rank = (t: Task) => (t.category_id === null ? -Infinity : order.get(t.category_id) ?? Infinity);
			return {
				...base,
				title: project.name,
				color: projectColor(project.color),
				tasks: top.filter((t) => t.project_id === project.id).sort((a, b) => rank(a) - rank(b) || byPosition(a, b)),
				nested: true,
				defaults: { project_id: project.id },
			};
		}
		case 'category': {
			const category = categories.find((c) => c.id === target.id);
			const project = category && projects.find((p) => p.id === category.project_id);
			if (!category || !project) return MISSING;
			return {
				...base,
				title: `${project.name} / ${category.name}`,
				color: projectColor(project.color),
				tasks: top.filter((t) => t.category_id === category.id).sort(byPosition),
				nested: true,
				defaults: { project_id: project.id, category_id: category.id },
			};
		}
		case 'filter': {
			const filter = filters.find((f) => f.id === target.id);
			if (!filter) return MISSING;
			const ctx: GroupCtx = { projects, categories, today };
			return {
				...base,
				title: filter.name,
				color: projectColor(filter.color),
				tasks: open.filter(compileFilter(filter.criteria, ctx)).sort(byDue),
				defaults: filterDefaults(filter.criteria, ctx),
			};
		}
		case 'task': {
			const task = tasks.find((t) => t.id === target.id) ?? (data.pinnedTask?.id === target.id ? data.pinnedTask : null);
			if (!task) return MISSING;
			const project = projects.find((p) => p.id === task.project_id);
			return {
				...base,
				title: plainText(task.title),
				color: project ? projectColor(project.color) : null,
				tasks: [task],
				nested: true,
				parentId: task.parent_id ?? task.id,
			};
		}
	}
}

/** A new task from a pin's Add row; a subtask's place follows its parent. */
export function pinNewTask(view: PinView, data: TaskIn): TaskIn {
	return view.parentId ? { ...data, parent_id: view.parentId, project_id: null, category_id: null } : data;
}
