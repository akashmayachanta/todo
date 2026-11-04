<script lang="ts" generics="T extends string | number">
	import { placePanel, placementStyle, type Placement } from '$lib/dropdown';
	import './Dropdown.css';

	type Option = T | { value: T; label: string };

	/**
	 * Themed replacement for the native `<select>`. `options` may be plain values or
	 * `{ value, label }` pairs. Keyboard: arrows, Home/End, Enter/Space, type-ahead, Esc.
	 */
	let { value = $bindable(), options, id, placeholder = 'Select…', disabled = false, ariaLabel, class: className = '', onchange }: {
		value?: T;
		options: Option[];
		id?: string;
		placeholder?: string;
		disabled?: boolean;
		ariaLabel?: string;
		/** Extra classes for the wrapper, e.g. to set a width. */
		class?: string;
		onchange?: (value: T) => void;
	} = $props();

	const uid = $props.id();

	let open = $state(false);
	let active = $state(0);
	let place = $state<Placement | null>(null);
	let root = $state<HTMLDivElement | null>(null);
	let trigger = $state<HTMLButtonElement | null>(null);
	let list = $state<HTMLDivElement | null>(null);
	let typed = '';
	let typedTimer: ReturnType<typeof setTimeout> | undefined;

	let items = $derived(options.map((o) => (typeof o === 'object' ? o : { value: o, label: String(o) })));
	let selectedIndex = $derived(items.findIndex((i) => i.value === value));
	let selected = $derived(items[selectedIndex]);

	function show() {
		if (disabled || !trigger) return;
		place = placePanel(trigger);
		active = Math.max(0, selectedIndex);
		open = true;
	}

	function close() { open = false; }

	function pick(i: number) {
		const item = items[i];
		if (!item) return;
		if (item.value !== value) {
			value = item.value;
			onchange?.(item.value);
		}
		close();
		trigger?.focus();
	}

	function move(to: number) {
		active = Math.min(items.length - 1, Math.max(0, to));
	}

	function typeahead(char: string) {
		clearTimeout(typedTimer);
		typed += char.toLowerCase();
		typedTimer = setTimeout(() => (typed = ''), 600);
		const order = [...items.keys()];
		// A repeated single letter cycles through the matches; a longer prefix narrows them.
		const from = typed.length === 1 ? active + 1 : active;
		const hit = [...order.slice(from), ...order.slice(0, from)].find((i) => items[i].label.toLowerCase().startsWith(typed));
		if (hit === undefined) return;
		if (open) active = hit; else pick(hit);
	}

	function onKeydown(e: KeyboardEvent) {
		if (disabled || e.altKey || e.ctrlKey || e.metaKey) return;
		if (!open) {
			if (['ArrowDown', 'ArrowUp', 'Enter', ' '].includes(e.key)) { e.preventDefault(); show(); }
			else if (e.key.length === 1) typeahead(e.key);
			return;
		}
		switch (e.key) {
			case 'ArrowDown': e.preventDefault(); move(active + 1); break;
			case 'ArrowUp':   e.preventDefault(); move(active - 1); break;
			case 'Home':      e.preventDefault(); move(0); break;
			case 'End':       e.preventDefault(); move(items.length - 1); break;
			case 'Enter':
			case ' ':         e.preventDefault(); pick(active); break;
			case 'Escape':    e.stopPropagation(); close(); break; // keep a surrounding modal open
			case 'Tab':       close(); break;
			default:          if (e.key.length === 1) typeahead(e.key);
		}
	}

	function onPointerDown(e: PointerEvent) {
		if (open && root && !e.composedPath().includes(root)) close();
	}

	// The panel is fixed to the viewport, so it must close rather than drift when the page moves.
	function onScroll(e: Event) {
		if (open && !(list && e.target instanceof Node && list.contains(e.target))) close();
	}

	$effect(() => {
		if (!open || !list) return;
		list.querySelector(`[data-index="${active}"]`)?.scrollIntoView({ block: 'nearest' });
	});
</script>

<svelte:window onpointerdown={onPointerDown} onscrollcapture={onScroll} onresize={close} />

<div class="dd {className}" bind:this={root}>
	<button
		{id}
		bind:this={trigger}
		type="button"
		role="combobox"
		class="input dd-trigger"
		aria-label={ariaLabel}
		aria-haspopup="listbox"
		aria-expanded={open}
		aria-controls="{uid}-list"
		aria-activedescendant={open ? `${uid}-opt-${active}` : undefined}
		{disabled}
		onclick={() => (open ? close() : show())}
		onkeydown={onKeydown}
	>
		<span class="dd-value" class:dd-placeholder={!selected}>{selected ? selected.label : placeholder}</span>
		<svg class="dd-caret" fill="none" stroke="currentColor" viewBox="0 0 24 24" aria-hidden="true"><path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M19 9l-7 7-7-7" /></svg>
	</button>

	{#if open && place}
		<div bind:this={list} id="{uid}-list" role="listbox" class="dd-panel" style={placementStyle(place)}>
			{#each items as item, i (item.value)}
				<!-- Focus stays on the trigger (aria-activedescendant), so options take pointer input only. -->
				<!-- svelte-ignore a11y_click_events_have_key_events -->
				<div
					id="{uid}-opt-{i}"
					role="option"
					class="dd-option"
					data-index={i}
					data-active={i === active}
					aria-selected={i === selectedIndex}
					tabindex="-1"
					onpointermove={() => (active = i)}
					onclick={() => pick(i)}
				>
					<span class="truncate">{item.label}</span>
					{#if i === selectedIndex}
						<svg class="dd-tick" fill="none" stroke="currentColor" viewBox="0 0 24 24" aria-hidden="true"><path stroke-linecap="round" stroke-linejoin="round" stroke-width="2.5" d="M5 13l4 4L19 7" /></svg>
					{/if}
				</div>
			{:else}
				<div class="dd-empty">No options</div>
			{/each}
		</div>
	{/if}
</div>
