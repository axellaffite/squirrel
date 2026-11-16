<script lang="ts">
	import { goto, replaceState } from '$app/navigation';
	import { page } from '$app/state';
	import { onMount, untrack } from 'svelte';
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
	import { AlbumData } from '#lib/photos/album.svelte.ts';
	import { albumNameProblem, photoCount } from '#lib/photos/albums.ts';
	import {
		deleteAlbum,
		renameAlbum,
		photoDetails,
		setAlbumCover,
		type AssetSummary
	} from '#lib/photos/api.ts';
	import { photoInAddress, withPhoto } from '#lib/photos/address.ts';
	import { Selection } from '#lib/photos/selection.svelte.ts';
	import { neighbourAfterRemoval, trashPhotos } from '#lib/photos/trash.ts';

	let { albumId }: { albumId: string } = $props();

	const data = new AlbumData(untrack(() => albumId));
	/** Errors from actions on this page (cover, removal, rename). */
	let error: string | null = $state(null);
	let notice: string | null = $state(null);
	let busy = $state(false);
	let renaming = $state(false);
	let confirmingDelete = $state(false);
	let viewing: AssetSummary | null = $state(null);
	let picking: readonly string[] | null = $state(null);
	let grid: PhotoGrid | undefined = $state();
	const selection = new Selection();

	const album = $derived(data.album);
	const items = $derived(data.items);

	async function reload() {
		await data.reload();
		selection.keepOnly(new Set(data.items.map((item) => item.id)));
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
			const result = await data.remove([...selection.ids]);
			notice = result.message;
			selection.keepOnly(new Set(data.items.map((item) => item.id)));
			if (!result.error) selection.clear();
			return result.error;
		});

	const makeCover = () =>
		run(async () => {
			const [id] = selection.ids;
			const result = await setAlbumCover(albumId, id);
			if (!result.ok) return result.error;
			data.album = result.data;
			notice = 'Cover changed.';
			selection.clear();
			return null;
		});

	async function rename(name: string): Promise<string | null> {
		const result = await renameAlbum(albumId, name);
		if (!result.ok) return result.error;
		data.album = result.data;
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
		data.drop(asset.id);
		if (after) open(after);
		else closeViewer();
		await reload();
		return null;
	}

	function closeViewer() {
		const id = viewing?.id;
		viewing = null;
		replaceState(withPhoto(page.url.href, null), {});
		if (id) void grid?.focusTile(id);
	}

	function keydown(event: KeyboardEvent) {
		if (event.key === 'Escape' && selection.active && !viewing && !picking && !confirmingDelete) selection.clear();
	}

	/** Shows a photo and keeps `?photo=<id>` in the address, as on the timeline. */
	function open(asset: AssetSummary) {
		viewing = asset;
		replaceState(withPhoto(page.url.href, asset.id), {});
	}

	async function openFromAddress() {
		const id = photoInAddress(page.url);
		if (!id) return;
		const result = await photoDetails(id);
		// The user may have opened another photo meanwhile: theirs wins.
		if (viewing) return;
		if (result.ok) viewing = result.data.asset;
		else replaceState(withPhoto(page.url.href, null), {});
	}

	onMount(() => {
		void reload();
		void openFromAddress();
	});
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
	{#if data.albumError}<Alert>{data.albumError}</Alert>{/if}
	{#if error ?? data.pageError}<Alert>{error ?? data.pageError}</Alert>{/if}

	{#if album && album.item_count === 0 && items.length === 0}
		<EmptyState icon="library" title="This album is empty">Select photos in the timeline, then choose Add to album.</EmptyState>
	{:else if items.length > 0}
		<section aria-label="Photos in the album">
			<PhotoGrid
				bind:this={grid}
				{items}
				selected={selection.ids}
				selecting={selection.active}
				onOpen={open}
				onToggle={toggle}
				onNearEnd={() => void data.nextPage()}
			/>
		</section>
	{:else if !data.albumError && !data.pageError}
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
	<NameDialog title="Rename album" initial={album.name} submitLabel="Save" validate={albumNameProblem} onSubmit={rename} onClose={() => (renaming = false)} />
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
		onNavigate={open}
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
