<script lang="ts">
	import { onMount } from 'svelte';
	import type { TaskProject, TaskSection } from '$lib/api';
	import DropdownMenu from '$lib/components/DropdownMenu.svelte';
	import { withFlip } from '$lib/flip';
	import { placeBeside, sameOrder, stepTarget } from '$lib/ordering';
	import { sortMove, type MoveDrop } from '$lib/sortable';
	import { formatDuration } from '$lib/taskParse';
	import PlaceMark from './PlaceMark.svelte';
	import { projectColor } from './shared';

	/**
	 * The Projects screen: every project with its sections under it. A project is dragged
	 * by its row to reorder it (its sections go along); a section is dragged to another
	 * place in its project, or into another project, where its tasks go with it. The
	 * menus do the same from the keyboard.
	 */
	let {
		projects, sections, counts, minutes, showCounts, board = false,
		oneditproject, ondeleteproject, onreorderprojects,
		onplacesection, onmovesection, onrenamesection, ondeletesection,
	}: {
		projects: TaskProject[];
		sections: TaskSection[];
		counts: { projects: Record<string, number>; sections: Record<string, number> };
		minutes: { projects: Record<string, number>; sections: Record<string, number> };
		showCounts: boolean;
		/** Projects side by side as columns, rather than one under another. */
		board?: boolean;
		oneditproject: (project: TaskProject) => void;
		ondeleteproject: (project: TaskProject) => void;
		onreorderprojects: (ids: string[]) => void;
		/** Puts a section before or after another (`targetId`), or last, in a project. */
		onplacesection: (id: string, projectId: string, targetId: string | null, after: boolean) => void;
		/** Opens "Move to" for a section. */
		onmovesection: (section: TaskSection) => void;
		onrenamesection: (section: TaskSection, name: string) => void;
		ondeletesection: (section: TaskSection) => void;
	} = $props();

	let byProject = $derived.by(() => {
		const map = new Map<string, TaskSection[]>();
		for (const c of [...sections].sort((a, b) => a.position - b.position || a.id.localeCompare(b.id))) {
			const list = map.get(c.project_id) ?? [];
			list.push(c);
			map.set(c.project_id, list);
		}
		return map;
	});

	const count = (n: number | undefined) => (showCounts && n ? n : 0);

	/** Which projects have their sections hidden, remembered in this browser. */
	const COLLAPSED_KEY = 'tasks-projects-collapsed';
	let collapsed = $state<string[]>([]);

	onMount(() => {
		try {
			const list = JSON.parse(localStorage.getItem(COLLAPSED_KEY) ?? '[]');
			if (Array.isArray(list)) collapsed = list.filter((n) => typeof n === 'string');
		} catch {
			/* storage blocked or bad value: all expanded */
		}
	});

	function setCollapsed(ids: string[]) {
		collapsed = ids;
		try {
			localStorage.setItem(COLLAPSED_KEY, JSON.stringify(ids));
		} catch {
			/* storage blocked: it is forgotten on reload */
		}
	}

	function toggle(id: string) {
		setCollapsed(collapsed.includes(id) ? collapsed.filter((x) => x !== id) : [...collapsed, id]);
	}

	/** Projects that have sections to hide. */
	let foldable = $derived(projects.filter((p) => byProject.has(p.id)));
	let allCollapsed = $derived(foldable.length > 0 && foldable.every((p) => collapsed.includes(p.id)));

	function moveProject(id: string, targetId: string, after: boolean) {
		const current = projects.map((p) => p.id);
		const ids = placeBeside(current, id, targetId, after);
		if (ids && !sameOrder(ids, current)) onreorderprojects(ids);
	}

	/** A drag let go (sortable.ts glides the rows to their new places). */
	function drop(d: MoveDrop) {
		if (d.kind === 'project') {
			if (d.targetId) moveProject(d.id, d.targetId, d.after);
		} else if (d.zone) {
			onplacesection(d.id, d.zone, d.targetId, d.after);
		}
	}

	function stepProject(i: number, by: -1 | 1) {
		const id = projects[i].id;
		const to = stepTarget(projects.map((p) => p.id), id, by);
		if (to) withFlip(() => moveProject(id, to.targetId, to.after));
	}

	function stepSection(c: TaskSection, by: -1 | 1) {
		const to = stepTarget((byProject.get(c.project_id) ?? []).map((x) => x.id), c.id, by);
		if (to) withFlip(() => onplacesection(c.id, c.project_id, to.targetId, to.after));
	}

	/** The section whose name is being typed over. */
	let renaming = $state<string | null>(null);
	let name = $state('');

	function saveName(c: TaskSection) {
		if (renaming !== c.id) return;
		renaming = null;
		onrenamesection(c, name);
	}

	function nameKey(e: KeyboardEvent, c: TaskSection) {
		if (e.key === 'Enter') {
			e.preventDefault();
			saveName(c);
		} else if (e.key === 'Escape') {
			e.preventDefault();
			e.stopPropagation();
			renaming = null;
		}
	}

	function focusNow(node: HTMLInputElement) {
		node.focus();
		node.select();
	}

	function onSectionMenu(c: TaskSection, id: string) {
		if (id === 'rename') {
			name = c.name;
			renaming = c.id;
		} else if (id === 'move') onmovesection(c);
		else if (id === 'up') stepSection(c, -1);
		else if (id === 'down') stepSection(c, 1);
		else ondeletesection(c);
	}
</script>

{#snippet dots()}
	<svg aria-hidden="true" class="w-4 h-4" fill="currentColor" viewBox="0 0 20 20"><path d="M6 10a2 2 0 11-4 0 2 2 0 014 0zm6 0a2 2 0 11-4 0 2 2 0 014 0zm6 0a2 2 0 11-4 0 2 2 0 014 0z" /></svg>
{/snippet}

{#if foldable.length}
	<div class="projects-tools">
		<button type="button" class="btn btn-ghost" onclick={() => setCollapsed(allCollapsed ? [] : foldable.map((p) => p.id))}>
			{allCollapsed ? 'Expand all' : 'Collapse all'}
		</button>
	</div>
{/if}

<div class={board ? 'projects-list projects-board task-board' : 'projects-list'}>
	{#each projects as p, i (p.id)}
		{@const cats = byProject.get(p.id) ?? []}
		{@const open = !collapsed.includes(p.id)}
		<section class="projects-card" data-move-id={p.id} data-move-kind="project" data-move-zone={p.id} data-flip="projects-card:{p.id}" aria-label={p.name}>
			<div class="projects-row projects-row-project" use:sortMove={{ id: p.id, kind: 'project', ondrop: drop }}>
				{#if cats.length}
					<button
						type="button"
						class="tasks-pane-expander projects-expander"
						aria-expanded={open}
						aria-label="{open ? 'Hide' : 'Show'} sections of {p.name}"
						onclick={() => toggle(p.id)}
					>
						<svg aria-hidden="true" class="w-3 h-3" fill="none" stroke="currentColor" viewBox="0 0 24 24"><path stroke-linecap="round" stroke-linejoin="round" stroke-width="2.5" d="M9 5l7 7-7 7" /></svg>
					</button>
				{:else}
					<span class="projects-expander" aria-hidden="true"></span>
				{/if}
				<PlaceMark color={projectColor(p.color)} />
				<a href="?project={p.id}" class="projects-name" draggable="false">{p.name}</a>
				{#if cats.length && !open}<span class="tasks-pane-count">{cats.length} {cats.length === 1 ? 'section' : 'sections'}</span>{/if}
				{#if count(minutes.projects[p.id])}<span class="tasks-pane-count tasks-pane-time">{formatDuration(minutes.projects[p.id])}</span>{/if}
				{#if count(counts.projects[p.id])}<span class="tasks-pane-count">{count(counts.projects[p.id])}</span>{/if}
				<DropdownMenu
					triggerClass="tasks-icon-btn"
					ariaLabel="Project actions for {p.name}"
					items={[
						{ id: 'edit', label: 'Edit' },
						{ id: 'up', label: 'Move up', disabled: i === 0 },
						{ id: 'down', label: 'Move down', disabled: i === projects.length - 1 },
						{ id: 'delete', label: 'Delete project', danger: true },
					]}
					onselect={(id) => (id === 'edit' ? oneditproject(p) : id === 'up' ? stepProject(i, -1) : id === 'down' ? stepProject(i, 1) : ondeleteproject(p))}
					icon={dots}
				/>
			</div>
			{#if cats.length && open}
				<ul class="projects-sections" aria-label="Sections of {p.name}">
					{#each cats as c, j (c.id)}
						<li
							class="projects-row projects-row-section"
							data-move-id={c.id}
							data-move-kind="section"
							data-flip="projects-section:{c.id}"
							use:sortMove={renaming === c.id ? null : { id: c.id, kind: 'section', ondrop: drop }}
						>
							<PlaceMark kind="section" color={projectColor(p.color)} />
							{#if renaming === c.id}
								<input
									class="projects-rename"
									bind:value={name}
									aria-label="Section name"
									use:focusNow
									onkeydown={(e) => nameKey(e, c)}
									onblur={() => saveName(c)}
								/>
							{:else}
								<a href="?project={p.id}#task-cat-{c.id}" class="projects-name" draggable="false">{c.name}</a>
								{#if count(minutes.sections[c.id])}<span class="tasks-pane-count tasks-pane-time">{formatDuration(minutes.sections[c.id])}</span>{/if}
								{#if count(counts.sections[c.id])}<span class="tasks-pane-count">{count(counts.sections[c.id])}</span>{/if}
								<DropdownMenu
									triggerClass="tasks-icon-btn"
									ariaLabel="Section actions for {c.name}"
									items={[
										{ id: 'rename', label: 'Rename' },
										{ id: 'move', label: 'Move to project', disabled: projects.length < 2 },
										{ id: 'up', label: 'Move up', disabled: j === 0 },
										{ id: 'down', label: 'Move down', disabled: j === cats.length - 1 },
										{ id: 'delete', label: 'Delete section', danger: true },
									]}
									onselect={(id) => onSectionMenu(c, id)}
									icon={dots}
								/>
							{/if}
						</li>
					{/each}
				</ul>
			{:else if !cats.length}
				<p class="projects-none">No sections</p>
			{/if}
		</section>
	{:else}
		<p class="tasks-empty">No projects yet.</p>
	{/each}
</div>
