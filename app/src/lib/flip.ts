/**
 * Glides things to their new place when a list is rearranged (FLIP: first, last,
 * invert, play). Mark each moving element with `data-flip="<unique key>"`, take a
 * `snapshot()` before the change, and call what it returns once the DOM has
 * updated (after `tick()`). Matching is by key, not by element, so a card that
 * moves to another board column (a new element there) still glides from its old
 * spot. A nested element moves with its parent and only animates its own share.
 * Off when the system asks for reduced motion.
 */
import { tick } from 'svelte';

const DURATION = 240;
const EASING = 'cubic-bezier(0.16, 1, 0.3, 1)'; // --ease-out

function reduced(): boolean {
	return typeof matchMedia === 'function' && matchMedia('(prefers-reduced-motion: reduce)').matches;
}

/** A transition's duration in ms, or 0 when the system asks for reduced motion. */
export function motion(ms: number): number {
	return reduced() ? 0 : ms;
}

/**
 * Where each `[data-flip]` is now; the function it returns plays the moves since.
 * `from` says where some keys start instead (a dragged item: where it was let go).
 */
export function snapshot(root: ParentNode = document, from?: Map<string, DOMRect>): () => void {
	if (typeof document === 'undefined' || reduced()) return () => {};
	// A key shown twice (a task in the list and in its open details) can't say which went where.
	const before = new Map<string, DOMRect | null>();
	for (const el of root.querySelectorAll<HTMLElement>('[data-flip]')) {
		const key = el.dataset.flip!;
		before.set(key, before.has(key) ? null : el.getBoundingClientRect());
	}
	for (const [key, rect] of from ?? []) if (before.get(key)) before.set(key, rect);
	return () => {
		const now = [...root.querySelectorAll<HTMLElement>('[data-flip]')];
		const count = new Map<string, number>();
		for (const el of now) count.set(el.dataset.flip!, (count.get(el.dataset.flip!) ?? 0) + 1);
		// Each element's whole move, so a child can take off its parent's share.
		const moved = new Map<Element, { dx: number; dy: number }>();
		for (const el of now) {
			const was = before.get(el.dataset.flip!);
			if (!was || count.get(el.dataset.flip!)! > 1) continue;
			const r = el.getBoundingClientRect();
			const dx = was.left - r.left;
			const dy = was.top - r.top;
			moved.set(el, { dx, dy });
			const parent = el.parentElement?.closest('[data-flip]');
			const own = parent ? moved.get(parent) : undefined;
			const x = dx - (own?.dx ?? 0);
			const y = dy - (own?.dy ?? 0);
			if (Math.abs(x) < 1 && Math.abs(y) < 1) continue;
			el.animate([{ transform: `translate(${x}px, ${y}px)` }, { transform: 'none' }], {
				duration: DURATION,
				easing: EASING,
			});
		}
	};
}

/** Runs `change` (a state update) and glides what it moved. Returns what `change` returns. */
export function withFlip<T>(change: () => T): T {
	const play = snapshot();
	const out = change();
	tick().then(play);
	return out;
}
