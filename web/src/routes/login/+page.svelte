<script lang="ts">
	import { api, errorMessage } from '#lib/api/client.ts';
	import { setMe } from '#lib/session.svelte.ts';

	let username = $state('');
	let password = $state('');
	let error: string | null = $state(null);
	let busy = $state(false);

	async function handleSubmit(event: SubmitEvent) {
		event.preventDefault();
		busy = true;
		error = null;
		try {
			const { data, error: apiError, response } = await api.POST('/api/auth/login', {
				body: { username: username.trim().toLowerCase(), password }
			});
			if (!data) {
				error = errorMessage(apiError, response);
				password = '';
				return;
			}
			// The layout guard sends a logged-in user from /login to the home page.
			setMe(data);
		} catch {
			error = errorMessage(null);
		} finally {
			busy = false;
		}
	}
</script>

<svelte:head><title>Log in · Squirrel</title></svelte:head>

<main class="card">
	<h1>Log in</h1>
	<form onsubmit={handleSubmit}>
		<label>Username <input name="username" autocomplete="username" autocapitalize="none" spellcheck="false" bind:value={username} required /></label>
		<label>Password <input name="password" type="password" autocomplete="current-password" bind:value={password} required /></label>
		{#if error}<p class="error" role="alert">{error}</p>{/if}
		<button type="submit" disabled={busy}>{busy ? 'Logging in…' : 'Log in'}</button>
	</form>
	<p class="muted">No account? Ask someone on this drive for an invitation link.</p>
</main>
