<script lang="ts">
	import { onMount } from 'svelte';
	import { getVersion } from '@tauri-apps/api/app';
	import { SYNC_LABELS, ago, hostOf } from './syncText';
	import type { AppStatus } from './tauriClient';

	/**
	 * The foot of the Tasks pane: a dot for the sync state (green synced, yellow
	 * changes waiting or the server out of reach, red an error), the server's
	 * host, and the app version. Click to sync now.
	 */
	let { status, onsync }: { status: AppStatus; onsync: () => void } = $props();

	let version = $state('');
	let now = $state(Date.now());

	onMount(() => {
		getVersion().then((v) => (version = v)).catch(() => {});
		const clock = setInterval(() => (now = Date.now()), 30_000);
		return () => clearInterval(clock);
	});

	let sync = $derived(status.sync);
	/** ok · busy · wait · bad: the dot's colour. */
	let tone = $derived(
		sync.state === 'error' ? 'bad'
			: sync.state === 'syncing' ? 'busy'
			: sync.state === 'synced' && !sync.pending ? 'ok'
			: 'wait',
	);
	let label = $derived(
		sync.state === 'synced' && sync.pending ? `${sync.pending} pending`
			: sync.state === 'synced' ? 'Synced'
			: SYNC_LABELS[sync.state],
	);
	let host = $derived(hostOf(status.server));
	let title = $derived(
		[
			`${SYNC_LABELS[sync.state]} · last synced ${ago(sync.last_synced_at, now)}`,
			sync.pending ? `${sync.pending} change${sync.pending === 1 ? '' : 's'} waiting to sync` : '',
			sync.message ?? '',
			status.server ? `Server: ${status.server.url}${status.server.tasks_base}` : '',
			'Click to sync now',
		]
			.filter(Boolean)
			.join('\n'),
	);
</script>

<button type="button" class="pane-status" data-tone={tone} {title} onclick={onsync}>
	<span class="pane-status-line">
		<span class="pane-status-dot" aria-hidden="true"></span>
		<span class="pane-status-label">{label}</span>
		{#if sync.state === 'synced' && sync.last_synced_at}<span class="pane-status-ago">· {ago(sync.last_synced_at, now)}</span>{/if}
	</span>
	<span class="pane-status-line">
		{#if host}
			<span class="pane-status-host">
				<svg aria-hidden="true" fill="none" stroke="currentColor" viewBox="0 0 24 24"><path stroke-linecap="round" stroke-linejoin="round" stroke-width="1.75" d="M5 12h14M5 12a2 2 0 01-2-2V6a2 2 0 012-2h14a2 2 0 012 2v4a2 2 0 01-2 2M5 12a2 2 0 00-2 2v4a2 2 0 002 2h14a2 2 0 002-2v-4a2 2 0 00-2-2m-2-4h.01M17 16h.01" /></svg>
				<span class="pane-status-host-text">{host}</span>
			</span>
		{/if}
		{#if version}<span class="pane-status-version">v{version}</span>{/if}
	</span>
</button>

<style>
	.pane-status {
		display: grid;
		gap: 0.2rem;
		width: 100%;
		min-width: 0;
		padding: 0.4rem 0.5rem;
		border-radius: 6px;
		font-size: var(--text-xs);
		color: var(--text-muted);
		text-align: left;
		transition: background-color var(--dur-micro) var(--ease-out), color var(--dur-micro) var(--ease-out);
	}
	.pane-status:hover {
		background-color: var(--bg-hover);
		color: var(--text-secondary);
	}
	.pane-status-line {
		display: flex;
		align-items: center;
		gap: 0.4rem;
		min-width: 0;
	}
	.pane-status-ago {
		color: var(--text-faint);
		white-space: nowrap;
	}
	.pane-status-dot {
		flex-shrink: 0;
		width: 0.5rem;
		height: 0.5rem;
		border-radius: 999px;
		background: var(--color-success);
	}
	[data-tone='wait'] .pane-status-dot {
		background: var(--color-warning);
	}
	[data-tone='bad'] .pane-status-dot {
		background: var(--color-danger);
	}
	[data-tone='busy'] .pane-status-dot {
		background: var(--color-info);
		animation: pane-pulse 1s ease-in-out infinite;
	}
	@keyframes pane-pulse {
		50% {
			opacity: 0.35;
		}
	}
	.pane-status-label {
		flex-shrink: 0;
		font-weight: 500;
		color: var(--text-secondary);
	}
	.pane-status-host {
		display: inline-flex;
		align-items: center;
		gap: 0.4rem;
		flex: 1;
		min-width: 0;
		white-space: nowrap;
		color: var(--text-faint);
	}
	.pane-status-host-text {
		min-width: 0;
		overflow: hidden;
		text-overflow: ellipsis;
	}
	.pane-status-host svg {
		width: 0.5rem;
		height: 0.75rem;
		flex-shrink: 0;
	}
	.pane-status-version {
		flex-shrink: 0;
		margin-left: auto;
		font-family: var(--font-outlier);
		color: var(--text-faint);
	}
</style>
