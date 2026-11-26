<script lang="ts">
	import type { Snippet } from 'svelte';
	import Icon from '#lib/components/icons/Icon.svelte';
	import Button from '#lib/components/ui/Button.svelte';
	import { contentUrl } from '#lib/files/format.ts';
	import { thumbnailUrl, type AssetSummary } from '#lib/photos/api.ts';
	import { averageColor, isBlurhash } from '#lib/photos/blurhash.ts';
	import { blurred } from '#lib/photos/blurred.svelte.ts';
	import { browserCanShow } from '#lib/photos/info.ts';
	import { placeholders } from '#lib/photos/placeholder.ts';
	import { PREVIEW_RETRY_MS, windowPreviewPx } from '#lib/photos/previewSize.ts';

	interface Props {
		asset: AssetSummary;
		/** Drawn over the photo, in its own frame (face boxes are relative to it). */
		overlay?: Snippet;
	}

	let { asset, overlay }: Props = $props();

	let failedId: string | null = $state(null);
	/** A preview the server was too busy to render (503/429) is asked for once more. */
	let retriedId: string | null = $state(null);
	let attempt = $state(0);

	/** Matched by the blur filter: the photo is not loaded until the user asks to see it. */
	const hidden = $derived(blurred.has(asset.id));

	const dimensions = $derived(asset.width && asset.height ? { width: asset.width, height: asset.height } : null);
	const source = $derived.by(() => {
		if (failedId === asset.id) return null;
		if (asset.thumbnail.state === 'ready') {
			return thumbnailUrl(asset.id, 'preview', asset.thumbnail.version, windowPreviewPx(dimensions));
		}
		return browserCanShow(asset.mime) ? contentUrl(asset.id) : null;
	});

	function failed() {
		const id = asset.id;
		if (asset.thumbnail.state !== 'ready' || retriedId === id) {
			failedId = id;
			return;
		}
		retriedId = id;
		setTimeout(() => {
			if (asset.id === id) attempt += 1;
		}, PREVIEW_RETRY_MS);
	}
	const hash = $derived(isBlurhash(asset.blurhash) ? asset.blurhash : null);
	const blur = $derived(hash ? placeholders.load(hash) : null);
	const ratio = $derived(asset.width && asset.height ? asset.width / asset.height : null);
</script>

<!-- The frame keeps the photo's aspect inside the stage (container units), so the blurred
     preview covers exactly where the photo will be. Switching photos is instant. -->
<div class="area">
	{#if hidden}
		<div
			class="frame veiled"
			class:sized={ratio !== null}
			style:--ratio={ratio ?? undefined}
			style:background-color={hash ? averageColor(hash) : undefined}
			style:background-image={blur ? `url("${blur}")` : undefined}
		>
			<div class="notice" role="group" aria-label="Blurred photo">
				<span class="well"><Icon name="eye-off" size={24} /></span>
				<p><strong>Hidden by your blur filter</strong></p>
				<p class="why">This photo matches the blur filter in your settings, so it stays blurred until you show it.</p>
				<Button onclick={() => blurred.reveal(asset.id)}>Show</Button>
			</div>
		</div>
	{:else if source}
		<div
			class="frame"
			class:sized={ratio !== null}
			style:--ratio={ratio ?? undefined}
			style:background-color={hash ? averageColor(hash) : undefined}
			style:background-image={blur ? `url("${blur}")` : undefined}
		>
			{#key `${source}#${attempt}`}
				<img src={source} alt={asset.name} decoding="async" draggable="false" onerror={failed} />
			{/key}
			{@render overlay?.()}
		</div>
	{:else}
		<div class="unavailable">
			<span class="well"><Icon name="image-off" size={24} /></span>
			<p>No preview for this photo.</p>
			<Button variant="secondary" href={contentUrl(asset.id, true)} download>
				<Icon name="download" />Download original
			</Button>
		</div>
	{/if}
</div>

<style>
	.area {
		display: grid;
		place-items: center;
		width: 100%;
		height: 100%;
		min-height: 0;
		container-type: size;
	}

	.frame {
		position: relative;
		display: grid;
		place-items: center;
		max-width: 100cqw;
		max-height: 100cqh;
		background-size: cover;
	}

	.frame.sized {
		width: min(100cqw, 100cqh * var(--ratio));
		aspect-ratio: var(--ratio);
	}

	img {
		width: 100%;
		height: 100%;
		max-width: 100cqw;
		max-height: 100cqh;
		object-fit: contain;
		user-select: none;
		-webkit-touch-callout: none;
	}

	/* A blurred photo keeps the photo's frame, so the page does not jump when it is shown. */
	.veiled:not(.sized) {
		width: 100cqw;
		height: 100cqh;
	}

	.notice {
		display: grid;
		justify-items: center;
		gap: var(--space-2);
		max-width: var(--measure);
		padding: var(--space-4);
		color: var(--rail-ink);
		text-align: center;
	}

	.notice p {
		margin: 0;
	}

	.why {
		font-size: var(--text-sm);
		line-height: var(--text-sm-lh);
	}

	.unavailable {
		display: grid;
		justify-items: center;
		gap: var(--space-3);
		color: var(--rail-ink);
		text-align: center;
	}

	/* On the graphite stage the button takes the rail's active colours (white pill, orchid ink). */
	.unavailable :global(.btn),
	.notice :global(.btn) {
		background: var(--rail-active);
		color: var(--rail-active-ink);
	}

	.well {
		display: grid;
		place-items: center;
		width: var(--well-lg);
		height: var(--well-lg);
		border-radius: var(--radius-full);
		background: var(--rail-well);
	}
</style>
