<script lang="ts">
	import type { Snippet } from 'svelte';
	import Alert from '#lib/components/ui/Alert.svelte';
	import Button from '#lib/components/ui/Button.svelte';
	import Dialog from '#lib/components/ui/Dialog.svelte';

	interface Props {
		title: string;
		/** What will happen, under the title. */
		children: Snippet;
		confirmLabel: string;
		/** Resolves an error message to show, or null on success (the dialog then closes). */
		onConfirm: () => Promise<string | null>;
		/** Runs once the dialog closed, however it closed. */
		onClose: () => void;
	}

	let { title, children, confirmLabel, onConfirm, onClose }: Props = $props();

	// Closed through the native dialog, so focus returns to the opener before the parent unmounts it.
	let dialog: Dialog | undefined = $state();
	let open = $state(true);
	let busy = $state(false);
	let error: string | null = $state(null);

	async function confirm() {
		busy = true;
		error = null;
		error = await onConfirm();
		busy = false;
		if (!error) dialog?.close();
	}
</script>

<Dialog bind:this={dialog} bind:open {title} onclose={onClose}>
	<div class="body">
		<p class="muted">{@render children()}</p>
		{#if error}<Alert>{error}</Alert>{/if}
	</div>
	{#snippet actions()}
		<Button variant="secondary" onclick={() => dialog?.close()}>Cancel</Button>
		<Button variant="danger-solid" onclick={confirm} disabled={busy}>{confirmLabel}</Button>
	{/snippet}
</Dialog>

<style>
	.body {
		display: grid;
		gap: var(--space-3);
	}
</style>
