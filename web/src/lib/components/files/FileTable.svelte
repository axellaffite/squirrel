<script lang="ts">
	import { untrack } from 'svelte';
	import NodeVisual from '#lib/components/files/NodeVisual.svelte';
	import Icon from '#lib/components/icons/Icon.svelte';
	import IconButton from '#lib/components/ui/IconButton.svelte';
	import Badge from '#lib/components/ui/Badge.svelte';
	import ListRow from '#lib/components/ui/ListRow.svelte';
	import type { FileNode } from '#lib/files/api.ts';
	import { editorUrl, isEditable } from '#lib/files/editable.ts';
	import { contentUrl, formatBytes, formatDate, kindLabel } from '#lib/files/format.ts';
	import { DEFAULT_SORT, nextSort, sortLabel, sortNodes, type Sort, type SortKey } from '#lib/files/sort.ts';

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
		/** Rename, move and trash are offered (false in a folder shared read-only). */
		canEdit?: boolean;
		/** Offers "Share" (owners only). */
		onShare?: (node: FileNode) => void;
	}

	let { items, selectedId = null, onOpen, onRename, onMove, onDelete, onActions, canEdit = true, onShare }: Props =
		$props();
	const editable = (node: FileNode) => canEdit && !node.is_root;

	// Rows present when the view loads arrive once; rows added later just appear.
	const arriving = new Set(untrack(() => items.slice(0, 8).map((node) => node.id)));
	const arrival = (node: FileNode, index: number) => (arriving.has(node.id) ? index : undefined);

	/** By name A to Z (folders first) until a header is clicked. */
	let sort: Sort = $state(DEFAULT_SORT);
	const sorted = $derived(sortNodes(items, sort));
	const COLUMNS: { key: SortKey; label: string }[] = [
		{ key: 'name', label: 'Name' },
		{ key: 'kind', label: 'Kind' },
		{ key: 'size', label: 'Size' },
		{ key: 'modified', label: 'Modified' }
	];
</script>

<!-- MASTER.md › "Table": a header band, then rows on the table's columns (subgrid, ≥ 768 px). -->
<div class="table">
<div class="thead">
	<span></span>
	{#each COLUMNS as column (column.key)}
		<button
			type="button"
			class="sort"
			class:active={sort.key === column.key}
			aria-label={sortLabel(sort, column.key, column.label)}
			onclick={() => (sort = nextSort(sort, column.key))}
		>
			{column.label}
			{#if sort.key === column.key}
				<Icon name={sort.dir === 'asc' ? 'chevron-up' : 'chevron-down'} />
			{/if}
		</button>
	{/each}
	<span></span>
</div>
<ul class="files" aria-label="Files and folders">
	{#each sorted as node, index (node.id)}
		<ListRow
			nodeId={node.id}
			selected={node.id === selectedId}
			arrive={arrival(node, index)}
		>
			{#snippet icon()}<NodeVisual {node} />{/snippet}
			{#snippet title()}
				<button type="button" class="row-link" onclick={() => onOpen(node)}>{node.name}</button>
			{/snippet}
			{#snippet cells()}
				<span class="cell"><Badge>{kindLabel(node)}</Badge></span>
				<span class="cell num">{node.kind === 'folder' ? '' : formatBytes(node.size)}</span>
				<span class="cell num">{formatDate(node.modified_at)}</span>
			{/snippet}
			{#snippet meta()}
				{#if node.kind === 'folder'}Folder{:else}{formatBytes(node.size)}{/if}
				<span aria-hidden="true">·</span>
				{formatDate(node.modified_at)}
			{/snippet}
			{#snippet actions()}
				<span class="inline">
					{#if isEditable(node)}
						<IconButton
							icon="file-text"
							label={canEdit ? `Edit ${node.name}` : `Open ${node.name} in editor`}
							tooltip={canEdit ? 'Edit' : 'Open in editor'}
							href={editorUrl(node.id)}
						/>
					{/if}
					{#if node.kind === 'file'}
						<IconButton icon="download" label={`Download ${node.name}`} tooltip="Download" href={contentUrl(node.id, true)} download />
					{/if}
					{#if onShare}
						<IconButton icon="share" label={`Share ${node.name}`} tooltip="Share" onclick={() => onShare(node)} />
					{/if}
					{#if editable(node)}
						<IconButton icon="pencil" label={`Rename ${node.name}`} tooltip="Rename" onclick={() => onRename(node)} />
						<IconButton icon="folder-input" label={`Move ${node.name}`} tooltip="Move" onclick={() => onMove(node)} />
						<IconButton icon="trash" tone="danger" label={`Delete ${node.name}`} tooltip="Delete" onclick={() => onDelete(node)} />
					{/if}
				</span>
				{#if node.kind === 'file' || editable(node) || onShare}
					<span class="more">
						<IconButton icon="more" label={`Actions for ${node.name}`} tooltip="Actions" onclick={() => onActions(node)} />
					</span>
				{/if}
			{/snippet}
		</ListRow>
	{/each}
</ul>
</div>

<style>
	.table {
		border: 1px solid var(--hair);
		border-radius: var(--radius-card);
		background: var(--card);
		box-shadow: var(--shadow-card);
	}

	.thead {
		display: none;
	}

	.sort {
		display: inline-flex;
		align-items: center;
		gap: var(--space-1);
		min-height: var(--table-head);
		padding: 0;
		border: 0;
		background: none;
		color: inherit;
		font: inherit;
		text-align: start;
		cursor: pointer;
		transition: color var(--dur-fast) var(--ease);
	}

	.sort:hover,
	.sort.active {
		color: var(--ink);
	}

	.sort:focus-visible {
		outline: var(--ring-width) solid var(--accent);
		outline-offset: var(--ring-offset);
		border-radius: var(--radius-sm);
	}

	.files {
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

	.inline {
		display: none;
	}

	.more {
		display: contents;
	}

	/* Columns: icon tile, name, kind, size, modified, actions. Kind, size and date take the
	   width of their widest cell; the name takes the rest. */
	@media (min-width: 48rem) {
		.table {
			display: grid;
			grid-template-columns: var(--well-sm) minmax(0, 1fr) auto auto auto auto;
			column-gap: var(--space-4);
		}

		.thead,
		.files {
			display: grid;
			grid-column: 1 / -1;
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

		.inline {
			display: flex;
			gap: 0;
		}

		.more {
			display: none;
		}
	}

	/* Pointer devices: a row's actions show on hover or keyboard focus, so the table reads calm. */
	@media (min-width: 48rem) and (hover: hover) {
		.inline {
			opacity: 0;
			transition: opacity var(--dur-fast) var(--ease);
		}

		:global(.row:is(:hover, :focus-within, .selected)) .inline {
			opacity: 1;
		}
	}
</style>
