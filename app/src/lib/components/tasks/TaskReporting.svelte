<script lang="ts">
	import { untrack } from 'svelte';
	import { api, errorDetail, type Task, type TaskProject, type TaskReport, type TaskReportItem } from '$lib/api';
	import { plainText } from '$lib/links';
	import { addDays, formatDuration, fromYmd } from '$lib/taskParse';
	import Avatar from '$shell/Avatar.svelte';
	import { me } from '$shell/profile';
	import PlaceMark from './PlaceMark.svelte';
	import { projectColor } from './shared';
	import './reporting.css';

	/**
	 * Reporting: every tick in the completion log (each occurrence of a repeating task
	 * too), newest first, grouped by local day, with the task and when it was ticked.
	 * Filtered by project and by how far back. The server sends a page at a time, each
	 * tick with its task, and how many ticks each day has in all.
	 */
	let { today, projects, tasks, onopen }: {
		today: string;
		projects: TaskProject[];
		/** The page's open tasks: when they change (a tick, an edit, a sync), the report refreshes. */
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
	/** Rows in a page: the first, and each "Show more". */
	const PAGE = 200;
	/** The most a quiet refresh fetches again (the server's limit). */
	const MAX = 1000;

	function stored(key: string): string | null {
		try {
			return localStorage.getItem(key);
		} catch {
			return null; // storage blocked: the defaults
		}
	}
	function remember(key: string, value: string) {
		try { localStorage.setItem(key, value); } catch { /* not remembered */ }
	}

	const storedRange = stored(RANGE_KEY);
	let range = $state<RangeKey>(RANGES.find((r) => r.key === storedRange)?.key ?? 'any');
	/** '' all, 'inbox', or a project id. */
	let projectPick = $state(stored(PROJECT_KEY) ?? '');

	let items = $state<TaskReportItem[] | null>(null);
	/** Every matching tick of each day loaded, not only those shown. */
	let counts = $state(new Map<string, number>());
	/** The cursor for "Show more"; null at the end. */
	let next = $state<string | null>(null);
	let error = $state('');
	let loadingMore = $state(false);

	let since = $derived.by(() => {
		const days = RANGES.find((r) => r.key === range)?.days;
		return days == null ? new Date(0).toISOString() : fromYmd(addDays(today, -days)).toISOString();
	});

	function fetchPage(before: string | null, limit: number): Promise<TaskReport> {
		return api.tasks.report({ since, project: projectPick, before, limit, utc_offset: -new Date().getTimezoneOffset() });
	}

	let loadNo = 0;
	/** The first page again; with `keep`, as many rows as are shown now (a quiet refresh). */
	async function load(keep = false) {
		const n = ++loadNo;
		const limit = keep && items ? Math.min(Math.max(items.length, PAGE), MAX) : PAGE;
		try {
			const r = await fetchPage(null, limit);
			if (n !== loadNo) return;
			items = r.items;
			counts = new Map(r.days.map((d) => [d.day, d.count]));
			next = r.next;
			error = '';
		} catch (e) {
			if (n === loadNo) error = errorDetail(e);
		}
	}

	async function more() {
		if (!next || loadingMore) return;
		const n = loadNo;
		loadingMore = true;
		try {
			const r = await fetchPage(next, PAGE);
			if (n !== loadNo) return; // a reload started meanwhile; its rows win
			items = [...(items ?? []), ...r.items];
			counts = new Map([...counts, ...r.days.map((d) => [d.day, d.count] as const)]);
			next = r.next;
		} catch (e) {
			error = errorDetail(e);
		} finally {
			loadingMore = false;
		}
	}

	// From the top when the range or the project changes.
	$effect(() => {
		void since;
		void projectPick;
		untrack(() => load());
	});

	// Quietly, a moment after the page's tasks change; not on the first run, which the
	// load above covers.
	let primed = false;
	$effect(() => {
		void tasks;
		if (!primed) {
			primed = true;
			return;
		}
		const timer = setTimeout(() => load(true), 400);
		return () => clearTimeout(timer);
	});

	let projectById = $derived(new Map(projects.map((p) => [p.id, p])));

	let groups = $derived.by(() => {
		const out: { day: string; rows: TaskReportItem[] }[] = [];
		for (const r of items ?? []) {
			const last = out[out.length - 1];
			if (last?.day === r.day) last.rows.push(r);
			else out.push({ day: r.day, rows: [r] });
		}
		return out;
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
			<select class="input" bind:value={projectPick} onchange={() => remember(PROJECT_KEY, projectPick)}>
				<option value="">All projects</option>
				<option value="inbox">Inbox</option>
				{#each projects as p (p.id)}<option value={p.id}>{p.name}</option>{/each}
			</select>
		</label>
		<label class="report-filter">
			<span class="sr-only">Date</span>
			<select class="input" bind:value={range} onchange={() => remember(RANGE_KEY, range)}>
				{#each RANGES as r (r.key)}<option value={r.key}>{r.label}</option>{/each}
			</select>
		</label>
	</div>

	{#if error}
		<div class="error-box">Couldn't load completed tasks: {error}</div>
	{:else if items === null}
		<div class="flex items-center gap-3 text-[var(--text-muted)]"><div class="spinner"></div>Loading…</div>
	{:else if !items.length}
		<p class="report-empty">No completed tasks {range === 'any' ? 'yet' : 'in this period'}.</p>
	{:else}
		{#each groups as g (g.day)}
			<section class="report-day" aria-label={dayLabel(g.day)}>
				<h2 class="report-day-head">{dayLabel(g.day)} <span class="report-day-count">{counts.get(g.day) ?? g.rows.length}</span></h2>
				<ul class="report-list">
					{#each g.rows as r (r.id)}
						{@const pl = place(r.task)}
						{@const at = new Date(r.completed_at)}
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
								{#if r.task?.duration_minutes}<span class="report-duration" title="Takes {formatDuration(r.task.duration_minutes)}">{formatDuration(r.task.duration_minutes)}</span>{/if}
								{#if pl}<span class="report-place">{pl.name}<PlaceMark color={pl.color} /></span>{/if}
								<time class="report-time" datetime={at.toISOString()} title={at.toLocaleString()}>{when(at)}</time>
							</span>
						</li>
					{/each}
				</ul>
			</section>
		{/each}
		{#if next}
			<button type="button" class="btn btn-secondary report-more" disabled={loadingMore} onclick={more}>
				{loadingMore ? 'Loading…' : 'Show more'}
			</button>
		{/if}
	{/if}
</div>
