<script lang="ts">
	import { onMount } from 'svelte';
	import { page } from '$app/state';
	import type { TaskCategory, TaskFilter, TaskProject } from '$lib/api';
	import DropdownMenu from '$lib/components/DropdownMenu.svelte';
	import { getPaneFooter, getPaneHeader } from '$lib/paneHeader';
	import { formatDuration } from '$lib/taskParse';
	import { projectColor, type TaskSettings, type TaskView } from './shared';

	/**
	 * The Tasks page's own pane: the account (from the app shell) at the top, the fixed views
	 * (Karma, the progress charts, last), the saved filters, then the projects.
	 * A project with categories has an expander that lists them; a category links to
	 * its section of the project. Projects are reordered by dragging them, or with
	 * Move up / Move down in their menu. On phones it is the Browse tab.
	 */
	let {
		projects, categories, filters, counts, minutes, view, projectId, filterId, settings, browsing = false,
		onaddproject, oneditproject, ondeleteproject, onreorderprojects, onaddfilter, oneditfilter, ondeletefilter,
	}: {
		projects: TaskProject[];
		categories: TaskCategory[];
		filters: TaskFilter[];
		counts: {
			inbox: number; today: number; upcoming: number;
			projects: Record<string, number>; categories: Record<string, number>; filters: Record<string, number>;
		};
		/** Minutes of open work in each project and category. */
		minutes: { projects: Record<string, number>; categories: Record<string, number> };
		view: TaskView;
		projectId: string | null;
		filterId: string | null;
		settings: TaskSettings;
		/** Phones: the pane is the Browse tab, shown in place of the list. */
		browsing?: boolean;
		onaddproject: () => void;
		oneditproject: (project: TaskProject) => void;
		ondeleteproject: (project: TaskProject) => void;
		onreorderprojects: (ids: string[]) => void;
		onaddfilter: () => void;
		oneditfilter: (filter: TaskFilter) => void;
		ondeletefilter: (filter: TaskFilter) => void;
	} = $props();

	const VIEWS = [
		{ key: 'inbox',    label: 'Inbox',    icon: 'M20 13V6a2 2 0 00-2-2H6a2 2 0 00-2 2v7m16 0v5a2 2 0 01-2 2H6a2 2 0 01-2-2v-5m16 0h-2.586a1 1 0 00-.707.293l-2.414 2.414a1 1 0 01-.707.293h-3.172a1 1 0 01-.707-.293l-2.414-2.414A1 1 0 006.586 13H4' },
		{ key: 'today',    label: 'Today',    icon: 'M8 7V3m8 4V3m-9 8h10M5 21h14a2 2 0 002-2V7a2 2 0 00-2-2H5a2 2 0 00-2 2v12a2 2 0 002 2z' },
		{ key: 'upcoming', label: 'Upcoming', icon: 'M13 7l5 5m0 0l-5 5m5-5H6' },
		{ key: 'karma',    label: 'Karma',    icon: 'M4 20h16M7 16v-4m5 4V8m5 8v-7' },
	] as const;

	/** Which projects have their categories shown, remembered in this browser. */
	const EXPANDED_KEY = 'tasks-pane-expanded';
	let expanded = $state<string[]>([]);

	onMount(() => {
		try {
			const list = JSON.parse(localStorage.getItem(EXPANDED_KEY) ?? '[]');
			if (Array.isArray(list)) expanded = list.filter((n) => typeof n === 'string');
		} catch {
			/* storage blocked or bad value: start collapsed */
		}
	});

	function toggle(id: string) {
		expanded = expanded.includes(id) ? expanded.filter((x) => x !== id) : [...expanded, id];
		try {
			localStorage.setItem(EXPANDED_KEY, JSON.stringify(expanded));
		} catch {
			/* storage blocked: it is forgotten on reload */
		}
	}

	let byProject = $derived.by(() => {
		const map = new Map<string, TaskCategory[]>();
		for (const c of [...categories].sort((a, b) => a.position - b.position || a.id.localeCompare(b.id))) {
			const list = map.get(c.project_id) ?? [];
			list.push(c);
			map.set(c.project_id, list);
		}
		return map;
	});

	let hash = $derived(page.url.hash);

	// ── Reordering projects ──────────────────────────────────────────────────

	/** The project being dragged, and where it would land: before or after another. */
	let dragId = $state<string | null>(null);
	let dropAt = $state<{ id: string; after: boolean } | null>(null);

	/** Moves a project to sit before or after another, and saves the order if it changed. */
	function moveProject(id: string, targetId: string, after: boolean) {
		if (id === targetId) return;
		const ids = projects.map((p) => p.id).filter((x) => x !== id);
		const at = ids.indexOf(targetId);
		if (at < 0) return;
		ids.splice(after ? at + 1 : at, 0, id);
		if (ids.some((x, i) => x !== projects[i].id)) onreorderprojects(ids);
	}

	function step(p: TaskProject, by: -1 | 1) {
		const i = projects.findIndex((x) => x.id === p.id);
		const target = projects[i + by];
		if (target) moveProject(p.id, target.id, by === 1);
	}

	function projectDragStart(e: DragEvent, p: TaskProject) {
		dragId = p.id;
		if (e.dataTransfer) {
			e.dataTransfer.setData('text/plain', p.name);
			e.dataTransfer.effectAllowed = 'move';
		}
	}

	function projectDragEnd() {
		dragId = null;
		dropAt = null;
	}

	function projectDragOver(e: DragEvent, p: TaskProject) {
		if (dragId === null) return; // a task card, or something from outside
		e.preventDefault();
		if (e.dataTransfer) e.dataTransfer.dropEffect = 'move';
		const rect = (e.currentTarget as HTMLElement).getBoundingClientRect();
		const after = e.clientY > rect.top + rect.height / 2;
		dropAt = dragId === p.id ? null : { id: p.id, after };
	}

	function projectDrop(e: DragEvent) {
		if (dragId === null) return;
		e.preventDefault();
		if (dropAt) moveProject(dragId, dropAt.id, dropAt.after);
		projectDragEnd();
	}

	const header = getPaneHeader();
	const footer = getPaneFooter();

	/** Counts are hidden when Settings says so. */
	const count = (n: number | undefined) => (settings.showCounts && n ? n : 0);
</script>

<nav class="tasks-pane" class:tasks-pane-browsing={browsing} aria-label="Task lists">
	{#if header}
		<div class="tasks-pane-top">{@render header()}</div>
	{/if}
	<div class="tasks-pane-lists">
		<div class="tasks-pane-views">
			{#each VIEWS as v}
				<a href="?view={v.key}" class="tasks-pane-link" aria-current={view === v.key ? 'page' : undefined}>
					<svg aria-hidden="true" class="w-4 h-4 shrink-0" fill="none" stroke="currentColor" viewBox="0 0 24 24"><path stroke-linecap="round" stroke-linejoin="round" stroke-width="1.75" d={v.icon} /></svg>
					<span class="tasks-pane-label">{v.label}</span>
					{#if v.key !== 'karma' && count(counts[v.key])}<span class="tasks-pane-count">{count(counts[v.key])}</span>{/if}
				</a>
			{/each}
		</div>

		<div class="tasks-pane-head">
			<span>Filters</span>
			<button type="button" class="tasks-icon-btn" onclick={onaddfilter} title="Add filter" aria-label="Add filter">
				<svg aria-hidden="true" class="w-3.5 h-3.5" fill="none" stroke="currentColor" viewBox="0 0 24 24"><path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M12 5v14m7-7H5" /></svg>
			</button>
		</div>

		{#each filters as f (f.id)}
			{@const here = view === 'filter' && filterId === f.id}
			<div class="tasks-pane-project" class:tasks-pane-project-active={here}>
				<a href="?filter={f.id}" class="tasks-pane-link" aria-current={here ? 'page' : undefined}>
					<svg aria-hidden="true" class="tasks-pane-filter-icon w-4 h-4 shrink-0" style="color: {projectColor(f.color)}" fill="none" stroke="currentColor" viewBox="0 0 24 24"><path stroke-linecap="round" stroke-linejoin="round" stroke-width="1.75" d="M3 4h18l-7 8.5V19l-4 2v-8.5L3 4z" /></svg>
					<span class="tasks-pane-label">{f.name}</span>
					{#if count(counts.filters[f.id])}<span class="tasks-pane-count">{count(counts.filters[f.id])}</span>{/if}
				</a>
				<DropdownMenu
					class="tasks-pane-menu"
					triggerClass="tasks-icon-btn"
					ariaLabel="Filter actions for {f.name}"
					items={[{ id: 'edit', label: 'Edit filter' }, { id: 'delete', label: 'Delete filter', danger: true }]}
					onselect={(id) => (id === 'edit' ? oneditfilter(f) : ondeletefilter(f))}
				>
					{#snippet icon()}
						<svg aria-hidden="true" class="w-3.5 h-3.5" fill="currentColor" viewBox="0 0 20 20"><path d="M6 10a2 2 0 11-4 0 2 2 0 014 0zm6 0a2 2 0 11-4 0 2 2 0 014 0zm6 0a2 2 0 11-4 0 2 2 0 014 0z" /></svg>
					{/snippet}
				</DropdownMenu>
			</div>
		{:else}
			<p class="tasks-pane-empty">No filters yet</p>
		{/each}

		<div class="tasks-pane-head">
			<span>Projects</span>
			<button type="button" class="tasks-icon-btn" onclick={onaddproject} title="Add project" aria-label="Add project">
				<svg aria-hidden="true" class="w-3.5 h-3.5" fill="none" stroke="currentColor" viewBox="0 0 24 24"><path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M12 5v14m7-7H5" /></svg>
			</button>
		</div>

		{#each projects as p, i (p.id)}
			{@const cats = byProject.get(p.id) ?? []}
			{@const open = cats.length > 0 && expanded.includes(p.id)}
			{@const here = view === 'project' && projectId === p.id}
			<!-- Dragging is a mouse shortcut; the menu's Move up / Move down does the same from the keyboard. -->
			<!-- svelte-ignore a11y_no_static_element_interactions -->
			<div
				class="tasks-pane-project"
				class:tasks-pane-project-active={here}
				class:tasks-pane-dragging={dragId === p.id}
				class:tasks-pane-drop-before={dropAt?.id === p.id && !dropAt.after}
				class:tasks-pane-drop-after={dropAt?.id === p.id && dropAt.after}
				draggable="true"
				ondragstart={(e) => projectDragStart(e, p)}
				ondragend={projectDragEnd}
				ondragover={(e) => projectDragOver(e, p)}
				ondrop={projectDrop}
			>
				<a href="?project={p.id}" class="tasks-pane-link" aria-current={here && !hash.startsWith('#task-cat-') ? 'page' : undefined}>
					<span class="tasks-dot" style="background-color: {projectColor(p.color)}"></span>
					<span class="tasks-pane-label">{p.name}</span>
					{#if count(minutes.projects[p.id])}<span class="tasks-pane-count tasks-pane-time">{formatDuration(minutes.projects[p.id])}</span>{/if}
					{#if count(counts.projects[p.id])}<span class="tasks-pane-count">{count(counts.projects[p.id])}</span>{/if}
				</a>
				{#if cats.length}
					<button
						type="button"
						class="tasks-pane-expander"
						aria-expanded={open}
						aria-controls="tasks-pane-cats-{p.id}"
						aria-label="{open ? 'Hide' : 'Show'} categories of {p.name}"
						onclick={() => toggle(p.id)}
					>
						<svg aria-hidden="true" class="w-3 h-3" fill="none" stroke="currentColor" viewBox="0 0 24 24"><path stroke-linecap="round" stroke-linejoin="round" stroke-width="2.5" d="M9 5l7 7-7 7" /></svg>
					</button>
				{/if}
				<DropdownMenu
					class="tasks-pane-menu"
					triggerClass="tasks-icon-btn"
					ariaLabel="Project actions for {p.name}"
					items={[
						{ id: 'edit', label: 'Rename or recolour' },
						{ id: 'up', label: 'Move up', disabled: i === 0 },
						{ id: 'down', label: 'Move down', disabled: i === projects.length - 1 },
						{ id: 'delete', label: 'Delete project', danger: true },
					]}
					onselect={(id) =>
						id === 'edit' ? oneditproject(p)
						: id === 'up' ? step(p, -1)
						: id === 'down' ? step(p, 1)
						: ondeleteproject(p)}
				>
					{#snippet icon()}
						<svg aria-hidden="true" class="w-3.5 h-3.5" fill="currentColor" viewBox="0 0 20 20"><path d="M6 10a2 2 0 11-4 0 2 2 0 014 0zm6 0a2 2 0 11-4 0 2 2 0 014 0zm6 0a2 2 0 11-4 0 2 2 0 014 0z" /></svg>
					{/snippet}
				</DropdownMenu>
			</div>
			{#if open}
				<ul id="tasks-pane-cats-{p.id}" class="tasks-pane-cats" aria-label="Categories of {p.name}">
					{#each cats as c (c.id)}
						<li>
							<a
								href="?project={p.id}#task-cat-{c.id}"
								class="tasks-pane-link tasks-pane-cat"
								aria-current={here && hash === `#task-cat-${c.id}` ? 'page' : undefined}
							>
								<span class="tasks-pane-cat-mark" style="border-color: {projectColor(p.color)}"></span>
								<span class="tasks-pane-label">{c.name}</span>
								{#if count(minutes.categories[c.id])}<span class="tasks-pane-count tasks-pane-time">{formatDuration(minutes.categories[c.id])}</span>{/if}
								{#if count(counts.categories[c.id])}<span class="tasks-pane-count">{count(counts.categories[c.id])}</span>{/if}
							</a>
						</li>
					{/each}
				</ul>
			{/if}
		{:else}
			<p class="tasks-pane-empty">No projects yet</p>
		{/each}
	</div>

	{#if footer}
		<div class="tasks-pane-foot">{@render footer()}</div>
	{/if}
</nav>
