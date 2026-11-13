<script lang="ts">
	import { onMount, untrack } from 'svelte';
	import { nameProblem } from '#lib/files/names.ts';

	let {
		title,
		initial = '',
		submitLabel,
		onSubmit,
		onClose
	}: {
		title: string;
		initial?: string;
		submitLabel: string;
		/** Resolves to an error message, or null on success. */
		onSubmit: (name: string) => Promise<string | null>;
		onClose: () => void;
	} = $props();

	let dialog: HTMLDialogElement;
	// The field starts from the current name, then belongs to the user.
	let name = $state(untrack(() => initial));
	let error: string | null = $state(null);
	let busy = $state(false);

	onMount(() => dialog.showModal());

	async function submit(event: SubmitEvent) {
		event.preventDefault();
		error = nameProblem(name);
		if (error) return;
		busy = true;
		try {
			error = await onSubmit(name.trim());
			if (!error) onClose();
		} finally {
			busy = false;
		}
	}
</script>

<dialog bind:this={dialog} onclose={onClose} aria-labelledby="name-dialog-title">
	<form onsubmit={submit}>
		<h2 id="name-dialog-title">{title}</h2>
		<label>Name <input bind:value={name} required autocomplete="off" /></label>
		{#if error}<p class="error" role="alert">{error}</p>{/if}
		<div class="dialog-actions">
			<button type="button" class="link" onclick={() => dialog.close()}>Cancel</button>
			<button type="submit" disabled={busy}>{submitLabel}</button>
		</div>
	</form>
</dialog>
