<script lang="ts">
	import { page } from '$app/state';
	import { onMount } from 'svelte';
	import PhotosTabs from '#lib/components/photos/PhotosTabs.svelte';
	import TagCard from '#lib/components/photos/TagCard.svelte';
	import Page from '#lib/components/shell/Page.svelte';
	import Alert from '#lib/components/ui/Alert.svelte';
	import EmptyState from '#lib/components/ui/EmptyState.svelte';
	import { listTags, type TagGroup } from '#lib/explore/api.ts';

	let tags: TagGroup[] | null = $state(null);
	let error: string | null = $state(null);

	onMount(async () => {
		const result = await listTags();
		if (result.ok) tags = result.data;
		else error = result.error;
	});
</script>

<svelte:head><title>Tags · Squirrel</title></svelte:head>

<Page title="Tags">
	{#snippet above()}<a class="back" href="/explore">Explore</a>{/snippet}
	{#snippet description()}{#if tags}<span class="tabular">{tags.length} {tags.length === 1 ? 'tag' : 'tags'}</span>{/if}{/snippet}

	<PhotosTabs path={page.url.pathname} />
	{#if error}<Alert>{error}</Alert>{/if}
	{#if tags === null}
		{#if !error}<p class="muted" aria-live="polite">Loading…</p>{/if}
	{:else if tags.length === 0}
		<EmptyState icon="tag" title="No tags yet">Tags appear as your photos are analysed: beach, dog, snow…</EmptyState>
	{:else}
		<ul class="cards" aria-label="Tags">
			{#each tags as tag (tag.tag)}
				<li><TagCard {tag} /></li>
			{/each}
		</ul>
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
