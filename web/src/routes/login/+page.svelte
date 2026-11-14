<script lang="ts">
	import { api, errorMessage } from '#lib/api/client.ts';
	import AuthCard from '#lib/components/shell/AuthCard.svelte';
	import Alert from '#lib/components/ui/Alert.svelte';
	import Button from '#lib/components/ui/Button.svelte';
	import TextField from '#lib/components/ui/TextField.svelte';
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

<AuthCard title="Log in" subtitle="Your private drive, at home.">
	<form class="login" onsubmit={handleSubmit}>
		<TextField
			label="Username"
			name="username"
			autocomplete="username"
			autocapitalize="none"
			spellcheck="false"
			bind:value={username}
			required
		/>
		<TextField
			label="Password"
			name="password"
			type="password"
			autocomplete="current-password"
			bind:value={password}
			required
		/>
		{#if error}<Alert>{error}</Alert>{/if}
		<Button type="submit" size="lg" full disabled={busy}>{busy ? 'Logging in…' : 'Log in'}</Button>
	</form>
	{#snippet footer()}No account? Ask someone on this drive for an invitation link.{/snippet}
</AuthCard>

<style>
	.login {
		display: grid;
		gap: var(--space-4);
	}
</style>
