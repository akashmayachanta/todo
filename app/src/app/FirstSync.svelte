<script lang="ts">
	import type { SyncStatus } from './tauriClient';

	/**
	 * Right after signing in on a device, before its first full sync: the
	 * account's tasks aren't here yet, so this waits for them (it needs the
	 * network once). After that the app works offline. The sync loop keeps
	 * retrying on its own; Retry just doesn't wait for it.
	 */
	let {
		sync,
		username,
		onretry,
		onsignout,
	}: { sync: SyncStatus; username: string | null; onretry: () => void; onsignout: () => void } = $props();

	const HEADINGS: Partial<Record<SyncStatus['state'], string>> = {
		offline: "You're offline",
		server_down: "The server isn't answering",
		error: "Couldn't get your tasks",
	};

	let heading = $derived(HEADINGS[sync.state] ?? 'Getting your tasks…');
	let waiting = $derived(sync.state === 'syncing' || sync.state === 'synced');
	let detail = $derived(
		sync.state === 'offline'
			? 'Connect to the internet once so your tasks can download to this device. After that, Tasks works offline.'
			: sync.state === 'server_down' || sync.state === 'error'
				? (sync.message ?? '') + ' It keeps trying on its own.'
				: `Downloading ${username ? `${username}'s` : 'your'} tasks to this device. After this, Tasks works offline.`,
	);
</script>

<div class="first-sync" aria-live="polite">
	<div class="first-sync-sheet">
		<h1 class="first-sync-title">{heading}</h1>
		<p class="first-sync-detail">{detail}</p>
		<div class="first-sync-actions">
			<button type="button" class="btn btn-secondary" onclick={onsignout}>Sign out</button>
			<button type="button" class="btn btn-primary" onclick={onretry} disabled={waiting}>
				{waiting ? 'Syncing…' : 'Try again'}
			</button>
		</div>
	</div>
</div>

<style>
	.first-sync {
		min-height: 100%;
		display: grid;
		place-items: center;
		padding: 1.5rem 1rem;
	}
	.first-sync-sheet {
		width: min(28rem, 100%);
		display: grid;
		gap: 0.9rem;
	}
	.first-sync-title {
		font-family: var(--font-display);
		font-size: 1.4rem;
		margin: 0;
		color: var(--text-primary);
	}
	.first-sync-detail {
		margin: 0;
		font-family: var(--font-prose);
		color: var(--text-secondary);
	}
	.first-sync-actions {
		display: flex;
		gap: 0.5rem;
		justify-content: flex-end;
	}
</style>
