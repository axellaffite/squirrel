<script lang="ts">
	import { onMount } from 'svelte';
	import NameDialog from '#lib/components/files/NameDialog.svelte';
	import Icon from '#lib/components/icons/Icon.svelte';
	import AlbumCover from '#lib/components/photos/AlbumCover.svelte';
	import Alert from '#lib/components/ui/Alert.svelte';
	import Dialog from '#lib/components/ui/Dialog.svelte';
	import { addedMessage, batches, photoCount, sortByName } from '#lib/photos/albums.ts';
	import { addToAlbum, createAlbum, listAlbums, type Album } from '#lib/photos/api.ts';

	interface Props {
		/** The photos to add. */
		ids: readonly string[];
		/** Called with what happened ("Added 3 photos to Holidays."), once the photos are in. */
		onDone: (message: string) => void;
		/** Runs once the picker closed, however it closed. */
		onClose: () => void;
	}

	let { ids, onDone, onClose }: Props = $props();

	let dialog: Dialog | undefined = $state();
	let open = $state(true);
	let albums: Album[] | null = $state(null);
	let error: string | null = $state(null);
	let busy = $state(false);
	let naming = $state(false);

	onMount(async () => {
		const result = await listAlbums();
		if (result.ok) albums = sortByName(result.data);
		else error = result.error;
	});

	async function add(album: Album) {
		busy = true;
		error = null;
		let changed = 0;
		for (const batch of batches(ids)) {
			const result = await addToAlbum(album.id, batch);
			if (!result.ok) {
				error = result.error;
				busy = false;
				return;
			}
			changed += result.data.changed;
		}
		busy = false;
		onDone(addedMessage(changed, ids.length, album.name));
		dialog?.close();
	}

	/** "New album…": the picker closes and the name dialog takes over. */
	function startNaming() {
		naming = true;
		open = false;
	}

	function pickerClosed() {
		if (!naming) onClose();
	}

	function namingClosed() {
		naming = false;
		onClose();
	}

	/** A new album created with the first batch, then the rest added to it. */
	async function create(name: string): Promise<string | null> {
		const [first = [], ...rest] = batches(ids);
		const created = await createAlbum(name, first);
		if (!created.ok) return created.error;
		const album = created.data;
		for (const batch of rest) {
			const result = await addToAlbum(album.id, batch);
			if (!result.ok) return result.error;
		}
		onDone(`Added ${photoCount(ids.length)} to ${album.name}.`);
		return null;
	}
</script>

{#if naming}
	<NameDialog title="New album" submitLabel="Create album" onSubmit={create} onClose={namingClosed} />
{/if}

<Dialog bind:this={dialog} bind:open title="Add to album" onclose={pickerClosed}>
	<div class="body">
		{#if error}<Alert>{error}</Alert>{/if}
		<ul class="albums" aria-busy={busy}>
			<li>
				<button type="button" class="album" disabled={busy} onclick={startNaming}>
					<span class="well"><Icon name="plus" /></span>
					<span class="text"><span class="name">New album…</span></span>
				</button>
			</li>
			{#each albums ?? [] as album (album.id)}
				<li>
					<button type="button" class="album" disabled={busy} onclick={() => add(album)}>
						<span class="well"><AlbumCover cover={album.cover ?? null} size="thumb" /></span>
						<span class="text">
							<span class="name">{album.name}</span>
							<span class="count tabular">{photoCount(album.item_count)}</span>
						</span>
					</button>
				</li>
			{/each}
		</ul>
		{#if albums === null && !error}<p class="muted" aria-live="polite">Loading…</p>{/if}
	</div>
</Dialog>

<style>
	.body {
		display: grid;
		gap: var(--space-3);
	}

	.albums {
		display: grid;
		gap: var(--space-1);
		max-height: var(--center-min);
		margin: 0;
		padding: var(--space-2);
		overflow-y: auto;
		list-style: none;
		border-radius: var(--radius-well);
		background: var(--tint);
	}

	.album {
		display: flex;
		align-items: center;
		gap: var(--space-3);
		width: 100%;
		min-height: var(--row-min);
		padding: var(--space-2);
		border: 0;
		border-radius: var(--radius-card);
		background: transparent;
		color: var(--ink);
		text-align: left;
		cursor: pointer;
		transition:
			background-color var(--dur-fast) var(--ease),
			box-shadow var(--dur) var(--ease),
			transform var(--dur-fast) var(--ease);
	}

	.well {
		display: grid;
		place-items: center;
		width: var(--well-md);
		height: var(--well-md);
		flex: none;
		overflow: hidden;
		border-radius: var(--radius-full);
		background: var(--card);
		box-shadow: var(--shadow-card);
	}

	.text {
		display: grid;
		min-width: 0;
	}

	.name {
		overflow: hidden;
		text-overflow: ellipsis;
		white-space: nowrap;
		font-weight: var(--weight-label);
	}

	.count {
		color: var(--ink-muted);
		font-size: var(--text-sm);
		line-height: var(--text-sm-lh);
	}

	@media (hover: hover) {
		.album:not(:disabled):hover {
			background: var(--card);
			box-shadow: var(--shadow-card);
		}
	}

	.album:focus-visible {
		outline: var(--ring-width) solid var(--accent);
		outline-offset: calc(-1 * var(--ring-width));
	}

	.album:not(:disabled):active {
		transform: translateY(var(--press));
	}

	.album:disabled {
		opacity: var(--disabled-opacity);
		cursor: not-allowed;
	}
</style>
