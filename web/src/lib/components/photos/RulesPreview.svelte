<script lang="ts">
	import AlbumCover from '#lib/components/photos/AlbumCover.svelte';
	import { photoCount } from '#lib/photos/albums.ts';
	import { PREVIEW_DELAY_MS, PREVIEW_THUMBS, rulesBody, rulesProblem, type RulesForm } from '#lib/photos/albumRules.ts';
	import { previewAlbum, type AssetSummary } from '#lib/photos/api.ts';

	interface Props {
		form: RulesForm;
	}

	let { form }: Props = $props();

	const problem = $derived(rulesProblem(form));
	/** The body as text: a change that sends the same request asks nothing new. */
	const key = $derived(problem ? null : JSON.stringify(rulesBody(form)));

	let count: number | null = $state(null);
	let newest: AssetSummary[] = $state.raw([]);
	let error: string | null = $state(null);
	let loading = $state(false);

	// Asks once the rules stayed still for a moment; an answer for older rules is dropped.
	$effect(() => {
		const body = key;
		if (body === null) {
			loading = false;
			return;
		}
		loading = true;
		const controller = new AbortController();
		const timer = setTimeout(async () => {
			const result = await previewAlbum(JSON.parse(body), controller.signal);
			if (controller.signal.aborted) return;
			loading = false;
			if (result.ok) {
				count = result.data.count;
				newest = result.data.newest.slice(0, PREVIEW_THUMBS);
				error = null;
			} else error = result.error;
		}, PREVIEW_DELAY_MS);
		return () => {
			clearTimeout(timer);
			controller.abort();
		};
	});

	const message = $derived.by(() => {
		if (problem) return problem;
		if (error) return error;
		if (count === null) return 'Counting matching photos…';
		return count === 1 ? '1 photo matches' : `${photoCount(count)} match`;
	});
</script>

<section class="preview" aria-label="Matching photos" aria-busy={loading}>
	<p class="count tabular" class:muted={problem !== null || count === null} class:error={!problem && error !== null} aria-live="polite">
		{message}
	</p>
	{#if !problem && newest.length > 0}
		<ul class="thumbs" class:stale={loading}>
			{#each newest as asset (asset.id)}
				<li><AlbumCover cover={asset} icon="image" /></li>
			{/each}
		</ul>
	{/if}
</section>

<style>
	.preview {
		display: grid;
		gap: var(--space-3);
		padding: var(--space-3);
		border-radius: var(--radius-well);
		background: var(--tint);
	}

	.count {
		font-weight: var(--weight-label);
		color: var(--ink);
	}

	.count.muted {
		color: var(--ink-muted);
	}

	.count.error {
		color: var(--danger);
	}

	/* One row of PREVIEW_THUMBS. */
	.thumbs {
		display: grid;
		grid-template-columns: repeat(6, minmax(0, 1fr));
		gap: var(--space-1);
		margin: 0;
		padding: 0;
		list-style: none;
		transition: opacity var(--dur) var(--ease);
	}

	.thumbs li {
		aspect-ratio: 1;
		overflow: hidden;
		border-radius: var(--radius-tile);
	}

	.stale {
		opacity: var(--disabled-opacity);
	}
</style>
