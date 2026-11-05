<script lang="ts">
	import { onMount, tick } from 'svelte';
	import type { Task, TaskCategory, TaskIn, TaskPatch, TaskPriority, TaskProject } from '$lib/api';
	import DatePicker from '$lib/components/DatePicker.svelte';
	import DropdownMenu from '$lib/components/DropdownMenu.svelte';
	import Select from '$lib/components/Select.svelte';
	import { askReminderPermission } from '$lib/stores/taskReminders';
	import { linkParts, plainText } from '$lib/links';
	import { firstOccurrence, formatDuration, normalizeRecurrence, parseQuickAdd, pastedLines, ymd, type QuickAdd } from '$lib/taskParse';
	import LinkedText from './LinkedText.svelte';
	import SmartTaskInput from './SmartTaskInput.svelte';
	import SmartTips from './SmartTips.svelte';
	import { dueAtOf, finishPatch, patchChips, smartPatch, smartTitle, titleEdit } from './smartEdit';
	import { PRIORITY_OPTIONS, defaultReminder, projectColor } from './shared';

	/**
	 * One task in a modal: the title, description and subtasks on the left, its fields in a
	 * side column on the right; every change is saved as it is made. The title and the
	 * subtask line are smart, like the quick add: "tomorrow 5pm", "p1", "#Project",
	 * "/Category", "every monday", "!30m" typed into them set those fields. The arrows in
	 * the head (or k / j) step to the previous and next task of the view.
	 */
	let { task, parent = null, subtasks, projects, categories, onprev = null, onnext = null, onpatch, ontoggle, onaddsubtask, onopen, ondelete, onclose }: {
		task: Task;
		/** The task this is a subtask of. */
		parent?: Task | null;
		subtasks: Task[];
		projects: TaskProject[];
		categories: TaskCategory[];
		/** Step to the previous / next task of the view; null at either end. */
		onprev?: (() => void) | null;
		onnext?: (() => void) | null;
		onpatch: (task: Task, patch: TaskPatch) => Promise<void>;
		ontoggle: (task: Task) => void;
		onaddsubtask: (parent: Task, data: Omit<TaskIn, 'parent_id' | 'project_id' | 'category_id'>) => Promise<void>;
		onopen: (task: Task) => void;
		ondelete: (task: Task) => void;
		onclose: () => void;
	} = $props();

	let title = $state('');
	let description = $state('');
	let recurrenceText = $state('');
	let recurrenceError = $state('');
	let locationText = $state('');
	let newSubtask = $state('');
	let addingSubtask = $state(false);
	let panel = $state<HTMLDivElement | null>(null);
	let subField = $state<SmartTaskInput | null>(null);

	// Each saved version of the task resets the fields to it.
	$effect.pre(() => {
		title = task.title;
		description = task.description ?? '';
		recurrenceText = task.recurrence ?? '';
		recurrenceError = '';
		locationText = task.location ?? '';
	});

	// Stepping to another task closes its subtask line.
	$effect.pre(() => {
		void task.id;
		addingSubtask = false;
		newSubtask = '';
	});

	let project = $derived(projects.find((p) => p.id === task.project_id) ?? null);
	let category = $derived(categories.find((c) => c.id === task.category_id) ?? null);

	let placeOptions = $derived([
		{ value: 'inbox', label: 'Inbox' },
		...projects.flatMap((p) => [
			{ value: `p${p.id}`, label: p.name },
			...categories.filter((c) => c.project_id === p.id).map((c) => ({ value: `c${c.id}`, label: `${p.name} / ${c.name}` })),
		]),
	]);
	let placeValue = $derived(task.category_id ? `c${task.category_id}` : task.project_id ? `p${task.project_id}` : 'inbox');

	function save(patch: TaskPatch) {
		return onpatch(task, finishPatch(task, patch));
	}

	// ── Duration ─────────────────────────────────────────────────────────────
	const DURATION_MINUTES = [15, 30, 45, 60, 90, 120, 180, 240, 480];
	let durationValue = $derived(task.duration_minutes ? String(task.duration_minutes) : 'none');
	let durationOptions = $derived([
		{ value: 'none', label: 'None' },
		...[...new Set([...DURATION_MINUTES, ...(task.duration_minutes ? [task.duration_minutes] : [])])]
			.sort((a, b) => a - b)
			.map((m) => ({ value: String(m), label: formatDuration(m) })),
	]);

	function saveDuration(value: string) {
		const minutes = value === 'none' ? null : Number(value);
		if (minutes !== task.duration_minutes) save({ duration_minutes: minutes });
	}

	// ── Reminder: a set time before the due time ──────────────────────────────
	// A task with a date but no time counts from 9:00 (as `localInstant` does).
	const REMINDER_MINUTES = [0, 5, 10, 15, 30, 60, 120, 1440];
	const reminderLabel = (m: number) =>
		m === 0 ? 'At the due time'
			: m < 60 ? `${m} minutes before`
			: m < 1440 ? `${m / 60} hour${m === 60 ? '' : 's'} before`
			: '1 day before';

	let dueAt = $derived(dueAtOf(task));
	let reminderValue = $derived.by(() => {
		if (!task.remind_at) return 'none';
		if (dueAt === null) return 'other';
		const before = (dueAt - new Date(task.remind_at).getTime()) / 60_000;
		return REMINDER_MINUTES.includes(before) ? String(before) : 'other';
	});
	let reminderOptions = $derived([
		{ value: 'none', label: 'No reminder' },
		...(dueAt === null ? [] : REMINDER_MINUTES.map((m) => ({ value: String(m), label: reminderLabel(m) }))),
		// A reminder set another way (e.g. "!5pm" in the quick add) shows as its time.
		...(reminderValue === 'other'
			? [{ value: 'other', label: new Date(task.remind_at!).toLocaleString(undefined, { day: 'numeric', month: 'short', hour: 'numeric', minute: '2-digit' }) }]
			: []),
	]);

	function saveReminder(value: string) {
		if (value === 'other' || value === reminderValue) return;
		if (value === 'none' || dueAt === null) {
			save({ remind_at: null });
			return;
		}
		askReminderPermission();
		save({ remind_at: new Date(dueAt - Number(value) * 60_000).toISOString() });
	}

	// ── Smart title ──────────────────────────────────────────────────────────
	let st = $derived(smartTitle(title, task, projects, categories));
	let titleChips = $derived(patchChips(smartPatch(st.smart, task), projects, categories));

	function saveTitle() {
		const patch = titleEdit(st.smart, task);
		if (!patch) {
			title = task.title;
			return;
		}
		if (patch.remind_at) askReminderPermission();
		title = patch.title ?? task.title;
		save(patch);
	}

	function blurActive() {
		(document.activeElement as HTMLElement | null)?.blur();
	}

	// ── Other fields ─────────────────────────────────────────────────────────
	function saveDescription() {
		const d = description.trim() || null;
		if (d !== (task.description ?? null)) save({ description: d });
	}

	function savePlace(value: string) {
		if (value === 'inbox') save({ project_id: null, category_id: null });
		else if (value.startsWith('p')) save({ project_id: value.slice(1), category_id: null });
		else {
			const c = categories.find((x) => x.id === value.slice(1));
			if (c) save({ project_id: c.project_id, category_id: c.id });
		}
	}

	function saveDate(value: string) {
		save(value ? { due_date: value } : { due_date: null, due_time: null });
	}

	/** A time on a task with no date puts it on today. */
	function saveTime(value: string) {
		if (!value) {
			if (task.due_time) save({ due_time: null });
			return;
		}
		if (task.due_date) {
			if (value !== task.due_time) save({ due_time: value });
			return;
		}
		save({ due_date: ymd(new Date()), due_time: value });
	}

	function saveRecurrence() {
		const text = recurrenceText.trim();
		if (!text) {
			recurrenceError = '';
			if (task.recurrence) save({ recurrence: null });
			return;
		}
		const recurrence = normalizeRecurrence(text);
		if (!recurrence) {
			recurrenceError = 'Try "every day", "every weekday", "every mon, thu" or "every 2 weeks".';
			return;
		}
		recurrenceError = '';
		recurrenceText = recurrence;
		if (recurrence === task.recurrence) return;
		// A repeating task needs a date to repeat from.
		save({ recurrence, ...(task.due_date ? {} : { due_date: firstOccurrence(recurrence) }) });
	}

	function saveLocation() {
		const loc = locationText.trim().slice(0, 500) || null;
		locationText = loc ?? '';
		if (loc !== (task.location ?? null)) save({ location: loc });
	}

	// ── Subtasks ─────────────────────────────────────────────────────────────
	const parseSubtask = (line: string) => parseQuickAdd(line, [], [], new Date(), null, task.due_date);
	let subParsed = $derived(parseSubtask(newSubtask));
	let subtasksDone = $derived(subtasks.filter((s) => s.completed_at).length);

	async function openSubtaskLine() {
		addingSubtask = true;
		await tick();
		subField?.focus();
	}

	async function createSubtask(p: QuickAdd, minutes: number | null = null) {
		if (p.remind_at) askReminderPermission();
		await onaddsubtask(task, {
			title: p.title,
			priority: p.priority ?? 4,
			due_date: p.due_date,
			due_time: p.due_date ? p.due_time : null,
			remind_at: p.remind_at ?? defaultReminder(p.due_date, p.due_date ? p.due_time : null),
			recurrence: p.recurrence,
			duration_minutes: minutes ?? p.duration_minutes,
		});
	}

	async function addSubtask() {
		if (!subParsed.title) return;
		await createSubtask(subParsed);
		newSubtask = '';
	}

	/** Several lines pasted into the sub-task line become one sub-task each, in order. */
	async function pasteSubtasks(e: ClipboardEvent) {
		const lines = pastedLines(e.clipboardData?.getData('text') ?? '');
		if (lines.length < 2) return;
		e.preventDefault();
		for (const line of lines) {
			const p = parseSubtask(line.text);
			if (p.title) await createSubtask(p, line.minutes);
		}
	}

	function closeSubtaskLine() {
		addingSubtask = false;
		newSubtask = '';
	}

	// ── Keys ─────────────────────────────────────────────────────────────────
	function typing(el: EventTarget | null): boolean {
		const node = el as HTMLElement | null;
		if (!node) return false;
		return node.isContentEditable || ['INPUT', 'TEXTAREA', 'SELECT'].includes(node.tagName) || !!node.closest('[role="listbox"], [role="menu"], [aria-haspopup]');
	}

	function onKeydown(e: KeyboardEvent) {
		if (e.defaultPrevented) return;
		if (e.key === 'Escape') {
			onclose();
			return;
		}
		if (e.ctrlKey || e.metaKey || e.altKey || typing(e.target)) return;
		if (e.key === 'k' && onprev) {
			e.preventDefault();
			onprev();
		} else if (e.key === 'j' && onnext) {
			e.preventDefault();
			onnext();
		}
	}

	function blurOnEnter(e: KeyboardEvent) {
		if (e.key === 'Enter') {
			e.preventDefault();
			(e.currentTarget as HTMLElement).blur();
		}
	}

	function onMenu(id: string) {
		if (id === 'delete') ondelete(task);
		else if (id === 'copy') {
			navigator.clipboard?.writeText(location.href).catch(() => { /* nothing to copy to */ });
		}
	}

	onMount(() => panel?.focus());

	// The title and description are edited as typed; their links open from a line under the title.
	let linkText = $derived(`${task.title}\n${task.description ?? ''}`);
	let hasLinks = $derived(linkParts(linkText).some((p) => p.url));
</script>

<svelte:window onkeydown={onKeydown} />

<div class="task-modal-scrim" onclick={onclose} aria-hidden="true"></div>
<div class="task-modal-overlay">
	<div bind:this={panel} class="task-modal" role="dialog" aria-modal="true" aria-label="Task: {plainText(task.title)}" tabindex="-1">
		<header class="task-modal-head">
			<nav class="task-modal-crumbs" aria-label="Where this task is">
				{#if parent}
					<button type="button" class="task-modal-crumb" onclick={() => onopen(parent)}>
						<svg aria-hidden="true" class="w-3.5 h-3.5 shrink-0" fill="none" stroke="currentColor" viewBox="0 0 24 24"><path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M15 19l-7-7 7-7" /></svg>
						<span class="truncate">{plainText(parent.title)}</span>
					</button>
				{:else}
					<span class="task-modal-crumb">
						{#if project}
							<span class="task-modal-hash" style="color: {projectColor(project.color)}">#</span>
							<span class="truncate">{project.name}</span>
						{:else}
							<svg aria-hidden="true" class="w-3.5 h-3.5 shrink-0" fill="none" stroke="currentColor" viewBox="0 0 24 24"><path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M20 13V7a2 2 0 00-2-2H6a2 2 0 00-2 2v6m16 0v4a2 2 0 01-2 2H6a2 2 0 01-2-2v-4m16 0h-4l-1 2h-6l-1-2H4" /></svg>
							<span>Inbox</span>
						{/if}
					</span>
					{#if category}
						<span class="task-modal-crumb-sep" aria-hidden="true">/</span>
						<span class="task-modal-crumb">
							<svg aria-hidden="true" class="w-3.5 h-3.5 shrink-0" fill="none" stroke="currentColor" viewBox="0 0 24 24"><rect x="5" y="4" width="14" height="16" rx="2" stroke-width="2" /><path stroke-linecap="round" stroke-width="2" d="M5 9h14" /></svg>
							<span class="truncate">{category.name}</span>
						</span>
					{/if}
				{/if}
			</nav>
			<div class="task-modal-head-actions">
				<button type="button" class="tasks-icon-btn" onclick={() => onprev?.()} disabled={!onprev} aria-label="Previous task" title="Previous task (k)">
					<svg aria-hidden="true" class="w-4 h-4" fill="none" stroke="currentColor" viewBox="0 0 24 24"><path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M5 15l7-7 7 7" /></svg>
				</button>
				<button type="button" class="tasks-icon-btn" onclick={() => onnext?.()} disabled={!onnext} aria-label="Next task" title="Next task (j)">
					<svg aria-hidden="true" class="w-4 h-4" fill="none" stroke="currentColor" viewBox="0 0 24 24"><path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M19 9l-7 7-7-7" /></svg>
				</button>
				<SmartTips />
				<DropdownMenu
					triggerClass="tasks-icon-btn"
					ariaLabel="Task actions"
					items={[{ id: 'copy', label: 'Copy link to task' }, { id: 'delete', label: 'Delete task', danger: true }]}
					onselect={onMenu}
				>
					{#snippet icon()}
						<svg aria-hidden="true" class="w-4 h-4" fill="currentColor" viewBox="0 0 20 20"><path d="M6 10a2 2 0 11-4 0 2 2 0 014 0zm6 0a2 2 0 11-4 0 2 2 0 014 0zm6 0a2 2 0 11-4 0 2 2 0 014 0z" /></svg>
					{/snippet}
				</DropdownMenu>
				<button type="button" class="tasks-icon-btn" onclick={onclose} aria-label="Close" title="Close (Esc)">
					<svg aria-hidden="true" class="w-5 h-5" fill="none" stroke="currentColor" viewBox="0 0 24 24"><path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M6 18L18 6M6 6l12 12" /></svg>
				</button>
			</div>
		</header>

		<div class="task-modal-body">
			<section class="task-modal-main" aria-label="Task">
				<div class="task-modal-title-row">
					<button
						type="button"
						class="task-check task-check-lg"
						data-priority={task.priority}
						aria-pressed={!!task.completed_at}
						aria-label={task.completed_at ? 'Mark not done' : 'Complete'}
						onclick={() => ontoggle(task)}
					>
						<svg aria-hidden="true" class="task-check-tick" fill="none" stroke="currentColor" viewBox="0 0 24 24"><path stroke-linecap="round" stroke-linejoin="round" stroke-width="3" d="M5 13l4 4L19 7" /></svg>
					</button>
					<div class="task-modal-title-col">
						<SmartTaskInput
							bind:value={title}
							tokens={st.smart.tokens}
							projects={st.projects}
							categories={st.categories}
							scopeProjectId={st.scope}
							ariaLabel="Title"
							class="task-modal-title {task.completed_at ? 'task-modal-title-done' : ''}"
							onenter={blurActive}
							onblur={saveTitle}
						/>
						{#if titleChips.length}
							<div class="task-modal-smart" aria-live="polite">
								{#each titleChips as chip}
									<span class="task-chip" data-kind={chip.kind} data-priority={chip.priority}>
										{#if chip.color}<span class="tasks-dot" style="background-color: {chip.color}"></span>{/if}
										{chip.label}
									</span>
								{/each}
								<span class="task-modal-smart-hint">Enter to apply</span>
							</div>
						{/if}
						{#if hasLinks}
							<div class="task-modal-links"><LinkedText text={linkText} linksOnly /></div>
						{/if}
						<div class="task-modal-desc-wrap">
							<svg aria-hidden="true" class="task-modal-desc-icon" fill="none" stroke="currentColor" viewBox="0 0 24 24"><path stroke-linecap="round" stroke-width="2" d="M4 6h16M4 12h16M4 18h10" /></svg>
							<textarea
								class="task-modal-desc"
								bind:value={description}
								rows="2"
								placeholder="Description"
								aria-label="Description"
								onblur={saveDescription}
							></textarea>
						</div>
					</div>
				</div>

				{#if !task.parent_id}
					<section class="task-modal-subtasks" aria-label="Sub-tasks">
						{#if subtasks.length}
							<h3 class="task-modal-section">
								Sub-tasks
								<span class="task-group-count">{subtasksDone}/{subtasks.length}</span>
							</h3>
						{/if}
						{#each subtasks as sub (sub.id)}
							{@const subName = plainText(sub.title)}
							<div class="task-modal-sub" class:task-row-done={!!sub.completed_at}>
								<button
									type="button"
									class="task-check task-check-sm"
									data-priority={sub.priority}
									aria-pressed={!!sub.completed_at}
									aria-label={sub.completed_at ? `Mark "${subName}" not done` : `Complete "${subName}"`}
									onclick={() => ontoggle(sub)}
								>
									<svg aria-hidden="true" class="task-check-tick" fill="none" stroke="currentColor" viewBox="0 0 24 24"><path stroke-linecap="round" stroke-linejoin="round" stroke-width="3" d="M5 13l4 4L19 7" /></svg>
								</button>
								<!-- Links sit inside, as in TaskRow: the pointer clicks this, the keyboard the button over it. -->
								<!-- svelte-ignore a11y_click_events_have_key_events, a11y_no_static_element_interactions -->
								<div class="task-modal-sub-title" onclick={() => onopen(sub)}>
									<button type="button" class="task-open" aria-label="Open {subName}"></button>
									<LinkedText text={sub.title} />
								</div>
								{#if sub.duration_minutes}<span class="task-modal-sub-time">{formatDuration(sub.duration_minutes)}</span>{/if}
							</div>
						{/each}
						{#if addingSubtask}
							<div class="task-modal-sub-line">
								<SmartTaskInput
									bind:this={subField}
									bind:value={newSubtask}
									tokens={subParsed.tokens}
									projects={[]}
									categories={[]}
									placeholder="Sub-task name, e.g. Book tickets fri 6pm p2"
									ariaLabel="New sub-task"
									class="task-modal-sub-add"
									onenter={addSubtask}
									onescape={closeSubtaskLine}
									onpaste={pasteSubtasks}
								/>
								<div class="task-modal-sub-actions">
									<button type="button" class="btn btn-ghost" onclick={closeSubtaskLine}>Cancel</button>
									<button type="button" class="btn btn-primary" onclick={addSubtask} disabled={!subParsed.title}>Add sub-task</button>
								</div>
							</div>
						{:else}
							<button type="button" class="task-add task-modal-sub-open" onclick={openSubtaskLine}>
								<svg aria-hidden="true" class="w-4 h-4" fill="none" stroke="currentColor" viewBox="0 0 24 24"><path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M12 5v14m7-7H5" /></svg>
								Add sub-task
							</button>
						{/if}
					</section>
				{/if}

				<p class="task-modal-added">
					Added {new Date(task.created_at).toLocaleDateString(undefined, { day: 'numeric', month: 'short', year: 'numeric' })}
					{#if task.completed_at}· Completed {new Date(task.completed_at).toLocaleDateString(undefined, { day: 'numeric', month: 'short', year: 'numeric' })}{/if}
				</p>
			</section>

			<aside class="task-modal-side" aria-label="Task details">
				<div class="task-side-item">
					<label class="task-side-label" for="task-place">Project</label>
					{#if task.parent_id}
						<span class="task-side-static">Same as its parent task</span>
					{:else}
						<Select id="task-place" class="task-side-control" value={placeValue} options={placeOptions} onchange={savePlace} />
					{/if}
				</div>

				<div class="task-side-item">
					<label class="task-side-label" for="task-date">Date</label>
					<div class="task-side-pair">
						<DatePicker id="task-date" value={task.due_date ?? ''} placeholder="No date" onchange={saveDate} />
						<input
							type="time"
							class="input task-side-time"
							aria-label="Time"
							value={task.due_time ?? ''}
							title={task.due_date ? undefined : 'A time with no date sets the date to today'}
							onchange={(e) => saveTime(e.currentTarget.value)}
						/>
					</div>
				</div>

				<div class="task-side-item">
					<label class="task-side-label" for="task-priority">Priority</label>
					<div class="task-side-priority" data-priority={task.priority}>
						<svg aria-hidden="true" class="task-side-flag" fill="currentColor" viewBox="0 0 24 24"><path d="M5 3a1 1 0 011 1v.5h12.38a1 1 0 01.8 1.6L16.5 9.5l2.68 3.4a1 1 0 01-.8 1.6H6V21a1 1 0 11-2 0V4a1 1 0 011-1z" /></svg>
						<Select id="task-priority" class="task-side-control" value={task.priority} options={PRIORITY_OPTIONS} onchange={(v: TaskPriority) => save({ priority: v })} />
					</div>
				</div>

				<div class="task-side-item">
					<label class="task-side-label" for="task-duration">Duration</label>
					<Select id="task-duration" class="task-side-control" value={durationValue} options={durationOptions} onchange={saveDuration} />
				</div>

				<div class="task-side-item">
					<label class="task-side-label" for="task-remind">Reminders</label>
					<Select
						id="task-remind"
						class="task-side-control"
						value={reminderValue}
						options={reminderOptions}
						disabled={dueAt === null && !task.remind_at}
						onchange={saveReminder}
					/>
					{#if dueAt === null}
						<p class="task-side-hint">Set a date to add a reminder.</p>
					{:else if !task.due_time}
						<p class="task-side-hint">No time set, so it counts from 9:00 am.</p>
					{/if}
				</div>

				<div class="task-side-item">
					<label class="task-side-label" for="task-repeat">Repeat</label>
					<input
						id="task-repeat"
						class="input task-side-input"
						bind:value={recurrenceText}
						placeholder="e.g. every monday"
						onblur={saveRecurrence}
						onkeydown={blurOnEnter}
					/>
					{#if recurrenceError}<p class="task-side-error">{recurrenceError}</p>{/if}
				</div>

				<div class="task-side-item">
					<label class="task-side-label" for="task-location">
						Location
						<span class="task-side-soon" title="Reminders when you arrive at this location are coming later">Arrival reminders soon</span>
					</label>
					<div class="task-side-location">
						<svg aria-hidden="true" class="task-side-pin" fill="none" stroke="currentColor" viewBox="0 0 24 24"><path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M12 21s-7-6.1-7-11.5a7 7 0 1114 0C19 14.9 12 21 12 21z" /><circle cx="12" cy="9.5" r="2.5" stroke-width="2" /></svg>
						<input
							id="task-location"
							class="input task-side-input"
							bind:value={locationText}
							maxlength="500"
							placeholder="Add a location"
							onblur={saveLocation}
							onkeydown={blurOnEnter}
						/>
					</div>
				</div>
			</aside>
		</div>
	</div>
</div>
