import { writable } from 'svelte/store';

export interface ConfirmOptions {
	title?: string;
	confirmLabel?: string;
	cancelLabel?: string;
	danger?: boolean;
}

interface ConfirmState extends ConfirmOptions {
	open: boolean;
	message: string;
	resolve: ((value: boolean) => void) | null;
}

const DEFAULT_STATE: ConfirmState = {
	open: false,
	title: 'Confirm',
	message: '',
	confirmLabel: 'Delete',
	cancelLabel: 'Cancel',
	danger: true,
	resolve: null,
};

function createConfirmStore() {
	const { subscribe, set } = writable<ConfirmState>({ ...DEFAULT_STATE });

	function ask(message: string, options: ConfirmOptions = {}): Promise<boolean> {
		return new Promise((resolve) => {
			set({
				...DEFAULT_STATE,
				...options,
				message,
				open: true,
				resolve: (value: boolean) => {
					set({ ...DEFAULT_STATE });
					resolve(value);
				},
			});
		});
	}

	return { subscribe, ask };
}

export const confirmStore = createConfirmStore();

/** Replaces window.confirm() with a themed modal. Resolves true/false instead of blocking the thread. */
export function confirmDialog(message: string, options?: ConfirmOptions): Promise<boolean> {
	return confirmStore.ask(message, options);
}
