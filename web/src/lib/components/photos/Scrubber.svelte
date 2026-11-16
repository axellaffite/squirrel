<script lang="ts">
	import { onMount } from 'svelte';
	import Icon from '#lib/components/icons/Icon.svelte';
	import { monthLabel, shortMonthLabel } from '#lib/photos/dates.ts';
	import type { MonthBox } from '#lib/photos/model.ts';
	import { monthAtFraction, positions, stepMonth, yearLabels } from '#lib/photos/scrubber.ts';

	/** The phone handle stays this long after the last scroll event. */
	const HANDLE_LINGER_MS = 1500;

	interface Props {
		boxes: readonly MonthBox[];
		total: number;
		/** Index of the month at the top of the view. */
		current: number;
		onJump: (index: number) => void;
	}

	let { boxes, total, current, onJump }: Props = $props();

	let column: HTMLDivElement | undefined = $state();
	let track: HTMLDivElement | undefined = $state();
	let columnHeight = $state(0);
	let dragging: number | null = $state(null);
	let handleShown = $state(false);

	const at = $derived(positions(boxes, total));
	const labels = $derived(yearLabels(boxes, at, columnHeight));
	const months = $derived(boxes.map((box) => box.month));
	const shownIndex = $derived(dragging ?? Math.max(0, current));
	const shownMonth = $derived(months[shownIndex]);
	const percent = (fraction: number) => `${fraction * 100}%`;

	function jumpFromPointer(event: PointerEvent, area: HTMLElement) {
		const rect = area.getBoundingClientRect();
		const index = monthAtFraction((event.clientY - rect.top) / Math.max(1, rect.height), at);
		if (index < 0) return;
		dragging = index;
		if (index !== current) onJump(index);
	}

	function pointerdown(event: PointerEvent) {
		const area = event.currentTarget === column ? column : track;
		if (!area) return;
		event.preventDefault();
		(event.currentTarget as HTMLElement).setPointerCapture(event.pointerId);
		jumpFromPointer(event, area);
	}

	function pointermove(event: PointerEvent) {
		if (dragging === null) return;
		const area = event.currentTarget === column ? column : track;
		if (area) jumpFromPointer(event, area);
	}

	function release() {
		dragging = null;
	}

	function keydown(event: KeyboardEvent) {
		const next = stepMonth(current, event.key, months);
		if (next === null) return;
		event.preventDefault();
		onJump(next);
	}

	onMount(() => {
		let timer: ReturnType<typeof setTimeout> | undefined;
		const scrolled = () => {
			handleShown = true;
			clearTimeout(timer);
			timer = setTimeout(() => {
				if (dragging === null) handleShown = false;
			}, HANDLE_LINGER_MS);
		};
		window.addEventListener('scroll', scrolled, { passive: true });
		return () => {
			clearTimeout(timer);
			window.removeEventListener('scroll', scrolled);
		};
	});

	// The column appears once months are known, so it is observed whenever it (re)mounts.
	$effect(() => {
		if (!column) return;
		const observer = new ResizeObserver(([entry]) => (columnHeight = entry.contentRect.height));
		observer.observe(column);
		return () => observer.disconnect();
	});
</script>

{#if boxes.length > 0 && shownMonth}
	<!-- ≥ 768 px: a column of years and month ticks, a slider for keyboards. -->
	<div
		bind:this={column}
		class="column"
		role="slider"
		tabindex="0"
		aria-label="Jump to month"
		aria-orientation="vertical"
		aria-valuemin={0}
		aria-valuemax={months.length - 1}
		aria-valuenow={shownIndex}
		aria-valuetext={monthLabel(shownMonth)}
		onpointerdown={pointerdown}
		onpointermove={pointermove}
		onpointerup={release}
		onpointercancel={release}
		onkeydown={keydown}
	>
		{#each at as fraction, index (months[index])}
			<span class="tick" style:top={percent(fraction)}></span>
		{/each}
		{#each labels as label (label.year)}
			<span class="year" style:top={percent(label.fraction)} aria-hidden="true">{label.year}</span>
		{/each}
		<span class="pill" class:active={dragging !== null} style:top={percent(at[shownIndex] ?? 0)} aria-hidden="true">
			{shortMonthLabel(shownMonth)}
		</span>
	</div>

	<!-- Phones: a round handle on the right edge while scrolling. -->
	<div class="phone-track" bind:this={track} class:shown={handleShown || dragging !== null}>
		<div
			class="handle"
			style:top={percent(at[shownIndex] ?? 0)}
			role="slider"
			tabindex="-1"
			aria-label="Jump to month"
			aria-orientation="vertical"
			aria-valuemin={0}
			aria-valuemax={months.length - 1}
			aria-valuenow={shownIndex}
			aria-valuetext={monthLabel(shownMonth)}
			onpointerdown={pointerdown}
			onpointermove={pointermove}
			onpointerup={release}
			onpointercancel={release}
			onkeydown={keydown}
		>
			<Icon name="chevron-up" /><Icon name="chevron-down" />
			{#if dragging !== null}<span class="handle-pill" aria-hidden="true">{shortMonthLabel(shownMonth)}</span>{/if}
		</div>
	</div>
{/if}

<style>
	.column {
		display: none;
	}

	@media (min-width: 48rem) {
		.column {
			position: sticky;
			top: calc(var(--frame) + var(--panel-pad));
			display: block;
			width: var(--scrubber-w);
			height: calc(100svh - 2 * var(--frame) - 2 * var(--panel-pad) - var(--rail-bar-h) - var(--space-3));
			border-radius: var(--radius-full);
			touch-action: none;
			cursor: ns-resize;
			user-select: none;
		}

		.phone-track {
			display: none;
		}
	}

	@media (min-width: 64rem) {
		.column {
			height: calc(100svh - 2 * var(--frame) - 2 * var(--panel-pad));
		}
	}

	.tick {
		position: absolute;
		right: 0;
		width: var(--space-2);
		height: var(--scrub-tick);
		border-radius: var(--radius-full);
		background: var(--edge);
	}

	.year {
		position: absolute;
		left: 0;
		transform: translateY(-50%);
		color: var(--ink-muted);
		font-size: var(--text-xs);
		line-height: var(--text-xs-lh);
		font-variant-numeric: tabular-nums;
	}

	.pill,
	.handle-pill {
		position: absolute;
		padding: var(--space-1) var(--space-2);
		border-radius: var(--radius-full);
		background: var(--card);
		box-shadow: var(--shadow-knob);
		color: var(--ink);
		font-size: var(--text-xs);
		line-height: var(--text-xs-lh);
		font-weight: var(--weight-label);
		font-variant-numeric: tabular-nums;
		white-space: nowrap;
	}

	.pill {
		right: 0;
		transform: translateY(-50%);
	}

	.column:focus-visible {
		outline: var(--ring-width) solid var(--accent);
		outline-offset: var(--ring-offset);
	}

	.phone-track {
		position: fixed;
		z-index: 5;
		top: calc(var(--frame) + var(--tap) + var(--space-5) + env(safe-area-inset-top));
		bottom: calc(var(--rail-bar-h) + var(--space-6) + env(safe-area-inset-bottom));
		right: calc(var(--frame) + env(safe-area-inset-right));
		width: var(--tap);
		pointer-events: none;
		opacity: 0;
		transition: opacity var(--dur-fast) var(--ease);
	}

	.phone-track.shown {
		opacity: 1;
	}

	.handle {
		position: absolute;
		left: 0;
		display: grid;
		place-items: center;
		width: var(--tap);
		height: var(--tap);
		border-radius: var(--radius-full);
		background: var(--card);
		box-shadow: var(--shadow-knob);
		color: var(--ink-muted);
		transform: translateY(-50%);
		touch-action: none;
		user-select: none;
	}

	.handle :global(.icon) {
		width: var(--space-4);
		height: var(--space-3);
	}

	.shown .handle {
		pointer-events: auto;
	}

	.handle-pill {
		right: calc(100% + var(--space-2));
	}

	@media (forced-colors: active) {
		.pill,
		.handle,
		.handle-pill {
			border: 1px solid CanvasText;
		}

		.tick {
			background: CanvasText;
		}
	}
</style>
