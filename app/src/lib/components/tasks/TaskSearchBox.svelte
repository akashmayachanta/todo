<script lang="ts">
	import { onMount } from 'svelte';

	/**
	 * The search box: results follow as you type, Esc clears it. `autofocus` is for the
	 * phone's box on the results page, which takes over from the one in the Browse tab.
	 */
	let { query, onsearch, class: className = '', autofocus = false }: {
		/** The search on screen. */
		query: string;
		onsearch: (text: string) => void;
		class?: string;
		autofocus?: boolean;
	} = $props();

	const DEBOUNCE_MS = 150;

	let input = $state<HTMLInputElement | null>(null);
	let text = $state('');
	let timer: ReturnType<typeof setTimeout> | undefined;

	export function focus() {
		input?.focus();
		input?.select();
	}

	// Leaving the search from elsewhere (a link in the pane) clears the box; while
	// typing, the search catching up mustn't overwrite what has been typed since.
	$effect(() => {
		const q = query;
		if (document.activeElement !== input) text = q;
	});

	function onInput() {
		clearTimeout(timer);
		timer = setTimeout(() => onsearch(text), DEBOUNCE_MS);
	}

	function onKeydown(e: KeyboardEvent) {
		if (e.key !== 'Escape') return;
		e.preventDefault();
		e.stopPropagation();
		clearTimeout(timer);
		text = '';
		onsearch('');
		input?.blur();
	}

	onMount(() => {
		// Only if it's showing: a box hidden by the layout can't take the focus.
		if (autofocus && input?.offsetParent) input.focus();
		return () => clearTimeout(timer);
	});
</script>

<div class="task-search {className}" role="search">
	<svg aria-hidden="true" class="task-search-icon" fill="none" stroke="currentColor" viewBox="0 0 24 24"><path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M21 21l-4.35-4.35M17 11a6 6 0 11-12 0 6 6 0 0112 0z" /></svg>
	<input
		bind:this={input}
		bind:value={text}
		class="task-search-input"
		type="text"
		placeholder="Search"
		aria-label="Search tasks (Ctrl+K)"
		autocomplete="off"
		spellcheck="false"
		oninput={onInput}
		onkeydown={onKeydown}
	/>
	{#if !text}<kbd class="task-search-key" aria-hidden="true">Ctrl K</kbd>{/if}
</div>
