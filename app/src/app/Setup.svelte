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
	// A saved server shows as a summary until Change is pressed.
	let editServer = $state(!saved.server?.url);
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

</script>

<div class="setup">
	<form class="setup-sheet" onsubmit={submit}>
		<header class="setup-intro">
			<h1 class="setup-title">{registering ? 'Create an account' : status.configured ? 'Sign in' : 'Tasks'}</h1>
			<p class="setup-lede">
				Your tasks live on this device and work offline. Sign in to sync them with your Tasks server.
			</p>
		</header>

		<!-- Where the server is, kept apart from who you are. -->
		<section class="setup-card" aria-labelledby="setup-server-title">
			<div class="setup-card-head">
				<h2 id="setup-server-title" class="setup-card-title">Server</h2>
				{#if !editServer}
					<button type="button" class="setup-link" onclick={() => (editServer = true)}>Change</button>
				{/if}
			</div>
			{#if editServer}
				<label class="setup-field">
					<span class="label">Host</span>
					<input class="input" type="url" required placeholder="https://your-domain.ngrok.app" bind:value={url} autocomplete="url" spellcheck="false" />
				</label>
				<label class="setup-field">
					<span class="label">API path</span>
					<input class="input setup-mono" bind:value={tasksBase} placeholder="/tasks-api" spellcheck="false" />
					<span class="setup-hint">The server's TASKS_API_BASE_PATH. Empty if the API is at the root.</span>
				</label>
			{:else}
				<dl class="setup-summary">
					<dt>Host</dt>
					<dd>{url}</dd>
					<dt>Path</dt>
					<dd class="setup-mono">{tasksBase || '/'}</dd>
				</dl>
			{/if}
		</section>

		<section class="setup-card" aria-labelledby="setup-account-title">
			<div class="setup-card-head">
				<h2 id="setup-account-title" class="setup-card-title">{registering ? 'New account' : 'Account'}</h2>
				<button type="button" class="setup-link" onclick={() => ((registering = !registering), (error = ''))}>
					{registering ? 'I have an account' : 'Register'}
				</button>
			</div>
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
		</section>

		{#if error}<p class="setup-error" role="alert">{error}</p>{/if}

		<div class="setup-actions">
			{#if oncancel}<button type="button" class="btn btn-secondary" onclick={oncancel}>Cancel</button>{/if}
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
		gap: 1rem;
	}
	.setup-intro {
		display: grid;
		gap: 0.4rem;
	}
	.setup-card {
		display: grid;
		gap: 0.75rem;
		padding: 1rem 1.1rem 1.1rem;
		border: 1px solid var(--clr-border);
		border-radius: 12px;
		background: var(--bg-card);
	}
	.setup-card-head {
		display: flex;
		align-items: baseline;
		justify-content: space-between;
	}
	.setup-card-title {
		margin: 0;
		font-family: var(--font-display);
		font-size: var(--text-sm);
		font-weight: 600;
		letter-spacing: 0.02em;
		text-transform: uppercase;
		color: var(--text-muted);
	}
	.setup-summary {
		display: grid;
		grid-template-columns: auto 1fr;
		gap: 0.3rem 1rem;
		margin: 0;
		font-size: var(--text-sm);
	}
	.setup-summary dt {
		color: var(--text-muted);
	}
	.setup-summary dd {
		margin: 0;
		min-width: 0;
		overflow-wrap: anywhere;
		color: var(--text-primary);
	}
	.setup-mono {
		font-family: var(--font-outlier);
	}
	.setup-hint {
		margin-top: 0.3rem;
		font-size: var(--text-xs);
		color: var(--text-muted);
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
