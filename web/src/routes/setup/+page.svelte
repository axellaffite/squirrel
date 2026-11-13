<script lang="ts">
	import { goto } from '$app/navigation';
	import { api, errorMessage } from '#lib/api/client.ts';
	import type { components } from '#lib/api/schema.d.ts';
	import AccountForm from '#lib/components/AccountForm.svelte';
	import { session, setMe } from '#lib/session.svelte.ts';

	async function createAdmin(account: components['schemas']['AccountRequest']) {
		const { data, error, response } = await api.POST('/api/setup/admin', { body: account });
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
	<AccountForm submitLabel="Create administrator account" onsubmit={createAdmin} />
</main>
