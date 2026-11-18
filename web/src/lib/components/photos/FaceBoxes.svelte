<script lang="ts">
	import type { Face } from '#lib/explore/api.ts';
	import { faceName, readingOrder } from '#lib/explore/faces.ts';

	interface Props {
		faces: readonly Face[];
		onPick: (face: Face) => void;
	}

	let { faces, onPick }: Props = $props();

	const ordered = $derived(readingOrder(faces));
	/** Smaller boxes sit above larger ones, so where their hit areas overlap the small face wins. */
	const layer = $derived.by(() => {
		const bySize = [...faces].sort((a, b) => b.box.width * b.box.height - a.box.width * a.box.height);
		return new Map(bySize.map((face, index) => [face.id, index + 1]));
	});
</script>

<!-- MASTER "Face box": a --photo-ring outline over each face, as buttons in reading order. -->
<div class="boxes" role="group" aria-label="Faces in this photo">
	{#each ordered as face (face.id)}
		<button
			type="button"
			class="box"
			style:left="{face.box.x * 100}%"
			style:top="{face.box.y * 100}%"
			style:width="{face.box.width * 100}%"
			style:height="{face.box.height * 100}%"
			style:z-index={layer.get(face.id)}
			aria-label={faceName(face.person?.name)}
			title={faceName(face.person?.name)}
			onclick={() => onPick(face)}
		></button>
	{/each}
</div>

<style>
	.boxes {
		position: absolute;
		inset: 0;
	}

	.box {
		position: absolute;
		padding: 0;
		border: var(--face-box) solid var(--photo-ring);
		border-radius: var(--radius-tile);
		background: transparent;
		cursor: pointer;
		transition: border-color var(--dur-fast) var(--ease);
	}

	/* A face box is often small: its hit area grows to the 44 px minimum around it. */
	.box::before {
		content: '';
		position: absolute;
		inset: 50% auto auto 50%;
		width: max(100%, var(--tap));
		height: max(100%, var(--tap));
		transform: translate(-50%, -50%);
	}

	@media (hover: hover) {
		.box:hover {
			border-color: var(--rail-ink);
		}
	}

	.box:focus-visible {
		outline: var(--ring-width) solid var(--accent);
		outline-offset: var(--ring-offset);
	}

	@media (forced-colors: active) {
		.box {
			border-color: Highlight;
		}
	}
</style>
