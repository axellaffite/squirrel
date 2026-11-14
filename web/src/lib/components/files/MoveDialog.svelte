<script lang="ts">
	import { onMount } from 'svelte';
	import Icon from '#lib/components/icons/Icon.svelte';
	import Alert from '#lib/components/ui/Alert.svelte';
	import Button from '#lib/components/ui/Button.svelte';
	import Dialog from '#lib/components/ui/Dialog.svelte';
	import { listFolder, type FileNode } from '#lib/files/api.ts';
	import { latestOnly } from '#lib/files/latest.ts';

	interface Props {
		node: FileNode;
		/** Resolves to an error message, or null on success. */
		onMove: (parentId: string) => Promise<string | null>;
		/** Runs once the dialog closed, however it closed. */
		onClose: () => void;
	}

	let { node, onMove, onClose }: Props = $props();

	let dialog: Dialog | undefined = $state();
	let open = $state(true);
	let trail: FileNode[] = $state([]);
	let folders: FileNode[] = $state([]);
	let loading = $state(true);
	let error: string | null = $state(null);
	let busy = $state(false);
	const here = $derived(trail.at(-1) ?? null);
	// Clicking quickly through folders must not let an older listing win.
	const track = latestOnly();

	async function browse(next: FileNode[]) {
		loading = true;
		const result = await track(listFolder(next.at(-1)?.id ?? null));
		if (result.stale) return;
		loading = false;
		if (!result.value.ok) {
			error = result.value.error;
			return;
		}
		trail = next;
		folders = result.value.data.filter((item) => item.kind === 'folder' && item.id !== node.id);
		error = null;
	}

	async function moveHere() {
		if (!here) return;
		busy = true;
		try {
			error = await onMove(here.id);
			if (!error) dialog?.close();
		} finally {
			busy = false;
		}
	}

	onMount(() => void browse([]));
</script>

<Dialog bind:this={dialog} bind:open title={`Move “${node.name}”`} onclose={onClose}>
	<div class="body">
		<div class="where">
			{#if trail.length > 0}
				<Button variant="quiet" size="sm" onclick={() => browse(trail.slice(0, -1))}>
					<Icon name="chevron-left" />Up
				</Button>
			{/if}
			<p class="location" aria-live="polite">
				{here ? `To: ${trail.map((folder) => folder.name).join(' › ')}` : 'Choose a folder'}
			</p>
		</div>
		<ul class="folders" aria-busy={loading}>
			{#each folders as folder (folder.id)}
				<li>
					<button type="button" class="folder" onclick={() => browse([...trail, folder])}>
						<span class="well"><Icon name="folder" /></span>
						<span class="name">{folder.name}</span>
						<Icon name="chevron-right" />
					</button>
				</li>
			{:else}
				{#if !loading && !error}<li class="none">No subfolders</li>{/if}
			{/each}
		</ul>
		{#if error}<Alert>{error}</Alert>{/if}
	</div>
	{#snippet actions()}
		<Button variant="secondary" onclick={() => dialog?.close()}>Cancel</Button>
		<Button onclick={moveHere} disabled={busy || !here || here.id === node.parent_id}>Move here</Button>
	{/snippet}
</Dialog>

<style>
	.body {
		display: grid;
		gap: var(--space-3);
	}

	.where {
		display: flex;
		flex-wrap: wrap;
		align-items: center;
		gap: var(--space-2);
		min-height: var(--tap);
	}

	.location {
		min-width: 0;
		color: var(--ink-muted);
		font-size: var(--text-sm);
		line-height: var(--text-sm-lh);
		overflow-wrap: anywhere;
	}

	.folders {
		display: grid;
		gap: var(--space-1);
		max-height: calc(var(--row-min) * 5);
		margin: 0;
		padding: var(--space-2);
		overflow: auto;
		list-style: none;
		border-radius: var(--radius-well);
		background: var(--tint);
	}

	.folder {
		display: flex;
		align-items: center;
		gap: var(--space-3);
		width: 100%;
		min-height: var(--row-min);
		padding: var(--space-2) var(--space-3) var(--space-2) var(--space-2);
		border: 0;
		border-radius: var(--radius-card);
		background: transparent;
		color: var(--ink-muted);
		text-align: left;
		cursor: pointer;
		transition:
			background-color var(--dur-fast) var(--ease),
			box-shadow var(--dur) var(--ease),
			color var(--dur-fast) var(--ease);
	}

	@media (hover: hover) {
		.folder:hover {
			color: var(--ink);
			background: var(--card);
			box-shadow: var(--shadow-card);
		}
	}

	.folder:focus-visible {
		outline: var(--ring-width) solid var(--accent);
		outline-offset: calc(-1 * var(--ring-width));
	}

	.folder:active {
		transform: translateY(var(--press));
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
		color: var(--ink);
	}

	.name {
		flex: 1;
		min-width: 0;
		color: var(--ink);
		font-weight: var(--weight-label);
		overflow-wrap: anywhere;
	}

	.none {
		padding: var(--space-3);
		color: var(--ink-muted);
		font-size: var(--text-sm);
	}
</style>
