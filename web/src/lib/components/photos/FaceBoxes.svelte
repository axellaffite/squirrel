<script lang="ts">
	import type { Face } from '#lib/explore/api.ts';
	import { faceName, readingOrder } from '#lib/explore/faces.ts';

	interface Props {
		faces: readonly Face[];
		onPick: (face: Face) => void;
	}

	let { faces, onPick }: Props = $props();

	const ordered = $derived(readingOrder(faces));
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
		box-shadow: 0 0 0 1px var(--photo-shade);
		cursor: pointer;
		transition:
			border-color var(--dur-fast) var(--ease),
			box-shadow var(--dur-fast) var(--ease);
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
