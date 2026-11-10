// The main window's title bar takes the sidebar's colour (--bg-surface) and the page's
// ink, so the top of the window and the sidebar read as one surface, as in Todoist.
// Windows 11 only; Rust leaves Windows 10's title bar alone (app_set_title_bar).

import { get } from 'svelte/store';
import { modeStore, themeStore } from '$lib/stores/theme';
import { call } from './tauriClient';
import { isDesktop } from './widgets';

/** A CSS colour (oklch and all) as RGB, by painting one pixel. */
function rgb(css: string): [number, number, number] | null {
	const c = document.createElement('canvas').getContext('2d', { willReadFrequently: true });
	if (!c || !css) return null;
	c.fillStyle = css;
	c.fillRect(0, 0, 1, 1);
	const [r, g, b] = c.getImageData(0, 0, 1, 1).data;
	return [r, g, b];
}

function paint() {
	const style = getComputedStyle(document.documentElement);
	const caption = rgb(style.getPropertyValue('--bg-surface').trim());
	const text = rgb(style.getPropertyValue('--text-primary').trim());
	if (!caption || !text) return;
	call('app_set_title_bar', { caption, text, dark: get(modeStore) === 'dark' }).catch((e) =>
		console.error('Could not colour the title bar', e),
	);
}

/** Keeps the title bar in the theme's colours; returns the function that stops. */
export async function followTitleBar(): Promise<() => void> {
	if (!isDesktop) return () => {};
	// After the theme's attribute lands on <html>, so the new colours are computed.
	const later = () => requestAnimationFrame(paint);
	const stop = themeStore.subscribe(later);
	// Windows redoes its title bar when the system's light/dark switch flips.
	const mq = window.matchMedia?.('(prefers-color-scheme: dark)');
	mq?.addEventListener('change', later);
	return () => {
		stop();
		mq?.removeEventListener('change', later);
	};
}
