<script lang="ts">
	import { onMount, tick } from 'svelte';
	import MonthSection from '#lib/components/photos/MonthSection.svelte';
	import type { Geometry } from '#lib/components/photos/Timeline.svelte';
	import type { AssetSummary } from '#lib/photos/api.ts';
	import { moveTileFocus } from '#lib/photos/keyboard.ts';
	import { readMetrics, sameMetrics } from '#lib/photos/metrics.ts';
	import type { Metrics } from '#lib/photos/model.ts';
	import { currentMonthIndex, jumpHolds, type Jump } from '#lib/photos/scrubber.ts';
	import { groupByMonth, layoutTimeline, type TimelineLayout } from '#lib/photos/searchTimeline.ts';

	interface Props {
		/** Newest capture first (`newestFirst`): grouped by month, then by day. */
		items: readonly AssetSummary[];
		selected: ReadonlySet<string>;
		selecting: boolean;
		onOpen: (asset: AssetSummary) => void;
		onToggle: (asset: AssetSummary, range: boolean) => void;
		/** Called when the view nears the end of what is loaded (load the next page). */
		onNearEnd?: () => void;
		/** Month boxes and the current month, for the scrubber. */
		geometry?: Geometry;
	}

	let { items, selected, selecting, onOpen, onToggle, onNearEnd, geometry = $bindable() }: Props = $props();

	let root: HTMLDivElement | undefined = $state();
	let width = $state(0);
	let metrics = $state.raw<Metrics | null>(null);
	let viewTop = $state(0);
	let viewHeight = $state(0);
	/** The month last jumped to (scrubber), held while the page cannot bring it to the top. */
	let jumped: Jump | null = $state(null);

	const timeline: TimelineLayout<AssetSummary> = $derived.by(() => {
		const m = metrics;
		return m && width > 0 ? layoutTimeline(groupByMonth(items), width, m) : { boxes: [], layouts: new Map(), height: 0 };
	});
	const overscan = $derived(viewHeight);
	const shown = $derived(
		timeline.boxes.filter((box) => box.top + box.height >= viewTop - overscan && box.top <= viewTop + viewHeight + overscan)
	);

	$effect(() => {
		geometry = { boxes: timeline.boxes, total: timeline.height, current: currentMonthIndex(timeline.boxes, viewTop, jumped?.month ?? null) };
	});

	// Within one viewport of the end: ask for more.
	$effect(() => {
		if (timeline.height > 0 && viewTop + 2 * viewHeight >= timeline.height) onNearEnd?.();
	});

	function measure() {
		if (!root) return;
		if (!jumpHolds(jumped, window.scrollY)) jumped = null;
		viewTop = -root.getBoundingClientRect().top;
		viewHeight = window.innerHeight;
	}

	/** Scrolls so month `index` starts at the top of the viewport, at once (no smooth scroll). */
	export function jumpTo(index: number) {
		const box = timeline.boxes[index];
		if (!root || !box) return;
		window.scrollTo({ top: window.scrollY + root.getBoundingClientRect().top + box.top, behavior: 'instant' });
		jumped = { month: box.month, scrollY: window.scrollY };
		measure();
	}

	const findTile = (id: string) => root?.querySelector<HTMLElement>(`[data-tile][data-id="${CSS.escape(id)}"]`);

	function locate(id: string): number | null {
		for (const box of timeline.boxes) {
			const layout = timeline.layouts.get(box.month);
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

<!-- Arrow keys move between tiles (each tile is a button, so Tab still reaches them in order). -->
<!-- svelte-ignore a11y_no_static_element_interactions -->
<div class="timeline" bind:this={root} style:height="{timeline.height}px" onkeydown={keydown}>
	{#each shown as box (box.month)}
		<MonthSection
			{box}
			layout={timeline.layouts.get(box.month)}
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
	/* One positioned box of the full height; only the months near the view are in the DOM. */
	.timeline {
		position: relative;
		min-width: 0;
	}
</style>
