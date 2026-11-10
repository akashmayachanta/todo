/**
 * Rearranging tasks. It uses pointer events rather than HTML5 drag, so it works by
 * touch on Android too. Two ways in:
 * - `sortGrip`: a grip that drags at once (the mouse); arrow keys on it do the same.
 * - `sortHold`: touch and hold the task itself (a phone, which has no grip).
 *
 * The rows say what they are through data attributes (TaskRow, TaskDetail, the page):
 * - `data-sort-id`, `data-sort-kind` ('top' or 'sub'): a task that can be dropped around.
 * - `data-sort-scope`: the list a top-level task stays in (a board's column).
 * - `data-sort-board`: around a board's columns; a top-level task can go to another
 *   column in it, landing by the nearest card (the drop says which column, `scope`).
 * - `data-sort-zone`: an area that takes a task with no row under the pointer (an empty category).
 *
 * This only finds where a task lands; the page decides what that changes.
 */
import { tick } from 'svelte';

export type SortKind = 'top' | 'sub';

export interface SortDrop {
	id: string;
	kind: SortKind;
	/** The task it landed by, or under when `into`. */
	targetId: string | null;
	after: boolean;
	/** A subtask dropped on a task: it goes last under that task. */
	into: boolean;
	/** The zone it landed in, when no task was under the pointer. */
	zone: string | null;
	/** The `data-sort-scope` it landed in (a board's column). */
	scope: string | null;
}

export interface SortApi {
	ondrop: (drop: SortDrop) => void;
	onstep: (id: string, kind: SortKind, by: -1 | 1) => void;
}

type Mode = 'before' | 'after' | 'into' | 'zone';
type Hit = { el: HTMLElement; mode: Mode };

const DRAG_FROM = 4; // px moved before a press becomes a drag
const HOLD_MS = 450; // how long a touch is held before it picks the task up
const HOLD_SLOP = 8; // px a finger can wander while holding; more is a scroll
const SCROLL_EDGE = 48; // px from a scroller's edge where it starts to scroll
const SCROLL_STEP = 10; // px a frame

let pressed = false;

/** A task is held: a card's own HTML5 drag mustn't start under it. */
export function sorting(): boolean {
	return pressed;
}

function half(el: HTMLElement, y: number): Mode {
	// A task's item also holds its subtasks; the line goes by the whole of it.
	const r = el.getBoundingClientRect();
	return y > r.top + r.height / 2 ? 'after' : 'before';
}

/** In a board column, the card a drop lands by: before the first whose middle is below, else after the last. */
function nearestIn(column: HTMLElement, y: number, source: HTMLElement): Hit {
	const items = [...column.querySelectorAll<HTMLElement>('[data-sort-kind="top"]')].filter((i) => i !== source);
	for (const i of items) {
		const r = i.getBoundingClientRect();
		if (y < r.top + r.height / 2) return { el: i, mode: 'before' };
	}
	const last = items.at(-1);
	return last ? { el: last, mode: 'after' } : { el: column, mode: 'zone' };
}

/** Where a drag would land under the pointer, if anywhere. */
function findHit(x: number, y: number, source: HTMLElement, kind: SortKind): Hit | null {
	const under = document.elementFromPoint(x, y);
	if (!under) return null;
	let item = under.closest<HTMLElement>('[data-sort-id]');
	if (kind === 'sub') {
		if (!item || item === source) return null;
		return item.dataset.sortKind === 'sub' ? { el: item, mode: half(item, y) } : { el: item, mode: 'into' };
	}
	const scope = source.closest<HTMLElement>('[data-sort-scope]');
	const underScope = under.closest<HTMLElement>('[data-sort-scope]');
	if (!scope || !underScope) return null;
	const board = scope.closest('[data-sort-board]');
	if (board && underScope.closest('[data-sort-board]') === board) return nearestIn(underScope, y, source);
	if (underScope !== scope) return null;
	// Over a subtask, it's the task that holds it.
	while (item && item.dataset.sortKind !== 'top') item = item.parentElement?.closest<HTMLElement>('[data-sort-id]') ?? null;
	if (item) return item === source ? null : { el: item, mode: half(item, y) };
	const zone = under.closest<HTMLElement>('[data-sort-zone]');
	return zone ? { el: zone, mode: 'zone' } : null;
}

function mark(hit: Hit | null, on: boolean) {
	if (hit) hit.el.classList.toggle(`sort-drop-${hit.mode}`, on);
}

/** The nearest scrollers around it, up and down and sideways (a board scrolls sideways). */
function scrollersOf(el: HTMLElement): { y: HTMLElement | null; x: HTMLElement | null } {
	let y: HTMLElement | null = null;
	let x: HTMLElement | null = null;
	for (let at = el.parentElement; at && !(x && y); at = at.parentElement) {
		const s = getComputedStyle(at);
		if (!y && (s.overflowY === 'auto' || s.overflowY === 'scroll') && at.scrollHeight > at.clientHeight) y = at;
		if (!x && (s.overflowX === 'auto' || s.overflowX === 'scroll') && at.scrollWidth > at.clientWidth) x = at;
	}
	return { y, x };
}

function edgeStep(at: number, lo: number, hi: number): number {
	return at < lo + SCROLL_EDGE ? -SCROLL_STEP : at > hi - SCROLL_EDGE ? SCROLL_STEP : 0;
}

type GripOptions = SortApi & { id: string; kind: SortKind };

/** One drag of a task, from picking it up to letting go. */
class Drag {
	hit: Hit | null = null;
	last = { x: 0, y: 0 };
	frame = 0;
	scrollers: { y: HTMLElement | null; x: HTMLElement | null } = { y: null, x: null };

	constructor(
		readonly source: HTMLElement,
		readonly opts: () => GripOptions,
	) {
		source.classList.add('sort-dragging');
		document.documentElement.classList.add('sort-active');
		this.scrollers = scrollersOf(source);
		this.frame = requestAnimationFrame(this.autoScroll);
	}

	setHit(next: Hit | null) {
		if (next?.el === this.hit?.el && next?.mode === this.hit?.mode) return;
		mark(this.hit, false);
		mark(next, true);
		this.hit = next;
	}

	move(x: number, y: number) {
		this.last = { x, y };
		this.setHit(findHit(x, y, this.source, this.opts().kind));
	}

	autoScroll = () => {
		const { x: sx, y: sy } = this.scrollers;
		let moved = false;
		if (sy) {
			const r = sy.getBoundingClientRect();
			const by = edgeStep(this.last.y, r.top, r.bottom);
			if (by) { sy.scrollTop += by; moved = true; }
		}
		if (sx) {
			const r = sx.getBoundingClientRect();
			const by = edgeStep(this.last.x, r.left, r.right);
			if (by) { sx.scrollLeft += by; moved = true; }
		}
		if (moved) this.move(this.last.x, this.last.y);
		this.frame = requestAnimationFrame(this.autoScroll);
	};

	end(commit: boolean) {
		const landed = commit ? this.hit : null;
		cancelAnimationFrame(this.frame);
		this.setHit(null);
		this.source.classList.remove('sort-dragging');
		document.documentElement.classList.remove('sort-active');
		if (!landed) return;
		const opts = this.opts();
		opts.ondrop({
			id: opts.id,
			kind: opts.kind,
			targetId: landed.mode === 'zone' ? null : landed.el.dataset.sortId ?? null,
			after: landed.mode === 'after',
			into: landed.mode === 'into',
			zone: landed.mode === 'zone' ? landed.el.dataset.sortZone ?? null : null,
			scope: landed.el.closest<HTMLElement>('[data-sort-scope]')?.dataset.sortScope ?? null,
		});
	}
}

/** Makes an element the grip of the `[data-sort-id]` item around it. */
export function sortGrip(node: HTMLElement, initial: GripOptions) {
	let opts = initial;
	let source: HTMLElement | null = null;
	let drag: Drag | null = null;
	let start = { x: 0, y: 0 };

	function finish(commit: boolean) {
		node.removeEventListener('pointermove', onMove);
		node.removeEventListener('pointerup', onUp);
		node.removeEventListener('pointercancel', onCancel);
		window.removeEventListener('keydown', onEscape, true);
		drag?.end(commit);
		drag = null;
		pressed = false;
		source = null;
	}

	function onMove(e: PointerEvent) {
		if (!drag) {
			if (Math.hypot(e.clientX - start.x, e.clientY - start.y) < DRAG_FROM) return;
			drag = new Drag(source!, () => opts);
		}
		drag.move(e.clientX, e.clientY);
	}

	const onUp = () => finish(true);
	const onCancel = () => finish(false);

	function onEscape(e: KeyboardEvent) {
		if (e.key !== 'Escape') return;
		e.preventDefault();
		e.stopPropagation();
		finish(false);
	}

	function onDown(e: PointerEvent) {
		if (e.button !== 0) return;
		source = node.closest<HTMLElement>('[data-sort-id]');
		if (!source) return;
		pressed = true;
		start = { x: e.clientX, y: e.clientY };
		node.setPointerCapture(e.pointerId);
		node.addEventListener('pointermove', onMove);
		node.addEventListener('pointerup', onUp);
		node.addEventListener('pointercancel', onCancel);
		window.addEventListener('keydown', onEscape, true);
	}

	async function onKey(e: KeyboardEvent) {
		if (e.key !== 'ArrowUp' && e.key !== 'ArrowDown') return;
		e.preventDefault();
		e.stopPropagation();
		opts.onstep(opts.id, opts.kind, e.key === 'ArrowUp' ? -1 : 1);
		// Moving a row in the DOM can take the focus off it.
		await tick();
		node.focus();
	}

	node.addEventListener('pointerdown', onDown);
	node.addEventListener('keydown', onKey);
	return {
		update(next: GripOptions) {
			opts = next;
		},
		destroy() {
			if (source) finish(false);
			node.removeEventListener('pointerdown', onDown);
			node.removeEventListener('keydown', onKey);
		},
	};
}

/**
 * Touch and hold a `[data-sort-id]` item to pick it up, then drag it (touch only; the
 * mouse has the grip, or a board card's own drag). A finger that moves before the hold
 * is a scroll and leaves it alone. Null options turn it off.
 */
export function sortHold(node: HTMLElement, initial: GripOptions | null) {
	let opts = initial;
	let timer = 0;
	let pointer: number | null = null;
	let start = { x: 0, y: 0 };
	let at = { x: 0, y: 0 };
	let drag: Drag | null = null;

	function reset() {
		clearTimeout(timer);
		window.removeEventListener('pointermove', onMove, true);
		window.removeEventListener('pointerup', onUp, true);
		window.removeEventListener('pointercancel', onCancel, true);
		pointer = null;
	}

	function pickUp() {
		if (!opts) return reset();
		pressed = true;
		drag = new Drag(node, () => opts!);
		drag.move(at.x, at.y);
		navigator.vibrate?.(15);
	}

	function finish(commit: boolean) {
		const was = drag;
		reset();
		drag = null;
		pressed = false;
		if (!was) return;
		was.end(commit);
		// The finger lifting off where it held mustn't also open the task.
		window.addEventListener('click', swallow, { capture: true, once: true });
		setTimeout(() => window.removeEventListener('click', swallow, true), 400);
	}

	function onMove(e: PointerEvent) {
		if (e.pointerId !== pointer) return;
		at = { x: e.clientX, y: e.clientY };
		if (drag) drag.move(e.clientX, e.clientY);
		else if (Math.hypot(e.clientX - start.x, e.clientY - start.y) > HOLD_SLOP) reset();
	}

	const onUp = (e: PointerEvent) => e.pointerId === pointer && finish(true);
	const onCancel = (e: PointerEvent) => e.pointerId === pointer && finish(false);

	function onDown(e: PointerEvent) {
		if (!opts || e.pointerType !== 'touch' || pointer !== null) return;
		// A subtask listed under it holds itself.
		if ((e.target as Element | null)?.closest('[data-sort-id]') !== node) return;
		if ((e.target as Element | null)?.closest('input, textarea, [contenteditable="true"]')) return;
		pointer = e.pointerId;
		start = at = { x: e.clientX, y: e.clientY };
		timer = window.setTimeout(pickUp, HOLD_MS);
		window.addEventListener('pointermove', onMove, true);
		window.addEventListener('pointerup', onUp, true);
		window.addEventListener('pointercancel', onCancel, true);
	}

	// Once it's picked up, the finger drags it instead of scrolling the page.
	function onTouchMove(e: TouchEvent) {
		if (drag) e.preventDefault();
	}

	// Android's long-press menu (and text selection) would take the hold.
	function onContextMenu(e: Event) {
		if (pointer !== null || drag) e.preventDefault();
	}

	node.addEventListener('pointerdown', onDown);
	node.addEventListener('touchmove', onTouchMove, { passive: false });
	node.addEventListener('contextmenu', onContextMenu);
	return {
		update(next: GripOptions | null) {
			opts = next;
		},
		destroy() {
			if (pointer !== null) finish(false);
			node.removeEventListener('pointerdown', onDown);
			node.removeEventListener('touchmove', onTouchMove);
			node.removeEventListener('contextmenu', onContextMenu);
		},
	};
}

function swallow(e: Event) {
	e.preventDefault();
	e.stopPropagation();
}
