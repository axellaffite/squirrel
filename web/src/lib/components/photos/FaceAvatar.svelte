<script lang="ts">
	import Icon from '#lib/components/icons/Icon.svelte';
	import { cropSource, cropStyle, type FaceCrop } from '#lib/explore/faces.ts';

	interface Props {
		/** The face to show; null shows the `user` icon. */
		face: FaceCrop | null | undefined;
		size?: 'sm' | 'md' | 'lg';
		/** The photo's width / height when known (no jump on the first frame). */
		photoAspect?: number | null;
	}

	let { face, size = 'sm', photoAspect = null }: Props = $props();

	let failed = $state(false);
	/** The avatar's width on screen (px), to choose the thumb or the preview. */
	let width = $state(0);
	/** The photo's width / height once it loaded, to keep the crop inside it. */
	let loadedAspect: number | null = $state(null);
	const aspect = $derived(loadedAspect ?? (photoAspect && photoAspect > 0 ? photoAspect : 1));
	const source = $derived(face && width > 0 ? cropSource(face, width, aspect) : null);
	const style = $derived(face ? cropStyle(face.box, aspect) : null);

	// Another face (or size): a past failure says nothing about the new image.
	$effect(() => {
		void source;
		failed = false;
	});

	// Another face: its photo's shape is not known yet.
	$effect(() => {
		void face?.face_id;
		loadedAspect = null;
	});

	function loaded(event: Event) {
		const img = event.currentTarget as HTMLImageElement;
		if (img.naturalWidth > 0 && img.naturalHeight > 0) loadedAspect = img.naturalWidth / img.naturalHeight;
	}
</script>

<!-- MASTER "Face avatar": a client-side crop of the photo's thumbnail; the server stores no face images. -->
<span class="avatar {size}" bind:clientWidth={width}>
	<Icon name="user" />
	{#if source && style && !failed}
		<img
			src={source}
			alt=""
			loading="lazy"
			decoding="async"
			style:height="{style.height}%"
			style:top="{style.top}%"
			style:transform="translateX({style.shift}%)"
			onload={loaded}
			onerror={() => (failed = true)}
		/>
	{/if}
</span>

<style>
	.avatar {
		position: relative;
		display: inline-grid;
		place-items: center;
		flex: none;
		overflow: hidden;
		border-radius: var(--radius-full);
		background: var(--tint);
		color: var(--ink-muted);
	}

	.sm {
		width: var(--face-sm);
		height: var(--face-sm);
	}

	.md {
		width: var(--face-md);
		height: var(--face-md);
	}

	.lg {
		width: var(--face-lg);
		height: var(--face-lg);
	}

	img {
		position: absolute;
		left: 50%;
		width: auto;
		max-width: none;
	}

	@media (forced-colors: active) {
		.avatar {
			border: 1px solid CanvasText;
		}
	}
</style>
