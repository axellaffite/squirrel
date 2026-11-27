<script lang="ts">
	import Icon from '#lib/components/icons/Icon.svelte';
	import PublicPreview from '#lib/components/public/PublicPreview.svelte';
	import Button from '#lib/components/ui/Button.svelte';
	import Dialog from '#lib/components/ui/Dialog.svelte';
	import { contentUrl } from '#lib/public/api.ts';
	import type { ViewedFile } from '#lib/public/view.ts';

	interface Props {
		token: string;
		file: ViewedFile;
		onClose: () => void;
	}

	let { token, file, onClose }: Props = $props();

	let open = $state(true);
</script>

<Dialog bind:open title={file.name} size="wide" onclose={onClose}>
	<PublicPreview {token} {file} />
	{#snippet actions()}
		<Button variant="secondary" href={contentUrl(token, file.id)} target="_blank" rel="noopener"><Icon name="external-link" />Open</Button>
		<Button href={contentUrl(token, file.id, true)} download><Icon name="download" />Download</Button>
	{/snippet}
</Dialog>
