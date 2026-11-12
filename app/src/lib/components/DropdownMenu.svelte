<script module lang="ts">
	export interface MenuItem {
		id: string;
		label: string;
		/** Set for a checkbox item (stays open when toggled); leave out for an action, which closes the menu. */
		checked?: boolean;
		disabled?: boolean;
		danger?: boolean;
		/** An icon from `MENU_ICONS`; defaults to the item's id. */
		icon?: string;
	}

	/** Outline icons for action items, keyed by the ids the menus use. */
	export const MENU_ICONS: Record<string, string> = {
		edit: 'M16.86 4.49l1.69-1.69a1.88 1.88 0 0 1 2.65 2.65L6.83 19.82a4.5 4.5 0 0 1-1.9 1.13l-2.68.8.8-2.68a4.5 4.5 0 0 1 1.13-1.9L16.86 4.49zm0 0L19.5 7.13',
		describe: 'M4 6h16M4 12h16M4 18h10',
		favorite: 'M21 8.25c0-2.49-2.1-4.5-4.69-4.5-1.93 0-3.6 1.13-4.31 2.73-.72-1.6-2.38-2.73-4.31-2.73C5.1 3.75 3 5.76 3 8.25c0 7.22 9 12 9 12s9-4.78 9-12z',
		unfavorite: 'M3 3l18 18M16.5 16.5L12 20.25s-9-4.78-9-12a4.5 4.5 0 0 1 1.63-3.47M8.25 3.79c1.62.16 3.04 1.2 3.75 2.69.72-1.6 2.38-2.73 4.31-2.73C18.9 3.75 21 5.76 21 8.25c0 2.1-.76 4.02-1.9 5.66',
		up: 'M12 19V5m0 0l-6 6m6-6l6 6',
		down: 'M12 5v14m0 0l-6-6m6 6l6-6',
		left: 'M19 12H5m0 0l6-6m-6 6l6 6',
		right: 'M5 12h14m0 0l-6-6m6 6l-6 6',
		move: 'M3 7.5A1.5 1.5 0 0 1 4.5 6h4.38l1.5 2h9.12A1.5 1.5 0 0 1 21 9.5v8a1.5 1.5 0 0 1-1.5 1.5h-15A1.5 1.5 0 0 1 3 17.5v-10zM10 13.5h5m0 0l-2-2m2 2l-2 2',
		pin: 'M12 17v5M9 10.76a2 2 0 0 1-1.11 1.79l-1.78.9A2 2 0 0 0 5 15.24V16a1 1 0 0 0 1 1h12a1 1 0 0 0 1-1v-.76a2 2 0 0 0-1.11-1.79l-1.78-.9A2 2 0 0 1 15 10.76V7a1 1 0 0 1 1-1 2 2 0 0 0 0-4H8a2 2 0 0 0 0 4 1 1 0 0 1 1 1z',
		copy: 'M10 13a5 5 0 0 0 7.54.54l3-3a5 5 0 0 0-7.07-7.07l-1.72 1.71M14 11a5 5 0 0 0-7.54-.54l-3 3a5 5 0 0 0 7.07 7.07l1.71-1.71',
		duplicate: 'M10 8h10a2 2 0 0 1 2 2v10a2 2 0 0 1-2 2H10a2 2 0 0 1-2-2V10a2 2 0 0 1 2-2zM4 16a2 2 0 0 1-2-2V4a2 2 0 0 1 2-2h10a2 2 0 0 1 2 2',
		clear: 'M18 6L7 17l-5-5M22 10l-7.5 7.5L13 16',
		delete: 'M3 6h18M19 6v14a2 2 0 0 1-2 2H7a2 2 0 0 1-2-2V6M8 6V4a2 2 0 0 1 2-2h4a2 2 0 0 1 2 2v2M10 11v6M14 11v6',
	};
	MENU_ICONS.rename = MENU_ICONS.edit;
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

	const iconOf = (item: MenuItem) => MENU_ICONS[item.icon ?? item.id];
	/** Every action item keeps the same indent once any of them has an icon. */
	let withIcons = $derived(items.some((i) => i.checked === undefined && iconOf(i)));

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
						{#if withIcons}
							<svg class="dd-icon" aria-hidden="true" fill="none" stroke="currentColor" viewBox="0 0 24 24">
								{#if iconOf(item)}<path stroke-linecap="round" stroke-linejoin="round" stroke-width="1.6" d={iconOf(item)} />{/if}
							</svg>
						{/if}
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
