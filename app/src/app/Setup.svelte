<script lang="ts">
	import { untrack } from 'svelte';
	import { errorDetail } from '$lib/api';
	import { call, type AppStatus, type ServerInfo } from './tauriClient';

	/**
	 * First run (and "Account"): where the server is and who you are. Sign in
	 * with your username and password, or register a new account. Either way
	 * this device gets its own token from the tasks server, kept in the Windows
	 * Credential Manager (Android: the app's private storage).
	 */
	let {
		status,
		ondone,
		oncancel,
	}: { status: AppStatus; ondone: (s: AppStatus) => void; oncancel?: () => void } = $props();

	// The form starts from what is saved; editing it doesn't change the status passed in.
	const saved = untrack(() => status);
	let url = $state(saved.server?.url ?? '');
	let tasksBase = $state(saved.server?.tasks_base ?? '/tasks-api');
	let username = $state(saved.username ?? '');
	let password = $state('');
	let confirm = $state('');
	let registering = $state(false);
	let advanced = $state(false);
	let busy = $state(false);
	let error = $state('');

	function server(): ServerInfo {
		return { url: url.trim(), tasks_base: tasksBase.trim() };
	}

	async function submit(e: SubmitEvent) {
		e.preventDefault();
		error = '';
		if (registering && password !== confirm) {
			error = "The passwords don't match";
			return;
		}
		busy = true;
		try {
			const next = await call<AppStatus>('app_sign_in', { server: server(), username, password, register: registering });
			password = '';
			confirm = '';
			ondone(next);
		} catch (err) {
			error = errorDetail(err);
		} finally {
			busy = false;
		}
	}

	async function signOut() {
		ondone(await call<AppStatus>('app_sign_out'));
	}
</script>

<div class="setup">
	<form class="setup-sheet" onsubmit={submit}>
		<h1 class="setup-title">{registering ? 'Create an account' : 'Tasks'}</h1>
		<p class="setup-lede">
			Your tasks live on this device and work offline. Sign in to sync them with your Tasks server.
		</p>

		<label class="setup-field">
			<span class="label">Server address</span>
			<input class="input" type="url" required placeholder="https://your-domain.ngrok.app" bind:value={url} autocomplete="url" />
		</label>
		<label class="setup-field">
			<span class="label">Username</span>
			<input class="input" required bind:value={username} autocomplete="username" autocapitalize="none" spellcheck="false" />
		</label>
		<label class="setup-field">
			<span class="label">Password</span>
			<input class="input"
				type="password"
				required
				minlength={registering ? 8 : undefined}
				bind:value={password}
				autocomplete={registering ? 'new-password' : 'current-password'}
			/>
		</label>
		{#if registering}
			<label class="setup-field">
				<span class="label">Password again</span>
				<input class="input" type="password" required bind:value={confirm} autocomplete="new-password" />
			</label>
		{/if}

		<button type="button" class="setup-link" onclick={() => ((registering = !registering), (error = ''))}>
			{registering ? 'Have an account? Sign in' : 'New here? Register'}
		</button>

		{#if advanced}
			<label class="setup-field">
				<span class="label">Tasks API path</span>
				<input class="input" bind:value={tasksBase} placeholder="/tasks-api" />
			</label>
		{:else}
			<button type="button" class="setup-link" onclick={() => (advanced = true)}>Path…</button>
		{/if}

		{#if error}<p class="setup-error" role="alert">{error}</p>{/if}

		<div class="setup-actions">
			{#if oncancel}<button type="button" class="btn btn-secondary" onclick={oncancel}>Cancel</button>{/if}
			{#if status.configured}<button type="button" class="btn btn-secondary" onclick={signOut}>Sign out</button>{/if}
			<button type="submit" class="btn btn-primary" disabled={busy}>
				{#if registering}{busy ? 'Registering…' : 'Register'}{:else}{busy ? 'Signing in…' : 'Sign in'}{/if}
			</button>
		</div>
		{#if !status.configured}
			<p class="setup-note">The first sign-in needs the internet, to download your tasks. After that, Tasks works offline.</p>
		{/if}
	</form>
</div>

<style>
	.setup {
		min-height: 100%;
		display: grid;
		place-items: center;
		padding: 1.5rem 1rem;
	}
	.setup-sheet {
		width: min(28rem, 100%);
		display: grid;
		gap: 0.9rem;
	}
	.setup-title {
		font-family: var(--font-display);
		font-size: 1.6rem;
		margin: 0;
		color: var(--text-primary);
	}
	.setup-lede,
	.setup-note {
		margin: 0;
		font-family: var(--font-prose);
		color: var(--text-secondary);
	}
	.setup-note {
		font-size: var(--text-label);
		color: var(--text-muted);
	}
	.setup-field {
		display: grid;
	}
	.setup-link {
		justify-self: start;
		background: none;
		border: 0;
		padding: 0;
		font: inherit;
		font-size: var(--text-label);
		color: var(--color-brand);
		text-decoration: underline;
		text-underline-offset: 0.2em;
		cursor: pointer;
	}
	.setup-link:hover {
		color: var(--text-primary);
	}
	.setup-error {
		margin: 0;
		color: var(--color-danger);
	}
	.setup-actions {
		display: flex;
		gap: 0.5rem;
		justify-content: flex-end;
		flex-wrap: wrap;
	}
</style>
