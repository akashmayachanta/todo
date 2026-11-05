<script lang="ts">
	import { sortGrip, type SortApi, type SortKind } from '$lib/sortable';

	/** The six-dot handle that moves a task: drag it, or press the arrow keys on it. */
	let { id, kind, name, sort }: { id: string; kind: SortKind; name: string; sort: SortApi } = $props();
</script>

<button
	type="button"
	class="task-grip"
	aria-label="Move {name}. Use the up and down arrow keys."
	title="Drag to move"
	use:sortGrip={{ id, kind, ondrop: sort.ondrop, onstep: sort.onstep }}
>
	<svg aria-hidden="true" viewBox="0 0 10 16" fill="currentColor">
		<circle cx="2.5" cy="3" r="1.25" /><circle cx="7.5" cy="3" r="1.25" />
		<circle cx="2.5" cy="8" r="1.25" /><circle cx="7.5" cy="8" r="1.25" />
		<circle cx="2.5" cy="13" r="1.25" /><circle cx="7.5" cy="13" r="1.25" />
	</svg>
</button>
