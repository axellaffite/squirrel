<script lang="ts">
	import { onMount } from 'svelte';
	import ConfirmDialog from '#lib/components/ConfirmDialog.svelte';
	import Page from '#lib/components/shell/Page.svelte';
	import Alert from '#lib/components/ui/Alert.svelte';
	import Toggle from '#lib/components/ui/Toggle.svelte';
	import { countsText, FacesSwitch } from '#lib/explore/facesSwitch.svelte.ts';
	import { mediaStatus } from '#lib/photos/api.ts';

	/** While photos are being analysed, the counts are read again this often. */
	const REFRESH_MS = 5000;

	const sw = new FacesSwitch();

	const counts = $derived(sw.settings ? countsText({ people: sw.settings.people_count, faces: sw.settings.faces_count }) : '');

	onMount(() => {
		void sw.load();
		const timer = setInterval(async () => {
			if (document.visibilityState !== 'visible' || !sw.settings?.faces_enabled) return;
			const status = await mediaStatus();
			if (status.ok && status.data.ml.queued > 0) void sw.load();
		}, REFRESH_MS);
		const visible = () => {
			if (document.visibilityState === 'visible') void sw.load();
		};
		document.addEventListener('visibilitychange', visible);
		return () => {
			clearInterval(timer);
			document.removeEventListener('visibilitychange', visible);
		};
	});
</script>

<svelte:head><title>Faces · Squirrel</title></svelte:head>

<Page title="Faces">
	{#snippet description()}Group your photos by the people in them.{/snippet}

	{#if sw.error}<Alert>{sw.error}</Alert>{/if}
	<p class="visually-hidden" aria-live="polite">{sw.notice ?? ''}</p>
	{#if sw.notice}<Alert tone="success" role="note">{sw.notice}</Alert>{/if}

	{#if sw.settings === null}
		{#if !sw.error}<p class="muted" aria-live="polite">Loading…</p>{/if}
	{:else}
		<section class="well" aria-label="Face recognition">
			<!-- The switch always shows the model's state: a flip is only a request to the model. -->
			<Toggle label="Recognise faces in my photos" bind:checked={() => sw.checked, () => {}} disabled={sw.busy} onchange={(next) => void sw.toggle(next)} />
			<p class="muted">
				Faces are found and grouped on this server only: nothing is sent anywhere, and nobody else sees your
				people.
				{#if sw.settings.faces_enabled}Turning this off deletes {counts}.{:else}Turning this on analyses your photos in the background.{/if}
			</p>
		</section>
	{/if}
</Page>

{#if sw.confirming}
	<ConfirmDialog
		title="Turn off face recognition?"
		confirmLabel="Turn off and delete"
		onConfirm={() => sw.turnOff()}
		onClose={() => sw.closed()}
	>
		{#if sw.confirmCounts}
			This deletes {countsText(sw.confirmCounts)}, with every name and correction you made. Turning it on again finds
			the faces anew, without your names.
		{:else}
			Counting your faces…
		{/if}
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
