/**
 * Rearranging tasks by a grip. It uses pointer events rather than HTML5 drag, so
 * it works by touch on Android too, and arrow keys on a focused grip do the same.
 *
 * The rows say what they are through data attributes (TaskRow, TaskDetail, the page):
 * - `data-sort-id`, `data-sort-kind` ('top' or 'sub'): a task that can be dropped around.
 * - `data-sort-scope`: the list a top-level task stays in (a board's column).
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
}

export interface SortApi {
	ondrop: (drop: SortDrop) => void;
	onstep: (id: string, kind: SortKind, by: -1 | 1) => void;
}

type Mode = 'before' | 'after' | 'into' | 'zone';
type Hit = { el: HTMLElement; mode: Mode };

const DRAG_FROM = 4; // px moved before a press becomes a drag
const SCROLL_EDGE = 48; // px from a scroller's edge where it starts to scroll
const SCROLL_STEP = 10; // px a frame

let pressed = false;

/** A grip is held: a card's own HTML5 drag mustn't start under it. */
export function sorting(): boolean {
	return pressed;
}

function half(el: HTMLElement, y: number): Mode {
	// A task's item also holds its subtasks; the line goes by the whole of it.
	const r = el.getBoundingClientRect();
	return y > r.top + r.height / 2 ? 'after' : 'before';
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
	const scope = source.closest('[data-sort-scope]');
	if (!scope || under.closest('[data-sort-scope]') !== scope) return null;
	// Over a subtask, it's the task that holds it.
	while (item && item.dataset.sortKind !== 'top') item = item.parentElement?.closest<HTMLElement>('[data-sort-id]') ?? null;
	if (item) return item === source ? null : { el: item, mode: half(item, y) };
	const zone = under.closest<HTMLElement>('[data-sort-zone]');
	return zone ? { el: zone, mode: 'zone' } : null;
}

function mark(hit: Hit | null, on: boolean) {
	if (hit) hit.el.classList.toggle(`sort-drop-${hit.mode}`, on);
}

function scrollerOf(el: HTMLElement): HTMLElement | null {
	for (let at = el.parentElement; at; at = at.parentElement) {
		const { overflowY } = getComputedStyle(at);
		if ((overflowY === 'auto' || overflowY === 'scroll') && at.scrollHeight > at.clientHeight) return at;
	}
	return null;
}

type GripOptions = SortApi & { id: string; kind: SortKind };

/** Makes an element the grip of the `[data-sort-id]` item around it. */
export function sortGrip(node: HTMLElement, initial: GripOptions) {
	let opts = initial;
	let source: HTMLElement | null = null;
	let scroller: HTMLElement | null = null;
	let hit: Hit | null = null;
	let dragging = false;
	let start = { x: 0, y: 0 };
	let last = { x: 0, y: 0 };
	let frame = 0;

	function setHit(next: Hit | null) {
		if (next?.el === hit?.el && next?.mode === hit?.mode) return;
		mark(hit, false);
		mark(next, true);
		hit = next;
	}

	function autoScroll() {
		if (!dragging) return;
		if (scroller) {
			const r = scroller.getBoundingClientRect();
			const by = last.y < r.top + SCROLL_EDGE ? -SCROLL_STEP : last.y > r.bottom - SCROLL_EDGE ? SCROLL_STEP : 0;
			if (by) {
				scroller.scrollTop += by;
				setHit(findHit(last.x, last.y, source!, opts.kind));
			}
		}
		frame = requestAnimationFrame(autoScroll);
	}

	function finish(commit: boolean) {
		const landed = commit && dragging ? hit : null;
		cancelAnimationFrame(frame);
		setHit(null);
		source?.classList.remove('sort-dragging');
		document.documentElement.classList.remove('sort-active');
		node.removeEventListener('pointermove', onMove);
		node.removeEventListener('pointerup', onUp);
		node.removeEventListener('pointercancel', onCancel);
		window.removeEventListener('keydown', onEscape, true);
		dragging = false;
		pressed = false;
		source = null;
		if (!landed) return;
		opts.ondrop({
			id: opts.id,
			kind: opts.kind,
			targetId: landed.mode === 'zone' ? null : landed.el.dataset.sortId ?? null,
			after: landed.mode === 'after',
			into: landed.mode === 'into',
			zone: landed.mode === 'zone' ? landed.el.dataset.sortZone ?? null : null,
		});
	}

	function onMove(e: PointerEvent) {
		last = { x: e.clientX, y: e.clientY };
		if (!dragging) {
			if (Math.hypot(e.clientX - start.x, e.clientY - start.y) < DRAG_FROM) return;
			dragging = true;
			source!.classList.add('sort-dragging');
			document.documentElement.classList.add('sort-active');
			scroller = scrollerOf(source!);
			frame = requestAnimationFrame(autoScroll);
		}
		setHit(findHit(e.clientX, e.clientY, source!, opts.kind));
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
