<script lang="ts">
	import { onMount, untrack } from 'svelte';
	import type { Task } from '$lib/api';
	import { addDays, fromYmd, timeRange, ymd } from '$lib/taskParse';
	import { byDue } from './shared';

	/**
	 * The view's dated tasks by day, week or month (weeks start Monday). Each day has a + to
	 * add a task on it, and its number opens it in the day view; dragging a task onto a day
	 * moves it there. With `undated`, a "No date" tray under the calendar holds the rest:
	 * drag one onto a day to schedule it, or a day's task onto the tray to clear its date.
	 */
	let { tasks, undated = null, today, onopen, ontoggle, onmove, onaddat }: {
		/** Tasks with a due date. */
		tasks: Task[];
		/** Tasks without one, or null for no tray. */
		undated?: Task[] | null;
		today: string;
		onopen: (task: Task) => void;
		ontoggle: (task: Task) => void;
		/** A new date for the task, or null to clear it. */
		onmove: (task: Task, date: string | null) => void;
		onaddat: (date: string) => void;
	} = $props();

	type Span = 'day' | 'week' | 'month';
	const SPANS: { key: Span; label: string }[] = [
		{ key: 'day', label: 'Day' },
		{ key: 'week', label: 'Week' },
		{ key: 'month', label: 'Month' },
	];

	/** Day, week or month, remembered in this browser. */
	const SPAN_KEY = 'tasks-calendar-span';
	let span = $state<Span>('month');
	/** A day inside the range shown; it starts on today. */
	let anchor = $state(untrack(() => today));
	let dragging = $state<Task | null>(null);
	/** The day (or 'none' for the tray) a task is being dragged over. */
	let over = $state<string | null>(null);

	onMount(() => {
		try {
			const saved = localStorage.getItem(SPAN_KEY);
			if (saved === 'day' || saved === 'week' || saved === 'month') span = saved;
		} catch {
			/* storage blocked: month */
		}
	});

	function setSpan(next: Span) {
		span = next;
		try {
			localStorage.setItem(SPAN_KEY, next);
		} catch {
			/* storage blocked: it is forgotten on reload */
		}
	}

	/** Opens one day in the day view. */
	function openDay(date: string) {
		anchor = date;
		setSpan('day');
	}

	const monday = (date: string) => addDays(date, -((fromYmd(date).getDay() + 6) % 7));
	const firstOfMonth = (date: string) => `${date.slice(0, 7)}-01`;

	function shift(by: -1 | 1) {
		if (span === 'day') anchor = addDays(anchor, by);
		else if (span === 'week') anchor = addDays(anchor, 7 * by);
		else {
			const d = fromYmd(firstOfMonth(anchor));
			d.setMonth(d.getMonth() + by, 1);
			anchor = ymd(d);
		}
	}

	/** Whether today is in the range shown, so "Today" has nothing to do. */
	let showsToday = $derived(
		span === 'day' ? anchor === today
		: span === 'week' ? monday(anchor) === monday(today)
		: anchor.slice(0, 7) === today.slice(0, 7),
	);

	let title = $derived.by(() => {
		if (span === 'day') {
			return fromYmd(anchor).toLocaleDateString(undefined, { weekday: 'long', day: 'numeric', month: 'long', year: 'numeric' });
		}
		if (span === 'week') {
			const start = monday(anchor);
			const fmt = new Intl.DateTimeFormat(undefined, { day: 'numeric', month: 'short', year: 'numeric' });
			return fmt.formatRange(fromYmd(start), fromYmd(addDays(start, 6)));
		}
		return fromYmd(anchor).toLocaleDateString(undefined, { month: 'long', year: 'numeric' });
	});

	/**
	 * The days shown, in rows of a week (or one row of one day). A month runs in whole weeks,
	 * from the Monday on or before the 1st to the Sunday on or after the last day.
	 */
	let weeks = $derived.by(() => {
		if (span === 'day') return [[anchor]];
		const week = (from: string) => Array.from({ length: 7 }, (_, i) => addDays(from, i));
		if (span === 'week') return [week(monday(anchor))];
		const out: string[][] = [];
		const prefix = anchor.slice(0, 7);
		let day = monday(firstOfMonth(anchor));
		while (out.length === 0 || day.startsWith(prefix)) {
			out.push(week(day));
			day = addDays(day, 7);
		}
		return out;
	});

	let weekdays = $derived(weeks[0].map((d) => fromYmd(d).toLocaleDateString(undefined, { weekday: 'short' })));

	const longDate = (date: string) => fromYmd(date).toLocaleDateString(undefined, { weekday: 'long', day: 'numeric', month: 'long' });

	let byDate = $derived.by(() => {
		const map = new Map<string, Task[]>();
		for (const t of tasks) {
			if (!t.due_date) continue;
			const list = map.get(t.due_date) ?? [];
			list.push(t);
			map.set(t.due_date, list);
		}
		for (const list of map.values()) list.sort(byDue);
		return map;
	});

	function dragStart(e: DragEvent, task: Task) {
		dragging = task;
		e.dataTransfer?.setData('text/plain', String(task.id));
		if (e.dataTransfer) e.dataTransfer.effectAllowed = 'move';
	}

	function dragEnd() {
		dragging = null;
		over = null;
	}

	function dragOver(e: DragEvent, target: string) {
		if (!dragging) return;
		e.preventDefault();
		if (e.dataTransfer) e.dataTransfer.dropEffect = 'move';
		over = target;
	}

	function dragLeave(e: DragEvent, target: string) {
		const el = e.currentTarget as HTMLElement;
		if (over === target && !el.contains(e.relatedTarget as Node | null)) over = null;
	}

	function drop(e: DragEvent, date: string | null) {
		e.preventDefault();
		const task = dragging;
		dragEnd();
		if (task && task.due_date !== date) onmove(task, date);
	}
</script>

{#snippet chip(task: Task, past: boolean)}
	<div
		class="tcal-chip"
		class:tcal-chip-overdue={past}
		class:tcal-chip-dragging={dragging?.id === task.id}
		draggable="true"
		role="listitem"
		ondragstart={(e) => dragStart(e, task)}
		ondragend={dragEnd}
	>
		<button
			type="button"
			class="task-check task-check-sm"
			data-priority={task.priority}
			aria-pressed="false"
			aria-label={task.recurrence ? `Complete "${task.title}" (moves to its next date)` : `Complete "${task.title}"`}
			onclick={() => ontoggle(task)}
		>
			<svg aria-hidden="true" class="task-check-tick" fill="none" stroke="currentColor" viewBox="0 0 24 24"><path stroke-linecap="round" stroke-linejoin="round" stroke-width="3" d="M5 13l4 4L19 7" /></svg>
		</button>
		<button type="button" class="tcal-chip-title" onclick={() => onopen(task)} title={task.title}>
			{#if task.due_time}<span class="tcal-chip-time">{timeRange(task.due_time, task.duration_minutes)}</span>{/if}
			{task.title}{#if task.recurrence}<span class="tcal-chip-icon" aria-label="Repeats"> ↻</span>{/if}
		</button>
	</div>
{/snippet}

<div class="tcal">
	<div class="tcal-bar">
		<h2 class="tcal-title">{title}</h2>
		<div class="tcal-nav">
			<div class="tasks-layout" role="group" aria-label="Calendar range">
				{#each SPANS as sp (sp.key)}
					<button type="button" class="tasks-layout-btn" aria-pressed={span === sp.key} onclick={() => setSpan(sp.key)}>{sp.label}</button>
				{/each}
			</div>
			<button type="button" class="tasks-icon-btn" onclick={() => shift(-1)} aria-label="Previous {span}" title="Previous {span}">
				<svg aria-hidden="true" class="w-4 h-4" fill="none" stroke="currentColor" viewBox="0 0 24 24"><path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M15 19l-7-7 7-7" /></svg>
			</button>
			<button type="button" class="btn btn-ghost tcal-today" onclick={() => (anchor = today)} disabled={showsToday}>Today</button>
			<button type="button" class="tasks-icon-btn" onclick={() => shift(1)} aria-label="Next {span}" title="Next {span}">
				<svg aria-hidden="true" class="w-4 h-4" fill="none" stroke="currentColor" viewBox="0 0 24 24"><path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M9 5l7 7-7 7" /></svg>
			</button>
		</div>
	</div>

	<div class="tcal-grid tcal-grid-{span}">
		{#if span === 'month'}
			{#each weekdays as w}
				<div class="tcal-weekday">{w}</div>
			{/each}
		{/if}
		{#each weeks as week}
			{#each week as date (date)}
				{@const list = byDate.get(date) ?? []}
				{@const past = date < today}
				<div
					class="tcal-cell"
					class:tcal-cell-out={span === 'month' && !date.startsWith(anchor.slice(0, 7))}
					class:tcal-cell-today={date === today}
					class:tcal-cell-past={past}
					class:tcal-cell-over={over === date}
					role="list"
					aria-label={longDate(date)}
					ondragover={(e) => dragOver(e, date)}
					ondragleave={(e) => dragLeave(e, date)}
					ondrop={(e) => drop(e, date)}
				>
					<div class="tcal-cell-head">
						{#if span === 'day'}
							<span class="tcal-day">{fromYmd(date).getDate()}</span>
						{:else}
							<span class="tcal-day-wrap">
								{#if span === 'week'}<span class="tcal-weekday-inline">{fromYmd(date).toLocaleDateString(undefined, { weekday: 'short' })}</span>{/if}
								<button type="button" class="tcal-day" onclick={() => openDay(date)} title="Open {longDate(date)}" aria-label="Open {longDate(date)}">{fromYmd(date).getDate()}</button>
							</span>
						{/if}
						<button
							type="button"
							class="tasks-icon-btn tcal-add"
							onclick={() => onaddat(date)}
							aria-label="Add task on {fromYmd(date).toLocaleDateString(undefined, { day: 'numeric', month: 'long' })}"
							title="Add task"
						>
							<svg aria-hidden="true" class="w-3.5 h-3.5" fill="none" stroke="currentColor" viewBox="0 0 24 24"><path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M12 5v14m7-7H5" /></svg>
						</button>
					</div>
					{#each list as task (task.id)}
						{@render chip(task, past)}
					{:else}
						{#if span === 'day'}<p class="tcal-day-empty">Nothing due. Use + or drag a task here.</p>{/if}
					{/each}
				</div>
			{/each}
		{/each}
	</div>

	{#if undated}
		<section
			class="tcal-tray"
			class:tcal-cell-over={over === 'none'}
			ondragover={(e) => dragOver(e, 'none')}
			ondragleave={(e) => dragLeave(e, 'none')}
			ondrop={(e) => drop(e, null)}
			aria-label="Tasks without a date"
		>
			<h2 class="task-group-head">No date <span class="task-group-count">{undated.length}</span></h2>
			<div class="tcal-tray-list" role="list">
				{#each [...undated].sort(byDue) as task (task.id)}
					{@render chip(task, false)}
				{:else}
					<p class="tcal-tray-empty">Every task here has a date. Drag one here to clear it.</p>
				{/each}
			</div>
		</section>
	{/if}
</div>
