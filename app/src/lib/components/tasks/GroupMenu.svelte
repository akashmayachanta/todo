<script lang="ts" generics="T extends string">
	import Select from '$lib/components/Select.svelte';

	/** "Group" picker in the Tasks header: how the list's sections and the board's columns are made. */
	let { value, options, onchange }: {
		value: T;
		options: readonly { value: T; label: string }[];
		onchange: (value: T) => void;
	} = $props();

	const uid = $props.id();
</script>

<div class="tasks-group-menu">
	<label class="tasks-group-label" for="tasks-group-{uid}">Group</label>
	<Select id="tasks-group-{uid}" class="tasks-group-select" {value} options={[...options]} {onchange} />
</div>
