<script lang="ts">
	import { untrack } from 'svelte';
	import NodeVisual from '#lib/components/files/NodeVisual.svelte';
	import IconButton from '#lib/components/ui/IconButton.svelte';
	import ListRow from '#lib/components/ui/ListRow.svelte';
	import type { FileNode } from '#lib/files/api.ts';
	import { contentUrl, formatBytes, formatDate } from '#lib/files/format.ts';

	interface Props {
		items: FileNode[];
		/** The row a dialog is acting on, highlighted. */
		selectedId?: string | null;
		onOpen: (node: FileNode) => void;
		onRename: (node: FileNode) => void;
		onMove: (node: FileNode) => void;
		onDelete: (node: FileNode) => void;
		/** Phones: one button per row opens an actions sheet. */
		onActions: (node: FileNode) => void;
	}

	let { items, selectedId = null, onOpen, onRename, onMove, onDelete, onActions }: Props = $props();

	// Rows present when the view loads arrive once; rows added later just appear.
	const arriving = new Set(untrack(() => items.slice(0, 8).map((node) => node.id)));
	const arrival = (node: FileNode, index: number) => (arriving.has(node.id) ? index : undefined);
</script>

<ul class="files" aria-label="Files and folders">
	{#each items as node, index (node.id)}
		<ListRow
			nodeId={node.id}
			selected={node.id === selectedId}
			arrive={arrival(node, index)}
		>
			{#snippet icon()}<NodeVisual {node} />{/snippet}
			{#snippet title()}
				<button type="button" class="row-link" onclick={() => onOpen(node)}>{node.name}</button>
			{/snippet}
			{#snippet meta()}
				{#if node.kind === 'folder'}Folder{:else}{formatBytes(node.size)}{/if}
				<span aria-hidden="true">·</span>
				{formatDate(node.modified_at)}
			{/snippet}
			{#snippet actions()}
				<span class="inline">
					{#if node.kind === 'file'}
						<IconButton icon="download" label={`Download ${node.name}`} tooltip="Download" href={contentUrl(node.id, true)} download />
					{/if}
					{#if !node.is_root}
						<IconButton icon="pencil" label={`Rename ${node.name}`} tooltip="Rename" onclick={() => onRename(node)} />
						<IconButton icon="folder-input" label={`Move ${node.name}`} tooltip="Move" onclick={() => onMove(node)} />
						<IconButton icon="trash" tone="danger" label={`Delete ${node.name}`} tooltip="Delete" onclick={() => onDelete(node)} />
					{/if}
				</span>
				{#if node.kind === 'file' || !node.is_root}
					<span class="more">
						<IconButton icon="more" label={`Actions for ${node.name}`} tooltip="Actions" onclick={() => onActions(node)} />
					</span>
				{/if}
			{/snippet}
		</ListRow>
	{/each}
</ul>

<style>
	.files {
		display: grid;
		gap: var(--space-2);
		margin: 0;
		padding: 0;
		list-style: none;
	}

	.inline {
		display: none;
	}

	.more {
		display: contents;
	}

	@media (min-width: 48rem) {
		.inline {
			display: flex;
			gap: 0;
		}

		.more {
			display: none;
		}
	}
</style>
