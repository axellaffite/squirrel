<script lang="ts">
	import { onMount } from 'svelte';
	import NodeVisual from '#lib/components/files/NodeVisual.svelte';
	import Icon from '#lib/components/icons/Icon.svelte';
	import type { IconName } from '#lib/components/icons/paths.ts';
	import ListRow from '#lib/components/ui/ListRow.svelte';
	import Meter from '#lib/components/ui/Meter.svelte';
	import { listFolder, storageUsage, type FileNode, type StorageUsage } from '#lib/files/api.ts';
	import { formatBytes, formatDate, storageSummary } from '#lib/files/format.ts';
	import { latestOnly } from '#lib/files/latest.ts';
	import { recentItems, uploadTarget } from '#lib/home.ts';
	import { mediaStatus } from '#lib/photos/api.ts';
	import { createPoller } from '#lib/photos/status.ts';
	import { session } from '#lib/session.svelte.ts';
	import { enqueue, onUploadsSettled } from '#lib/upload/uploads.svelte.ts';

	/** How many recently changed items the home page lists. */
	const RECENT_COUNT = 4;

	let usage = $state<StorageUsage | null>(null);
	let usageError: string | null = $state(null);
	let roots: FileNode[] = $state([]);
	let recent: FileNode[] | null = $state(null);
	let recentError: string | null = $state(null);
	const trackUsage = latestOnly();
	const trackRecent = latestOnly();

	const target = $derived(uploadTarget(roots));
	const percent = $derived(
		usage?.quota_bytes ? Math.min(100, Math.round((usage.used_bytes / usage.quota_bytes) * 100)) : null
	);
	const rootName = (id: string | null | undefined) => roots.find((root) => root.id === id)?.name;

	interface QuickLink {
		href: string;
		label: string;
		icon: IconName;
		/** Already in the bottom bar on phones. */
		wideOnly?: boolean;
	}

	const ROOT_ICONS: Record<string, IconName> = { Photos: 'image', Documents: 'file-text' };
	const quick: QuickLink[] = $derived([
		...roots.map((root) => ({ href: `/files/${root.id}`, label: root.name, icon: ROOT_ICONS[root.name] ?? 'folder' })),
		{ href: '/trash', label: 'Trash', icon: 'trash', wideOnly: true },
		...(session.me?.can_invite ? [{ href: '/settings/invitations', label: 'Invite', icon: 'user-plus' as const }] : [])
	]);

	async function loadUsage() {
		const result = await trackUsage(storageUsage());
		if (result.stale) return;
		if (result.value.ok) usage = result.value.data;
		else usageError = result.value.error;
	}

	/** Top-level folders, then what changed last directly inside them. */
	async function loadRecent() {
		const result = await trackRecent(
			listFolder(null).then(async (top) => {
				if (!top.ok) return { top, children: [] };
				const children = await Promise.all(top.data.map((root) => listFolder(root.id)));
				return { top, children };
			})
		);
		if (result.stale) return;
		const { top, children } = result.value;
		if (!top.ok) {
			recentError = top.error;
			return;
		}
		roots = top.data;
		const failed = children.find((listing) => !listing.ok);
		if (failed && !failed.ok) recentError = failed.error;
		else recentError = null;
		recent = recentItems(
			children.flatMap((listing) => (listing.ok ? listing.data : [])),
			RECENT_COUNT
		);
	}

	function chosen(event: Event) {
		const input = event.currentTarget as HTMLInputElement;
		const files = Array.from(input.files ?? []);
		input.value = '';
		if (target && files.length > 0) enqueue(files, target.id);
	}

	// After uploads, thumbnails appear once processing is done: refresh the list then, once.
	const poller = createPoller({
		fetch: mediaStatus,
		onCount: () => {},
		onSettled: () => void loadRecent(),
		visible: () => document.visibilityState === 'visible'
	});

	onMount(() => {
		void loadUsage();
		void loadRecent();
		const unsubscribe = onUploadsSettled(() => {
			void loadUsage();
			void loadRecent();
			poller.start({ assumePending: true });
		});
		return () => {
			unsubscribe();
			poller.stop();
		};
	});
</script>

<svelte:head><title>Home · Squirrel</title></svelte:head>

{#if session.me}
	<main class="home">
		<div class="col">
			<h1 class="hello arrive" style:--i={0}>Hello, {session.me.display_name}</h1>

			<section class="storage arrive" style:--i={1} aria-labelledby="storage-title">
				<div class="storage-top">
					<div class="storage-text" aria-live="polite">
						<h2 id="storage-title">
							{#if percent !== null}You've used {percent}% of your space
							{:else if usage}{formatBytes(usage.used_bytes)} stored
							{:else}Storage{/if}
						</h2>
						{#if usage}
							<p class="figures tabular">
								{storageSummary(usage)}{#if usage.trash_bytes > 0}<span class="sep" aria-hidden="true">·</span
									>{formatBytes(usage.trash_bytes)} in trash{/if}
							</p>
						{:else if usageError}
							<p class="figures">Storage use is unavailable right now.</p>
						{:else}
							<p class="figures">Loading…</p>
						{/if}
					</div>
					<label class="fab" class:disabled={!target} title={target ? `Upload files to ${target.name}` : undefined}>
						<input
							type="file"
							multiple
							class="visually-hidden"
							disabled={!target}
							aria-label={target ? `Upload files to ${target.name}` : 'Upload files'}
							onchange={chosen}
						/>
						<Icon name="upload" />
					</label>
				</div>
				{#if usage && usage.quota_bytes != null}
					<Meter label="Storage used" value={usage.used_bytes} max={usage.quota_bytes} valueText={storageSummary(usage)} />
				{/if}
			</section>

			<nav class="quick arrive" style:--i={2} aria-label="Shortcuts">
				{#each quick as link (link.href)}
					<a class="q" class:wide-only={link.wideOnly} href={link.href}>
						<span class="q-well"><Icon name={link.icon} /></span>
						<span class="q-label">{link.label}</span>
					</a>
				{/each}
			</nav>
		</div>

		<section class="col recent arrive" style:--i={3} aria-labelledby="recent-title">
			<div class="recent-head">
				<h2 id="recent-title">Recent</h2>
				<a class="all" href="/files">All files<Icon name="arrow-right" /></a>
			</div>
			{#if recentError}
				<p class="note">Recent files are unavailable right now.</p>
			{:else if recent === null}
				<p class="note">Loading…</p>
			{:else if recent.length === 0}
				<p class="note">Nothing here yet. Upload your first files with the round button.</p>
			{:else}
				<ul class="rows">
					{#each recent as node, index (node.id)}
						<ListRow arrive={index + 4}>
							{#snippet icon()}<NodeVisual {node} />{/snippet}
							{#snippet title()}
								<a class="row-link" href={`/files/${node.kind === 'folder' ? node.id : node.parent_id}`}>{node.name}</a>
							{/snippet}
							{#snippet meta()}
								{node.kind === 'folder' ? 'Folder' : formatBytes(node.size)}{#if rootName(node.parent_id)}<span
										class="sep"
										aria-hidden="true">·</span
									>in {rootName(node.parent_id)}{/if}<span class="sep" aria-hidden="true">·</span>{formatDate(node.modified_at)}
							{/snippet}
						</ListRow>
					{/each}
				</ul>
				<p class="note">Latest changes at the top of your folders.</p>
			{/if}
		</section>
	</main>
{/if}

<style>
	.home {
		display: grid;
		gap: var(--space-5);
		align-items: start;
	}

	@media (min-width: 64rem) {
		.home {
			grid-template-columns: 1.15fr 1fr;
			column-gap: var(--space-6);
		}
	}

	.col {
		display: grid;
		gap: var(--space-4);
		align-content: start;
		min-width: 0;
	}

	.hello {
		padding-block: var(--space-2);
		font-size: var(--text-display);
		line-height: var(--text-display-lh);
		letter-spacing: var(--track-tight);
		overflow-wrap: anywhere;
	}

	/* Storage: a tinted well with the orchid upload button and the pill meter. */
	.storage {
		display: grid;
		gap: var(--space-4);
		padding: var(--well-pad);
		border-radius: var(--radius-well);
		background: var(--tint);
	}

	.storage-top {
		display: flex;
		align-items: flex-start;
		justify-content: space-between;
		gap: var(--space-3);
	}

	.storage-text {
		display: grid;
		gap: var(--space-1);
		min-width: 0;
	}

	.storage h2 {
		font-size: var(--text-lg);
		line-height: var(--text-lg-lh);
		font-weight: var(--weight-label);
		letter-spacing: var(--track-label);
	}

	.figures {
		color: var(--ink-muted);
		font-variant-numeric: tabular-nums;
		font-size: var(--text-sm);
		line-height: var(--text-sm-lh);
	}

	.fab {
		position: relative;
		display: grid;
		place-items: center;
		width: var(--fab);
		height: var(--fab);
		flex: none;
		border-radius: var(--radius-full);
		background: var(--accent);
		color: var(--on-accent);
		box-shadow: var(--glow);
		cursor: pointer;
		transition:
			box-shadow var(--dur) var(--ease),
			transform var(--dur-fast) var(--ease);
	}

	@media (hover: hover) {
		.fab:not(.disabled):hover {
			box-shadow: var(--glow-hover);
		}
	}

	.fab:has(input:focus-visible) {
		outline: var(--ring-width) solid var(--accent);
		outline-offset: var(--ring-offset);
	}

	.fab:not(.disabled):active {
		transform: translateY(var(--press));
	}

	.fab.disabled {
		opacity: var(--disabled-opacity);
		box-shadow: none;
		cursor: not-allowed;
	}

	/* Shortcuts: tinted pills holding white round icon wells. */
	.quick {
		display: grid;
		grid-template-columns: repeat(auto-fit, minmax(var(--quick-min), 1fr));
		gap: var(--space-2);
	}

	.q {
		display: grid;
		justify-items: center;
		gap: var(--space-2);
		min-width: 0;
		padding: var(--space-3) 0;
		border-radius: var(--radius-full);
		background: var(--tint);
		color: var(--ink);
		font-size: var(--text-xs);
		line-height: var(--text-xs-lh);
		font-weight: var(--weight-label);
		text-decoration: none;
		transition:
			background-color var(--dur-fast) var(--ease),
			transform var(--dur-fast) var(--ease);
	}

	.q-well {
		display: grid;
		place-items: center;
		width: var(--well-md);
		height: var(--well-md);
		border-radius: var(--radius-full);
		background: var(--card);
		box-shadow: var(--shadow-card);
	}

	.sep {
		margin-inline: var(--space-1);
	}

	.q-label {
		max-width: 100%;
		overflow: hidden;
		text-overflow: ellipsis;
		white-space: nowrap;
	}

	@media (max-width: 47.99rem) {
		.wide-only {
			display: none;
		}
	}

	@media (min-width: 48rem) {
		.q {
			font-size: var(--text-sm);
			line-height: var(--text-sm-lh);
		}
	}

	@media (hover: hover) {
		.q:hover {
			background: var(--tint-strong);
		}
	}

	.q:focus-visible {
		outline: var(--ring-width) solid var(--accent);
		outline-offset: var(--ring-offset);
	}

	.q:active {
		transform: translateY(var(--press));
	}

	/* Recent: a tinted well of white row cards. */
	.recent {
		gap: var(--space-3);
		padding: var(--well-pad);
		border-radius: var(--radius-well);
		background: var(--tint);
	}

	.recent-head {
		display: flex;
		align-items: center;
		justify-content: space-between;
		gap: var(--space-3);
	}

	.all {
		display: inline-flex;
		align-items: center;
		gap: var(--space-1);
		min-height: var(--tap);
		padding-inline: var(--space-3);
		border-radius: var(--radius-full);
		color: var(--ink);
		font-size: var(--text-sm);
		font-weight: var(--weight-label);
		text-decoration: none;
		transition:
			background-color var(--dur-fast) var(--ease),
			transform var(--dur-fast) var(--ease);
	}

	@media (hover: hover) {
		.all:hover {
			background: var(--tint-strong);
		}
	}

	.all:active {
		transform: translateY(var(--press));
	}

	.rows {
		display: grid;
		gap: var(--space-2);
		margin: 0;
		padding: 0;
		list-style: none;
	}

	.note {
		padding-inline: var(--space-2);
		color: var(--ink-muted);
		font-size: var(--text-sm);
		line-height: var(--text-sm-lh);
	}
</style>
