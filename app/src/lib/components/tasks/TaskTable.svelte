<script lang="ts" module>
	import type { Task } from '$lib/api';
	import type { AddDefaults } from './taskFilters';

	/** A run of rows under a heading: a section, a day, a priority, a filter's bucket. */
	export type TableGroup = {
		key: string;
		/** null: no heading row (a view that is one list, like the Inbox). */
		title: string | null;
		sub?: string;
		color?: string | null;
		overdue?: boolean;
		/** Open tasks, in order. */
		tasks: Task[];
		/** Its completed tasks, listed after the open ones when completed tasks are shown. */
		done: Task[];
		add: { key: string; defaults: AddDefaults } | null;
	};
</script>

<script lang="ts">
	import type { Snippet } from 'svelte';
	import { plainText } from '$lib/links';
	import { describeDue, formatDuration } from '$lib/taskParse';
	import LinkedText from './LinkedText.svelte';
	import PlaceMark from './PlaceMark.svelte';

	/**
	 * The Table layout: one row a task, with its due date, priority, duration and (where the
	 * view mixes them) project as columns. Subtasks sit indented under their task; a group's
	 * completed tasks follow its open ones. A click opens a task, its check completes or
	 * reopens it.
	 */
	let { groups, subrows, placeOf = null, now, ontoggle, onopen, adder, empty = '' }: {
		groups: TableGroup[];
		/** The subtasks listed under a task (none when subtasks are hidden). */
		subrows: (task: Task) => Task[];
		/** Where a task lives; null leaves the Project column out. */
		placeOf?: ((task: Task) => { name: string; color: string | null }) | null;
		now: Date;
		ontoggle: (task: Task) => void;
		onopen: (task: Task) => void;
		adder: Snippet<[string, AddDefaults]>;
		/** Shown when no group has any task. */
		empty?: string;
	} = $props();

	const PRIORITY = { 1: 'P1', 2: 'P2', 3: 'P3', 4: '' } as const;

	let cols = $derived(placeOf ? 5 : 4);
	let anything = $derived(groups.some((g) => g.tasks.length || g.done.length));
</script>

{#snippet line(task: Task, sub: boolean)}
	{@const done = !!task.completed_at}
	{@const name = plainText(task.title)}
	{@const due = describeDue(task, now)}
	{@const place = placeOf && !sub ? placeOf(task) : null}
	<tr class="ttable-row" class:ttable-row-done={done} class:ttable-row-sub={sub}>
		<td class="ttable-title">
			<div class="ttable-title-in">
				<button
					type="button"
					class="task-check"
					class:task-check-sm={sub}
					data-priority={task.priority}
					aria-pressed={done}
					aria-label={done ? `Mark "${name}" not done` : task.recurrence ? `Complete "${name}" (moves to its next date)` : `Complete "${name}"`}
					title={done ? 'Mark not done' : 'Complete'}
					onclick={() => ontoggle(task)}
				>
					<svg aria-hidden="true" class="task-check-tick" fill="none" stroke="currentColor" viewBox="0 0 24 24"><path stroke-linecap="round" stroke-linejoin="round" stroke-width="3" d="M5 13l4 4L19 7" /></svg>
				</button>
				<!-- Links sit inside, so this can't be a button: the pointer clicks the cell, the keyboard the button over it. -->
				<!-- svelte-ignore a11y_click_events_have_key_events, a11y_no_static_element_interactions -->
				<div class="task-main ttable-main" onclick={() => onopen(task)}>
					<button type="button" class="task-open" aria-label="Open {name}"></button>
					<span class="task-title ttable-name"><LinkedText text={task.title} /></span>
				</div>
			</div>
		</td>
		<td class="ttable-due" class:task-due-overdue={due?.overdue && !done} class:task-due-today={due?.today && !due?.overdue && !done}>{due?.label ?? ''}</td>
		<td class="ttable-pri" data-priority={task.priority}>{PRIORITY[task.priority]}</td>
		<td class="ttable-num">{task.duration_minutes ? formatDuration(task.duration_minutes) : ''}</td>
		{#if placeOf}
			<td class="ttable-place">
				{#if place}
					<span class="ttable-place-in">{#if place.color}<PlaceMark color={place.color} />{/if}<span class="truncate">{place.name}</span></span>
				{/if}
			</td>
		{/if}
	</tr>
	{#each subrows(task) as s (s.id)}
		{@render line(s, true)}
	{/each}
{/snippet}

<div class="ttable-wrap">
	<table class="ttable">
		<thead>
			<tr>
				<th scope="col">Task</th>
				<th scope="col" class="ttable-due">Due</th>
				<th scope="col" class="ttable-pri">Priority</th>
				<th scope="col" class="ttable-num">Duration</th>
				{#if placeOf}<th scope="col" class="ttable-place">Project</th>{/if}
			</tr>
		</thead>
		{#each groups as g (g.key)}
			<tbody>
				{#if g.title !== null}
					<tr class="ttable-group">
						<th scope="colgroup" colspan={cols} class:task-group-head-overdue={g.overdue}>
							{#if g.color}<PlaceMark color={g.color} />{/if}
							{g.title}
							{#if g.sub}<span class="task-group-sub">{g.sub}</span>{/if}
							<span class="task-group-count">{g.tasks.length}</span>
						</th>
					</tr>
				{/if}
				{#each g.tasks as t (t.id)}
					{@render line(t, false)}
				{/each}
				{#each g.done as t (t.id)}
					{@render line(t, false)}
				{/each}
				{#if g.add}
					<tr class="ttable-add"><td colspan={cols}>{@render adder(g.add.key, g.add.defaults)}</td></tr>
				{/if}
			</tbody>
		{/each}
	</table>
</div>
{#if !anything && empty}
	<p class="tasks-empty">{empty}</p>
{/if}
