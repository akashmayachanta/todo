<script lang="ts">
	/**
	 * The mark before a project or category name: `#` for a project and `/` for a
	 * category, the characters that pick them when typing a task, in the project's
	 * colour. Filters and priority groups keep the plain dot (`.tasks-dot`).
	 */
	let {
		kind = 'project',
		color,
		large = false,
	}: { kind?: 'project' | 'category'; color: string | null | undefined; large?: boolean } = $props();
</script>

<span class="tasks-mark" class:tasks-mark-lg={large} data-kind={kind} style="color: {color ?? 'var(--text-muted)'}" aria-hidden="true">{kind === 'category' ? '/' : '#'}</span>
