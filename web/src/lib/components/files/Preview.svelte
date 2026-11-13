<script lang="ts">
	import { onMount } from 'svelte';
	import type { FileNode } from '#lib/files/api.ts';
	import { contentUrl, formatBytes, previewKind } from '#lib/files/format.ts';

	/** Text previews show the start of the file only. */
	const TEXT_PREVIEW_BYTES = 256 * 1024;

	let { node, onClose }: { node: FileNode; onClose: () => void } = $props();

	let dialog: HTMLDialogElement;
	const kind = $derived(previewKind(node.mime));
	const url = $derived(contentUrl(node.id));
	let text: string | null = $state(null);
	let textError: string | null = $state(null);

	onMount(() => {
		dialog.showModal();
		if (kind === 'text') void loadText();
	});

	async function loadText() {
		try {
			const response = await fetch(url, {
				headers: { Range: `bytes=0-${TEXT_PREVIEW_BYTES - 1}` },
				credentials: 'same-origin'
			});
			if (!response.ok) throw new Error(String(response.status));
			text = await response.text();
		} catch {
			textError = 'Could not load the preview.';
		}
	}
</script>

<dialog bind:this={dialog} onclose={onClose} class="preview" aria-labelledby="preview-title">
	<header>
		<h2 id="preview-title">{node.name}</h2>
		<span class="muted">{formatBytes(node.size)}</span>
		<a href={url} target="_blank" rel="noopener">Open in new tab</a>
		<a href={contentUrl(node.id, true)} download>Download</a>
		<button class="link" onclick={() => dialog.close()}>Close</button>
	</header>
	{#if kind === 'image'}
		<img src={url} alt={node.name} />
	{:else if kind === 'pdf'}
		<iframe src={url} title={`PDF preview of ${node.name}`}></iframe>
	{:else if kind === 'video'}
		<!-- svelte-ignore a11y_media_has_caption -->
		<video src={url} controls preload="metadata"></video>
	{:else if kind === 'audio'}
		<audio src={url} controls preload="metadata"></audio>
	{:else if kind === 'text'}
		{#if textError}<p class="error" role="alert">{textError}</p>
		{:else if text === null}<p class="muted">Loading…</p>
		{:else}
			<pre data-testid="text-preview">{text}</pre>
			{#if node.size > TEXT_PREVIEW_BYTES}<p class="muted">Showing the first {formatBytes(TEXT_PREVIEW_BYTES)}.</p>{/if}
		{/if}
	{:else}
		<p class="muted">No preview for this kind of file. Download it to open it.</p>
	{/if}
</dialog>

<style>
	.preview { width: min(90vw, 960px); }
	header { display: flex; flex-wrap: wrap; gap: 1rem; align-items: baseline; margin-bottom: 1rem; }
	h2 { margin: 0; font-size: 1.1rem; overflow-wrap: anywhere; }
	img, video { max-width: 100%; max-height: 75vh; display: block; margin: 0 auto; }
	iframe { width: 100%; height: 75vh; border: 0; background: white; }
	pre { max-height: 70vh; overflow: auto; white-space: pre-wrap; overflow-wrap: anywhere; }
</style>
