<script lang="ts">
	import SharedByMeAlbum from '#lib/components/sharing/SharedByMeAlbum.svelte';
	import SharedByMeRow from '#lib/components/sharing/SharedByMeRow.svelte';
	import Alert from '#lib/components/ui/Alert.svelte';
	import type { FileNode } from '#lib/files/api.ts';
	import type { SharedByMe } from '#lib/sharing/api.ts';
	import { splitShared } from '#lib/sharing/logic.ts';

	interface Props {
		/** `null` while loading. */
		items: SharedByMe[] | null;
		error: string | null;
		onOpen: (node: FileNode) => void;
		onManage: (item: SharedByMe) => void;
	}

	let { items, error, onOpen, onManage }: Props = $props();

	const groups = $derived(splitShared(items ?? []));
</script>

<section class="part" aria-labelledby="shared-by-me">
	<!-- Named by the page's tabs: the heading stays for the region's name. -->
	<h2 id="shared-by-me" class="visually-hidden">Shared by me</h2>
	{#if error}<Alert>{error}</Alert>{/if}
	{#if items === null}
		{#if !error}<p class="muted" aria-live="polite">Loading…</p>{/if}
	{:else if items.length === 0}
		<p class="muted">Share a folder, a file or an album and it shows up here, with the people you shared it with.</p>
	{:else}
		{#if groups.nodes.length > 0}
			<section class="group" aria-labelledby="shared-by-me-files">
				<h3 id="shared-by-me-files">Folders and files</h3>
				<ul class="rows" aria-label="Folders and files I shared">
					{#each groups.nodes as item, index (item.node?.id)}
						{#if item.node}
							<SharedByMeRow {item} node={item.node} arrive={index < 8 ? index : undefined} {onOpen} onManage={() => onManage(item)} />
						{/if}
					{/each}
				</ul>
			</section>
		{/if}
		{#if groups.albums.length > 0}
			<section class="group" aria-labelledby="shared-by-me-albums">
				<h3 id="shared-by-me-albums">Albums</h3>
				<ul class="albums" aria-label="Albums I shared">
					{#each groups.albums as item (item.album?.id)}
						{#if item.album}
							<SharedByMeAlbum {item} album={item.album} onManage={() => onManage(item)} />
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

	/* Section labels (MASTER three tiers): small, muted. */
	h3 {
		color: var(--ink-muted);
		font-size: var(--text-sm);
		line-height: var(--text-sm-lh);
		font-weight: var(--weight-label);
		letter-spacing: 0;
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
