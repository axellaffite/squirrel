<script lang="ts">
	import Icon from '#lib/components/icons/Icon.svelte';
	import { previewKind } from '#lib/files/format.ts';
	import { contentUrl, previewUrl } from '#lib/public/api.ts';
	import type { ViewedFile } from '#lib/public/view.ts';

	interface Props {
		token: string;
		file: ViewedFile;
	}

	let { token, file }: Props = $props();

	const kind = $derived(previewKind(file.mime));
	/** Device pixels of the screen's long side, for a sharp but not oversized preview. */
	const px = Math.max(window.screen.width, window.screen.height) * (window.devicePixelRatio || 1);
</script>

<div class="stage">
	{#if kind === 'image' && file.hasThumbnail}
		<img src={previewUrl(token, file.id, px)} alt={file.name} />
	{:else if kind === 'video'}
		<!-- svelte-ignore a11y_media_has_caption -->
		<video controls preload="metadata" src={contentUrl(token, file.id)}></video>
	{:else if kind === 'audio'}
		<audio controls preload="metadata" src={contentUrl(token, file.id)}></audio>
	{:else if kind === 'pdf'}
		<iframe title={file.name} src={contentUrl(token, file.id)}></iframe>
	{:else}
		<p class="none"><Icon name="file" />No preview for this file.</p>
	{/if}
</div>

<style>
	.stage {
		display: grid;
		place-items: center;
		width: 100%;
	}

	img,
	video {
		max-width: 100%;
		max-height: var(--preview-max);
		border-radius: var(--radius-sm);
	}

	audio {
		width: 100%;
	}

	iframe {
		width: 100%;
		height: var(--preview-max);
		border: 0;
		border-radius: var(--radius-sm);
	}

	.none {
		display: flex;
		align-items: center;
		gap: var(--space-2);
		margin: 0;
		color: var(--ink-muted);
	}
</style>
