<script lang="ts">
	import Comments from '#lib/components/sharing/Comments.svelte';
	import Dialog from '#lib/components/ui/Dialog.svelte';
	import type { Subject } from '#lib/sharing/api.ts';

	interface Props {
		subject: Subject;
		name: string;
		/** Runs once the sheet closed, however it closed. */
		onClose: () => void;
	}

	let { subject, name, onClose }: Props = $props();

	let open = $state(true);
</script>

<Dialog bind:open title={`Comments on “${name}”`} onclose={onClose}>
	<Comments {subject} {name} />
</Dialog>
