<script lang="ts">
	import { onMount } from 'svelte';
	import { getCurrentWindow } from '@tauri-apps/api/window';
	import { api, errorDetail } from '$lib/api';
	import type { Task, TaskCategory, TaskFilter, TaskIn, TaskProject } from '$lib/api';
	import QuickAdd from '$lib/components/tasks/QuickAdd.svelte';
	import TaskRow from '$lib/components/tasks/TaskRow.svelte';
	import { pinFromLabel, pinNewTask, pinView, type PinView } from '$lib/components/tasks/pins';
	import { byPosition, completionPatch } from '$lib/components/tasks/shared';
	import { toast } from '$lib/stores/toast';
	import { askReminderPermission } from '$lib/stores/taskReminders';
	import { ymd } from '$lib/taskParse';
	import { onTasksChanged } from '$lib/tasks/client';
	import { closePin, showTask } from '$shell/widgets';
	import '../../tasks/tasks.css';
	import '../widget.css';

	/**
	 * A list or a task pinned to the desktop: a small always-on-top window (src-tauri/src/widgets.rs).
	 * Its label says what it shows. Tasks can be ticked and added here; clicking one opens
	 * it in the main window, and closing the window unpins it.
	 */
	const DUE_ONLY = new Set(['due'] as const);

	const target = pinFromLabel(getCurrentWindow().label);

	let tasks = $state<Task[]>([]);
	let pinnedTask = $state<Task | null>(null);
	let projects = $state<TaskProject[]>([]);
	let categories = $state<TaskCategory[]>([]);
	let filters = $state<TaskFilter[]>([]);
	let loaded = $state(false);
	let adding = $state(false);
	let now = $state(new Date());

	let today = $derived(ymd(now));
	let view = $derived<PinView | null>(
		target && loaded ? pinView(target, { tasks, pinnedTask, projects, categories, filters, today }) : null,
	);
	let openCount = $derived(view?.tasks.filter((t) => !t.completed_at).length ?? 0);

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

	async function load() {
		try {
			[tasks, projects, categories, filters] = await Promise.all([
				api.tasks.list(), api.tasks.projects.list(), api.tasks.categories.list(), api.tasks.filters.list(),
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

<div class="widget-pin">
	<!-- The bar moves the window. -->
	<div class="widget-pin-bar" data-tauri-drag-region>
		{#if view?.color}<span class="tasks-dot" style="background-color: {view.color}"></span>{/if}
		<span class="widget-pin-title" title={view?.title}>{view?.title ?? 'Tasks'}</span>
		{#if openCount}<span class="widget-pin-count">{openCount}</span>{/if}
		<button type="button" class="tasks-icon-btn" onclick={closePin} title="Close and unpin" aria-label="Close and unpin">
			<svg aria-hidden="true" class="w-3.5 h-3.5" fill="none" stroke="currentColor" viewBox="0 0 24 24"><path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M6 18L18 6M6 6l12 12" /></svg>
		</button>
	</div>

	<div class="widget-pin-list">
		{#if !target}
			<p class="widget-pin-empty">This pin can't be read.</p>
		{:else if view?.missing}
			<p class="widget-pin-empty">What was pinned here has been deleted.</p>
			<button type="button" class="btn btn-secondary" onclick={closePin}>Close</button>
		{:else if view}
			{#each view.tasks as task (task.id)}
				<TaskRow
					{task}
					{subtasksOf}
					{now}
					nested={view.nested}
					showNote={false}
					fields={DUE_ONLY}
					compact
					ontoggle={toggle}
					onopen={open}
				/>
			{:else}
				<p class="widget-pin-empty">Nothing here.</p>
			{/each}
			{#if adding}
				<QuickAdd {projects} {categories} defaults={view.defaults} onadd={add} oncancel={() => (adding = false)} />
			{:else}
				<button type="button" class="task-add" onclick={() => (adding = true)}>
					<svg aria-hidden="true" class="w-4 h-4" fill="none" stroke="currentColor" viewBox="0 0 24 24"><path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M12 5v14m7-7H5" /></svg>
					{view.parentId ? 'Add sub-task' : 'Add task'}
				</button>
			{/if}
		{/if}
	</div>
</div>
