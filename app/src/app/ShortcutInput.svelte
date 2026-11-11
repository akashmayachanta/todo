<script lang="ts">
	import { keyLabels, keysFromEvent } from '$lib/shortcutKeys';

	/**
	 * A shortcut field for Settings → Desktop: click it and press the keys. Esc stops
	 * without a change; the × turns the shortcut off.
	 */
	let { id, value, onchange }: {
		id?: string;
		/** "Alt+N", or null when off. */
		value: string | null;
		onchange: (keys: string | null) => void;
	} = $props();

	let recording = $state(false);

	function onKeydown(e: KeyboardEvent) {
		if (!recording) {
			if (e.key === 'Enter' || e.key === ' ') {
				e.preventDefault();
				recording = true;
			}
			return;
		}
		e.preventDefault();
		e.stopPropagation();
		if (e.key === 'Escape' && !e.ctrlKey && !e.altKey && !e.shiftKey && !e.metaKey) {
			recording = false;
			return;
		}
		const keys = keysFromEvent(e);
		if (!keys) return;
		recording = false;
		if (keys !== value) onchange(keys);
	}
</script>

<div class="sc-field" class:sc-recording={recording}>
	<button
		{id}
		type="button"
		class="sc-keys"
		aria-label={recording ? 'Press the new shortcut' : value ? `Shortcut ${keyLabels(value).join(' ')}. Press Enter to change it` : 'No shortcut. Press Enter to set one'}
		onclick={() => (recording = true)}
		onkeydown={onKeydown}
		onblur={() => (recording = false)}
	>
		{#if recording}
			<span class="sc-hint">Press the keys…</span>
		{:else if value}
			{#each keyLabels(value) as k, i (i)}<kbd>{k}</kbd>{/each}
		{:else}
			<span class="sc-hint">Off</span>
		{/if}
	</button>
	{#if value && !recording}
		<button type="button" class="sc-clear" aria-label="Turn this shortcut off" onclick={() => onchange(null)}>
			<svg aria-hidden="true" fill="none" stroke="currentColor" viewBox="0 0 24 24"><path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M6 18L18 6M6 6l12 12" /></svg>
		</button>
	{/if}
</div>

<style>
	.sc-field {
		display: flex;
		align-items: center;
		max-width: 24rem;
		border: 1px solid var(--clr-border-light);
		border-radius: 8px;
		background: var(--bg-card);
		transition: border-color var(--dur-micro) var(--ease-out), box-shadow var(--dur-micro) var(--ease-out);
	}
	.sc-recording {
		border-color: var(--color-brand);
		box-shadow: 0 0 0 3px var(--color-brand-glow);
	}
	.sc-keys {
		display: flex;
		flex: 1;
		align-items: center;
		gap: 0.3rem;
		min-height: 2.25rem;
		padding: 0.3rem 0.6rem;
		text-align: left;
		cursor: pointer;
	}
	.sc-keys:focus-visible {
		outline: none;
	}
	.sc-field:has(.sc-keys:focus-visible) {
		border-color: var(--color-brand);
	}
	kbd {
		padding: 0.1rem 0.4rem;
		border-radius: 4px;
		background: var(--bg-surface);
		border: 1px solid var(--clr-border);
		font-family: var(--font-outlier);
		font-size: var(--text-xs);
		color: var(--text-primary);
	}
	.sc-hint {
		font-size: var(--text-sm);
		color: var(--text-muted);
	}
	.sc-clear {
		display: grid;
		place-items: center;
		width: 2rem;
		height: 2rem;
		margin-right: 0.15rem;
		border-radius: 6px;
		color: var(--text-muted);
	}
	.sc-clear:hover {
		background: var(--bg-hover);
		color: var(--text-primary);
	}
	.sc-clear svg {
		width: 0.95rem;
		height: 0.95rem;
	}
</style>
