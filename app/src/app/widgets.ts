// The desktop's extra windows (src-tauri/src/widgets.rs): none of them exist on Android.

import type { PinTarget } from '$lib/components/tasks/pins';
import { isTauri } from './platform';
import { call } from './tauriClient';

/** The Windows app; not Android, and not a browser (which has no pins, badge, title bar or updates). */
export const isDesktop = isTauri && typeof navigator !== 'undefined' && !/android/i.test(navigator.userAgent);

export function pinToDesktop(target: PinTarget): Promise<void> {
	return call('app_pin_open', { kind: target.kind, id: target.id });
}

/** Closes the pin this page is in, which unpins it. */
export function closePin(): Promise<void> {
	return call('app_pin_close');
}

/** Opens a task in the main window. */
export function showTask(id: string): Promise<void> {
	return call('app_show_task', { id });
}

/** Brings up the main window at a Tasks page view, given as its query string ("project=…"). */
export function showView(query: string): Promise<void> {
	return call('app_show_view', { query });
}

/** Hides the quick add popup this page is in. */
export function hideQuickAdd(): Promise<void> {
	return call('app_quick_hide');
}
