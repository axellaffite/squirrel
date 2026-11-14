<script lang="ts">
	import type { Snippet } from 'svelte';
	import type { components } from '#lib/api/schema.d.ts';
	import Alert from '#lib/components/ui/Alert.svelte';
	import Button from '#lib/components/ui/Button.svelte';
	import TextField from '#lib/components/ui/TextField.svelte';
	import { PASSWORD_MIN_LENGTH, validateAccount, type AccountFields } from '#lib/validation.ts';

	type AccountRequest = components['schemas']['AccountRequest'];

	interface Props {
		submitLabel: string;
		/** Returns an error message to display, or null on success. */
		onsubmit: (account: AccountRequest) => Promise<string | null>;
		/** Extra fields rendered above the account fields (e.g. the setup code). */
		before?: Snippet;
	}

	let { submitLabel, onsubmit, before }: Props = $props();

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
			if (error) {
				fields.password = '';
				fields.confirmPassword = '';
			}
			busy = false;
		}
	}
</script>

<form class="account-form" onsubmit={handleSubmit} novalidate>
	{@render before?.()}
	<TextField
		label="Username"
		name="username"
		autocomplete="username"
		autocapitalize="none"
		spellcheck="false"
		hint="3–32 characters: lowercase letters, digits, dots, dashes or underscores."
		bind:value={fields.username}
		required
	/>
	<TextField label="Display name" name="display_name" autocomplete="name" bind:value={fields.displayName} required />
	<TextField
		label="Password"
		name="password"
		type="password"
		autocomplete="new-password"
		hint={`At least ${PASSWORD_MIN_LENGTH} characters.`}
		bind:value={fields.password}
		required
	/>
	<TextField
		label="Confirm password"
		name="confirm_password"
		type="password"
		autocomplete="new-password"
		bind:value={fields.confirmPassword}
		required
	/>
	{#if error}<Alert>{error}</Alert>{/if}
	<Button type="submit" size="lg" full disabled={busy}>{busy ? 'Please wait…' : submitLabel}</Button>
</form>

<style>
	.account-form {
		display: grid;
		gap: var(--space-4);
	}
</style>
