/**
 * The Tasks pane hidden (the sidebar button), remembered in this browser. Wide
 * windows only: on phones the pane is the Browse tab.
 */
import { writable } from 'svelte/store';
import { browser } from '$app/environment';

const KEY = 'tasks-pane-collapsed';

function saved(): boolean {
	try {
		return browser && localStorage.getItem(KEY) === '1';
	} catch {
		return false;
	}
}

export const paneCollapsed = writable<boolean>(saved());

paneCollapsed.subscribe((v) => {
	try {
		if (browser) localStorage.setItem(KEY, v ? '1' : '0');
	} catch {
		/* storage blocked: it is forgotten on reload */
	}
});

export function togglePane() {
	paneCollapsed.update((v) => !v);
}
