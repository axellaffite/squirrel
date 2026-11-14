<script lang="ts">
	import type { Snippet } from 'svelte';
	import Icon from '#lib/components/icons/Icon.svelte';
	import type { IconName } from '#lib/components/icons/paths.ts';

	interface Props {
		icon: IconName;
		title: string;
		/** Explanation or next step under the title. */
		children?: Snippet;
	}

	let { icon, title, children }: Props = $props();
</script>

<div class="empty arrive" style:--i={0}>
	<span class="well"><Icon name={icon} /></span>
	<p class="title">{title}</p>
	{#if children}<p class="text">{@render children()}</p>{/if}
</div>

<style>
	.empty {
		display: grid;
		justify-items: center;
		gap: var(--space-1);
		padding: var(--space-7) var(--space-4);
		border-radius: var(--radius-well);
		background: var(--tint);
		text-align: center;
	}

	.well {
		display: grid;
		place-items: center;
		width: var(--well-lg);
		height: var(--well-lg);
		margin-bottom: var(--space-3);
		border-radius: var(--radius-full);
		background: var(--card);
		box-shadow: var(--shadow-card);
		color: var(--ink);
	}

	.title {
		font-size: var(--text-lg);
		line-height: var(--text-lg-lh);
		font-weight: var(--weight-label);
		letter-spacing: var(--track-label);
	}

	.text {
		color: var(--ink-muted);
		font-size: var(--text-sm);
		line-height: var(--text-sm-lh);
	}

	@media (forced-colors: active) {
		.empty {
			border: 1px dashed CanvasText;
		}
	}
</style>
