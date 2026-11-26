<script lang="ts">
	import SharedAlbum from '#lib/components/sharing/SharedAlbum.svelte';
	import SharedNodeRow from '#lib/components/sharing/SharedNodeRow.svelte';
	import Alert from '#lib/components/ui/Alert.svelte';
	import type { FileNode } from '#lib/files/api.ts';
	import type { SharedItem } from '#lib/sharing/api.ts';
	import { splitShared } from '#lib/sharing/logic.ts';

	interface Props {
		/** `null` while loading. */
		items: SharedItem[] | null;
		error: string | null;
		onOpen: (node: FileNode) => void;
		onLeave: (item: SharedItem) => void;
	}

	let { items, error, onOpen, onLeave }: Props = $props();

	const groups = $derived(splitShared(items ?? []));
</script>

<section class="part" aria-labelledby="shared-with-me">
	<h2 id="shared-with-me">Shared with me</h2>
	{#if error}<Alert>{error}</Alert>{/if}
	{#if items === null}
		{#if !error}<p class="muted" aria-live="polite">Loading…</p>{/if}
	{:else if items.length === 0}
		<p class="muted">When someone shares a folder, a file or an album with you, it shows up here.</p>
	{:else}
		{#if groups.nodes.length > 0}
			<section class="group" aria-labelledby="shared-with-me-files">
				<h3 id="shared-with-me-files">Folders and files</h3>
				<ul class="rows" aria-label="Folders and files shared with me">
					{#each groups.nodes as item, index (item.share_id)}
						{#if item.node}
							<SharedNodeRow {item} node={item.node} arrive={index < 8 ? index : undefined} {onOpen} onLeave={() => onLeave(item)} />
						{/if}
					{/each}
				</ul>
			</section>
		{/if}
		{#if groups.albums.length > 0}
			<section class="group" aria-labelledby="shared-with-me-albums">
				<h3 id="shared-with-me-albums">Albums</h3>
				<ul class="albums" aria-label="Albums shared with me">
					{#each groups.albums as item (item.share_id)}
						{#if item.album}
							<SharedAlbum {item} album={item.album} onLeave={() => onLeave(item)} />
						{/if}
					{/each}
				</ul>
			</section>
		{/if}
	{/if}
</section>

<style>
	.part {
		display: grid;
		gap: var(--space-4);
	}

	.group {
		display: grid;
		gap: var(--space-3);
	}

	h2 {
		font-size: var(--text-xl);
		line-height: var(--text-xl-lh);
		font-weight: var(--weight-heading);
		letter-spacing: var(--track-heading);
	}

	h3 {
		font-size: var(--text-lg);
		line-height: var(--text-lg-lh);
		font-weight: var(--weight-heading);
		letter-spacing: var(--track-heading);
	}

	.rows {
		display: grid;
		gap: var(--space-2);
		margin: 0;
		padding: 0;
		list-style: none;
	}

	.albums {
		display: grid;
		grid-template-columns: repeat(2, minmax(0, 1fr));
		gap: var(--space-3);
		margin: 0;
		padding: 0;
		list-style: none;
	}

	@media (min-width: 48rem) {
		.albums {
			grid-template-columns: repeat(auto-fill, minmax(var(--album-min), 1fr));
			gap: var(--space-4);
		}
	}
</style>
