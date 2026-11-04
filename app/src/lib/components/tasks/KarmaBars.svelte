<script module lang="ts">
	export type Bar = {
		key: string;
		/** Under the column, e.g. "3" or "Mon". */
		axis: string;
		/** In the tooltip and table, e.g. "Tuesday 3 November". */
		long: string;
		value: number;
		/** Today, or this week: its value is labelled. */
		current?: boolean;
	};
</script>

<script lang="ts">
	/**
	 * One series of columns (tasks completed per day or per week) against a dashed goal
	 * line. Drawn in pixels from the measured width, so text never stretches. Each column's
	 * band is its hover target; the chart takes focus and the arrow keys step through the
	 * columns with the same tooltip. The current column carries its value on its cap, and
	 * a table view under the chart has every value.
	 */

	let { bars, goal, title, unit, axisWidth = 28 }: {
		bars: Bar[];
		goal: number;
		/** Names the chart for screen readers and heads the table. */
		title: string;
		/** The table's first column, e.g. "Day" or "Week of". */
		unit: string;
		/** About how wide one axis label is, so crowded ones can be skipped. */
		axisWidth?: number;
	} = $props();

	const HEIGHT = 200;
	const M = { top: 18, right: 8, bottom: 24, left: 30 };
	const BAR_MAX = 24;

	let width = $state(0);
	let active = $state<number | null>(null);

	let plotW = $derived(Math.max(0, width - M.left - M.right));
	let plotH = HEIGHT - M.top - M.bottom;
	let band = $derived(bars.length ? plotW / bars.length : 0);
	/** Capped at 24px, and never so wide that neighbours lose their 2px of air. */
	let barW = $derived(Math.max(2, Math.min(BAR_MAX, band * 0.68, band - 2)));

	/** A round step giving at most four gridlines, and a top that clears the goal. */
	let scale = $derived.by(() => {
		const peak = Math.max(goal, ...bars.map((b) => b.value), 1);
		const steps = [1, 2, 5, 10, 20, 25, 50, 100, 200, 250, 500, 1000];
		const step = steps.find((s) => peak / s <= 4) ?? Math.ceil(peak / 4);
		const max = Math.ceil(peak / step) * step;
		const ticks: number[] = [];
		for (let v = 0; v <= max; v += step) ticks.push(v);
		return { max, ticks };
	});

	const y = (v: number) => M.top + plotH - (v / scale.max) * plotH;
	const cx = (i: number) => M.left + band * i + band / 2;

	/** A column with a 2px rounded top (the app's control radius) and a square foot on the baseline. */
	function column(i: number, v: number): string {
		const h = (v / scale.max) * plotH;
		if (h <= 0) return '';
		const r = Math.min(2, h, barW / 2);
		const x0 = cx(i) - barW / 2;
		const x1 = x0 + barW;
		const base = M.top + plotH;
		const top = base - h;
		return `M${x0},${base}V${top + r}Q${x0},${top} ${x0 + r},${top}H${x1 - r}Q${x1},${top} ${x1},${top + r}V${base}Z`;
	}

	/** Every nth axis label, so they never collide. */
	let every = $derived(band > 0 ? Math.max(1, Math.ceil(axisWidth / band)) : 1);
	const showAxis = (i: number) => (bars.length - 1 - i) % every === 0; // keep the latest one

	let goalY = $derived(y(goal));
	let tip = $derived(active === null ? null : bars[active]);

	function onkey(e: KeyboardEvent) {
		if (!bars.length) return;
		const last = bars.length - 1;
		const at = active ?? last;
		const next = e.key === 'ArrowLeft' ? at - 1 : e.key === 'ArrowRight' ? at + 1 : e.key === 'Home' ? 0 : e.key === 'End' ? last : null;
		if (next === null) return;
		e.preventDefault();
		active = Math.max(0, Math.min(last, next));
	}

	const n = (v: number) => v.toLocaleString();
	const versus = (v: number) => (v >= goal ? `Goal of ${n(goal)} met` : `${n(goal - v)} short of ${n(goal)}`);
</script>

<div class="kbars" bind:clientWidth={width}>
	{#if width > 0}
		<!-- The chart takes focus so the arrow keys can read each column (the tooltip follows). -->
		<!-- svelte-ignore a11y_no_noninteractive_tabindex, a11y_no_noninteractive_element_interactions -->
		<svg
			class="kbars-svg"
			width={width}
			height={HEIGHT}
			role="img"
			aria-label="{title}. Use the arrow keys to read each value."
			tabindex="0"
			onkeydown={onkey}
			onfocus={() => (active ??= bars.length - 1)}
			onblur={() => (active = null)}
			onpointerleave={() => (active = null)}
		>
			{#each scale.ticks as t (t)}
				<line class={t === 0 ? 'kbars-base' : 'kbars-grid'} x1={M.left} x2={M.left + plotW} y1={y(t)} y2={y(t)} />
				<text class="kbars-tick" x={M.left - 6} y={y(t)} dy="0.32em" text-anchor="end">{n(t)}</text>
			{/each}

			{#if active !== null}
				<rect class="kbars-hover" x={M.left + band * active} y={M.top} width={band} height={plotH} />
			{/if}

			{#each bars as b, i (b.key)}
				<path class="kbars-bar" d={column(i, b.value)} />
				{#if b.current && b.value > 0}
					<text class="kbars-value" x={cx(i)} y={y(b.value) - 5} text-anchor="middle">{n(b.value)}</text>
				{/if}
				{#if showAxis(i)}
					<text class="kbars-axis" class:kbars-axis-current={b.current} x={cx(i)} y={HEIGHT - 6} text-anchor="middle">{b.axis}</text>
				{/if}
			{/each}

			<line class="kbars-goal" x1={M.left} x2={M.left + plotW} y1={goalY} y2={goalY} />
			<text class="kbars-goal-label" x={M.left + plotW} y={goalY - 4} text-anchor="end">Goal {n(goal)}</text>

			<!-- Hit targets: the whole band of each column, on top of everything. -->
			{#each bars as b, i (b.key)}
				<rect
					class="kbars-hit"
					x={M.left + band * i}
					y={M.top}
					width={band}
					height={plotH}
					onpointerenter={() => (active = i)}
					role="presentation"
				/>
			{/each}
		</svg>

		{#if tip && active !== null}
			<div
				class="kbars-tip"
				style="left: {Math.min(Math.max(cx(active), 70), width - 70)}px; top: {Math.min(y(tip.value), goalY) - 8}px"
				role="status"
			>
				<strong class="kbars-tip-value">{n(tip.value)} completed</strong>
				<span class="kbars-tip-label">{tip.long}</span>
				<span class="kbars-tip-goal">{versus(tip.value)}</span>
			</div>
		{/if}
	{/if}
</div>

<details class="kbars-table">
	<summary>Show as table</summary>
	<table>
		<caption class="sr-only">{title}</caption>
		<thead>
			<tr><th scope="col">{unit}</th><th scope="col">Completed</th><th scope="col">Goal {n(goal)}</th></tr>
		</thead>
		<tbody>
			{#each [...bars].reverse() as b (b.key)}
				<tr>
					<th scope="row">{b.long}</th>
					<td>{n(b.value)}</td>
					<td>{b.value >= goal ? 'Met' : `${n(goal - b.value)} short`}</td>
				</tr>
			{/each}
		</tbody>
	</table>
</details>
