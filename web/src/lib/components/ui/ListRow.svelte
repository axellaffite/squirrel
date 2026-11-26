<script lang="ts">
	import type { Snippet } from 'svelte';

	interface Props {
		/** Content of the inset icon well (usually an Icon). */
		icon: Snippet;
		/** The name. An element with class `row-link` inside stretches over the whole row. */
		title: Snippet;
		/** Secondary line: sizes, dates, status (tabular figures). */
		meta?: Snippet;
		/** Wrapping text under the metadata, e.g. a search snippet. */
		detail?: Snippet;
		/** Quiet action buttons on the right. */
		actions?: Snippet;
		/**
		 * Table cells between the name and the actions (≥ 768 px). The row then joins its list's
		 * columns (CSS subgrid): the list must be a grid with one track per child of the row.
		 */
		cells?: Snippet;
		/** Highlights the row (e.g. the item a dialog is acting on). */
		selected?: boolean;
		/** Index for the arrival stagger; omit to skip the arrival animation. */
		arrive?: number;
		testId?: string;
		nodeId?: string;
	}

	let { icon, title, meta, detail, actions, cells, selected = false, arrive, testId, nodeId }: Props =
		$props();
</script>

<li
	class="row"
	class:selected
	class:has-cells={cells !== undefined}
	class:arrive={arrive !== undefined}
	style:--i={arrive}
	data-testid={testId}
	data-node-id={nodeId}
>
	<span class="well">{@render icon()}</span>
	<div class="main">
		<div class="title">{@render title()}</div>
		{#if meta}<div class="meta">{@render meta()}</div>{/if}
		{#if detail}<div class="detail">{@render detail()}</div>{/if}
	</div>
	{#if cells}<div class="cells">{@render cells()}</div>{/if}
	{#if actions}<div class="actions">{@render actions()}</div>{/if}
</li>

<style>
	/* A flat table row inside its list's grouped card (MASTER.md › "Table"): 8 px tint icon tile,
	   name and metadata, quiet actions; a hairline between rows. Wide actions (e.g. an armed
	   confirmation) wrap under the name instead of crushing it. */
	.row {
		--row-radius: calc(var(--radius-card) - 1px);
		position: relative;
		display: flex;
		flex-wrap: wrap;
		align-items: center;
		gap: var(--space-2) var(--space-3);
		min-height: var(--row-min);
		padding: var(--space-2) var(--space-2) var(--space-2) var(--space-3);
		transition: background-color var(--dur-fast) var(--ease);
	}

	.row:not(:first-child) {
		border-top: 1px solid var(--hair);
	}

	.row:first-child {
		border-top-left-radius: var(--row-radius);
		border-top-right-radius: var(--row-radius);
	}

	.row:last-child {
		border-bottom-left-radius: var(--row-radius);
		border-bottom-right-radius: var(--row-radius);
	}

	/* Focus and selection rings sit 4 px inside the row, radius 12 - 4 = 8, so they never touch
	   the card's corners or a neighbour's ring. */
	.row::after {
		content: '';
		position: absolute;
		inset: var(--space-1);
		border-radius: calc(var(--radius-card) - var(--space-1));
		pointer-events: none;
	}

	.selected {
		background: var(--accent-soft);
	}

	.selected::after {
		box-shadow: inset 0 0 0 1px color-mix(in srgb, var(--accent) 45%, transparent);
	}

	.well {
		display: grid;
		place-items: center;
		width: var(--well-sm);
		height: var(--well-sm);
		flex: none;
		border-radius: var(--radius-control);
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

	.detail {
		margin-top: var(--space-1);
		color: var(--ink-muted);
		font-size: var(--text-sm);
		line-height: var(--text-sm-lh);
		overflow-wrap: anywhere;
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
		border-radius: inherit;
	}

	.title :global(.row-link:focus-visible) {
		outline: none;
	}

	.row:has(:global(.row-link:focus-visible))::after {
		box-shadow: 0 0 0 var(--ring-width) var(--accent);
	}

	@media (hover: hover) {
		.row:has(:global(.row-link:hover)) {
			background: var(--tint);
		}

		.selected:has(:global(.row-link:hover)) {
			background: var(--accent-soft);
		}
	}

	.row:has(:global(.row-link:active)) .main {
		transform: translateY(var(--press));
	}

	.main {
		transition: transform var(--dur-fast) var(--ease);
	}

	.cells {
		display: none;
	}

	@media (min-width: 48rem) {
		.has-cells {
			display: grid;
			grid-column: 1 / -1;
			grid-template-columns: subgrid;
			row-gap: 0;
			column-gap: normal;
		}

		.has-cells .cells {
			display: contents;
		}

		.has-cells .meta {
			display: none;
		}

		.has-cells .actions {
			margin-left: 0;
		}

		/* A table row sits under its header band: no rounded top corners. */
		.has-cells:first-child {
			border-top-left-radius: 0;
			border-top-right-radius: 0;
		}
	}

	@media (forced-colors: active) {
		.row {
			border: 1px solid CanvasText;
		}

		.selected {
			outline: var(--ring-width) solid Highlight;
		}
	}
</style>
