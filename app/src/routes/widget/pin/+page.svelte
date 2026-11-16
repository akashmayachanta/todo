<script lang="ts">
	import { onMount } from 'svelte';
	import { getCurrentWindow } from '@tauri-apps/api/window';
	import { api, errorDetail } from '$lib/api';
	import type { Task, TaskSection, TaskFilter, TaskIn, TaskProject } from '$lib/api';
	import PlaceMark from '$lib/components/tasks/PlaceMark.svelte';
	import QuickAdd from '$lib/components/tasks/QuickAdd.svelte';
	import TaskRow from '$lib/components/tasks/TaskRow.svelte';
	import {
		PIN_COLORS, loadPinColor, pinBackground, pinFromLabel, pinNewTask, pinView, savePinColor,
		type PinColorChoice, type PinView,
	} from '$lib/components/tasks/pins';
	import { byPosition, completionPatch, parentFinder, parentMinutes, subtaskMinutes } from '$lib/components/tasks/shared';
	import { toast } from '$lib/stores/toast';
	import { askReminderPermission } from '$lib/stores/taskReminders';
	import { formatDuration, ymd } from '$lib/taskParse';
	import { onTasksChanged } from '$lib/tasks/client';
	import { closePin, showTask } from '$shell/widgets';
	import '../../tasks/tasks.css';
	import '../widget.css';

	/**
	 * A list or a task pinned to the desktop: a small always-on-top window (src-tauri/src/widgets.rs).
	 * Its label says what it shows. Tasks can be ticked and added here; clicking one opens
	 * it in the main window, and closing the window unpins it.
	 */
	const ROW_FIELDS = new Set(['due', 'duration'] as const);

	const label = getCurrentWindow().label;
	const target = pinFromLabel(label);

	let tasks = $state<Task[]>([]);
	let pinnedTask = $state<Task | null>(null);
	let projects = $state<TaskProject[]>([]);
	let sections = $state<TaskSection[]>([]);
	let filters = $state<TaskFilter[]>([]);
	let loaded = $state(false);
	let adding = $state(false);
	let now = $state(new Date());
	let colorChoice = $state<PinColorChoice>(loadPinColor(label));
	let picking = $state(false);

	let today = $derived(ymd(now));
	let view = $derived<PinView | null>(
		target && loaded ? pinView(target, { tasks, pinnedTask, projects, sections, filters, today }) : null,
	);
	// Unset, a pin takes a pale shade of its project's (or filter's) colour.
	let background = $derived(pinBackground(colorChoice, view?.color ?? null));
	let openCount = $derived(view?.count ?? view?.tasks.filter((t) => !t.completed_at).length ?? 0);

	// Open work listed here. A nested view lists parents whose duration already is their
	// subtasks' total; a flat one lists subtasks itself, so a rolled-up parent is skipped.
	let totalMinutes = $derived.by(() => {
		if (!view) return 0;
		let sum = 0;
		for (const t of view.tasks) {
			if (t.completed_at || !t.duration_minutes) continue;
			if (!view.nested && t.parent_id === null && subtaskMinutes(subtasksOf(t.id)) !== null) continue;
			sum += t.duration_minutes;
		}
		return sum;
	});

	let subtaskMap = $derived.by(() => {
		const map = new Map<string, Task[]>();
		for (const t of tasks) {
			if (t.parent_id === null) continue;
			map.set(t.parent_id, [...(map.get(t.parent_id) ?? []), t]);
		}
		for (const list of map.values()) list.sort(byPosition);
		return map;
	});
	const subtasksOf = (id: string) => subtaskMap.get(id) ?? [];
	let parentOf = $derived(parentFinder(tasks));

	async function load() {
		try {
			[tasks, projects, sections, filters] = await Promise.all([
				api.tasks.list(), api.tasks.projects.list(), api.tasks.sections.list(), api.tasks.filters.list(),
			]);
			// A pinned task that is done isn't among the open ones; gone altogether, the window says so.
			pinnedTask = target?.kind === 'task' && target.id && !tasks.some((t) => t.id === target.id)
				? await api.tasks.get(target.id).catch(() => null)
				: null;
			loaded = true;
		} catch (e) {
			toast.error(errorDetail(e));
		}
	}

	async function toggle(task: Task) {
		try {
			await api.tasks.update(task.id, completionPatch(task, now));
			await load();
			// A done subtask's time leaves its task's remaining total; an undone one comes back.
			const parent = task.parent_id !== null && task.duration_minutes ? tasks.find((t) => t.id === task.parent_id) : null;
			const next = parent ? parentMinutes(parent, subtasksOf(parent.id)) : undefined;
			if (parent && next !== undefined) {
				await api.tasks.update(parent.id, { duration_minutes: next });
				await load();
			}
		} catch (e) {
			toast.error(errorDetail(e));
		}
	}

	async function add(data: TaskIn) {
		if (!view) return;
		try {
			const created = await api.tasks.create(pinNewTask(view, data));
			if (created.remind_at) askReminderPermission();
			await load();
		} catch (e) {
			toast.error(errorDetail(e));
			throw e;
		}
	}

	function pickColor(choice: PinColorChoice) {
		colorChoice = choice;
		savePinColor(label, choice);
		picking = false;
	}

	function unpin() {
		savePinColor(label, null);
		closePin();
	}

	function open(task: Task) {
		showTask(task.id).catch((e) => toast.error(errorDetail(e)));
	}

	onMount(() => {
		load();
		const stopChanged = onTasksChanged(load);
		// Keeps "Today" and the overdue marks right past midnight.
		const clock = setInterval(() => (now = new Date()), 60_000);
		return () => {
			stopChanged();
			clearInterval(clock);
		};
	});
</script>

<svelte:window onkeydown={(e) => { if (picking && e.key === 'Escape') picking = false; }} />

<div class="widget-pin" class:widget-pin-tinted={background} style:--pin-bg={background}>
	<!-- The bar moves the window. -->
	<div class="widget-pin-bar" data-tauri-drag-region>
		{#if view?.color && view.mark}<PlaceMark kind={view.mark} color={view.color} />
		{:else if view?.color}<span class="tasks-dot" style="background-color: {view.color}"></span>{/if}
		<span class="widget-pin-title" title={view?.title}>{view?.title ?? 'Tasks'}</span>
		{#if totalMinutes}<span class="widget-pin-count" title="Time of the open tasks here">{formatDuration(totalMinutes)}</span>{/if}
		{#if openCount}<span class="widget-pin-count">{openCount}</span>{/if}
		<button type="button" class="tasks-icon-btn" onclick={() => (picking = !picking)} title="Colour" aria-label="Colour" aria-expanded={picking}>
			<svg aria-hidden="true" class="w-3.5 h-3.5" fill="none" stroke="currentColor" viewBox="0 0 24 24"><path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M12 21a9 9 0 1 1 9-9c0 1.66-1.34 3-3 3h-1.5a1.5 1.5 0 0 0-1.06 2.56A1.5 1.5 0 0 1 14.38 21H12z" /><circle cx="7.5" cy="11" r="1" fill="currentColor" /><circle cx="10.5" cy="7.5" r="1" fill="currentColor" /><circle cx="15" cy="7.5" r="1" fill="currentColor" /></svg>
		</button>
		<button type="button" class="tasks-icon-btn" onclick={unpin} title="Close and unpin" aria-label="Close and unpin">
			<svg aria-hidden="true" class="w-3.5 h-3.5" fill="none" stroke="currentColor" viewBox="0 0 24 24"><path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M6 18L18 6M6 6l12 12" /></svg>
		</button>
	</div>

	{#if picking}
		<div class="widget-pin-colors" role="group" aria-label="Pin colour">
			{#each PIN_COLORS as c (c.key)}
				<button
					type="button"
					class="widget-pin-swatch"
					style:background-color={c.value}
					title={c.label}
					aria-label={c.label}
					aria-pressed={colorChoice === c.key}
					onclick={() => pickColor(c.key)}
				></button>
			{/each}
			<span class="widget-pin-colors-gap"></span>
			{#if view?.color}
				<button type="button" class="widget-pin-swatch" style:background-color={pinBackground(null, view.color)}
					title="Project colour" aria-label="Project colour" aria-pressed={colorChoice === null} onclick={() => pickColor(null)}></button>
			{/if}
			<button type="button" class="widget-pin-swatch widget-pin-swatch-none" title="Reset to white or black" aria-label="Reset to white or black"
				aria-pressed={!background} onclick={() => pickColor('none')}></button>
		</div>
	{/if}

	<div class="widget-pin-list">
		{#if !target}
			<p class="widget-pin-empty">This pin can't be read.</p>
		{:else if view?.missing}
			<p class="widget-pin-empty">What was pinned here has been deleted.</p>
			<button type="button" class="btn btn-secondary" onclick={unpin}>Close</button>
		{:else if view}
			{#each view.tasks as task (task.id)}
				<TaskRow
					{task}
					parent={parentOf(task)}
					{subtasksOf}
					{now}
					nested={view.nested}
					subtaskFilter={view.subtaskFilter}
					showNote={false}
					fields={ROW_FIELDS}
					compact
					ontoggle={toggle}
					onopen={open}
				/>
			{:else}
				<p class="widget-pin-empty">Nothing here.</p>
			{/each}
			{#if adding}
				<QuickAdd {projects} {sections} defaults={view.defaults} onadd={add} oncancel={() => (adding = false)} />
			{:else}
				<button type="button" class="task-add" onclick={() => (adding = true)}>
					<svg aria-hidden="true" class="w-4 h-4" fill="none" stroke="currentColor" viewBox="0 0 24 24"><path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M12 5v14m7-7H5" /></svg>
					{view.parentId ? 'Add sub-task' : 'Add task'}
				</button>
			{/if}
		{/if}
	</div>
</div>
