<script lang="ts">
	import { untrack } from 'svelte';
	import Icon from '#lib/components/icons/Icon.svelte';
	import type { IconName } from '#lib/components/icons/paths.ts';
	import Dialog from '#lib/components/ui/Dialog.svelte';
	import type { FileNode } from '#lib/files/api.ts';
	import { contentUrl } from '#lib/files/format.ts';

	interface Props {
		node: FileNode;
		/** Actions get the node back: the parent's own reference may be gone once the sheet closed. */
		onOpen: (node: FileNode) => void;
		onRename: (node: FileNode) => void;
		onMove: (node: FileNode) => void;
		onDelete: (node: FileNode) => void;
		/** Runs once the sheet closed, however it closed. */
		onClose: () => void;
		/** Rename, move and trash are offered (false in a folder shared read-only). */
		canEdit?: boolean;
		/** Offers "Share" (owners only). */
		onShare?: (node: FileNode) => void;
	}

	let { node, onOpen, onRename, onMove, onDelete, onClose, canEdit = true, onShare }: Props = $props();

	let dialog: Dialog | undefined = $state();
	let open = $state(true);
	/** The chosen action runs after the sheet closed, so focus and the next dialog start clean. */
	let chosen: (() => void) | null = null;
	// The node this sheet was opened for, kept for the action that runs after it closed.
	const target = untrack(() => node);

	interface Action {
		label: string;
		icon: IconName;
		run: (node: FileNode) => void;
		danger?: boolean;
	}

	const actions: Action[] = $derived([
		{ label: node.kind === 'folder' ? 'Open' : 'Preview', icon: node.kind === 'folder' ? 'folder' : 'external-link', run: onOpen },
		...(onShare ? [{ label: 'Share', icon: 'share' as const, run: onShare }] : []),
		...(node.is_root || !canEdit
			? []
			: [
					{ label: 'Rename', icon: 'pencil' as const, run: onRename },
					{ label: 'Move', icon: 'folder-input' as const, run: onMove },
					{ label: 'Move to trash', icon: 'trash' as const, run: onDelete, danger: true }
				])
	]);

	function choose(run: (node: FileNode) => void) {
		chosen = () => run(target);
		dialog?.close();
	}

	function closed() {
		onClose();
		chosen?.();
	}
</script>

<Dialog bind:this={dialog} bind:open title={node.name} onclose={closed}>
	<ul class="actions">
		{#if node.kind === 'file'}
			<li>
				<a class="action" href={contentUrl(node.id, true)} download aria-label={`Download ${node.name}`}>
					<span class="well"><Icon name="download" /></span>Download
				</a>
			</li>
		{/if}
		{#each actions as action (action.label)}
			<li>
				<button
					type="button"
					class="action"
					class:danger={action.danger}
					aria-label={`${action.label} ${node.name}`}
					onclick={() => choose(action.run)}
				>
					<span class="well"><Icon name={action.icon} /></span>{action.label}
				</button>
			</li>
		{/each}
	</ul>
</Dialog>

<style>
	.actions {
		display: grid;
		gap: var(--space-1);
		margin: 0;
		padding: var(--space-2);
		list-style: none;
		border-radius: var(--radius-well);
		background: var(--tint);
	}

	.action {
		display: flex;
		align-items: center;
		gap: var(--space-3);
		width: 100%;
		min-height: var(--row-min);
		padding: var(--space-2);
		border: 0;
		border-radius: var(--radius-card);
		background: transparent;
		color: var(--ink);
		font-size: var(--text-md);
		font-weight: var(--weight-label);
		text-align: left;
		text-decoration: none;
		cursor: pointer;
		transition:
			background-color var(--dur-fast) var(--ease),
			box-shadow var(--dur) var(--ease),
			transform var(--dur-fast) var(--ease);
	}

	.danger {
		color: var(--danger);
	}

	.well {
		display: grid;
		place-items: center;
		width: var(--well-sm);
		height: var(--well-sm);
		flex: none;
		border-radius: var(--radius-full);
		background: var(--card);
		box-shadow: var(--shadow-card);
	}

	@media (hover: hover) {
		.action:hover {
			background: var(--card);
			box-shadow: var(--shadow-card);
		}
	}

	.action:focus-visible {
		outline: var(--ring-width) solid var(--accent);
		outline-offset: calc(-1 * var(--ring-width));
	}

	.action:active {
		transform: translateY(var(--press));
	}
</style>
