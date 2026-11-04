<script lang="ts">
	import type { TaskLayout } from './shared';

	/** List / Board / Calendar switch for the current view. */
	let { layout, options, onchange }: {
		layout: TaskLayout;
		options: readonly TaskLayout[];
		onchange: (layout: TaskLayout) => void;
	} = $props();

	const LAYOUTS: { key: TaskLayout; label: string; icon: string }[] = [
		{ key: 'list',     label: 'List',     icon: 'M4 6h16M4 12h16M4 18h16' },
		{ key: 'board',    label: 'Board',    icon: 'M4 4h4v16H4zM10 4h4v10h-4zM16 4h4v13h-4z' },
		{ key: 'calendar', label: 'Calendar', icon: 'M8 7V3m8 4V3m-9 8h10M5 21h14a2 2 0 002-2V7a2 2 0 00-2-2H5a2 2 0 00-2 2v12a2 2 0 002 2z' },
	];
</script>

<div class="tasks-layout" role="group" aria-label="Layout">
	{#each LAYOUTS.filter((l) => options.includes(l.key)) as l (l.key)}
		<button
			type="button"
			class="tasks-layout-btn"
			aria-pressed={layout === l.key}
			title="{l.label} layout"
			onclick={() => onchange(l.key)}
		>
			<svg aria-hidden="true" class="w-4 h-4" fill="none" stroke="currentColor" viewBox="0 0 24 24"><path stroke-linecap="round" stroke-linejoin="round" stroke-width="1.75" d={l.icon} /></svg>
			<span class="tasks-layout-label">{l.label}</span>
		</button>
	{/each}
</div>
