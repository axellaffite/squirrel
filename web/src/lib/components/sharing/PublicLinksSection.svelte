<script lang="ts">
	import { onMount, untrack } from 'svelte';
	import Icon from '#lib/components/icons/Icon.svelte';
	import Alert from '#lib/components/ui/Alert.svelte';
	import Button from '#lib/components/ui/Button.svelte';
	import IconButton from '#lib/components/ui/IconButton.svelte';
	import ListRow from '#lib/components/ui/ListRow.svelte';
	import TextField from '#lib/components/ui/TextField.svelte';
	import type { Subject } from '#lib/sharing/api.ts';
	import { removeById } from '#lib/sharing/logic.ts';
	import {
		createPublicLink,
		expiryFromDay,
		linkMeta,
		linkPasswordProblem,
		linkTitle,
		listPublicLinks,
		revokePublicLink,
		today,
		type PublicLink
	} from '#lib/sharing/publicLinks.ts';

	interface Props {
		subject: Subject;
	}

	let { subject }: Props = $props();

	const target = untrack(() => subject);
	/** `null` while loading. */
	let enabled: boolean | null = $state(null);
	let links: PublicLink[] = $state([]);
	let day = $state('');
	let password = $state('');
	let dayError: string | null = $state(null);
	let passwordError: string | null = $state(null);
	let error: string | null = $state(null);
	let busy = $state(false);
	let newUrl: string | null = $state(null);
	let copied = $state(false);
	let copyError: string | null = $state(null);

	onMount(async () => {
		const listed = await listPublicLinks(target);
		if (listed.ok) {
			enabled = listed.data.enabled;
			links = listed.data.links;
		} else {
			error = listed.error;
		}
	});

	async function create(event: SubmitEvent) {
		event.preventDefault();
		const expiry = expiryFromDay(day);
		dayError = 'error' in expiry ? expiry.error : null;
		passwordError = linkPasswordProblem(password);
		if ('error' in expiry || passwordError) return;
		busy = true;
		error = null;
		copied = false;
		copyError = null;
		const created = await createPublicLink(target, {
			expiresAt: expiry.expiresAt,
			password: password === '' ? null : password
		});
		busy = false;
		if (!created.ok) {
			error = created.error;
			return;
		}
		newUrl = created.data.url;
		links = [created.data.link, ...links];
		day = '';
		password = '';
	}

	async function revoke(link: PublicLink) {
		error = null;
		const result = await revokePublicLink(link.id);
		if (result.ok) links = removeById(links, link.id);
		else error = result.error;
	}

	async function copy() {
		if (!newUrl) return;
		copyError = null;
		try {
			await navigator.clipboard.writeText(newUrl);
			copied = true;
		} catch {
			copied = false;
			copyError = 'Could not copy automatically. Select the link and copy it manually.';
		}
	}
</script>

<section class="links" aria-labelledby="public-links-heading">
	<h3 id="public-links-heading">Public link</h3>
	{#if enabled === null}
		{#if !error}<p class="muted" aria-live="polite">Loading…</p>{/if}
	{:else if !enabled}
		<p class="muted">
			Public links are off on this server. An administrator turns them on by setting its public address
			(<code>SQUIRREL_PUBLIC_URL</code>).
		</p>
	{:else}
		<p class="muted">Anyone with the link can view and download it, without an account.</p>
		<form class="form" onsubmit={create}>
			<TextField label="Expires (optional)" type="date" min={today()} bind:value={day} error={dayError} />
			<TextField
				label="Password (optional)"
				type="password"
				autocomplete="new-password"
				bind:value={password}
				error={passwordError}
			/>
			<Button type="submit" disabled={busy}><Icon name="link" />Create link</Button>
		</form>
	{/if}

	<div class="live" aria-live="polite">
		{#if newUrl}
			<div class="new-link">
				<p>Copy this link now. It will not be shown again.</p>
				<p class="url" data-testid="public-link-url"><code>{newUrl}</code></p>
				<div class="copy">
					<Button variant="secondary" size="sm" onclick={copy}><Icon name="copy" />Copy link</Button>
					{#if copied}<span class="copied"><Icon name="check" />Copied</span>{/if}
				</div>
			</div>
		{/if}
	</div>
	{#if copyError}<Alert>{copyError}</Alert>{/if}
	{#if error}<Alert>{error}</Alert>{/if}

	{#if links.length > 0}
		<ul class="rows" aria-label="Public links">
			{#each links as link (link.id)}
				<ListRow testId="public-link-row">
					{#snippet icon()}<Icon name="link" />{/snippet}
					{#snippet title()}{linkTitle(link)}{/snippet}
					{#snippet meta()}{linkMeta(link)}{/snippet}
					{#snippet actions()}
						<IconButton icon="x" label={`Revoke the link ${linkTitle(link).toLowerCase()}`} tooltip="Revoke" onclick={() => revoke(link)} />
					{/snippet}
				</ListRow>
			{/each}
		</ul>
	{/if}
</section>

<style>
	.links,
	.form {
		display: grid;
		gap: var(--space-3);
	}

	h3 {
		margin: 0;
		font-size: var(--text-md);
		line-height: var(--text-md-lh);
		font-weight: var(--weight-strong);
	}

	@media (min-width: 48rem) {
		.form {
			grid-template-columns: minmax(0, 1fr) minmax(0, 1fr) auto;
			align-items: end;
		}
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

	.new-link p {
		margin: 0;
	}

	.url code {
		overflow-wrap: anywhere;
	}

	.copy,
	.copied {
		display: flex;
		align-items: center;
		gap: var(--space-2);
	}

	.copied {
		color: var(--success);
	}

	.rows {
		display: grid;
		gap: var(--space-2);
		margin: 0;
		padding: var(--space-2);
		list-style: none;
		border-radius: var(--radius-well);
		background: var(--tint);
	}

	.muted {
		margin: 0;
	}
</style>
