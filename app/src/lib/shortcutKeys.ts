/**
 * Keyboard shortcuts as Settings → Desktop saves them and the app's Rust side
 * registers them (tauri-plugin-global-shortcut): modifiers then one key, joined
 * by "+", e.g. "Alt+N", "Control+Space", "Super+Alt+S".
 */

const MODIFIERS = ['Control', 'Super', 'Alt', 'Shift'] as const;

/** The key part of an event, or null for a key a shortcut can't end with. */
function keyName(code: string): string | null {
	if (/^Key[A-Z]$/.test(code)) return code.slice(3);
	if (/^Digit[0-9]$/.test(code)) return code.slice(5);
	if (/^F([1-9]|1[0-9]|2[0-4])$/.test(code)) return code;
	if (['Space', 'Enter', 'Tab', 'Backspace', 'Delete', 'Insert', 'Home', 'End', 'PageUp', 'PageDown',
		'ArrowUp', 'ArrowDown', 'ArrowLeft', 'ArrowRight', 'Comma', 'Period', 'Slash', 'Semicolon',
		'Quote', 'BracketLeft', 'BracketRight', 'Backslash', 'Minus', 'Equal', 'Backquote'].includes(code)) return code;
	return null;
}

/**
 * The shortcut a key press makes, or null while only modifiers are down or the key
 * can't be used. A shortcut needs a modifier, except a function key alone.
 */
export function keysFromEvent(e: KeyboardEvent): string | null {
	const key = keyName(e.code);
	if (!key) return null;
	const mods = MODIFIERS.filter((m) => (m === 'Control' ? e.ctrlKey : m === 'Super' ? e.metaKey : m === 'Alt' ? e.altKey : e.shiftKey));
	if (!mods.length && !/^F\d+$/.test(key)) return null;
	return [...mods, key].join('+');
}

/** Whether a key press is `keys`. Matched on the key's position, so macOS Option+N works too. */
export function matchesKeys(e: KeyboardEvent, keys: string | null): boolean {
	if (!keys) return false;
	const parts = keys.split('+');
	const key = parts.pop()!;
	const has = (m: string) => parts.includes(m);
	return (
		e.ctrlKey === has('Control') && e.metaKey === has('Super') && e.altKey === has('Alt') && e.shiftKey === has('Shift') &&
		keyName(e.code) === key
	);
}

/** The keys to show, as Windows names them: ["Win", "Alt", "S"]. */
export function keyLabels(keys: string): string[] {
	const names: Record<string, string> = { Control: 'Ctrl', Super: 'Win', ArrowUp: '↑', ArrowDown: '↓', ArrowLeft: '←', ArrowRight: '→' };
	return keys.split('+').map((k) => names[k] ?? k);
}
