<script lang="ts">
	import PhotoTile from '#lib/components/photos/PhotoTile.svelte';
	import type { AssetSummary } from '#lib/photos/api.ts';
	import type { Block } from '#lib/photos/justified.ts';
	import type { Cell } from '#lib/photos/model.ts';

	interface Props {
		/** The blocks to render (already narrowed to the visible window). */
		blocks: readonly Block<Cell<AssetSummary>>[];
		selected: ReadonlySet<string>;
		selecting: boolean;
		onOpen: (asset: AssetSummary) => void;
		onToggle: (asset: AssetSummary, range: boolean) => void;
	}

	let { blocks, selected, selecting, onOpen, onToggle }: Props = $props();
</script>

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

<style>
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
</style>
