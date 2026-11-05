import { derived, get, writable } from 'svelte/store';
import { browser } from '$app/environment';

export type Mode = 'dark' | 'light';
export type Accent = 'mono' | 'magenta' | 'green';
/** Light, dark, or following the system (no saved mode, as app.html reads it). */
export type ModePref = Mode | 'system';

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

function read(key: string): string | null {
	try {
		return browser ? localStorage.getItem(key) : null;
	} catch {
		return null;
	}
}

function write(key: string, value: string | null) {
	try {
		if (value === null) localStorage.removeItem(key);
		else localStorage.setItem(key, value);
	} catch {
		/* not remembered after a reload */
	}
}

const systemDark = () => browser && !!window.matchMedia?.('(prefers-color-scheme: dark)').matches;

function savedPref(): ModePref {
	const m = read('theme-mode');
	return m === 'dark' || m === 'light' ? m : 'system';
}

/** What was picked in Settings: light, dark or system. */
export const modePref = writable<ModePref>(savedPref());
/** The mode in effect (the system's when the pick is 'system'). */
export const modeStore = writable<Mode>(get(modePref) === 'system' ? (systemDark() ? 'dark' : 'light') : (get(modePref) as Mode));

const savedAccent = read('theme-accent');
export const accentStore = writable<Accent>(ACCENTS.some((a) => a.value === savedAccent) ? (savedAccent as Accent) : 'mono');

/** Combined `data-theme` value, e.g. "dark-magenta" — see layout.css */
export const themeStore = derived([modeStore, accentStore], ([$mode, $accent]) => `${$mode}-${$accent}`);

/** Puts the mode and accent in effect on <html>. */
export function applyTheme() {
	if (!browser) return;
	const pref = get(modePref);
	modeStore.set(pref === 'system' ? (systemDark() ? 'dark' : 'light') : pref);
	document.documentElement.dataset.theme = get(themeStore);
}

export function setModePref(pref: ModePref) {
	write('theme-mode', pref === 'system' ? null : pref);
	modePref.set(pref);
	applyTheme();
}

export function setAccent(accent: Accent) {
	write('theme-accent', accent);
	accentStore.set(accent);
	applyTheme();
}

/** Follows the system's light / dark switch while the pick is 'system'; returns the function that stops. */
export function followSystemTheme(): () => void {
	if (!browser || !window.matchMedia) return () => {};
	const mq = window.matchMedia('(prefers-color-scheme: dark)');
	const onChange = () => get(modePref) === 'system' && applyTheme();
	mq.addEventListener('change', onChange);
	return () => mq.removeEventListener('change', onChange);
}
