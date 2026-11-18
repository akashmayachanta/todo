<script lang="ts">
	import { onMount } from 'svelte';
	import { api, errorDetail, type Task, type TaskProject } from '$lib/api';
	import { onTasksChanged } from '$lib/tasks/client';
	import { plainText } from '$lib/links';
	import { addDays, fromYmd, ymd } from '$lib/taskParse';
	import Avatar from '$shell/Avatar.svelte';
	import { me } from '$shell/profile';
	import PlaceMark from './PlaceMark.svelte';
	import { projectColor } from './shared';
	import './reporting.css';

	/**
	 * Reporting: every tick in the completion log (each occurrence of a repeating task
	 * too), newest first, grouped by local day, with the task and when it was ticked.
	 * Filtered by project and by how far back. Titles come from the done tasks, the open
	 * ones (a repeating task stays open) and, failing those, a fetch by id.
	 */
	let { today, projects, tasks, onopen }: {
		today: string;
		projects: TaskProject[];
		/** The page's open tasks: it reloads when they change. */
		tasks: Task[];
		onopen: (task: Task) => void;
	} = $props();

	const RANGES = [
		{ key: 'any', label: 'Any date', days: null },
		{ key: 'today', label: 'Today', days: 0 },
		{ key: '7', label: 'Last 7 days', days: 6 },
		{ key: '30', label: 'Last 30 days', days: 29 },
		{ key: '90', label: 'Last 90 days', days: 89 },
	] as const;
	type RangeKey = (typeof RANGES)[number]['key'];
	const RANGE_KEY = 'tasks-reporting-range';
	const PROJECT_KEY = 'tasks-reporting-project';
	/** Rows shown at first; "Show more" adds this many again. */
	const PAGE = 200;

	interface Row { key: string; at: Date; task: Task | null }

	let rows = $state<Row[] | null>(null);
	let error = $state('');
	let range = $state<RangeKey>('any');
	/** '' all, 'inbox', or a project id. */
	let projectPick = $state('');
	let shown = $state(PAGE);

	onMount(() => {
		try {
			const r = localStorage.getItem(RANGE_KEY);
			if (RANGES.some((x) => x.key === r)) range = r as RangeKey;
			projectPick = localStorage.getItem(PROJECT_KEY) ?? '';
		} catch {
			/* storage blocked: the defaults */
		}
		return onTasksChanged(() => load());
	});

	function remember(key: string, value: string) {
		try { localStorage.setItem(key, value); } catch { /* not remembered */ }
	}

	let since = $derived.by(() => {
		const days = RANGES.find((r) => r.key === range)?.days;
		return days == null ? new Date(0).toISOString() : fromYmd(addDays(today, -days)).toISOString();
	});

	let loadNo = 0;
	async function load() {
		const n = ++loadNo;
		try {
			const [log, done] = await Promise.all([
				api.tasks.completions(since),
				api.tasks.list({ completed: true, limit: 1000 }),
			]);
			const byId = new Map<string, Task>();
			for (const t of [...done, ...tasks]) byId.set(t.id, t);
			const missing = [...new Set(log.items.map((c) => c.task_id).filter((id): id is string => !!id && !byId.has(id)))];
			const fetched = await Promise.allSettled(missing.map((id) => api.tasks.get(id)));
			for (const f of fetched) if (f.status === 'fulfilled') byId.set(f.value.id, f.value);
			if (n !== loadNo) return;
			rows = log.items
				.map((c, i) => ({ key: `${c.completed_at}-${i}`, at: new Date(c.completed_at), task: c.task_id ? byId.get(c.task_id) ?? null : null }))
				.reverse();
			error = '';
		} catch (e) {
			if (n === loadNo) error = errorDetail(e);
		}
	}

	// Again when the range changes, or the page's tasks do (a tick, an edit).
	$effect(() => {
		void since;
		void tasks;
		load();
	});

	let projectById = $derived(new Map(projects.map((p) => [p.id, p])));

	let filtered = $derived(
		(rows ?? []).filter((r) => {
			if (!projectPick) return true;
			if (!r.task) return false;
			return projectPick === 'inbox' ? !r.task.project_id : r.task.project_id === projectPick;
		}),
	);

	let groups = $derived.by(() => {
		const out: { day: string; rows: Row[] }[] = [];
		for (const r of filtered.slice(0, shown)) {
			const day = ymd(r.at);
			const last = out[out.length - 1];
			if (last?.day === day) last.rows.push(r);
			else out.push({ day, rows: [r] });
		}
		return out;
	});

	/** Day headings count every row of the day, not only those shown so far. */
	let perDay = $derived.by(() => {
		const map = new Map<string, number>();
		for (const r of filtered) map.set(ymd(r.at), (map.get(ymd(r.at)) ?? 0) + 1);
		return map;
	});

	function dayLabel(day: string) {
		const d = fromYmd(day);
		const date = d.toLocaleDateString(undefined, { day: 'numeric', month: 'short', year: day.slice(0, 4) === today.slice(0, 4) ? undefined : 'numeric' });
		const weekday = d.toLocaleDateString(undefined, { weekday: 'long' });
		const rel = day === today ? 'Today' : day === addDays(today, -1) ? 'Yesterday' : '';
		return [date, rel, weekday].filter(Boolean).join(' · ');
	}

	const rtf = new Intl.RelativeTimeFormat(undefined, { numeric: 'auto' });
	/** Within a day: "18 hours ago"; older: the date and time. */
	function when(at: Date) {
		const mins = Math.round((Date.now() - at.getTime()) / 60_000);
		if (mins < 1) return 'just now';
		if (mins < 60) return rtf.format(-mins, 'minute');
		if (mins < 24 * 60) return rtf.format(-Math.round(mins / 60), 'hour');
		return at.toLocaleString(undefined, { day: 'numeric', month: 'short', year: 'numeric', hour: '2-digit', minute: '2-digit' });
	}

	function place(task: Task | null) {
		if (!task) return null;
		const p = task.project_id ? projectById.get(task.project_id) : null;
		return p ? { name: p.name, color: projectColor(p.color) } : { name: 'Inbox', color: null };
	}
</script>

<div class="report">
	<div class="report-filters">
		<label class="report-filter">
			<span class="sr-only">Project</span>
			<select class="input" bind:value={projectPick} onchange={() => { shown = PAGE; remember(PROJECT_KEY, projectPick); }}>
				<option value="">All projects</option>
				<option value="inbox">Inbox</option>
				{#each projects as p (p.id)}<option value={p.id}>{p.name}</option>{/each}
			</select>
		</label>
		<label class="report-filter">
			<span class="sr-only">Date</span>
			<select class="input" bind:value={range} onchange={() => { shown = PAGE; remember(RANGE_KEY, range); }}>
				{#each RANGES as r (r.key)}<option value={r.key}>{r.label}</option>{/each}
			</select>
		</label>
	</div>

	{#if error}
		<div class="error-box">Couldn't load completed tasks: {error}</div>
	{:else if rows === null}
		<div class="flex items-center gap-3 text-[var(--text-muted)]"><div class="spinner"></div>Loading…</div>
	{:else if !filtered.length}
		<p class="report-empty">No completed tasks {range === 'any' ? 'yet' : 'in this period'}.</p>
	{:else}
		{#each groups as g (g.day)}
			<section class="report-day" aria-label={dayLabel(g.day)}>
				<h2 class="report-day-head">{dayLabel(g.day)} <span class="report-day-count">{perDay.get(g.day)}</span></h2>
				<ul class="report-list">
					{#each g.rows as r (r.key)}
						{@const pl = place(r.task)}
						<li class="report-row">
							<span class="report-who">
								<Avatar name={$me} size={26} />
								<span class="report-badge" aria-hidden="true">
									<svg viewBox="0 0 16 16" fill="none" stroke="currentColor" stroke-width="2.5"><path stroke-linecap="round" stroke-linejoin="round" d="M4.5 8.5l2.25 2.25L11.5 6" /></svg>
								</span>
							</span>
							<span class="report-verb">You completed</span>
							{#if r.task}
								<button type="button" class="report-chip" data-priority={r.task.priority} onclick={() => onopen(r.task!)} title="Open task">
									<span class="report-chip-check" aria-hidden="true"></span>
									<span class="report-chip-title">{plainText(r.task.title)}</span>
								</button>
							{:else}
								<span class="report-chip report-chip-gone">
									<span class="report-chip-check" aria-hidden="true"></span>
									<span class="report-chip-title">A deleted task</span>
								</span>
							{/if}
							<span class="report-meta">
								{#if pl}<span class="report-place">{pl.name}<PlaceMark color={pl.color} /></span>{/if}
								<time class="report-time" datetime={r.at.toISOString()} title={r.at.toLocaleString()}>{when(r.at)}</time>
							</span>
						</li>
					{/each}
				</ul>
			</section>
		{/each}
		{#if filtered.length > shown}
			<button type="button" class="btn btn-secondary report-more" onclick={() => (shown += PAGE)}>Show more</button>
		{/if}
	{/if}
</div>
