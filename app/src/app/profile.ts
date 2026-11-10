/**
 * The profile: a photo (a small square JPEG) and a display name. They live in
 * the tasks DB (the `profile` row, see core/migrations/0002_profile.sql), so
 * they sync like the tasks and follow the account to every device.
 *
 * The stores are keyed by username, which is what the components look up.
 * `startProfile` fills them from the local DB and keeps them current.
 */
import { writable } from 'svelte/store';
import { browser } from '$app/environment';
import { onTasksChanged } from '$lib/tasks/client';
import { call, write } from './tauriClient';

const SIZE = 128;
const QUALITY = 0.8;

interface ProfileRow {
	display_name: string | null;
	photo: string | null;
}

export const photos = writable<Record<string, string>>({});
export const displayNames = writable<Record<string, string>>({});

function show(username: string, row: ProfileRow) {
	const put = (store: typeof photos, value: string | null) =>
		store.update((all) => {
			const next = { ...all };
			if (value) next[username] = value;
			else delete next[username];
			return next;
		});
	put(photos, row.photo);
	put(displayNames, row.display_name);
}

/** The middle square of the image, scaled down. */
async function squareJpeg(source: Blob): Promise<string> {
	const bitmap = await createImageBitmap(source);
	const side = Math.min(bitmap.width, bitmap.height);
	const canvas = document.createElement('canvas');
	canvas.width = canvas.height = SIZE;
	const ctx = canvas.getContext('2d');
	if (!ctx) throw new Error("This device can't resize the photo");
	ctx.imageSmoothingQuality = 'high';
	ctx.drawImage(bitmap, (bitmap.width - side) / 2, (bitmap.height - side) / 2, side, side, 0, 0, SIZE, SIZE);
	bitmap.close();
	return canvas.toDataURL('image/jpeg', QUALITY);
}

async function update(username: string, patch: Partial<ProfileRow>) {
	show(username, await write<ProfileRow>('profile_update', { patch }));
}

export async function setPhoto(username: string, file: File) {
	if (!file.type.startsWith('image/')) throw new Error('Pick an image file');
	await update(username, { photo: await squareJpeg(file) });
}

export function removePhoto(username: string) {
	return update(username, { photo: null });
}

/** Saves the name shown for `username`; an empty one goes back to the username. */
export function setDisplayName(username: string, name: string) {
	const clean = name.trim().slice(0, 60);
	return update(username, { display_name: clean && clean !== username ? clean : null });
}

/** The display name, else the username. */
export const nameFor = (names: Record<string, string>, username: string | null) =>
	(username && names[username]) || username || 'Account';

// ── Loading ─────────────────────────────────────────────────────────────────

let current: string | null = null;
let stopListening: (() => void) | null = null;

async function refresh() {
	if (!current) return;
	const username = current;
	try {
		const row = await call<ProfileRow>('profile_get');
		if (username === current) show(username, row);
	} catch {
		/* the next change loads it */
	}
}

/**
 * Shows `username`'s profile from the local DB, and again whenever the tasks
 * change (a sync pulled it from another device, or another window saved it).
 * Pass `ready` once this device's first sync has finished: only then is it safe
 * to move a profile saved here by an older version up to the account.
 */
export async function startProfile(username: string, ready: boolean) {
	if (!browser) return;
	if (current !== username) {
		current = username;
		stopListening?.();
		stopListening = onTasksChanged(refresh);
	}
	await refresh();
	if (ready) await adoptLegacy(username);
}

// Before profiles synced they were kept in localStorage, per username.
const LEGACY_PHOTOS = 'profile-photos';
const LEGACY_NAMES = 'profile-names';

function legacy(key: string): Record<string, string> {
	try {
		const raw = JSON.parse(localStorage.getItem(key) ?? '{}');
		return raw && typeof raw === 'object' ? raw : {};
	} catch {
		return {};
	}
}

function forget(key: string, username: string) {
	try {
		const all = legacy(key);
		delete all[username];
		localStorage.setItem(key, JSON.stringify(all));
	} catch {
		/* left behind, harmless */
	}
}

/** Moves a photo and name kept on this device into the account, unless the account already has a profile. */
async function adoptLegacy(username: string) {
	const photo = legacy(LEGACY_PHOTOS)[username];
	const name = legacy(LEGACY_NAMES)[username];
	if (!photo && !name) return;
	try {
		const row = await call<ProfileRow>('profile_get');
		if (!row.photo && !row.display_name) {
			const patch: Partial<ProfileRow> = {};
			if (name) patch.display_name = name;
			if (photo) patch.photo = await squareJpeg(await (await fetch(photo)).blob());
			await update(username, patch);
		}
		forget(LEGACY_PHOTOS, username);
		forget(LEGACY_NAMES, username);
	} catch {
		/* tried again the next time the app starts */
	}
}
