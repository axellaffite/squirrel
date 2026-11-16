<script lang="ts">
	import Icon from '#lib/components/icons/Icon.svelte';
	import Alert from '#lib/components/ui/Alert.svelte';
	import Button from '#lib/components/ui/Button.svelte';
	import Dialog from '#lib/components/ui/Dialog.svelte';
	import IconButton from '#lib/components/ui/IconButton.svelte';
	import EmptyState from '#lib/components/ui/EmptyState.svelte';
	import type { FileNode } from '#lib/files/api.ts';
	import { contentUrl, formatBytes, formatDate, previewKind } from '#lib/files/format.ts';
	import { fetchTextPreview, TEXT_PREVIEW_BYTES } from '#lib/files/textPreview.ts';
	import { thumbnailUrl } from '#lib/photos/api.ts';

	interface Props {
		node: FileNode;
		/** Runs once the dialog closed, however it closed. */
		onClose: () => void;
	}

	let { node, onClose }: Props = $props();

	let open = $state(true);
	const kind = $derived(previewKind(node.mime));
	const url = $derived(contentUrl(node.id));
	/** Images show the 1440 px preview once it exists; "Open in new tab" keeps the original. */
	const imageUrl = $derived(node.thumbnail ? thumbnailUrl(node.id, 'preview', node.thumbnail.version) : url);
	let text: string | null = $state(null);
	let textError: string | null = $state(null);

	// Text previews are fetched (not framed); a newer file or closing the preview aborts the request.
	$effect(() => {
		if (kind !== 'text') return;
		const controller = new AbortController();
		text = null;
		textError = null;
		fetchTextPreview(url, node.size, controller.signal)
			.then((value) => {
				if (!controller.signal.aborted) text = value;
			})
			.catch(() => {
				if (!controller.signal.aborted) textError = 'Could not load the preview.';
			});
		return () => controller.abort();
	});
</script>

<Dialog bind:open title={node.name} size="wide" onclose={onClose}>
	{#snippet tools()}
		<IconButton icon="external-link" label={`Open ${node.name} in new tab`} tooltip="Open in new tab" fill="tint" href={url} newTab />
		<IconButton icon="download" label={`Download ${node.name}`} tooltip="Download" fill="tint" href={contentUrl(node.id, true)} download />
	{/snippet}
	<div class="body">
		<p class="meta tabular">{formatBytes(node.size)} <span aria-hidden="true">·</span> {formatDate(node.modified_at)}</p>
		<div class="stage">
			{#if kind === 'image'}
				<img src={imageUrl} alt={node.name} />
			{:else if kind === 'pdf'}
				<!--
					The server sends PDFs inline with a strict CSP but without `sandbox` (built-in PDF viewers
					refuse sandboxed documents), and `frame-ancestors 'self'` so only this app can frame them.
					No referrer leaves with the frame's own requests.
				-->
				<iframe src={url} title={`PDF preview of ${node.name}`} referrerpolicy="no-referrer"></iframe>
			{:else if kind === 'video'}
				<!-- svelte-ignore a11y_media_has_caption -->
				<video src={url} controls preload="metadata"></video>
			{:else if kind === 'audio'}
				<audio src={url} controls preload="metadata"></audio>
			{:else if kind === 'text'}
				{#if textError}<Alert>{textError}</Alert>
				{:else if text === null}<p class="muted" aria-live="polite">Loading…</p>
				{:else}
					<pre data-testid="text-preview">{text}</pre>
				{/if}
			{:else}
				<EmptyState icon="file" title="No preview for this kind of file">Download it to open it.</EmptyState>
			{/if}
		</div>
		{#if kind === 'text' && text !== null && node.size > TEXT_PREVIEW_BYTES}
			<p class="meta">Showing the first {formatBytes(TEXT_PREVIEW_BYTES)}.</p>
		{/if}
		{#if kind === null}
			<div class="download">
				<Button href={contentUrl(node.id, true)} download><Icon name="download" />Download</Button>
			</div>
		{/if}
	</div>
</Dialog>

<style>
	.body {
		display: grid;
		gap: var(--space-3);
	}

	.meta {
		color: var(--ink-muted);
		font-size: var(--text-sm);
		line-height: var(--text-sm-lh);
	}

	.stage {
		display: grid;
		place-items: center;
		min-width: 0;
		border-radius: var(--radius-card);
		overflow: hidden;
	}

	img,
	video {
		max-width: 100%;
		max-height: var(--preview-max);
		border-radius: var(--radius-card);
	}

	audio {
		width: 100%;
	}

	iframe {
		width: 100%;
		height: var(--preview-max);
		border: 0;
		border-radius: var(--radius-card);
		background: var(--fill);
	}

	pre {
		width: 100%;
		max-height: var(--preview-max);
		margin: 0;
		padding: var(--space-4);
		overflow: auto;
		border-radius: var(--radius-card);
		background: var(--tint);
		font-family: var(--font-mono);
		font-size: var(--text-sm);
		line-height: var(--text-sm-lh);
		white-space: pre-wrap;
		overflow-wrap: anywhere;
	}

	.stage :global(.empty) {
		width: 100%;
	}

	.download {
		display: flex;
		justify-content: center;
	}
</style>
