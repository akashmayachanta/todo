<script lang="ts">
	import { onMount } from 'svelte';
	import { getVersion } from '@tauri-apps/api/app';
	import { SYNC_LABELS, ago, hostOf } from './syncText';
	import type { AppStatus } from './tauriClient';

	/**
	 * The foot of the Tasks pane: a dot for the sync state (green synced, yellow
	 * changes waiting or the server out of reach, red an error), the server's
	 * host, and the app version. Click to sync now; the icon beside it forces a sync,
	 * which starts at once, pulls everything from the server again and reloads the tasks.
	 */
	let { status, onsync, onforcesync }: { status: AppStatus; onsync: () => void; onforcesync: () => void } = $props();

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
	let canSync = $derived(sync.state !== 'setup' && sync.state !== 'signed_out');
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

<div class="pane-status-wrap">
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
{#if canSync}
	<button
		type="button"
		class="pane-status-force"
		class:pane-status-force-busy={sync.state === 'syncing'}
		title="Force sync: get everything from the server again and refresh the tasks"
		aria-label="Force sync"
		onclick={onforcesync}
	>
		<svg aria-hidden="true" fill="none" stroke="currentColor" viewBox="0 0 24 24"><path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M4 4v5h.582m15.356 2A8.001 8.001 0 004.582 9m0 0H9m11 11v-5h-.581m0 0a8.003 8.003 0 01-15.357-2m15.357 2H15" /></svg>
	</button>
{/if}
</div>

<style>
	.pane-status-wrap {
		position: relative;
		min-width: 0;
	}
	.pane-status-force {
		position: absolute;
		top: 0;
		right: 0.125rem;
		display: inline-flex;
		align-items: center;
		justify-content: center;
		width: 1.5rem;
		height: 1.5rem;
		border-radius: 6px;
		color: var(--text-muted);
		transition: background-color var(--dur-micro) var(--ease-out), color var(--dur-micro) var(--ease-out);
	}
	.pane-status-force:hover {
		background-color: var(--bg-hover);
		color: var(--text-primary);
	}
	.pane-status-force svg {
		width: 0.875rem;
		height: 0.875rem;
	}
	.pane-status-force-busy svg {
		animation: pane-spin 1s linear infinite;
	}
	@keyframes pane-spin {
		to {
			transform: rotate(360deg);
		}
	}
	@media (prefers-reduced-motion: reduce) {
		.pane-status-force-busy svg {
			animation: none;
		}
	}
	/* Its first line stops short of the icon, which sits over its right end. */
	.pane-status-line:first-child {
		padding-right: 1.5rem;
	}
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
		corner-shape: round;
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
