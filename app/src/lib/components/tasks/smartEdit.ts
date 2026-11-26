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
	const shown = previewPatch(task, fields);
	return { ...st, fields, patch, shown, next: { ...task, ...shown } as Task };
}

// ── What tasks-core does on save ─────────────────────────────────────────────
// Every editor previews through these, so its chips show what the save will do.
// They mirror core/src/reminder.rs and ops.rs; change them together.

/**
 * A change to an existing task as tasks-core will save it: the patch, plus the
 * reminder that moves with a new due date or time. Use it for any preview of an edit.
 */
export function previewPatch(task: Task, patch: TaskPatch): TaskPatch {
	if ('remind_at' in patch) return patch;
	const remind_at = reminderAfterDueChange(task, patch);
	return remind_at === task.remind_at ? patch : { ...patch, remind_at };
}

/** The reminder tasks-core gives a new task that doesn't name one: at its due moment, if still ahead. */
export function defaultReminder(due_date: string | null | undefined, due_time: string | null | undefined): string | null {
	if (!due_date) return null;
	const at = dueAtOf({ due_date, due_time: due_time ?? null })!;
	return at > Date.now() ? new Date(at).toISOString() : null;
}

/**
 * The reminder tasks-core will give a task once this change is saved, when the change
 * doesn't set one itself (core/src/reminder.rs, ops.rs `update_task`): a new due date
 * or time moves the reminder so it keeps its lead, or brings the default one at the
 * due moment if that is still ahead; taking the date off takes the reminder off.
 */
export function reminderAfterDueChange(task: Task, fields: TaskPatch): string | null {
	if (!('due_date' in fields) && !('due_time' in fields)) return task.remind_at;
	const date = 'due_date' in fields ? (fields.due_date ?? null) : task.due_date;
	if (!date) return null;
	const time = 'due_time' in fields ? (fields.due_time ?? null) : task.due_time;
	const to = dueAtOf({ due_date: date, due_time: time })!;
	if (task.remind_at) {
		const from = dueAtOf(task);
		const at = new Date(task.remind_at).getTime();
		if (from === null || Number.isNaN(at)) return task.remind_at;
		return new Date(to - (from - at)).toISOString();
	}
	const sets = !!fields.due_date || !!fields.due_time;
	return sets ? defaultReminder(date, time) : null;
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
	} else if ('remind_at' in p) {
		chips.push({ kind: 'reminder', label: 'No reminder' });
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
