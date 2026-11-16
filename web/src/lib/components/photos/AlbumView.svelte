<script lang="ts">
	import { goto } from '$app/navigation';
	import { onMount } from 'svelte';
	import ConfirmDialog from '#lib/components/ConfirmDialog.svelte';
	import Breadcrumbs from '#lib/components/files/Breadcrumbs.svelte';
	import NameDialog from '#lib/components/files/NameDialog.svelte';
	import Icon from '#lib/components/icons/Icon.svelte';
	import AlbumActions from '#lib/components/photos/AlbumActions.svelte';
	import AlbumPicker from '#lib/components/photos/AlbumPicker.svelte';
	import Lightbox from '#lib/components/photos/Lightbox.svelte';
	import PhotoGrid from '#lib/components/photos/PhotoGrid.svelte';
	import SelectionBar from '#lib/components/photos/SelectionBar.svelte';
	import Page from '#lib/components/shell/Page.svelte';
	import Alert from '#lib/components/ui/Alert.svelte';
	import Button from '#lib/components/ui/Button.svelte';
	import EmptyState from '#lib/components/ui/EmptyState.svelte';
	import { batches, photoCount } from '#lib/photos/albums.ts';
	import {
		deleteAlbum,
		getAlbum,
		listAlbumPage,
		removeFromAlbum,
		renameAlbum,
		setAlbumCover,
		type Album,
		type AssetSummary
	} from '#lib/photos/api.ts';
	import { Selection } from '#lib/photos/selection.svelte.ts';
	import { neighbourAfterRemoval, trashPhotos } from '#lib/photos/trash.ts';

	let { albumId }: { albumId: string } = $props();

	let album = $state<Album | null>(null);
	let items: AssetSummary[] = $state.raw([]);
	let cursor: string | null = null;
	let finished = $state(false);
	let loadingPage = false;
	let error: string | null = $state(null);
	let notice: string | null = $state(null);
	let busy = $state(false);
	let renaming = $state(false);
	let confirmingDelete = $state(false);
	let viewing: AssetSummary | null = $state(null);
	let picking: readonly string[] | null = $state(null);
	let grid: PhotoGrid | undefined = $state();
	const selection = new Selection();

	async function loadAlbum() {
		const result = await getAlbum(albumId);
		if (result.ok) album = result.data;
		else error = result.status === 404 ? 'This album does not exist or was deleted.' : result.error;
	}

	async function nextPage() {
		if (loadingPage || finished) return;
		loadingPage = true;
		const result = await listAlbumPage(albumId, cursor);
		loadingPage = false;
		if (!result.ok) {
			error = result.error;
			finished = true;
			return;
		}
		items = [...items, ...result.data.items];
		cursor = result.data.next_cursor ?? null;
		finished = cursor === null;
	}

	/** After a change: the album (count, cover) and its photos from the start. */
	async function reload() {
		items = [];
		cursor = null;
		finished = false;
		await Promise.all([loadAlbum(), nextPage()]);
		selection.keepOnly(new Set(items.map((item) => item.id)));
	}

	function toggle(asset: AssetSummary, range: boolean) {
		if (range) selection.selectRange(asset.id, items.map((item) => item.id));
		else selection.toggle(asset.id);
	}

	async function run(work: () => Promise<string | null>) {
		busy = true;
		notice = null;
		error = await work();
		busy = false;
	}

	const removeSelected = () =>
		run(async () => {
			let removed = 0;
			for (const batch of batches([...selection.ids])) {
				const result = await removeFromAlbum(albumId, batch);
				if (!result.ok) return result.error;
				removed += result.data.changed;
			}
			notice = `Removed ${photoCount(removed)} from the album.`;
			selection.clear();
			await reload();
			return null;
		});

	const makeCover = () =>
		run(async () => {
			const [id] = selection.ids;
			const result = await setAlbumCover(albumId, id);
			if (!result.ok) return result.error;
			album = result.data;
			notice = 'Cover changed.';
			selection.clear();
			return null;
		});

	async function rename(name: string): Promise<string | null> {
		const result = await renameAlbum(albumId, name);
		if (!result.ok) return result.error;
		album = result.data;
		return null;
	}

	let deleted = false;

	async function remove(): Promise<string | null> {
		const result = await deleteAlbum(albumId);
		if (!result.ok) return result.error;
		deleted = true;
		return null;
	}

	/** The album is gone once its confirmation closed: back to the albums. */
	function confirmClosed() {
		confirmingDelete = false;
		if (deleted) void goto('/photos/albums');
	}

	async function trashViewed(asset: AssetSummary): Promise<string | null> {
		const after = neighbourAfterRemoval(items, asset.id);
		const result = await trashPhotos([asset.id]);
		if (result.error) return result.error;
		items = items.filter((item) => item.id !== asset.id);
		if (after) viewing = after;
		else closeViewer();
		await reload();
		return null;
	}

	function closeViewer() {
		const id = viewing?.id;
		viewing = null;
		if (id) void grid?.focusTile(id);
	}

	function keydown(event: KeyboardEvent) {
		if (event.key === 'Escape' && selection.active && !viewing && !picking && !confirmingDelete) selection.clear();
	}

	onMount(() => void reload());
</script>

<svelte:head><title>{album?.name ?? 'Album'} · Squirrel</title></svelte:head>
<svelte:window onkeydown={keydown} />

<Page title={album?.name ?? 'Album'}>
	{#snippet above()}<Breadcrumbs path={[]} current={album?.name ?? null} rootLabel="Albums" rootHref="/photos/albums" />{/snippet}
	{#snippet description()}{#if album}<span class="tabular">{photoCount(album.item_count)}</span>{/if}{/snippet}
	{#snippet actions()}
		{#if album}
			<AlbumActions name={album.name} onRename={() => (renaming = true)} onDelete={() => (confirmingDelete = true)} />
		{/if}
	{/snippet}

	<div class="live" aria-live="polite">{#if notice}<Alert tone="success" role="status">{notice}</Alert>{/if}</div>
	{#if error}<Alert>{error}</Alert>{/if}

	{#if album && album.item_count === 0 && items.length === 0}
		<EmptyState icon="library" title="This album is empty">Select photos in the timeline, then choose Add to album.</EmptyState>
	{:else if items.length > 0}
		<section aria-label="Photos in the album">
			<PhotoGrid
				bind:this={grid}
				{items}
				selected={selection.ids}
				selecting={selection.active}
				onOpen={(asset) => (viewing = asset)}
				onToggle={toggle}
				onNearEnd={nextPage}
			/>
		</section>
	{:else if !error}
		<p class="muted" aria-live="polite">Loading…</p>
	{/if}
</Page>

{#if selection.active}
	<SelectionBar count={selection.size} onClear={() => selection.clear()}>
		{#if selection.size === 1}
			<Button variant="secondary" size="sm" aria-label="Use the selected photo as the album cover" disabled={busy} onclick={makeCover}>
				<Icon name="images" /><span class="label">Set as cover</span>
			</Button>
		{/if}
		<Button variant="danger" size="sm" aria-label="Remove the selected photos from the album" disabled={busy} onclick={removeSelected}>
			<Icon name="x" /><span class="label">Remove from album</span>
		</Button>
	</SelectionBar>
{/if}

{#if renaming && album}
	<NameDialog title="Rename album" initial={album.name} submitLabel="Save" onSubmit={rename} onClose={() => (renaming = false)} />
{/if}

{#if confirmingDelete}
	<ConfirmDialog title="Delete the album?" confirmLabel="Delete album" onConfirm={remove} onClose={confirmClosed}>
		The photos stay in your timeline.
	</ConfirmDialog>
{/if}

{#if viewing}
	<Lightbox
		asset={viewing}
		{items}
		onNavigate={(asset) => (viewing = asset)}
		onClose={closeViewer}
		onDelete={trashViewed}
		onAddToAlbum={(asset) => (picking = [asset.id])}
	/>
{/if}

{#if picking}
	<AlbumPicker ids={picking} onDone={(message) => (notice = message)} onClose={() => (picking = null)} />
{/if}

<style>
	.live {
		display: contents;
	}
</style>
