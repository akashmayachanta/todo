<script lang="ts">
	import { onMount } from 'svelte';
	import { goto } from '$app/navigation';

	// The app is the Tasks page; its links and the ?task= deep link use /tasks.
	onMount(() => {
		goto('/tasks', { replaceState: true });
	});
</script>
