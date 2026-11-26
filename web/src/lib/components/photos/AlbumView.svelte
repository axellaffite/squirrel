<script lang="ts">
	import { goto, replaceState } from '$app/navigation';
	import { page } from '$app/state';
	import { onMount, tick, untrack } from 'svelte';
	import ConfirmDialog from '#lib/components/ConfirmDialog.svelte';
	import Breadcrumbs from '#lib/components/files/Breadcrumbs.svelte';
	import NameDialog from '#lib/components/files/NameDialog.svelte';
	import Icon from '#lib/components/icons/Icon.svelte';
	import AlbumActions from '#lib/components/photos/AlbumActions.svelte';
	import AlbumPicker from '#lib/components/photos/AlbumPicker.svelte';
	import AlbumRulesDialog from '#lib/components/photos/AlbumRulesDialog.svelte';
	import RulesSummary from '#lib/components/photos/RulesSummary.svelte';
	import Lightbox from '#lib/components/photos/Lightbox.svelte';
	import PhotoGrid from '#lib/components/photos/PhotoGrid.svelte';
	import SelectionBar from '#lib/components/photos/SelectionBar.svelte';
	import Page from '#lib/components/shell/Page.svelte';
	import CommentsDialog from '#lib/components/sharing/CommentsDialog.svelte';
	import ShareDialog from '#lib/components/sharing/ShareDialog.svelte';
	import IconButton from '#lib/components/ui/IconButton.svelte';
	import Alert from '#lib/components/ui/Alert.svelte';
	import Badge from '#lib/components/ui/Badge.svelte';
	import Button from '#lib/components/ui/Button.svelte';
	import EmptyState from '#lib/components/ui/EmptyState.svelte';
	import { AlbumData } from '#lib/photos/album.svelte.ts';
	import { albumNameProblem, photoCount } from '#lib/photos/albums.ts';
	import {
		deleteAlbum,
		renameAlbum,
		photoDetails,
		setAlbumCover,
		updateAlbumRules,
		type AlbumRules,
		type AssetSummary
	} from '#lib/photos/api.ts';
	import { photoInAddress, withPhoto } from '#lib/photos/address.ts';
	import { Selection } from '#lib/photos/selection.svelte.ts';
	import { openRequest, withoutOpenRequest } from '#lib/notifications/logic.ts';
	import { abilities, memberName } from '#lib/sharing/logic.ts';
	import { neighbourAfterRemoval, trashPhotos } from '#lib/photos/trash.ts';

	let { albumId }: { albumId: string } = $props();

	const data = new AlbumData(untrack(() => albumId));
	/** Errors from actions on this page (cover, removal, rename). */
	let error: string | null = $state(null);
	let notice: string | null = $state(null);
	let busy = $state(false);
	let renaming = $state(false);
	let editingRules = $state(false);
	let confirmingDelete = $state(false);
	let viewing: AssetSummary | null = $state(null);
	let picking: readonly string[] | null = $state(null);
	let sharing = $state(false);
	let commenting = $state(false);
	let grid: PhotoGrid | undefined = $state();
	const selection = new Selection();

	const album = $derived(data.album);
	const items = $derived(data.items);
	/** Others' albums: writers add and remove their own photos; the rest stays with the owner. */
	const can = $derived(abilities(album?.access ?? 'read'));
	/** A dynamic album's photos come from its rules: none are added or removed by hand. */
	const dynamic = $derived(album?.dynamic ?? false);
	const canRemove = $derived(can.edit && !dynamic);

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

	async function saveRules(_name: string, rules: AlbumRules): Promise<string | null> {
		const result = await updateAlbumRules(albumId, rules);
		if (!result.ok) return result.error;
		selection.clear();
		await reload();
		notice = 'Rules saved.';
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

	/** Escape clears the selection, unless something else (a dialog, a menu) handles it. */
	function keydown(event: KeyboardEvent) {
		if (event.key !== 'Escape' || event.defaultPrevented || !selection.active) return;
		if (viewing || picking || confirmingDelete || renaming || editingRules || sharing || commenting) return;
		const target = event.target as Element;
		if (target === document.body || target.closest('.album-grid, [role=toolbar]')) selection.clear();
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

	/** `?comments=1` (from a notification) opens the thread once, then leaves the address clean. */
	async function commentsFromAddress() {
		await tick();
		if (!openRequest(page.url)?.comments) return;
		replaceState(withoutOpenRequest(page.url), {});
		commenting = true;
	}

	$effect(() => {
		void page.url.search;
		untrack(() => void commentsFromAddress());
	});

	onMount(() => {
		void reload();
		void openFromAddress();
	});
</script>

<svelte:head><title>{album?.name ?? 'Album'} · Squirrel</title></svelte:head>
<svelte:window onkeydown={keydown} />

<Page title={album?.name ?? 'Album'}>
	{#snippet above()}
		{#if album && !can.owner}<Breadcrumbs path={[]} current={album.name} rootLabel="Shared" rootHref="/shared" />
		{:else}<Breadcrumbs path={[]} current={album?.name ?? null} rootLabel="Albums" rootHref="/photos/albums" />{/if}
	{/snippet}
	{#snippet description()}
		{#if album}<span class="tabular">{photoCount(album.item_count)}</span>{/if}
		{#if album?.shared_by}<span aria-hidden="true"> · </span>Shared by {memberName(album.shared_by)}{/if}
		{#if dynamic}{' '}<Badge tone="accent">Dynamic</Badge>{/if}
		{#if album?.rules}<RulesSummary rules={album.rules} />{/if}
	{/snippet}
	{#snippet actions()}
		{#if album}
			<span class="subject-tools">
				{#if can.owner}
					<IconButton icon="share" label={`Share ${album.name}`} tooltip="Share" fill="tint" onclick={() => (sharing = true)} />
				{/if}
				<IconButton icon="message-circle" label={`Comments on ${album.name}`} tooltip="Comments" fill="tint" onclick={() => (commenting = true)} />
			</span>
			{#if can.owner}
				<AlbumActions
					name={album.name}
					onRename={() => (renaming = true)}
					onEditRules={dynamic ? () => (editingRules = true) : null}
					onDelete={() => (confirmingDelete = true)}
				/>
			{/if}
		{/if}
	{/snippet}

	<!-- Always rendered and never display: contents, so screen readers reliably hear the change. -->
	<p class="visually-hidden" aria-live="polite">{notice ?? ''}</p>
	{#if notice}<Alert tone="success" role="note">{notice}</Alert>{/if}
	{#if data.albumError}<Alert>{data.albumError}</Alert>{/if}
	{#if error ?? data.pageError}<Alert>{error ?? data.pageError}</Alert>{/if}

	{#if album && album.item_count === 0 && items.length === 0}
		<EmptyState icon={dynamic ? 'sparkles' : 'library'} title={dynamic ? 'No photos match yet' : 'This album is empty'}>
			{#if dynamic}Photos that match its rules will show up here on their own.
			{:else if can.edit}Select photos in the timeline, then choose Add to album.{:else}Photos added to it will show up here.{/if}
		</EmptyState>
	{:else if items.length > 0}
		<section class="album-grid" aria-label="Photos in the album">
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

{#if selection.active && (canRemove || can.owner)}
	<SelectionBar count={selection.size} onClear={() => selection.clear()}>
		{#if selection.size === 1 && can.owner}
			<Button variant="secondary" size="sm" disabled={busy} onclick={makeCover}>
				<Icon name="images" /><span class="label">Set as cover</span>
			</Button>
		{/if}
		{#if canRemove}
			<Button variant="danger" size="sm" disabled={busy} onclick={removeSelected}>
				<Icon name="x" /><span class="label">Remove from album</span>
			</Button>
		{/if}
	</SelectionBar>
{/if}

{#if renaming && album}
	<NameDialog title="Rename album" initial={album.name} submitLabel="Save" validate={albumNameProblem} onSubmit={rename} onClose={() => (renaming = false)} />
{/if}

{#if editingRules && album}
	<AlbumRulesDialog title="Edit rules" submitLabel="Save rules" initial={album.rules ?? null} onSubmit={saveRules} onClose={() => (editingRules = false)} />
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

{#if sharing && album}
	<ShareDialog subject={{ album_id: album.id }} name={album.name} readOnly={dynamic} onClose={() => (sharing = false)} />
{/if}

{#if commenting && album}
	<CommentsDialog subject={{ album_id: album.id }} name={album.name} onClose={() => (commenting = false)} />
{/if}

{#if picking}
	<AlbumPicker ids={picking} onDone={(message) => (notice = message)} onClose={() => (picking = null)} />
{/if}

<style>
	.subject-tools {
		display: flex;
		align-items: center;
		gap: var(--space-2);
	}
</style>
