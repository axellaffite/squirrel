<script lang="ts">
	import Icon from '#lib/components/icons/Icon.svelte';
	import Button from '#lib/components/ui/Button.svelte';
	import { downloadLabel, downloadPhotos } from '#lib/photos/download.ts';

	interface Props {
		/** The selected photo ids, in the order the zip keeps. */
		ids: readonly string[];
		/** The selected photos loaded on the page, for a single photo's name. */
		known: readonly { id: string; name: string }[];
		/** A message when the download could not start, null when it did. */
		onResult: (error: string | null) => void;
	}

	let { ids, known, onResult }: Props = $props();
	/** The zip is being prepared: a second click would ask for another one. */
	let busy = $state(false);

	async function download() {
		if (busy) return;
		busy = true;
		const error = await downloadPhotos(ids);
		busy = false;
		onResult(error);
	}
</script>

<Button variant="secondary" size="sm" disabled={busy} aria-busy={busy} aria-label={downloadLabel(known, ids.length)} onclick={download}>
	<Icon name="download" /><span class="label">{busy ? 'Preparing…' : 'Download'}</span>
</Button>
