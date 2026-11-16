import type { Task, TaskSection, TaskFilter, TaskIn, TaskProject } from '$lib/api';
import { plainText } from '$lib/links';
import { addDays } from '$lib/taskParse';
import { byDue, byPosition, parentFinder, projectColor, shownWhenDue, taskRoots } from './shared';
import { compileFilter, filterDefaults, type AddDefaults, type GroupCtx } from './taskFilters';

/**
 * What a pinned window shows. The window's label names its list
 * (`pin-today`, `pin-project-<id>`; src-tauri/src/state.rs has the same kinds).
 */
export type PinKind = 'inbox' | 'today' | 'upcoming' | 'project' | 'section' | 'filter' | 'task';

export interface PinTarget {
	kind: PinKind;
	id: string | null;
}

export function pinFromLabel(label: string): PinTarget | null {
	const m = /^pin-(inbox|today|upcoming|project|section|filter|task)(?:-(.+))?$/.exec(label);
	return m ? { kind: m[1] as PinKind, id: m[2] ?? null } : null;
}

export interface PinData {
	/** Open tasks, each with all its subtasks, as the main page loads them. */
	tasks: Task[];
	/** A pinned task when it isn't among those (done). */
	pinnedTask: Task | null;
	projects: TaskProject[];
	sections: TaskSection[];
	filters: TaskFilter[];
	today: string;
}

export interface PinView {
	/** Its project, section or task was deleted. */
	missing: boolean;
	title: string;
	/** A CSS colour for the mark before the title. */
	color: string | null;
	/** That mark: `#` for a project (or a task in one), `/` for a section, a dot for a filter. */
	mark?: 'project' | 'section';
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
	const { tasks, projects, sections, filters, today } = data;
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
			const order = new Map(sections.map((c) => [c.id, c.position]));
			const rank = (t: Task) => (t.section_id === null ? -Infinity : order.get(t.section_id) ?? Infinity);
			return {
				...base,
				title: project.name,
				color: projectColor(project.color),
				mark: 'project',
				tasks: top.filter((t) => t.project_id === project.id).sort((a, b) => rank(a) - rank(b) || byPosition(a, b)),
				nested: true,
				defaults: { project_id: project.id },
			};
		}
		case 'section': {
			const section = sections.find((c) => c.id === target.id);
			const project = section && projects.find((p) => p.id === section.project_id);
			if (!section || !project) return MISSING;
			return {
				...base,
				title: `${project.name} / ${section.name}`,
				color: projectColor(project.color),
				mark: 'section',
				tasks: top.filter((t) => t.section_id === section.id).sort(byPosition),
				nested: true,
				defaults: { project_id: project.id, section_id: section.id },
			};
		}
		case 'filter': {
			const filter = filters.find((f) => f.id === target.id);
			if (!filter) return MISSING;
			const ctx: GroupCtx = { projects, sections, today };
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
				mark: 'project',
				tasks: [task],
				nested: true,
				parentId: task.parent_id ?? task.id,
			};
		}
	}
}

/** A new task from a pin's Add row; a subtask's place follows its parent. */
export function pinNewTask(view: PinView, data: TaskIn): TaskIn {
	return view.parentId ? { ...data, parent_id: view.parentId, project_id: null, section_id: null } : data;
}

// ── A pin's colour ───────────────────────────────────────────────────────────

/** Sticky-note pastels a pin can be painted in. The key is what is saved. */
export const PIN_COLORS = [
	{ key: 'yellow',   label: 'Yellow',   value: '#fdf3b4' },
	{ key: 'peach',    label: 'Peach',    value: '#fde0c8' },
	{ key: 'pink',     label: 'Pink',     value: '#fbd8e4' },
	{ key: 'lavender', label: 'Lavender', value: '#e6dcf8' },
	{ key: 'blue',     label: 'Blue',     value: '#d6e8fb' },
	{ key: 'mint',     label: 'Mint',     value: '#d4f1e0' },
	{ key: 'lime',     label: 'Lime',     value: '#e8f2c8' },
	{ key: 'sand',     label: 'Sand',     value: '#ece6dc' },
] as const;

/**
 * What a pin was set to: a PIN_COLORS key, `none` for the app's own white or black,
 * or null when never set, which paints it a pale shade of its list's colour.
 */
export type PinColorChoice = (typeof PIN_COLORS)[number]['key'] | 'none' | null;

// Pins are this device's, so their colours are too: kept in the windows' shared localStorage.
const colorKey = (label: string) => `tasks.pin-color.${label}`;

export function loadPinColor(label: string): PinColorChoice {
	try {
		const v = localStorage.getItem(colorKey(label));
		return v === 'none' || PIN_COLORS.some((c) => c.key === v) ? (v as PinColorChoice) : null;
	} catch {
		return null;
	}
}

export function savePinColor(label: string, choice: PinColorChoice) {
	try {
		if (choice === null) localStorage.removeItem(colorKey(label));
		else localStorage.setItem(colorKey(label), choice);
	} catch {
		// Not saved: the pin keeps it until it closes.
	}
}

/** The pin's background, or null for the app's own. `listColor` is the view's colour. */
export function pinBackground(choice: PinColorChoice, listColor: string | null): string | null {
	if (choice === 'none') return null;
	const picked = PIN_COLORS.find((c) => c.key === choice);
	if (picked) return picked.value;
	return listColor ? `color-mix(in oklch, ${listColor} 24%, white)` : null;
}
