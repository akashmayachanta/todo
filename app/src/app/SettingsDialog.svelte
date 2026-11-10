<script module lang="ts">
	export type SettingsTab = 'account' | 'server' | 'general' | 'theme';
</script>

<script lang="ts">
	import { onMount, untrack } from 'svelte';
	import { errorDetail } from '$lib/api';
	import { toast } from '$lib/stores/toast';
	import { confirmDialog } from '$lib/stores/confirm';
	import { taskSettings } from '$lib/stores/taskSettings';
	import { ACCENTS, ACCENT_SWATCH, accentStore, modePref, modeStore, setAccent, setModePref, type ModePref } from '$lib/stores/theme';
	import Select from '$lib/components/Select.svelte';
	import Avatar from './Avatar.svelte';
	import { displayNames, photos, removePhoto, setDisplayName, setPhoto } from './profile';
	import { SYNC_LABELS, ago, syncTrouble } from './syncText';
	import { call, type AppStatus, type ServerInfo } from './tauriClient';

	/**
	 * Settings: Account (photo, who is signed in, sign out), Server (the host and
	 * API path, kept apart from the account), General (how tasks are shown) and
	 * Theme. Account and server live in the app; General and Theme in this browser.
	 */
	let {
		status,
		tab = 'account',
		onstatus,
		onsync,
		onswitch,
		onclose,
	}: {
		status: AppStatus;
		tab?: SettingsTab;
		onstatus: (s: AppStatus) => void;
		onsync: () => void;
		/** Sign in as someone else (the sign-in screen). */
		onswitch: () => void;
		onclose: () => void;
	} = $props();

	const TABS: { key: SettingsTab; label: string; icon: string }[] = [
		{ key: 'account', label: 'Account', icon: 'M16 7a4 4 0 11-8 0 4 4 0 018 0zM12 14a7 7 0 00-7 7h14a7 7 0 00-7-7z' },
		{ key: 'server', label: 'Server', icon: 'M5 12h14M5 12a2 2 0 01-2-2V6a2 2 0 012-2h14a2 2 0 012 2v4a2 2 0 01-2 2M5 12a2 2 0 00-2 2v4a2 2 0 002 2h14a2 2 0 002-2v-4a2 2 0 00-2-2m-2-4h.01M17 16h.01' },
		{ key: 'general', label: 'General', icon: 'M4 6h16M4 12h10M4 18h7' },
		{ key: 'theme', label: 'Theme', icon: 'M12 3a9 9 0 100 18c.83 0 1.5-.67 1.5-1.5 0-.39-.15-.74-.39-1.01a1.5 1.5 0 011.12-2.49H16a5 5 0 005-5c0-4.42-4.03-8-9-8zM6.5 12a1.5 1.5 0 110-3 1.5 1.5 0 010 3zm3-4a1.5 1.5 0 110-3 1.5 1.5 0 010 3zm5 0a1.5 1.5 0 110-3 1.5 1.5 0 010 3zm3 4a1.5 1.5 0 110-3 1.5 1.5 0 010 3z' },
	];

	let current = $state<SettingsTab>(untrack(() => tab));
	let now = $state(Date.now());
	onMount(() => {
		const clock = setInterval(() => (now = Date.now()), 30_000);
		return () => clearInterval(clock);
	});

	// ── Account ──────────────────────────────────────────────────────────────

	let fileInput = $state<HTMLInputElement>();
	let photoBusy = $state(false);
	let hasPhoto = $derived(!!status.username && !!$photos[status.username]);
	let displayName = $state(untrack(() => (status.username ? $displayNames[status.username] ?? '' : '')));

	async function saveName() {
		if (status.username) {
			try {
				await setDisplayName(status.username, displayName);
			} catch (err) {
				toast.error(errorDetail(err));
			}
		}
		displayName = status.username ? $displayNames[status.username] ?? '' : '';
	}

	async function pickPhoto(e: Event) {
		const input = e.currentTarget as HTMLInputElement;
		const file = input.files?.[0];
		input.value = '';
		if (!file || !status.username) return;
		photoBusy = true;
		try {
			await setPhoto(status.username, file);
		} catch (err) {
			toast.error(errorDetail(err));
		} finally {
			photoBusy = false;
		}
	}

	async function signOut() {
		const pending = status.sync.pending;
		const ok = await confirmDialog(
			`Sign out of ${status.username ?? 'this account'} on this device? Your tasks stay here.` +
				(pending ? ` ${pending} change${pending === 1 ? '' : 's'} not synced yet will sync when you sign back in.` : ''),
		);
		if (!ok) return;
		onstatus(await call<AppStatus>('app_sign_out'));
		onclose();
	}

	// ── Server ───────────────────────────────────────────────────────────────

	const saved = untrack(() => status.server);
	let host = $state(saved?.url ?? '');
	let path = $state(saved?.tasks_base ?? '/tasks-api');
	let password = $state('');
	let connecting = $state(false);
	let serverError = $state('');
	const norm = (s: string) => s.trim().replace(/\/+$/, '');
	let serverChanged = $derived(norm(host) !== norm(status.server?.url ?? '') || path.trim() !== (status.server?.tasks_base ?? '').trim());

	function resetServer() {
		host = status.server?.url ?? '';
		path = status.server?.tasks_base ?? '/tasks-api';
		password = '';
		serverError = '';
	}

	async function connect(e: SubmitEvent) {
		e.preventDefault();
		serverError = '';
		connecting = true;
		try {
			const server: ServerInfo = { url: host.trim(), tasks_base: path.trim() };
			const next = await call<AppStatus>('app_sign_in', { server, username: status.username ?? '', password, register: false });
			password = '';
			onstatus(next);
			toast.success(`Connected to ${server.url}`);
		} catch (err) {
			serverError = errorDetail(err);
		} finally {
			connecting = false;
		}
	}

	// ── General ──────────────────────────────────────────────────────────────

	type Toggle = 'showSubtasks' | 'showNotes' | 'showPlace' | 'showCounts' | 'showCompleted';
	const TOGGLES: { key: Toggle; label: string; hint?: string; sub?: boolean }[] = [
		{ key: 'showSubtasks', label: 'Show subtasks', hint: 'Under their task in the Inbox and projects' },
		{ key: 'showNotes', label: 'Show descriptions', hint: 'The first line, under the title' },
		{ key: 'showPlace', label: 'Show project in Today and Upcoming' },
		{ key: 'showCounts', label: 'Show task counts', hint: 'Beside each list in the sidebar' },
		{ key: 'showCompleted', label: 'Show completed tasks', hint: 'Tasks under the open ones in the Inbox and projects, and subtasks everywhere' },
	];
	const START_VIEWS = [
		{ value: 'today', label: 'Today' },
		{ value: 'inbox', label: 'Inbox' },
		{ value: 'upcoming', label: 'Upcoming' },
	] as const;

	// ── Reminders on this phone (Android) ────────────────────────────────────

	type ReminderHealth = { notifications: boolean; exactAlarms: boolean; batteryUnrestricted: boolean };
	type HealthKey = keyof ReminderHealth;
	const HEALTH: { key: HealthKey; label: string; ok: string; off: string }[] = [
		{ key: 'notifications', label: 'Notifications', ok: 'Allowed', off: 'Off: reminders are not shown' },
		{ key: 'exactAlarms', label: 'Alarms & reminders', ok: 'Allowed: reminders fire on the minute', off: 'Off: Android may hold reminders back by minutes' },
		{ key: 'batteryUnrestricted', label: 'Battery', ok: 'Unrestricted: background sync runs', off: 'Optimized: background sync may be held back' },
	];
	let health = $state<ReminderHealth | null>(null);

	async function loadHealth() {
		if (status.platform !== 'android') return;
		health = await call<ReminderHealth | null>('app_reminder_health').catch(() => null);
	}

	async function fixHealth(kind: HealthKey) {
		try {
			await call('app_open_reminder_setting', { kind });
		} catch (e) {
			toast.error(errorDetail(e));
		}
	}

	// Coming back from Android's settings brings the page to the front again.
	onMount(() => {
		loadHealth();
		const recheck = () => document.visibilityState === 'visible' && loadHealth();
		document.addEventListener('visibilitychange', recheck);
		return () => document.removeEventListener('visibilitychange', recheck);
	});

	// ── Theme ────────────────────────────────────────────────────────────────

	const MODE_PREFS: { value: ModePref; label: string }[] = [
		{ value: 'system', label: 'System' },
		{ value: 'light', label: 'Light' },
		{ value: 'dark', label: 'Dark' },
	];

	function onKey(e: KeyboardEvent) {
		if (e.key === 'Escape' && !e.defaultPrevented) {
			e.preventDefault();
			onclose();
		}
	}

	function focusOnMount(node: HTMLElement) {
		node.focus();
	}
</script>

<svelte:window onkeydown={onKey} />

<div class="modal-backdrop" onclick={onclose} aria-hidden="true"></div>
<div class="modal-overlay">
	<div class="set-panel" role="dialog" aria-modal="true" aria-labelledby="settings-title" tabindex="-1" use:focusOnMount>
		<nav class="set-nav" aria-label="Settings sections">
			<h2 id="settings-title" class="set-nav-title">Settings</h2>
			{#each TABS as t (t.key)}
				<button type="button" class="set-nav-item" aria-current={current === t.key ? 'page' : undefined} onclick={() => (current = t.key)}>
					<svg aria-hidden="true" fill="none" stroke="currentColor" viewBox="0 0 24 24"><path stroke-linecap="round" stroke-linejoin="round" stroke-width="1.75" d={t.icon} /></svg>
					{t.label}
				</button>
			{/each}
		</nav>

		<section class="set-body">
			<header class="set-head">
				<h3 class="set-title">{TABS.find((t) => t.key === current)?.label}</h3>
				<button type="button" class="modal-close" onclick={onclose} aria-label="Close settings">
					<svg aria-hidden="true" class="w-5 h-5" fill="none" stroke="currentColor" viewBox="0 0 24 24"><path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M6 18L18 6M6 6l12 12" /></svg>
				</button>
			</header>

			<div class="set-scroll">
				{#if current === 'account'}
					<div class="set-group">
						<span class="set-label">Photo</span>
						<div class="set-photo">
							<Avatar name={status.username} size={64} />
							<div class="set-photo-actions">
								<div class="set-row-buttons">
									<button type="button" class="btn btn-secondary" disabled={photoBusy} onclick={() => fileInput?.click()}>
										{photoBusy ? 'Saving…' : hasPhoto ? 'Change photo' : 'Upload photo'}
									</button>
									{#if hasPhoto}
										<button type="button" class="btn btn-ghost set-text-btn" onclick={() => status.username && removePhoto(status.username).catch((err) => toast.error(errorDetail(err)))}>Remove</button>
									{/if}
								</div>
								<p class="set-hint">Kept on this device. A square crop of the middle of the picture.</p>
							</div>
							<input bind:this={fileInput} type="file" accept="image/*" class="sr-only" tabindex="-1" onchange={pickPhoto} />
						</div>
					</div>

					<div class="set-pair">
						<label class="set-group">
							<span class="set-label">Display name</span>
							<input
								class="input"
								bind:value={displayName}
								placeholder={status.username ?? ''}
								maxlength="60"
								autocomplete="name"
								onchange={saveName}
								onkeydown={(e) => e.key === 'Enter' && e.currentTarget.blur()}
							/>
							<span class="set-hint">Shown in the sidebar and menus, on all your devices.</span>
						</label>
						<label class="set-group">
							<span class="set-label">Username</span>
							<input class="input set-readonly" value="@{status.username ?? ''}" readonly tabindex="-1" />
							<span class="set-hint">You sign in with it. It can't be changed.</span>
						</label>
					</div>

					<div class="set-group">
						<span class="set-label">Sync</span>
						<p class="set-value">
							<span class="set-dot" data-state={status.sync.state}></span>
							{SYNC_LABELS[status.sync.state]}
							<span class="set-muted">· last synced {ago(status.sync.last_synced_at, now)}</span>
						</p>
						{#if syncTrouble(status.sync) && status.sync.message}<p class="set-hint">{status.sync.message}</p>{/if}
					</div>

					<div class="set-actions">
						<button type="button" class="btn btn-secondary" onclick={onswitch}>Sign in as someone else</button>
						<button type="button" class="btn btn-secondary set-danger" onclick={signOut}>Sign out</button>
					</div>
				{:else if current === 'server'}
					<form class="set-form" onsubmit={connect}>
						<p class="set-hint set-lede">Where this device syncs. Your tasks stay on this device and work offline either way.</p>
						<label class="set-group">
							<span class="set-label">Host</span>
							<input class="input" type="url" required placeholder="https://your-domain.ngrok.app" bind:value={host} autocomplete="url" spellcheck="false" />
						</label>
						<label class="set-group">
							<span class="set-label">API path</span>
							<input class="input set-mono" bind:value={path} placeholder="/tasks-api" spellcheck="false" />
							<span class="set-hint">The server's TASKS_API_BASE_PATH. Leave it empty if the API is at the root.</span>
						</label>

						{#if serverChanged}
							<div class="set-callout">
								<label class="set-group">
									<span class="set-label">Password for {status.username}</span>
									<input class="input" type="password" required bind:value={password} autocomplete="current-password" />
								</label>
								<p class="set-hint">
									Changing the server signs this device in again. If the account there is a different one, the tasks on
									this device are replaced by that account's.
								</p>
								{#if serverError}<p class="set-error" role="alert">{serverError}</p>{/if}
								<div class="set-row-buttons set-row-end">
									<button type="button" class="btn btn-secondary" onclick={resetServer}>Cancel</button>
									<button type="submit" class="btn btn-primary" disabled={connecting}>{connecting ? 'Connecting…' : 'Connect'}</button>
								</div>
							</div>
						{:else}
							<div class="set-group">
								<span class="set-label">Status</span>
								<p class="set-value">
									<span class="set-dot" data-state={status.sync.state}></span>
									{SYNC_LABELS[status.sync.state]}
									<span class="set-muted">· last synced {ago(status.sync.last_synced_at, now)}</span>
								</p>
								{#if status.sync.pending}<p class="set-hint">{status.sync.pending} change{status.sync.pending === 1 ? '' : 's'} waiting to sync</p>{/if}
							</div>
							<div class="set-actions">
								<button type="button" class="btn btn-secondary" disabled={status.sync.state === 'syncing'} onclick={onsync}>
									{status.sync.state === 'syncing' ? 'Syncing…' : 'Sync now'}
								</button>
							</div>
						{/if}
					</form>
				{:else if current === 'general'}
					<div class="set-group">
						<label class="set-label" for="settings-start-view">Open on</label>
						<div class="set-select">
							<Select
								id="settings-start-view"
								value={$taskSettings.startView}
								options={[...START_VIEWS]}
								onchange={(v) => taskSettings.update('startView', v as 'today' | 'inbox' | 'upcoming')}
							/>
						</div>
					</div>
					<div class="set-group">
						<span class="set-label">Show</span>
						<div class="set-toggles">
							{#each TOGGLES as t (t.key)}
								{@const disabled = t.sub && !$taskSettings.showSubtasks}
								<label class="set-toggle" class:set-toggle-off={disabled}>
									<span class="set-toggle-text">
										<span class="set-toggle-label">
											{#if t.sub}<svg aria-hidden="true" class="set-toggle-sub" fill="none" stroke="currentColor" viewBox="0 0 24 24"><path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M6 4v8a4 4 0 004 4h8m0 0l-3-3m3 3l-3 3" /></svg>{/if}
											{t.label}
										</span>
										{#if t.hint}<span class="set-hint">{t.hint}</span>{/if}
									</span>
									<input
										type="checkbox"
										role="switch"
										class="set-switch"
										checked={$taskSettings[t.key]}
										{disabled}
										onchange={(e) => taskSettings.update(t.key, e.currentTarget.checked)}
									/>
								</label>
							{/each}
						</div>
					</div>
					{#if health}
						<div class="set-group">
							<span class="set-label">Reminders on this phone</span>
							<div class="set-toggles">
								{#each HEALTH as h (h.key)}
									<div class="set-toggle">
										<span class="set-toggle-text">
											<span class="set-toggle-label">
												<span class="set-dot" data-state={health[h.key] ? 'synced' : 'error'}></span>
												{h.label}
											</span>
											<span class="set-hint">{health[h.key] ? h.ok : h.off}</span>
										</span>
										{#if !health[h.key]}
											<button type="button" class="btn btn-secondary" onclick={() => fixHealth(h.key)}>Allow</button>
										{/if}
									</div>
								{/each}
							</div>
							<span class="set-hint">Some phones also stop apps in the background on their own. If reminders still come late, look for the app in the phone's autostart or background app settings.</span>
						</div>
					{/if}
				{:else}
					<div class="set-group">
						<span class="set-label">Mode</span>
						<div class="set-segment" role="radiogroup" aria-label="Mode">
							{#each MODE_PREFS as m (m.value)}
								<button type="button" role="radio" aria-checked={$modePref === m.value} onclick={() => setModePref(m.value)}>{m.label}</button>
							{/each}
						</div>
					</div>
					<div class="set-group">
						<span class="set-label">Accent</span>
						<div class="set-accents" role="radiogroup" aria-label="Accent">
							{#each ACCENTS as a (a.value)}
								<button type="button" role="radio" class="set-accent" aria-checked={$accentStore === a.value} onclick={() => setAccent(a.value)}>
									<span class="set-accent-dot" style="background-color: {ACCENT_SWATCH[$modeStore][a.value]}"></span>
									{a.label}
								</button>
							{/each}
						</div>
					</div>
				{/if}
			</div>
		</section>
	</div>
</div>

<style>
	.set-panel {
		pointer-events: auto;
		display: flex;
		width: min(46rem, 100%);
		height: min(34rem, calc(100dvh - 2rem));
		overflow: hidden;
		border-radius: 14px;
		background: var(--bg-card);
		border: 1px solid var(--clr-border-light);
		box-shadow: var(--shadow-xl);
		outline: none;
	}
	.set-nav {
		display: flex;
		flex-direction: column;
		gap: 2px;
		width: 12rem;
		flex-shrink: 0;
		padding: 1rem 0.5rem;
		background: var(--bg-surface);
		border-right: 1px solid var(--clr-border);
	}
	.set-nav-title {
		margin: 0 0 0.6rem;
		padding: 0 0.6rem;
		font-family: var(--font-display);
		font-size: var(--text-lg);
		font-weight: 700;
		color: var(--text-primary);
	}
	.set-nav-item {
		display: flex;
		align-items: center;
		gap: 0.6rem;
		padding: 0.45rem 0.6rem;
		border-radius: 8px;
		font-size: var(--text-sm);
		color: var(--text-secondary);
		text-align: left;
		transition: background-color var(--dur-micro) var(--ease-out), color var(--dur-micro) var(--ease-out);
	}
	.set-nav-item svg {
		width: 1.05rem;
		height: 1.05rem;
		flex-shrink: 0;
	}
	.set-nav-item:hover {
		background: var(--bg-hover);
		color: var(--text-primary);
	}
	.set-nav-item[aria-current='page'] {
		background: var(--color-brand-glow);
		color: var(--color-brand);
		font-weight: 500;
	}
	.set-body {
		display: flex;
		flex-direction: column;
		flex: 1;
		min-width: 0;
	}
	.set-head {
		display: flex;
		align-items: center;
		justify-content: space-between;
		padding: 1rem 1.5rem 0.75rem;
		border-bottom: 1px solid var(--clr-border);
	}
	.set-title {
		margin: 0;
		font-family: var(--font-display);
		font-size: var(--text-md);
		font-weight: 600;
		color: var(--text-primary);
	}
	.set-scroll {
		flex: 1;
		overflow-y: auto;
		padding: 1.25rem 1.5rem 1.5rem;
		display: grid;
		align-content: start;
		gap: 1.35rem;
	}
	.set-form {
		display: grid;
		gap: 1.1rem;
	}
	.set-group {
		display: grid;
		gap: 0.35rem;
	}
	.set-label {
		font-family: var(--font-prose);
		font-size: var(--text-label);
		font-weight: 600;
		color: var(--text-primary);
	}
	.set-value {
		margin: 0;
		display: flex;
		align-items: center;
		flex-wrap: wrap;
		gap: 0.4rem;
		font-size: var(--text-sm);
		color: var(--text-secondary);
	}
	.set-muted {
		color: var(--text-muted);
	}
	.set-hint {
		margin: 0;
		font-size: var(--text-xs);
		color: var(--text-muted);
	}
	.set-lede {
		font-size: var(--text-sm);
	}
	.set-mono {
		font-family: var(--font-outlier);
	}
	.set-error {
		margin: 0;
		font-size: var(--text-sm);
		color: var(--color-danger);
	}
	.set-pair {
		display: grid;
		grid-template-columns: 1fr 1fr;
		align-items: start;
		gap: 1rem;
	}
	.set-readonly {
		background: var(--bg-surface);
		border-style: dashed;
		color: var(--text-secondary);
		cursor: default;
	}
	.set-photo {
		display: flex;
		align-items: center;
		gap: 1rem;
	}
	.set-photo-actions {
		display: grid;
		gap: 0.4rem;
	}
	.set-row-buttons {
		display: flex;
		flex-wrap: wrap;
		gap: 0.5rem;
	}
	.set-row-end {
		justify-content: flex-end;
	}
	.set-text-btn {
		padding-inline: 0.75rem;
		font-size: var(--text-sm);
	}
	.set-actions {
		display: flex;
		flex-wrap: wrap;
		gap: 0.5rem;
		padding-top: 1rem;
		border-top: 1px solid var(--clr-border);
	}
	.set-danger {
		color: var(--color-danger);
	}
	.set-dot {
		width: 0.5rem;
		height: 0.5rem;
		border-radius: 999px;
		corner-shape: round;
		background: var(--text-muted);
	}
	.set-dot[data-state='synced'] {
		background: var(--color-success);
	}
	.set-dot[data-state='syncing'] {
		background: var(--color-info);
	}
	.set-dot[data-state='offline'],
	.set-dot[data-state='server_down'] {
		background: var(--color-warning);
	}
	.set-dot[data-state='error'] {
		background: var(--color-danger);
	}
	.set-callout {
		display: grid;
		gap: 0.75rem;
		padding: 1rem;
		border-radius: 10px;
		background: var(--bg-surface);
		border: 1px solid var(--clr-border);
	}
	.set-select {
		max-width: 14rem;
	}

	/* General: rows with a switch on the right */
	.set-toggles {
		display: grid;
		border: 1px solid var(--clr-border);
		border-radius: 10px;
	}
	.set-toggle {
		display: flex;
		align-items: center;
		justify-content: space-between;
		gap: 1rem;
		min-height: 3.25rem;
		padding: 0.625rem 0.875rem;
		font-size: var(--text-sm);
		color: var(--text-primary);
		cursor: pointer;
	}
	.set-toggle + .set-toggle {
		border-top: 1px solid var(--clr-border);
	}
	.set-toggle-off {
		cursor: default;
	}
	.set-toggle-off .set-toggle-text {
		opacity: 0.5;
	}
	.set-toggle-text {
		display: grid;
		gap: 0.125rem;
		min-width: 0;
	}
	.set-toggle-label {
		display: flex;
		align-items: center;
		gap: 0.375rem;
	}
	/* A sub-option: an elbow arrow, the text kept in line with the rest. */
	.set-toggle-sub {
		width: 0.875rem;
		height: 0.875rem;
		flex-shrink: 0;
		color: var(--text-faint);
	}
	.set-switch {
		appearance: none;
		position: relative;
		flex-shrink: 0;
		box-sizing: border-box;
		width: 36px;
		height: 20px;
		margin: 0;
		padding: 0;
		border: 0;
		border-radius: 999px;
		corner-shape: round;
		background: var(--clr-border-light);
		cursor: inherit;
		transition: background-color var(--dur-short) var(--ease-out);
	}
	.set-switch::after {
		content: '';
		position: absolute;
		top: 2px;
		left: 2px;
		width: 16px;
		height: 16px;
		border-radius: 999px;
		corner-shape: round;
		background: #fff;
		box-shadow: 0 1px 2px rgb(0 0 0 / 0.25);
		transition: transform var(--dur-short) var(--ease-out);
	}
	.set-switch:checked {
		background: var(--color-brand);
	}
	.set-switch:checked::after {
		transform: translateX(16px);
	}
	.set-switch:disabled {
		opacity: 0.45;
	}
	/* A soft ring instead of the app's outline, so a switch keeps its shape. */
	.set-switch:focus-visible {
		outline: none;
		box-shadow: 0 0 0 3px var(--color-brand-glow);
	}

	/* Theme */
	.set-segment {
		display: inline-flex;
		justify-self: start;
		padding: 3px;
		border-radius: 10px;
		background: var(--bg-surface);
		border: 1px solid var(--clr-border);
	}
	.set-segment button {
		padding: 0.35rem 1rem;
		border-radius: 7px;
		font-size: var(--text-sm);
		color: var(--text-secondary);
	}
	.set-segment button[aria-checked='true'] {
		background: var(--bg-card);
		color: var(--text-primary);
		font-weight: 500;
		box-shadow: var(--shadow-sm);
	}
	.set-accents {
		display: flex;
		flex-wrap: wrap;
		gap: 0.5rem;
	}
	.set-accent {
		display: inline-flex;
		align-items: center;
		gap: 0.5rem;
		padding: 0.45rem 0.85rem 0.45rem 0.55rem;
		border-radius: 10px;
		border: 1px solid var(--clr-border);
		font-size: var(--text-sm);
		color: var(--text-secondary);
	}
	.set-accent[aria-checked='true'] {
		border-color: var(--color-brand);
		color: var(--text-primary);
		box-shadow: 0 0 0 1px var(--color-brand);
	}
	.set-accent-dot {
		width: 1rem;
		height: 1rem;
		border-radius: 999px;
		corner-shape: round;
	}

	/* Phones: the sections become tabs across the top. The panel is as tall as the
	   section in it, centred, and scrolls only when that is taller than the screen. */
	@media (max-width: 720px) {
		.set-panel {
			flex-direction: column;
			height: auto;
			max-height: calc(100dvh - 2rem - env(safe-area-inset-top) - env(safe-area-inset-bottom));
		}
		.set-scroll {
			flex: 0 1 auto;
			min-height: 0;
		}
		.set-nav {
			flex-direction: row;
			width: auto;
			overflow-x: auto;
			padding: 0.5rem;
			border-right: 0;
			border-bottom: 1px solid var(--clr-border);
		}
		.set-nav-title {
			display: none;
		}
		.set-pair {
			grid-template-columns: 1fr;
		}
		.set-nav-item {
			white-space: nowrap;
		}
		.set-head {
			padding-inline: 1rem;
		}
		.set-scroll {
			padding-inline: 1rem;
		}
	}
</style>
