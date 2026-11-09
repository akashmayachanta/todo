// App updates from the Tasks server (src-tauri/src/updates.rs). Each time the app
// opens or comes back to the front, it asks the server for a newer build and, if
// there is one, asks the user. "Later" only lasts until the next time.
import { errorDetail } from '$lib/api';
import { confirmDialog } from '$lib/stores/confirm';
import { toast } from '$lib/stores/toast';
import { call } from './tauriClient';
import { isDesktop } from './widgets';

interface UpdateInfo {
	current: string;
	version: string;
	size: number;
}

// One question or download at a time.
let busy = false;

function megabytes(bytes: number): string {
	return `${(bytes / 1024 / 1024).toFixed(1)} MB`;
}

export async function checkForUpdate(): Promise<void> {
	// Android has no updater (see updates.rs).
	if (busy || !isDesktop) return;
	busy = true;
	let downloading: number | null = null;
	try {
		const info = await call<UpdateInfo | null>('app_update_check').catch(() => null);
		if (!info) return;
		const then = isDesktop
			? 'Tasks will restart into it.'
			: "Android's installer opens next; tap Install there.";
		const yes = await confirmDialog(
			`Version ${info.version} is on the server (this is ${info.current}). Download it (${megabytes(info.size)})? ${then}`,
			{ title: 'Update available', confirmLabel: 'Update', cancelLabel: 'Later', danger: false },
		);
		if (!yes) return;
		downloading = toast.info('Downloading the update…', 15 * 60_000);
		await call('app_update_install');
	} catch (e) {
		toast.error(`Couldn't update: ${errorDetail(e)}`, 8000);
	} finally {
		if (downloading !== null) toast.dismiss(downloading);
		busy = false;
	}
}
