<script lang="ts">
	import NodeVisual from '#lib/components/files/NodeVisual.svelte';
	import Button from '#lib/components/ui/Button.svelte';
	import ListRow from '#lib/components/ui/ListRow.svelte';
	import type { FileNode } from '#lib/files/api.ts';
	import type { SharedByMe } from '#lib/sharing/api.ts';
	import { sharedWithSummary } from '#lib/sharing/logic.ts';

	interface Props {
		item: SharedByMe;
		node: FileNode;
		arrive?: number;
		onOpen: (node: FileNode) => void;
		onManage: () => void;
	}

	let { item, node, arrive, onOpen, onManage }: Props = $props();
</script>

<ListRow nodeId={node.id} {arrive} testId="shared-by-me-item">
	{#snippet icon()}<NodeVisual {node} />{/snippet}
	{#snippet title()}
		<button type="button" class="row-link" onclick={() => onOpen(node)}>{node.name}</button>
	{/snippet}
	{#snippet meta()}
		{node.kind === 'folder' ? 'Folder' : 'File'}<span class="sep" aria-hidden="true">·</span>{sharedWithSummary(
			item.people.map((share) => share.person)
		)}
	{/snippet}
	{#snippet actions()}
		<Button variant="quiet" size="sm" aria-label={`Manage sharing of ${node.name}`} onclick={onManage}>Manage</Button>
	{/snippet}
</ListRow>
