<script lang="ts">
	import { page } from '$app/state';
	import Icon from '#lib/components/icons/Icon.svelte';
	import PublicAlbum from '#lib/components/public/PublicAlbum.svelte';
	import PublicFolder from '#lib/components/public/PublicFolder.svelte';
	import PublicViewer from '#lib/components/public/PublicViewer.svelte';
	import AuthCard from '#lib/components/shell/AuthCard.svelte';
	import Alert from '#lib/components/ui/Alert.svelte';
	import Button from '#lib/components/ui/Button.svelte';
	import TextField from '#lib/components/ui/TextField.svelte';
	import { formatBytes } from '#lib/files/format.ts';
	import { contentUrl, linkInfo, unlock, type LinkInfo } from '#lib/public/api.ts';
	import { fileOfNode, linkByline, linkName, type ViewedFile } from '#lib/public/view.ts';

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
	<main class="shell">
		<div class="panel">
			<header class="head">
				<span class="wordmark">Squirrel</span>
				<h1>{linkName(info)}</h1>
				<p class="muted">{linkByline(info)}</p>
			</header>
			{#if info.kind === 'folder'}
				<PublicFolder {token} onOpen={(file) => (viewed = file)} onGone={recheck} />
			{:else if info.kind === 'album'}
				<PublicAlbum {token} onOpen={(file) => (viewed = file)} onGone={recheck} />
			{:else if info.node}
				{@const node = info.node}
				<section class="single" aria-label="Shared file">
					<p class="muted">{formatBytes(node.size)}</p>
					<div class="actions">
						<Button variant="secondary" onclick={() => (viewed = fileOfNode(node))}><Icon name="image" />Preview</Button>
						<Button href={contentUrl(token, node.id, true)} download><Icon name="download" />Download</Button>
					</div>
				</section>
			{/if}
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
		display: grid;
		gap: var(--space-5);
		align-content: start;
		width: min(100%, var(--content-max));
		min-height: calc(100svh - 2 * var(--frame));
		margin: 0 auto;
		padding: var(--panel-pad);
		border: 1px solid var(--hair);
		border-radius: var(--radius-panel);
		background: var(--panel);
		box-shadow: var(--panel-edge);
	}

	.head {
		display: grid;
		gap: var(--space-1);
	}

	.wordmark {
		color: var(--ink-muted);
		font-size: var(--text-sm);
		line-height: var(--text-sm-lh);
		font-weight: var(--weight-strong);
		letter-spacing: var(--track-heading);
	}

	h1 {
		margin: 0;
		overflow-wrap: anywhere;
		font-size: var(--text-2xl);
		line-height: var(--text-2xl-lh);
		font-weight: var(--weight-heading);
		letter-spacing: var(--track-heading);
	}

	.muted {
		margin: 0;
	}

	.single,
	.actions {
		display: flex;
		flex-wrap: wrap;
		align-items: center;
		gap: var(--space-3);
	}

	.single {
		justify-content: space-between;
	}
</style>
