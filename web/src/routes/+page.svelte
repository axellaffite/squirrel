<script lang="ts">
	import { onMount } from 'svelte';
	import NameDialog from '#lib/components/files/NameDialog.svelte';
	import NodeVisual from '#lib/components/files/NodeVisual.svelte';
	import Icon from '#lib/components/icons/Icon.svelte';
	import type { IconName } from '#lib/components/icons/paths.ts';
	import Page from '#lib/components/shell/Page.svelte';
	import Badge from '#lib/components/ui/Badge.svelte';
	import Button from '#lib/components/ui/Button.svelte';
	import ListRow from '#lib/components/ui/ListRow.svelte';
	import Meter from '#lib/components/ui/Meter.svelte';
	import { createFolder, listFolder, storageUsage, type FileNode, type StorageUsage } from '#lib/files/api.ts';
	import { formatBytes, formatDate, kindLabel, storageSummary } from '#lib/files/format.ts';
	import { latestOnly } from '#lib/files/latest.ts';
	import { recentItems, uploadTarget } from '#lib/home.ts';
	import { listMonths, mediaStatus } from '#lib/photos/api.ts';
	import { sharedWithMe } from '#lib/sharing/api.ts';
	import { createPoller } from '#lib/photos/status.ts';
	import { session } from '#lib/session.svelte.ts';
	import { enqueue, onUploadsSettled } from '#lib/upload/uploads.svelte.ts';

	/** How many recently changed items the home page lists. */
	const RECENT_COUNT = 5;
	const count = new Intl.NumberFormat();

	let usage = $state<StorageUsage | null>(null);
	let usageError: string | null = $state(null);
	let roots: FileNode[] = $state([]);
	let recent: FileNode[] | null = $state(null);
	let recentError: string | null = $state(null);
	/** Real counts only (MASTER rule 6): null until loaded, and the card shows no figure then. */
	let photoCount: number | null = $state(null);
	let driveCount: number | null = $state(null);
	let sharedCount: number | null = $state(null);
	let newFolderOpen = $state(false);
	let fileInput: HTMLInputElement | undefined = $state();
	const trackUsage = latestOnly();
	const trackRecent = latestOnly();

	const target = $derived(uploadTarget(roots));
	const percent = $derived(
		usage?.quota_bytes ? Math.min(100, Math.round((usage.used_bytes / usage.quota_bytes) * 100)) : null
	);
	const rootName = (id: string | null | undefined) => roots.find((root) => root.id === id)?.name;

	interface Overview {
		href: string;
		label: string;
		icon: IconName;
		line: string | null;
	}

	const plural = (n: number, one: string, many: string) => `${count.format(n)} ${n === 1 ? one : many}`;
	const overview: Overview[] = $derived([
		{ href: '/photos', label: 'Photos', icon: 'images', line: photoCount === null ? null : plural(photoCount, 'photo', 'photos') },
		{ href: '/files', label: 'Files', icon: 'folder', line: driveCount === null ? null : plural(driveCount, 'item', 'items') },
		{ href: '/shared', label: 'Shared', icon: 'users', line: sharedCount === null ? null : `${count.format(sharedCount)} shared with you` }
	]);

	async function loadCounts() {
		const [months, shared] = await Promise.all([listMonths(), sharedWithMe()]);
		if (months.ok) photoCount = months.data.reduce((sum, m) => sum + m.count, 0);
		if (shared.ok) sharedCount = shared.data.length;
	}

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
		driveCount = children.reduce((sum, listing) => sum + (listing.ok ? listing.data.length : 0), 0);
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
		onProgress: () => void loadRecent(),
		visible: () => document.visibilityState === 'visible'
	});

	async function newFolder(name: string): Promise<string | null> {
		if (!target) return 'There is no folder to create it in.';
		const result = await createFolder(target.id, name);
		if (!result.ok) return result.error ?? 'Something went wrong.';
		await loadRecent();
		return null;
	}

	onMount(() => {
		void loadUsage();
		void loadRecent();
		void loadCounts();
		const unsubscribe = onUploadsSettled(() => {
			void loadUsage();
			void loadRecent();
			poller.start({ assumePending: true });
		});
		// Coming back to the tab: catch up on what finished meanwhile.
		const visible = () => poller.start();
		document.addEventListener('visibilitychange', visible);
		return () => {
			unsubscribe();
			poller.stop();
			document.removeEventListener('visibilitychange', visible);
		};
	});
</script>

<svelte:head><title>Home · Squirrel</title></svelte:head>

{#if session.me}
	<Page title={`Hello, ${session.me.display_name}`}>
		{#snippet description()}Everything you keep at home, in one place.{/snippet}
		{#snippet actions()}
			<Button variant="secondary" size="sm" disabled={!target} onclick={() => (newFolderOpen = true)}>
				<Icon name="plus" />New folder
			</Button>
			<input
				bind:this={fileInput}
				type="file"
				multiple
				class="visually-hidden"
				tabindex="-1"
				aria-hidden="true"
				disabled={!target}
				onchange={chosen}
			/>
			<Button
				size="sm"
				disabled={!target}
				aria-label={target ? `Upload files to ${target.name}` : 'Upload files'}
				onclick={() => fileInput?.click()}
			>
				<Icon name="upload" />Upload
			</Button>
		{/snippet}

		<!-- Phones: the sidebar (and its storage block) is not shown, so storage stays here. -->
		{#if usage}
			<section class="storage arrive" style:--i={1} aria-label="Storage">
				<p class="figures tabular">
					{storageSummary(usage)}{#if usage.trash_bytes > 0}<span class="sep" aria-hidden="true">·</span
						>{formatBytes(usage.trash_bytes)} in trash{/if}
				</p>
				{#if usage.quota_bytes != null}
					<Meter label="Storage used" value={usage.used_bytes} max={usage.quota_bytes} valueText={storageSummary(usage)} />
				{/if}
			</section>
		{:else if usageError}
			<p class="note storage">Storage use is unavailable right now.</p>
		{/if}

		<nav class="cards arrive" style:--i={2} aria-label="Shortcuts">
			{#each overview as card (card.href)}
				<a class="card" href={card.href}>
					<span class="tile"><Icon name={card.icon} /></span>
					<span class="card-text">
						<span class="card-label">{card.label}</span>
						<span class="card-line tabular">{card.line ?? ' '}</span>
					</span>
				</a>
			{/each}
		</nav>

		<section class="recent arrive" style:--i={3} aria-labelledby="recent-title">
			<div class="recent-head">
				<h2 id="recent-title">Recent files</h2>
				<a class="all" href="/files">View all</a>
			</div>
			{#if recentError}
				<p class="note">Recent files are unavailable right now.</p>
			{:else if recent === null}
				<p class="note">Loading…</p>
			{:else if recent.length === 0}
				<p class="note">Nothing here yet. Upload your first files with the Upload button.</p>
			{:else}
				<div class="table">
					<div class="thead" aria-hidden="true">
						<span></span><span>Name</span><span>Kind</span><span>Size</span><span>Modified</span>
					</div>
					<ul class="rows">
						{#each recent as node, index (node.id)}
							<ListRow arrive={index + 4}>
								{#snippet icon()}<NodeVisual {node} />{/snippet}
								{#snippet title()}
									<a class="row-link" href={`/files/${node.kind === 'folder' ? node.id : node.parent_id}`}>{node.name}</a>
								{/snippet}
								{#snippet cells()}
									<span class="cell"><Badge>{kindLabel(node)}</Badge></span>
									<span class="cell num">{node.kind === 'folder' ? '' : formatBytes(node.size)}</span>
									<span class="cell num">{formatDate(node.modified_at)}</span>
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
					<p class="tfoot">Latest changes at the top of your folders.</p>
				</div>
			{/if}
		</section>
	</Page>

	{#if newFolderOpen}
		<NameDialog title="New folder" submitLabel="Create folder" onSubmit={newFolder} onClose={() => (newFolderOpen = false)} />
	{/if}
{/if}

<style>
	.tabular {
		font-variant-numeric: tabular-nums;
	}

	.note {
		margin: 0;
		color: var(--ink-muted);
		font-size: var(--text-sm);
	}

	.storage {
		display: grid;
		gap: var(--space-2);
	}

	.figures {
		margin: 0;
		color: var(--ink-muted);
		font-size: var(--text-sm);
	}

	.sep {
		margin-inline: var(--space-1);
	}

	@media (min-width: 64rem) {
		.storage {
			display: none;
		}
	}

	/* Overview cards: white, hairline, near-flat; an 8 px icon tile over the label and its count. */
	.cards {
		display: grid;
		grid-template-columns: repeat(3, minmax(0, 1fr));
		gap: var(--space-3);
	}

	@media (min-width: 64rem) {
		.cards {
			grid-template-columns: repeat(4, minmax(0, 1fr));
		}
	}

	.card {
		display: grid;
		gap: var(--space-4);
		padding: var(--card-pad);
		border: 1px solid var(--hair);
		border-radius: var(--radius-card);
		background: var(--card);
		box-shadow: var(--shadow-card);
		color: var(--ink);
		text-decoration: none;
		transition:
			border-color var(--dur) var(--ease),
			box-shadow var(--dur) var(--ease),
			transform var(--dur-fast) var(--ease);
	}

	@media (hover: hover) {
		.card:hover {
			border-color: var(--hair-strong);
			box-shadow: var(--shadow-hover);
		}
	}

	.card:focus-visible {
		outline: var(--ring-width) solid var(--accent);
		outline-offset: var(--ring-offset);
	}

	.card:active {
		transform: translateY(var(--press));
	}

	.tile {
		display: grid;
		place-items: center;
		width: var(--well-sm);
		height: var(--well-sm);
		border-radius: var(--radius-control);
		background: var(--tint);
	}

	.card-text {
		display: grid;
		min-width: 0;
	}

	.card-label {
		font-weight: var(--weight-label);
		letter-spacing: var(--track-label);
	}

	.card-line {
		overflow: hidden;
		text-overflow: ellipsis;
		white-space: nowrap;
		color: var(--ink-muted);
		font-size: var(--text-sm);
		line-height: var(--text-sm-lh);
	}

	.recent {
		display: grid;
		gap: var(--space-3);
		min-width: 0;
	}

	.recent-head {
		display: flex;
		align-items: baseline;
		justify-content: space-between;
		gap: var(--space-3);
	}

	.recent-head h2 {
		margin: 0;
		color: var(--ink-muted);
		font-size: var(--text-sm);
		line-height: var(--text-sm-lh);
		font-weight: var(--weight-label);
		letter-spacing: 0;
	}

	.all {
		border-radius: var(--radius-sm);
		color: var(--ink-muted);
		font-size: var(--text-sm);
		text-decoration: none;
		transition: color var(--dur-fast) var(--ease);
	}

	@media (hover: hover) {
		.all:hover {
			color: var(--ink);
		}
	}

	.all:focus-visible {
		outline: var(--ring-width) solid var(--accent);
		outline-offset: var(--ring-offset);
	}

	/* MASTER.md › "Table": header band, rows on shared columns (subgrid, ≥ 768 px), a footer line. */
	.table {
		border: 1px solid var(--hair);
		border-radius: var(--radius-card);
		background: var(--card);
		box-shadow: var(--shadow-card);
	}

	.thead {
		display: none;
	}

	.rows {
		display: grid;
		margin: 0;
		padding: 0;
		list-style: none;
	}

	.cell {
		white-space: nowrap;
	}

	.num {
		color: var(--ink-muted);
		font-size: var(--text-sm);
		line-height: var(--text-sm-lh);
		font-variant-numeric: tabular-nums;
	}

	.tfoot {
		max-width: none;
		margin: 0;
		padding: var(--space-3) var(--space-3);
		border-top: 1px solid var(--hair);
		color: var(--ink-muted);
		font-size: var(--text-sm);
		line-height: var(--text-sm-lh);
	}

	@media (min-width: 48rem) {
		.table {
			display: grid;
			grid-template-columns: var(--well-sm) minmax(0, 1fr) auto auto auto;
			column-gap: var(--space-4);
		}

		.thead,
		.rows,
		.tfoot {
			grid-column: 1 / -1;
		}

		.thead,
		.rows {
			display: grid;
			grid-template-columns: subgrid;
		}

		.thead {
			align-items: center;
			min-height: var(--table-head);
			padding: 0 var(--space-2) 0 var(--space-3);
			border-bottom: 1px solid var(--hair);
			border-radius: calc(var(--radius-card) - 1px) calc(var(--radius-card) - 1px) 0 0;
			background: var(--head);
			color: var(--ink-muted);
			font-size: var(--text-sm);
			line-height: var(--text-sm-lh);
			font-weight: var(--weight-label);
		}
	}
</style>
