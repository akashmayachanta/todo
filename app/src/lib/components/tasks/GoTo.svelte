<script lang="ts">
	import PlaceMark from '$lib/components/tasks/PlaceMark.svelte';
	import type { TaskFilter, TaskProject, TaskSection } from '$lib/api';
	import { plainText } from '$lib/links';
	import { projectColor } from './shared';
	import './quickadd.css';

	/**
	 * The quick add's "go to" line, opened by typing `>`: the views, projects, sections and
	 * filters whose names match, best first. Up / Down pick, Enter goes there (`ongo` with
	 * the Tasks page's query string, plus `#task-cat-<id>` for a section), Esc cancels,
	 * Backspace on an empty line goes back to adding a task.
	 */
	let { projects, sections, filters, initial = '', ongo, onback, oncancel }: {
		projects: TaskProject[];
		sections: TaskSection[];
		filters: TaskFilter[];
		/** What was typed after the `>`. */
		initial?: string;
		ongo: (query: string) => void;
		onback: () => void;
		oncancel: () => void;
	} = $props();

	interface Place {
		key: string;
		name: string;
		kind: 'view' | 'project' | 'section' | 'filter';
		/** What the list says next to the name: the kind, or a section's project. */
		label: string;
		color: string | null;
		query: string;
	}

	const VIEWS: Place[] = [
		{ key: 'v-inbox', name: 'Inbox', kind: 'view', label: 'view', color: null, query: 'view=inbox' },
		{ key: 'v-today', name: 'Today', kind: 'view', label: 'view', color: null, query: 'view=today' },
		{ key: 'v-upcoming', name: 'Upcoming', kind: 'view', label: 'view', color: null, query: 'view=upcoming' },
		{ key: 'v-projects', name: 'Projects', kind: 'view', label: 'view', color: null, query: 'view=projects' },
	];
	const MAX = 8;

	// svelte-ignore state_referenced_locally
	let text = $state(initial);
	let picked = $state(0);
	let input = $state<HTMLInputElement | null>(null);

	let places = $derived.by<Place[]>(() => {
		const ordered = [...projects].sort((a, b) => Number(b.favorite) - Number(a.favorite) || a.position - b.position);
		const projectPlaces = ordered.map((p): Place => ({
			key: `p-${p.id}`, name: plainText(p.name), kind: 'project', label: 'project', color: projectColor(p.color),
			query: `project=${encodeURIComponent(p.id)}`,
		}));
		// Each project's sections, in the projects' order.
		const sectionPlaces = ordered.flatMap((p) =>
			sections
				.filter((s) => s.project_id === p.id)
				.sort((a, b) => a.position - b.position)
				.map((s): Place => ({
					key: `s-${s.id}`, name: plainText(s.name), kind: 'section', label: `in ${plainText(p.name)}`, color: projectColor(p.color),
					query: `project=${encodeURIComponent(p.id)}#task-cat-${s.id}`,
				})),
		);
		const filterPlaces = [...filters]
			.sort((a, b) => a.position - b.position)
			.map((f): Place => ({
				key: `f-${f.id}`, name: plainText(f.name), kind: 'filter', label: 'filter', color: projectColor(f.color),
				query: `filter=${encodeURIComponent(f.id)}`,
			}));
		return [...VIEWS, ...projectPlaces, ...sectionPlaces, ...filterPlaces];
	});

	/** Names starting with the words first, then names containing them; the list order otherwise. */
	let matches = $derived.by(() => {
		const q = text.trim().toLowerCase();
		if (!q) return places.slice(0, MAX);
		const rank = (p: Place) => {
			const n = p.name.toLowerCase();
			if (n.startsWith(q)) return 0;
			if (n.split(/\s+/).some((w) => w.startsWith(q))) return 1;
			return n.includes(q) ? 2 : -1;
		};
		return places
			.map((p, i) => ({ p, r: rank(p), i }))
			.filter((x) => x.r >= 0)
			.sort((a, b) => a.r - b.r || a.i - b.i)
			.slice(0, MAX)
			.map((x) => x.p);
	});

	$effect(() => {
		// A new list starts at its best match.
		void matches;
		picked = 0;
	});

	function go(place: Place | undefined) {
		if (place) ongo(place.query);
	}

	function onKeydown(e: KeyboardEvent) {
		if (e.key === 'ArrowDown') {
			e.preventDefault();
			if (matches.length) picked = (picked + 1) % matches.length;
		} else if (e.key === 'ArrowUp') {
			e.preventDefault();
			if (matches.length) picked = (picked - 1 + matches.length) % matches.length;
		} else if (e.key === 'Enter') {
			e.preventDefault();
			go(matches[picked]);
		} else if (e.key === 'Escape') {
			e.preventDefault();
			e.stopPropagation();
			oncancel();
		} else if (e.key === 'Backspace' && !text) {
			e.preventDefault();
			onback();
		}
	}

	export function focus() {
		input?.focus();
	}

	$effect(() => {
		input?.focus();
	});
</script>

<div class="go-to">
	<div class="go-to-line">
		<span class="go-to-prompt" aria-hidden="true">&gt;</span>
		<input
			bind:this={input}
			bind:value={text}
			class="quick-add-input go-to-input"
			placeholder="Go to a project, section, view or filter"
			aria-label="Go to"
			role="combobox"
			aria-expanded="true"
			aria-controls="go-to-list"
			aria-activedescendant={matches[picked] ? `go-to-${matches[picked].key}` : undefined}
			autocomplete="off"
			spellcheck="false"
			onkeydown={onKeydown}
		/>
	</div>
	{#if matches.length}
		<ul id="go-to-list" class="go-to-list" role="listbox" aria-label="Places">
			{#each matches as place, i (place.key)}
				<li
					id="go-to-{place.key}"
					class="go-to-item"
					role="option"
					aria-selected={i === picked}
					onpointerenter={() => (picked = i)}
					onpointerdown={(e) => { e.preventDefault(); go(place); }}
				>
					{#if place.color}<PlaceMark color={place.color} />{/if}
					<span class="go-to-name">{place.name}</span>
					<span class="go-to-kind">{place.label}</span>
				</li>
			{/each}
		</ul>
	{:else}
		<p class="go-to-none">Nothing called “{text.trim()}”</p>
	{/if}
	<p class="go-to-hint">↑↓ pick · Enter go · Backspace back to adding</p>
</div>
