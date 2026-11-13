<script lang="ts">
	import { onMount } from 'svelte';
	import { api, errorMessage } from '#lib/api/client.ts';
	import type { components } from '#lib/api/schema.d.ts';
	import { session } from '#lib/session.svelte.ts';

	type Invitation = components['schemas']['InvitationDto'];

	let invitations: Invitation[] = $state([]);
	let emailHint = $state('');
	let newLink: string | null = $state(null);
	let copied = $state(false);
	let error: string | null = $state(null);
	let copyError: string | null = $state(null);
	let busy = $state(false);

	const canInvite = $derived(session.me?.can_invite ?? false);
	const dateFormat = new Intl.DateTimeFormat(undefined, { dateStyle: 'medium', timeStyle: 'short' });

	/** Reloads the list. Returns an error message, or null (and clears the error) on success. */
	async function refresh(): Promise<string | null> {
		try {
			const { data, error: apiError, response } = await api.GET('/api/invitations');
			if (!data) return (error = errorMessage(apiError, response));
			invitations = data;
			error = null;
			return null;
		} catch {
			return (error = errorMessage(null));
		}
	}

	async function create(event: SubmitEvent) {
		event.preventDefault();
		busy = true;
		error = null;
		copied = false;
		copyError = null;
		try {
			const { data, error: apiError, response } = await api.POST('/api/invitations', {
				body: { email_hint: emailHint.trim() || null }
			});
			if (!data) {
				error = errorMessage(apiError, response);
				return;
			}
			// Set before refreshing: the link is shown once and must survive a failed refresh.
			newLink = `${window.location.origin}/join/${data.token}`;
			emailHint = '';
		} catch {
			error = errorMessage(null);
			return;
		} finally {
			busy = false;
		}
		await refresh();
	}

	async function revoke(id: string) {
		let revokeError: string | null = null;
		try {
			const { error: apiError, response } = await api.DELETE('/api/invitations/{id}', {
				params: { path: { id } }
			});
			if (!response.ok) revokeError = errorMessage(apiError, response);
		} catch {
			revokeError = errorMessage(null);
		}
		await refresh();
		// Shown after the refresh, which clears the error slot when it succeeds.
		if (revokeError) error = revokeError;
	}

	async function copyLink() {
		if (!newLink) return;
		copyError = null;
		try {
			await navigator.clipboard.writeText(newLink);
			copied = true;
		} catch {
			copied = false;
			copyError = 'Could not copy automatically. Select the link and copy it manually.';
		}
	}

	onMount(refresh);
</script>

<svelte:head><title>Invitations · Squirrel</title></svelte:head>

<main class="page">
	<h1>Invitations</h1>
	{#if canInvite}
		<p class="muted">Each link lets one person create an account. Links expire after 7 days.</p>

		<form onsubmit={create}>
			<label>Who is it for? (optional) <input bind:value={emailHint} placeholder="bob@example.com" /></label>
			<button type="submit" disabled={busy}>Create invitation</button>
		</form>
	{:else}
		<p class="muted">
			Only administrators can invite people to this drive. Ask an administrator if someone needs an
			account.
		</p>
	{/if}

	<div aria-live="polite">
		{#if newLink}
			<section>
				<p>Send this link now. It will not be shown again.</p>
				<p data-testid="invite-link"><code>{newLink}</code></p>
				<button class="link" onclick={copyLink}>Copy link</button>
				{#if copied}<p class="muted">Copied</p>{/if}
			</section>
		{/if}
	</div>
	{#if copyError}<p class="error" role="alert">{copyError}</p>{/if}

	{#if error}<p class="error" role="alert">{error}</p>{/if}

	<table>
		<thead><tr><th>For</th><th>Status</th><th>Expires</th><th><span class="visually-hidden">Actions</span></th></tr></thead>
		<tbody>
			{#each invitations as invitation (invitation.id)}
				<tr>
					<td>{invitation.email_hint ?? '—'}</td>
					<td>{invitation.status}</td>
					<td>{dateFormat.format(new Date(invitation.expires_at))}</td>
					<td>
						{#if invitation.status === 'active'}
							<button
								class="link"
								aria-label={`Revoke invitation for ${invitation.email_hint ?? 'unnamed recipient'}`}
								onclick={() => revoke(invitation.id)}>Revoke</button
							>
						{/if}
					</td>
				</tr>
			{:else}
				<tr><td colspan="4" class="muted">No invitations yet.</td></tr>
			{/each}
		</tbody>
	</table>
</main>
