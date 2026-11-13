<script lang="ts">
	import { goto } from '$app/navigation';
	import { page } from '$app/state';
	import { api, errorMessage } from '#lib/api/client.ts';
	import type { components } from '#lib/api/schema.d.ts';
	import AccountForm from '#lib/components/AccountForm.svelte';
	import { session, setMe } from '#lib/session.svelte.ts';

	type Status = 'checking' | 'valid' | 'invalid' | 'error';

	const token = $derived(page.params.token ?? '');
	let status: Status = $state('checking');
	let checkError: string | null = $state(null);
	let attempt = $state(0);
	let joined = $state(false);

	$effect(() => {
		const current = token;
		void attempt; // re-run the check when the user retries
		let cancelled = false;
		status = 'checking';
		checkError = null;
		api
			.GET('/api/join/{token}', { params: { path: { token: current } } })
			.then(({ data, error, response }) => {
				if (cancelled) return;
				if (data?.valid === true) status = 'valid';
				else if (data?.valid === false) status = 'invalid';
				else {
					status = 'error';
					checkError = errorMessage(error, response);
				}
			})
			.catch(() => {
				if (cancelled) return;
				status = 'error';
				checkError = errorMessage(null);
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
		joined = true;
		setMe(data);
		// The guard lets logged-in users stay on /join/ (to explain why the link is unusable),
		// so navigate home explicitly.
		await goto('/', { replaceState: true });
		return null;
	}
</script>

<svelte:head><title>Join · Squirrel</title></svelte:head>

<main class="card">
	<h1>Join Squirrel</h1>
	{#if session.me && !joined}
		<p>You are already logged in as <strong>{session.me.display_name}</strong>. Log out to use this invitation.</p>
	{:else if status === 'checking'}
		<p class="muted">Checking your invitation…</p>
	{:else if status === 'invalid'}
		<p class="error">This invitation is invalid, expired or already used. Ask for a new link.</p>
	{:else if status === 'error'}
		<div role="alert"><p class="error">Could not check this invitation: {checkError}</p></div>
		<button onclick={() => attempt++}>Retry</button>
	{:else}
		<p class="muted">You have been invited. Choose your account details.</p>
		<AccountForm submitLabel="Join Squirrel" onsubmit={join} />
	{/if}
</main>
