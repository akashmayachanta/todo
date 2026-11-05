<script lang="ts">
	import './quickadd.css';

	/** An info button that opens a card of what the smart task line understands. */
	let { align = 'right' }: { align?: 'left' | 'right' } = $props();

	const uid = $props.id();
	let open = $state(false);
	let root = $state<HTMLDivElement | null>(null);

	const TIPS = [
		{ what: 'Date', how: ['today', 'tod', 'tomorrow', 'tom', 'next fri', 'in 3 days', '12/11'] },
		{ what: 'Time', how: ['5pm', '10p', 'tom 9:30a'], note: 'A time alone lands on the day you are adding to or the task already has, else today.' },
		{ what: 'Duration', how: ['for 30m', 'for 1h', 'for 1h30m', 'for 2 hours'] },
		{ what: 'Priority', how: ['p1', 'p2', 'p3', 'p4'], note: 'p1 is the highest.' },
		{ what: 'Project', how: ['#Project'], note: 'Type # to pick a project or category.' },
		{ what: 'Category', how: ['#Category', '/Category', '#Project /Category'], note: 'Type / to pick from categories only. If a project has the same name, # means the project.' },
		{ what: 'Repeat', how: ['every day', 'every weekday', 'every mon, thu', 'every 2 weeks'] },
		{ what: 'Reminder', how: ['!10m', '!30m', '!1h'], note: 'A due time reminds you at that time on its own; !10m makes it 10 minutes before instead. Or pick one in the task window.' },
	];

	function onWindowPointer(e: PointerEvent) {
		if (open && root && !root.contains(e.target as Node)) open = false;
	}

	function onKeydown(e: KeyboardEvent) {
		if (e.key === 'Escape' && open) {
			e.preventDefault();
			e.stopPropagation();
			open = false;
		}
	}
</script>

<svelte:window onpointerdown={onWindowPointer} />

<!-- svelte-ignore a11y_no_static_element_interactions -->
<div class="smart-tips" bind:this={root} onkeydown={onKeydown}>
	<button
		type="button"
		class="smart-tips-btn"
		aria-expanded={open}
		aria-controls="{uid}-tips"
		aria-label="Smart input tips"
		title="Smart input tips"
		onclick={() => (open = !open)}
	>
		<svg aria-hidden="true" class="w-4 h-4" fill="none" stroke="currentColor" viewBox="0 0 24 24"><path stroke-linecap="round" stroke-linejoin="round" stroke-width="1.75" d="M13 16h-1v-4h-1m1-4h.01M21 12a9 9 0 11-18 0 9 9 0 0118 0z" /></svg>
	</button>

	{#if open}
		<div id="{uid}-tips" class="smart-tips-card" class:smart-tips-card-left={align === 'left'} role="dialog" aria-label="Smart input tips">
			<p class="smart-tips-lead">Type these into a task's name and they set its fields:</p>
			<dl class="smart-tips-list">
				{#each TIPS as tip (tip.what)}
					<dt>{tip.what}</dt>
					<dd>
						<span class="smart-tips-codes">{#each tip.how as h}<code>{h}</code>{/each}</span>
						{#if tip.note}<span class="smart-tips-note">{tip.note}</span>{/if}
					</dd>
				{/each}
			</dl>
			<p class="smart-tips-example">e.g. <code>Call bank tomorrow 5pm p1 #Home !30m</code></p>
			<p class="smart-tips-where">
				Works in <strong>Add task</strong> (<kbd>Alt</kbd>+<kbd>N</kbd> anywhere), the
				<strong>Add task</strong> rows (<kbd>q</kbd>), and a task's title and subtask line in
				its side panel. Enter applies it.
			</p>
		</div>
	{/if}
</div>
