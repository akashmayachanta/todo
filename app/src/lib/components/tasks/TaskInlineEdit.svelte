<script lang="ts">
	import PlaceMark from '$lib/components/tasks/PlaceMark.svelte';
	import { formatInstant } from '$lib/datePrefs.svelte';
	import { onMount, untrack } from 'svelte';
	import type { Task, TaskCategory, TaskPatch, TaskProject } from '$lib/api';
	import { askReminderPermission } from '$lib/stores/taskReminders';
	import { describeDue, formatDuration } from '$lib/taskParse';
	import SmartTaskInput from './SmartTaskInput.svelte';
	import { finishPatch, settlePatch, smartPatch, smartTitle, titleEdit } from './smartEdit';
	import { SUBTASK_MAX, placeName, projectColor, subtaskLimit } from './shared';

	/**
	 * A task edited where it stands (Alt+click, or the pencil): its title as a smart line
	 * and its description. The chips show the task as it will be once saved. Enter in the
	 * title or Ctrl+Enter in the description saves, Esc cancels.
	 */
	let { task, projects, categories, onpatch, ondone }: {
		task: Task;
		projects: TaskProject[];
		categories: TaskCategory[];
		onpatch: (task: Task, patch: TaskPatch) => Promise<void>;
		ondone: () => void;
	} = $props();

	// The editor starts from the task as it was opened.
	let title = $state(untrack(() => task.title));
	let description = $state(untrack(() => task.description ?? ''));
	let busy = $state(false);
	let field = $state<SmartTaskInput | null>(null);
	let root = $state<HTMLFormElement | null>(null);

	let st = $derived(smartTitle(title, task, projects, categories));
	let limit = $derived(task.parent_id ? subtaskLimit(st.smart.title, task.title) : null);
	/** The task as it will be once saved, with the reminder a new due time brings or moves. */
	let next = $derived({ ...task, ...settlePatch(task, smartPatch(st.smart, task)) });
	let due = $derived(describeDue(next));
	let project = $derived(projects.find((p) => p.id === next.project_id) ?? null);

	async function save() {
		if (busy) return;
		if (limit?.over || (!st.smart.title && !Object.keys(smartPatch(st.smart, task)).length)) {
			// Too long for a subtask, or emptied: there is nothing to save it as.
			field?.focus();
			return;
		}
		const patch: TaskPatch = titleEdit(st.smart, task) ?? {};
		const d = description.trim() || null;
		if (d !== (task.description ?? null)) patch.description = d;
		if (!Object.keys(patch).length) {
			ondone();
			return;
		}
		if (patch.remind_at) askReminderPermission();
		busy = true;
		try {
			await onpatch(task, finishPatch(task, patch));
			ondone();
		} finally {
			busy = false;
		}
	}

	function onDescKey(e: KeyboardEvent) {
		if (e.key === 'Enter' && (e.ctrlKey || e.metaKey)) {
			e.preventDefault();
			save();
		} else if (e.key === 'Escape') {
			e.preventDefault();
			e.stopPropagation();
			ondone();
		}
	}

	/** Esc anywhere in the editor cancels, unless a suggestion list took it. */
	function onKey(e: KeyboardEvent) {
		if (e.key === 'Escape' && !e.defaultPrevented) {
			e.preventDefault();
			e.stopPropagation();
			ondone();
		}
	}

	onMount(() => {
		field?.focus();
		root?.scrollIntoView({ block: 'nearest' });
	});
</script>

<!-- svelte-ignore a11y_no_noninteractive_element_interactions -->
<form
	bind:this={root}
	class="task-inline-edit"
	aria-label="Edit {task.title}"
	onsubmit={(e) => { e.preventDefault(); save(); }}
	onkeydown={onKey}
>
	<SmartTaskInput
		bind:this={field}
		bind:value={title}
		tokens={st.smart.tokens}
		projects={st.projects}
		categories={st.categories}
		scopeProjectId={st.scope}
		ariaLabel="Title"
		class="task-inline-title"
		onenter={save}
		onescape={ondone}
	/>
	<textarea
		class="task-inline-desc"
		bind:value={description}
		rows="2"
		placeholder="Description"
		aria-label="Description"
		onkeydown={onDescKey}
	></textarea>

	<div class="task-inline-chips">
		{#if due}
			<span class="task-chip" data-kind="date">
				<svg aria-hidden="true" class="w-3 h-3" fill="none" stroke="currentColor" viewBox="0 0 24 24"><path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M8 7V3m8 4V3m-9 8h10M5 21h14a2 2 0 002-2V7a2 2 0 00-2-2H5a2 2 0 00-2 2v12a2 2 0 002 2z" /></svg>
				{due.label}
			</span>
		{/if}
		{#if next.remind_at}
			<span class="task-chip" data-kind="reminder" title="Reminder {new Date(next.remind_at).toLocaleString()}">
				<svg aria-hidden="true" class="w-3 h-3" fill="none" stroke="currentColor" viewBox="0 0 24 24"><path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M12 8v4l2.5 2.5M5 3L2 6m20 0l-3-3M12 21a8 8 0 100-16 8 8 0 000 16z" /></svg>
				{formatInstant(next.remind_at)}
			</span>
		{/if}
		{#if next.duration_minutes}
			<span class="task-chip" data-kind="duration">for {formatDuration(next.duration_minutes)}</span>
		{/if}
		{#if next.recurrence}
			<span class="task-chip" data-kind="recurrence">↻ {next.recurrence}</span>
		{/if}
		{#if next.priority < 4}
			<span class="task-chip" data-kind="priority" data-priority={next.priority}>
				<svg aria-hidden="true" class="w-3 h-3" fill="currentColor" viewBox="0 0 24 24"><path d="M5 3a1 1 0 011 1v.5h12.38a1 1 0 01.8 1.6L16.5 9.5l2.68 3.4a1 1 0 01-.8 1.6H6V21a1 1 0 11-2 0V4a1 1 0 011-1z" /></svg>
				P{next.priority}
			</span>
		{/if}
		{#if !task.parent_id}
			<span class="task-chip" data-kind="project">
				{#if project}<PlaceMark color={projectColor(project.color)} />{:else}#{/if}
				{placeName(next, projects, categories)}
			</span>
		{/if}
	</div>

	<div class="task-inline-foot">
		<span class="task-inline-hint">
			{#if limit?.shown}<span class="task-char-count" class:task-char-count-over={limit.over}>{limit.count}/{SUBTASK_MAX}</span> · {/if}Enter to save · Esc to cancel
		</span>
		<div class="task-inline-actions">
			<button type="button" class="tasks-icon-btn" onclick={ondone} aria-label="Cancel" title="Cancel (Esc)">
				<svg aria-hidden="true" class="w-4 h-4" fill="none" stroke="currentColor" viewBox="0 0 24 24"><path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M6 18L18 6M6 6l12 12" /></svg>
			</button>
			<button type="submit" class="task-inline-save" disabled={busy || limit?.over} aria-label="Save" title="Save (Enter)">
				<svg aria-hidden="true" class="w-4 h-4" fill="none" stroke="currentColor" viewBox="0 0 24 24"><path stroke-linecap="round" stroke-linejoin="round" stroke-width="2.5" d="M5 13l4 4L19 7" /></svg>
			</button>
		</div>
	</div>
</form>
