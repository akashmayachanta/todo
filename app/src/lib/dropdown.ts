/** Where a dropdown panel goes: fixed to the viewport so scrolling modals and tables can't clip it. */
export interface Placement {
	left?: number;
	right?: number;
	top?: number;
	bottom?: number;
	minWidth: number;
	maxHeight: number;
}

const GAP = 4;
const MARGIN = 8;

/**
 * Opens below the trigger, or above it when there is more room there. `align: 'end'` lines the
 * panel's right edge up with the trigger's instead of the left.
 */
export function placePanel(trigger: HTMLElement, wanted = 256, align: 'start' | 'end' = 'start'): Placement {
	const r = trigger.getBoundingClientRect();
	const below = window.innerHeight - r.bottom - GAP - MARGIN;
	const above = r.top - GAP - MARGIN;
	const up = below < Math.min(wanted, 160) && above > below;
	const horizontal = align === 'end' ? { right: window.innerWidth - r.right } : { left: r.left };
	return {
		...horizontal,
		...(up ? { bottom: window.innerHeight - r.top + GAP } : { top: r.bottom + GAP }),
		minWidth: r.width,
		maxHeight: Math.max(96, Math.min(wanted, up ? above : below)),
	};
}

export function placementStyle(p: Placement): string {
	const side = [
		p.left !== undefined && `left:${p.left}px`,
		p.right !== undefined && `right:${p.right}px`,
		p.top !== undefined && `top:${p.top}px`,
		p.bottom !== undefined && `bottom:${p.bottom}px`,
	].filter(Boolean);
	return [...side, `min-width:${p.minWidth}px`, `max-height:${p.maxHeight}px`].join(';');
}
