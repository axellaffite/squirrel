<script lang="ts">
	import { project } from '#lib/photos/geo.ts';

	/** Same-origin static asset; the version suffix changes whenever it is rebuilt. */
	const LAND = '/maps/world-110m-v1.svg#land';

	interface Props {
		latitude: number;
		longitude: number;
		/** Where it is, for the accessible name ("Paris, France"); coordinates otherwise. */
		place?: string | null;
	}

	let { latitude, longitude, place = null }: Props = $props();

	const point = $derived(project(latitude, longitude));
	const label = $derived(`Map showing ${place ?? `${latitude.toFixed(4)}, ${longitude.toFixed(4)}`}`);
</script>

<!-- An offline locator: world outlines and a dot. Nothing is fetched from a map service. -->
<div class="locator" role="img" aria-label={label}>
	<svg viewBox="0 0 360 180" aria-hidden="true" focusable="false"><use href={LAND} /></svg>
	<span class="dot" style:left="{(point.x / 360) * 100}%" style:top="{(point.y / 180) * 100}%"></span>
</div>

<style>
	.locator {
		position: relative;
		overflow: hidden;
		border-radius: var(--radius-well);
		background: var(--tint);
	}

	svg {
		width: 100%;
		height: auto;
		fill: var(--tint-strong);
	}

	.dot {
		position: absolute;
		width: var(--locator-dot);
		height: var(--locator-dot);
		border-radius: var(--radius-full);
		background: var(--accent);
		box-shadow: 0 0 0 var(--ring-width) var(--card);
		transform: translate(-50%, -50%);
	}

	@media (forced-colors: active) {
		svg {
			fill: GrayText;
		}

		.dot {
			background: Highlight;
		}
	}
</style>
