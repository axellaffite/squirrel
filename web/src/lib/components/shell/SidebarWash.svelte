<script lang="ts">
	import {
		photoColors,
		presetColors,
		scrimAlpha,
		washBackground,
		type SidebarChoice
	} from '#lib/sidebar.ts';
	import { decodeBlurhash } from '#lib/photos/blurhash.ts';
	import { untrack } from 'svelte';

	interface Props {
		choice: SidebarChoice;
	}

	let { choice }: Props = $props();

	/** Blurhash decoded to a small PNG, scaled up smoothly by the browser. */
	function photoBackground(hash: string): string {
		const width = 32;
		const height = 48;
		const canvas = document.createElement('canvas');
		canvas.width = width;
		canvas.height = height;
		const ctx = canvas.getContext('2d');
		if (!ctx) return 'var(--rail)';
		ctx.putImageData(new ImageData(new Uint8ClampedArray(decodeBlurhash(hash, width, height)), width, height), 0, 0);
		return `center / cover no-repeat url(${canvas.toDataURL()})`;
	}

	function layerFor(c: SidebarChoice): { key: string; background: string; alpha: number } {
		if (c.kind === 'photo') {
			try {
				return { key: `photo:${c.id}`, background: photoBackground(c.blurhash), alpha: scrimAlpha(photoColors(c.blurhash)) };
			} catch {
				// A broken hash falls back to the graphite rail.
			}
		}
		const preset = c.kind === 'preset' ? c.preset : 'graphite';
		return { key: preset, background: washBackground(preset), alpha: scrimAlpha(presetColors(preset)) };
	}

	type Layer = ReturnType<typeof layerFor>;

	const layer = $derived(layerFor(choice));
	/** The previous wash stays underneath while the new one fades in over it. */
	let layers: Layer[] = $state([]);
	$effect(() => {
		const next = layer;
		const kept = untrack(() => layers.filter((l) => l.key !== next.key).slice(-1));
		layers = [...kept, next];
	});
</script>

<!-- Decorative: the wash and its scrim sit behind the sidebar's content. A new wash cross-fades in. -->
<div class="wash" aria-hidden="true">
	{#each layers as l (l.key)}
		<div class="layer" style:background={l.background}></div>
	{/each}
	<div class="scrim" style:opacity={layer.alpha}></div>
</div>

<style>
	.wash {
		position: absolute;
		inset: 0;
		z-index: -1;
		overflow: hidden;
		border-radius: inherit;
		background: var(--rail);
	}

	.layer {
		position: absolute;
		inset: 0;
		animation: wash-in var(--dur-slow) var(--ease) both;
	}

	.scrim {
		position: absolute;
		inset: 0;
		background: var(--rail-scrim);
		transition: opacity var(--dur-slow) var(--ease);
	}

	@keyframes wash-in {
		from {
			opacity: 0;
		}
	}
</style>
