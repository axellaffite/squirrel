<script lang="ts">
	import type { Snippet } from 'svelte';

	let { enabled, onFiles, children }: { enabled: boolean; onFiles: (files: File[]) => void; children: Snippet } =
		$props();
	let over = $state(false);

	function hasFiles(event: DragEvent): boolean {
		return event.dataTransfer?.types.includes('Files') ?? false;
	}

	function dragOver(event: DragEvent) {
		if (!enabled || !hasFiles(event)) return;
		event.preventDefault();
		over = true;
	}

	function drop(event: DragEvent) {
		over = false;
		if (!enabled || !hasFiles(event)) return;
		event.preventDefault();
		// Folders dropped here arrive as zero-byte entries without a type in some browsers;
		// only regular files are uploaded (folder upload is not part of Phase 2).
		const files = Array.from(event.dataTransfer?.files ?? []).filter((file) => file.type !== '' || file.size > 0);
		if (files.length > 0) onFiles(files);
	}
</script>

<div
	class="dropzone"
	class:over
	role="region"
	aria-label="Drop files here to upload"
	ondragover={dragOver}
	ondragleave={() => (over = false)}
	ondrop={drop}
>
	{@render children()}
</div>

<style>
	.dropzone { border: 2px dashed transparent; border-radius: var(--radius-md); padding: var(--space-1); }
	.dropzone.over { border-color: var(--accent); }
</style>
