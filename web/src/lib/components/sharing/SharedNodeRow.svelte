<script lang="ts">
	import NodeVisual from '#lib/components/files/NodeVisual.svelte';
	import Badge from '#lib/components/ui/Badge.svelte';
	import Button from '#lib/components/ui/Button.svelte';
	import ListRow from '#lib/components/ui/ListRow.svelte';
	import type { FileNode } from '#lib/files/api.ts';
	import type { SharedItem } from '#lib/sharing/api.ts';
	import { memberName, permissionLabel } from '#lib/sharing/logic.ts';

	interface Props {
		item: SharedItem;
		node: FileNode;
		arrive?: number;
		onOpen: (node: FileNode) => void;
		onLeave: () => void;
	}

	let { item, node, arrive, onOpen, onLeave }: Props = $props();
</script>

<ListRow nodeId={node.id} {arrive} testId="shared-item">
	{#snippet icon()}<NodeVisual {node} />{/snippet}
	{#snippet title()}
		<button type="button" class="row-link" onclick={() => onOpen(node)}>{node.name}</button>
	{/snippet}
	{#snippet meta()}
		{node.kind === 'folder' ? 'Folder' : 'File'}<span class="sep" aria-hidden="true">·</span>{memberName(item.owner)}
	{/snippet}
	{#snippet actions()}
		<Badge>{permissionLabel(item.permission)}</Badge>
		<Button variant="quiet" size="sm" aria-label={`Leave ${node.name}`} onclick={onLeave}>Leave</Button>
	{/snippet}
</ListRow>
