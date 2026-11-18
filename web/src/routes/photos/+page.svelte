<script lang="ts">
	import { onMount, untrack } from 'svelte';
	import { replaceState } from '$app/navigation';
	import { page } from '$app/state';
	import ConfirmDialog from '#lib/components/ConfirmDialog.svelte';
	import Icon from '#lib/components/icons/Icon.svelte';
	import AlbumPicker from '#lib/components/photos/AlbumPicker.svelte';
	import Lightbox from '#lib/components/photos/Lightbox.svelte';
	import PhotosTabs from '#lib/components/photos/PhotosTabs.svelte';
	import Scrubber from '#lib/components/photos/Scrubber.svelte';
	import SelectionBar from '#lib/components/photos/SelectionBar.svelte';
	import Timeline, { type Geometry } from '#lib/components/photos/Timeline.svelte';
	import Page from '#lib/components/shell/Page.svelte';
	import Alert from '#lib/components/ui/Alert.svelte';
	import Button from '#lib/components/ui/Button.svelte';
	import EmptyState from '#lib/components/ui/EmptyState.svelte';
	import FilterBar from '#lib/components/photos/FilterBar.svelte';
	import { listPeople, type Person } from '#lib/explore/api.ts';
	import { filterFromParams, isEmptyFilter, sameFilter, withFilter, type PhotoFilter } from '#lib/explore/filters.ts';
	import { listFolder, type FileNode } from '#lib/files/api.ts';
	import { mediaStatus, photoDetails, type AssetSummary } from '#lib/photos/api.ts';
	import { photoInAddress, withPhoto } from '#lib/photos/address.ts';
	import { Selection } from '#lib/photos/selection.svelte.ts';
	import { createPoller, preparingText } from '#lib/photos/status.ts';
	import { TimelineData } from '#lib/photos/timeline.svelte.ts';
	import { neighbourAfterRemoval, trashPhotos } from '#lib/photos/trash.ts';
	import { enqueue, onUploadsSettled } from '#lib/upload/uploads.svelte.ts';

	const data = new TimelineData();
	let photosFolder = $state<FileNode | null>(null);
	let picker: HTMLInputElement | undefined = $state();
	let preparing = $state(0);
	/** Said once when preparing starts and once when it ends, not at every poll. */
	let announcement = $state('');
	let geometry: Geometry | undefined = $state();
	let timeline: Timeline | undefined = $state();

	const selection = new Selection();
	let viewing: AssetSummary | null = $state(null);
	let confirmingDelete = $state(false);
	/** Photos the album picker is open for. */
	let picking: readonly string[] | null = $state(null);
	let notice: string | null = $state(null);
	let people: Person[] = $state([]);

	const total = $derived(data.total);
	const filtering = $derived(!isEmptyFilter(data.filter));

	// The address is the filter's source of truth: a link to the same page with another filter
	// (the rail's Photos item, a tag in the lightbox) changes what is shown.
	$effect(() => {
		const wanted = filterFromParams(page.url.searchParams);
		untrack(() => {
			if (sameFilter(wanted, data.filter)) return;
			viewing = null;
			selection.clear();
			void data.setFilter(wanted);
		});
	});

	/** Shows only matching photos, and keeps the filter in the address (`?tag=…`). */
	function filterBy(filter: PhotoFilter) {
		selection.clear();
		replaceState(withFilter(page.url.href, filter), {});
		void data.setFilter(filter);
	}

	const poller = createPoller({
		fetch: mediaStatus,
		onCount: (count) => {
			if (preparing === 0 && count > 0) announcement = preparingText(count);
			else if (preparing > 0 && count === 0) announcement = 'Photos and documents are ready.';
			preparing = count;
		},
		onSettled: () => void data.reload(),
		onProgress: () => void data.reload(),
		visible: () => document.visibilityState === 'visible'
	});

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

	/** Keeps `?photo=<id>` in the address, so a reload reopens the same photo. */
	function rememberPhoto(id: string | null) {
		replaceState(withPhoto(page.url.href, id), {});
	}

	function open(asset: AssetSummary) {
		viewing = asset;
		rememberPhoto(asset.id);
	}

	function closeViewer() {
		const id = viewing?.id;
		viewing = null;
		rememberPhoto(null);
		if (id) void timeline?.focusTile(id);
	}

	function toggle(asset: AssetSummary, range: boolean) {
		if (range) selection.selectRange(asset.id, data.ordered.map((item) => item.id));
		else selection.toggle(asset.id);
	}

	async function openFromAddress() {
		const id = photoInAddress(page.url);
		if (!id) return;
		const result = await photoDetails(id);
		// The user may have opened another photo meanwhile: theirs wins.
		if (viewing) return;
		if (result.ok) viewing = result.data.asset;
		else rememberPhoto(null);
	}

	async function deleteViewed(asset: AssetSummary): Promise<string | null> {
		const after = neighbourAfterRemoval(data.ordered, asset.id);
		const result = await trashPhotos([asset.id]);
		if (result.error) return result.error;
		data.remove(result.moved);
		if (after) open(after);
		else closeViewer();
		void data.reload();
		return null;
	}

	async function deleteSelected(): Promise<string | null> {
		const result = await trashPhotos([...selection.ids]);
		const moved = new Set(result.moved);
		data.remove(moved);
		selection.keepOnly(new Set([...selection.ids].filter((id) => !moved.has(id))));
		void data.reload();
		return result.error;
	}

	function added(message: string) {
		notice = message;
		selection.clear();
	}

	/** Escape clears the selection, unless something else (a menu, the upload panel) handles it. */
	function keydown(event: KeyboardEvent) {
		if (event.key !== 'Escape' || event.defaultPrevented || !selection.active) return;
		if (viewing || confirmingDelete || picking) return;
		const target = event.target as Element;
		if (target === document.body || target.closest('.library, [role=toolbar]')) selection.clear();
	}

	onMount(() => {
		const initial = filterFromParams(page.url.searchParams);
		if (isEmptyFilter(initial)) void data.reload();
		else void data.setFilter(initial);
		void listPeople().then((result) => {
			if (result.ok) people = result.data;
		});
		void findPhotosFolder();
		void openFromAddress();
		poller.start();
		const restart = () => poller.start();
		document.addEventListener('visibilitychange', restart);
		const unsubscribe = onUploadsSettled(() => {
			void data.reload();
			poller.start();
		});
		return () => {
			poller.stop();
			unsubscribe();
			document.removeEventListener('visibilitychange', restart);
		};
	});
</script>

<svelte:head><title>Photos · Squirrel</title></svelte:head>
<svelte:window onkeydown={keydown} />

<Page title="Photos">
	{#snippet description()}{#if total !== null}<span class="tabular">{total} {total === 1 ? 'photo' : 'photos'}{filtering ? ' match' : ''}</span>{/if}{/snippet}
	{#snippet actions()}
		<input bind:this={picker} type="file" accept="image/*" multiple class="visually-hidden" tabindex="-1" aria-hidden="true" onchange={chosen} />
		<Button onclick={() => picker?.click()} disabled={!photosFolder} aria-label="Upload photos to Photos">
			<Icon name="upload" />Upload
		</Button>
	{/snippet}

	<PhotosTabs path={page.url.pathname} />
	<FilterBar filter={data.filter} {people} onChange={filterBy} />

	<!-- Always rendered and never display: contents, so screen readers reliably hear the change. -->
	<p class="visually-hidden" aria-live="polite">{notice ?? ''}</p>
	{#if notice}<Alert tone="success" role="note">{notice}</Alert>{/if}
	<p class="visually-hidden" aria-live="polite">{announcement}</p>
	{#if preparing > 0}<Alert tone="info" role="note">{preparingText(preparing)}</Alert>{/if}
	{#if data.error}<Alert>{data.error}</Alert>{/if}

	{#if data.months === null}
		{#if !data.error}<p class="muted" aria-live="polite">Loading…</p>{/if}
	{:else if data.months.length === 0 && filtering}
		<EmptyState icon="sliders-horizontal" title="No photos match these filters">
			Remove a filter above, or clear them all to see every photo.
		</EmptyState>
	{:else if data.months.length === 0}
		<EmptyState icon="images" title="No photos yet">
			Upload photos, or add images anywhere in your drive: they appear here by date.
		</EmptyState>
	{:else}
		<section class="library" aria-label="Photo timeline">
			<Timeline bind:this={timeline} bind:geometry {data} selected={selection.ids} selecting={selection.active} onOpen={open} onToggle={toggle} />
			{#if geometry}
				<Scrubber
					boxes={geometry.boxes}
					total={geometry.total}
					current={geometry.current}
					onJump={(index) => timeline?.jumpTo(index)}
				/>
			{/if}
		</section>
	{/if}
</Page>

{#if selection.active}
	<SelectionBar count={selection.size} onClear={() => selection.clear()}>
		<Button variant="secondary" size="sm" onclick={() => (picking = [...selection.ids])}>
			<Icon name="library" /><span class="label">Add to album</span>
		</Button>
		<Button variant="danger" size="sm" onclick={() => (confirmingDelete = true)}>
			<Icon name="trash" /><span class="label">Delete</span>
		</Button>
	</SelectionBar>
{/if}

{#if confirmingDelete}
	<ConfirmDialog
		title={`Move ${selection.size} ${selection.size === 1 ? 'photo' : 'photos'} to the trash?`}
		confirmLabel="Move to trash"
		onConfirm={deleteSelected}
		onClose={() => (confirmingDelete = false)}
	>
		They stay in the trash for 30 days before they are deleted for good.
	</ConfirmDialog>
{/if}

{#if viewing}
	<Lightbox
		asset={viewing}
		items={data.ordered}
		onNavigate={open}
		onClose={closeViewer}
		onDelete={deleteViewed}
		onAddToAlbum={(asset) => (picking = [asset.id])}
	/>
{/if}

{#if picking}
	<AlbumPicker ids={picking} onDone={added} onClose={() => (picking = null)} />
{/if}

<style>
	.library {
		display: grid;
		min-width: 0;
	}

	@media (min-width: 48rem) {
		.library {
			grid-template-columns: minmax(0, 1fr) var(--scrubber-w);
			gap: var(--space-4);
			align-items: start;
		}
	}
</style>
