<script lang="ts">
	import { onMount } from 'svelte';
	import { api, errorMessage } from '#lib/api/client.ts';
	import { expiryNotice, newestInvitation, validityDays } from '#lib/invitations.ts';
	import type { components } from '#lib/api/schema.d.ts';
	import { session } from '#lib/session.svelte.ts';
	import Icon from '#lib/components/icons/Icon.svelte';
	import Page from '#lib/components/shell/Page.svelte';
	import Alert from '#lib/components/ui/Alert.svelte';
	import Badge from '#lib/components/ui/Badge.svelte';
	import Button from '#lib/components/ui/Button.svelte';
	import EmptyState from '#lib/components/ui/EmptyState.svelte';
	import ListRow from '#lib/components/ui/ListRow.svelte';
	import TextField from '#lib/components/ui/TextField.svelte';

	type Invitation = components['schemas']['InvitationDto'];

	let invitations: Invitation[] = $state([]);
	let emailHint = $state('');
	let newLink: string | null = $state(null);
	let copied = $state(false);
	let error: string | null = $state(null);
	let copyError: string | null = $state(null);
	let busy = $state(false);
	let created: Invitation | null = $state(null);

	const canInvite = $derived(session.me?.can_invite ?? false);
	// Derived from what the server returned, so it follows the server's configured validity.
	const expiry = $derived(expiryNotice(validityDays(created ?? newestInvitation(invitations))));
	const dateFormat = new Intl.DateTimeFormat(undefined, { dateStyle: 'medium', timeStyle: 'short' });
	let loaded = $state(false);

	const STATUS: Record<Invitation['status'], { label: string; tone: 'accent' | 'neutral' | 'success' | 'danger' }> = {
		active: { label: 'Active', tone: 'accent' },
		used: { label: 'Used', tone: 'success' },
		expired: { label: 'Expired', tone: 'neutral' },
		revoked: { label: 'Revoked', tone: 'danger' }
	};

	/** Reloads the list. Returns an error message, or null (and clears the error) on success. */
	async function refresh(): Promise<string | null> {
		try {
			const { data, error: apiError, response } = await api.GET('/api/invitations');
			if (!data) return (error = errorMessage(apiError, response));
			invitations = data;
			loaded = true;
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
			created = data.invitation;
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

<Page title="Invitations" back={{ href: '/settings', label: 'Settings' }}>
	{#snippet description()}
		{#if canInvite}Each link lets one person create an account. {expiry}{:else}Only administrators can invite people
			to this drive. Ask an administrator if someone needs an account.{/if}
	{/snippet}

	{#if canInvite}
		<form class="create" onsubmit={create}>
			<div class="field">
				<TextField label="Who is it for? (optional)" bind:value={emailHint} placeholder="bob@example.com" />
			</div>
			<Button type="submit" disabled={busy}><Icon name="link" />Create invitation</Button>
		</form>
	{/if}

	<div class="live" aria-live="polite">
		{#if newLink}
			<section class="new-link" aria-label="New invitation link">
				<p>Send this link now. It will not be shown again.</p>
				<p class="link" data-testid="invite-link"><code>{newLink}</code></p>
				<div class="copy">
					<Button variant="secondary" size="sm" onclick={copyLink}><Icon name="copy" />Copy link</Button>
					{#if copied}<span class="copied"><Icon name="check" />Copied</span>{/if}
				</div>
			</section>
		{/if}
	</div>
	{#if copyError}<Alert>{copyError}</Alert>{/if}

	{#if error}<Alert>{error}</Alert>{/if}

	{#if invitations.length > 0}
		<ul class="rows" aria-label="Invitations">
			{#each invitations as invitation (invitation.id)}
				<ListRow>
					{#snippet icon()}<Icon name="mail" />{/snippet}
					{#snippet title()}{invitation.email_hint ?? 'Anyone with the link'}{/snippet}
					{#snippet meta()}Expires {dateFormat.format(new Date(invitation.expires_at))}{/snippet}
					{#snippet actions()}
						<Badge tone={STATUS[invitation.status].tone}>{STATUS[invitation.status].label}</Badge>
						{#if invitation.status === 'active'}
							<Button
								variant="quiet"
								size="sm"
								aria-label={`Revoke invitation for ${invitation.email_hint ?? 'unnamed recipient'}`}
								onclick={() => revoke(invitation.id)}>Revoke</Button
							>
						{/if}
					{/snippet}
				</ListRow>
			{/each}
		</ul>
	{:else if loaded && !error}
		<EmptyState icon="user-plus" title="No invitations yet">Links you create appear here with their status.</EmptyState>
	{/if}
</Page>

<style>
	.create {
		display: grid;
		gap: var(--space-3);
		padding: var(--well-pad);
		border-radius: var(--radius-well);
		background: var(--tint);
	}

	@media (min-width: 48rem) {
		.create {
			grid-template-columns: minmax(0, 1fr) auto;
			align-items: end;
		}
	}

	.field {
		min-width: 0;
	}

	.live {
		display: contents;
	}

	.new-link {
		display: grid;
		gap: var(--space-3);
		padding: var(--card-pad);
		border-radius: var(--radius-card);
		background: var(--card);
		box-shadow: var(--shadow-card), var(--select-ring);
	}

	.link {
		max-width: none;
		padding: var(--space-3) var(--space-4);
		border-radius: var(--radius-sm);
		background: var(--tint);
	}

	.copy {
		display: flex;
		flex-wrap: wrap;
		align-items: center;
		gap: var(--space-3);
	}

	.copied {
		display: inline-flex;
		align-items: center;
		gap: var(--space-1);
		color: var(--success);
		font-size: var(--text-sm);
		font-weight: var(--weight-label);
	}

	/* One grouped card; ListRow draws the hairline between rows (MASTER.md › "Table"). */
	.rows {
		display: grid;
		margin: 0;
		padding: 0;
		list-style: none;
		border: 1px solid var(--hair);
		border-radius: var(--radius-card);
		background: var(--card);
		box-shadow: var(--shadow-card);
	}
</style>
