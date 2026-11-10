<script lang="ts">
	import { onMount } from 'svelte';
	import { listen } from '@tauri-apps/api/event';
	import { api, errorDetail } from '$lib/api';
	import type { TaskCategory, TaskIn, TaskProject } from '$lib/api';
	import QuickAdd from '$lib/components/tasks/QuickAdd.svelte';
	import SmartTips from '$lib/components/tasks/SmartTips.svelte';
	import { placeName } from '$lib/components/tasks/shared';
	import { toast } from '$lib/stores/toast';
	import { askReminderPermission } from '$lib/stores/taskReminders';
	import { dayLabel } from '$lib/taskParse';
	import { onTasksChanged } from '$lib/tasks/client';
	import { hideQuickAdd } from '$shell/widgets';
	import '../../tasks/tasks.css';
	import '../widget.css';

	/**
	 * The quick add popup: its own window, kept loaded and hidden (src-tauri/src/widgets.rs),
	 * so Alt+N shows it at once from any app without bringing up the main window. Esc,
	 * clicking away or losing focus hides it; after adding, it says where the task went.
	 */
	const ADDED_FOR_MS = 1000;

	let projects = $state<TaskProject[]>([]);
	let categories = $state<TaskCategory[]>([]);
	/** Bumped each time it opens, for an empty line with the focus in it. */
	let session = $state(0);
	let added = $state('');
	let addedTimer: ReturnType<typeof setTimeout> | undefined;

	async function loadPlaces() {
		try {
			[projects, categories] = await Promise.all([api.tasks.projects.list(), api.tasks.categories.list()]);
		} catch (e) {
			toast.error(errorDetail(e));
		}
	}

	async function add(data: TaskIn) {
		try {
			const created = await api.tasks.create(data);
			if (created.remind_at) askReminderPermission();
			added = `Added to ${placeName(created, projects, categories)}${created.due_date ? `, ${dayLabel(created.due_date, new Date())}` : ''}`;
			addedTimer = setTimeout(() => {
				added = '';
				hideQuickAdd();
			}, ADDED_FOR_MS);
		} catch (e) {
			toast.error(errorDetail(e));
			throw e;
		}
	}

	function focusLine() {
		document.querySelector<HTMLInputElement>('.quick-add-input')?.focus();
	}

	onMount(() => {
		document.documentElement.classList.add('widget-transparent');
		loadPlaces();
		const stopOpen = listen('tasks://quick-open', () => {
			clearTimeout(addedTimer);
			added = '';
			session++;
			loadPlaces();
		});
		const stopChanged = onTasksChanged(loadPlaces);
		return () => {
			clearTimeout(addedTimer);
			stopOpen.then((stop) => stop());
			stopChanged();
		};
	});
</script>

<!-- The window can get the focus after the line was made. -->
<svelte:window onfocus={focusLine} />

<!-- Around the card the window is see-through, and a click there counts as clicking away. -->
<!-- svelte-ignore a11y_no_static_element_interactions -->
<div class="widget-quick" onpointerdown={(e) => { if (e.target === e.currentTarget) hideQuickAdd(); }}>
	<div class="widget-quick-card" role="dialog" aria-label="Add task">
		{#if added}
			<p class="widget-quick-added" role="status">{added}</p>
		{:else}
			<div class="widget-quick-row">
				{#key session}
					<QuickAdd {projects} {categories} onadd={add} oncancel={hideQuickAdd} />
				{/key}
				<SmartTips />
			</div>
		{/if}
	</div>
</div>
