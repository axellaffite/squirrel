<script lang="ts">
	import AlbumCover from '#lib/components/photos/AlbumCover.svelte';
	import { photoCount } from '#lib/photos/albums.ts';
	import type { Album } from '#lib/photos/api.ts';

	interface Props {
		album: Album;
	}

	let { album }: Props = $props();
</script>

<!-- A white card: 4:3 cover, name and count. The whole card is the link. -->
<a class="card" href={`/photos/albums/${album.id}`}>
	<span class="cover"><AlbumCover cover={album.cover ?? null} /></span>
	<span class="text">
		<span class="name">{album.name}</span>
		<span class="count tabular">{photoCount(album.item_count)}</span>
	</span>
</a>

<style>
	.card {
		display: grid;
		gap: var(--space-2);
		padding: var(--space-2) var(--space-2) var(--space-3);
		border-radius: var(--radius-card);
		background: var(--card);
		box-shadow: var(--shadow-card);
		color: var(--ink);
		text-decoration: none;
		transition:
			box-shadow var(--dur) var(--ease),
			transform var(--dur-fast) var(--ease);
	}

	.cover {
		display: block;
		aspect-ratio: 4 / 3;
		overflow: hidden;
		border-radius: var(--radius-tile);
	}

	.text {
		display: grid;
		min-width: 0;
		padding-inline: var(--space-1);
	}

	.name {
		overflow: hidden;
		text-overflow: ellipsis;
		white-space: nowrap;
		font-weight: var(--weight-label);
		letter-spacing: var(--track-label);
	}

	.count {
		color: var(--ink-muted);
		font-size: var(--text-sm);
		line-height: var(--text-sm-lh);
	}

	@media (hover: hover) {
		.card:hover {
			box-shadow: var(--shadow-hover);
		}
	}

	.card:focus-visible {
		outline: var(--ring-width) solid var(--accent);
		outline-offset: var(--ring-offset);
	}

	.card:active {
		transform: translateY(var(--press));
	}

	@media (forced-colors: active) {
		.card {
			border: 1px solid CanvasText;
		}
	}
</style>
