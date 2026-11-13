<script lang="ts">
	import { onMount } from 'svelte';
	import type { FileNode } from '#lib/files/api.ts';
	import { contentUrl } from '#lib/files/format.ts';

	let { node, onClose }: { node: FileNode; onClose: () => void } = $props();
	let dialog: HTMLDialogElement;

	onMount(() => dialog.showModal());
</script>

<dialog bind:this={dialog} onclose={onClose} aria-labelledby="preview-title">
	<h2 id="preview-title">{node.name}</h2>
	<a href={contentUrl(node.id, true)} download>Download</a>
	<button class="link" onclick={() => dialog.close()}>Close</button>
</dialog>
