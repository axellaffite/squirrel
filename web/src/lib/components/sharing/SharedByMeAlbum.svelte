<script lang="ts">
	import AlbumCard from '#lib/components/photos/AlbumCard.svelte';
	import Button from '#lib/components/ui/Button.svelte';
	import type { Album } from '#lib/photos/api.ts';
	import type { SharedByMe } from '#lib/sharing/api.ts';
	import { sharedWithSummary } from '#lib/sharing/logic.ts';

	interface Props {
		item: SharedByMe;
		album: Album;
		onManage: () => void;
	}

	let { item, album, onManage }: Props = $props();
</script>

<!-- One card: the album (a link), then a footer with who it is shared with and Manage. -->
<li class="shared" data-testid="shared-by-me-album">
	<AlbumCard {album}>
		{#snippet footer()}
			<span class="people">{sharedWithSummary(item.people.map((share) => share.person))}</span>
			<Button variant="quiet" size="sm" aria-label={`Manage sharing of ${album.name}`} onclick={onManage}>Manage</Button>
		{/snippet}
	</AlbumCard>
</li>

<style>
	.shared {
		display: grid;
		min-width: 0;
	}

	.people {
		min-width: 0;
		overflow: hidden;
		text-overflow: ellipsis;
		white-space: nowrap;
		color: var(--ink-muted);
		font-size: var(--text-sm);
		line-height: var(--text-sm-lh);
	}
</style>
