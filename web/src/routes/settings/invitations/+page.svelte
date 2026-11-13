<script lang="ts">
	import { onMount } from 'svelte';
	import { api, errorMessage } from '#lib/api/client.ts';
	import type { components } from '#lib/api/schema.d.ts';

	type Invitation = components['schemas']['InvitationDto'];

	let invitations: Invitation[] = $state([]);
	let emailHint = $state('');
	let newLink: string | null = $state(null);
	let copied = $state(false);
	let error: string | null = $state(null);
	let busy = $state(false);

	const dateFormat = new Intl.DateTimeFormat(undefined, { dateStyle: 'medium', timeStyle: 'short' });

	async function refresh() {
		const { data, error: apiError, response } = await api.GET('/api/invitations');
		if (data) invitations = data;
		else error = errorMessage(apiError, response);
	}

	async function create(event: SubmitEvent) {
		event.preventDefault();
		busy = true;
		error = null;
		copied = false;
		try {
			const { data, error: apiError, response } = await api.POST('/api/invitations', {
				body: { email_hint: emailHint.trim() || null }
			});
			if (!data) {
				error = errorMessage(apiError, response);
				return;
			}
			newLink = `${window.location.origin}/join/${data.token}`;
			emailHint = '';
			await refresh();
		} catch {
			error = errorMessage(null);
		} finally {
			busy = false;
		}
	}

	async function revoke(id: string) {
		error = null;
		const { error: apiError, response } = await api.DELETE('/api/invitations/{id}', {
			params: { path: { id } }
		});
		if (apiError) error = errorMessage(apiError, response);
		await refresh();
	}

	async function copyLink() {
		if (!newLink) return;
		try {
			await navigator.clipboard.writeText(newLink);
			copied = true;
		} catch {
			error = 'Could not copy automatically. Select the link and copy it manually.';
		}
	}

	onMount(refresh);
</script>

<main class="page">
	<h1>Invitations</h1>
	<p class="muted">Each link lets one person create an account. Links expire after 7 days.</p>

	<form onsubmit={create}>
		<label>Who is it for? (optional) <input bind:value={emailHint} placeholder="bob@example.com" /></label>
		<button type="submit" disabled={busy}>Create invitation</button>
	</form>

	{#if newLink}
		<section aria-live="polite">
			<p>Send this link now. It will not be shown again.</p>
			<p data-testid="invite-link"><code>{newLink}</code></p>
			<button class="link" onclick={copyLink}>{copied ? 'Copied' : 'Copy link'}</button>
		</section>
	{/if}

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
							<button class="link" onclick={() => revoke(invitation.id)}>Revoke</button>
						{/if}
					</td>
				</tr>
			{:else}
				<tr><td colspan="4" class="muted">No invitations yet.</td></tr>
			{/each}
		</tbody>
	</table>
</main>
