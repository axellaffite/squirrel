<script lang="ts">
	import Icon from '#lib/components/icons/Icon.svelte';
	import type { FileNode } from '#lib/files/api.ts';
	import { nodeIcon } from '#lib/files/nodeIcon.ts';
	import { thumbnailUrl } from '#lib/photos/api.ts';
	import { averageColor, isBlurhash } from '#lib/photos/blurhash.ts';

	interface Props {
		node: FileNode;
	}

	let { node }: Props = $props();

	let failed = $state(false);
	const thumb = $derived(failed ? null : (node.thumbnail ?? null));
	const tone = $derived(thumb && isBlurhash(thumb.blurhash) ? averageColor(thumb.blurhash) : undefined);
</script>

<!-- Fills the row's round well: the thumbnail over its average colour, else the type icon. -->
{#if thumb}
	<span class="thumb" style:background-color={tone}>
		<img
			src={thumbnailUrl(node.id, 'thumb', thumb.version)}
			alt=""
			loading="lazy"
			decoding="async"
			onerror={() => (failed = true)}
		/>
	</span>
{:else}
	<Icon name={nodeIcon(node)} />
{/if}

<style>
	.thumb {
		display: block;
		width: 100%;
		height: 100%;
		border-radius: var(--radius-full);
		overflow: hidden;
	}

	img {
		width: 100%;
		height: 100%;
		object-fit: cover;
	}
</style>
