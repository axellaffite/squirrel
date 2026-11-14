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
	<span class="fill" class:warn={ratio >= warnAt} style:transform="scaleX({ratio})"></span>
</div>

<style>
	.meter {
		position: relative;
		height: var(--meter-track);
		overflow: hidden;
		border-radius: var(--radius-full);
		background: var(--surface);
		box-shadow: var(--inset-2);
	}

	.fill {
		position: absolute;
		inset: 0;
		border-radius: inherit;
		background: var(--accent);
		transform-origin: left center;
		transition:
			transform var(--dur) var(--ease),
			background-color var(--dur) var(--ease);
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
