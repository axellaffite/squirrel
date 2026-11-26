<script lang="ts">
	import CoverCard from '#lib/components/photos/CoverCard.svelte';
	import Locator from '#lib/components/photos/Locator.svelte';
	import type { PlaceGroup } from '#lib/explore/api.ts';
	import { countryName, photosHref, placeFilter } from '#lib/explore/filters.ts';
	import { photoCount } from '#lib/photos/albums.ts';

	interface Props {
		place: PlaceGroup;
	}

	let { place }: Props = $props();

	const country = $derived(place.country ?? countryName(place.country_code));
	const name = $derived(place.city ?? country);
	const href = $derived(photosHref(placeFilter(place)));
</script>

<CoverCard {href} {name} count={photoCount(place.count)} cover={place.cover} icon="map-pin">
	{#snippet extra()}
		{#if place.location}
			<span class="locator"><Locator latitude={place.location.latitude} longitude={place.location.longitude} place={place.city ? `${place.city}, ${country}` : country} /></span>
		{/if}
	{/snippet}
</CoverCard>

<style>
	/* The map sits a step below the count, as it did before the owner line joined the text block. */
	.locator {
		display: block;
		margin-top: var(--space-2);
	}
</style>
