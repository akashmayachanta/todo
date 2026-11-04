import { writable } from 'svelte/store';

export type ToastVariant = 'success' | 'error' | 'info';

export interface ToastItem {
	id: number;
	message: string;
	variant: ToastVariant;
}

const DEFAULT_DURATION = 2000;
// Errors stay a bit longer so they can be read; the cross lets users dismiss any toast early.
const ERROR_DURATION = 4000;

function createToastStore() {
	const { subscribe, update } = writable<ToastItem[]>([]);
	let counter = 0;

	function dismiss(id: number) {
		update((items) => items.filter((t) => t.id !== id));
	}

	function push(
		message: string,
		variant: ToastVariant = 'info',
		duration = variant === 'error' ? ERROR_DURATION : DEFAULT_DURATION,
	) {
		const id = ++counter;
		update((items) => [...items, { id, message, variant }]);
		setTimeout(() => dismiss(id), duration);
		return id;
	}

	/** Back-compat with the old "✓ "/"✗ " message-prefix convention — strips the
	 *  prefix and routes to the matching variant so call sites don't need to change. */
	function notify(message: string, duration?: number) {
		if (message.startsWith('✓ ')) return push(message.slice(2), 'success', duration);
		if (message.startsWith('✗ ')) return push(message.slice(2), 'error', duration);
		return push(message, 'info', duration);
	}

	return {
		subscribe,
		dismiss,
		notify,
		success: (message: string, duration?: number) => push(message, 'success', duration),
		error: (message: string, duration?: number) => push(message, 'error', duration),
		info: (message: string, duration?: number) => push(message, 'info', duration),
	};
}

export const toast = createToastStore();
