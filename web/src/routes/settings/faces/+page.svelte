<script lang="ts">
	import { onMount } from 'svelte';
	import ConfirmDialog from '#lib/components/ConfirmDialog.svelte';
	import Page from '#lib/components/shell/Page.svelte';
	import Alert from '#lib/components/ui/Alert.svelte';
	import Toggle from '#lib/components/ui/Toggle.svelte';
	import { getMlSettings, setFacesEnabled, type MlSettings } from '#lib/explore/people.ts';

	let settings = $state<MlSettings | null>(null);
	let error: string | null = $state(null);
	let notice: string | null = $state(null);
	let confirming = $state(false);
	let busy = $state(false);
	/** What the switch shows: it only moves once the server agreed. */
	let checked = $state(false);

	const counts = $derived(
		settings
			? `${settings.people_count} ${settings.people_count === 1 ? 'person' : 'people'} and ${settings.faces_count} ${settings.faces_count === 1 ? 'face' : 'faces'}`
			: ''
	);

	async function load() {
		const result = await getMlSettings();
		if (result.ok) {
			settings = result.data;
			checked = result.data.faces_enabled;
		} else error = result.error;
	}

	async function changed(next: boolean) {
		// The switch flipped itself: put it back until the change is done (or confirmed).
		checked = !next;
		notice = null;
		if (!next) {
			confirming = true;
			return;
		}
		busy = true;
		const result = await setFacesEnabled(true);
		busy = false;
		if (!result.ok) {
			error = result.error;
			return;
		}
		settings = result.data;
		checked = true;
		error = null;
		notice = 'Face recognition is on. Your photos are being analysed; people appear in Explore as faces are found.';
	}

	async function turnOff(): Promise<string | null> {
		const result = await setFacesEnabled(false);
		if (!result.ok) return result.error;
		settings = result.data;
		checked = false;
		notice = 'Face recognition is off. Every face and person was deleted.';
		return null;
	}

	onMount(load);
</script>

<svelte:head><title>Faces · Squirrel</title></svelte:head>

<Page title="Faces">
	{#snippet description()}Group your photos by the people in them.{/snippet}

	{#if error}<Alert>{error}</Alert>{/if}
	<p class="visually-hidden" aria-live="polite">{notice ?? ''}</p>
	{#if notice}<Alert tone="success" role="note">{notice}</Alert>{/if}

	{#if settings === null}
		{#if !error}<p class="muted" aria-live="polite">Loading…</p>{/if}
	{:else}
		<section class="well" aria-label="Face recognition">
			<Toggle label="Recognise faces in my photos" bind:checked disabled={busy} onchange={changed} />
			<p class="muted">
				Faces are found and grouped on this server only: nothing is sent anywhere, and nobody else sees your
				people.
				{#if settings.faces_enabled}Turning this off deletes {counts}.{:else}Turning this on analyses your photos in the background.{/if}
			</p>
		</section>
	{/if}
</Page>

{#if confirming && settings}
	<ConfirmDialog
		title="Turn off face recognition?"
		confirmLabel="Turn off and delete"
		onConfirm={turnOff}
		onClose={() => (confirming = false)}
	>
		This deletes {counts}, with every name and correction you made. Turning it on again finds the faces anew, without
		your names.
	</ConfirmDialog>
{/if}

<style>
	.well {
		display: grid;
		gap: var(--space-3);
		max-width: var(--measure);
		padding: var(--well-pad);
		border-radius: var(--radius-well);
		background: var(--tint);
	}
</style>
