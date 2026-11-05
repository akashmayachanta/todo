<script lang="ts">
	import { onMount } from 'svelte';
	import type { TaskCategory, TaskIn, TaskPriority, TaskProject } from '$lib/api';
	import { dayLabel, formatDuration, formatTime, parseQuickAdd } from '$lib/taskParse';
	import SmartTaskInput from './SmartTaskInput.svelte';
	import { defaultReminder, projectColor } from './shared';
	import './quickadd.css';

	/**
	 * One line that becomes a task: "Call bank tomorrow 5pm p1 #Home !30m". The parts read as
	 * date, priority, place, recurrence or reminder are highlighted in the line and shown as
	 * chips under it. `#` offers the projects and categories, `/` the categories. Enter adds, Esc cancels.
	 */
	let { projects, categories, defaults = {}, onadd, oncancel }: {
		projects: TaskProject[];
		categories: TaskCategory[];
		/** Where the task goes, its date and priority unless the line says otherwise. */
		defaults?: { project_id?: string | null; category_id?: string | null; due_date?: string | null; priority?: TaskPriority };
		onadd: (data: TaskIn) => Promise<void>;
		oncancel: () => void;
	} = $props();

	let text = $state('');
	let busy = $state(false);
	let field = $state<SmartTaskInput | null>(null);
	let now = $state(new Date());

	let parsed = $derived(parseQuickAdd(text, projects, categories, now, defaults.project_id ?? null, defaults.due_date ?? null));

	let result = $derived.by((): TaskIn => {
		const project_id = parsed.project_id ?? defaults.project_id ?? null;
		const category_id = parsed.category_id
			?? (project_id === (defaults.project_id ?? null) ? defaults.category_id ?? null : null);
		// "no date" in the line beats the day being added to.
		const datedByLine = parsed.tokens.some((t) => t.kind === 'date');
		const due_date = datedByLine ? parsed.due_date : parsed.due_date ?? defaults.due_date ?? null;
		const due_time = parsed.due_date ? parsed.due_time : null;
		return {
			title: parsed.title,
			project_id,
			category_id,
			priority: parsed.priority ?? defaults.priority ?? 4,
			due_date,
			due_time,
			duration_minutes: parsed.duration_minutes,
			// A due time brings a reminder at that time unless the line names one.
			remind_at: parsed.remind_at ?? defaultReminder(due_date, due_time, now),
			recurrence: parsed.recurrence,
		};
	});

	let project = $derived(projects.find((p) => p.id === result.project_id) ?? null);
	let category = $derived(categories.find((c) => c.id === result.category_id) ?? null);
	// `/` offers the categories of the project named in the line, else of the one being added to.
	let scopeProjectId = $derived(
		parsed.tokens.some((t) => t.kind === 'project' && !t.unknown) ? parsed.project_id : defaults.project_id ?? null,
	);

	async function submit() {
		if (busy || !result.title) return;
		busy = true;
		try {
			await onadd(result);
			text = '';
			now = new Date();
		} finally {
			busy = false;
			field?.focus();
		}
	}

	onMount(() => field?.focus());
</script>

<div class="quick-add">
	<SmartTaskInput
		bind:this={field}
		bind:value={text}
		tokens={parsed.tokens}
		{projects}
		{categories}
		{scopeProjectId}
		placeholder="Task name, e.g. Call bank tomorrow 5pm p1 #Project or #Category !30m"
		ariaLabel="New task"
		onenter={submit}
		onescape={oncancel}
	/>

	<div class="quick-add-foot">
		<div class="quick-add-chips">
			{#if result.due_date}
				<span class="task-chip" data-kind="date">{dayLabel(result.due_date, now)}{result.due_time ? ` ${formatTime(result.due_time)}` : ''}</span>
			{/if}
			{#if result.duration_minutes}
				<span class="task-chip" data-kind="duration">for {formatDuration(result.duration_minutes)}</span>
			{/if}
			{#if result.recurrence}
				<span class="task-chip" data-kind="recurrence">↻ {result.recurrence}</span>
			{/if}
			{#if result.remind_at}
				<span class="task-chip" data-kind="reminder">
					<svg aria-hidden="true" class="w-3 h-3" fill="none" stroke="currentColor" viewBox="0 0 24 24"><path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M15 17h5l-1.405-1.405A2.032 2.032 0 0118 14.158V11a6.002 6.002 0 00-4-5.659V5a2 2 0 10-4 0v.341C7.67 6.165 6 8.388 6 11v3.159c0 .538-.214 1.055-.595 1.436L4 17h5m6 0v1a3 3 0 11-6 0v-1m6 0H9" /></svg>
					{new Date(result.remind_at).toLocaleString(undefined, { day: 'numeric', month: 'short', hour: 'numeric', minute: '2-digit' })}
				</span>
			{/if}
			{#if result.priority && result.priority < 4}
				<span class="task-chip" data-kind="priority" data-priority={result.priority}>P{result.priority}</span>
			{/if}
			<span class="task-chip" data-kind="project">
				{#if project}<span class="tasks-dot" style="background-color: {projectColor(project.color)}"></span>{/if}
				{project ? project.name : 'Inbox'}{category ? ` / ${category.name}` : ''}
			</span>
		</div>
		<div class="quick-add-actions">
			<button type="button" class="btn btn-ghost" onclick={oncancel}>Cancel</button>
			<button type="button" class="btn btn-primary" onclick={submit} disabled={busy || !result.title}>Add task</button>
		</div>
	</div>
</div>
