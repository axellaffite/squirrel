<script lang="ts">
	import Icon from '#lib/components/icons/Icon.svelte';
	import type { IconName } from '#lib/components/icons/paths.ts';
	import { thumbnailUrl, type AssetSummary, type ThumbnailSize } from '#lib/photos/api.ts';
	import { averageColor, isBlurhash } from '#lib/photos/blurhash.ts';
	import { blurred } from '#lib/photos/blurred.svelte.ts';
	import { placeholders } from '#lib/photos/placeholder.ts';

	interface Props {
		/** The album's cover photo; null for an empty album. */
		cover: AssetSummary | null;
		size?: ThumbnailSize;
		/** Shown without a cover. */
		icon?: IconName;
	}

	let { cover, size = 'thumb', icon = 'library' }: Props = $props();

	/** A cover the blur filter matches shows only its blurred placeholder. */
	const hidden = $derived(cover !== null && blurred.has(cover.id));
	const ready = $derived(cover?.thumbnail.state === 'ready' && !hidden);
	const hash = $derived(cover && isBlurhash(cover.blurhash) ? cover.blurhash : null);
	const tone = $derived(hash ? averageColor(hash) : undefined);
	const blur = $derived(hidden && hash ? placeholders.load(hash) : null);
</script>

<!-- Fills its container: the cover photo over its average colour, or the album icon. -->
<span class="cover" style:background-color={tone} style:background-image={blur ? `url("${blur}")` : undefined}>
	{#if cover && ready}
		<img src={thumbnailUrl(cover.id, size, cover.thumbnail.version)} alt="" loading="lazy" decoding="async" />
	{:else if hidden}
		<span class="veil"><Icon name="eye-off" /></span>
	{:else}
		<Icon name={icon} />
	{/if}
</span>

<style>
	.cover {
		display: grid;
		place-items: center;
		width: 100%;
		height: 100%;
		background-color: var(--tint);
		background-size: cover;
		color: var(--ink-muted);
	}

	.veil {
		display: grid;
		place-items: center;
		width: 100%;
		height: 100%;
		background: var(--photo-shade);
		color: var(--photo-ring);
	}

	img {
		width: 100%;
		height: 100%;
		object-fit: cover;
	}
</style>
