<script lang="ts">
	import { onMount, untrack } from 'svelte';
	import RulesEditor from '#lib/components/photos/RulesEditor.svelte';
	import RulesPreview from '#lib/components/photos/RulesPreview.svelte';
	import Alert from '#lib/components/ui/Alert.svelte';
	import Button from '#lib/components/ui/Button.svelte';
	import Dialog from '#lib/components/ui/Dialog.svelte';
	import TextField from '#lib/components/ui/TextField.svelte';
	import { listPeople, type Person } from '#lib/explore/api.ts';
	import { formFromRules, rulesBody, rulesProblem, type RulesForm } from '#lib/photos/albumRules.ts';
	import { albumNameProblem } from '#lib/photos/albums.ts';
	import type { AlbumRules } from '#lib/photos/api.ts';

	interface Props {
		title: string;
		submitLabel: string;
		/** Asks for a name too (a new album). */
		named?: boolean;
		/** The rules to start from (editing). */
		initial?: AlbumRules | null;
		/** Resolves to an error message, or null on success. */
		onSubmit: (name: string, rules: AlbumRules) => Promise<string | null>;
		/** Runs once the dialog closed, however it closed. */
		onClose: () => void;
	}

	let { title, submitLabel, named = false, initial = null, onSubmit, onClose }: Props = $props();

	let dialog: Dialog | undefined = $state();
	let open = $state(true);
	let name = $state('');
	let nameError: string | null = $state(null);
	// The form starts from the album's rules, then belongs to the user.
	let form: RulesForm = $state.raw(untrack(() => formFromRules(initial)));
	let people: Person[] = $state.raw([]);
	let error: string | null = $state(null);
	let busy = $state(false);
	const uid = $props.id();
	const formId = `rules-form-${uid}`;

	// Without the people list (it failed) chosen people show as "Unnamed"; the rules still work.
	onMount(async () => {
		const result = await listPeople(true);
		if (result.ok) people = result.data;
	});

	async function submit(event: SubmitEvent) {
		event.preventDefault();
		nameError = named ? albumNameProblem(name) : null;
		error = rulesProblem(form);
		if (nameError || error) return;
		busy = true;
		try {
			error = await onSubmit(name.trim(), rulesBody(form));
			if (!error) dialog?.close();
		} finally {
			busy = false;
		}
	}
</script>

<Dialog bind:this={dialog} bind:open {title} onclose={onClose}>
	<!-- The editor stays outside the form: Enter in a picker's search field must not submit it. -->
	<div class="body">
		<form id={formId} class="name" onsubmit={submit} novalidate>
			{#if named}
				<TextField label="Name" bind:value={name} error={nameError} required autofocus autocomplete="off" spellcheck="false" />
			{/if}
		</form>
		<p class="muted">Photos matching every condition below show up on their own, now and later.</p>
		<RulesEditor {form} {people} onChange={(next) => (form = next)} />
		<RulesPreview {form} />
		{#if error}<Alert>{error}</Alert>{/if}
	</div>
	{#snippet actions()}
		<Button variant="secondary" onclick={() => dialog?.close()}>Cancel</Button>
		<Button type="submit" form={formId} disabled={busy}>{submitLabel}</Button>
	{/snippet}
</Dialog>

<style>
	.body {
		display: grid;
		gap: var(--space-4);
	}

	/* No box of its own: without a name field it takes no gap. */
	.name {
		display: contents;
	}
</style>
