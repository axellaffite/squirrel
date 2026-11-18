<script lang="ts">
	import CoverCard from '#lib/components/photos/CoverCard.svelte';
	import { photoCount } from '#lib/photos/albums.ts';
	import type { Album } from '#lib/photos/api.ts';

	interface Props {
		album: Album;
	}

	let { album }: Props = $props();
</script>

<CoverCard href={`/photos/albums/${album.id}`} name={album.name} count={photoCount(album.item_count)} cover={album.cover ?? null} />
