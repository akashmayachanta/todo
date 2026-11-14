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
	<!-- Laid out like the Ctrl+Space popup (routes/widget/quick): no heading, the card where that window puts it. -->
	<div class="quick-add-backdrop" onclick={closeQuickAdd} aria-hidden="true"></div>
	<div class="quick-add-overlay">
		<div class="widget-quick-card quick-add-panel" role="dialog" aria-modal="true" aria-label="Add task">
			<div class="widget-quick-row">
				{#if ready}
					<QuickAdd {projects} {categories} defaults={$quickAdd} onadd={add} oncancel={closeQuickAdd} />
				{:else}
					<div class="quick-add-loading"><div class="spinner"></div>Loading…</div>
				{/if}
				<SmartTips />
			</div>
		</div>
	</div>
{/if}
