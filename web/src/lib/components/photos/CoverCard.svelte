<script lang="ts">
	import type { Snippet } from 'svelte';
	import type { IconName } from '#lib/components/icons/paths.ts';
	import AlbumCover from '#lib/components/photos/AlbumCover.svelte';
	import type { AssetSummary } from '#lib/photos/api.ts';

	interface Props {
		href: string;
		name: string;
		/** "12 photos". */
		count: string;
		cover: AssetSummary | null;
		/** Shown when there is no cover. */
		icon?: IconName;
		/** Under the name and count (a place's locator). */
		extra?: Snippet;
	}

	let { href, name, count, cover, icon = 'library', extra }: Props = $props();
</script>

<!-- MASTER "Album card", also for tags and places: 4:3 cover, name and count. The whole card is the link. -->
<a class="card" {href}>
	<span class="cover"><AlbumCover {cover} {icon} /></span>
	<span class="text">
		<span class="name">{name}</span>
		<span class="count tabular">{count}</span>
	</span>
	{#if extra}<span class="extra">{@render extra()}</span>{/if}
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
		aspect-ratio: var(--album-ratio);
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

	.extra {
		display: block;
		padding-inline: var(--space-1);
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
