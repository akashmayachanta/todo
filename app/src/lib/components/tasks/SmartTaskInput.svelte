<script lang="ts">
	import type { TaskCategory, TaskProject } from '$lib/api';
	import type { ParsedToken, TokenKind } from '$lib/taskParse';
	import { projectColor } from './shared';
	import './quickadd.css';

	/**
	 * The smart task line, shared by the quick add and the task drawer: the parts the
	 * parent read as date, priority, place, recurrence or reminder (`tokens`) are tinted
	 * in a mirror behind the input, `#` offers the projects and categories, and `/` the categories.
	 * Enter calls `onenter`, Esc `onescape` (when given), unless a suggestion list is open.
	 */
	let {
		value = $bindable(''),
		tokens,
		projects,
		categories,
		scopeProjectId = null,
		placeholder = '',
		ariaLabel,
		class: className = '',
		inputClass = '',
		onenter,
		onescape,
		onblur,
		onpaste,
	}: {
		value?: string;
		tokens: ParsedToken[];
		projects: TaskProject[];
		categories: TaskCategory[];
		/** The project whose categories `/` offers; null offers every category. */
		scopeProjectId?: string | null;
		placeholder?: string;
		ariaLabel: string;
		/** Extra classes for the wrapper (font, border). */
		class?: string;
		inputClass?: string;
		onenter?: () => void;
		onescape?: () => void;
		onblur?: () => void;
		onpaste?: (e: ClipboardEvent) => void;
	} = $props();

	let caret = $state(0);
	let input = $state<HTMLInputElement | null>(null);
	let mirror = $state<HTMLDivElement | null>(null);
	let active = $state(0);
	let dismissedAt = $state<string | null>(null);

	export function focus() {
		input?.focus();
	}

	// The line split into plain text and highlighted tokens, for the mirror behind the input.
	let segments = $derived.by(() => {
		const out: { text: string; kind?: TokenKind; unknown?: boolean }[] = [];
		let at = 0;
		for (const t of [...tokens].sort((a, b) => a.start - b.start)) {
			if (t.start < at || t.end > value.length) continue;
			if (t.start > at) out.push({ text: value.slice(at, t.start) });
			out.push({ text: value.slice(t.start, t.end), kind: t.kind, unknown: t.unknown });
			at = t.end;
		}
		if (at < value.length) out.push({ text: value.slice(at) });
		return out;
	});

	// `#` or `/` right before the caret, with what has been typed after it.
	let suggest = $derived.by(() => {
		const before = value.slice(0, caret);
		const m = /(^|\s)([#/])([^#/\n]*)$/.exec(before);
		if (!m || dismissedAt === before) return null;
		const query = m[3].toLowerCase().trimStart();
		const start = before.length - m[3].length - 1;
		// "#Work / Back" is still the category; "and / or" is just a slash.
		const afterProject = tokens.some((t) => t.kind === 'project' && !t.unknown && t.end <= start && !value.slice(t.end, start).trim());
		if (m[2] === '/' && /^\s/.test(m[3]) && !afterProject) return null;
		if (m[2] === '#') {
			const items = [
				...projects
					.filter((p) => p.name.toLowerCase().includes(query))
					.map((p) => ({ id: p.id, label: p.name, insert: `#${p.name}`, color: projectColor(p.color) })),
				...hashCategories(query, start),
			];
			return items.length ? { start, trigger: '#', items: items.slice(0, 8) } : null;
		}
		const items = categories
			.filter((c) => (scopeProjectId === null || c.project_id === scopeProjectId) && c.name.toLowerCase().includes(query))
			.map((c) => {
				const p = projects.find((x) => x.id === c.project_id);
				return { id: c.id, label: scopeProjectId === null && p ? `${p.name} / ${c.name}` : c.name, insert: `/${c.name}`, color: projectColor(p?.color) };
			});
		return items.length ? { start, trigger: '/', items: items.slice(0, 8) } : null;
	});

	/**
	 * The categories `#` offers, the current project's first. Picking one shows where
	 * it is, `#Work / Backend`, or just `/Backend` when the line already names a project.
	 */
	function hashCategories(query: string, start: number) {
		const named = tokens.some((t) => t.kind === 'project' && !t.unknown && t.start !== start);
		const inScope = (c: TaskCategory) => c.project_id === scopeProjectId;
		return categories
			.filter((c) => c.name.toLowerCase().includes(query) && (!named || inScope(c)))
			.sort((a, b) => Number(inScope(b)) - Number(inScope(a)))
			.map((c) => {
				const p = projects.find((x) => x.id === c.project_id);
				return {
					id: c.id,
					label: p ? `${p.name} / ${c.name}` : c.name,
					insert: named ? `/${c.name}` : p ? `#${p.name} / ${c.name}` : `#${c.name}`,
					color: projectColor(p?.color),
				};
			});
	}

	$effect(() => {
		// Keep the highlighted item in range as the list narrows.
		if (suggest && active >= suggest.items.length) active = 0;
	});

	// A value set from outside (cleared after an add, reset to a saved title) moves the caret to its end.
	$effect(() => {
		if (caret > value.length) caret = value.length;
	});

	function syncCaret() {
		if (!input) return;
		caret = input.selectionStart ?? value.length;
		if (mirror) mirror.scrollLeft = input.scrollLeft;
	}

	function pickSuggestion(i: number) {
		if (!suggest || !input) return;
		const item = suggest.items[i];
		const insert = `${item.insert} `;
		const after = value.slice(caret).replace(/^\S*/, '').replace(/^\s/, '');
		value = value.slice(0, suggest.start) + insert + after;
		const pos = suggest.start + insert.length;
		active = 0;
		queueMicrotask(() => {
			input?.setSelectionRange(pos, pos);
			syncCaret();
		});
	}

	function onKeydown(e: KeyboardEvent) {
		if (suggest) {
			if (e.key === 'ArrowDown') { e.preventDefault(); active = (active + 1) % suggest.items.length; return; }
			if (e.key === 'ArrowUp')   { e.preventDefault(); active = (active - 1 + suggest.items.length) % suggest.items.length; return; }
			if (e.key === 'Enter' || e.key === 'Tab') { e.preventDefault(); pickSuggestion(active); return; }
			if (e.key === 'Escape') { e.preventDefault(); e.stopPropagation(); dismissedAt = value.slice(0, caret); return; }
		}
		if (e.key === 'Enter') { e.preventDefault(); onenter?.(); }
		else if (e.key === 'Escape' && onescape) { e.preventDefault(); e.stopPropagation(); onescape(); }
	}
</script>

<div class="quick-add-field {className}">
	<div class="quick-add-mirror" bind:this={mirror} aria-hidden="true">{#each segments as seg}{#if seg.kind}<mark class="quick-add-token" data-kind={seg.kind} class:quick-add-token-unknown={seg.unknown}>{seg.text}</mark>{:else}{seg.text}{/if}{/each}</div>
	<input
		bind:this={input}
		bind:value
		class="quick-add-input {inputClass}"
		type="text"
		{placeholder}
		aria-label={ariaLabel}
		autocomplete="off"
		spellcheck="false"
		oninput={syncCaret}
		onkeyup={syncCaret}
		onclick={syncCaret}
		onscroll={syncCaret}
		onkeydown={onKeydown}
		{onblur}
		{onpaste}
	/>

	{#if suggest}
		<div class="quick-add-suggest" role="listbox" aria-label={suggest.trigger === '#' ? 'Projects and categories' : 'Categories'}>
			{#each suggest.items as item, i (item.id)}
				<!-- Focus stays in the input; options take pointer input only. -->
				<!-- svelte-ignore a11y_click_events_have_key_events -->
				<div
					role="option"
					tabindex="-1"
					aria-selected={i === active}
					class="quick-add-option"
					onpointerdown={(e) => { e.preventDefault(); pickSuggestion(i); }}
					onpointermove={() => (active = i)}
				>
					<span class="tasks-dot" style="background-color: {item.color}"></span>
					{item.label}
				</div>
			{/each}
		</div>
	{/if}
</div>
