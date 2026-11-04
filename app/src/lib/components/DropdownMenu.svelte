<script module lang="ts">
	export interface MenuItem {
		id: string;
		label: string;
		/** Set for a checkbox item (stays open when toggled); leave out for an action, which closes the menu. */
		checked?: boolean;
		disabled?: boolean;
		danger?: boolean;
	}
</script>

<script lang="ts">
	import type { Snippet } from 'svelte';
	import { placePanel, placementStyle, type Placement } from '$lib/dropdown';
	import './Dropdown.css';

	/**
	 * A button that opens a themed menu: checkbox items (e.g. a column picker), which keep the
	 * menu open while toggled, and/or action items, which close it. Keyboard: arrows move
	 * between items, Esc closes. With `label` empty and an `icon`, the trigger is an icon
	 * button (give it `ariaLabel`).
	 */
	let { label = '', icon, items, ontoggle, onselect, align = 'end', ariaLabel, triggerClass = 'btn btn-secondary', class: className = '' }: {
		label?: string;
		icon?: Snippet;
		items: MenuItem[];
		ontoggle?: (id: string, checked: boolean) => void;
		onselect?: (id: string) => void;
		align?: 'start' | 'end';
		ariaLabel?: string;
		/** Classes for the trigger button. */
		triggerClass?: string;
		class?: string;
	} = $props();

	const uid = $props.id();

	let open = $state(false);
	let place = $state<Placement | null>(null);
	let root = $state<HTMLDivElement | null>(null);
	let trigger = $state<HTMLButtonElement | null>(null);
	let panel = $state<HTMLDivElement | null>(null);

	function show() {
		if (!trigger) return;
		place = placePanel(trigger, 320, align);
		open = true;
	}

	function close(refocus = false) {
		open = false;
		if (refocus) trigger?.focus();
	}

	function choose(item: MenuItem) {
		if (item.checked === undefined) {
			close(true);
			onselect?.(item.id);
		} else {
			ontoggle?.(item.id, !item.checked);
		}
	}

	function focusItem(delta: number | 'first' | 'last') {
		const buttons = [...(panel?.querySelectorAll<HTMLButtonElement>('button:not(:disabled)') ?? [])];
		if (buttons.length === 0) return;
		const at = buttons.indexOf(document.activeElement as HTMLButtonElement);
		const next = delta === 'first' ? 0 : delta === 'last' ? buttons.length - 1 : (at + delta + buttons.length) % buttons.length;
		buttons[next].focus();
	}

	function onTriggerKeydown(e: KeyboardEvent) {
		if (e.key === 'ArrowDown') {
			e.preventDefault();
			if (!open) show();
			// The panel renders on the next tick.
			queueMicrotask(() => focusItem('first'));
		}
	}

	function onPanelKeydown(e: KeyboardEvent) {
		switch (e.key) {
			case 'ArrowDown': e.preventDefault(); focusItem(1); break;
			case 'ArrowUp':   e.preventDefault(); focusItem(-1); break;
			case 'Home':      e.preventDefault(); focusItem('first'); break;
			case 'End':       e.preventDefault(); focusItem('last'); break;
			case 'Tab':       close(); break;
		}
	}

	function onKeydown(e: KeyboardEvent) {
		if (e.key === 'Escape' && open) {
			e.stopPropagation(); // keep a surrounding modal open
			close(true);
		}
	}

	function onPointerDown(e: PointerEvent) {
		if (open && root && !e.composedPath().includes(root)) close();
	}

	function onScroll(e: Event) {
		if (open && !(panel && e.target instanceof Node && panel.contains(e.target))) close();
	}
</script>

<svelte:window onpointerdown={onPointerDown} onscrollcapture={onScroll} onresize={() => close()} />

<div class="dd dd-inline {className}" bind:this={root} onkeydown={onKeydown} role="presentation">
	<button
		bind:this={trigger}
		type="button"
		class="{triggerClass} dd-menu-trigger"
		aria-label={ariaLabel}
		aria-haspopup="menu"
		aria-expanded={open}
		aria-controls="{uid}-menu"
		onclick={(e) => { e.stopPropagation(); if (open) close(); else show(); }}
		onkeydown={onTriggerKeydown}
	>
		{@render icon?.()}
		{#if label}
			{label}
			<svg class="dd-caret" fill="none" stroke="currentColor" viewBox="0 0 24 24" aria-hidden="true"><path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M19 9l-7 7-7-7" /></svg>
		{/if}
	</button>

	{#if open && place}
		<div bind:this={panel} id="{uid}-menu" role="menu" tabindex="-1" class="dd-panel" style={placementStyle(place)} onkeydown={onPanelKeydown}>
			{#each items as item (item.id)}
				{#if item.checked === undefined}
					<button
						type="button"
						role="menuitem"
						class="dd-item"
						class:dd-item-danger={item.danger}
						disabled={item.disabled}
						onclick={(e) => { e.stopPropagation(); choose(item); }}
					>
						{item.label}
					</button>
				{:else}
					<button
						type="button"
						role="menuitemcheckbox"
						class="dd-item"
						aria-checked={item.checked}
						disabled={item.disabled}
						onclick={(e) => { e.stopPropagation(); choose(item); }}
					>
						<span class="dd-check" aria-hidden="true">
							<svg aria-hidden="true" class="w-3 h-3" fill="none" stroke="currentColor" viewBox="0 0 24 24"><path stroke-linecap="round" stroke-linejoin="round" stroke-width="3" d="M5 13l4 4L19 7" /></svg>
						</span>
						{item.label}
					</button>
				{/if}
			{/each}
		</div>
	{/if}
</div>
