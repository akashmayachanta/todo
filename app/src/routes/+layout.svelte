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
	import { isQuickAddShortcut, openQuickAdd } from '$lib/stores/taskQuickAdd';
	import { modeStore, themeStore } from '$lib/stores/theme';
	import { call, tauriTasksClient, type AppStatus, type SyncStatus } from '$shell/tauriClient';
	import Setup from '$shell/Setup.svelte';
	import FirstSync from '$shell/FirstSync.svelte';
	import SyncChip from '$shell/SyncChip.svelte';

	// Before any page loads: tasks come from the local DB (Rust). Reminders are
	// system notifications scheduled there too (src-tauri/src/reminders.rs).
	setTasksClient(tauriTasksClient);

	let { children } = $props();

	let status = $state<AppStatus | null>(null);
	// The Account screen, opened from the app bar (signed in already).
	let showSetup = $state(false);
	// Signed in, but the first sync hasn't brought the tasks here yet.
	let waitingForFirstSync = $derived(!!status?.configured && !status.sync.ready);
	// Until a mode is picked here, the theme follows the system (see app.html).
	let modeChosen = $state(hasSavedMode());

	function hasSavedMode(): boolean {
		try {
			return localStorage.getItem('theme-mode') !== null;
		} catch {
			return false;
		}
	}

	$effect(() => {
		if (modeChosen) document.documentElement.dataset.theme = $themeStore;
	});

	function toggleMode() {
		const dark = document.documentElement.dataset.theme?.startsWith('dark');
		modeChosen = true;
		modeStore.set(dark ? 'light' : 'dark');
	}

	function openTask(id: string) {
		goto(`/tasks?task=${encodeURIComponent(id)}`);
	}

	function syncNow() {
		call('app_sync_now').catch(() => {});
	}

	async function signOut() {
		status = await call<AppStatus>('app_sign_out');
	}

	onMount(() => {
		call<AppStatus>('app_status').then((s) => (status = s));
		call<string | null>('app_take_open_task').then((id) => id && openTask(id));
		call('app_ask_notification_permission').catch(() => {});

		const stops = [
			listen<SyncStatus>('tasks://sync', (e) => {
				if (status) status = { ...status, sync: e.payload, configured: e.payload.state !== 'signed_out' && status.configured };
			}),
			listen('tasks://changed', () => tasksChanged()),
			listen('tasks://quick-add', () => openQuickAdd()),
			listen<string>('tasks://open-task', (e) => openTask(e.payload)),
		];

		// Sync as soon as the network comes back, or the app comes back to the front
		// (Android resumes it), instead of waiting for the next retry.
		function onVisible() {
			if (document.visibilityState === 'visible') syncNow();
		}
		window.addEventListener('online', syncNow);
		document.addEventListener('visibilitychange', onVisible);

		// Alt+N while the window has focus (the global shortcut covers the rest of Windows).
		function onKey(e: KeyboardEvent) {
			if (isQuickAddShortcut(e)) {
				e.preventDefault();
				openQuickAdd();
			}
		}
		document.addEventListener('keydown', onKey);
		return () => {
			window.removeEventListener('online', syncNow);
			document.removeEventListener('visibilitychange', onVisible);
			document.removeEventListener('keydown', onKey);
			stops.forEach((p) => p.then((stop) => stop()));
		};
	});
</script>

<div class="app-shell">
	{#if status && (!status.configured || showSetup)}
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
		<header class="app-bar">
			<span class="app-bar-title">Tasks</span>
			{#if status}
				<SyncChip sync={status.sync} onsync={syncNow} onaccount={() => (showSetup = true)} />
				<button type="button" class="app-bar-button" onclick={() => (showSetup = true)}>Account</button>
			{/if}
			<button type="button" class="app-bar-button" onclick={toggleMode} aria-label="Switch light or dark mode">◐</button>
		</header>
		<main class="app-main">
			{@render children()}
		</main>
	{/if}
</div>

<ToastHost />
<ConfirmDialog />
<QuickAddDialog />
