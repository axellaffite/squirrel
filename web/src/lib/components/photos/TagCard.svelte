<script lang="ts">
	import CoverCard from '#lib/components/photos/CoverCard.svelte';
	import type { TagGroup } from '#lib/explore/api.ts';
	import { photosHref, tagLabel } from '#lib/explore/filters.ts';
	import { photoCount } from '#lib/photos/albums.ts';

	interface Props {
		tag: TagGroup;
	}

	let { tag }: Props = $props();
</script>

<CoverCard href={photosHref({ tag: tag.tag })} name={tagLabel(tag.tag)} count={photoCount(tag.count)} cover={tag.cover} icon="tag" />
