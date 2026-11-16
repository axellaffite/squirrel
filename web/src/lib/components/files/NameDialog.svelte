<script lang="ts">
	import { untrack } from 'svelte';
	import Button from '#lib/components/ui/Button.svelte';
	import Dialog from '#lib/components/ui/Dialog.svelte';
	import TextField from '#lib/components/ui/TextField.svelte';
	import { nameProblem } from '#lib/files/names.ts';

	interface Props {
		title: string;
		initial?: string;
		submitLabel: string;
		/** Checks the name before sending it (file-name rules by default). */
		validate?: (name: string) => string | null;
		/** Resolves to an error message, or null on success. */
		onSubmit: (name: string) => Promise<string | null>;
		/** Runs once the dialog closed, however it closed. */
		onClose: () => void;
	}

	let { title, initial = '', submitLabel, validate = nameProblem, onSubmit, onClose }: Props = $props();

	let dialog: Dialog | undefined = $state();
	let open = $state(true);
	// The field starts from the current name, then belongs to the user.
	let name = $state(untrack(() => initial));
	let error: string | null = $state(null);
	let busy = $state(false);
	const uid = $props.id();
	const formId = `name-form-${uid}`;

	async function submit(event: SubmitEvent) {
		event.preventDefault();
		error = validate(name);
		if (error) return;
		busy = true;
		try {
			error = await onSubmit(name.trim());
			if (!error) dialog?.close();
		} finally {
			busy = false;
		}
	}
</script>

<Dialog bind:this={dialog} bind:open {title} onclose={onClose}>
	<form id={formId} onsubmit={submit} novalidate>
		<TextField
			label="Name"
			bind:value={name}
			{error}
			required
			autofocus
			autocomplete="off"
			spellcheck="false"
		/>
	</form>
	{#snippet actions()}
		<Button variant="secondary" onclick={() => dialog?.close()}>Cancel</Button>
		<Button type="submit" form={formId} disabled={busy}>{submitLabel}</Button>
	{/snippet}
</Dialog>
