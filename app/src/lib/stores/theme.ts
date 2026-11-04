import { derived, writable } from 'svelte/store';
import { browser } from '$app/environment';

export type Mode = 'dark' | 'light';
export type Accent = 'mono' | 'magenta' | 'green';

export const MODES: { value: Mode; label: string }[] = [
	{ value: 'dark', label: 'Dark' },
	{ value: 'light', label: 'Light' },
];

export const ACCENTS: { value: Accent; label: string }[] = [
	{ value: 'mono', label: 'Ink' },
	{ value: 'magenta', label: 'Magenta' },
	{ value: 'green', label: 'Sea green' },
];

/** Preview swatch color per accent, tuned per mode (light on dark, deep on light) — mirrors --color-brand in layout.css */
export const ACCENT_SWATCH: Record<Mode, Record<Accent, string>> = {
	dark: { mono: 'oklch(92% 0.008 245)', magenta: 'oklch(72% 0.150 350)', green: 'oklch(76% 0.110 170)' },
	light: { mono: 'oklch(22% 0.020 245)', magenta: 'oklch(45% 0.180 350)', green: 'oklch(46% 0.100 170)' },
};

function persisted<T extends string>(key: string, fallback: T) {
	const initial = browser ? ((localStorage.getItem(key) as T | null) ?? fallback) : fallback;
	const { subscribe, set } = writable<T>(initial);
	return {
		subscribe,
		set(value: T) {
			if (browser) localStorage.setItem(key, value);
			set(value);
		},
	};
}

export const modeStore = persisted<Mode>('theme-mode', 'light');
export const accentStore = persisted<Accent>('theme-accent', 'mono');

// A stored accent that no longer exists (e.g. the old 'yellow' or 'blue') falls back to the default
if (browser && !ACCENTS.some((a) => a.value === localStorage.getItem('theme-accent'))) accentStore.set('mono');

/** Combined `data-theme` value, e.g. "dark-magenta" — see layout.css */
export const themeStore = derived([modeStore, accentStore], ([$mode, $accent]) => `${$mode}-${$accent}`);
