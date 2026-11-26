<script lang="ts">
	import type { Snippet } from 'svelte';
	import type { IconName } from '#lib/components/icons/paths.ts';
	import AlbumCover from '#lib/components/photos/AlbumCover.svelte';
	import Badge from '#lib/components/ui/Badge.svelte';
	import type { AssetSummary } from '#lib/photos/api.ts';

	interface Props {
		href: string;
		name: string;
		count: string;
		cover: AssetSummary | null;
		icon?: IconName;
		/** A short label after the name (e.g. "Dynamic"). */
		badge?: string | null;
		extra?: Snippet;
		/** A row under a hairline, inside the card but outside the link (e.g. who shared it, Leave). */
		footer?: Snippet;
	}

	let { href, name, count, cover, icon = 'library', badge = null, extra, footer }: Props = $props();
</script>

<!--
	MASTER "Album card", also for tags and places: 4:3 cover, name and count, the link. An optional
	footer holds controls of its own, so it sits beside the link rather than inside it.
-->
<div class="card">
	<a class="link" {href}>
		<span class="cover"><AlbumCover {cover} {icon} /></span>
		<span class="text">
			<span class="title">
				<span class="name">{name}</span>
				{#if badge}<Badge tone="accent">{badge}</Badge>{/if}
			</span>
			<span class="count tabular">{count}</span>
			{#if extra}<span class="extra">{@render extra()}</span>{/if}
		</span>
	</a>
	{#if footer}<div class="footer">{@render footer()}</div>{/if}
</div>

<style>
	.card {
		display: grid;
		min-width: 0;
		border: 1px solid var(--hair);
		border-radius: var(--radius-card);
		background: var(--card);
		box-shadow: var(--shadow-card);
		color: var(--ink);
		transition:
			border-color var(--dur) var(--ease),
			box-shadow var(--dur) var(--ease),
			transform var(--dur-fast) var(--ease);
	}

	.link {
		display: grid;
		gap: var(--space-2);
		padding: var(--space-2) var(--space-2) var(--space-3);
		border-radius: inherit;
		color: inherit;
		text-decoration: none;
	}

	/* Concentric with the card: 12 px corners, 8 px padding, 4 px cover. */
	.cover {
		display: block;
		aspect-ratio: var(--album-ratio);
		overflow: hidden;
		border-radius: calc(var(--radius-card) - var(--space-2));
	}

	.text {
		display: grid;
		min-width: 0;
		padding-inline: var(--space-1);
	}

	.title {
		display: flex;
		align-items: center;
		gap: var(--space-2);
		min-width: 0;
	}

	.name {
		min-width: 0;
		overflow: hidden;
		text-overflow: ellipsis;
		white-space: nowrap;
		font-weight: var(--weight-label);
		letter-spacing: var(--track-label);
	}

	.extra {
		display: block;
		color: var(--ink-muted);
		font-size: var(--text-sm);
		line-height: var(--text-sm-lh);
	}

	.count {
		color: var(--ink-muted);
		font-size: var(--text-sm);
		line-height: var(--text-sm-lh);
	}

	.footer {
		display: flex;
		align-items: center;
		justify-content: space-between;
		gap: var(--space-2);
		min-width: 0;
		padding: var(--space-1) var(--space-1) var(--space-1) var(--space-3);
		border-top: 1px solid var(--hair);
	}

	@media (hover: hover) {
		.card:has(.link:hover) {
			border-color: var(--hair-strong);
			box-shadow: var(--shadow-hover);
		}
	}

	.link:focus-visible {
		outline: var(--ring-width) solid var(--accent);
		outline-offset: var(--ring-offset);
	}

	.card:has(.link:active) {
		transform: translateY(var(--press));
	}

	@media (forced-colors: active) {
		.card {
			border: 1px solid CanvasText;
		}
	}
</style>
