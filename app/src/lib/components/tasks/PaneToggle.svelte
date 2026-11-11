<script lang="ts">
	import { paneCollapsed, togglePane } from '$lib/stores/paneCollapsed';

	/** The sidebar button: hides the Tasks pane, or brings it back. */
	let { class: klass = '' }: { class?: string } = $props();
</script>

<button
	type="button"
	class="tasks-pane-toggle {klass}"
	aria-label={$paneCollapsed ? 'Open sidebar' : 'Close sidebar'}
	aria-expanded={!$paneCollapsed}
	title={$paneCollapsed ? 'Open sidebar' : 'Close sidebar'}
	onclick={togglePane}
>
	<svg aria-hidden="true" fill="none" stroke="currentColor" viewBox="0 0 24 24">
		<rect x="3.5" y="4.5" width="17" height="15" rx="2.5" stroke-width="1.6" />
		<path stroke-linecap="round" stroke-width="1.6" d="M9.5 4.5v15" />
	</svg>
</button>
