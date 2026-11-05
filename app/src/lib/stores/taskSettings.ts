/**
 * The Tasks display settings (Settings → General), shared by the Tasks page and
 * the Settings dialog, remembered in this browser.
 */
import { writable } from 'svelte/store';
import { browser } from '$app/environment';
import { DEFAULT_TASK_SETTINGS, loadTaskSettings, saveTaskSettings, type TaskSettings } from '$lib/components/tasks/shared';

const store = writable<TaskSettings>(browser ? loadTaskSettings() : { ...DEFAULT_TASK_SETTINGS });

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
