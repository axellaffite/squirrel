<script lang="ts">
	import PhotoTile from '#lib/components/photos/PhotoTile.svelte';
	import type { AssetSummary } from '#lib/photos/api.ts';
	import { monthLabel } from '#lib/photos/dates.ts';
	import { visible } from '#lib/photos/justified.ts';
	import type { MonthBox, MonthLayout } from '#lib/photos/model.ts';

	interface Props {
		box: MonthBox;
		/** Absent until the month's items are laid out. */
		layout: MonthLayout<AssetSummary> | undefined;
		/** The visible window, relative to this month's top. */
		viewTop: number;
		viewHeight: number;
		overscan: number;
		selected: ReadonlySet<string>;
		selecting: boolean;
		onOpen: (asset: AssetSummary) => void;
		onToggle: (asset: AssetSummary, range: boolean) => void;
	}

	let { box, layout, viewTop, viewHeight, overscan, selected, selecting, onOpen, onToggle }: Props = $props();

	const headingId = $derived(`month-${box.month}`);
	const blocks = $derived(layout ? visible(layout.blocks, viewTop, viewHeight, overscan) : []);
</script>

<section class="month" style:transform="translateY({box.top}px)" style:height="{box.height}px" aria-labelledby={headingId}>
	<h2 class="month-head" id={headingId}>{monthLabel(box.month)}</h2>
	{#if layout}
		{#each blocks as block (block.key)}
			{#if block.kind === 'header'}
				<h3 class="day" style:transform="translateY({block.top}px)" style:height="{block.height}px">{block.label}</h3>
			{:else}
				<div class="row" style:transform="translateY({block.top}px)" style:height="{block.height}px">
					{#each block.items as cell (cell.item.asset.id)}
						<PhotoTile
							asset={cell.item.asset}
							left={cell.left}
							width={cell.width}
							height={block.height}
							selected={selected.has(cell.item.asset.id)}
							{selecting}
							{onOpen}
							{onToggle}
						/>
					{/each}
				</div>
			{/if}
		{/each}
	{:else}
		<!-- Not loaded yet: tinted rows of the real row height, never invented photos. -->
		<div class="pending" aria-hidden="true"></div>
	{/if}
</section>

<style>
	.month {
		position: absolute;
		top: 0;
		left: 0;
		right: 0;
	}

	.month-head {
		display: flex;
		align-items: flex-start;
		height: var(--photo-month-head);
		font-size: var(--text-lg);
		line-height: var(--text-lg-lh);
		font-weight: var(--weight-heading);
		letter-spacing: var(--track-heading);
	}

	.day,
	.row {
		position: absolute;
		top: 0;
		left: 0;
		right: 0;
	}

	.day {
		display: flex;
		align-items: flex-start;
		margin: 0;
		color: var(--ink-muted);
		font-size: var(--text-sm);
		line-height: var(--text-sm-lh);
		font-weight: var(--weight-label);
		letter-spacing: 0;
		font-variant-numeric: tabular-nums;
	}

	.pending {
		position: absolute;
		top: var(--photo-month-head);
		left: 0;
		right: 0;
		bottom: 0;
		border-radius: var(--radius-tile);
		background: repeating-linear-gradient(
			to bottom,
			var(--tint) 0 var(--photo-row),
			transparent var(--photo-row) calc(var(--photo-row) + var(--space-1))
		);
	}
</style>
