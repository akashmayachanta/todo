<script lang="ts">
	import { errorDetail } from '$lib/api';
	import { webLogin } from './webClient';

	/**
	 * Signing in on tasks-server's web page (a browser, not the app). The server is
	 * the one serving this page, so only the account is asked for. The browser gets
	 * a device token of its own ("Browser" in the device list), kept in this browser.
	 */

	let username = $state('');
	let password = $state('');
	let busy = $state(false);
	let error = $state('');

	async function submit(e: SubmitEvent) {
		e.preventDefault();
		error = '';
		busy = true;
		try {
			await webLogin(username.trim(), password);
			password = '';
		} catch (err) {
			error = errorDetail(err);
		} finally {
			busy = false;
		}
	}
</script>

<div class="web-signin">
	<form class="web-signin-sheet" onsubmit={submit}>
		<header class="web-signin-intro">
			<h1 class="web-signin-title">Tasks</h1>
			<p class="web-signin-lede">Sign in with your Tasks account. In the browser, Tasks works online only.</p>
		</header>

		<section class="web-signin-card">
			<label class="web-signin-field">
				<span class="label">Username</span>
				<input class="input" required bind:value={username} autocomplete="username" autocapitalize="none" spellcheck="false" />
			</label>
			<label class="web-signin-field">
				<span class="label">Password</span>
				<input class="input" type="password" required bind:value={password} autocomplete="current-password" />
			</label>
		</section>

		{#if error}<p class="web-signin-error" role="alert">{error}</p>{/if}

		<div class="web-signin-actions">
			<button type="submit" class="btn btn-primary" disabled={busy}>{busy ? 'Signing in…' : 'Sign in'}</button>
		</div>
	</form>
</div>

<style>
	.web-signin {
		min-height: 100%;
		display: grid;
		place-items: center;
		padding: 1.5rem 1rem;
	}
	.web-signin-sheet {
		width: min(24rem, 100%);
		display: grid;
		gap: 1rem;
	}
	.web-signin-intro {
		display: grid;
		gap: 0.4rem;
	}
	.web-signin-title {
		font-family: var(--font-display);
		font-size: 1.6rem;
		margin: 0;
		color: var(--text-primary);
	}
	.web-signin-lede {
		margin: 0;
		font-family: var(--font-prose);
		color: var(--text-secondary);
	}
	.web-signin-card {
		display: grid;
		gap: 0.75rem;
		padding: 1rem 1.1rem 1.1rem;
		border: 1px solid var(--clr-border);
		border-radius: 12px;
		background: var(--bg-card);
	}
	.web-signin-field {
		display: grid;
	}
	.web-signin-error {
		margin: 0;
		color: var(--color-danger);
	}
	.web-signin-actions {
		display: flex;
		justify-content: flex-end;
	}
</style>
