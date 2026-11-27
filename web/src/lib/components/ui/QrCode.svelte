<script lang="ts">
	import { qrShape } from '#lib/qr.ts';

	interface Props {
		/** What the code says when scanned. */
		value: string;
		/** What it is, for screen readers. */
		label: string;
	}

	let { value, label }: Props = $props();
	const shape = $derived(qrShape(value));
</script>

<svg class="qr" viewBox="0 0 {shape.size} {shape.size}" role="img" aria-label={label} shape-rendering="crispEdges">
	<path d={shape.path} />
</svg>

<style>
	/* Dark on light in both themes: scanners do not all read an inverted code. In the dark
	   theme the two tokens swap, so the code stays ink on paper. */
	.qr {
		display: block;
		width: var(--qr-side);
		height: var(--qr-side);
		--qr-side: calc(3 * var(--space-8));
		border-radius: var(--radius-card);
		background: light-dark(var(--card), var(--ink));
		fill: light-dark(var(--ink), var(--card));
	}

	@media (forced-colors: active) {
		.qr {
			background: Canvas;
			fill: CanvasText;
		}
	}
</style>
