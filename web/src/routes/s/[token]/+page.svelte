<script lang="ts">
	import { page } from '$app/state';
	import Icon from '#lib/components/icons/Icon.svelte';
	import PublicAlbum from '#lib/components/public/PublicAlbum.svelte';
	import PublicFolder from '#lib/components/public/PublicFolder.svelte';
	import PublicViewer from '#lib/components/public/PublicViewer.svelte';
	import AuthCard from '#lib/components/shell/AuthCard.svelte';
	import Alert from '#lib/components/ui/Alert.svelte';
	import Button from '#lib/components/ui/Button.svelte';
	import IconButton from '#lib/components/ui/IconButton.svelte';
	import TextField from '#lib/components/ui/TextField.svelte';
	import PublicPreview from '#lib/components/public/PublicPreview.svelte';
	import { contentUrl, linkInfo, unlock, type LinkInfo } from '#lib/public/api.ts';
	import { fileOfNode, linkDetails, linkExpiry, linkIcon, linkName, type ViewedFile } from '#lib/public/view.ts';

	type Status = 'loading' | 'gone' | 'error' | 'locked' | 'open';

	const token = $derived(page.params.token ?? '');
	let status: Status = $state('loading');
	let info = $state<LinkInfo | null>(null);
	let loadError: string | null = $state(null);
	let attempt = $state(0);
	let password = $state('');
	let unlockError: string | null = $state(null);
	let unlocking = $state(false);
	let viewed: ViewedFile | null = $state(null);
	/** The folder or album view, whose "Download all" the header button runs. */
	let collection: { downloadAll: () => Promise<void> } | undefined = $state();
	let zipCount = $state(0);
	let zipping = $state(false);

	$effect(() => {
		const current = token;
		void attempt; // re-run when the visitor retries or unlocks
		let cancelled = false;
		linkInfo(current).then((result) => {
			if (cancelled) return;
			if (result.ok) {
				info = result.data;
				status = result.data.locked ? 'locked' : 'open';
			} else if (result.gone) {
				status = 'gone';
			} else {
				loadError = result.error;
				status = 'error';
			}
		});
		return () => {
			cancelled = true;
		};
	});

	async function submit(event: SubmitEvent) {
		event.preventDefault();
		if (password === '') {
			unlockError = 'Enter the password.';
			return;
		}
		unlocking = true;
		unlockError = await unlock(token, password);
		unlocking = false;
		if (unlockError === null) {
			password = '';
			attempt++;
		}
	}

	/** The link closed or locked while browsing: ask the server what it is now. */
	function recheck() {
		viewed = null;
		attempt++;
	}

	const title = $derived(info && !info.locked ? linkName(info) : 'Shared with you');
</script>

<svelte:head>
	<title>{title} · Squirrel</title>
	<meta name="robots" content="noindex, nofollow" />
	<meta name="referrer" content="no-referrer" />
</svelte:head>

{#if status === 'loading'}
	<main class="center"><p class="muted" aria-live="polite">Loading…</p></main>
{:else if status === 'gone'}
	<AuthCard title="Link unavailable" subtitle="This link is invalid, has expired or was revoked.">
		<p class="muted">Ask the person who shared it for a new link.</p>
	</AuthCard>
{:else if status === 'error'}
	<AuthCard title="Something went wrong">
		<Alert>{loadError}</Alert>
		<Button variant="secondary" full onclick={() => attempt++}><Icon name="refresh" />Retry</Button>
	</AuthCard>
{:else if status === 'locked'}
	<AuthCard title="Password needed" subtitle="Whoever shared this protected it with a password.">
		<form class="unlock" onsubmit={submit}>
			<TextField label="Password" type="password" autocomplete="current-password" bind:value={password} error={unlockError} />
			<Button type="submit" full disabled={unlocking}><Icon name="arrow-right" />{unlocking ? 'Checking…' : 'Open'}</Button>
		</form>
	</AuthCard>
{:else if info}
	{@const expiry = linkExpiry(info)}
	<main class="shell">
		<div class="panel">
			<header class="head">
				<div class="title">
					<span class="well" aria-hidden="true"><Icon name={linkIcon(info)} /></span>
					<div class="names">
						<h1>{linkName(info)}</h1>
						<p class="muted">{linkDetails(info)}</p>
					</div>
				</div>
				<div class="actions">
					{#if info.kind === 'file' && info.node}
						<IconButton
							icon="external-link"
							label="Open the original in a new tab"
							fill="tint"
							href={contentUrl(token, info.node.id)}
							newTab
						/>
						<Button size="sm" href={contentUrl(token, info.node.id, true)} download><Icon name="download" />Download</Button>
					{:else if zipCount > 0}
						<Button size="sm" disabled={zipping} onclick={() => collection?.downloadAll()}>
							<Icon name="download" />{zipping ? 'Preparing…' : 'Download all'}
						</Button>
					{/if}
				</div>
			</header>
			<div class="body" class:stage={info.kind === 'file'}>
				{#if info.kind === 'folder'}
					<PublicFolder bind:this={collection} bind:zipCount bind:zipping {token} onOpen={(file) => (viewed = file)} onGone={recheck} />
				{:else if info.kind === 'album'}
					<PublicAlbum bind:this={collection} bind:zipCount bind:zipping {token} onOpen={(file) => (viewed = file)} onGone={recheck} />
				{:else if info.node}
					<PublicPreview {token} file={fileOfNode(info.node)} />
				{/if}
			</div>
			<footer class="foot">
				<span class="wordmark">Squirrel</span>
				{#if expiry}<span>{expiry}</span>{/if}
			</footer>
		</div>
	</main>
{/if}

{#if viewed}
	<PublicViewer {token} file={viewed} onClose={() => (viewed = null)} />
{/if}

<style>
	.center {
		display: grid;
		place-items: center;
		min-height: var(--center-min);
		padding: var(--space-6) var(--gutter);
	}

	.unlock {
		display: grid;
		gap: var(--space-4);
	}

	.shell {
		min-height: 100svh;
		padding: calc(var(--frame) + env(safe-area-inset-top)) var(--frame) calc(var(--frame) + env(safe-area-inset-bottom));
	}

	.panel {
		display: flex;
		flex-direction: column;
		width: min(100%, var(--content-max));
		min-height: calc(100svh - 2 * var(--frame));
		margin: 0 auto;
		overflow: hidden;
		border: 1px solid var(--hair);
		border-radius: var(--radius-panel);
		background: var(--panel);
		box-shadow: var(--panel-edge);
	}

	.head {
		display: flex;
		flex-wrap: wrap;
		align-items: center;
		gap: var(--space-3) var(--space-4);
		padding: var(--space-3) var(--space-4);
		border-bottom: 1px solid var(--hair);
	}

	.title {
		display: flex;
		flex: 1 1 18rem;
		align-items: center;
		gap: var(--space-3);
		min-width: 0;
	}

	.well {
		display: grid;
		flex: none;
		place-items: center;
		width: var(--well-md);
		height: var(--well-md);
		border-radius: var(--radius-control);
		background: var(--tint);
		color: var(--ink-muted);
	}

	.names {
		min-width: 0;
	}

	h1 {
		margin: 0;
		overflow: hidden;
		font-size: var(--text-lg);
		line-height: var(--text-lg-lh);
		font-weight: var(--weight-label);
		letter-spacing: var(--track-label);
		text-overflow: ellipsis;
		white-space: nowrap;
	}

	.names .muted {
		font-size: var(--text-sm);
		line-height: var(--text-sm-lh);
	}

	.actions {
		display: flex;
		align-items: center;
		gap: var(--space-2);
	}

	.body {
		display: grid;
		flex: 1;
		align-content: start;
		gap: var(--space-3);
		padding: var(--panel-pad);
	}

	.body.stage {
		place-items: center;
		align-content: center;
		background: var(--head);
	}

	.foot {
		display: flex;
		flex-wrap: wrap;
		justify-content: space-between;
		gap: var(--space-2);
		padding: var(--space-3) var(--space-4);
		border-top: 1px solid var(--hair);
		color: var(--ink-muted);
		font-size: var(--text-sm);
		line-height: var(--text-sm-lh);
	}

	.wordmark {
		font-weight: var(--weight-strong);
		letter-spacing: var(--track-heading);
	}

	.muted {
		margin: 0;
	}
</style>
