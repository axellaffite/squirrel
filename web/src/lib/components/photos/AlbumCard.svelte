<script lang="ts">
	import type { Snippet } from 'svelte';
	import CoverCard from '#lib/components/photos/CoverCard.svelte';
	import { photoCount } from '#lib/photos/albums.ts';
	import type { Album } from '#lib/photos/api.ts';

	interface Props {
		album: Album;
		/** A muted line under the count, inside the link. */
		extra?: Snippet;
		/** Inside the card, under a hairline, outside the link. */
		footer?: Snippet;
	}

	let { album, extra, footer }: Props = $props();
</script>

<CoverCard
	href={`/photos/albums/${album.id}`}
	name={album.name}
	count={photoCount(album.item_count)}
	cover={album.cover ?? null}
	icon={album.dynamic ? 'sparkles' : 'library'}
	badge={album.dynamic ? 'Dynamic' : null}
	{extra}
	{footer}
/>
