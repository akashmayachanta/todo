/**
 * The profile, per username, kept on this device (localStorage): a photo (a
 * small square JPEG) and a display name. They don't sync: the server has
 * nowhere to keep them yet.
 */
import { writable } from 'svelte/store';
import { browser } from '$app/environment';

const KEY = 'profile-photos';
const SIZE = 192;

function read(): Record<string, string> {
	try {
		const raw = JSON.parse(localStorage.getItem(KEY) ?? '{}');
		return raw && typeof raw === 'object' ? raw : {};
	} catch {
		return {};
	}
}

export const photos = writable<Record<string, string>>(browser ? read() : {});

function save(next: Record<string, string>) {
	localStorage.setItem(KEY, JSON.stringify(next));
	photos.set(next);
}

/** The middle square of the image, scaled down. */
async function squareJpeg(file: File): Promise<string> {
	const bitmap = await createImageBitmap(file);
	const side = Math.min(bitmap.width, bitmap.height);
	const canvas = document.createElement('canvas');
	canvas.width = canvas.height = SIZE;
	const ctx = canvas.getContext('2d');
	if (!ctx) throw new Error("This device can't resize the photo");
	ctx.imageSmoothingQuality = 'high';
	ctx.drawImage(bitmap, (bitmap.width - side) / 2, (bitmap.height - side) / 2, side, side, 0, 0, SIZE, SIZE);
	bitmap.close();
	return canvas.toDataURL('image/jpeg', 0.86);
}

export async function setPhoto(username: string, file: File) {
	if (!file.type.startsWith('image/')) throw new Error('Pick an image file');
	const url = await squareJpeg(file);
	save({ ...read(), [username]: url });
}

export function removePhoto(username: string) {
	const next = read();
	delete next[username];
	save(next);
}

// ── Display name ────────────────────────────────────────────────────────────

const NAMES_KEY = 'profile-names';

function readNames(): Record<string, string> {
	try {
		const raw = JSON.parse(localStorage.getItem(NAMES_KEY) ?? '{}');
		return raw && typeof raw === 'object' ? raw : {};
	} catch {
		return {};
	}
}

export const displayNames = writable<Record<string, string>>(browser ? readNames() : {});

/** Saves the name shown for `username`; an empty one goes back to the username. */
export function setDisplayName(username: string, name: string) {
	const next = readNames();
	const clean = name.trim().slice(0, 60);
	if (clean && clean !== username) next[username] = clean;
	else delete next[username];
	try {
		localStorage.setItem(NAMES_KEY, JSON.stringify(next));
	} catch {
		/* not remembered after a reload */
	}
	displayNames.set(next);
}

/** The display name, else the username. */
export const nameFor = (names: Record<string, string>, username: string | null) =>
	(username && names[username]) || username || 'Account';
