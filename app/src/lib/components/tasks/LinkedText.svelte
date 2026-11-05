<script lang="ts">
	import { openUrl } from '@tauri-apps/plugin-opener';
	import { linkParts } from '$lib/links';

	/** Task text with its links live; they open in the default browser. `linksOnly` leaves out the words around them. */
	let { text, linksOnly = false }: { text: string; linksOnly?: boolean } = $props();

	let parts = $derived(linkParts(text).filter((p) => p.url || !linksOnly));

	function open(e: MouseEvent, url: string) {
		e.preventDefault();
		e.stopPropagation(); // Not a click on the row around it.
		openUrl(url).catch(() => window.open(url, '_blank', 'noopener'));
	}
</script>

{#each parts as part}{#if part.url}<a class="task-link" href={part.url} title={part.url} draggable="false" onclick={(e) => open(e, part.url!)}>{part.text}</a>{:else}{part.text}{/if}{/each}
