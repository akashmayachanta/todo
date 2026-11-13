<script lang="ts">
	import PlaceMark from '$lib/components/tasks/PlaceMark.svelte';
	import { formatInstantLong } from '$lib/datePrefs.svelte';
	import type { Snippet } from 'svelte';
	import type { Task, TaskRowField } from '$lib/api';
	import DropdownMenu, { type MenuItem } from '$lib/components/DropdownMenu.svelte';
	import { plainText } from '$lib/links';
	import { sortHold, type SortApi } from '$lib/sortable';
	import { describeDue, formatDuration } from '$lib/taskParse';
	import LinkedText from './LinkedText.svelte';
	import TaskGrip from './TaskGrip.svelte';
	import TaskRow from './TaskRow.svelte';
	import { subtaskMinutes } from './shared';
	import { collapsedSubtasks, toggleSubtasks } from './collapsed';

	/**
	 * One task: a round check ringed in its priority colour, the title, and a line of what
	 * else it has (due, reminder, recurrence, subtasks done/total, location, where it lives).
	 * With `nested`, its open subtasks are listed under it (its done ones too with
	 * `showDoneSubtasks`). A click opens it; Alt+click (or Edit in its "⋯" menu) edits it in place,
	 * where the page's `editor` takes the row's place. With `sort`, its grip moves it, or by touch, holding it. `compact`
	 * is for a pinned window: no edit or delete, and without them it can leave out `ondelete`.
	 * `parent` names a subtask's task where the subtask isn't listed under it.
	 * `subtaskFilter` narrows the subtasks listed under it (Today).
	 */
	let {
		task, parent = null, subtasksOf, nested = false, subtaskFilter = null, showDoneSubtasks = false, showNote = true, place = null, fields = null, now,
		editingId = null, editor, sort = null, compact = false, ontoggle, onopen, onedit, onmove = null, onduplicate = null, onpin = null, ondelete,
		selectedIds = null, onselect = null, ondragstart = null, ondragend = null,
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
		/** Moves it to another project or category; offered for tasks, not subtasks. */
		onmove?: ((task: Task) => void) | null;
		onduplicate?: ((task: Task) => void) | null;
		/** Pins it to the desktop; null where there are no pins (Android). */
		onpin?: ((task: Task) => void) | null;
		ondelete?: (task: Task) => void;
		/** The tasks picked with Ctrl/Shift+click; a picked row is highlighted. */
		selectedIds?: ReadonlySet<string> | null;
		/** Ctrl (⌘) + click picks or unpicks it; Shift + click picks the rows up to it. */
		onselect?: ((task: Task, how: 'toggle' | 'range') => void) | null;
		/** A picked row drags by the mouse (HTML5), with the other picked ones, to a board column or the side pane. */
		ondragstart?: ((e: DragEvent, task: Task) => void) | null;
		ondragend?: (() => void) | null;
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
	let selected = $derived(!!selectedIds?.has(task.id));
	/** Only picked rows drag natively, so a plain press still selects text and follows links. */
	let nativeDrag = $derived(selected && !!ondragstart && !editing);
	/** The subtask count shows or hides the subtasks listed under it. */
	let canCollapse = $derived(nested && subtaskCount && shownSubtasks.length > 0);
	let collapsed = $derived(canCollapse && $collapsedSubtasks.includes(task.id));
	let doneCount = $derived(subtasks.length - openSubtasks.length);

	/** The "⋯" menu, which holds what the row used to show as icons. */
	let menuItems = $derived<MenuItem[]>([
		...(onedit ? [{ id: 'edit', label: 'Edit' }] : []),
		...(onmove && !task.parent_id ? [{ id: 'move', label: 'Move to…' }] : []),
		...(onduplicate ? [{ id: 'duplicate', label: 'Duplicate' }] : []),
		...(onpin ? [{ id: 'pin', label: 'Pin to desktop' }] : []),
		{ id: 'delete', label: 'Delete', danger: true },
	]);

	function onMenu(id: string) {
		if (id === 'edit') onedit?.(task);
		else if (id === 'move') onmove?.(task);
		else if (id === 'duplicate') onduplicate?.(task);
		else if (id === 'pin') onpin?.(task);
		else ondelete?.(task);
	}

	function onMainClick(e: MouseEvent) {
		if (onselect && (e.ctrlKey || e.metaKey || e.shiftKey)) {
			// A link in the title is clicked through to the row, not followed.
			e.preventDefault();
			onselect(task, e.shiftKey ? 'range' : 'toggle');
			return;
		}
		if (e.altKey && onedit) {
			e.preventDefault();
			onedit(task);
			return;
		}
		onopen(task);
	}
</script>

<!-- svelte-ignore a11y_no_static_element_interactions -->
<div
	class="task-item"
	data-flip={task.id}
	data-select-id={onselect ? task.id : undefined}
	draggable={nativeDrag || undefined}
	ondragstart={nativeDrag ? (e) => { if (e.target === e.currentTarget) { e.stopPropagation(); ondragstart?.(e, task); } } : undefined}
	ondragend={nativeDrag ? () => ondragend?.() : undefined}
	data-sort-id={sort ? task.id : undefined}
	data-sort-kind={sort ? (task.parent_id ? 'sub' : 'top') : undefined}
	use:sortHold={sort && !editing ? { id: task.id, kind: task.parent_id ? 'sub' : 'top', ondrop: sort.ondrop, onstep: sort.onstep } : null}
>
	{#if editing && editor}
		{@render editor(task)}
	{:else}
		<div class="task-row" class:task-row-done={done} class:task-row-compact={compact} class:task-row-selected={selected}>
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
							<span class="task-due" class:task-due-overdue={due.overdue && !done} class:task-due-today={due.today && !due.overdue} class:task-due-tomorrow={due.tomorrow && !done} class:task-due-week={due.thisWeek && !done} class:task-due-progress={due.inProgress && !done}>
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
							<span class="task-meta-icon" title="Reminder {formatInstantLong(reminder)}">
								<svg aria-label="Has a reminder" class="task-meta-svg" fill="none" stroke="currentColor" viewBox="0 0 24 24"><path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M12 8v4l2.5 2.5M5 3L2 6m20 0l-3-3M12 21a8 8 0 100-16 8 8 0 000 16z" /></svg>
							</span>
						{/if}
						{#if subtaskCount && canCollapse}
							<button
								type="button"
								class="task-subtoggle"
								aria-expanded={!collapsed}
								title="{collapsed ? 'Show' : 'Hide'} sub-tasks, {doneCount} of {subtasks.length} completed"
								aria-label="{collapsed ? 'Show' : 'Hide'} sub-tasks of {name}, {doneCount} of {subtasks.length} completed"
								onclick={(e) => { e.stopPropagation(); toggleSubtasks(task.id); }}
							>
								<svg aria-hidden="true" class="task-meta-svg" fill="none" stroke="currentColor" viewBox="0 0 24 24"><path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M6 3v12a3 3 0 003 3h9m0 0l-3-3m3 3l-3 3" /></svg>
								{doneCount}/{subtasks.length}
								<svg aria-hidden="true" class="task-subtoggle-chev" fill="none" stroke="currentColor" viewBox="0 0 24 24"><path stroke-linecap="round" stroke-linejoin="round" stroke-width="2.5" d="M6 9l6 6 6-6" /></svg>
							</button>
						{:else if subtaskCount}
							<span class="task-meta-icon" title="Subtasks done">
								<svg aria-hidden="true" class="task-meta-svg" fill="none" stroke="currentColor" viewBox="0 0 24 24"><path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M9 6h11M9 12h11M9 18h11M4.5 6h.01M4.5 12h.01M4.5 18h.01" /></svg>
								{doneCount}/{subtasks.length}
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
								{#if place.color}<PlaceMark color={place.color} />{/if}
								{place.name}
							</span>
						{/if}
					</span>
				{/if}
			</div>

			{#if !compact}
				<DropdownMenu
					class="task-actions"
					triggerClass="tasks-icon-btn"
					ariaLabel="Actions for {name}"
					items={menuItems}
					onselect={onMenu}
				>
					{#snippet icon()}
						<svg aria-hidden="true" class="w-3.5 h-3.5" fill="currentColor" viewBox="0 0 20 20"><path d="M6 10a2 2 0 11-4 0 2 2 0 014 0zm6 0a2 2 0 11-4 0 2 2 0 014 0zm6 0a2 2 0 11-4 0 2 2 0 014 0z" /></svg>
					{/snippet}
				</DropdownMenu>
			{/if}
		</div>
	{/if}

	{#if nested && shownSubtasks.length && !collapsed}
		<div class="task-subtasks">
			{#each shownSubtasks as sub (sub.id)}
				<TaskRow task={sub} {subtasksOf} {showNote} {fields} {now} {editingId} {editor} {sort} {compact} {ontoggle} {onopen} {onedit} {onmove} {onduplicate} {onpin} {ondelete} {selectedIds} {onselect} />
			{/each}
		</div>
	{/if}
</div>
