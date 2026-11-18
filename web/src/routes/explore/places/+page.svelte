<script lang="ts">
	import { page } from '$app/state';
	import { onMount } from 'svelte';
	import PhotosTabs from '#lib/components/photos/PhotosTabs.svelte';
	import PlaceCard from '#lib/components/photos/PlaceCard.svelte';
	import Page from '#lib/components/shell/Page.svelte';
	import Alert from '#lib/components/ui/Alert.svelte';
	import EmptyState from '#lib/components/ui/EmptyState.svelte';
	import { listPlaces, placesByCountry, type PlaceGroup } from '#lib/explore/api.ts';
	import { countryName } from '#lib/explore/filters.ts';

	let places: PlaceGroup[] | null = $state(null);
	let error: string | null = $state(null);

	const countries = $derived(places ? placesByCountry(places) : []);

	onMount(async () => {
		const result = await listPlaces('city');
		if (result.ok) places = result.data;
		else error = result.error;
	});
</script>

<svelte:head><title>Places · Squirrel</title></svelte:head>

<Page title="Places">
	{#snippet above()}<a class="back" href="/explore">Explore</a>{/snippet}
	{#snippet description()}{#if places}<span class="tabular">{places.length} {places.length === 1 ? 'place' : 'places'}</span>{/if}{/snippet}

	<PhotosTabs path={page.url.pathname} />
	{#if error}<Alert>{error}</Alert>{/if}
	{#if places === null}
		{#if !error}<p class="muted" aria-live="polite">Loading…</p>{/if}
	{:else if places.length === 0}
		<EmptyState icon="map-pin" title="No places yet">
			Photos taken with location turned on are placed here, named from an offline map.
		</EmptyState>
	{:else}
		{#each countries as country (country.code)}
			<section class="country" aria-labelledby={`country-${country.code}`}>
				<h2 id={`country-${country.code}`}>{country.name ?? countryName(country.code)}</h2>
				<ul class="cards">
					{#each country.places as place (`${place.region}/${place.city}`)}
						<li><PlaceCard {place} /></li>
					{/each}
				</ul>
			</section>
		{/each}
	{/if}
</Page>

<style>
	.back {
		border-radius: var(--radius-sm);
		color: var(--ink-muted);
		font-size: var(--text-sm);
		line-height: var(--text-sm-lh);
		text-underline-offset: var(--underline-offset);
	}

	.back:focus-visible {
		outline: var(--ring-width) solid var(--accent);
		outline-offset: var(--ring-offset);
	}

	.country {
		display: grid;
		gap: var(--space-3);
	}

	h2 {
		font-size: var(--text-xl);
		line-height: var(--text-xl-lh);
		font-weight: var(--weight-heading);
		letter-spacing: var(--track-heading);
	}

	.cards {
		display: grid;
		grid-template-columns: repeat(2, minmax(0, 1fr));
		gap: var(--space-3);
		margin: 0;
		padding: 0;
		list-style: none;
	}

	@media (min-width: 48rem) {
		.cards {
			grid-template-columns: repeat(auto-fill, minmax(var(--album-min), 1fr));
			gap: var(--space-4);
		}
	}
</style>
