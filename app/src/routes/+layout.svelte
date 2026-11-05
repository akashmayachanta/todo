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
	import { isQuickAddShortcut, openQuickAdd, quickAdd } from '$lib/stores/taskQuickAdd';
	import { applyTheme, followSystemTheme } from '$lib/stores/theme';
	import { setPaneFooter, setPaneHeader } from '$lib/paneHeader';
	import { call, tauriTasksClient, type AppStatus, type SyncStatus } from '$shell/tauriClient';
	import Setup from '$shell/Setup.svelte';
	import FirstSync from '$shell/FirstSync.svelte';
	import AccountMenu from '$shell/AccountMenu.svelte';
	import PaneStatus from '$shell/PaneStatus.svelte';
	import SettingsDialog, { type SettingsTab } from '$shell/SettingsDialog.svelte';

	// Before any page loads: tasks come from the local DB (Rust). Reminders are
	// system notifications scheduled there too (src-tauri/src/reminders.rs).
	setTasksClient(tauriTasksClient);

	let { children } = $props();

	let status = $state<AppStatus | null>(null);
	// The sign-in screen, opened from Settings → Account (signed in already).
	let showSetup = $state(false);
	// Signed in, but the first sync hasn't brought the tasks here yet.
	let waitingForFirstSync = $derived(!!status?.configured && !status.sync.ready);
	// Settings, open on one of its sections.
	let settingsTab = $state<SettingsTab | null>(null);
	let ready = $derived(!!status && status.configured && !showSetup && !waitingForFirstSync);

	// The account menu tops the Tasks pane (lib/components/tasks/TasksPane.svelte).
	setPaneHeader(paneHeader);
	setPaneFooter(paneFooter);

	function openSettings(tab: SettingsTab = 'account') {
		settingsTab = tab;
	}

	function openTask(id: string) {
		goto(`/tasks?task=${encodeURIComponent(id)}`);
	}

	function syncNow() {
		call('app_sync_now').catch(() => {});
	}

	function forceSync() {
		call('app_force_sync').catch(() => {});
	}

	async function signOut() {
		status = await call<AppStatus>('app_sign_out');
	}

	onMount(() => {
		applyTheme();
		const stopTheme = followSystemTheme();
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
		// (Android resumes it), instead of waiting for the next retry. The page loads
		// its tasks again too: a sleeping page can miss the event for a sync's changes.
		function onVisible() {
			if (document.visibilityState !== 'visible') return;
			syncNow();
			tasksChanged();
		}
		window.addEventListener('online', syncNow);
		document.addEventListener('visibilitychange', onVisible);

		// Alt+N while the window has focus (the global shortcut covers the rest of Windows); Ctrl+, Settings.
		function onKey(e: KeyboardEvent) {
			if (isQuickAddShortcut(e)) {
				e.preventDefault();
				openQuickAdd();
			} else if (e.key === ',' && (e.ctrlKey || e.metaKey) && !e.altKey && ready) {
				e.preventDefault();
				openSettings(settingsTab ?? 'account');
			}
		}
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
		<AccountMenu {status} onsync={syncNow} onsettings={openSettings} />
	{/if}
{/snippet}

{#snippet paneFooter()}
	{#if status}
		<PaneStatus {status} onsync={syncNow} onforcesync={forceSync} />
	{/if}
{/snippet}

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
		<main class="app-main">
			{@render children()}
		</main>
		{#if !$quickAdd}
			<button type="button" class="fab" onclick={() => openQuickAdd()} aria-label="Add task" title="Add task (Alt+N)">
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
