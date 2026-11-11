<script lang="ts">
	import { Calendar } from 'vanilla-calendar-pro';
	import { modeStore } from '$lib/stores/theme';
	import { datePrefs } from '$lib/datePrefs.svelte';
	import 'vanilla-calendar-pro/styles/layout.css';
	import 'vanilla-calendar-pro/styles/themes/light.css';
	import 'vanilla-calendar-pro/styles/themes/dark.css';
	import './DatePicker.css';

	/** `value` is a `YYYY-MM-DD` string, or '' when no date is chosen. */
	let { value = $bindable(''), id, placeholder = 'Select date', onchange }: {
		value?: string;
		id?: string;
		placeholder?: string;
		/** Called when a date is picked or cleared. */
		onchange?: (value: string) => void;
	} = $props();

	let open = $state(false);
	let root = $state<HTMLDivElement | null>(null);
	let host = $state<HTMLDivElement | null>(null);

	function parse(v: string) {
		const [y, m, d] = v.split('-').map(Number);
		return y && m && d ? new Date(y, m - 1, d) : null;
	}

	let label = $derived.by(() => {
		const d = parse(value);
		return d ? d.toLocaleDateString(undefined, { day: 'numeric', month: 'short', year: 'numeric' }) : '';
	});

	function close() { open = false; }

	function pick(v: string) {
		const changed = v !== value;
		value = v;
		close();
		if (changed) onchange?.(v);
	}

	function today() {
		const n = new Date();
		const pad = (x: number) => String(x).padStart(2, '0');
		pick(`${n.getFullYear()}-${pad(n.getMonth() + 1)}-${pad(n.getDate())}`);
	}

	// The calendar is built fresh on each open so it always starts on the selected month.
	$effect(() => {
		if (!open || !host) return;
		const current = parse(value) ?? new Date();
		const cal = new Calendar(host, {
			selectedTheme: $modeStore,
			selectionDatesMode: 'single',
			selectedDates: value ? [value] : [],
			selectedMonth: current.getMonth() as Calendar['selectedMonth'],
			selectedYear: current.getFullYear(),
			firstWeekday: datePrefs.weekStart as Calendar['firstWeekday'],
			selectedWeekends: [],
			onClickDate(self) {
				const [picked] = self.context.selectedDates;
				if (picked) pick(picked);
			},
		});
		cal.init();
		return () => cal.destroy();
	});

	function onKeydown(e: KeyboardEvent) {
		if (e.key === 'Escape' && open) {
			e.stopPropagation(); // keep the surrounding modal open
			close();
		}
	}

	function onPointerDown(e: PointerEvent) {
		if (open && root && !e.composedPath().includes(root)) close();
	}
</script>

<svelte:window onpointerdown={onPointerDown} />

<div class="date-picker" bind:this={root} onkeydown={onKeydown} role="presentation">
	<button {id} type="button" class="input date-picker-trigger" aria-haspopup="dialog" aria-expanded={open}
		onclick={() => (open = !open)}>
		<svg aria-hidden="true" class="w-4 h-4 shrink-0" fill="none" stroke="currentColor" viewBox="0 0 24 24"><path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M8 7V3m8 4V3m-9 8h10M5 21h14a2 2 0 002-2V7a2 2 0 00-2-2H5a2 2 0 00-2 2v12a2 2 0 002 2z" /></svg>
		<span class:date-picker-placeholder={!label}>{label || placeholder}</span>
	</button>

	{#if open}
		<div class="date-picker-pop">
			<div bind:this={host}></div>
			<div class="date-picker-footer">
				<button type="button" class="date-picker-link" onclick={today}>Today</button>
				<button type="button" class="date-picker-link" onclick={() => pick('')} disabled={!value}>Clear</button>
			</div>
		</div>
	{/if}
</div>
