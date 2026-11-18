<script lang="ts">
	import Icon from '#lib/components/icons/Icon.svelte';
	import type { IconName } from '#lib/components/icons/paths.ts';
	import { thumbnailUrl, type AssetSummary, type ThumbnailSize } from '#lib/photos/api.ts';
	import { averageColor, isBlurhash } from '#lib/photos/blurhash.ts';

	interface Props {
		/** The album's cover photo; null for an empty album. */
		cover: AssetSummary | null;
		size?: ThumbnailSize;
		/** Shown without a cover. */
		icon?: IconName;
	}

	let { cover, size = 'thumb', icon = 'library' }: Props = $props();

	const ready = $derived(cover?.thumbnail.state === 'ready');
	const tone = $derived(cover && isBlurhash(cover.blurhash) ? averageColor(cover.blurhash) : undefined);
</script>

<!-- Fills its container: the cover photo over its average colour, or the album icon. -->
<span class="cover" style:background-color={tone}>
	{#if cover && ready}
		<img src={thumbnailUrl(cover.id, size, cover.thumbnail.version)} alt="" loading="lazy" decoding="async" />
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
		color: var(--ink-muted);
	}

	img {
		width: 100%;
		height: 100%;
		object-fit: cover;
	}
</style>
