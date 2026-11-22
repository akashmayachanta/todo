import { formatInstant } from '$lib/datePrefs.svelte';
import type { Task, TaskSection, TaskPatch, TaskProject } from '$lib/api';
import { pickedDayLabel, formatDuration, formatTime, localInstant, nextDayFor, parseEdit, ymd, type QuickAdd } from '$lib/taskParse';
import { projectColor } from './shared';

/**
 * Editing a task's title as a smart line, shared by the task modal and the inline
 * editor: "tomorrow 5pm", "p1", "#Project", "/Section", "every monday", "!30m"
 * typed into the title set those fields. Only what is typed is read; a subtask's
 * place follows its parent, so `#` and `/` mean nothing in one.
 */
export function smartTitle(text: string, task: Task, projects: TaskProject[], sections: TaskSection[]) {
	const ps = task.parent_id ? [] : projects;
	const cs = task.parent_id ? [] : sections;
	const smart = parseEdit(text, task.title, ps, cs, new Date(), task.project_id, task.due_date, task.due_time);
	const scope = smart.tokens.some((t) => t.kind === 'project' && !t.unknown) ? smart.project_id : task.project_id;
	return { smart, projects: ps, sections: cs, scope };
}

/** What the parts read from the title change on the task. */
export function smartPatch(smart: QuickAdd, task: Task): TaskPatch {
	const kinds = new Set(smart.tokens.filter((t) => !t.unknown).map((t) => t.kind));
	const patch: TaskPatch = {};
	if (kinds.has('date')) {
		patch.due_date = smart.due_date;
		patch.due_time = smart.due_time;
	}
	if (kinds.has('recurrence')) {
		patch.recurrence = smart.recurrence;
		// A repeat with no date in the line starts from its first day after the current date.
		if (!kinds.has('date') && smart.due_date !== task.due_date) patch.due_date = smart.due_date;
	}
	if (kinds.has('priority')) patch.priority = smart.priority ?? 4;
	if (!task.parent_id && (kinds.has('project') || kinds.has('section'))) {
		patch.project_id = smart.project_id;
		patch.section_id = smart.section_id;
	}
	if (kinds.has('reminder')) patch.remind_at = smart.remind_at;
	if (kinds.has('duration')) patch.duration_minutes = smart.duration_minutes;
	return patch;
}

/**
 * A task's title edited as a smart line, worked out once for every editor (the task
 * page and the in-list editor), so what they preview is what they save.
 * - `fields`: what the line sets, already settled (a time past today moves to tomorrow).
 * - `next`: the task as it will be once saved.
 * - `patch`: the whole change to save, title included; null when nothing changes or
 *   no title would be left.
 */
export function smartEdit(text: string, task: Task, projects: TaskProject[], sections: TaskSection[]) {
	const st = smartTitle(text, task, projects, sections);
	const fields = settlePatch(task, smartPatch(st.smart, task));
	// A line that only sets fields ("tomorrow p1") keeps the title.
	const title = st.smart.title || (Object.keys(fields).length ? task.title : '');
	let patch: TaskPatch | null = null;
	if (title) {
		const p: TaskPatch = { ...fields, ...(title !== task.title ? { title } : {}) };
		if (Object.keys(p).length) patch = p;
	}
	return { ...st, fields, patch, next: { ...task, ...fields } as Task };
}

export type SmartChip = { kind: string; label: string; priority?: number; color?: string };

/** The chips that show what a smart title will set. */
export function patchChips(p: TaskPatch, projects: TaskProject[], sections: TaskSection[]): SmartChip[] {
	const chips: SmartChip[] = [];
	if ('due_date' in p && p.due_date) {
		chips.push({ kind: 'date', label: pickedDayLabel(p.due_date) + (p.due_time ? ` ${formatTime(p.due_time)}` : '') });
	} else if ('due_date' in p) {
		chips.push({ kind: 'date', label: 'No date' });
	}
	if (p.duration_minutes) chips.push({ kind: 'duration', label: `for ${formatDuration(p.duration_minutes)}` });
	else if ('duration_minutes' in p) chips.push({ kind: 'duration', label: 'No duration' });
	if (p.recurrence) chips.push({ kind: 'recurrence', label: `↻ ${p.recurrence}` });
	if (p.remind_at) {
		chips.push({ kind: 'reminder', label: `Remind ${formatInstant(p.remind_at)}` });
	}
	if (p.priority) chips.push({ kind: 'priority', label: `P${p.priority}`, priority: p.priority });
	if ('project_id' in p) {
		const proj = projects.find((x) => x.id === p.project_id);
		const cat = sections.find((x) => x.id === p.section_id);
		chips.push({ kind: 'project', label: (proj?.name ?? 'Inbox') + (cat ? ` / ${cat.name}` : ''), color: proj ? projectColor(proj.color) : undefined });
	}
	return chips;
}

/** When the task is due, as a timestamp; a date with no time counts from 10:00 (as `localInstant` does). */
export function dueAtOf(task: Pick<Task, 'due_date' | 'due_time'>): number | null {
	return task.due_date ? new Date(localInstant(task.due_date, task.due_time)).getTime() : null;
}

/**
 * A patch as it should be saved. A new time for a task due today or earlier, without
 * picking another day, moves it to when that time next comes: today if still ahead,
 * else tomorrow. Due today at 18:00, set to 10:00 at 19:00: tomorrow 10:00. Future
 * dates keep their day. The reminder is left to tasks-core: a task given a date
 * gets its default reminder there, and a reminder moves with its due moment.
 */
export function settlePatch(task: Task, patch: TaskPatch): TaskPatch {
	if (patch.due_time && patch.due_time !== task.due_time && task.due_date && (patch.due_date ?? task.due_date) === task.due_date) {
		const now = new Date();
		if (task.due_date <= ymd(now)) {
			const day = nextDayFor(patch.due_time, now);
			if (day !== task.due_date) patch = { ...patch, due_date: day };
		}
	}
	return patch;
}
