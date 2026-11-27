<script lang="ts">
	import { onMount, untrack } from 'svelte';
	import Icon from '#lib/components/icons/Icon.svelte';
	import Alert from '#lib/components/ui/Alert.svelte';
	import Button from '#lib/components/ui/Button.svelte';
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
			<div class="fields">
				<TextField label="Expires (optional)" type="date" min={today()} bind:value={day} error={dayError} />
				<!-- Shown as typed: the owner passes it on. Not a login, so password managers keep out. -->
				<TextField
					label="Password (optional)"
					type="text"
					placeholder="None"
					autocomplete="off"
					autocapitalize="off"
					spellcheck={false}
					data-1p-ignore
					data-lpignore="true"
					data-bwignore
					data-form-type="other"
					bind:value={password}
					error={passwordError}
				/>
			</div>
			<p class="hint">The password shows as you type: you'll pass it on with the link. 10 characters or more.</p>
			<Button type="submit" full disabled={busy}><Icon name="link" />Create link</Button>
		</form>
	{/if}

	<div class="live" aria-live="polite">
		{#if newUrl}
			<div class="new-link">
				<p><strong>Link created.</strong> <span class="muted">Copy it now: it won't be shown again.</span></p>
				<div class="copy">
					<input
						class="url"
						type="text"
						readonly
						aria-label="New link"
						value={newUrl}
						data-testid="public-link-url"
						onfocus={(event) => event.currentTarget.select()}
					/>
					<Button variant="secondary" size="sm" class={copied ? 'copied' : ''} onclick={copy}>
						{#if copied}<Icon name="check" />Copied{:else}<Icon name="copy" />Copy{/if}
					</Button>
				</div>
			</div>
		{/if}
	</div>
	{#if copyError}<Alert>{copyError}</Alert>{/if}
	{#if error}<Alert>{error}</Alert>{/if}

	{#if links.length > 0}
		<h4 class="label">Active links</h4>
		<ul class="rows" aria-label="Public links">
			{#each links as link (link.id)}
				<ListRow testId="public-link-row">
					{#snippet icon()}<Icon name="link" />{/snippet}
					{#snippet title()}{linkTitle(link)}{/snippet}
					{#snippet meta()}{linkMeta(link)}{/snippet}
					{#snippet actions()}
						<Button variant="danger" size="sm" aria-label={`Revoke the link ${linkTitle(link).toLowerCase()}`} onclick={() => revoke(link)}>Revoke</Button>
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
		font-size: var(--text-lg);
		line-height: var(--text-lg-lh);
		font-weight: var(--weight-strong);
	}

	/* Two columns at every width: the dialog is narrow even on a wide screen. */
	.fields {
		display: grid;
		grid-template-columns: repeat(2, minmax(0, 1fr));
		gap: var(--space-3);
		align-items: start;
	}

	.hint,
	.label {
		margin: 0;
		color: var(--ink-muted);
		font-size: var(--text-sm);
		line-height: var(--text-sm-lh);
		font-weight: var(--weight-body);
	}

	.live {
		display: contents;
	}

	.new-link {
		display: grid;
		gap: var(--space-3);
		padding: var(--card-pad);
		border-radius: var(--radius-card);
		background: var(--accent-soft);
		box-shadow: var(--select-ring);
	}

	.new-link p {
		margin: 0;
		font-size: var(--text-sm);
		line-height: var(--text-sm-lh);
	}

	.new-link strong {
		font-weight: var(--weight-strong);
	}

	.copy {
		display: flex;
		align-items: center;
		gap: var(--space-2);
	}

	.url {
		flex: 1;
		min-width: 0;
		min-height: var(--control-sm);
		padding: 0 var(--space-3);
		border: 1px solid var(--hair-strong);
		border-radius: var(--radius-control);
		background: var(--card);
		color: var(--ink);
		font-family: var(--font-mono);
		font-size: var(--text-sm);
		text-overflow: ellipsis;
	}

	.url:focus-visible {
		outline: var(--ring-width) solid var(--accent);
		outline-offset: var(--ring-offset);
	}

	.copy :global(.copied) {
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
