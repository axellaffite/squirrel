<script lang="ts">
	import { onMount } from 'svelte';
	import Icon from '#lib/components/icons/Icon.svelte';
	import Alert from '#lib/components/ui/Alert.svelte';
	import Button from '#lib/components/ui/Button.svelte';
	import EmptyState from '#lib/components/ui/EmptyState.svelte';
	import { albumPhotos, thumbUrl, zipUrl, type AlbumPhoto } from '#lib/public/api.ts';
	import { fileOfPhoto, MAX_ZIP_FILES, type ViewedFile } from '#lib/public/view.ts';

	interface Props {
		token: string;
		onOpen: (file: ViewedFile) => void;
		onGone: () => void;
		/** How many photos "Download all" would zip (read by the page header). */
		zipCount?: number;
		/** A zip is being prepared. */
		zipping?: boolean;
	}

	let { token, onOpen, onGone, zipCount = $bindable(0), zipping = $bindable(false) }: Props = $props();

	let photos: AlbumPhoto[] = $state([]);
	let nextCursor: string | null = $state(null);
	let loaded = $state(false);
	let loading = $state(false);
	let error: string | null = $state(null);

	async function load(cursor: string | null) {
		loading = true;
		error = null;
		const result = await albumPhotos(token, cursor);
		loading = false;
		if (!result.ok) {
			if (result.gone || result.locked) onGone();
			else error = result.error;
			return;
		}
		photos = cursor ? [...photos, ...result.data.items] : result.data.items;
		nextCursor = result.data.next_cursor ?? null;
		loaded = true;
	}

	$effect(() => {
		zipCount = Math.min(photos.length, MAX_ZIP_FILES);
	});

	onMount(() => {
		void load(null);
	});

	export async function downloadAll() {
		zipping = true;
		error = null;
		const result = await zipUrl(
			token,
			photos.slice(0, MAX_ZIP_FILES).map((photo) => photo.id)
		);
		zipping = false;
		if (result.ok) window.location.assign(result.data);
		else error = result.error;
	}
</script>

{#if error}<Alert>{error}</Alert>{/if}

{#if loaded && photos.length === 0}
	<EmptyState icon="images" title="This album is empty" />
{:else}
	<ul class="grid" aria-label="Photos">
		{#each photos as photo (photo.id)}
			<li>
				<button class="tile" type="button" aria-label={`Open ${photo.name}`} data-testid="public-photo" onclick={() => onOpen(fileOfPhoto(photo))}>
					{#if photo.thumbnail.state === 'ready'}
						<img src={thumbUrl(token, photo.id, photo.thumbnail.version)} alt="" loading="lazy" />
					{:else}
						<Icon name="image" />
					{/if}
				</button>
			</li>
		{/each}
	</ul>
	{#if nextCursor}
		<Button variant="secondary" disabled={loading} onclick={() => load(nextCursor)}>Show more</Button>
	{/if}
{/if}

<style>
	.grid {
		display: grid;
		grid-template-columns: repeat(auto-fill, minmax(var(--photo-row), 1fr));
		gap: var(--space-1);
		margin: 0;
		padding: 0;
		list-style: none;
	}

	.tile {
		display: grid;
		place-items: center;
		width: 100%;
		aspect-ratio: 1;
		padding: 0;
		overflow: hidden;
		border: 0;
		border-radius: var(--radius-tile);
		background: var(--tint);
		color: var(--ink-muted);
		cursor: pointer;
	}

	.tile:focus-visible {
		outline: var(--ring-width) solid var(--accent);
		outline-offset: var(--ring-offset);
	}

	.tile img {
		width: 100%;
		height: 100%;
		object-fit: cover;
		transition: transform var(--dur-fast) var(--ease);
	}

	.tile:hover img {
		transform: scale(1.02);
	}

	@media (prefers-reduced-motion: reduce) {
		.tile img {
			transition: none;
		}
	}
</style>
