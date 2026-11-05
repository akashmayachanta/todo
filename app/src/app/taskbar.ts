// Today's count as a badge on the Windows taskbar button. Rust keeps the badge and
// sets it again when the window comes back from the tray (src-tauri/src/widgets.rs).

import { call } from './tauriClient';
import { isDesktop } from './widgets';

const SIZE = 32;

function draw(count: number): Uint8ClampedArray | null {
	const canvas = document.createElement('canvas');
	canvas.width = canvas.height = SIZE;
	const c = canvas.getContext('2d');
	if (!c) return null;

	c.fillStyle = '#dc2626';
	c.beginPath();
	c.arc(SIZE / 2, SIZE / 2, SIZE / 2, 0, Math.PI * 2);
	c.fill();

	// Windows shows this at about 16 px, so the digits fill the circle.
	const label = count > 99 ? '99+' : String(count);
	c.fillStyle = '#ffffff';
	c.font = `700 ${label.length === 1 ? 26 : label.length === 2 ? 20 : 14}px system-ui, "Segoe UI", sans-serif`;
	c.textAlign = 'center';
	c.textBaseline = 'middle';
	c.fillText(label, SIZE / 2, SIZE / 2 + 1);

	return c.getImageData(0, 0, SIZE, SIZE).data;
}

let shown: number | null = null;

/** Shows the count on the taskbar button; 0 removes the badge. */
export function setTaskbarCount(count: number) {
	if (!isDesktop || count === shown) return;
	shown = count;
	const pixels = count > 0 ? draw(count) : null;
	call('app_set_taskbar_badge', { size: SIZE, rgba: pixels ? Array.from(pixels) : null }).catch((e) =>
		console.error('Could not set the taskbar count', e),
	);
}
