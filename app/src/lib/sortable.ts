/**
 * Rearranging tasks and categories. It uses pointer events rather than HTML5 drag, so
 * it works by touch on Android too. A floating copy of the picked-up item follows the
 * pointer while the item waits faded in its place, a line shows where it will land,
 * and on letting go everything glides to its new place (flip.ts). Three ways in:
 * - `sortGrip`: a task's grip, which drags at once; arrow keys on it do the same.
 * - `sortHold`: touch and hold the task itself (a phone, which has no grip).
 * - `sortCategory`: a category's head. The mouse drags it at once, a finger holds it first.
 *
 * The rows say what they are through data attributes (TaskRow, TaskDetail, the page):
 * - `data-sort-id`, `data-sort-kind` ('top' or 'sub'): a task that can be dropped around.
 * - `data-sort-scope`: the list a top-level task stays in (a board's column).
 * - `data-sort-board`: around a board's columns; a top-level task can go to another
 *   column in it, landing by the nearest card (the drop says which column, `scope`).
 * - `data-sort-zone`: an area that takes a task with no row under the pointer (an empty category).
 * - `data-sort-lift` on a task's wrapper (a board card): what is picked up when the task is.
 * - `data-cat-id` in a `data-cat-list`: a category that can be dropped around. A list
 *   laid out in a row (the board) goes by left and right, else by above and below.
 *
 * This only finds where something lands; the page decides what that changes.
 */
import { tick } from 'svelte';
import { snapshot } from './flip';

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
type Point = { x: number; y: number };

const DRAG_FROM = 4; // px moved before a press becomes a drag
const HOLD_MS = 450; // how long a touch is held before it picks the item up
const HOLD_SLOP = 8; // px a finger can wander while holding; more is a scroll
const SCROLL_EDGE = 48; // px from a scroller's edge where it starts to scroll
const SCROLL_STEP = 10; // px a frame

let pressed = false;
/** The pointer a press has taken, so an item's own grip and the item's hold don't both act on it. */
let claimed: number | null = null;

/** A task is held: a card's own HTML5 drag mustn't start under it. */
export function sorting(): boolean {
	return pressed;
}

/** The topmost element under the point that isn't the dragged item (which follows the pointer). */
function under(x: number, y: number, source: HTMLElement): Element | null {
	return document.elementsFromPoint(x, y).find((el) => !source.contains(el)) ?? null;
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

/** Where a task would land under the pointer, if anywhere. */
function findTaskHit(x: number, y: number, source: HTMLElement, kind: SortKind): Hit | null {
	const at = under(x, y, source);
	if (!at) return null;
	let item = at.closest<HTMLElement>('[data-sort-id]');
	if (kind === 'sub') {
		if (!item || item === source) return null;
		return item.dataset.sortKind === 'sub' ? { el: item, mode: half(item, y) } : { el: item, mode: 'into' };
	}
	const scope = source.closest<HTMLElement>('[data-sort-scope]');
	const underScope = at.closest<HTMLElement>('[data-sort-scope]');
	if (!scope || !underScope) return null;
	const board = scope.closest('[data-sort-board]');
	if (board && underScope.closest('[data-sort-board]') === board) return nearestIn(underScope, y, source);
	if (underScope !== scope) return null;
	// Over a subtask, it's the task that holds it.
	while (item && item.dataset.sortKind !== 'top') item = item.parentElement?.closest<HTMLElement>('[data-sort-id]') ?? null;
	if (item) return item === source ? null : { el: item, mode: half(item, y) };
	const zone = at.closest<HTMLElement>('[data-sort-zone]');
	return zone ? { el: zone, mode: 'zone' } : null;
}

/** Where a category would land under the pointer: before or after another in its list. */
function findCategoryHit(x: number, y: number, source: HTMLElement): Hit | null {
	const list = source.closest<HTMLElement>('[data-cat-list]');
	const item = under(x, y, source)?.closest<HTMLElement>('[data-cat-id]');
	if (!list || !item || item === source || item.closest('[data-cat-list]') !== list) return null;
	const s = getComputedStyle(list);
	const row = s.display.includes('flex') && s.flexDirection.startsWith('row');
	const r = item.getBoundingClientRect();
	const after = row ? x > r.left + r.width / 2 : y > r.top + r.height / 2;
	return { el: item, mode: after ? 'after' : 'before' };
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

/** What a drag is of: where it lands under the pointer, and what landing there does. */
interface Target {
	find(x: number, y: number, source: HTMLElement): Hit | null;
	land(hit: Hit, source: HTMLElement): void;
}

/**
 * The copy that follows the pointer. It's `position: fixed`, so a scrolling list or
 * board around it can't clip it, but it stays next to the item so the page's styles
 * still reach it. It takes no part in hit tests or drops: no pointer events, and none
 * of the data attributes the drag looks for.
 */
function makeGhost(lift: HTMLElement): HTMLElement {
	const r = lift.getBoundingClientRect();
	const ghost = lift.cloneNode(true) as HTMLElement;
	for (const el of [ghost, ...ghost.querySelectorAll<HTMLElement>('*')]) {
		el.removeAttribute('id');
		for (const name of [...el.getAttributeNames()]) {
			if (/^data-(sort|cat|flip|card)/.test(name)) el.removeAttribute(name);
		}
	}
	ghost.classList.remove('sort-dragging', 'sort-drop-before', 'sort-drop-after', 'sort-drop-into', 'sort-drop-zone');
	ghost.classList.add('sort-ghost');
	if (lift.hasAttribute('data-cat-id')) ghost.classList.add('sort-ghost-group');
	ghost.setAttribute('aria-hidden', 'true');
	ghost.inert = true;
	Object.assign(ghost.style, {
		left: `${r.left}px`,
		top: `${r.top}px`,
		width: `${r.width}px`,
		height: `${r.height}px`,
	});
	lift.after(ghost);
	return ghost;
}

/** One drag, from picking the item up to letting go. */
class Drag {
	hit: Hit | null = null;
	last: Point;
	frame = 0;
	/** What's picked up (a board card lifts whole, not just the task in it); it stays as the placeholder. */
	readonly lift: HTMLElement;
	readonly ghost: HTMLElement;
	readonly scrollers: { y: HTMLElement | null; x: HTMLElement | null };

	constructor(
		readonly source: HTMLElement,
		readonly start: Point,
		readonly target: () => Target,
	) {
		const wrap = source.parentElement;
		this.lift = wrap?.hasAttribute('data-sort-lift') ? wrap : source;
		this.last = start;
		this.scrollers = scrollersOf(source);
		this.ghost = makeGhost(this.lift);
		this.lift.classList.add('sort-dragging');
		document.documentElement.classList.add('sort-active');
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
		this.ghost.style.translate = `${x - this.start.x}px ${y - this.start.y}px`;
		this.setHit(this.target().find(x, y, this.source));
	}

	autoScroll = () => {
		const { x: sx, y: sy } = this.scrollers;
		let moved = false;
		if (sy) {
			const r = sy.getBoundingClientRect();
			const by = edgeStep(this.last.y, r.top, r.bottom);
			const to = Math.max(0, Math.min(sy.scrollHeight - sy.clientHeight, sy.scrollTop + by));
			if (to !== sy.scrollTop) { sy.scrollTop = to; moved = true; }
		}
		if (sx) {
			const r = sx.getBoundingClientRect();
			const by = edgeStep(this.last.x, r.left, r.right);
			const to = Math.max(0, Math.min(sx.scrollWidth - sx.clientWidth, sx.scrollLeft + by));
			if (to !== sx.scrollLeft) { sx.scrollLeft = to; moved = true; }
		}
		if (moved) this.move(this.last.x, this.last.y);
		this.frame = requestAnimationFrame(this.autoScroll);
	};

	end(commit: boolean) {
		const landed = commit ? this.hit : null;
		cancelAnimationFrame(this.frame);
		this.setHit(null);
		// The item glides from where the copy was let go to its place (or back home,
		// when it wasn't dropped anywhere).
		const at = this.ghost.getBoundingClientRect();
		this.ghost.remove();
		const key = this.lift.dataset.flip;
		const play = snapshot(document, key ? new Map([[key, at]]) : undefined);
		this.lift.classList.remove('sort-dragging');
		document.documentElement.classList.remove('sort-active');
		if (landed) this.target().land(landed, this.source);
		tick().then(play);
	}
}

/** How a press picks an item up: `drag` once it moves a little, `hold` after a moment still. */
type Pickup = 'drag' | 'hold' | null;

interface PressOptions {
	/** The item a press on this target picks up, or null to leave the press alone. */
	source(e: PointerEvent): HTMLElement | null;
	mouse: Pickup;
	touch: Pickup;
	target(): Target;
}

/** Turns presses on `node` into drags. Returns its teardown. */
function pressToDrag(node: HTMLElement, opts: () => PressOptions | null): () => void {
	let pointer: number | null = null;
	let source: HTMLElement | null = null;
	let pickup: Pickup = null;
	let timer = 0;
	let start: Point = { x: 0, y: 0 };
	let at: Point = { x: 0, y: 0 };
	let drag: Drag | null = null;

	function begin() {
		const o = opts();
		if (!o || !source) return finish(false);
		pressed = true;
		drag = new Drag(source, start, o.target);
		drag.move(at.x, at.y);
		if (pickup === 'hold') navigator.vibrate?.(15);
	}

	function finish(commit: boolean) {
		clearTimeout(timer);
		window.removeEventListener('pointermove', onMove, true);
		window.removeEventListener('pointerup', onUp, true);
		window.removeEventListener('pointercancel', onCancel, true);
		window.removeEventListener('keydown', onEscape, true);
		const was = drag;
		drag = null;
		if (claimed === pointer) claimed = null;
		pointer = null;
		source = null;
		pressed = false;
		if (!was) return;
		was.end(commit);
		// Letting go where it was picked up mustn't also open what's there.
		window.addEventListener('click', swallow, { capture: true, once: true });
		setTimeout(() => window.removeEventListener('click', swallow, true), 400);
	}

	function onMove(e: PointerEvent) {
		if (e.pointerId !== pointer) return;
		at = { x: e.clientX, y: e.clientY };
		if (drag) return drag.move(at.x, at.y);
		const far = Math.hypot(at.x - start.x, at.y - start.y);
		if (pickup === 'drag' && far >= DRAG_FROM) begin();
		// A finger that moves before the hold is a scroll.
		else if (pickup === 'hold' && far > HOLD_SLOP) finish(false);
	}

	const onUp = (e: PointerEvent) => e.pointerId === pointer && finish(true);
	const onCancel = (e: PointerEvent) => e.pointerId === pointer && finish(false);

	function onEscape(e: KeyboardEvent) {
		if (e.key !== 'Escape') return;
		e.preventDefault();
		e.stopPropagation();
		finish(false);
	}

	function onDown(e: PointerEvent) {
		const o = opts();
		// The innermost handler (a grip inside a row) gets the press first and keeps it.
		if (!o || pointer !== null || e.button !== 0 || claimed === e.pointerId) return;
		const how = e.pointerType === 'touch' ? o.touch : o.mouse;
		if (!how) return;
		const item = o.source(e);
		if (!item) return;
		pointer = claimed = e.pointerId;
		source = item;
		pickup = how;
		start = at = { x: e.clientX, y: e.clientY };
		if (how === 'drag') {
			pressed = true;
			// Keeps the moves coming when the pointer leaves the window.
			node.setPointerCapture?.(e.pointerId);
		} else {
			timer = window.setTimeout(begin, HOLD_MS);
		}
		window.addEventListener('pointermove', onMove, true);
		window.addEventListener('pointerup', onUp, true);
		window.addEventListener('pointercancel', onCancel, true);
		window.addEventListener('keydown', onEscape, true);
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
	return () => {
		if (pointer !== null) finish(false);
		node.removeEventListener('pointerdown', onDown);
		node.removeEventListener('touchmove', onTouchMove);
		node.removeEventListener('contextmenu', onContextMenu);
	};
}

type GripOptions = SortApi & { id: string; kind: SortKind };

function taskTarget(opts: GripOptions): Target {
	return {
		find: (x, y, source) => findTaskHit(x, y, source, opts.kind),
		land: (hit) =>
			opts.ondrop({
				id: opts.id,
				kind: opts.kind,
				targetId: hit.mode === 'zone' ? null : hit.el.dataset.sortId ?? null,
				after: hit.mode === 'after',
				into: hit.mode === 'into',
				zone: hit.mode === 'zone' ? hit.el.dataset.sortZone ?? null : null,
				scope: hit.el.closest<HTMLElement>('[data-sort-scope]')?.dataset.sortScope ?? null,
			}),
	};
}

/** Makes an element the grip of the `[data-sort-id]` item around it. */
export function sortGrip(node: HTMLElement, initial: GripOptions) {
	let opts = initial;
	const teardown = pressToDrag(node, () => ({
		source: () => node.closest<HTMLElement>('[data-sort-id]'),
		mouse: 'drag',
		touch: 'drag',
		target: () => taskTarget(opts),
	}));

	async function onKey(e: KeyboardEvent) {
		if (e.key !== 'ArrowUp' && e.key !== 'ArrowDown') return;
		e.preventDefault();
		e.stopPropagation();
		opts.onstep(opts.id, opts.kind, e.key === 'ArrowUp' ? -1 : 1);
		// Moving a row in the DOM can take the focus off it.
		await tick();
		node.focus();
	}

	node.addEventListener('keydown', onKey);
	return {
		update(next: GripOptions) {
			opts = next;
		},
		destroy() {
			teardown();
			node.removeEventListener('keydown', onKey);
		},
	};
}

/**
 * Touch and hold a `[data-sort-id]` item to pick it up, then drag it (touch only; the
 * mouse has the grip, or a board card's own drag). Null options turn it off.
 */
export function sortHold(node: HTMLElement, initial: GripOptions | null) {
	let opts = initial;
	const teardown = pressToDrag(node, () =>
		opts && {
			source: (e) => {
				const t = e.target as Element | null;
				// A subtask listed under it holds itself.
				if (t?.closest('[data-sort-id]') !== node) return null;
				if (t?.closest('input, textarea, [contenteditable="true"]')) return null;
				return node;
			},
			mouse: null,
			touch: 'hold',
			target: () => taskTarget(opts!),
		},
	);
	return {
		update(next: GripOptions | null) {
			opts = next;
		},
		destroy: teardown,
	};
}

export interface CategorySortOptions {
	id: string;
	/** It landed before or after the category `targetId`. */
	ondrop: (id: string, targetId: string, after: boolean) => void;
}

/**
 * Makes a category's head pick up the `[data-cat-id]` around it: the mouse drags it
 * at once, a finger holds it first. Its buttons and the rename box are left alone.
 * Null options turn it off.
 */
export function sortCategory(node: HTMLElement, initial: CategorySortOptions | null) {
	let opts = initial;
	const teardown = pressToDrag(node, () =>
		opts && {
			source: (e) => {
				if ((e.target as Element | null)?.closest('button, a, input, textarea, [role="menu"]')) return null;
				return node.closest<HTMLElement>('[data-cat-id]');
			},
			mouse: 'drag',
			touch: 'hold',
			target: () => ({
				find: findCategoryHit,
				land: (hit) => {
					const target = hit.el.dataset.catId;
					if (opts && target) opts.ondrop(opts.id, target, hit.mode === 'after');
				},
			}),
		},
	);
	return {
		update(next: CategorySortOptions | null) {
			opts = next;
		},
		destroy: teardown,
	};
}

function swallow(e: Event) {
	e.preventDefault();
	e.stopPropagation();
}
