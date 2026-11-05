// Today's count on the Windows taskbar icon, at the top right. Windows only ever
// puts an overlay icon at the bottom right, so the number is drawn into the icon
// itself, and Rust sets the result as the window's icon (src-tauri/src/widgets.rs).

import { call } from './tauriClient';
import { isDesktop } from './widgets';

const SIZE = 64;
// The same drawing as icons-src/icon.svg, whose coordinates are out of 1024.
const ICON_SCALE = SIZE / 1024;
const ICON_GREEN = '#14532d';

function draw(count: number): Uint8ClampedArray | null {
	const canvas = document.createElement('canvas');
	canvas.width = canvas.height = SIZE;
	const c = canvas.getContext('2d');
	if (!c) return null;

	c.save();
	c.scale(ICON_SCALE, ICON_SCALE);
	c.fillStyle = ICON_GREEN;
	c.beginPath();
	c.roundRect(0, 0, 1024, 1024, 232);
	c.fill();
	c.strokeStyle = '#ffffff';
	c.lineWidth = 120;
	c.lineCap = 'round';
	c.lineJoin = 'round';
	c.beginPath();
	c.moveTo(292, 540);
	c.lineTo(448, 696);
	c.lineTo(748, 368);
	c.stroke();
	c.restore();

	// The badge is large: the taskbar shows this at about 32 px.
	const label = count > 99 ? '99+' : String(count);
	const radius = 21;
	const [x, y] = [SIZE - radius, radius];
	c.fillStyle = '#dc2626';
	c.strokeStyle = '#ffffff';
	c.lineWidth = 3;
	c.beginPath();
	c.arc(x, y, radius - 1.5, 0, Math.PI * 2);
	c.fill();
	c.stroke();
	c.fillStyle = '#ffffff';
	c.font = `700 ${label.length === 1 ? 32 : label.length === 2 ? 26 : 19}px system-ui, "Segoe UI", sans-serif`;
	c.textAlign = 'center';
	c.textBaseline = 'middle';
	c.fillText(label, x, y + 1);

	return c.getImageData(0, 0, SIZE, SIZE).data;
}

let shown: number | null = null;

/** Shows the count on the taskbar icon; 0 goes back to the plain icon. */
export function setTaskbarCount(count: number) {
	if (!isDesktop || count === shown) return;
	shown = count;
	const pixels = count > 0 ? draw(count) : null;
	call('app_set_taskbar_icon', { size: SIZE, rgba: pixels ? Array.from(pixels) : null }).catch((e) =>
		console.error('Could not set the taskbar count', e),
	);
}
