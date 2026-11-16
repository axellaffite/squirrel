<script lang="ts">
	import { onDestroy } from 'svelte';
	import Icon from '#lib/components/icons/Icon.svelte';
	import { thumbnailUrl, type AssetSummary } from '#lib/photos/api.ts';
	import { averageColor, isBlurhash } from '#lib/photos/blurhash.ts';
	import { tileDateTime } from '#lib/photos/dates.ts';
	import { placeholders, whenIdle } from '#lib/photos/placeholder.ts';

	/** A press held this long on a touch screen selects the photo. */
	const LONG_PRESS_MS = 500;
	/** Moving further than this (px) means the finger is scrolling, not pressing. */
	const LONG_PRESS_SLOP = 10;

	interface Props {
		asset: AssetSummary;
		left: number;
		width: number;
		height: number;
		selected: boolean;
		/** Selection mode: a press toggles instead of opening. */
		selecting: boolean;
		onOpen: (asset: AssetSummary) => void;
		onToggle: (asset: AssetSummary, range: boolean) => void;
	}

	let { asset, left, width, height, selected, selecting, onOpen, onToggle }: Props = $props();

	/** The thumbnail version whose image failed to load; a new version gets a new chance. */
	let brokenVersion: number | null = $state(null);
	let loaded = $state(false);
	let blur: string | null = $state(null);
	let press: { x: number; y: number; timer: ReturnType<typeof setTimeout> } | null = null;
	let swallowClick = false;
	/** The last press came from a finger: its context menu (long press) is suppressed. */
	let touching = false;

	const broken = $derived(brokenVersion === asset.thumbnail.version);
	const thumbState = $derived(broken ? 'failed' : asset.thumbnail.state);
	const ready = $derived(thumbState === 'ready');
	const unavailable = $derived(thumbState === 'failed' || thumbState === 'unsupported');
	const hash = $derived(isBlurhash(asset.blurhash) ? asset.blurhash : null);
	const when = $derived(tileDateTime(asset.taken_at));
	const label = $derived(
		selecting
			? `Select ${asset.name}`
			: `Open photo taken ${when}${thumbState === 'pending' ? ', preview being prepared' : ''}`
	);

	// The average colour shows at once; the blurred image is decoded when the browser is idle.
	$effect(() => {
		if (!hash || loaded || unavailable) return;
		const known = placeholders.cached(hash);
		if (known !== undefined) {
			blur = known;
			return;
		}
		return whenIdle(() => (blur = placeholders.load(hash)));
	});

	function click(event: MouseEvent) {
		if (swallowClick) {
			swallowClick = false;
			return;
		}
		const onCircle = (event.target as Element).closest('.check') !== null;
		if (selecting || onCircle || event.shiftKey) onToggle(asset, event.shiftKey);
		else onOpen(asset);
	}

	function keydown(event: KeyboardEvent) {
		if (event.key !== ' ') return;
		// Space selects (and enters selection mode); Enter keeps the button's own click.
		event.preventDefault();
		onToggle(asset, event.shiftKey);
	}

	function keyup(event: KeyboardEvent) {
		if (event.key === ' ') event.preventDefault();
	}

	function cancelPress() {
		if (press) clearTimeout(press.timer);
		press = null;
	}

	function pointerdown(event: PointerEvent) {
		// A long press often ends without a click: never let it eat the next tap.
		swallowClick = false;
		touching = event.pointerType === 'touch';
		if (!touching) return;
		cancelPress();
		const timer = setTimeout(() => {
			press = null;
			swallowClick = true;
			onToggle(asset, false);
		}, LONG_PRESS_MS);
		press = { x: event.clientX, y: event.clientY, timer };
	}

	function pointermove(event: PointerEvent) {
		if (press && Math.hypot(event.clientX - press.x, event.clientY - press.y) > LONG_PRESS_SLOP) cancelPress();
	}

	onDestroy(cancelPress);
</script>

<button
	type="button"
	class="tile"
	class:selected
	class:selecting
	data-tile
	data-id={asset.id}
	style:left="{left}px"
	style:width="{width}px"
	style:height="{height}px"
	aria-label={label}
	aria-pressed={selecting ? selected : undefined}
	title={unavailable ? 'Preview unavailable' : undefined}
	onclick={click}
	onkeydown={keydown}
	onkeyup={keyup}
	onpointerdown={pointerdown}
	onpointermove={pointermove}
	onpointerup={cancelPress}
	onpointercancel={cancelPress}
	oncontextmenu={(event) => {
		if (touching) event.preventDefault();
	}}
>
	<span
		class="placeholder"
		style:background-color={hash && !unavailable ? averageColor(hash) : undefined}
		style:background-image={blur && !unavailable ? `url("${blur}")` : undefined}
	>
		{#if unavailable}<Icon name="image-off" />{:else if !hash && !ready}<Icon name="image" />{/if}
	</span>
	{#if ready}
		<img
			src={thumbnailUrl(asset.id, 'thumb', asset.thumbnail.version)}
			alt=""
			loading="lazy"
			decoding="async"
			draggable="false"
			onload={() => (loaded = true)}
			onerror={() => (brokenVersion = asset.thumbnail.version)}
		/>
	{/if}
	<span class="check" aria-hidden="true">{#if selected}<Icon name="check" />{/if}</span>
</button>

<style>
	.tile {
		position: absolute;
		top: 0;
		padding: 0;
		border: 0;
		border-radius: var(--radius-tile);
		background: transparent;
		cursor: pointer;
		-webkit-touch-callout: none;
		user-select: none;
		transition: box-shadow var(--dur-fast) var(--ease);
	}

	.placeholder,
	img {
		position: absolute;
		inset: 0;
		width: 100%;
		height: 100%;
		border-radius: var(--radius-tile);
	}

	.placeholder {
		display: grid;
		place-items: center;
		background-color: var(--tint);
		background-size: cover;
		color: var(--ink-muted);
	}

	/* The swap from blur to photo is instant: tiles never animate in. */
	img {
		object-fit: cover;
	}

	.check {
		position: absolute;
		top: var(--space-2);
		left: var(--space-2);
		display: grid;
		place-items: center;
		width: var(--select-check);
		height: var(--select-check);
		border-radius: var(--radius-full);
		background: var(--photo-shade);
		box-shadow: inset 0 0 0 var(--ring-width) var(--photo-ring);
		color: var(--on-accent);
		opacity: 0;
		transition:
			opacity var(--dur-fast) var(--ease),
			background-color var(--dur-fast) var(--ease);
	}

	.check :global(.icon) {
		width: calc(var(--select-check) - var(--space-2));
		height: calc(var(--select-check) - var(--space-2));
	}

	@media (hover: hover) {
		.tile:hover .check {
			opacity: 1;
		}
	}

	.selecting .check,
	.tile:focus-visible .check {
		opacity: 1;
	}

	.selected {
		box-shadow: var(--select-ring);
	}

	.selected .check {
		background: var(--accent);
		box-shadow: none;
		opacity: 1;
	}

	.tile:active {
		transform: translateY(var(--press));
	}

	@media (forced-colors: active) {
		.tile {
			border: 1px solid CanvasText;
		}

		.selected {
			outline: var(--ring-width) solid Highlight;
		}

		.check {
			border: 1px solid CanvasText;
		}
	}
</style>
