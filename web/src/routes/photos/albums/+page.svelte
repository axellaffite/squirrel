<script lang="ts">
	import { goto } from '$app/navigation';
	import { page } from '$app/state';
	import { onMount } from 'svelte';
	import NameDialog from '#lib/components/files/NameDialog.svelte';
	import Icon from '#lib/components/icons/Icon.svelte';
	import AlbumCard from '#lib/components/photos/AlbumCard.svelte';
	import PhotosTabs from '#lib/components/photos/PhotosTabs.svelte';
	import Page from '#lib/components/shell/Page.svelte';
	import Alert from '#lib/components/ui/Alert.svelte';
	import Button from '#lib/components/ui/Button.svelte';
	import EmptyState from '#lib/components/ui/EmptyState.svelte';
	import { createAlbum, listAlbums, type Album } from '#lib/photos/api.ts';

	let albums = $state<Album[] | null>(null);
	let error: string | null = $state(null);
	let naming = $state(false);

	async function load() {
		const result = await listAlbums();
		if (result.ok) {
			albums = result.data;
			error = null;
		} else error = result.error;
	}

	async function create(name: string): Promise<string | null> {
		const result = await createAlbum(name);
		if (!result.ok) return result.error;
		void goto(`/photos/albums/${result.data.id}`);
		return null;
	}

	onMount(load);
</script>

<svelte:head><title>Albums · Squirrel</title></svelte:head>

<Page title="Photos">
	{#snippet description()}{#if albums}<span class="tabular">{albums.length} {albums.length === 1 ? 'album' : 'albums'}</span>{/if}{/snippet}
	{#snippet actions()}
		<Button onclick={() => (naming = true)}><Icon name="plus" />New album</Button>
	{/snippet}

	<PhotosTabs path={page.url.pathname} />

	{#if error}<Alert>{error}</Alert>{/if}
	{#if albums === null}
		{#if !error}<p class="muted" aria-live="polite">Loading…</p>{/if}
	{:else if albums.length === 0}
		<EmptyState icon="library" title="No albums yet">Select photos in the timeline, then choose Add to album.</EmptyState>
	{:else}
		<ul class="albums" aria-label="Albums">
			{#each albums as album (album.id)}
				<li><AlbumCard {album} /></li>
			{/each}
		</ul>
	{/if}
</Page>

{#if naming}
	<NameDialog title="New album" submitLabel="Create album" onSubmit={create} onClose={() => (naming = false)} />
{/if}

<style>
	.albums {
		display: grid;
		grid-template-columns: repeat(2, minmax(0, 1fr));
		gap: var(--space-3);
		margin: 0;
		padding: 0;
		list-style: none;
	}

	@media (min-width: 48rem) {
		.albums {
			grid-template-columns: repeat(auto-fill, minmax(var(--album-min), 1fr));
			gap: var(--space-4);
		}
	}
</style>
