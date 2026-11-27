<script lang="ts">
	import Icon from '#lib/components/icons/Icon.svelte';
	import Alert from '#lib/components/ui/Alert.svelte';
	import Button from '#lib/components/ui/Button.svelte';
	import EmptyState from '#lib/components/ui/EmptyState.svelte';
	import ListRow from '#lib/components/ui/ListRow.svelte';
	import { formatBytes } from '#lib/files/format.ts';
	import { nodeIcon } from '#lib/files/nodeIcon.ts';
	import { listFolder, thumbUrl, zipUrl, type FolderListing, type PublicNode } from '#lib/public/api.ts';
	import { DEFAULT_SORT, sortNodes } from '#lib/files/sort.ts';
	import { fileOfNode, MAX_ZIP_FILES, zippable, type ViewedFile } from '#lib/public/view.ts';

	interface Props {
		token: string;
		onOpen: (file: ViewedFile) => void;
		/** The link closed meanwhile (revoked, expired) or got locked. */
		onGone: () => void;
		/** How many files "Download all" would zip (read by the page header). */
		zipCount?: number;
		/** A zip is being prepared. */
		zipping?: boolean;
	}

	let { token, onOpen, onGone, zipCount = $bindable(0), zipping = $bindable(false) }: Props = $props();

	/** The folder shown: `null` is the link's own folder. */
	let folderId: string | null = $state(null);
	let listing: FolderListing | null = $state(null);
	let items: PublicNode[] = $state([]);
	let nextCursor: string | null = $state(null);
	let error: string | null = $state(null);
	let loading = $state(false);
	const files = $derived(zippable(items));
	/** Folders first, then files, by name. */
	const sorted = $derived(sortNodes(items, DEFAULT_SORT));

	$effect(() => {
		zipCount = files.length;
	});

	async function load(id: string | null, cursor: string | null) {
		loading = true;
		error = null;
		const result = await listFolder(token, id, cursor);
		loading = false;
		if (!result.ok) {
			if (result.gone || result.locked) onGone();
			else error = result.error;
			return;
		}
		listing = result.data;
		items = cursor ? [...items, ...result.data.items] : result.data.items;
		nextCursor = result.data.next_cursor ?? null;
	}

	$effect(() => {
		void load(folderId, null);
	});

	function enter(node: PublicNode) {
		if (node.kind === 'folder') folderId = node.id;
		else onOpen(fileOfNode(node));
	}

	export async function downloadAll() {
		zipping = true;
		error = null;
		const result = await zipUrl(token, files.slice(0, MAX_ZIP_FILES));
		zipping = false;
		if (result.ok) window.location.assign(result.data);
		else error = result.error;
	}
</script>

{#if listing}
	<div class="bar">
		<nav aria-label="Folder path">
			<ol class="crumbs">
				{#each listing.path as segment, index (segment.id)}
					<li>
						<button class="crumb" type="button" onclick={() => (folderId = index === 0 ? null : segment.id)}>{segment.name}</button>
						<Icon name="chevron-right" />
					</li>
				{/each}
				<li aria-current="page">{listing.folder.name}</li>
			</ol>
		</nav>
	</div>
{/if}

{#if error}<Alert>{error}</Alert>{/if}

{#if listing && items.length === 0 && !loading}
	<EmptyState icon="folder" title="This folder is empty" />
{:else}
	<ul class="rows" aria-label="Files">
		{#each sorted as node (node.id)}
			<ListRow testId="public-node" nodeId={node.id}>
				{#snippet icon()}
					{#if node.thumbnail}
						<img class="thumb" src={thumbUrl(token, node.id, node.thumbnail.version)} alt="" loading="lazy" />
					{:else}
						<Icon name={nodeIcon(node)} />
					{/if}
				{/snippet}
				{#snippet title()}<button class="row-link" type="button" onclick={() => enter(node)}>{node.name}</button>{/snippet}
				{#snippet meta()}{node.kind === 'folder' ? 'Folder' : formatBytes(node.size)}{/snippet}
			</ListRow>
		{/each}
	</ul>
	{#if nextCursor}
		<Button variant="secondary" disabled={loading} onclick={() => load(folderId, nextCursor)}>Show more</Button>
	{/if}
{/if}

<style>
	.bar {
		display: flex;
		flex-wrap: wrap;
		align-items: center;
		justify-content: space-between;
		gap: var(--space-3);
	}

	.crumbs {
		display: flex;
		flex-wrap: wrap;
		align-items: center;
		gap: var(--space-1);
		margin: 0;
		padding: 0;
		list-style: none;
		color: var(--ink-muted);
	}

	.crumbs li {
		display: inline-flex;
		align-items: center;
		gap: var(--space-1);
	}

	.crumbs [aria-current] {
		color: var(--ink);
		font-weight: var(--weight-label);
	}

	.crumb {
		min-height: var(--tap);
		padding: 0;
		border: 0;
		background: none;
		color: inherit;
		font: inherit;
		cursor: pointer;
	}

	.crumb:hover {
		color: var(--ink);
	}

	.crumb:focus-visible {
		outline: var(--ring-width) solid var(--accent);
		outline-offset: var(--ring-offset);
		border-radius: var(--radius-sm);
	}

	.rows {
		display: grid;
		gap: var(--space-2);
		margin: 0;
		padding: 0;
		list-style: none;
	}

	.thumb {
		width: 100%;
		height: 100%;
		object-fit: cover;
		border-radius: var(--radius-tile);
	}
</style>
