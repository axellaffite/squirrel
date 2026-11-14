<script lang="ts">
	interface Props {
		/** Accessible name, e.g. "Storage used". */
		label: string;
		value: number;
		max: number;
		/** Human-readable value for assistive tech and the caption, e.g. "1.2 GB of 10 GB". */
		valueText: string;
		/** Switches the fill to the warning colour from this ratio (0–1). */
		warnAt?: number;
	}

	let { label, value, max, valueText, warnAt = 0.9 }: Props = $props();

	const ratio = $derived(max > 0 ? Math.min(Math.max(value / max, 0), 1) : 0);
</script>

<div
	class="meter"
	role="meter"
	aria-label={label}
	aria-valuemin={0}
	aria-valuemax={max}
	aria-valuenow={Math.min(value, max)}
	aria-valuetext={valueText}
>
	<span class="fill" class:warn={ratio >= warnAt} class:empty={ratio === 0} style:--ratio={ratio}></span>
</div>

<style>
	/* A pill track (tint) holding a white pill fill, after the storage bar of the reference. */
	.meter {
		position: relative;
		height: var(--meter-track);
		padding: var(--meter-pad);
		border-radius: var(--radius-full);
		background: var(--track);
	}

	.fill {
		display: block;
		height: 100%;
		width: 100%;
		border-radius: var(--radius-full);
		background: var(--fill);
		box-shadow: var(--shadow-knob);
		/* Clipped rather than scaled, so the pill keeps round ends; never narrower than a circle. */
		--knob: calc(var(--meter-track) - 2 * var(--meter-pad));
		clip-path: inset(0 calc((100% - var(--knob)) * (1 - var(--ratio))) 0 0 round var(--radius-full));
		transition:
			clip-path var(--dur) var(--ease),
			background-color var(--dur) var(--ease);
	}

	.empty {
		visibility: hidden;
	}

	.warn {
		background: var(--warning);
	}

	@media (forced-colors: active) {
		.meter {
			border: 1px solid CanvasText;
		}

		.fill {
			background: Highlight;
		}
	}
</style>
