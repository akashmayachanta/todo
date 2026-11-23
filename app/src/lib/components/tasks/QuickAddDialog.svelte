<script lang="ts">
	import { goto } from '$app/navigation';
	import { resolve } from '$app/paths';
	import { api, errorDetail } from '$lib/api';
	import type { TaskFilter, TaskSection, TaskIn, TaskProject } from '$lib/api';
	import { toast } from '$lib/stores/toast';
	import { askReminderPermission } from '$lib/stores/taskReminders';
	import { closeQuickAdd, quickAdd, taskAdded } from '$lib/stores/taskQuickAdd';
	import { dayLabel } from '$lib/taskParse';
	import GoTo from './GoTo.svelte';
	import QuickAdd from './QuickAdd.svelte';
	import SmartTips from './SmartTips.svelte';
	import { placeName } from './shared';

	/**
	 * The app-wide quick add dialog, mounted once in the root layout. A line starting
	 * with `>` goes to a view, project, section or filter instead, as in the Ctrl+Space popup.
	 */

	let projects = $state<TaskProject[]>([]);
	let sections = $state<TaskSection[]>([]);
	let filters = $state<TaskFilter[]>([]);
	let ready = $state(false);
	/** Set while going somewhere: what was typed after the `>`. */
	let goTo = $state<string | null>(null);

	// Fresh projects, sections and filters each time it opens, for the `#`, `/` and `>` suggestions.
	$effect(() => {
		if (!$quickAdd) {
			ready = false;
			goTo = null;
			return;
		}
		let live = true;
		Promise.all([api.tasks.projects.list(), api.tasks.sections.list(), api.tasks.filters.list()])
			.then(([p, c, f]) => {
				if (!live) return;
				projects = p;
				sections = c;
				filters = f;
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
			toast.success(`Added to ${placeName(created, projects, sections)}${created.due_date ? `, ${dayLabel(created.due_date, new Date())}` : ''}`);
			closeQuickAdd();
		} catch (e) {
			toast.error(errorDetail(e));
			throw e;
		}
	}

	function go(query: string) {
		closeQuickAdd();
		goto(`${resolve('/tasks' as any)}?${query}`);
	}
</script>

<svelte:window onkeydown={(e) => { if (e.key === 'Escape' && $quickAdd && !e.defaultPrevented) closeQuickAdd(); }} />

{#if $quickAdd}
	<!-- Laid out like the Ctrl+Space popup (routes/widget/quick): no heading, the card where that window puts it. -->
	<div class="quick-add-backdrop" onclick={closeQuickAdd} aria-hidden="true"></div>
	<div class="quick-add-overlay">
		<div class="widget-quick-card quick-add-panel" role="dialog" aria-modal="true" aria-label={goTo !== null ? 'Go to' : 'Add task'}>
			{#if !ready}
				<div class="widget-quick-row">
					<div class="quick-add-loading"><div class="spinner"></div>Loading…</div>
					<SmartTips />
				</div>
			{:else if goTo !== null}
				<GoTo {projects} {sections} {filters} initial={goTo} ongo={go} onback={() => (goTo = null)} oncancel={closeQuickAdd} />
			{:else}
				<div class="widget-quick-row">
					<QuickAdd {projects} {sections} defaults={$quickAdd} onadd={add} oncancel={closeQuickAdd} ongoto={(q) => (goTo = q)} />
					<SmartTips />
				</div>
			{/if}
		</div>
	</div>
{/if}
