<script lang="ts">
	import { api, errorDetail } from '$lib/api';
	import type { TaskCategory, TaskIn, TaskProject } from '$lib/api';
	import { toast } from '$lib/stores/toast';
	import { askReminderPermission } from '$lib/stores/taskReminders';
	import { closeQuickAdd, quickAdd, taskAdded } from '$lib/stores/taskQuickAdd';
	import { dayLabel } from '$lib/taskParse';
	import QuickAdd from './QuickAdd.svelte';
	import SmartTips from './SmartTips.svelte';
	import { placeName } from './shared';

	/** The app-wide quick add dialog, mounted once in the root layout. */

	let projects = $state<TaskProject[]>([]);
	let categories = $state<TaskCategory[]>([]);
	let ready = $state(false);

	// Fresh projects and categories each time it opens, for the `#` and `/` suggestions.
	$effect(() => {
		if (!$quickAdd) {
			ready = false;
			return;
		}
		let live = true;
		Promise.all([api.tasks.projects.list(), api.tasks.categories.list()])
			.then(([p, c]) => {
				if (!live) return;
				projects = p;
				categories = c;
			})
			.catch((e) => toast.error(errorDetail(e)))
			.finally(() => {
				if (live) ready = true;
			});
		return () => {
			live = false;
		};
	});

	async function add(data: TaskIn) {
		try {
			const created = await api.tasks.create(data);
			taskAdded(created);
			if (created.remind_at) askReminderPermission();
			toast.success(`Added to ${placeName(created, projects, categories)}${created.due_date ? `, ${dayLabel(created.due_date, new Date())}` : ''}`);
			closeQuickAdd();
		} catch (e) {
			toast.error(errorDetail(e));
			throw e;
		}
	}
</script>

<svelte:window onkeydown={(e) => { if (e.key === 'Escape' && $quickAdd && !e.defaultPrevented) closeQuickAdd(); }} />

{#if $quickAdd}
	<div class="modal-backdrop" onclick={closeQuickAdd} aria-hidden="true"></div>
	<div class="modal-overlay quick-add-overlay">
		<div class="modal-panel quick-add-panel" role="dialog" aria-modal="true" aria-labelledby="quick-add-title">
			<div class="modal-header">
				<h2 id="quick-add-title" class="modal-title">Add task</h2>
				<div class="quick-add-head-actions">
					<SmartTips />
					<button onclick={closeQuickAdd} class="modal-close" aria-label="Close">
						<svg aria-hidden="true" class="w-5 h-5" fill="none" stroke="currentColor" viewBox="0 0 24 24"><path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M6 18L18 6M6 6l12 12" /></svg>
					</button>
				</div>
			</div>
			<div class="modal-body">
				{#if ready}
					<QuickAdd {projects} {categories} defaults={$quickAdd} onadd={add} oncancel={closeQuickAdd} />
				{:else}
					<div class="flex items-center gap-3 text-[var(--text-muted)]"><div class="spinner"></div>Loading…</div>
				{/if}
			</div>
		</div>
	</div>
{/if}
