<script lang="ts">
	import { onMount } from 'svelte';
	import { api, errorDetail, type TaskCompletions } from '$lib/api';
	import { addDays, fromYmd, ymd } from '$lib/taskParse';
	import KarmaBars, { type Bar } from './KarmaBars.svelte';
	import { clampGoal } from './shared';
	import './karma.css';

	/**
	 * Karma: what has been completed, from the completion log (every tick, each occurrence
	 * of a repeating task too). Headline tiles for today, this week, the streaks and all
	 * time, then tasks per day and per week against the goals. Weeks start Monday, days
	 * are this browser's local days. A year is loaded, which bounds the streaks.
	 */
	let { today, dailyGoal, weeklyGoal, ongoals }: {
		today: string;
		dailyGoal: number;
		weeklyGoal: number;
		ongoals: (daily: number, weekly: number) => void;
	} = $props();

	const WEEKS_LOADED = 53;
	const DAY_RANGES = [7, 14, 30] as const;
	const WEEKS_SHOWN = 12;
	const RANGE_KEY = 'tasks-karma-days';

	let data = $state<TaskCompletions | null>(null);
	let error = $state('');
	let days = $state<(typeof DAY_RANGES)[number]>(14);

	const monday = (date: string) => addDays(date, -((fromYmd(date).getDay() + 6) % 7));
	let thisWeek = $derived(monday(today));
	/** The first day loaded: the streaks can't see past it. */
	let start = $derived(addDays(thisWeek, -7 * (WEEKS_LOADED - 1)));

	onMount(() => {
		try {
			const saved = Number(localStorage.getItem(RANGE_KEY));
			if ((DAY_RANGES as readonly number[]).includes(saved)) days = saved as (typeof DAY_RANGES)[number];
		} catch {
			/* storage blocked: two weeks */
		}
		api.tasks.completions(fromYmd(start).toISOString())
			.then((d) => (data = d))
			.catch((e) => (error = errorDetail(e)));
	});

	function setDays(n: (typeof DAY_RANGES)[number]) {
		days = n;
		try { localStorage.setItem(RANGE_KEY, String(n)); } catch { /* not remembered */ }
	}

	/** Ticks per local day. */
	let perDay = $derived.by(() => {
		const map = new Map<string, number>();
		for (const c of data?.items ?? []) {
			const d = ymd(new Date(c.completed_at));
			map.set(d, (map.get(d) ?? 0) + 1);
		}
		return map;
	});
	const onDay = (d: string) => perDay.get(d) ?? 0;

	/** Ticks per week, keyed by its Monday. */
	let perWeek = $derived.by(() => {
		const map = new Map<string, number>();
		for (const [d, n] of perDay) {
			const w = monday(d);
			map.set(w, (map.get(w) ?? 0) + n);
		}
		return map;
	});
	const onWeek = (w: string) => perWeek.get(w) ?? 0;

	/**
	 * Runs of goals met, counted back from now. Today (or this week) only counts once its
	 * goal is met, so an unfinished day doesn't break the streak. `capped` means the run
	 * reaches the first day loaded, so it may be longer.
	 */
	function streak(units: string[], count: (u: string) => number, goal: number) {
		// units: oldest first, ending with the current one.
		let i = units.length - 1;
		if (count(units[i]) < goal) i--;
		let current = 0;
		while (i >= 0 && count(units[i]) >= goal) { current++; i--; }
		let best = 0, run = 0;
		for (const u of units) {
			run = count(u) >= goal ? run + 1 : 0;
			best = Math.max(best, run);
		}
		return { current, best, capped: i < 0 && current > 0 };
	}

	let allDays = $derived.by(() => {
		const out: string[] = [];
		for (let d = start; d <= today; d = addDays(d, 1)) out.push(d);
		return out;
	});
	let allWeeks = $derived(Array.from({ length: WEEKS_LOADED }, (_, i) => addDays(start, 7 * i)));

	let dayStreak = $derived(streak(allDays, onDay, dailyGoal));
	let weekStreak = $derived(streak(allWeeks, onWeek, weeklyGoal));

	let dayBars = $derived<Bar[]>(
		allDays.slice(-days).map((d) => {
			const date = fromYmd(d);
			return {
				key: d,
				axis: days === 7 ? date.toLocaleDateString(undefined, { weekday: 'short' }) : String(date.getDate()),
				long: date.toLocaleDateString(undefined, { weekday: 'long', day: 'numeric', month: 'long' }),
				value: onDay(d),
				current: d === today,
			};
		}),
	);

	let weekBars = $derived<Bar[]>(
		allWeeks.slice(-WEEKS_SHOWN).map((w) => {
			const fmt = new Intl.DateTimeFormat(undefined, { day: 'numeric', month: 'short' });
			return {
				key: w,
				axis: fmt.format(fromYmd(w)),
				long: `Week of ${fromYmd(w).toLocaleDateString(undefined, { day: 'numeric', month: 'long', year: 'numeric' })}`,
				value: onWeek(w),
				current: w === thisWeek,
			};
		}),
	);

	let todayCount = $derived(onDay(today));
	let weekCount = $derived(onWeek(thisWeek));

	const n = (v: number) => v.toLocaleString();
	const plural = (v: number, one: string, many: string) => `${n(v)} ${v === 1 ? one : many}`;
	const pct = (v: number, goal: number) => Math.min(100, Math.round((v / goal) * 100));

	function setGoal(which: 'daily' | 'weekly', raw: string) {
		const d = which === 'daily' ? clampGoal(Number(raw), dailyGoal) : dailyGoal;
		const w = which === 'weekly' ? clampGoal(Number(raw), weeklyGoal) : weeklyGoal;
		ongoals(d, w);
	}
</script>

{#snippet meter(value: number, goal: number, label: string)}
	<div
		class="karma-meter"
		role="meter"
		aria-label={label}
		aria-valuemin="0"
		aria-valuemax={goal}
		aria-valuenow={Math.min(value, goal)}
		aria-valuetext="{n(value)} of {n(goal)}"
	>
		<span class="karma-meter-fill" style="width: {pct(value, goal)}%"></span>
	</div>
{/snippet}

{#if error}
	<div class="error-box">Couldn't load your progress: {error}</div>
{:else if !data}
	<div class="flex items-center gap-3 text-[var(--text-muted)]"><div class="spinner"></div>Loading…</div>
{:else}
	<div class="karma">
		<section class="karma-tiles" aria-label="Summary">
			<div class="karma-tile">
				<span class="karma-tile-label">Today</span>
				<span class="karma-tile-value">{n(todayCount)}<span class="karma-tile-of"> / {n(dailyGoal)}</span></span>
				{@render meter(todayCount, dailyGoal, 'Today against the daily goal')}
				<span class="karma-tile-note">{todayCount >= dailyGoal ? 'Daily goal met' : `${plural(dailyGoal - todayCount, 'task', 'tasks')} to go`}</span>
			</div>
			<div class="karma-tile">
				<span class="karma-tile-label">This week</span>
				<span class="karma-tile-value">{n(weekCount)}<span class="karma-tile-of"> / {n(weeklyGoal)}</span></span>
				{@render meter(weekCount, weeklyGoal, 'This week against the weekly goal')}
				<span class="karma-tile-note">{weekCount >= weeklyGoal ? 'Weekly goal met' : `${plural(weeklyGoal - weekCount, 'task', 'tasks')} to go`}</span>
			</div>
			<div class="karma-tile">
				<span class="karma-tile-label">Daily streak</span>
				<span class="karma-tile-value">{n(dayStreak.current)}{dayStreak.capped ? '+' : ''}<span class="karma-tile-of"> {dayStreak.current === 1 ? 'day' : 'days'}</span></span>
				<span class="karma-tile-note">Best this year: {plural(dayStreak.best, 'day', 'days')}</span>
			</div>
			<div class="karma-tile">
				<span class="karma-tile-label">Weekly streak</span>
				<span class="karma-tile-value">{n(weekStreak.current)}{weekStreak.capped ? '+' : ''}<span class="karma-tile-of"> {weekStreak.current === 1 ? 'week' : 'weeks'}</span></span>
				<span class="karma-tile-note">Best this year: {plural(weekStreak.best, 'week', 'weeks')}</span>
			</div>
			<div class="karma-tile">
				<span class="karma-tile-label">All time</span>
				<span class="karma-tile-value">{n(data.total)}</span>
				<span class="karma-tile-note">{data.total === 1 ? 'task' : 'tasks'} completed</span>
			</div>
		</section>

		<div class="karma-goals">
			<label class="karma-goal">
				<span>Daily goal</span>
				<input class="input karma-goal-input" type="number" min="1" max="999" step="1" value={dailyGoal} onchange={(e) => setGoal('daily', e.currentTarget.value)} />
			</label>
			<label class="karma-goal">
				<span>Weekly goal</span>
				<input class="input karma-goal-input" type="number" min="1" max="999" step="1" value={weeklyGoal} onchange={(e) => setGoal('weekly', e.currentTarget.value)} />
			</label>
		</div>

		<section class="karma-card" aria-labelledby="karma-daily-title">
			<div class="karma-card-head">
				<div>
					<h2 id="karma-daily-title" class="karma-card-title">Daily</h2>
					<p class="karma-card-sub">Tasks completed each day, last {days} days</p>
				</div>
				<div class="tasks-layout" role="group" aria-label="Days shown">
					{#each DAY_RANGES as r (r)}
						<button type="button" class="tasks-layout-btn" aria-pressed={days === r} onclick={() => setDays(r)}>{r}d</button>
					{/each}
				</div>
			</div>
			<KarmaBars bars={dayBars} goal={dailyGoal} title="Tasks completed each day, last {days} days" unit="Day" axisWidth={days === 7 ? 34 : 22} />
		</section>

		<section class="karma-card" aria-labelledby="karma-weekly-title">
			<div class="karma-card-head">
				<div>
					<h2 id="karma-weekly-title" class="karma-card-title">Weekly</h2>
					<p class="karma-card-sub">Tasks completed each week (Monday to Sunday), last {WEEKS_SHOWN} weeks</p>
				</div>
			</div>
			<KarmaBars bars={weekBars} goal={weeklyGoal} title="Tasks completed each week, last {WEEKS_SHOWN} weeks" unit="Week" axisWidth={44} />
		</section>

		{#if data.total === 0}
			<p class="tasks-empty">Nothing completed yet. Tick a task and it shows up here.</p>
		{/if}
	</div>
{/if}
