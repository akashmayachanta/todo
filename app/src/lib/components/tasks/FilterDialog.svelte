<script lang="ts">
	import type {
		Task, TaskCategory, TaskDueWindow, TaskFilter, TaskFilterIn, TaskGroupBy, TaskPriority, TaskProject, TaskRowField,
	} from '$lib/api';
	import Select from '$lib/components/Select.svelte';
	import { PROJECT_COLORS, projectColor } from './shared';
	import {
		DEFAULT_FILTER_FIELDS, DUE_WINDOWS, FILTER_GROUPS, ROW_FIELDS, compileFilter, emptyCriteria, pruneCriteria,
	} from './taskFilters';

	/**
	 * New or edit filter: name and colour, what it matches (projects and their
	 * categories, priorities, due windows), which row details it shows and how it
	 * groups. The count of open tasks it matches updates as you pick.
	 */
	let { filter, projects, categories, tasks, today, onsave, onclose }: {
		/** null for a new filter. */
		filter: TaskFilter | null;
		projects: TaskProject[];
		categories: TaskCategory[];
		/** The open tasks, for the live count. */
		tasks: Task[];
		today: string;
		onsave: (data: TaskFilterIn) => Promise<void>;
		onclose: () => void;
	} = $props();

	const ctx = $derived({ projects, categories, today });

	// The dialog edits its own copy; a deleted project's id is dropped on open.
	// svelte-ignore state_referenced_locally
	const start = filter ? pruneCriteria(filter.criteria, { projects, categories, today }) : emptyCriteria();
	// svelte-ignore state_referenced_locally
	let name = $state(filter?.name ?? '');
	// svelte-ignore state_referenced_locally
	let color = $state(filter?.color ?? 'blue');
	let picked = $state<(string | null)[]>([...start.projects]);
	let pickedCats = $state<string[]>([...start.categories]);
	let priorities = $state<TaskPriority[]>([...start.priorities]);
	let due = $state<TaskDueWindow[]>([...start.due]);
	// svelte-ignore state_referenced_locally
	let fields = $state<TaskRowField[]>([...(filter?.fields ?? DEFAULT_FILTER_FIELDS)]);
	// svelte-ignore state_referenced_locally
	let groupBy = $state<TaskGroupBy>(filter?.group_by ?? 'none');
	let saving = $state(false);
	/** Every project the filter picked has been deleted: saving as is would widen it to every project. */
	// svelte-ignore state_referenced_locally
	const placesGone = !!filter && filter.criteria.projects.length > 0 && start.projects.length === 0;

	const PRIORITIES: TaskPriority[] = [1, 2, 3, 4];

	let criteria = $derived({ projects: picked, categories: pickedCats, priorities, due });
	let matching = $derived.by(() => {
		const test = compileFilter(criteria, ctx);
		return tasks.filter((t) => !t.completed_at && test(t)).length;
	});

	const catsOf = (projectId: string) =>
		categories.filter((c) => c.project_id === projectId).sort((a, b) => a.position - b.position || a.id.localeCompare(b.id));

	function toggle<T>(list: T[], value: T): T[] {
		return list.includes(value) ? list.filter((v) => v !== value) : [...list, value];
	}

	function toggleProject(id: string | null) {
		picked = toggle(picked, id);
		// Unpicking a project unpicks its categories.
		if (id !== null && !picked.includes(id)) {
			const own = new Set(catsOf(id).map((c) => c.id));
			pickedCats = pickedCats.filter((c) => !own.has(c));
		}
	}

	async function save() {
		if (!name.trim() || saving) return;
		saving = true;
		try {
			await onsave({
				name: name.trim(),
				color,
				criteria: pruneCriteria(criteria, ctx),
				// Kept in the order they are offered.
				fields: ROW_FIELDS.map((f) => f.value).filter((f) => fields.includes(f)),
				group_by: groupBy,
			});
		} catch {
			/* the page shows the error; the dialog stays open */
		} finally {
			saving = false;
		}
	}

	function focusOnMount(node: HTMLElement) {
		node.focus();
	}

	function onKeydown(e: KeyboardEvent) {
		if (e.key === 'Escape') {
			e.stopPropagation();
			onclose();
		}
	}
</script>

<svelte:window onkeydown={onKeydown} />

<div class="modal-backdrop" onclick={onclose} aria-hidden="true"></div>
<div class="modal-overlay">
	<div class="modal-panel max-w-lg" role="dialog" aria-modal="true" aria-labelledby="filter-dialog-title">
		<div class="modal-header">
			<h2 id="filter-dialog-title" class="modal-title">{filter ? 'Edit filter' : 'New filter'}</h2>
			<button onclick={onclose} class="modal-close" aria-label="Close">
				<svg aria-hidden="true" class="w-5 h-5" fill="none" stroke="currentColor" viewBox="0 0 24 24"><path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M6 18L18 6M6 6l12 12" /></svg>
			</button>
		</div>
		<form class="modal-body filter-form" onsubmit={(e) => { e.preventDefault(); save(); }}>
			<div>
				<label for="filter-name" class="label">Name</label>
				<input id="filter-name" class="input" bind:value={name} required maxlength="500" use:focusOnMount />
			</div>

			<fieldset>
				<legend class="label">Colour</legend>
				<div class="project-swatches">
					{#each PROJECT_COLORS as c (c.key)}
						<label class="project-swatch" title={c.label}>
							<input type="radio" name="filter-color" value={c.key} bind:group={color} class="sr-only" />
							<span class="project-swatch-dot" style="background-color: {c.value}"></span>
							<span class="sr-only">{c.label}</span>
						</label>
					{/each}
				</div>
			</fieldset>

			<fieldset>
				<legend class="label">Projects <span class="filter-hint">none picked: any</span></legend>
				{#if placesGone && !picked.length}
					<p class="filter-warning" role="alert">The projects this filter picked were deleted. Pick one, or saving makes it match every project.</p>
				{/if}
				<div class="filter-places">
					<label class="tasks-setting">
						<input type="checkbox" checked={picked.includes(null)} onchange={() => toggleProject(null)} />
						<span>Inbox</span>
					</label>
					{#each projects as p (p.id)}
						{@const cats = catsOf(p.id)}
						<label class="tasks-setting">
							<input type="checkbox" checked={picked.includes(p.id)} onchange={() => toggleProject(p.id)} />
							<span class="tasks-dot" style="background-color: {projectColor(p.color)}"></span>
							<span>{p.name}</span>
						</label>
						{#if picked.includes(p.id) && cats.length}
							<div class="filter-cats" role="group" aria-label="Categories of {p.name}">
								<span class="filter-hint">Categories (none picked: all)</span>
								{#each cats as c (c.id)}
									<label class="tasks-setting tasks-setting-sub">
										<input type="checkbox" checked={pickedCats.includes(c.id)} onchange={() => (pickedCats = toggle(pickedCats, c.id))} />
										<span>{c.name}</span>
									</label>
								{/each}
							</div>
						{/if}
					{/each}
				</div>
			</fieldset>

			<fieldset>
				<legend class="label">Priority <span class="filter-hint">none picked: any</span></legend>
				<div class="filter-chips">
					{#each PRIORITIES as p (p)}
						<button type="button" class="filter-chip" data-priority={p} aria-pressed={priorities.includes(p)} onclick={() => (priorities = toggle(priorities, p))}>
							P{p}
						</button>
					{/each}
				</div>
			</fieldset>

			<fieldset>
				<legend class="label">Due <span class="filter-hint">none picked: any</span></legend>
				<div class="filter-chips">
					{#each DUE_WINDOWS as w (w.value)}
						<button type="button" class="filter-chip" aria-pressed={due.includes(w.value)} onclick={() => (due = toggle(due, w.value))}>
							{w.label}
						</button>
					{/each}
				</div>
			</fieldset>

			<fieldset>
				<legend class="label">Show on each task</legend>
				<div class="filter-fields">
					{#each ROW_FIELDS as f (f.value)}
						<label class="tasks-setting">
							<input type="checkbox" checked={fields.includes(f.value)} onchange={() => (fields = toggle(fields, f.value))} />
							<span>{f.label}</span>
						</label>
					{/each}
				</div>
			</fieldset>

			<div>
				<label for="filter-group" class="label">Group by</label>
				<Select id="filter-group" bind:value={groupBy} options={FILTER_GROUPS} />
			</div>

			<div class="filter-foot">
				<p class="filter-count" aria-live="polite">Matches {matching} open task{matching === 1 ? '' : 's'}</p>
				<div class="flex gap-3">
					<button type="button" class="btn btn-secondary" onclick={onclose}>Cancel</button>
					<button type="submit" class="btn btn-primary" disabled={!name.trim() || saving}>{filter ? 'Save' : 'Add filter'}</button>
				</div>
			</div>
		</form>
	</div>
</div>
