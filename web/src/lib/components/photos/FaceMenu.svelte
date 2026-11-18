<script lang="ts">
	import Icon from '#lib/components/icons/Icon.svelte';
	import FaceAvatar from '#lib/components/photos/FaceAvatar.svelte';
	import Alert from '#lib/components/ui/Alert.svelte';
	import Button from '#lib/components/ui/Button.svelte';
	import Dialog from '#lib/components/ui/Dialog.svelte';
	import type { Face } from '#lib/explore/api.ts';
	import { faceName, personLabel } from '#lib/explore/faces.ts';
	import { unassignFace } from '#lib/explore/people.ts';

	interface Props {
		face: Face;
		/** "This is…": the person picker opens once this menu closed. */
		onChoosePerson: () => void;
		/** The face's person changed (reload the faces). */
		onChanged: (message: string) => void;
		onClose: () => void;
	}

	let { face, onChoosePerson, onChanged, onClose }: Props = $props();

	let dialog: Dialog | undefined = $state();
	let open = $state(true);
	let busy = $state(false);
	let error: string | null = $state(null);
	let choose = false;

	const crop = $derived({ face_id: face.id, photo_id: face.photo_id, thumbnail_version: face.thumbnail_version, box: face.box });
	const who = $derived(face.person ? personLabel(face.person.name) : null);

	async function notThem() {
		busy = true;
		const result = await unassignFace(face.id);
		busy = false;
		if (!result.ok) {
			error = result.error;
			return;
		}
		onChanged(`This face is no longer ${who}.`);
		dialog?.close();
	}

	function thisIs() {
		choose = true;
		dialog?.close();
	}

	function closed() {
		if (choose) onChoosePerson();
		onClose();
	}
</script>

<Dialog bind:this={dialog} bind:open title={faceName(face.person?.name)} onclose={closed}>
	<div class="body">
		<div class="face">
			<FaceAvatar face={crop} size="lg" />
			{#if who && face.person}
				<a class="person" href={`/explore/people/${face.person.id}`}>{who}<Icon name="chevron-right" /></a>
			{:else}
				<p class="muted">Nobody is named for this face yet.</p>
			{/if}
		</div>
		{#if error}<Alert>{error}</Alert>{/if}
	</div>
	{#snippet actions()}
		{#if who}<Button variant="secondary" disabled={busy} onclick={notThem}>Not {who}</Button>{/if}
		<Button onclick={thisIs} disabled={busy}>This is…</Button>
	{/snippet}
</Dialog>

<style>
	.body {
		display: grid;
		gap: var(--space-3);
	}

	.face {
		display: flex;
		align-items: center;
		gap: var(--space-4);
	}

	.person {
		display: inline-flex;
		align-items: center;
		gap: var(--space-1);
		min-height: var(--tap);
		border-radius: var(--radius-sm);
		color: var(--ink);
		font-size: var(--text-lg);
		line-height: var(--text-lg-lh);
		font-weight: var(--weight-label);
		text-underline-offset: var(--underline-offset);
	}

	.person:focus-visible {
		outline: var(--ring-width) solid var(--accent);
		outline-offset: var(--ring-offset);
	}
</style>
