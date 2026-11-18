<script lang="ts">
	import { page } from '$app/state';
	import { onMount } from 'svelte';
	import FaceAvatar from '#lib/components/photos/FaceAvatar.svelte';
	import PhotosTabs from '#lib/components/photos/PhotosTabs.svelte';
	import PlaceCard from '#lib/components/photos/PlaceCard.svelte';
	import TagCard from '#lib/components/photos/TagCard.svelte';
	import Page from '#lib/components/shell/Page.svelte';
	import Alert from '#lib/components/ui/Alert.svelte';
	import Button from '#lib/components/ui/Button.svelte';
	import EmptyState from '#lib/components/ui/EmptyState.svelte';
	import { listPeople, listPlaces, listTags, type Person, type PlaceGroup, type TagGroup } from '#lib/explore/api.ts';
	import { personLabel } from '#lib/explore/faces.ts';
	import { photosHref } from '#lib/explore/filters.ts';

	/** Cards shown per section before "See all". */
	const PREVIEW = 6;

	let people: Person[] | null = $state(null);
	let places: PlaceGroup[] | null = $state(null);
	let tags: TagGroup[] | null = $state(null);
	let error: string | null = $state(null);

	const loaded = $derived(people !== null && places !== null && tags !== null);
	const empty = $derived((people ?? []).length + (places ?? []).length + (tags ?? []).length === 0);

	onMount(async () => {
		const [p, pl, t] = await Promise.all([listPeople(), listPlaces('city'), listTags()]);
		people = p.ok ? p.data : [];
		places = pl.ok ? pl.data : [];
		tags = t.ok ? t.data : [];
		const failed = [p, pl, t].find((outcome) => !outcome.ok);
		if (failed && !failed.ok) error = failed.error;
	});
</script>

<svelte:head><title>Explore · Squirrel</title></svelte:head>

<Page title="Explore">
	{#snippet description()}People, places and things in your photos.{/snippet}

	<PhotosTabs path={page.url.pathname} />
	{#if error}<Alert>{error}</Alert>{/if}

	{#if !loaded}
		{#if !error}<p class="muted" aria-live="polite">Loading…</p>{/if}
	{:else if empty && !error}
		<EmptyState icon="sparkles" title="Nothing to explore yet">
			People, places and tags appear here as your photos are analysed.
		</EmptyState>
	{:else}
		{#if people && people.length > 0}
			<section class="section" aria-labelledby="explore-people">
				<h2 id="explore-people">People</h2>
				<ul class="people">
					{#each people.slice(0, 12) as person (person.id)}
						<li>
							<a class="person" href={photosHref({ person: person.id })}>
								<FaceAvatar face={person.cover} size="md" />
								<span class="name" class:unnamed={!person.name}>{personLabel(person.name)}</span>
							</a>
						</li>
					{/each}
				</ul>
			</section>
		{/if}

		{#if places && places.length > 0}
			<section class="section" aria-labelledby="explore-places">
				<div class="head">
					<h2 id="explore-places">Places</h2>
					<Button variant="quiet" size="sm" href="/explore/places">See all</Button>
				</div>
				<ul class="cards">
					{#each places.slice(0, PREVIEW) as place (`${place.country_code}/${place.region}/${place.city}`)}
						<li><PlaceCard {place} /></li>
					{/each}
				</ul>
			</section>
		{/if}

		{#if tags && tags.length > 0}
			<section class="section" aria-labelledby="explore-tags">
				<div class="head">
					<h2 id="explore-tags">Tags</h2>
					<Button variant="quiet" size="sm" href="/explore/tags">See all</Button>
				</div>
				<ul class="cards">
					{#each tags.slice(0, PREVIEW) as tag (tag.tag)}
						<li><TagCard {tag} /></li>
					{/each}
				</ul>
			</section>
		{/if}
	{/if}
</Page>

<style>
	.section {
		display: grid;
		gap: var(--space-3);
	}

	.head {
		display: flex;
		align-items: center;
		justify-content: space-between;
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

	.people {
		display: flex;
		gap: var(--space-3);
		margin: 0;
		padding: var(--space-1) 0 var(--space-2);
		overflow-x: auto;
		list-style: none;
	}

	.person {
		display: grid;
		justify-items: center;
		gap: var(--space-2);
		width: var(--person-min);
		padding: var(--space-2);
		border-radius: var(--radius-card);
		color: var(--ink);
		text-decoration: none;
		transition:
			background-color var(--dur-fast) var(--ease),
			transform var(--dur-fast) var(--ease);
	}

	.name {
		max-width: 100%;
		overflow: hidden;
		text-overflow: ellipsis;
		white-space: nowrap;
		font-size: var(--text-sm);
		line-height: var(--text-sm-lh);
		font-weight: var(--weight-label);
	}

	.unnamed {
		color: var(--ink-muted);
	}

	@media (hover: hover) {
		.person:hover {
			background: var(--tint-strong);
		}
	}

	.person:focus-visible {
		outline: var(--ring-width) solid var(--accent);
		outline-offset: var(--ring-offset);
	}

	.person:active {
		transform: translateY(var(--press));
	}

	@media (min-width: 48rem) {
		.cards {
			grid-template-columns: repeat(auto-fill, minmax(var(--album-min), 1fr));
			gap: var(--space-4);
		}
	}
</style>
