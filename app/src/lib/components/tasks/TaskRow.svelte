<script lang="ts">
	import type { Snippet } from 'svelte';
	import type { Task, TaskRowField } from '$lib/api';
	import { plainText } from '$lib/links';
	import type { SortApi } from '$lib/sortable';
	import { describeDue, formatDuration } from '$lib/taskParse';
	import LinkedText from './LinkedText.svelte';
	import TaskGrip from './TaskGrip.svelte';
	import TaskRow from './TaskRow.svelte';
	import { subtaskMinutes } from './shared';

	/**
	 * One task: a round check ringed in its priority colour, the title, and a line of what
	 * else it has (due, reminder, recurrence, subtasks done/total, location, where it lives).
	 * With `nested`, its open subtasks are listed under it (its done ones too with
	 * `showDoneSubtasks`). A click opens it; Alt+click (or the pencil) edits it in place,
	 * where the page's `editor` takes the row's place. With `sort`, a grip moves it. `compact`
	 * is for a pinned window: no edit or delete, and without them it can leave out `ondelete`.
	 * `parent` names a subtask's task where the subtask isn't listed under it.
	 * `subtaskFilter` narrows the subtasks listed under it (Today).
	 */
	let {
		task, parent = null, subtasksOf, nested = false, subtaskFilter = null, showDoneSubtasks = false, showNote = true, place = null, fields = null, now,
		editingId = null, editor, sort = null, compact = false, ontoggle, onopen, onedit, onpin = null, ondelete,
	}: {
		task: Task;
		/** The task this subtask belongs to, when the list doesn't already show it. */
		parent?: Task | null;
		/** Every subtask of a task, done ones too. */
		subtasksOf: (id: string) => Task[];
		nested?: boolean;
		subtaskFilter?: ((parent: Task, subtask: Task) => boolean) | null;
		showDoneSubtasks?: boolean;
		/** The first line of the description under the title. */
		showNote?: boolean;
		/** Where it lives, shown in the date views; color is a CSS colour, null for the Inbox. */
		place?: { name: string; color: string | null } | null;
		/** The details shown (a filter's choice); null shows them all, with the note as showNote says. */
		fields?: ReadonlySet<TaskRowField> | null;
		now: Date;
		/** The task being edited in place, if any. */
		editingId?: string | null;
		editor?: Snippet<[Task]>;
		/** Rearranging by the grip; it passes down to the subtasks listed under it. */
		sort?: SortApi | null;
		compact?: boolean;
		ontoggle: (task: Task) => void;
		onopen: (task: Task) => void;
		onedit?: (task: Task) => void;
		/** Pins it to the desktop; null where there are no pins (Android). */
		onpin?: ((task: Task) => void) | null;
		ondelete?: (task: Task) => void;
	} = $props();

	let subtasks = $derived(task.parent_id === null ? subtasksOf(task.id) : []);
	let openSubtasks = $derived(subtasks.filter((s) => !s.completed_at));
	const shows = (f: TaskRowField) => !fields || fields.has(f);
	let due = $derived(shows('due') ? describeDue(task, now) : null);
	let done = $derived(!!task.completed_at);
	let name = $derived(plainText(task.title));
	let parentName = $derived(parent ? plainText(parent.title) : '');
	let noteShown = $derived(fields ? fields.has('description') : showNote);
	let note = $derived(noteShown ? task.description?.split('\n').find((l) => l.trim()) ?? '' : '');
	/** Its duration (what's left of its subtasks' when they have them); left out when the due time already shows it as a range. */
	let rolledUp = $derived(subtaskMinutes(subtasks) !== null);
	let duration = $derived(shows('duration') && task.duration_minutes && !(due && task.due_time) ? task.duration_minutes : null);
	let repeat = $derived(shows('repeat') ? task.recurrence : null);
	let reminder = $derived(shows('reminder') ? task.remind_at : null);
	let subtaskCount = $derived(shows('subtasks') && subtasks.length > 0);
	let shownSubtasks = $derived(
		(showDoneSubtasks ? subtasks : openSubtasks).filter((s) => !subtaskFilter || subtaskFilter(task, s)),
	);
	let editing = $derived(!!editor && editingId === task.id);

	function onMainClick(e: MouseEvent) {
		if (e.altKey && onedit) {
			e.preventDefault();
			onedit(task);
			return;
		}
		onopen(task);
	}
</script>

<div class="task-item" data-sort-id={sort ? task.id : undefined} data-sort-kind={sort ? (task.parent_id ? 'sub' : 'top') : undefined}>
	{#if editing && editor}
		{@render editor(task)}
	{:else}
		<div class="task-row" class:task-row-done={done} class:task-row-compact={compact}>
			{#if sort}<TaskGrip id={task.id} kind={task.parent_id ? 'sub' : 'top'} {name} {sort} />{/if}
			<button
				type="button"
				class="task-check"
				data-priority={task.priority}
				aria-pressed={done}
				aria-label={done ? `Mark "${name}" not done` : task.recurrence ? `Complete "${name}" (moves to its next date)` : `Complete "${name}"`}
				onclick={() => ontoggle(task)}
			>
				<svg aria-hidden="true" class="task-check-tick" fill="none" stroke="currentColor" viewBox="0 0 24 24"><path stroke-linecap="round" stroke-linejoin="round" stroke-width="3" d="M5 13l4 4L19 7" /></svg>
			</button>

			<!-- Links sit inside, so this can't be a button: the pointer clicks the row, the keyboard the button over it. -->
			<!-- svelte-ignore a11y_click_events_have_key_events, a11y_no_static_element_interactions -->
			<div class="task-main" onclick={onMainClick} title={onedit ? 'Click to open · Alt+click to edit here' : undefined}>
				<button type="button" class="task-open" aria-label="Open {name}"></button>
				<!-- A subtask stays on one line; the task itself shows its full title when opened. -->
				<span class="task-title" class:task-title-one={!!task.parent_id} title={task.parent_id ? name : undefined}><LinkedText text={task.title} /></span>
				{#if parentName}<span class="task-parent" title={parentName}>Subtask of {parentName}</span>{/if}
				{#if note}<span class="task-note"><LinkedText text={note} /></span>{/if}
				{#if due || reminder || repeat || duration || subtaskCount || task.location || place}
					<span class="task-meta">
						{#if due}
							<span class="task-due" class:task-due-overdue={due.overdue && !done} class:task-due-today={due.today && !due.overdue} class:task-due-progress={due.inProgress && !done}>
								<svg aria-hidden="true" class="task-meta-svg" fill="none" stroke="currentColor" viewBox="0 0 24 24"><path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M8 7V3m8 4V3m-9 8h10M5 21h14a2 2 0 002-2V7a2 2 0 00-2-2H5a2 2 0 00-2 2v12a2 2 0 002 2z" /></svg>
								{due.label}
							</span>
						{/if}
						{#if duration}
							<span class="task-meta-icon" title={rolledUp ? `${formatDuration(duration)} left in its open subtasks` : `Takes ${formatDuration(duration)}`}>{formatDuration(duration)}</span>
						{/if}
						{#if repeat}
							<span class="task-meta-icon" title={repeat} aria-label="Repeats {repeat}">↻</span>
						{/if}
						{#if reminder}
							<span class="task-meta-icon" title="Reminder {new Date(reminder).toLocaleString()}">
								<svg aria-label="Has a reminder" class="task-meta-svg" fill="none" stroke="currentColor" viewBox="0 0 24 24"><path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M12 8v4l2.5 2.5M5 3L2 6m20 0l-3-3M12 21a8 8 0 100-16 8 8 0 000 16z" /></svg>
							</span>
						{/if}
						{#if subtaskCount}
							<span class="task-meta-icon" title="Subtasks done">
								<svg aria-hidden="true" class="task-meta-svg" fill="none" stroke="currentColor" viewBox="0 0 24 24"><path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M9 6h11M9 12h11M9 18h11M4.5 6h.01M4.5 12h.01M4.5 18h.01" /></svg>
								{subtasks.length - openSubtasks.length}/{subtasks.length}
							</span>
						{/if}
						{#if task.location}
							<span class="task-meta-icon task-meta-location" title={task.location}>
								<svg aria-label="Location" class="task-meta-svg" fill="none" stroke="currentColor" viewBox="0 0 24 24"><path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M12 21s-7-6.1-7-11.5a7 7 0 1114 0C19 14.9 12 21 12 21z" /><circle cx="12" cy="9.5" r="2.5" stroke-width="2" /></svg>
								<span class="truncate">{task.location}</span>
							</span>
						{/if}
						{#if place}
							<span class="task-place">
								{#if place.color}<span class="tasks-dot" style="background-color: {place.color}"></span>{/if}
								{place.name}
							</span>
						{/if}
					</span>
				{/if}
			</div>

			{#if !compact}
			<div class="task-actions">
				{#if onedit}
					<button type="button" class="tasks-icon-btn" onclick={() => onedit(task)} title="Edit here (Alt+click)" aria-label="Edit {name}">
						<svg aria-hidden="true" class="w-3.5 h-3.5" fill="none" stroke="currentColor" viewBox="0 0 24 24"><path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M11 5H6a2 2 0 00-2 2v11a2 2 0 002 2h11a2 2 0 002-2v-5m-1.414-9.414a2 2 0 112.828 2.828L11.828 15H9v-2.828l8.586-8.586z" /></svg>
					</button>
				{/if}
				{#if onpin}
					<button type="button" class="tasks-icon-btn" onclick={() => onpin(task)} title="Pin to desktop" aria-label="Pin {name} to desktop">
						<svg aria-hidden="true" class="w-3.5 h-3.5" fill="none" stroke="currentColor" viewBox="0 0 24 24"><path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M12 17v5M7 17h10l-1.5-4.5V6a2 2 0 00-2-2h-3a2 2 0 00-2 2v6.5L7 17z" /></svg>
					</button>
				{/if}
				<button type="button" class="tasks-icon-btn tasks-icon-btn-danger" onclick={() => ondelete?.(task)} title="Delete" aria-label="Delete {name}">
					<svg aria-hidden="true" class="w-3.5 h-3.5" fill="none" stroke="currentColor" viewBox="0 0 24 24"><path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M19 7l-.867 12.142A2 2 0 0116.138 21H7.862a2 2 0 01-1.995-1.858L5 7m5 4v6m4-6v6m1-10V4a1 1 0 00-1-1h-4a1 1 0 00-1 1v3M4 7h16" /></svg>
				</button>
			</div>
			{/if}
		</div>
	{/if}

	{#if nested && shownSubtasks.length}
		<div class="task-subtasks">
			{#each shownSubtasks as sub (sub.id)}
				<TaskRow task={sub} {subtasksOf} {showNote} {fields} {now} {editingId} {editor} {sort} {compact} {ontoggle} {onopen} {onedit} {onpin} {ondelete} />
			{/each}
		</div>
	{/if}
</div>
