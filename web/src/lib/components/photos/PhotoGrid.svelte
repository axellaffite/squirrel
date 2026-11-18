<script lang="ts">
	import { onMount, tick } from 'svelte';
	import GridBlocks from '#lib/components/photos/GridBlocks.svelte';
	import type { AssetSummary } from '#lib/photos/api.ts';
	import { visible } from '#lib/photos/justified.ts';
	import { moveTileFocus } from '#lib/photos/keyboard.ts';
	import { readMetrics, sameMetrics } from '#lib/photos/metrics.ts';
	import { layoutMonth, type Metrics } from '#lib/photos/model.ts';

	interface Props {
		/** Newest first; grouped by day (unless `days` is false), without month sections. */
		items: readonly AssetSummary[];
		selected: ReadonlySet<string>;
		selecting: boolean;
		onOpen: (asset: AssetSummary) => void;
		onToggle: (asset: AssetSummary, range: boolean) => void;
		/** Called when the view nears the end of what is loaded (load the next page). */
		onNearEnd?: () => void;
		/** `false`: keep the given order, without day headings (search results, best first). */
		days?: boolean;
	}

	let { items, selected, selecting, onOpen, onToggle, onNearEnd, days = true }: Props = $props();

	let root: HTMLDivElement | undefined = $state();
	let width = $state(0);
	let metrics = $state.raw<Metrics | null>(null);
	let viewTop = $state(0);
	let viewHeight = $state(0);

	const layout = $derived.by(() => {
		const m = metrics;
		return m && width > 0 ? layoutMonth(items, width, { ...m, monthHead: 0 }, undefined, days) : { blocks: [], height: 0 };
	});
	const shown = $derived(visible(layout.blocks, viewTop, viewHeight, viewHeight));

	// Within one viewport of the end: ask for more.
	$effect(() => {
		if (layout.height > 0 && viewTop + 2 * viewHeight >= layout.height) onNearEnd?.();
	});

	function measure() {
		if (!root) return;
		viewTop = -root.getBoundingClientRect().top;
		viewHeight = window.innerHeight;
	}

	const findTile = (id: string) => root?.querySelector<HTMLElement>(`[data-tile][data-id="${CSS.escape(id)}"]`);

	/** Brings a photo's tile into view and focuses it (e.g. when the lightbox closes). */
	export async function focusTile(id: string) {
		if (!root) return;
		let tile = findTile(id);
		if (!tile) {
			const block = layout.blocks.find((b) => b.kind === 'row' && b.items.some((cell) => cell.item.asset.id === id));
			if (!block) return;
			const rootTop = window.scrollY + root.getBoundingClientRect().top;
			window.scrollTo({ top: rootTop + block.top - window.innerHeight / 3, behavior: 'instant' });
			measure();
			await tick();
			tile = findTile(id);
		}
		tile?.focus({ preventScroll: true });
		tile?.scrollIntoView({ block: 'nearest' });
	}

	function keydown(event: KeyboardEvent) {
		if (root) moveTileFocus(root, event);
	}

	onMount(() => {
		if (!root) return;
		const element = root;
		let frame = 0;
		const schedule = () => {
			if (frame) return;
			frame = requestAnimationFrame(() => {
				frame = 0;
				measure();
			});
		};
		const observer = new ResizeObserver(([entry]) => {
			const next = readMetrics(element);
			if (!sameMetrics(metrics, next)) metrics = next;
			width = Math.floor(entry.contentRect.width);
			schedule();
		});
		observer.observe(element);
		window.addEventListener('scroll', schedule, { passive: true });
		window.addEventListener('resize', schedule, { passive: true });
		return () => {
			cancelAnimationFrame(frame);
			observer.disconnect();
			window.removeEventListener('scroll', schedule);
			window.removeEventListener('resize', schedule);
		};
	});
</script>

<!-- svelte-ignore a11y_no_static_element_interactions -->
<div class="grid" bind:this={root} style:height="{layout.height}px" onkeydown={keydown}>
	<GridBlocks blocks={shown} {selected} {selecting} {onOpen} {onToggle} />
</div>

<style>
	.grid {
		position: relative;
		min-width: 0;
	}
</style>
