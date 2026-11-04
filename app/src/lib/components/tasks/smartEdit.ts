import type { Task, TaskCategory, TaskPatch, TaskProject } from '$lib/api';
import { askReminderPermission } from '$lib/stores/taskReminders';
import { dayLabel, formatDuration, formatTime, localInstant, parseEdit, type QuickAdd } from '$lib/taskParse';
import { defaultReminder, projectColor } from './shared';

/**
 * Editing a task's title as a smart line, shared by the task modal and the inline
 * editor: "tomorrow 5pm", "p1", "#Project", "/Category", "every monday", "!30m"
 * typed into the title set those fields. Only what is typed is read; a subtask's
 * place follows its parent, so `#` and `/` mean nothing in one.
 */
export function smartTitle(text: string, task: Task, projects: TaskProject[], categories: TaskCategory[]) {
	const ps = task.parent_id ? [] : projects;
	const cs = task.parent_id ? [] : categories;
	const smart = parseEdit(text, task.title, ps, cs, new Date(), task.project_id, task.due_date);
	const scope = smart.tokens.some((t) => t.kind === 'project' && !t.unknown) ? smart.project_id : task.project_id;
	return { smart, projects: ps, categories: cs, scope };
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
	if (!task.parent_id && (kinds.has('project') || kinds.has('category'))) {
		patch.project_id = smart.project_id;
		patch.category_id = smart.category_id;
	}
	if (kinds.has('reminder')) patch.remind_at = smart.remind_at;
	if (kinds.has('duration')) patch.duration_minutes = smart.duration_minutes;
	return patch;
}

/**
 * The whole change a smart title makes: its fields and the title left once they are
 * taken out. null when nothing changes or no title would be left.
 */
export function titleEdit(smart: QuickAdd, task: Task): TaskPatch | null {
	const patch = smartPatch(smart, task);
	// A line that only sets fields ("tomorrow p1") keeps the title.
	const title = smart.title || (Object.keys(patch).length ? task.title : '');
	if (!title) return null;
	if (title !== task.title) patch.title = title;
	return Object.keys(patch).length ? patch : null;
}

export type SmartChip = { kind: string; label: string; priority?: number; color?: string };

/** The chips that show what a smart title will set. */
export function patchChips(p: TaskPatch, projects: TaskProject[], categories: TaskCategory[]): SmartChip[] {
	const chips: SmartChip[] = [];
	if ('due_date' in p && p.due_date) {
		chips.push({ kind: 'date', label: dayLabel(p.due_date) + (p.due_time ? ` ${formatTime(p.due_time)}` : '') });
	}
	if (p.duration_minutes) chips.push({ kind: 'duration', label: `for ${formatDuration(p.duration_minutes)}` });
	if (p.recurrence) chips.push({ kind: 'recurrence', label: `↻ ${p.recurrence}` });
	if (p.remind_at) {
		chips.push({ kind: 'reminder', label: `Remind ${new Date(p.remind_at).toLocaleString(undefined, { day: 'numeric', month: 'short', hour: 'numeric', minute: '2-digit' })}` });
	}
	if (p.priority) chips.push({ kind: 'priority', label: `P${p.priority}`, priority: p.priority });
	if ('project_id' in p) {
		const proj = projects.find((x) => x.id === p.project_id);
		const cat = categories.find((x) => x.id === p.category_id);
		chips.push({ kind: 'project', label: (proj?.name ?? 'Inbox') + (cat ? ` / ${cat.name}` : ''), color: proj ? projectColor(proj.color) : undefined });
	}
	return chips;
}

/** When the task is due, as a timestamp; a date with no time counts from 9:00 (as `localInstant` does). */
export function dueAtOf(task: Pick<Task, 'due_date' | 'due_time'>): number | null {
	return task.due_date ? new Date(localInstant(task.due_date, task.due_time)).getTime() : null;
}

/**
 * A patch as it should be saved: a task getting its first due time, with no reminder
 * yet, gets one at that time; a change of date or time moves an existing reminder
 * with it, the same time before.
 */
export function finishPatch(task: Task, patch: TaskPatch): TaskPatch {
	if (patch.due_time && !task.due_time && !task.remind_at && !('remind_at' in patch)) {
		const remind_at = defaultReminder(patch.due_date ?? task.due_date, patch.due_time);
		if (remind_at) {
			askReminderPermission();
			return { ...patch, remind_at };
		}
		return patch;
	}
	const dueAt = dueAtOf(task);
	if (!task.remind_at || 'remind_at' in patch || dueAt === null) return patch;
	if (!('due_date' in patch) && !('due_time' in patch)) return patch;
	const date = 'due_date' in patch ? patch.due_date : task.due_date;
	if (!date) return patch;
	const time = 'due_time' in patch ? patch.due_time ?? null : task.due_time;
	const before = dueAt - new Date(task.remind_at).getTime();
	return { ...patch, remind_at: new Date(new Date(localInstant(date, time)).getTime() - before).toISOString() };
}
