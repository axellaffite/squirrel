<script lang="ts">
	import { onMount, tick, untrack } from 'svelte';
	import MonthSection from '#lib/components/photos/MonthSection.svelte';
	import type { AssetSummary, MonthCount } from '#lib/photos/api.ts';
	import { moveTileFocus } from '#lib/photos/keyboard.ts';
	import { readMetrics, sameMetrics } from '#lib/photos/metrics.ts';
	import {
		layoutMonth,
		monthAt,
		monthPlaceholders,
		monthsToLoad,
		reanchor,
		replaceEstimate,
		type Metrics,
		type MonthBox,
		type MonthLayout
	} from '#lib/photos/model.ts';
	import type { TimelineData } from '#lib/photos/timeline.svelte.ts';

	export interface Geometry {
		boxes: readonly MonthBox[];
		total: number;
		/** Index of the month at the top of the view. */
		current: number;
	}

	interface Props {
		data: TimelineData;
		selected: ReadonlySet<string>;
		selecting: boolean;
		onOpen: (asset: AssetSummary) => void;
		onToggle: (asset: AssetSummary, range: boolean) => void;
		/** Month boxes and the current month, for the scrubber. */
		geometry?: Geometry;
	}

	let { data, selected, selecting, onOpen, onToggle, geometry = $bindable() }: Props = $props();

	let root: HTMLDivElement | undefined = $state();
	let width = $state(0);
	let metrics: Metrics | null = $state.raw(null);
	let boxes: readonly MonthBox[] = $state.raw([]);
	let total = $state(0);
	let layouts: ReadonlyMap<string, MonthLayout<AssetSummary>> = $state.raw(new Map());
	let viewTop = $state(0);
	let viewHeight = $state(0);
	/** Set on unmount: late month answers must not scroll whatever page is shown by then. */
	let destroyed = false;

	const overscan = $derived(viewHeight);
	const shown = $derived(
		boxes.filter((box) => box.top + box.height >= viewTop - overscan && box.top <= viewTop + viewHeight + overscan)
	);

	$effect(() => {
		geometry = { boxes, total, current: monthAt(boxes, Math.max(0, viewTop)) };
	});

	// Months, width or token sizes changed: lay everything out again, keeping the view in place.
	$effect(() => {
		const months = data.months;
		const w = width;
		const m = metrics;
		if (!months || !m || w <= 0) return;
		untrack(() => rebuild(months, w, m));
	});

	function rebuild(months: readonly MonthCount[], w: number, m: Metrics) {
		const before = boxes;
		const top = viewTop;
		const next = new Map<string, MonthLayout<AssetSummary>>();
		let placed: { boxes: readonly MonthBox[]; height: number } = monthPlaceholders(months, w, m);
		for (const { month } of months) {
			const items = data.items[month];
			if (!items) continue;
			const layout = layoutMonth(items, w, m);
			next.set(month, layout);
			placed = replaceEstimate(placed.boxes, month, layout.height, -Infinity);
		}
		layouts = next;
		boxes = placed.boxes;
		total = placed.height;
		if (before.length > 0 && top > 0) scrollByPx(reanchor(before, placed.boxes, top) - top);
		pump();
	}

	async function fetchMonth(month: string) {
		const outcome = await data.loadMonth(month);
		if (destroyed) return;
		if (outcome.status === 'loaded' && metrics && width > 0) {
			const layout = layoutMonth(outcome.items, width, metrics);
			layouts = new Map(layouts).set(month, layout);
			const result = replaceEstimate(boxes, month, layout.height, viewTop);
			boxes = result.boxes;
			total = result.height;
			scrollByPx(result.delta);
		}
		// A stale answer (a reload overtook it) still frees a slot for the month it left unloaded.
		// A failed month is skipped until the next reload, so this cannot loop on errors.
		pump();
	}

	function pump() {
		if (destroyed) return;
		for (const month of monthsToLoad(boxes, viewTop, viewTop + viewHeight, data.loading, data.settled)) {
			void fetchMonth(month);
		}
	}

	function scrollByPx(delta: number) {
		if (destroyed || Math.abs(delta) < 1) return;
		window.scrollBy({ top: delta, behavior: 'instant' });
		// The browser clamps near the bottom: read where it actually went.
		measure();
	}

	function measure() {
		if (!root) return;
		viewTop = -root.getBoundingClientRect().top;
		viewHeight = window.innerHeight;
		pump();
	}

	/** Scrolls so month `index` starts at the top of the viewport, at once (no smooth scroll). */
	export function jumpTo(index: number) {
		const box = boxes[index];
		if (!root || !box) return;
		window.scrollTo({ top: window.scrollY + root.getBoundingClientRect().top + box.top, behavior: 'instant' });
		measure();
	}

	const findTile = (id: string) => root?.querySelector<HTMLElement>(`[data-tile][data-id="${CSS.escape(id)}"]`);

	function locate(id: string): number | null {
		for (const box of boxes) {
			const layout = layouts.get(box.month);
			const block = layout?.blocks.find((b) => b.kind === 'row' && b.items.some((cell) => cell.item.asset.id === id));
			if (block) return box.top + block.top;
		}
		return null;
	}

	/** Brings a photo's tile into view and focuses it (e.g. when the lightbox closes). */
	export async function focusTile(id: string) {
		if (!root) return;
		let tile = findTile(id);
		if (!tile) {
			const top = locate(id);
			if (top === null) return;
			const rootTop = window.scrollY + root.getBoundingClientRect().top;
			window.scrollTo({ top: rootTop + top - window.innerHeight / 3, behavior: 'instant' });
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
			const nextMetrics = readMetrics(element);
			if (!sameMetrics(metrics, nextMetrics)) metrics = nextMetrics;
			width = Math.floor(entry.contentRect.width);
			schedule();
		});
		observer.observe(element);
		window.addEventListener('scroll', schedule, { passive: true });
		window.addEventListener('resize', schedule, { passive: true });
		return () => {
			destroyed = true;
			cancelAnimationFrame(frame);
			observer.disconnect();
			window.removeEventListener('scroll', schedule);
			window.removeEventListener('resize', schedule);
		};
	});
</script>

<!-- Arrow keys move between tiles (each tile is a button, so Tab still reaches them in order). -->
<!-- svelte-ignore a11y_no_static_element_interactions -->
<div class="timeline" bind:this={root} style:height="{total}px" onkeydown={keydown}>
	{#each shown as box (box.month)}
		<MonthSection
			{box}
			layout={layouts.get(box.month)}
			viewTop={viewTop - box.top}
			{viewHeight}
			{overscan}
			{selected}
			{selecting}
			{onOpen}
			{onToggle}
		/>
	{/each}
</div>

<style>
	/* One positioned box of the full height; only the blocks near the view are in the DOM. The
	   browser's own scroll anchoring is off: the timeline compensates its own height changes. */
	.timeline {
		position: relative;
		min-width: 0;
		overflow-anchor: none;
	}
</style>
