<script lang="ts">
	import { goto } from '$app/navigation';
	import { onMount } from 'svelte';
	import ConfirmDialog from '#lib/components/ConfirmDialog.svelte';
	import Preview from '#lib/components/files/Preview.svelte';
	import Page from '#lib/components/shell/Page.svelte';
	import SharedAlbum from '#lib/components/sharing/SharedAlbum.svelte';
	import SharedNodeRow from '#lib/components/sharing/SharedNodeRow.svelte';
	import Alert from '#lib/components/ui/Alert.svelte';
	import EmptyState from '#lib/components/ui/EmptyState.svelte';
	import type { FileNode } from '#lib/files/api.ts';
	import { deleteShare, sharedWithMe, type SharedItem } from '#lib/sharing/api.ts';
	import { memberName, splitShared } from '#lib/sharing/logic.ts';

	let items: SharedItem[] | null = $state(null);
	let error: string | null = $state(null);
	let leaving: SharedItem | null = $state(null);
	let previewing: FileNode | null = $state(null);

	const groups = $derived(splitShared(items ?? []));
	/** Files shared for writing (directly or through a folder above) open in the editor as "Edit". */
	const canEditShared = (node: FileNode) => items?.find((item) => item.node?.id === node.id)?.permission === 'write';
	const subjectName = (item: SharedItem) => item.node?.name ?? item.album?.name ?? '';

	async function load() {
		const result = await sharedWithMe();
		if (result.ok) {
			items = result.data;
			error = null;
		} else error = result.error;
	}

	function open(node: FileNode) {
		if (node.kind === 'folder') void goto(`/files/${node.id}`);
		else previewing = node;
	}

	async function leave(item: SharedItem): Promise<string | null> {
		const result = await deleteShare(item.share_id);
		if (!result.ok) return result.error;
		items = (items ?? []).filter((entry) => entry.share_id !== item.share_id);
		return null;
	}

	onMount(load);
</script>

<svelte:head><title>Shared · Squirrel</title></svelte:head>

<Page title="Shared">
	{#snippet description()}Folders, files and albums other people shared with you.{/snippet}

	{#if error}<Alert>{error}</Alert>{/if}
	{#if items === null}
		{#if !error}<p class="muted" aria-live="polite">Loading…</p>{/if}
	{:else if items.length === 0}
		<EmptyState icon="users" title="Nothing shared with you yet">
			When someone shares a folder, a file or an album with you, it shows up here.
		</EmptyState>
	{:else}
		{#if groups.nodes.length > 0}
			<section class="group" aria-labelledby="shared-files">
				<h2 id="shared-files">Folders and files</h2>
				<ul class="rows" aria-label="Shared folders and files">
					{#each groups.nodes as item, index (item.share_id)}
						{#if item.node}
							<SharedNodeRow {item} node={item.node} arrive={index < 8 ? index : undefined} onOpen={open} onLeave={() => (leaving = item)} />
						{/if}
					{/each}
				</ul>
			</section>
		{/if}
		{#if groups.albums.length > 0}
			<section class="group" aria-labelledby="shared-albums">
				<h2 id="shared-albums">Albums</h2>
				<ul class="albums" aria-label="Shared albums">
					{#each groups.albums as item (item.share_id)}
						{#if item.album}
							<SharedAlbum {item} album={item.album} onLeave={() => (leaving = item)} />
						{/if}
					{/each}
				</ul>
			</section>
		{/if}
	{/if}
</Page>

{#if leaving}
	{@const item = leaving}
	<ConfirmDialog title={`Leave “${subjectName(item)}”?`} confirmLabel="Leave" onConfirm={() => leave(item)} onClose={() => (leaving = null)}>
		You will no longer see it. {memberName(item.owner)} can share it with you again.
	</ConfirmDialog>
{/if}

{#if previewing}
	<Preview node={previewing} canEdit={canEditShared(previewing)} onClose={() => (previewing = null)} />
{/if}

<style>
	.group {
		display: grid;
		gap: var(--space-3);
	}

	h2 {
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
