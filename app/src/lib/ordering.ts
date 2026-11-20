/**
 * The new order after a drop or a Move up / Move down, for tasks, sections and
 * projects alike. The drag code (sortable.ts, the pane's and the Projects screen's)
 * only reports where something landed; these work out the order to show and save.
 * Plain lists of ids in, lists out: no DOM, no saving.
 */

/**
 * `ids` with `id` put before or after `targetId`. With no target it goes last.
 * `id` need not be in `ids` yet (a task joining another section). Null when the
 * target isn't in the list, or is `id` itself: there is nowhere to put it.
 */
export function placeBeside(ids: readonly string[], id: string, targetId: string | null, after: boolean): string[] | null {
	if (targetId === id) return null;
	const others = ids.filter((x) => x !== id);
	if (targetId === null) return [...others, id];
	const at = others.indexOf(targetId);
	if (at < 0) return null;
	others.splice(after ? at + 1 : at, 0, id);
	return others;
}

/** Whether two orders are the same, so a drop back where it was saves nothing. */
export function sameOrder(a: readonly string[], b: readonly string[]): boolean {
	return a.length === b.length && a.every((x, i) => x === b[i]);
}

/**
 * For Move up / Move down (and the arrow keys on a grip): the item `by` places from
 * `id`, and whether `id` goes after it. Null at either end.
 */
export function stepTarget(ids: readonly string[], id: string, by: -1 | 1): { targetId: string; after: boolean } | null {
	const i = ids.indexOf(id);
	const other = i < 0 ? undefined : ids[i + by];
	return other === undefined ? null : { targetId: other, after: by === 1 };
}

/**
 * The positions to show at once for items put in this order, as tasks-core's
 * reorder_tasks saves them: the order takes the positions the items already
 * held, smallest first, and items that shared one get their own.
 */
export function heldPositions(ordered: readonly { id: string; position: number }[]): Map<string, number> {
	const slots = ordered.map((t) => t.position).sort((a, b) => a - b);
	const out = new Map<string, number>();
	let last = -Infinity;
	ordered.forEach((t, i) => {
		last = Math.max(slots[i], last + 1);
		out.set(t.id, last);
	});
	return out;
}

/** The positions to show at once for sections put in this order: 1, 2, 3…, as tasks-core's reorder_sections saves them. */
export function rankPositions(ids: readonly string[]): Map<string, number> {
	return new Map(ids.map((x, i) => [x, i + 1]));
}
