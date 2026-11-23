/// <reference types="@sveltejs/kit" />
/// <reference no-default-lib="true"/>
/// <reference lib="esnext" />
/// <reference lib="webworker" />

// The browser copy's service worker, so a phone can install it (registered in
// +layout.svelte, never inside Tauri). It only keeps the app's own files: the page opens
// at once, even on a bad connection. Tasks still come from the server; the API
// (/tasks-api, /sync) is outside its scope and never touched.
import { base, build, files, version } from '$service-worker';

const sw = self as unknown as ServiceWorkerGlobalScope;

const CACHE = `tasks-shell-${version}`;
const INDEX = `${base}/`;
/** This build's files: hashed, so a cached copy is always right. */
const ASSETS = new Set([...build, ...files]);
/** Fetched on install. Fonts come in on first use instead, as there are many subsets. */
const PRECACHE = [INDEX, ...[...ASSETS].filter((p) => !/\.woff2?$/.test(p))];

sw.addEventListener('install', (event) => {
	event.waitUntil(
		caches
			.open(CACHE)
			.then((cache) => cache.addAll(PRECACHE))
			.then(() => sw.skipWaiting()),
	);
});

sw.addEventListener('activate', (event) => {
	event.waitUntil(
		caches
			.keys()
			.then((keys) => Promise.all(keys.filter((k) => k !== CACHE).map((k) => caches.delete(k))))
			.then(() => sw.clients.claim()),
	);
});

sw.addEventListener('fetch', (event) => {
	const req = event.request;
	if (req.method !== 'GET') return;
	const url = new URL(req.url);
	if (url.origin !== sw.location.origin) return;

	if (ASSETS.has(url.pathname)) {
		event.respondWith(cacheFirst(req, url.pathname));
	} else if (req.mode === 'navigate' && (url.pathname === base || url.pathname.startsWith(INDEX))) {
		event.respondWith(page(req));
	}
});

async function cacheFirst(req: Request, key: string): Promise<Response> {
	const cache = await caches.open(CACHE);
	const hit = await cache.match(key);
	if (hit) return hit;
	const res = await fetch(req);
	if (res.ok) cache.put(key, res.clone());
	return res;
}

/** Any page is the single-page app's index.html: the newest one, or the saved one offline. */
async function page(req: Request): Promise<Response> {
	const cache = await caches.open(CACHE);
	try {
		const res = await fetch(req);
		if (res.ok && res.headers.get('content-type')?.startsWith('text/html')) cache.put(INDEX, res.clone());
		return res;
	} catch (e) {
		const hit = await cache.match(INDEX);
		if (hit) return hit;
		throw e;
	}
}
