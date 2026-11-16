<script lang="ts">
	import { onMount } from 'svelte';
	import { page } from '$app/state';
	import Icon from '#lib/components/icons/Icon.svelte';
	import PhotosTabs from '#lib/components/photos/PhotosTabs.svelte';
	import Page from '#lib/components/shell/Page.svelte';
	import Alert from '#lib/components/ui/Alert.svelte';
	import Button from '#lib/components/ui/Button.svelte';
	import EmptyState from '#lib/components/ui/EmptyState.svelte';
	import { listFolder, type FileNode } from '#lib/files/api.ts';
	import { listMonths, type MonthCount } from '#lib/photos/api.ts';
	import { enqueue } from '#lib/upload/uploads.svelte.ts';

	let months = $state<MonthCount[] | null>(null);
	let error: string | null = $state(null);
	let photosFolder = $state<FileNode | null>(null);
	let picker: HTMLInputElement | undefined = $state();

	const total = $derived(months?.reduce((sum, entry) => sum + entry.count, 0) ?? null);

	async function load() {
		const result = await listMonths();
		if (result.ok) months = result.data;
		else error = result.error;
	}

	async function findPhotosFolder() {
		const roots = await listFolder(null);
		if (roots.ok) photosFolder = roots.data.find((root) => root.name === 'Photos') ?? null;
	}

	function chosen(event: Event) {
		const input = event.currentTarget as HTMLInputElement;
		const files = Array.from(input.files ?? []);
		input.value = '';
		if (photosFolder && files.length > 0) enqueue(files, photosFolder.id);
	}

	onMount(() => {
		void load();
		void findPhotosFolder();
	});
</script>

<svelte:head><title>Photos · Squirrel</title></svelte:head>

<Page title="Photos">
	{#snippet description()}{#if total !== null}<span class="tabular">{total} {total === 1 ? 'photo' : 'photos'}</span>{/if}{/snippet}
	{#snippet actions()}
		<input bind:this={picker} type="file" accept="image/*" multiple class="visually-hidden" tabindex="-1" aria-hidden="true" onchange={chosen} />
		<Button onclick={() => picker?.click()} disabled={!photosFolder} aria-label="Upload photos to Photos">
			<Icon name="upload" />Upload
		</Button>
	{/snippet}

	<PhotosTabs path={page.url.pathname} />

	{#if error}
		<Alert>{error}</Alert>
	{:else if months === null}
		<p class="muted" aria-live="polite">Loading…</p>
	{:else if months.length === 0}
		<EmptyState icon="images" title="No photos yet">
			Upload photos, or add images anywhere in your drive: they appear here by date.
		</EmptyState>
	{/if}
</Page>
