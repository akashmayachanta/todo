<script lang="ts">
	import { onMount } from 'svelte';
	import { goto } from '$app/navigation';
	import { resolve } from '$app/paths';

	// The app is the Tasks page; its links and the ?task= deep link use /tasks
	// (under /tasks-app on tasks-server's web page).
	onMount(() => {
		goto(resolve('/tasks' as any), { replaceState: true });
	});
</script>
