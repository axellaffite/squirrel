<script lang="ts">
	import { onMount } from 'svelte';
	import { listFolder, type FileNode } from '#lib/files/api.ts';

	let {
		node,
		onMove,
		onClose
	}: {
		node: FileNode;
		/** Resolves to an error message, or null on success. */
		onMove: (parentId: string) => Promise<string | null>;
		onClose: () => void;
	} = $props();

	let dialog: HTMLDialogElement;
	let trail: FileNode[] = $state([]);
	let folders: FileNode[] = $state([]);
	let error: string | null = $state(null);
	const here = $derived(trail.at(-1) ?? null);

	async function browse(next: FileNode[]) {
		const listing = await listFolder(next.at(-1)?.id ?? null);
		if (!listing.ok) return (error = listing.error);
		trail = next;
		folders = listing.data.filter((item) => item.kind === 'folder' && item.id !== node.id);
		error = null;
	}

	async function moveHere() {
		if (!here) return;
		error = await onMove(here.id);
		if (!error) dialog.close();
	}

	onMount(() => {
		dialog.showModal();
		void browse([]);
	});
</script>

<dialog bind:this={dialog} onclose={onClose} aria-labelledby="move-dialog-title">
	<h2 id="move-dialog-title">Move “{node.name}”</h2>
	<p class="muted">{here ? `To: ${trail.map((f) => f.name).join(' › ')}` : 'Choose a folder'}</p>
	{#if trail.length > 0}<button class="link" onclick={() => browse(trail.slice(0, -1))}>Up</button>{/if}
	<ul class="folder-picker">
		{#each folders as folder (folder.id)}
			<li><button class="link" onclick={() => browse([...trail, folder])}>📁 {folder.name}</button></li>
		{:else}
			<li class="muted">No subfolders</li>
		{/each}
	</ul>
	{#if error}<p class="error" role="alert">{error}</p>{/if}
	<div class="dialog-actions">
		<button type="button" class="link" onclick={() => dialog.close()}>Cancel</button>
		<button onclick={moveHere} disabled={!here || here.id === node.parent_id}>Move here</button>
	</div>
</dialog>
