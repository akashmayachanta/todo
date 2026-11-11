/**
 * The Tasks display settings (Settings → General), shared by the Tasks page and
 * the Settings dialog, remembered in this browser. The date & time ones are also
 * put in $lib/datePrefs for the code that shows and reads dates, and the time
 * format is handed to the app's Rust side for the reminder toasts.
 */
import { writable } from 'svelte/store';
import { browser } from '$app/environment';
import { invoke } from '@tauri-apps/api/core';
import { DEFAULT_TASK_SETTINGS, loadTaskSettings, saveTaskSettings, SETTINGS_KEY, type TaskSettings } from '$lib/components/tasks/shared';
import { datePrefs } from '$lib/datePrefs.svelte';

const store = writable<TaskSettings>(browser ? loadTaskSettings() : { ...DEFAULT_TASK_SETTINGS });

let clockSent: string | null = null;
store.subscribe((s) => {
	datePrefs.timeFormat = s.timeFormat;
	datePrefs.dateFormat = s.dateFormat;
	datePrefs.weekStart = s.weekStart;
	datePrefs.nextWeek = s.nextWeek;
	datePrefs.weekend = s.weekend;
	datePrefs.smartDates = s.smartDates;
	if (browser && clockSent !== s.timeFormat) {
		clockSent = s.timeFormat;
		invoke('app_set_clock', { hour12: s.timeFormat === '12h' }).catch(() => {});
	}
});

// The desktop's small windows stay loaded; a change made in the main window reaches them here.
if (browser) {
	window.addEventListener('storage', (e) => {
		if (e.key === SETTINGS_KEY) store.set(loadTaskSettings());
	});
}

export const taskSettings = {
	subscribe: store.subscribe,
	set(next: TaskSettings) {
		saveTaskSettings(next);
		store.set(next);
	},
	update<K extends keyof TaskSettings>(key: K, value: TaskSettings[K]) {
		store.update((s) => {
			const next = { ...s, [key]: value };
			saveTaskSettings(next);
			return next;
		});
	},
};
