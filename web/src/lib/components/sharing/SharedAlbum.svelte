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

<!-- The album card (a link), then who shared it and the way out, outside the link. -->
<li class="shared" data-testid="shared-album">
	<AlbumCard {album} />
	<div class="foot">
		<span class="owner">{memberName(item.owner)}</span>
		<Badge>{permissionLabel(item.permission)}</Badge>
		<Button variant="quiet" size="sm" aria-label={`Leave ${album.name}`} onclick={onLeave}>Leave</Button>
	</div>
</li>

<style>
	.shared {
		display: grid;
		gap: var(--space-1);
		min-width: 0;
	}

	.foot {
		display: flex;
		flex-wrap: wrap;
		align-items: center;
		gap: var(--space-1) var(--space-2);
		padding-inline: var(--space-1);
	}

	.owner {
		min-width: 0;
		overflow: hidden;
		text-overflow: ellipsis;
		white-space: nowrap;
		color: var(--ink-muted);
		font-size: var(--text-sm);
		line-height: var(--text-sm-lh);
	}
</style>
