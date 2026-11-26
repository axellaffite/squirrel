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

<!-- The album card (a link), then who it is shared with and the sharing dialog, outside the link. -->
<li class="shared" data-testid="shared-by-me-album">
	<AlbumCard {album} />
	<div class="foot">
		<span class="people">{sharedWithSummary(item.people.map((share) => share.person))}</span>
		<Button variant="quiet" size="sm" aria-label={`Manage sharing of ${album.name}`} onclick={onManage}>Manage</Button>
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
