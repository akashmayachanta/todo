<script lang="ts">
	import type { SyncStatus } from './tauriClient';

	/** The sync state in the app bar: synced, syncing, offline (this device), server down, error or
	 *  signed out. Click to sync now. */
	let { sync, onsync, onaccount }: { sync: SyncStatus; onsync: () => void; onaccount: () => void } = $props();

	const LABELS: Record<SyncStatus['state'], string> = {
		setup: 'Not set up',
		syncing: 'Syncing…',
		synced: 'Synced',
		offline: 'Offline',
		server_down: 'Server down',
		error: 'Sync error',
		signed_out: 'Signed out',
	};

	function ago(iso: string | null): string {
		if (!iso) return 'never';
		const s = Math.round((Date.now() - new Date(iso).getTime()) / 1000);
		if (s < 60) return 'just now';
		if (s < 3600) return `${Math.floor(s / 60)} min ago`;
		return new Date(iso).toLocaleString();
	}

	let title = $derived(
		[
			sync.message,
			`Last synced: ${ago(sync.last_synced_at)}`,
			sync.pending ? `${sync.pending} change${sync.pending === 1 ? '' : 's'} waiting to sync` : '',
		]
			.filter(Boolean)
			.join('\n'),
	);
	let needsAccount = $derived(sync.state === 'signed_out' || sync.state === 'setup');
</script>

<button
	type="button"
	class="sync-chip"
	data-state={sync.state}
	{title}
	onclick={() => (needsAccount ? onaccount() : onsync())}
>
	<span class="sync-chip-dot" aria-hidden="true"></span>
	{LABELS[sync.state]}
	{#if sync.pending && sync.state !== 'synced'}<span class="sync-chip-count">{sync.pending}</span>{/if}
</button>

<style>
	.sync-chip {
		display: inline-flex;
		align-items: center;
		gap: 0.4rem;
		padding: 0.15rem 0.6rem;
		border: 1px solid var(--clr-border);
		border-radius: 999px;
		font: inherit;
		font-size: 0.8rem;
		background: transparent;
		color: var(--text-secondary);
		cursor: pointer;
		white-space: nowrap;
	}
	.sync-chip-dot {
		width: 0.5rem;
		height: 0.5rem;
		border-radius: 999px;
		background: var(--text-muted);
	}
	.sync-chip[data-state='synced'] .sync-chip-dot {
		background: var(--color-success);
	}
	.sync-chip[data-state='syncing'] .sync-chip-dot {
		background: var(--color-info);
	}
	.sync-chip[data-state='offline'] .sync-chip-dot,
	.sync-chip[data-state='server_down'] .sync-chip-dot,
	.sync-chip[data-state='signed_out'] .sync-chip-dot {
		background: var(--color-warning);
	}
	.sync-chip[data-state='error'] .sync-chip-dot {
		background: var(--color-danger);
	}
	.sync-chip-count {
		font-variant-numeric: tabular-nums;
		opacity: 0.75;
	}
</style>
