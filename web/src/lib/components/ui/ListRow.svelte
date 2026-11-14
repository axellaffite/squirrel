<script lang="ts">
	import type { Snippet } from 'svelte';

	interface Props {
		/** Content of the inset icon well (usually an Icon). */
		icon: Snippet;
		/** The name. An element with class `row-link` inside stretches over the whole row. */
		title: Snippet;
		/** Secondary line: sizes, dates, status (tabular figures). */
		meta?: Snippet;
		/** Quiet action buttons on the right. */
		actions?: Snippet;
		/** Highlights the row (e.g. the item a dialog is acting on). */
		selected?: boolean;
		/** Index for the arrival stagger; omit to skip the arrival animation. */
		arrive?: number;
		testId?: string;
		nodeId?: string;
	}

	let { icon, title, meta, actions, selected = false, arrive, testId, nodeId }: Props =
		$props();
</script>

<li
	class="row"
	class:selected
	class:arrive={arrive !== undefined}
	style:--i={arrive}
	data-testid={testId}
	data-node-id={nodeId}
>
	<span class="well">{@render icon()}</span>
	<div class="main">
		<div class="title">{@render title()}</div>
		{#if meta}<div class="meta">{@render meta()}</div>{/if}
	</div>
	{#if actions}<div class="actions">{@render actions()}</div>{/if}
</li>

<style>
	/* A white row card: round tint icon well, name and metadata, quiet round actions. */
	/* Wide actions (e.g. an armed confirmation) wrap under the name instead of crushing it. */
	.row {
		position: relative;
		display: flex;
		flex-wrap: wrap;
		align-items: center;
		gap: var(--space-2) var(--space-3);
		min-height: var(--row-min);
		padding: var(--space-2) var(--space-2) var(--space-2) var(--space-3);
		border-radius: var(--radius-card);
		background: var(--card);
		box-shadow: var(--shadow-card);
		transition: box-shadow var(--dur) var(--ease);
	}

	.selected {
		box-shadow: var(--shadow-card), var(--select-ring);
	}

	.well {
		display: grid;
		place-items: center;
		width: var(--well-sm);
		height: var(--well-sm);
		flex: none;
		border-radius: var(--radius-full);
		background: var(--tint);
		color: var(--ink);
	}

	.main {
		display: grid;
		flex: 1 1 var(--row-name-min);
		min-width: 0;
	}

	.title {
		min-width: 0;
		overflow: hidden;
		text-overflow: ellipsis;
		white-space: nowrap;
		font-weight: var(--weight-label);
		letter-spacing: var(--track-label);
	}

	.meta {
		overflow: hidden;
		text-overflow: ellipsis;
		white-space: nowrap;
		color: var(--ink-muted);
		font-size: var(--text-sm);
		line-height: var(--text-sm-lh);
		font-variant-numeric: tabular-nums;
	}

	/* Separators between metadata parts: <span class="sep" aria-hidden="true">·</span> */
	.meta :global(.sep) {
		margin-inline: var(--space-1);
	}

	.actions {
		position: relative;
		z-index: 1;
		display: flex;
		justify-content: flex-end;
		align-items: center;
		gap: var(--space-1);
		flex: none;
		margin-left: auto;
	}

	/* A `.row-link` (the name) covers the row, so the whole row opens the item. */
	.title :global(.row-link) {
		display: inline;
		padding: 0;
		border: 0;
		background: none;
		color: inherit;
		font: inherit;
		letter-spacing: inherit;
		text-align: left;
		text-decoration: none;
		cursor: pointer;
	}

	.title :global(.row-link)::after {
		content: '';
		position: absolute;
		inset: 0;
		border-radius: var(--radius-card);
	}

	.title :global(.row-link:focus-visible) {
		outline: none;
	}

	.row:has(:global(.row-link:focus-visible)) {
		outline: var(--ring-width) solid var(--accent);
		outline-offset: var(--ring-offset);
	}

	@media (hover: hover) {
		.row:has(:global(.row-link:hover)) {
			box-shadow: var(--shadow-hover);
		}

		.selected:has(:global(.row-link:hover)) {
			box-shadow: var(--shadow-hover), var(--select-ring);
		}
	}

	.row:has(:global(.row-link:active)) {
		transform: translateY(var(--press));
	}

	@media (forced-colors: active) {
		.row {
			border: 1px solid CanvasText;
		}

		.selected {
			outline: 2px solid Highlight;
		}
	}
</style>
