<script lang="ts">
	import '@fontsource-variable/hanken-grotesk';
	import '@fontsource-variable/figtree/wght.css';
	import '@fontsource-variable/figtree/wght-italic.css';
	import '@fontsource/ibm-plex-mono/400.css';
	import '@fontsource/ibm-plex-mono/500.css';
	import '../app.css';
	import '$lib/styles/global.css';
	import { onMount } from 'svelte';
	import { goto } from '$app/navigation';
	import { listen } from '@tauri-apps/api/event';
	import ToastHost from '$lib/components/ToastHost.svelte';
	import ConfirmDialog from '$lib/components/ConfirmDialog.svelte';
	import QuickAddDialog from '$lib/components/tasks/QuickAddDialog.svelte';
	import { setTasksClient, tasksChanged } from '$lib/tasks/client';
	import { isQuickAddShortcut, openQuickAdd, quickAdd, quickAddKeys, type QuickAddDefaults } from '$lib/stores/taskQuickAdd';
	import { addDays, ymd } from '$lib/taskParse';
	import { keyLabels } from '$lib/shortcutKeys';
	import { applyTheme, followSystemTheme } from '$lib/stores/theme';
	// Loaded in every window (the pins and popup too): it puts the date & time settings in effect.
	import '$lib/stores/taskSettings';
	import { setPaneFooter, setPaneHeader } from '$lib/paneHeader';
	import { getCurrentWindow } from '@tauri-apps/api/window';
	import { EVENT_LOCAL_CHANGED, call, tauriTasksClient, type AppStatus, type SyncStatus } from '$shell/tauriClient';
	import Setup from '$shell/Setup.svelte';
	import FirstSync from '$shell/FirstSync.svelte';
	import AccountMenu from '$shell/AccountMenu.svelte';
	import PaneStatus from '$shell/PaneStatus.svelte';
	import { checkForUpdate } from '$shell/updates';
	import { followTitleBar } from '$shell/titlebar';
	import SettingsDialog, { type SettingsTab } from '$shell/SettingsDialog.svelte';
	import { startProfile } from '$shell/profile';
	import { isTauri } from '$shell/platform';
	import { API_BASE, followServerChanges, webSession, webSignOut, webTasksClient } from '$shell/webClient';
	import WebSignIn from '$shell/WebSignIn.svelte';
	import { base, resolve } from '$app/paths';
	import { dev } from '$app/environment';

	// Before any page loads: in the app, tasks come from the local DB (Rust), and
	// reminders are system notifications scheduled there too (src-tauri/src/reminders.rs).
	// In a browser (tasks-server's /tasks-app page) they come straight from the server.
	setTasksClient(isTauri ? tauriTasksClient : webTasksClient);

	let { children } = $props();

	// The popup and the pins are pages of their own, without the app around them.
	// Anything that only makes sense once (a tapped task, Alt+N) stays with the main window.
	const widget = isTauri && window.location.pathname.startsWith(`${base}/widget`);

	let status = $state<AppStatus | null>(null);

	// The browser has no sync of its own: every call is a server round, so it is always "synced".
	$effect(() => {
		if (isTauri) return;
		const session = $webSession;
		status = session
			? {
					configured: true,
					server: { url: window.location.origin, tasks_base: API_BASE },
					username: session.username,
					sync: { state: 'synced', message: null, last_synced_at: null, pending: 0, ready: true },
					platform: 'web',
				}
			: null;
	});

	// The browser hears of other devices' writes over GET /sync/events.
	$effect(() => {
		if (!isTauri && $webSession) return followServerChanges();
	});
	// The sign-in screen, opened from Settings → Account (signed in already).
	let showSetup = $state(false);
	// Signed in, but the first sync hasn't brought the tasks here yet.
	let waitingForFirstSync = $derived(!!status?.configured && !status.sync.ready);
	// Settings, open on one of its sections.
	let settingsTab = $state<SettingsTab | null>(null);
	let ready = $derived(!!status && status.configured && !showSetup && !waitingForFirstSync);

	// The signed-in account's photo and name, from the local DB (they sync with the tasks).
	$effect(() => {
		if (status?.configured && status.username) startProfile(status.username, status.sync.ready);
	});

	// The account menu tops the Tasks pane (lib/components/tasks/TasksPane.svelte).
	setPaneHeader(paneHeader);
	setPaneFooter(paneFooter);

	function openSettings(tab: SettingsTab = 'account') {
		settingsTab = tab;
	}

	function openTask(id: string) {
		goto(`${resolve('/tasks' as any)}?task=${encodeURIComponent(id)}`);
	}

	/**
	 * The quick add, with what Android's widget or the app icon's "Add task" asked for
	 * (app_take_quick_add: `{project_id?, section_id?, due?: 'today' | 'tomorrow'}`;
	 * `project_id: null` is the Inbox),
	 * else the page's own defaults. `force` opens it even when nothing was asked (Alt+N).
	 */
	async function takeQuickAdd(force: boolean) {
		const asked = await call<string | null>('app_take_quick_add').catch(() => null);
		if (!asked) {
			if (force) openQuickAdd();
			return;
		}
		try {
			const a = JSON.parse(asked) as { project_id?: string | null; section_id?: string; due?: string };
			// The app icon's "Add task" asks for nothing in particular: the page's defaults.
			if (!Object.keys(a).length) {
				openQuickAdd();
				return;
			}
			const today = ymd(new Date());
			const defaults: QuickAddDefaults = {};
			if ('project_id' in a) defaults.project_id = a.project_id ?? null;
			if (a.section_id) defaults.section_id = a.section_id;
			if (a.due === 'today') defaults.due_date = today;
			else if (a.due === 'tomorrow') defaults.due_date = addDays(today, 1);
			openQuickAdd(defaults);
		} catch {
			openQuickAdd();
		}
	}

	/** A view Android's widget or an app shortcut asked for: the Tasks page's query string. */
	async function takeOpenView() {
		const query = await call<string | null>('app_take_open_view').catch(() => null);
		if (query) goto(`${resolve('/tasks' as any)}?${query}`);
	}

	function syncNow() {
		// The browser has nothing to push: "sync" loads the tasks again.
		if (!isTauri) return tasksChanged();
		call('app_sync_now').catch(() => {});
	}

	function forceSync() {
		if (!isTauri) return tasksChanged();
		call('app_force_sync').catch(() => {});
	}

	// The quick add shortcut while the window has focus (the global one covers the rest of Windows); Ctrl+, Settings.
	function onKey(e: KeyboardEvent) {
		if (widget) return;
		if (isQuickAddShortcut(e)) {
			e.preventDefault();
			openQuickAdd();
		} else if (e.key === ',' && (e.ctrlKey || e.metaKey) && !e.altKey && ready) {
			e.preventDefault();
			openSettings(settingsTab ?? 'account');
		}
	}

	/** The browser: no Tauri events or commands, just the keys and a reload when the tab comes back. */
	function onMountWeb(stopTheme: () => void) {
		function onVisible() {
			if (document.visibilityState === 'visible') tasksChanged();
		}
		// Lets a phone install the page as an app (src/service-worker.ts). Needs HTTPS.
		if (!dev && 'serviceWorker' in navigator) {
			navigator.serviceWorker.register(`${base}/service-worker.js`).catch(() => {});
		}
		window.addEventListener('online', tasksChanged);
		document.addEventListener('visibilitychange', onVisible);
		document.addEventListener('keydown', onKey);
		return () => {
			window.removeEventListener('online', tasksChanged);
			document.removeEventListener('visibilitychange', onVisible);
			document.removeEventListener('keydown', onKey);
			stopTheme();
		};
	}

	async function signOut() {
		if (!isTauri) return webSignOut();
		status = await call<AppStatus>('app_sign_out');
	}

	onMount(() => {
		applyTheme();
		const stopTheme = followSystemTheme();
		if (!isTauri) return onMountWeb(stopTheme);
		const stops = [
			listen('tasks://changed', () => tasksChanged()),
			// Another window (a pin, the popup, the main one) wrote something.
			listen<string>(EVENT_LOCAL_CHANGED, (e) => {
				if (e.payload !== getCurrentWindow().label) tasksChanged();
			}),
		];
		if (!widget) {
			call<AppStatus>('app_status').then((s) => {
				status = s;
				if (s.configured && s.sync.ready) checkForUpdate();
			});
			call<string | null>('app_take_open_task').then((id) => id && openTask(id));
			// Android: the widget or an app shortcut started the app.
			takeOpenView().then(() => takeQuickAdd(false));
			// Settings → Desktop's quick add shortcut, for the window's own key handler below.
			call<{ quick_add: string | null } | null>('app_desktop_get')
				.then((d) => d && quickAddKeys.set(d.quick_add))
				.catch(() => {});
			call('app_ask_notification_permission').catch(() => {});
			stops.push(followTitleBar());
			stops.push(
				listen<SyncStatus>('tasks://sync', (e) => {
					if (status) status = { ...status, sync: e.payload, configured: e.payload.state !== 'signed_out' && status.configured };
				}),
				listen('tasks://quick-add', () => takeQuickAdd(true)),
				listen('tasks://open-view', () => takeOpenView()),
				listen<string>('tasks://open-task', (e) => openTask(e.payload)),
				// The quick add popup's `>`: a view, project or filter, as the Tasks page's query string.
				listen<string>('tasks://show-view', (e) => goto(`${resolve('/tasks' as any)}?${e.payload}`)),
			);
		}

		// Sync as soon as the network comes back, or the app comes back to the front
		// (Android resumes it), instead of waiting for the next retry. The page loads
		// its tasks again too: a sleeping page can miss the event for a sync's changes.
		function onVisible() {
			const visible = document.visibilityState === 'visible';
			// Android keeps the live sync connection open only while the app is on screen.
			if (!widget) call('app_foreground', { visible }).catch(() => {});
			if (!visible) return;
			syncNow();
			tasksChanged();
			if (!widget && ready) checkForUpdate();
		}
		window.addEventListener('online', syncNow);
		document.addEventListener('visibilitychange', onVisible);

		document.addEventListener('keydown', onKey);
		return () => {
			window.removeEventListener('online', syncNow);
			document.removeEventListener('visibilitychange', onVisible);
			document.removeEventListener('keydown', onKey);
			stopTheme();
			stops.forEach((p) => p.then((stop) => stop()));
		};
	});
</script>

{#snippet paneHeader()}
	{#if status}
		<AccountMenu {status} onsync={syncNow} onsettings={openSettings} onsignout={isTauri ? undefined : signOut} />
	{/if}
{/snippet}

{#snippet paneFooter()}
	{#if status}
		<PaneStatus {status} onsync={syncNow} onforcesync={forceSync} />
	{/if}
{/snippet}

{#if widget}
	{@render children()}
	<ToastHost />
{:else}
<div class="app-shell">
	{#if !isTauri && !$webSession}
		<!-- tasks-server's web page: the browser signs in with the account. -->
		<main class="app-main">
			<WebSignIn />
		</main>
	{:else if status && (!status.configured || showSetup)}
		<!-- First run (or signed out): signing in is required, and needs the network once. -->
		<main class="app-main">
			<Setup
				{status}
				ondone={(s) => {
					status = s;
					showSetup = false;
				}}
				oncancel={status.configured ? () => (showSetup = false) : undefined}
			/>
		</main>
	{:else if status && waitingForFirstSync}
		<main class="app-main">
			<FirstSync sync={status.sync} username={status.username} onretry={syncNow} onsignout={signOut} />
		</main>
	{:else if status}
		<main class="app-main">
			{@render children()}
		</main>
		{#if !$quickAdd}
			<button type="button" class="fab" onclick={() => openQuickAdd()} aria-label="Add task" title={$quickAddKeys ? `Add task (${keyLabels($quickAddKeys).join('+')})` : 'Add task'}>
				<svg aria-hidden="true" fill="none" stroke="currentColor" viewBox="0 0 24 24"><path stroke-linecap="round" stroke-width="2.25" d="M12 5v14m7-7H5" /></svg>
			</button>
		{/if}
	{/if}
</div>

{#if status && ready && settingsTab}
	<SettingsDialog
		{status}
		tab={settingsTab}
		onstatus={(s) => (status = s)}
		onsync={syncNow}
		onswitch={() => {
			settingsTab = null;
			showSetup = true;
		}}
		onclose={() => (settingsTab = null)}
	/>
{/if}

<ToastHost />
<ConfirmDialog />
<QuickAddDialog />
{/if}
