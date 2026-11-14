<script lang="ts">
	import { api, errorMessage } from '#lib/api/client.ts';
	import type { components } from '#lib/api/schema.d.ts';
	import AccountForm from '#lib/components/AccountForm.svelte';
	import AuthCard from '#lib/components/shell/AuthCard.svelte';
	import TextField from '#lib/components/ui/TextField.svelte';
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
		// The layout guard sends a logged-in user from /setup to the home page.
		session.needsSetup = false;
		setMe(data);
		return null;
	}
</script>

<svelte:head><title>Set up · Squirrel</title></svelte:head>

<AuthCard title="Welcome to Squirrel" subtitle="Create the first account. It will administer this drive.">
	<AccountForm submitLabel="Create administrator account" onsubmit={createAdmin}>
		{#snippet before()}
			<TextField
				label="Setup code"
				name="setup_token"
				autocomplete="off"
				autocapitalize="none"
				spellcheck="false"
				hint="Printed in the Squirrel server log on first start."
				bind:value={setupCode}
				required
			/>
		{/snippet}
	</AccountForm>
</AuthCard>
