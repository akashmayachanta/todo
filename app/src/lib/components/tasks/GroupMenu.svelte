<script lang="ts" generics="T extends string">
	import { tick } from 'svelte';
	import { placePanel, placementStyle, type Placement } from '$lib/dropdown';
	import '$lib/components/Dropdown.css';

	/**
	 * "Group" in the Tasks header: how the list's sections and the board's columns are made.
	 * A compact button (stack icon and the current grouping) that sits with the layout switch,
	 * opening a short list with a tick by the current one.
	 */
	let { value, options, onchange }: {
		value: T;
		options: readonly { value: T; label: string }[];
		onchange: (value: T) => void;
	} = $props();

	const uid = $props.id();

	let open = $state(false);
	let place = $state<Placement | null>(null);
	let root = $state<HTMLDivElement | null>(null);
	let trigger = $state<HTMLButtonElement | null>(null);
	let panel = $state<HTMLDivElement | null>(null);

	let current = $derived(options.find((o) => o.value === value) ?? options[0]);

	async function show() {
		if (!trigger) return;
		place = placePanel(trigger, 320, 'end');
		open = true;
		await tick();
		panel?.querySelector<HTMLElement>('[aria-checked="true"]')?.focus();
	}

	function close(refocus = false) {
		open = false;
		if (refocus) trigger?.focus();
	}

	function pick(v: T) {
		close(true);
		if (v !== value) onchange(v);
	}

	function onPanelKeydown(e: KeyboardEvent) {
		const items = [...(panel?.querySelectorAll<HTMLElement>('[role="menuitemradio"]') ?? [])];
		const at = items.indexOf(document.activeElement as HTMLElement);
		if (e.key === 'ArrowDown' || e.key === 'ArrowUp') {
			e.preventDefault();
			items[(at + (e.key === 'ArrowDown' ? 1 : -1) + items.length) % items.length]?.focus();
		} else if (e.key === 'Escape') {
			e.preventDefault();
			e.stopPropagation();
			close(true);
		} else if (e.key === 'Tab') {
			close();
		}
	}

	function onPointerDown(e: PointerEvent) {
		if (open && root && !e.composedPath().includes(root)) close();
	}
</script>

<svelte:window onpointerdown={onPointerDown} onresize={() => close()} />

<div class="tasks-group-menu" bind:this={root}>
	<button
		bind:this={trigger}
		type="button"
		class="tasks-head-btn"
		aria-haspopup="menu"
		aria-expanded={open}
		aria-controls="{uid}-menu"
		title="Group by"
		onclick={() => (open ? close() : show())}
	>
		<svg aria-hidden="true" class="w-4 h-4" fill="none" stroke="currentColor" viewBox="0 0 24 24"><path stroke-linecap="round" stroke-linejoin="round" stroke-width="1.75" d="M4 5h16v4H4zM4 15h16v4H4z" /></svg>
		<span class="tasks-head-btn-label"><span class="sr-only">Group by </span>{current?.label}</span>
		<svg aria-hidden="true" class="tasks-head-btn-caret" fill="none" stroke="currentColor" viewBox="0 0 24 24"><path stroke-linecap="round" stroke-linejoin="round" stroke-width="2.25" d="M19 9l-7 7-7-7" /></svg>
	</button>

	{#if open && place}
		<div bind:this={panel} id="{uid}-menu" role="menu" aria-label="Group by" tabindex="-1" class="dd-panel tasks-group-panel" style={placementStyle(place)} onkeydown={onPanelKeydown}>
			<p class="tasks-group-panel-title">Group by</p>
			{#each options as o (o.value)}
				<button type="button" role="menuitemradio" class="dd-item" aria-checked={o.value === value} onclick={() => pick(o.value)}>
					<span class="tasks-group-panel-label">{o.label}</span>
					{#if o.value === value}
						<svg aria-hidden="true" class="dd-tick" fill="none" stroke="currentColor" viewBox="0 0 24 24"><path stroke-linecap="round" stroke-linejoin="round" stroke-width="2.5" d="M5 13l4 4L19 7" /></svg>
					{/if}
				</button>
			{/each}
		</div>
	{/if}
</div>
