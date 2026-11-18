<script lang="ts">
	import { tick } from 'svelte';
	import { placePanel, placementStyle, type Placement } from '$lib/dropdown';
	import '$lib/components/Dropdown.css';
	import type { TaskLayout } from './shared';

	/**
	 * "Display" in the Tasks header: a panel with the view's layout (list, board, calendar,
	 * table) as tiles, and under them the "Show completed tasks" switch. The button shows the
	 * current layout's icon. `showCompleted` null leaves the switch out.
	 */
	let { layout, options, onchange, showCompleted = null, oncompleted }: {
		layout: TaskLayout;
		options: readonly TaskLayout[];
		onchange: (layout: TaskLayout) => void;
		showCompleted?: boolean | null;
		oncompleted?: (show: boolean) => void;
	} = $props();

	const uid = $props.id();

	const LAYOUTS: { key: TaskLayout; label: string; icon: string }[] = [
		{ key: 'list',     label: 'List',     icon: 'M4 6h16M4 12h16M4 18h16' },
		{ key: 'board',    label: 'Board',    icon: 'M4 4h4v16H4zM10 4h4v10h-4zM16 4h4v13h-4z' },
		{ key: 'calendar', label: 'Calendar', icon: 'M8 7V3m8 4V3m-9 8h10M5 21h14a2 2 0 002-2V7a2 2 0 00-2-2H5a2 2 0 00-2 2v12a2 2 0 002 2z' },
		{ key: 'table',    label: 'Table',    icon: 'M4 5h16v14H4zM4 10h16M4 15h16M10 10v9' },
	];

	let shown = $derived(LAYOUTS.filter((l) => options.includes(l.key)));
	let current = $derived(LAYOUTS.find((l) => l.key === layout) ?? LAYOUTS[0]);

	let open = $state(false);
	let place = $state<Placement | null>(null);
	let root = $state<HTMLDivElement | null>(null);
	let trigger = $state<HTMLButtonElement | null>(null);
	let panel = $state<HTMLDivElement | null>(null);

	async function show() {
		if (!trigger) return;
		place = placePanel(trigger, 220, 'end');
		open = true;
		await tick();
		panel?.querySelector<HTMLElement>('[aria-checked="true"]')?.focus();
	}

	function close(refocus = false) {
		open = false;
		if (refocus) trigger?.focus();
	}

	function pick(l: TaskLayout) {
		if (l !== layout) onchange(l);
	}

	function onPanelKeydown(e: KeyboardEvent) {
		const tiles = [...(panel?.querySelectorAll<HTMLElement>('[role="radio"]') ?? [])];
		const at = tiles.indexOf(document.activeElement as HTMLElement);
		if (at >= 0 && (e.key === 'ArrowRight' || e.key === 'ArrowLeft')) {
			e.preventDefault();
			const next = tiles[(at + (e.key === 'ArrowRight' ? 1 : -1) + tiles.length) % tiles.length];
			next?.focus();
			next?.click();
		} else if (e.key === 'Escape') {
			e.preventDefault();
			e.stopPropagation();
			close(true);
		}
	}

	function onPointerDown(e: PointerEvent) {
		if (open && root && !e.composedPath().includes(root)) close();
	}

	function onFocusOut(e: FocusEvent) {
		const to = e.relatedTarget as Node | null;
		if (open && to && root && !root.contains(to)) close();
	}
</script>

<svelte:window onpointerdown={onPointerDown} onresize={() => close()} />

<div class="tasks-display" bind:this={root} onfocusout={onFocusOut}>
	<button
		bind:this={trigger}
		type="button"
		class="tasks-head-btn"
		aria-haspopup="dialog"
		aria-expanded={open}
		aria-controls="{uid}-panel"
		title="Display: layout and completed tasks"
		onclick={() => (open ? close() : show())}
	>
		<svg aria-hidden="true" class="w-4 h-4" fill="none" stroke="currentColor" viewBox="0 0 24 24"><path stroke-linecap="round" stroke-linejoin="round" stroke-width="1.75" d={current.icon} /></svg>
		<span class="tasks-head-btn-label">Display<span class="sr-only">: {current.label}</span></span>
		{#if showCompleted}<span class="tasks-display-dot" title="Showing completed tasks"></span>{/if}
		<svg aria-hidden="true" class="tasks-head-btn-caret" fill="none" stroke="currentColor" viewBox="0 0 24 24"><path stroke-linecap="round" stroke-linejoin="round" stroke-width="2.25" d="M19 9l-7 7-7-7" /></svg>
	</button>

	{#if open && place}
		<!-- svelte-ignore a11y_no_noninteractive_element_interactions -->
		<div bind:this={panel} id="{uid}-panel" role="dialog" aria-label="Display" tabindex="-1" class="dd-panel tasks-display-panel" style={placementStyle(place)} onkeydown={onPanelKeydown}>
			<p class="tasks-group-panel-title">Layout</p>
			<div class="tasks-display-tiles" role="radiogroup" aria-label="Layout">
				{#each shown as l (l.key)}
					<button
						type="button"
						role="radio"
						class="tasks-display-tile"
						aria-checked={layout === l.key}
						tabindex={layout === l.key ? 0 : -1}
						onclick={() => pick(l.key)}
					>
						<svg aria-hidden="true" fill="none" stroke="currentColor" viewBox="0 0 24 24"><path stroke-linecap="round" stroke-linejoin="round" stroke-width="1.6" d={l.icon} /></svg>
						<span>{l.label}</span>
					</button>
				{/each}
			</div>
			{#if showCompleted !== null}
				<label class="tasks-display-switch">
					<span>Show completed tasks</span>
					<input
						type="checkbox"
						role="switch"
						class="tasks-switch"
						checked={showCompleted}
						onchange={(e) => oncompleted?.(e.currentTarget.checked)}
					/>
				</label>
			{/if}
		</div>
	{/if}
</div>
