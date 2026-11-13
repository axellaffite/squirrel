<script lang="ts">
	import { goto } from '$app/navigation';
	import { api, errorMessage } from '#lib/api/client.ts';
	import type { components } from '#lib/api/schema.d.ts';
	import AccountForm from '#lib/components/AccountForm.svelte';
	import { session, setMe } from '#lib/session.svelte.ts';

	let setupCode = $state('');

	async function createAdmin(account: components['schemas']['AccountRequest']) {
		const setupToken = setupCode.trim();
		if (!setupToken) return 'Enter the setup code from the server log.';
		const { data, error, response } = await api.POST('/api/setup/admin', {
			body: { ...account, setup_token: setupToken }
		});
		if (response.status === 403) return 'This setup code is incorrect. Check the server log.';
		if (!data) return errorMessage(error, response);
		session.needsSetup = false;
		setMe(data);
		await goto('/');
		return null;
	}
</script>

<main class="card">
	<h1>Welcome to Squirrel</h1>
	<p class="muted">Create the first account. It will administer this drive.</p>
	<AccountForm submitLabel="Create administrator account" onsubmit={createAdmin}>
		{#snippet before()}
			<label>Setup code <input name="setup_token" autocomplete="off" spellcheck="false" aria-describedby="setup-code-hint" bind:value={setupCode} required /></label>
			<p id="setup-code-hint" class="muted">Printed in the Squirrel server log on first start.</p>
		{/snippet}
	</AccountForm>
</main>
