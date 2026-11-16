<script lang="ts">
	import { replaceState } from '$app/navigation';
	import { page } from '$app/state';
	import { onMount, untrack } from 'svelte';
	import NodeVisual from '#lib/components/files/NodeVisual.svelte';
	import SearchField from '#lib/components/files/SearchField.svelte';
	import Page from '#lib/components/shell/Page.svelte';
	import Alert from '#lib/components/ui/Alert.svelte';
	import Button from '#lib/components/ui/Button.svelte';
	import EmptyState from '#lib/components/ui/EmptyState.svelte';
	import ListRow from '#lib/components/ui/ListRow.svelte';
	import {
		hasShortWord,
		isSearchable,
		kindLabel,
		normalizeQuery,
		pagesLabel,
		resultHref,
		shortDate,
		snippetParts,
		type SearchHit
	} from '#lib/files/search.ts';
	import { Searcher } from '#lib/files/searcher.svelte.ts';

	const searcher = new Searcher();
	let value = $state(untrack(() => page.url.searchParams.get('q') ?? ''));

	const typed = $derived(normalizeQuery(value));
	const showShortHint = $derived(isSearchable(typed) && hasShortWord(typed));
	const settled = $derived(searcher.query !== '' && searcher.query === typed && !searcher.loading);

	/** Keeps `?q=` in the address, so reloading or going back shows the same search. */
	function remember(query: string) {
		const url = new URL(page.url.href);
		if (query) url.searchParams.set('q', query);
		else url.searchParams.delete('q');
		replaceState(url, {});
	}

	function typing(next: string) {
		searcher.type(next);
		remember(normalizeQuery(next));
	}

	function metaParts(hit: SearchHit): string[] {
		return [kindLabel(hit.node.mime), pagesLabel(hit.page_count), shortDate(hit.node.modified_at)].filter(
			(part): part is string => part !== null
		);
	}

	onMount(() => {
		searcher.now(value);
		return () => searcher.dispose();
	});
</script>

<svelte:head><title>Search · Squirrel</title></svelte:head>

<Page title="Search">
	{#snippet description()}Find files by name, and PDFs and text files by their content.{/snippet}

	<div class="field">
		<SearchField bind:value oninput={typing} onsubmit={(query) => searcher.now(query)} autofocus />
		{#if showShortHint}<p class="hint">Words of one or two letters only match whole words.</p>{/if}
	</div>

	<div class="live" aria-live="polite">
		{#if searcher.error}<Alert>{searcher.error}</Alert>{/if}
		{#if settled}<p class="visually-hidden">{searcher.items.length}{searcher.hasMore ? ' or more' : ''} results</p>{/if}
	</div>

	{#if searcher.items.length > 0}
		<ul class="rows" aria-label="Search results" aria-busy={searcher.loading}>
			{#each searcher.items as hit (hit.node.id)}
				<ListRow nodeId={hit.node.id}>
					{#snippet icon()}<NodeVisual node={hit.node} />{/snippet}
					{#snippet title()}<a class="row-link" href={resultHref(hit.node)}>{hit.node.name}</a>{/snippet}
					{#snippet meta()}
						{#each metaParts(hit) as part, index (index)}{#if index > 0}<span class="sep" aria-hidden="true">·</span>{/if}{part}{/each}
					{/snippet}
					{#snippet detail()}
						{#if hit.snippet.length > 0}
							<p class="snippet">
								{#each snippetParts(hit.snippet) as part, index (index)}{#if part.highlight}<mark>{part.text}</mark>{:else}{part.text}{/if}{/each}
							</p>
						{/if}
					{/snippet}
				</ListRow>
			{/each}
		</ul>
		{#if searcher.hasMore}
			<div class="more">
				<Button variant="secondary" disabled={searcher.loading} onclick={() => searcher.more()}>Load more</Button>
			</div>
		{/if}
	{:else if settled && !searcher.error}
		<EmptyState icon="search" title="Nothing found">
			Search looks at file names, PDFs and text files. Scanned documents without text are not searchable yet.
		</EmptyState>
	{:else if searcher.loading}
		<p class="muted">Searching…</p>
	{/if}
</Page>

<style>
	.field {
		display: grid;
		gap: var(--space-2);
		max-width: var(--measure);
	}

	.hint {
		padding-inline: var(--space-4);
		color: var(--ink-muted);
		font-size: var(--text-sm);
		line-height: var(--text-sm-lh);
	}

	.live {
		display: contents;
	}

	.rows {
		display: grid;
		gap: var(--space-2);
		margin: 0;
		padding: 0;
		list-style: none;
	}

	.snippet {
		max-width: none;
	}

	/* MASTER: --accent-soft is the text highlight colour. */
	mark {
		border-radius: var(--radius-sm);
		background: var(--accent-soft);
		color: var(--ink);
	}

	.more {
		display: flex;
		justify-content: center;
	}

	@media (forced-colors: active) {
		mark {
			background: Highlight;
			color: HighlightText;
		}
	}
</style>
