<script lang="ts">
	import { replaceState } from '$app/navigation';
	import { page } from '$app/state';
	import { onMount, untrack } from 'svelte';
	import SearchField from '#lib/components/files/SearchField.svelte';
	import Icon from '#lib/components/icons/Icon.svelte';
	import AlbumPicker from '#lib/components/photos/AlbumPicker.svelte';
	import FilterBar from '#lib/components/photos/FilterBar.svelte';
	import Lightbox from '#lib/components/photos/Lightbox.svelte';
	import PhotoGrid from '#lib/components/photos/PhotoGrid.svelte';
	import Scrubber from '#lib/components/photos/Scrubber.svelte';
	import SearchTimeline from '#lib/components/photos/SearchTimeline.svelte';
	import type { Geometry } from '#lib/components/photos/Timeline.svelte';
	import Page from '#lib/components/shell/Page.svelte';
	import Alert from '#lib/components/ui/Alert.svelte';
	import Button from '#lib/components/ui/Button.svelte';
	import EmptyState from '#lib/components/ui/EmptyState.svelte';
	import SegmentedControl from '#lib/components/ui/SegmentedControl.svelte';
	import { listPeople, warmUpSearch, type Person, type SearchOrder } from '#lib/explore/api.ts';
	import { filterFromParams, withFilter, type PhotoFilter } from '#lib/explore/filters.ts';
	import { orderFromParams, PhotoSearcher, withOrder } from '#lib/explore/searcher.svelte.ts';
	import { normalizeQuery } from '#lib/files/search.ts';
	import type { AssetSummary } from '#lib/photos/api.ts';
	import { newestFirst } from '#lib/photos/searchTimeline.ts';
	import { neighbourAfterRemoval, trashPhotos } from '#lib/photos/trash.ts';

	const ORDERS: readonly { value: SearchOrder; label: string }[] = [
		{ value: 'date', label: 'Timeline' },
		{ value: 'relevance', label: 'Best match' }
	];

	const searcher = new PhotoSearcher();
	let value = $state(untrack(() => page.url.searchParams.get('q') ?? ''));
	let people: Person[] = $state([]);
	let viewing: AssetSummary | null = $state(null);
	let picking: readonly string[] | null = $state(null);
	let notice: string | null = $state(null);
	let grid: PhotoGrid | undefined = $state();
	let timeline: SearchTimeline | undefined = $state();
	let geometry: Geometry | undefined = $state();
	const noSelection: ReadonlySet<string> = new Set();

	const typed = $derived(normalizeQuery(value));
	const settled = $derived(searcher.query !== '' && searcher.query === typed && !searcher.loading);
	/** The view follows the order the results came in, not one still loading. */
	const asTimeline = $derived(searcher.itemsOrder === 'date');
	/** The results in the order shown, which is also the lightbox's. */
	const shown = $derived(asTimeline ? newestFirst(searcher.items) : searcher.items);

	/** Keeps `?q=`, the filter and the order in the address, so reloading or going back shows the same search. */
	function remember(query: string, filter: PhotoFilter, order: SearchOrder) {
		const url = withOrder(withFilter(page.url.href, filter), order);
		if (query) url.searchParams.set('q', query);
		else url.searchParams.delete('q');
		replaceState(url, {});
	}

	function typing(next: string) {
		searcher.type(next);
		remember(normalizeQuery(next), searcher.filter, searcher.order);
	}

	function filtered(filter: PhotoFilter) {
		searcher.setFilter(filter);
		remember(typed, searcher.filter, searcher.order);
	}

	function ordered(order: SearchOrder) {
		searcher.setOrder(order);
		remember(typed, searcher.filter, searcher.order);
	}

	function closeViewer() {
		const id = viewing?.id;
		viewing = null;
		if (id) void (asTimeline ? timeline?.focusTile(id) : grid?.focusTile(id));
	}

	async function deleteViewed(asset: AssetSummary): Promise<string | null> {
		const after = neighbourAfterRemoval(shown, asset.id);
		const result = await trashPhotos([asset.id]);
		if (result.error) return result.error;
		searcher.items = searcher.items.filter((item) => !result.moved.includes(item.id));
		viewing = after;
		return null;
	}

	onMount(() => {
		void warmUpSearch();
		void listPeople().then((result) => {
			if (result.ok) people = result.data;
		});
		searcher.setOrder(orderFromParams(page.url.searchParams));
		searcher.setFilter(filterFromParams(page.url.searchParams));
		searcher.now(value);
		return () => searcher.dispose();
	});

	// The address is the search's source of truth: following a link to /search (the rail's
	// Search item) or to another query shows that search. Typing keeps it in step (`remember`).
	$effect(() => {
		const q = page.url.searchParams.get('q') ?? '';
		const filter = filterFromParams(page.url.searchParams);
		const order = orderFromParams(page.url.searchParams);
		untrack(() => {
			searcher.setOrder(order);
			searcher.setFilter(filter);
			if (normalizeQuery(q) === typed) return;
			value = q;
			viewing = null;
			if (normalizeQuery(q)) searcher.now(q);
			else searcher.type('');
		});
	});
</script>

<svelte:head><title>Search photos · Squirrel</title></svelte:head>

<Page title="Search">
	{#snippet description()}Describe what is in a photo, in any language: “dog on the beach”, “gâteau d’anniversaire”, “snowy mountains”.{/snippet}
	{#snippet actions()}
		<Button variant="secondary" href="/files/search"><Icon name="file-text" />Search files</Button>
	{/snippet}

	<div class="field">
		<SearchField bind:value oninput={typing} onsubmit={(query) => searcher.now(query)} label="Search photos" placeholder="Search your photos" wide autofocus />
		<FilterBar filter={searcher.filter} {people} onChange={filtered} />
		{#if !searcher.unavailable}
			<SegmentedControl label="Show results" name="search-order" options={ORDERS} value={searcher.order} onchange={ordered} />
		{/if}
	</div>

	{#if searcher.unavailable}
		<Alert tone="info">Photo search isn't set up on this server. Ask your administrator to install the search models.</Alert>
	{:else if searcher.error}
		<Alert>{searcher.error}</Alert>
	{/if}
	<!-- Always rendered and never display: contents, so screen readers reliably hear the count. -->
	<p class="visually-hidden" aria-live="polite">
		{#if settled}{searcher.items.length}{searcher.hasMore ? ' or more' : ''} {searcher.items.length === 1 ? 'photo' : 'photos'}{/if}
	</p>
	<p class="visually-hidden" aria-live="polite">{notice ?? ''}</p>
	{#if notice}<Alert tone="success" role="note">{notice}</Alert>{/if}

	{#if searcher.items.length > 0 && asTimeline}
		<section class="results timeline" aria-label="Search results" aria-busy={searcher.loading}>
			<SearchTimeline
				bind:this={timeline}
				bind:geometry
				items={shown}
				selected={noSelection}
				selecting={false}
				onOpen={(asset) => (viewing = asset)}
				onToggle={(asset) => (viewing = asset)}
				onNearEnd={() => searcher.more()}
			/>
			{#if geometry}
				<Scrubber boxes={geometry.boxes} total={geometry.total} current={geometry.current} onJump={(index) => timeline?.jumpTo(index)} />
			{/if}
		</section>
	{:else if searcher.items.length > 0}
		<section class="results" aria-label="Search results" aria-busy={searcher.loading}>
			<PhotoGrid
				bind:this={grid}
				items={shown}
				days={false}
				selected={noSelection}
				selecting={false}
				onOpen={(asset) => (viewing = asset)}
				onToggle={(asset) => (viewing = asset)}
				onNearEnd={() => searcher.more()}
			/>
		</section>
	{:else if settled && !searcher.error}
		<EmptyState icon="search" title={`No photos match “${searcher.query}”`}>
			Try other words, or fewer filters. Photos are searched by what they show.
		</EmptyState>
	{:else if searcher.loading}
		<p class="muted">Searching…</p>
	{:else if typed === '' && !searcher.unavailable}
		<EmptyState icon="sparkles" title="Search by what is in your photos">
			Type a few words. Results show by date, or best match first.
		</EmptyState>
	{/if}
</Page>

{#if viewing}
	<Lightbox
		asset={viewing}
		items={shown}
		onNavigate={(asset) => (viewing = asset)}
		onClose={closeViewer}
		onDelete={deleteViewed}
		onAddToAlbum={(asset) => (picking = [asset.id])}
	/>
{/if}

{#if picking}
	<AlbumPicker
		ids={picking}
		onDone={(message) => (notice = message)}
		onClose={() => (picking = null)}
	/>
{/if}

<style>
	.field {
		display: grid;
		gap: var(--space-3);
		max-width: var(--measure);
	}

	.results {
		display: grid;
		min-width: 0;
	}

	@media (min-width: 48rem) {
		.timeline {
			grid-template-columns: minmax(0, 1fr) var(--scrubber-w);
			gap: var(--space-4);
			align-items: start;
		}
	}
</style>
