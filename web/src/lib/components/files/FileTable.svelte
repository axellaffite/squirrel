<script lang="ts">
	import type { FileNode } from '#lib/files/api.ts';
	import { contentUrl, formatBytes, formatDate } from '#lib/files/format.ts';

	let {
		items,
		onOpen,
		onRename,
		onMove,
		onDelete
	}: {
		items: FileNode[];
		onOpen: (node: FileNode) => void;
		onRename: (node: FileNode) => void;
		onMove: (node: FileNode) => void;
		onDelete: (node: FileNode) => void;
	} = $props();
</script>

<table class="files-table">
	<thead>
		<tr><th>Name</th><th>Size</th><th>Modified</th><th><span class="visually-hidden">Actions</span></th></tr>
	</thead>
	<tbody>
		{#each items as node (node.id)}
			<tr data-node-id={node.id}>
				<td>
					<button class="link name" onclick={() => onOpen(node)}>
						<span aria-hidden="true">{node.kind === 'folder' ? '📁' : '📄'}</span>
						{node.name}
					</button>
				</td>
				<td>{node.kind === 'folder' ? '—' : formatBytes(node.size)}</td>
				<td>{formatDate(node.modified_at)}</td>
				<td class="row-actions">
					{#if node.kind === 'file'}<a href={contentUrl(node.id, true)} download>Download</a>{/if}
					{#if !node.is_root}
						<button class="link" onclick={() => onRename(node)}>Rename</button>
						<button class="link" onclick={() => onMove(node)}>Move</button>
						<button class="link danger" onclick={() => onDelete(node)}>Delete</button>
					{/if}
				</td>
			</tr>
		{:else}
			<tr><td colspan="4" class="muted">This folder is empty. Drop files here or use “Upload files”.</td></tr>
		{/each}
	</tbody>
</table>
