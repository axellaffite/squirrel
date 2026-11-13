<script lang="ts">
	import { goto } from '$app/navigation';
	import { page } from '$app/state';
	import { api, errorMessage } from '#lib/api/client.ts';
	import type { components } from '#lib/api/schema.d.ts';
	import AccountForm from '#lib/components/AccountForm.svelte';
	import { session, setMe } from '#lib/session.svelte.ts';

	const token = $derived(page.params.token ?? '');
	let status: 'checking' | 'valid' | 'invalid' = $state('checking');

	$effect(() => {
		const current = token;
		let cancelled = false;
		status = 'checking';
		api
			.GET('/api/join/{token}', { params: { path: { token: current } } })
			.then(({ data }) => {
				if (!cancelled) status = data?.valid ? 'valid' : 'invalid';
			})
			.catch(() => {
				if (!cancelled) status = 'invalid';
			});
		return () => {
			cancelled = true;
		};
	});

	async function join(account: components['schemas']['AccountRequest']) {
		const { data, error, response } = await api.POST('/api/join/{token}', {
			params: { path: { token } },
			body: account
		});
		if (!data) return errorMessage(error, response);
		setMe(data);
		await goto('/');
		return null;
	}
</script>

<main class="card">
	<h1>Join Squirrel</h1>
	{#if session.me}
		<p>You are already logged in as <strong>{session.me.display_name}</strong>. Log out to use this invitation.</p>
	{:else if status === 'checking'}
		<p class="muted">Checking your invitation…</p>
	{:else if status === 'invalid'}
		<p class="error">This invitation is invalid, expired or already used. Ask for a new link.</p>
	{:else}
		<p class="muted">You have been invited. Choose your account details.</p>
		<AccountForm submitLabel="Join Squirrel" onsubmit={join} />
	{/if}
</main>
