<script lang="ts">
	import Icon from '#lib/components/icons/Icon.svelte';
	import Button from '#lib/components/ui/Button.svelte';
	import Dialog from '#lib/components/ui/Dialog.svelte';
	import { previewKind } from '#lib/files/format.ts';
	import { contentUrl, previewUrl } from '#lib/public/api.ts';
	import type { ViewedFile } from '#lib/public/view.ts';

	interface Props {
		token: string;
		file: ViewedFile;
		onClose: () => void;
	}

	let { token, file, onClose }: Props = $props();

	let open = $state(true);
	const kind = $derived(previewKind(file.mime));
	/** Device pixels of the screen's long side, for a sharp but not oversized preview. */
	const px = Math.max(window.screen.width, window.screen.height) * (window.devicePixelRatio || 1);
</script>

<Dialog bind:open title={file.name} size="wide" onclose={onClose}>
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
			<p class="muted"><Icon name="file" />No preview for this file.</p>
		{/if}
	</div>
	{#snippet actions()}
		<Button variant="secondary" href={contentUrl(token, file.id)} target="_blank" rel="noopener"><Icon name="external-link" />Open</Button>
		<Button href={contentUrl(token, file.id, true)} download><Icon name="download" />Download</Button>
	{/snippet}
</Dialog>

<style>
	.stage {
		display: grid;
		place-items: center;
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

	.muted {
		display: flex;
		align-items: center;
		gap: var(--space-2);
		margin: 0;
	}
</style>
