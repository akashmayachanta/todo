<script lang="ts">
	import { tick } from 'svelte';
	import { datePrefs } from '$lib/datePrefs.svelte';
	import { addDays, dayLabel, formatTime, fromYmd, parseQuickAdd, ymd } from '$lib/taskParse';

	/**
	 * The task view's Date field, after Todoist's: a heading with a + (or the date
	 * set, with × to clear), opening a panel with a "Type a date" box read like
	 * quick add, Today / Tomorrow / This weekend / Next week, a month calendar, and
	 * Time and Repeat. Dates are local `YYYY-MM-DD`, times `HH:MM`.
	 */
	let {
		date,
		time,
		recurrence,
		recurrenceText = $bindable(''),
		recurrenceError = '',
		ondate,
		ontime,
		onrecurrence,
	}: {
		date: string | null;
		time: string | null;
		recurrence: string | null;
		/** The Repeat box's text (TaskDetail keeps it, and checks it in `onrecurrence`). */
		recurrenceText?: string;
		recurrenceError?: string;
		/** A date picked, or '' to clear it (and its time). */
		ondate: (value: string) => void;
		/** A time picked, or '' to clear it. A time with no date puts it on today. */
		ontime: (value: string) => void;
		/** Reads and saves the Repeat box. */
		onrecurrence: () => void;
	} = $props();

	let open = $state(false);
	let root = $state<HTMLDivElement | null>(null);
	let typed = $state('');
	let typedBox = $state<HTMLInputElement | null>(null);
	let timeShown = $state(false);
	let repeatShown = $state(false);

	const today = $derived(ymd(new Date()));
	/** The month on show: its first day. */
	let month = $state(firstOfMonth(ymd(new Date())));

	function firstOfMonth(d: string) {
		return `${d.slice(0, 7)}-01`;
	}

	/** The next `weekday` (0 = Sunday) after today; a week on when today is that day. */
	function nextWeekday(weekday: number) {
		const now = new Date();
		const ahead = (weekday - now.getDay() + 7) % 7 || 7;
		return addDays(today, ahead);
	}

	/** This weekend: today when it's that day, else the coming one. */
	function weekendDay() {
		const now = new Date();
		return now.getDay() === datePrefs.weekend ? today : nextWeekday(datePrefs.weekend);
	}

	const weekday = (d: string) => fromYmd(d).toLocaleDateString(undefined, { weekday: 'short' });
	const longDay = (d: string) => fromYmd(d).toLocaleDateString(undefined, { weekday: 'short', day: 'numeric', month: 'short' }).replace(/,/g, '');

	let picks = $derived([
		{ key: 'today', label: 'Today', date: today, hint: weekday(today) },
		{ key: 'tomorrow', label: 'Tomorrow', date: addDays(today, 1), hint: weekday(addDays(today, 1)) },
		{ key: 'weekend', label: 'This weekend', date: weekendDay(), hint: weekday(weekendDay()) },
		{ key: 'nextweek', label: 'Next week', date: nextWeekday(datePrefs.nextWeek), hint: longDay(nextWeekday(datePrefs.nextWeek)) },
	]);

	/** What the typed text reads as, if it names a date or a time. */
	let reading = $derived.by(() => {
		if (!typed.trim()) return null;
		const p = parseQuickAdd(typed, [], [], new Date());
		if (!p.due_date && !p.due_time && !p.recurrence) return null;
		return { date: p.due_date ?? (p.due_time ? today : null), time: p.due_time, recurrence: p.recurrence };
	});

	/** The weeks of the month on show, each a row of 7 (null = a day of another month). */
	let weeks = $derived.by(() => {
		const first = fromYmd(month);
		const lead = (first.getDay() - datePrefs.weekStart + 7) % 7;
		const days = new Date(first.getFullYear(), first.getMonth() + 1, 0).getDate();
		const cells: (string | null)[] = Array(lead).fill(null);
		for (let i = 0; i < days; i++) cells.push(addDays(month, i));
		while (cells.length % 7) cells.push(null);
		const out: (string | null)[][] = [];
		for (let i = 0; i < cells.length; i += 7) out.push(cells.slice(i, i + 7));
		return out;
	});

	let weekdayNames = $derived.by(() => {
		const sunday = fromYmd('2023-01-01'); // a Sunday
		return Array.from({ length: 7 }, (_, i) => {
			const d = new Date(sunday);
			d.setDate(1 + ((datePrefs.weekStart + i) % 7));
			return d.toLocaleDateString(undefined, { weekday: 'narrow' });
		});
	});

	let monthTitle = $derived(fromYmd(month).toLocaleDateString(undefined, { month: 'short', year: 'numeric' }));
	let atThisMonth = $derived(month === firstOfMonth(today));

	function stepMonth(by: number) {
		const d = fromYmd(month);
		month = ymd(new Date(d.getFullYear(), d.getMonth() + by, 1));
	}

	/** What the field shows when a date is set: "Today 5:00 pm". */
	let summary = $derived(date ? `${dayLabel(date)}${time ? ` ${formatTime(time)}` : ''}` : time ? formatTime(time) : '');

	async function show() {
		open = true;
		typed = '';
		timeShown = !!time;
		repeatShown = !!recurrence;
		month = firstOfMonth(date ?? today);
		await tick();
		typedBox?.focus();
	}

	function close() {
		open = false;
	}

	function pick(d: string) {
		if (d !== date) ondate(d);
		close();
	}

	function applyTyped() {
		if (!reading) return;
		if (reading.date && reading.date !== date) ondate(reading.date);
		if (reading.time) ontime(reading.time);
		if (reading.recurrence) {
			recurrenceText = reading.recurrence;
			onrecurrence();
		}
		close();
	}

	function onKeydown(e: KeyboardEvent) {
		if (e.key === 'Escape' && open) {
			e.preventDefault();
			e.stopPropagation(); // keep the task view open
			close();
		}
	}

	function onPointerDown(e: PointerEvent) {
		if (open && root && !e.composedPath().includes(root)) close();
	}
</script>

<svelte:window onpointerdown={onPointerDown} />

<div class="due" bind:this={root} onkeydown={onKeydown} role="presentation">
	<div class="due-head">
		<button type="button" class="due-trigger" aria-haspopup="dialog" aria-expanded={open} onclick={() => (open ? close() : show())}>
			<span class="task-side-label">Date</span>
			{#if !date && !time}
				<svg aria-hidden="true" class="due-plus" fill="none" stroke="currentColor" viewBox="0 0 24 24"><path stroke-linecap="round" stroke-width="2" d="M12 5v14m7-7H5" /></svg>
			{/if}
		</button>
	</div>
	{#if date || time}
		<div class="due-value">
			<button type="button" class="due-value-open" data-when={!date ? 'later' : date < today ? 'past' : date === today ? 'today' : 'later'} onclick={show}>
				<svg aria-hidden="true" class="due-value-icon" fill="none" stroke="currentColor" viewBox="0 0 24 24"><rect x="4" y="5" width="16" height="15" rx="2" stroke-width="1.75" /><path stroke-linecap="round" stroke-width="1.75" d="M4 10h16M9 3v4m6-4v4" /></svg>
				<span>{summary}</span>
				{#if recurrence}<span class="due-value-repeat" title={recurrence}>↻</span>{/if}
			</button>
			<button type="button" class="due-clear" aria-label="Remove the date" title="Remove the date" onclick={() => ondate('')}>
				<svg aria-hidden="true" fill="none" stroke="currentColor" viewBox="0 0 24 24"><path stroke-linecap="round" stroke-width="2" d="M6 6l12 12M18 6L6 18" /></svg>
			</button>
		</div>
	{/if}

	{#if open}
		<div class="due-pop" role="dialog" aria-label="Date">
			<div class="due-typed">
				<input
					bind:this={typedBox}
					bind:value={typed}
					class="due-typed-input"
					placeholder="Type a date"
					aria-label="Type a date"
					onkeydown={(e) => { if (e.key === 'Enter') { e.preventDefault(); applyTyped(); } }}
				/>
				{#if reading}
					<button type="button" class="due-reading" onclick={applyTyped}>
						{reading.date ? longDay(reading.date) : ''}{reading.time ? ` ${formatTime(reading.time)}` : ''}{reading.recurrence ? ` · ${reading.recurrence}` : ''}
					</button>
				{/if}
			</div>

			<ul class="due-picks">
				{#each picks as p (p.key)}
					<li>
						<button type="button" class="due-pick" data-kind={p.key} onclick={() => pick(p.date)}>
							{#if p.key === 'today'}
								<svg aria-hidden="true" class="due-pick-icon" viewBox="0 0 24 24" fill="none" stroke="currentColor"><rect x="4" y="4" width="16" height="16" rx="2" stroke-width="1.75" /><text x="12" y="16.5" text-anchor="middle" font-size="9" font-weight="700" fill="currentColor" stroke="none">{fromYmd(today).getDate()}</text></svg>
							{:else if p.key === 'tomorrow'}
								<svg aria-hidden="true" class="due-pick-icon" viewBox="0 0 24 24" fill="none" stroke="currentColor"><circle cx="12" cy="12" r="4" stroke-width="1.75" /><path stroke-linecap="round" stroke-width="1.75" d="M12 2v2m0 16v2M4.9 4.9l1.4 1.4m11.4 11.4l1.4 1.4M2 12h2m16 0h2M4.9 19.1l1.4-1.4M17.7 6.3l1.4-1.4" /></svg>
							{:else if p.key === 'weekend'}
								<svg aria-hidden="true" class="due-pick-icon" viewBox="0 0 24 24" fill="none" stroke="currentColor"><path stroke-linecap="round" stroke-linejoin="round" stroke-width="1.75" d="M5 11V8a2 2 0 012-2h10a2 2 0 012 2v3M3 13a2 2 0 014 0v1h10v-1a2 2 0 014 0v4a1 1 0 01-1 1H4a1 1 0 01-1-1v-4zM6 18v2m12-2v2" /></svg>
							{:else}
								<svg aria-hidden="true" class="due-pick-icon" viewBox="0 0 24 24" fill="none" stroke="currentColor"><rect x="4" y="4" width="16" height="16" rx="2" stroke-width="1.75" /><path stroke-linecap="round" stroke-linejoin="round" stroke-width="1.75" d="M9 12h6m-2.5-2.5L15 12l-2.5 2.5" /></svg>
							{/if}
							<span class="due-pick-label">{p.label}</span>
							<span class="due-pick-hint">{p.hint}</span>
						</button>
					</li>
				{/each}
				{#if date}
					<li>
						<button type="button" class="due-pick" data-kind="none" onclick={() => { ondate(''); close(); }}>
							<svg aria-hidden="true" class="due-pick-icon" viewBox="0 0 24 24" fill="none" stroke="currentColor"><circle cx="12" cy="12" r="8" stroke-width="1.75" /><path stroke-linecap="round" stroke-width="1.75" d="M6.5 17.5l11-11" /></svg>
							<span class="due-pick-label">No date</span>
						</button>
					</li>
				{/if}
			</ul>

			<div class="due-cal">
				<div class="due-cal-head">
					<span class="due-cal-title">{monthTitle}</span>
					<div class="due-cal-nav">
						<button type="button" aria-label="Previous month" disabled={atThisMonth} onclick={() => stepMonth(-1)}>
							<svg aria-hidden="true" fill="none" stroke="currentColor" viewBox="0 0 24 24"><path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M15 18l-6-6 6-6" /></svg>
						</button>
						<button type="button" aria-label="This month" disabled={atThisMonth} onclick={() => (month = firstOfMonth(today))}>
							<svg aria-hidden="true" fill="none" stroke="currentColor" viewBox="0 0 24 24"><circle cx="12" cy="12" r="3.5" stroke-width="2" /></svg>
						</button>
						<button type="button" aria-label="Next month" onclick={() => stepMonth(1)}>
							<svg aria-hidden="true" fill="none" stroke="currentColor" viewBox="0 0 24 24"><path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M9 6l6 6-6 6" /></svg>
						</button>
					</div>
				</div>
				<div class="due-cal-grid" role="grid" aria-label={monthTitle}>
					<div class="due-cal-row" role="row">
						{#each weekdayNames as w, i (i)}<span class="due-cal-wd" role="columnheader">{w}</span>{/each}
					</div>
					{#each weeks as week, wi (wi)}
						<div class="due-cal-row" role="row">
							{#each week as d, di (di)}
								{#if d}
									<button
										type="button"
										role="gridcell"
										class="due-cal-day"
										class:due-cal-past={d < today}
										class:due-cal-today={d === today}
										aria-selected={d === date}
										aria-label={fromYmd(d).toLocaleDateString(undefined, { weekday: 'long', day: 'numeric', month: 'long' })}
										onclick={() => pick(d)}
									>{fromYmd(d).getDate()}</button>
								{:else}
									<span class="due-cal-blank" role="gridcell"></span>
								{/if}
							{/each}
						</div>
					{/each}
				</div>
			</div>

			<div class="due-foot">
				{#if timeShown}
					<div class="due-foot-row">
						<input
							type="time"
							class="input due-time"
							aria-label="Time"
							value={time ?? ''}
							title={date ? undefined : 'A time with no date sets the date to today'}
							onchange={(e) => ontime(e.currentTarget.value)}
						/>
						{#if time}
							<button type="button" class="due-clear" aria-label="Remove the time" title="Remove the time" onclick={() => ontime('')}>
								<svg aria-hidden="true" fill="none" stroke="currentColor" viewBox="0 0 24 24"><path stroke-linecap="round" stroke-width="2" d="M6 6l12 12M18 6L6 18" /></svg>
							</button>
						{/if}
					</div>
				{:else}
					<button type="button" class="due-foot-btn" onclick={() => (timeShown = true)}>
						<svg aria-hidden="true" fill="none" stroke="currentColor" viewBox="0 0 24 24"><circle cx="12" cy="12" r="8" stroke-width="1.75" /><path stroke-linecap="round" stroke-width="1.75" d="M12 8v4l2.5 2" /></svg>
						Time
					</button>
				{/if}
				{#if repeatShown}
					<input
						class="input due-repeat"
						bind:value={recurrenceText}
						placeholder="e.g. every monday"
						aria-label="Repeat"
						onblur={onrecurrence}
						onkeydown={(e) => { if (e.key === 'Enter') { e.preventDefault(); onrecurrence(); } }}
					/>
					{#if recurrenceError}<p class="task-side-error">{recurrenceError}</p>{/if}
				{:else}
					<button type="button" class="due-foot-btn" onclick={() => (repeatShown = true)}>
						<svg aria-hidden="true" fill="none" stroke="currentColor" viewBox="0 0 24 24"><path stroke-linecap="round" stroke-linejoin="round" stroke-width="1.75" d="M17 2l3 3-3 3M4 11V9a4 4 0 014-4h12M7 22l-3-3 3-3m13-3v2a4 4 0 01-4 4H4" /></svg>
						Repeat
					</button>
				{/if}
			</div>
		</div>
	{/if}
</div>

<style>
	.due {
		position: relative;
	}

	.due-head {
		display: flex;
	}

	.due-trigger {
		display: flex;
		flex: 1;
		align-items: center;
		justify-content: space-between;
		margin: -0.25rem -0.375rem;
		padding: 0.25rem 0.375rem;
		border-radius: var(--radius-control);
		color: var(--text-muted);
	}

	.due-trigger:hover,
	.due-trigger[aria-expanded='true'] {
		background-color: var(--bg-hover);
	}

	.due-plus {
		width: 1rem;
		height: 1rem;
	}

	.due-value {
		display: flex;
		align-items: center;
		gap: 0.25rem;
		margin-top: 0.375rem;
	}

	.due-value-open {
		display: inline-flex;
		flex: 1;
		min-width: 0;
		align-items: center;
		gap: 0.375rem;
		margin-left: -0.375rem;
		padding: 0.125rem 0.375rem;
		border-radius: var(--radius-control);
		font-size: var(--text-sm);
		color: var(--text-primary);
		text-align: left;
	}

	.due-value-open[data-when='today'] { color: var(--color-success); }
	.due-value-open[data-when='past'] { color: var(--color-danger); }

	.due-value-open:hover {
		background-color: var(--bg-hover);
	}

	.due-value-icon {
		flex-shrink: 0;
		width: 0.875rem;
		height: 0.875rem;
	}

	.due-value-repeat {
		color: var(--text-muted);
	}

	.due-clear {
		display: inline-flex;
		flex-shrink: 0;
		align-items: center;
		justify-content: center;
		width: 1.5rem;
		height: 1.5rem;
		border-radius: var(--radius-control);
		color: var(--text-faint);
	}

	.due-clear:hover {
		color: var(--text-primary);
		background-color: var(--bg-hover);
	}

	.due-clear svg {
		width: 0.875rem;
		height: 0.875rem;
	}

	/* The panel */
	.due-pop {
		position: absolute;
		top: calc(100% + 0.375rem);
		right: -0.5rem;
		z-index: var(--z-dropdown);
		width: 16rem;
		max-height: min(36rem, 82vh);
		overflow-y: auto;
		border: var(--rule-hair) solid var(--clr-border-light);
		border-radius: 12px;
		background-color: var(--bg-card);
		box-shadow: 0 12px 32px color-mix(in oklab, var(--color-scrim) 22%, transparent);
		font-family: var(--font-body);
	}

	.due-typed {
		padding: 0.5rem 0.75rem;
		border-bottom: var(--rule-hair) solid var(--clr-border);
	}

	.due-typed-input {
		width: 100%;
		padding: 0.125rem 0;
		border: 0;
		background: transparent;
		color: var(--text-primary);
		font-size: var(--text-sm);
		outline: none;
		box-shadow: none;
	}

	.due-typed-input:focus {
		outline: none;
		box-shadow: none;
	}

	.due-typed-input::placeholder {
		color: var(--text-faint);
	}

	.due-reading {
		display: block;
		width: 100%;
		margin-top: 0.25rem;
		padding: 0.25rem 0.375rem;
		border-radius: var(--radius-control);
		font-size: var(--text-xs);
		color: var(--color-brand);
		background-color: var(--color-brand-glow);
		text-align: left;
	}

	.due-picks {
		margin: 0;
		padding: 0.25rem 0;
		list-style: none;
		border-bottom: var(--rule-hair) solid var(--clr-border);
	}

	.due-pick {
		display: flex;
		width: 100%;
		align-items: center;
		gap: 0.75rem;
		padding: 0.4375rem 0.75rem;
		font-size: var(--text-sm);
		color: var(--text-primary);
		text-align: left;
	}

	.due-pick:hover {
		background-color: var(--bg-hover);
	}

	.due-pick-icon {
		flex-shrink: 0;
		width: 1.125rem;
		height: 1.125rem;
	}

	.due-pick[data-kind='today'] .due-pick-icon { color: oklch(55% 0.15 145); }
	.due-pick[data-kind='tomorrow'] .due-pick-icon { color: oklch(68% 0.16 60); }
	.due-pick[data-kind='weekend'] .due-pick-icon { color: oklch(55% 0.15 250); }
	.due-pick[data-kind='nextweek'] .due-pick-icon { color: oklch(52% 0.2 295); }
	.due-pick[data-kind='none'] .due-pick-icon { color: var(--text-muted); }

	.due-pick-label {
		flex: 1;
		font-weight: 600;
	}

	.due-pick-hint {
		font-size: var(--text-xs);
		color: var(--text-muted);
	}

	/* The month */
	.due-cal {
		padding: 0.5rem 0.75rem 0.25rem;
		border-bottom: var(--rule-hair) solid var(--clr-border);
	}

	.due-cal-head {
		display: flex;
		align-items: center;
		justify-content: space-between;
		margin-bottom: 0.375rem;
	}

	.due-cal-title {
		font-size: var(--text-sm);
		font-weight: 700;
		color: var(--text-primary);
	}

	.due-cal-nav {
		display: flex;
		gap: 0.125rem;
	}

	.due-cal-nav button {
		display: inline-flex;
		align-items: center;
		justify-content: center;
		width: 1.5rem;
		height: 1.5rem;
		border-radius: var(--radius-control);
		color: var(--text-muted);
	}

	.due-cal-nav button:hover:not(:disabled) {
		color: var(--text-primary);
		background-color: var(--bg-hover);
	}

	.due-cal-nav button:disabled {
		opacity: 0.35;
	}

	.due-cal-nav svg {
		width: 0.875rem;
		height: 0.875rem;
	}

	.due-cal-row {
		display: grid;
		grid-template-columns: repeat(7, 1fr);
		justify-items: center;
	}

	.due-cal-wd {
		padding: 0.125rem 0;
		font-size: 0.6875rem;
		color: var(--text-muted);
	}

	.due-cal-day,
	.due-cal-blank {
		width: 1.875rem;
		height: 1.875rem;
	}

	.due-cal-day {
		display: inline-flex;
		align-items: center;
		justify-content: center;
		margin: 0.0625rem 0;
		border-radius: 9999px;
		corner-shape: round;
		font-size: 0.8125rem;
		color: var(--text-primary);
	}

	.due-cal-day:hover {
		background-color: var(--bg-hover);
	}

	.due-cal-past {
		color: var(--text-faint);
	}

	.due-cal-today {
		font-weight: 700;
		color: var(--color-danger);
	}

	.due-cal-day[aria-selected='true'] {
		font-weight: 700;
		color: var(--color-brand-text);
		background-color: var(--color-brand);
	}

	/* Time and Repeat */
	/* Time and Repeat side by side in one short row; an open box takes the full width. */
	.due-foot {
		display: flex;
		flex-wrap: wrap;
		gap: 0.375rem;
		padding: 0.5rem 0.75rem;
	}

	.due-foot > * {
		flex: 1 1 100%;
	}

	.due-foot > .due-foot-btn {
		flex: 1 1 0;
	}

	.due-foot-row {
		display: flex;
		align-items: center;
		gap: 0.25rem;
	}

	.due-foot-btn {
		display: inline-flex;
		align-items: center;
		justify-content: center;
		gap: 0.3125rem;
		padding: 0.25rem 0.5rem;
		border: var(--rule-hair) solid var(--clr-border-light);
		border-radius: var(--radius-control);
		font-size: var(--text-xs);
		color: var(--text-secondary);
	}

	.due-foot-btn:hover {
		color: var(--text-primary);
		background-color: var(--bg-hover);
	}

	.due-foot-btn svg {
		width: 0.8125rem;
		height: 0.8125rem;
	}

	.due-time,
	.due-repeat {
		width: 100%;
	}
</style>
