<script lang="ts">
	import AlbumCard from '#lib/components/photos/AlbumCard.svelte';
	import Badge from '#lib/components/ui/Badge.svelte';
	import Button from '#lib/components/ui/Button.svelte';
	import type { Album } from '#lib/photos/api.ts';
	import type { SharedItem } from '#lib/sharing/api.ts';
	import { memberName, permissionLabel } from '#lib/sharing/logic.ts';

	interface Props {
		item: SharedItem;
		album: Album;
		onLeave: () => void;
	}

	let { item, album, onLeave }: Props = $props();
</script>

<!-- One card: the album and who shared it (the link), then a footer with your right and Leave. -->
<li class="shared" data-testid="shared-album">
	<AlbumCard {album}>
		{#snippet extra()}<span class="owner">From {memberName(item.owner)}</span>{/snippet}
		{#snippet footer()}
			<Badge>{permissionLabel(item.permission)}</Badge>
			<Button variant="quiet" size="sm" aria-label={`Leave ${album.name}`} onclick={onLeave}>Leave</Button>
		{/snippet}
	</AlbumCard>
</li>

<style>
	.shared {
		display: grid;
		min-width: 0;
	}

	.owner {
		display: block;
		min-width: 0;
		overflow: hidden;
		text-overflow: ellipsis;
		white-space: nowrap;
		color: var(--ink-muted);
		font-size: var(--text-sm);
		line-height: var(--text-sm-lh);
	}
</style>
