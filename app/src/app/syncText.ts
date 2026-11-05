import type { ServerInfo, SyncStatus } from './tauriClient';

export const SYNC_LABELS: Record<SyncStatus['state'], string> = {
	setup: 'Not set up',
	syncing: 'Syncing…',
	synced: 'Synced',
	offline: 'Offline',
	server_down: 'Server down',
	error: 'Sync error',
	signed_out: 'Signed out',
};

/** States that need the user to notice them. */
export const syncTrouble = (s: SyncStatus) => s.state === 'offline' || s.state === 'server_down' || s.state === 'error';

/** "just now", "12 min ago", "3 h ago", or the date. */
export function ago(iso: string | null, now = Date.now()): string {
	if (!iso) return 'never';
	const s = Math.round((now - new Date(iso).getTime()) / 1000);
	if (s < 60) return 'just now';
	if (s < 3600) return `${Math.floor(s / 60)} min ago`;
	if (s < 86_400) return `${Math.floor(s / 3600)} h ago`;
	return new Date(iso).toLocaleDateString(undefined, { day: 'numeric', month: 'short' });
}

/** What the Sync row says on the right: "36 min ago", "Offline", "Syncing…". */
export function syncMeta(s: SyncStatus, now = Date.now()): string {
	if (s.state === 'synced') return ago(s.last_synced_at, now);
	return SYNC_LABELS[s.state];
}

/** The server's host, e.g. "tasks.example.com". */
export function hostOf(server: ServerInfo | null): string {
	if (!server) return '';
	try {
		return new URL(server.url).host;
	} catch {
		return server.url;
	}
}
