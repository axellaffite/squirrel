<script lang="ts">
	import type { Snippet } from 'svelte';
	import Icon from '#lib/components/icons/Icon.svelte';
	import { collectDroppedFiles, dragDepth } from '#lib/files/drop.ts';

	interface Props {
		enabled: boolean;
		onFiles: (files: File[]) => void;
		children: Snippet;
	}

	let { enabled, onFiles, children }: Props = $props();

	const depth = dragDepth();
	let over = $state(false);
	let notice: string | null = $state(null);

	function hasFiles(event: DragEvent): boolean {
		return event.dataTransfer?.types.includes('Files') ?? false;
	}

	function dragEnter(event: DragEvent) {
		if (!hasFiles(event)) return;
		event.preventDefault();
		over = depth.enter() && enabled;
	}

	function dragOver(event: DragEvent) {
		if (!hasFiles(event)) return;
		// Also when disabled, so a stray drop does not make the browser open the file.
		event.preventDefault();
		if (event.dataTransfer) event.dataTransfer.dropEffect = enabled ? 'copy' : 'none';
	}

	function dragLeave(event: DragEvent) {
		if (!hasFiles(event)) return;
		over = depth.leave() && enabled;
	}

	function drop(event: DragEvent) {
		over = depth.reset();
		if (!hasFiles(event)) return;
		event.preventDefault();
		if (!enabled) {
			notice = 'Open a folder to add files.';
			return;
		}
		const { files, skippedFolders } = collectDroppedFiles(event.dataTransfer?.items, event.dataTransfer?.files ?? []);
		notice =
			skippedFolders > 0
				? "Folders can't be uploaded yet. Drop the files inside them instead."
				: null;
		if (files.length > 0) onFiles(files);
	}
</script>

<div
	class="dropzone"
	class:over
	role="region"
	aria-label="Drop files here to upload"
	ondragenter={dragEnter}
	ondragover={dragOver}
	ondragleave={dragLeave}
	ondrop={drop}
>
	<p class="notice" class:shown={notice} role="status">
		{#if notice}<Icon name="alert" />{notice}{/if}
	</p>
	{@render children()}
</div>

<style>
	.dropzone {
		display: grid;
		padding: var(--space-2);
		margin: calc(-1 * var(--space-2));
		border-radius: var(--radius-well);
		transition:
			background-color var(--dur-fast) var(--ease),
			box-shadow var(--dur-fast) var(--ease);
	}

	.over {
		background: var(--tint);
		box-shadow: var(--select-ring);
	}

	.notice {
		display: flex;
		gap: var(--space-2);
		align-items: flex-start;
		color: var(--warning);
		font-size: var(--text-sm);
		line-height: var(--text-sm-lh);
		font-weight: var(--weight-label);
	}

	.notice.shown {
		margin-bottom: var(--space-3);
	}

	@media (forced-colors: active) {
		.over {
			outline: var(--ring-width) dashed Highlight;
		}
	}
</style>
