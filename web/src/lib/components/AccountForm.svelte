<script lang="ts">
	import type { components } from '#lib/api/schema.d.ts';
	import { validateAccount, type AccountFields } from '#lib/validation.ts';

	type AccountRequest = components['schemas']['AccountRequest'];

	interface Props {
		submitLabel: string;
		/** Returns an error message to display, or null on success. */
		onsubmit: (account: AccountRequest) => Promise<string | null>;
	}

	let { submitLabel, onsubmit }: Props = $props();

	let fields: AccountFields = $state({
		username: '',
		displayName: '',
		password: '',
		confirmPassword: ''
	});
	let error: string | null = $state(null);
	let busy = $state(false);

	async function handleSubmit(event: SubmitEvent) {
		event.preventDefault();
		error = validateAccount(fields);
		if (error) return;
		busy = true;
		try {
			error = await onsubmit({
				username: fields.username.trim().toLowerCase(),
				display_name: fields.displayName.trim(),
				password: fields.password
			});
		} catch {
			error = 'Something went wrong. Please try again.';
		} finally {
			busy = false;
		}
	}
</script>

<form onsubmit={handleSubmit} novalidate>
	<label>Username <input name="username" autocomplete="username" bind:value={fields.username} required /></label>
	<label>Display name <input name="display_name" autocomplete="name" bind:value={fields.displayName} required /></label>
	<label>Password <input name="password" type="password" autocomplete="new-password" bind:value={fields.password} required /></label>
	<label>Confirm password <input name="confirm_password" type="password" autocomplete="new-password" bind:value={fields.confirmPassword} required /></label>
	{#if error}<p class="error" role="alert">{error}</p>{/if}
	<button type="submit" disabled={busy}>{busy ? 'Please wait…' : submitLabel}</button>
</form>
