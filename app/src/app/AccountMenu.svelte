<script lang="ts">
	import { onMount, tick } from 'svelte';
	import { placePanel, placementStyle, type Placement } from '$lib/dropdown';
	import Avatar from './Avatar.svelte';
	import { displayNames, nameFor } from './profile';
	import { SYNC_LABELS, ago, hostOf, syncMeta, syncTrouble } from './syncText';
	import type { AppStatus } from './tauriClient';
	import type { SettingsTab } from './SettingsDialog.svelte';

	/**
	 * The top of the Tasks pane: the account (photo, name, ▾) with a Settings
	 * gear beside it. The account opens a menu: who is signed in and where,
	 * Settings, and Sync with when it last synced.
	 */
	let {
		status,
		onsync,
		onsettings,
	}: { status: AppStatus; onsync: () => void; onsettings: (tab?: SettingsTab) => void } = $props();

	let open = $state(false);
	let place = $state<Placement | null>(null);
	let trigger: HTMLButtonElement;
	let panel = $state<HTMLElement>();
	let now = $state(Date.now());

	let sync = $derived(status.sync);
	let trouble = $derived(syncTrouble(sync));
	let host = $derived(hostOf(status.server));
	let shown = $derived(nameFor($displayNames, status.username));
	let hasName = $derived(!!status.username && shown !== status.username);

	onMount(() => {
		// Keeps "12 min ago" right while the menu is open.
		const clock = setInterval(() => (now = Date.now()), 30_000);
		return () => clearInterval(clock);
	});

	async function toggle() {
		if (open) return close();
		place = placePanel(trigger, 360);
		now = Date.now();
		open = true;
		await tick();
		items()[0]?.focus();
	}

	function close(refocus = false) {
		open = false;
		if (refocus) trigger?.focus();
	}

	function items(): HTMLElement[] {
		return panel ? [...panel.querySelectorAll<HTMLElement>('[role="menuitem"]:not(:disabled)')] : [];
	}

	function onPanelKey(e: KeyboardEvent) {
		const list = items();
		const i = list.indexOf(document.activeElement as HTMLElement);
		if (e.key === 'Escape') {
			e.preventDefault();
			e.stopPropagation();
			close(true);
		} else if (e.key === 'ArrowDown' || e.key === 'ArrowUp') {
			e.preventDefault();
			const step = e.key === 'ArrowDown' ? 1 : -1;
			list[(i + step + list.length) % list.length]?.focus();
		} else if (e.key === 'Tab') {
			close();
		}
	}

	function onWindowPointer(e: PointerEvent) {
		if (!open) return;
		const t = e.target as Node;
		if (!panel?.contains(t) && !trigger.contains(t)) close();
	}

	function pick(fn: () => void) {
		close();
		fn();
	}

	let syncTitle = $derived(
		[
			sync.message,
			`Last synced: ${ago(sync.last_synced_at, now)}`,
			sync.pending ? `${sync.pending} change${sync.pending === 1 ? '' : 's'} waiting to sync` : '',
		]
			.filter(Boolean)
			.join('\n'),
	);
</script>

<svelte:window onpointerdown={onWindowPointer} onresize={() => close()} />

<div class="acct">
	<button
		bind:this={trigger}
		type="button"
		class="acct-trigger"
		aria-haspopup="menu"
		aria-expanded={open}
		title={trouble ? SYNC_LABELS[sync.state] : undefined}
		onclick={toggle}
	>
		<span class="acct-photo">
			<Avatar name={status.username} size={26} />
			{#if trouble || sync.state === 'syncing'}
				<span class="acct-badge" data-state={sync.state} aria-hidden="true"></span>
			{/if}
		</span>
		<span class="acct-name">{shown}</span>
		<svg aria-hidden="true" class="acct-caret" fill="none" stroke="currentColor" viewBox="0 0 24 24"><path stroke-linecap="round" stroke-linejoin="round" stroke-width="2.25" d="M19 9l-7 7-7-7" /></svg>
	</button>
	<button type="button" class="acct-icon" onclick={() => onsettings()} aria-label="Settings" title="Settings (Ctrl+,)">
		<svg aria-hidden="true" fill="none" stroke="currentColor" viewBox="0 0 24 24"><path stroke-linecap="round" stroke-linejoin="round" stroke-width="1.75" d="M10.325 4.317c.426-1.756 2.924-1.756 3.35 0a1.724 1.724 0 002.573 1.066c1.543-.94 3.31.826 2.37 2.37a1.724 1.724 0 001.065 2.572c1.756.426 1.756 2.924 0 3.35a1.724 1.724 0 00-1.066 2.573c.94 1.543-.826 3.31-2.37 2.37a1.724 1.724 0 00-2.572 1.065c-.426 1.756-2.924 1.756-3.35 0a1.724 1.724 0 00-2.573-1.066c-1.543.94-3.31-.826-2.37-2.37a1.724 1.724 0 00-1.065-2.572c-1.756-.426-1.756-2.924 0-3.35a1.724 1.724 0 001.066-2.573c-.94-1.543.826-3.31 2.37-2.37.996.608 2.296.07 2.572-1.065z M15 12a3 3 0 11-6 0 3 3 0 016 0z" /></svg>
	</button>
</div>

{#if open && place}
	<!-- svelte-ignore a11y_interactive_supports_focus -->
	<div
		bind:this={panel}
		class="dd-panel acct-panel"
		style={placementStyle({ ...place, minWidth: Math.max(place.minWidth, 272) })}
		role="menu"
		aria-label="Account"
		tabindex="-1"
		onkeydown={onPanelKey}
	>
		<button type="button" role="menuitem" class="acct-head" onclick={() => pick(() => onsettings('account'))}>
			<Avatar name={status.username} size={36} />
			<span class="acct-head-text">
				<span class="acct-head-name">{shown}</span>
				<span class="acct-head-sub">{[hasName ? `@${status.username}` : '', host].filter(Boolean).join(' · ')}</span>
			</span>
		</button>

		<div class="acct-rule" role="separator"></div>

		<button type="button" role="menuitem" class="dd-item acct-item" onclick={() => pick(() => onsettings())}>
			<svg aria-hidden="true" fill="none" stroke="currentColor" viewBox="0 0 24 24"><path stroke-linecap="round" stroke-linejoin="round" stroke-width="1.75" d="M10.325 4.317c.426-1.756 2.924-1.756 3.35 0a1.724 1.724 0 002.573 1.066c1.543-.94 3.31.826 2.37 2.37a1.724 1.724 0 001.065 2.572c1.756.426 1.756 2.924 0 3.35a1.724 1.724 0 00-1.066 2.573c.94 1.543-.826 3.31-2.37 2.37a1.724 1.724 0 00-2.572 1.065c-.426 1.756-2.924 1.756-3.35 0a1.724 1.724 0 00-2.573-1.066c-1.543.94-3.31-.826-2.37-2.37a1.724 1.724 0 00-1.065-2.572c-1.756-.426-1.756-2.924 0-3.35a1.724 1.724 0 001.066-2.573c-.94-1.543.826-3.31 2.37-2.37.996.608 2.296.07 2.572-1.065z M15 12a3 3 0 11-6 0 3 3 0 016 0z" /></svg>
			<span class="acct-item-label">Settings</span>
			<kbd class="acct-kbd">Ctrl ,</kbd>
		</button>

		<div class="acct-rule" role="separator"></div>

		<button
			type="button"
			role="menuitem"
			class="dd-item acct-item"
			title={syncTitle}
			disabled={sync.state === 'syncing'}
			onclick={() => {
				onsync();
				now = Date.now();
			}}
		>
			<svg aria-hidden="true" class:acct-spin={sync.state === 'syncing'} fill="none" stroke="currentColor" viewBox="0 0 24 24"><path stroke-linecap="round" stroke-linejoin="round" stroke-width="1.75" d="M4 4v5h.582m15.356 2A8.001 8.001 0 004.582 9m0 0H9m11 11v-5h-.581m0 0a8.003 8.003 0 01-15.357-2m15.357 2H15" /></svg>
			<span class="acct-item-label">Sync</span>
			<span class="acct-meta" data-state={sync.state}>
				{#if trouble}<span class="acct-dot" aria-hidden="true"></span>{/if}
				{syncMeta(sync, now)}
			</span>
		</button>
		{#if trouble && sync.message}
			<p class="acct-note">{sync.message}</p>
		{/if}
		{#if sync.pending && sync.state !== 'synced'}
			<p class="acct-note">{sync.pending} change{sync.pending === 1 ? '' : 's'} waiting to sync</p>
		{/if}

	</div>
{/if}

<style>
	.acct {
		display: flex;
		align-items: center;
		gap: 0.25rem;
	}
	.acct-trigger {
		display: flex;
		align-items: center;
		gap: 0.5rem;
		min-width: 0;
		padding: 0.3rem 0.45rem 0.3rem 0.3rem;
		border-radius: 8px;
		font: inherit;
		font-size: var(--text-sm);
		font-weight: 600;
		color: var(--text-primary);
		transition: background-color var(--dur-micro) var(--ease-out);
	}
	.acct-trigger:hover,
	.acct-trigger[aria-expanded='true'] {
		background-color: var(--bg-hover);
	}
	.acct-photo {
		position: relative;
		display: inline-flex;
	}
	.acct-badge {
		position: absolute;
		right: -1px;
		bottom: -1px;
		width: 0.6rem;
		height: 0.6rem;
		border-radius: 999px;
		corner-shape: round;
		border: 2px solid var(--bg-base);
		background: var(--color-warning);
	}
	.acct-badge[data-state='error'] {
		background: var(--color-danger);
	}
	.acct-badge[data-state='syncing'] {
		background: var(--color-info);
	}
	.acct-name {
		min-width: 0;
		overflow: hidden;
		text-overflow: ellipsis;
		white-space: nowrap;
	}
	.acct-caret {
		width: 0.8rem;
		height: 0.8rem;
		flex-shrink: 0;
		color: var(--text-muted);
		transition: transform var(--dur-micro) var(--ease-out);
	}
	.acct-trigger[aria-expanded='true'] .acct-caret {
		transform: rotate(180deg);
	}
	.acct-icon {
		display: inline-grid;
		place-items: center;
		width: 2rem;
		height: 2rem;
		margin-left: auto;
		flex-shrink: 0;
		border-radius: 8px;
		color: var(--text-muted);
		transition: color var(--dur-micro) var(--ease-out), background-color var(--dur-micro) var(--ease-out);
	}
	.acct-icon:hover {
		color: var(--text-primary);
		background-color: var(--bg-hover);
	}
	.acct-icon svg {
		width: 1.15rem;
		height: 1.15rem;
	}

	.acct-panel {
		border-radius: 12px;
		padding: 0.35rem;
	}
	.acct-head {
		display: flex;
		align-items: center;
		gap: 0.65rem;
		width: 100%;
		padding: 0.5rem;
		border-radius: 8px;
		text-align: left;
	}
	.acct-head:hover,
	.acct-head:focus-visible {
		outline: none;
		background-color: var(--bg-raised);
	}
	.acct-head-text {
		display: grid;
		min-width: 0;
	}
	.acct-head-name {
		font-weight: 600;
		color: var(--text-primary);
		overflow: hidden;
		text-overflow: ellipsis;
		white-space: nowrap;
	}
	.acct-head-sub {
		font-size: var(--text-xs);
		color: var(--text-muted);
		overflow: hidden;
		text-overflow: ellipsis;
		white-space: nowrap;
	}
	.acct-rule {
		height: 1px;
		margin: 0.3rem 0.25rem;
		background: var(--clr-border);
	}
	.acct-item {
		gap: 0.65rem;
		padding-block: 0.45rem;
	}
	.acct-item svg {
		width: 1.05rem;
		height: 1.05rem;
		flex-shrink: 0;
		color: var(--text-secondary);
	}
	.acct-item:disabled {
		opacity: 1;
		cursor: default;
	}
	.acct-item-label {
		flex: 1;
	}
	.acct-kbd,
	.acct-meta {
		font-size: var(--text-xs);
		color: var(--text-muted);
		white-space: nowrap;
	}
	.acct-kbd {
		font-family: var(--font-outlier);
	}
	.acct-meta {
		display: inline-flex;
		align-items: center;
		gap: 0.35rem;
	}
	.acct-meta[data-state='offline'],
	.acct-meta[data-state='server_down'] {
		color: var(--color-warning);
	}
	.acct-meta[data-state='error'] {
		color: var(--color-danger);
	}
	.acct-dot {
		width: 0.45rem;
		height: 0.45rem;
		border-radius: 999px;
		corner-shape: round;
		background: currentColor;
	}
	.acct-spin {
		animation: acct-spin 900ms linear infinite;
	}
	@keyframes acct-spin {
		to {
			transform: rotate(-360deg);
		}
	}
	.acct-note {
		margin: 0 0.6rem 0.35rem 2.35rem;
		font-size: var(--text-xs);
		color: var(--text-muted);
		max-width: 16rem;
	}

</style>
