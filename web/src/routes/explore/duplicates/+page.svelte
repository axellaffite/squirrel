<script lang="ts">
	import { page } from '$app/state';
	import { onMount } from 'svelte';
	import DuplicatePanel from '#lib/components/photos/DuplicatePanel.svelte';
	import Lightbox from '#lib/components/photos/Lightbox.svelte';
	import PhotoGrid from '#lib/components/photos/PhotoGrid.svelte';
	import PhotosTabs from '#lib/components/photos/PhotosTabs.svelte';
	import Page from '#lib/components/shell/Page.svelte';
	import Alert from '#lib/components/ui/Alert.svelte';
	import Button from '#lib/components/ui/Button.svelte';
	import EmptyState from '#lib/components/ui/EmptyState.svelte';
	import SegmentedControl from '#lib/components/ui/SegmentedControl.svelte';
	import {
		keepDuplicate,
		listDuplicates,
		restoreDuplicates,
		type DuplicateGroup,
		type GroupState
	} from '#lib/duplicates/api.ts';
	import { DuplicatesReview } from '#lib/duplicates/review.svelte.ts';
	import type { AssetSummary } from '#lib/photos/api.ts';
	import { trashPhotos } from '#lib/photos/trash.ts';

	const actions = { keep: keepDuplicate, restore: restoreDuplicates };
	const reviews: Record<GroupState, DuplicatesReview> = {
		open: new DuplicatesReview(actions),
		resolved: new DuplicatesReview(actions, { keepsResolved: true })
	};
	const loaded: Record<GroupState, boolean> = $state({ open: false, resolved: false });
	/** A page being fetched, per tab (one at a time). */
	const fetching: Record<GroupState, boolean> = { open: false, resolved: false };

	let tab: GroupState = $state('open');
	let error: string | null = $state(null);
	/** The last kept group, for "Undo". */
	let lastKept: { group: DuplicateGroup; name: string } | null = $state(null);
	let grid: PhotoGrid | undefined = $state();

	const review = $derived(reviews[tab]);
	const viewing = $derived(review.shown?.asset ?? null);
	const options = $derived([
		{ value: 'open' as const, label: loaded.open ? `To review (${reviews.open.openCount})` : 'To review' },
		{ value: 'resolved' as const, label: 'Resolved' }
	]);

	/** The first page of `state` (again), or the next one. */
	async function fetchPage(state: GroupState, next: boolean) {
		const target = reviews[state];
		if (fetching[state] || (next && !target.cursor)) return;
		fetching[state] = true;
		try {
			const outcome = await listDuplicates(state, next ? target.cursor : null);
			if (!outcome.ok) {
				error = outcome.error;
				return;
			}
			if (next) target.append(outcome.data);
			else target.load(outcome.data);
			loaded[state] = true;
		} finally {
			fetching[state] = false;
		}
	}

	onMount(() => {
		void fetchPage('open', false);
		void fetchPage('resolved', false);
	});

	function chooseTab(value: GroupState) {
		review.close();
		tab = value;
	}

	function badge(asset: AssetSummary) {
		const count = review.countFor(asset.id);
		return count ? { text: `×${count}`, label: `one of ${count} near-identical photos` } : null;
	}

	function closeViewer() {
		const tile = review.tile?.id;
		review.close();
		if (tile) void grid?.focusTile(tile);
	}

	async function keep() {
		const current = review.group;
		const kept = review.shown?.asset.id;
		const resolved = await review.keepShown();
		if (!resolved || !current || !kept) return;
		const name = current.members.find((member) => member.asset.id === kept)?.asset.name ?? 'the photo';
		lastKept = { group: current, name };
		void fetchPage('resolved', false);
		// Keep a few groups ahead loaded while deciding one after another.
		if (review.groups.length - review.groupIndex < 5) void fetchPage('open', true);
	}

	async function undo() {
		if (!lastKept) return;
		const outcome = await restoreDuplicates(lastKept.group.id);
		if (!outcome.ok) {
			error = outcome.error;
			return;
		}
		lastKept = null;
		if (outcome.data) reviews.open.restoreToQueue(outcome.data);
		void fetchPage('resolved', false);
	}

	async function trashShown(asset: AssetSummary): Promise<string | null> {
		const result = await trashPhotos([asset.id]);
		if (result.error) return result.error;
		review.close();
		await Promise.all([fetchPage('open', false), fetchPage('resolved', false)]);
		return null;
	}
</script>

<svelte:head><title>Duplicates · Squirrel</title></svelte:head>

<Page title="Duplicates">
	{#snippet description()}Near-identical photos taken within a day. Keep one: the others are hidden everywhere, never deleted.{/snippet}

	<PhotosTabs path={page.url.pathname} />
	<SegmentedControl label="Duplicates" name="duplicates-state" {options} value={tab} onchange={chooseTab} />

	{#if error}<Alert>{error}</Alert>{/if}
	{#if review.error && !viewing}<Alert>{review.error}</Alert>{/if}
	{#if lastKept && tab === 'open'}
		<Alert tone="success">
			Kept {lastKept.name}; the other photos are hidden.
			<Button variant="quiet" size="sm" onclick={undo}>Undo</Button>
		</Alert>
	{/if}

	{#if !loaded[tab]}
		<p class="muted" aria-live="polite">Loading…</p>
	{:else if review.groups.length === 0}
		{#if tab === 'open'}
			<EmptyState icon="check-circle" title="No duplicates to review">
				Near-identical photos show up here once they are analysed.
			</EmptyState>
		{:else}
			<EmptyState icon="eye-off" title="Nothing hidden">Photos you hide by keeping another one show up here.</EmptyState>
		{/if}
	{:else}
		<section aria-label={tab === 'open' ? 'Groups to review' : 'Resolved groups'}>
			{#key tab}
				<PhotoGrid
					bind:this={grid}
					items={review.tiles}
					selected={new Set()}
					selecting={false}
					days={false}
					{badge}
					onOpen={(asset) => review.open(asset.id)}
					onToggle={() => {}}
					onNearEnd={() => void fetchPage(tab, true)}
				/>
			{/key}
		</section>
	{/if}
</Page>

{#if viewing}
	<Lightbox
		asset={viewing}
		items={review.tiles}
		current={review.tile?.id}
		onNavigate={(asset) => review.open(asset.id)}
		onClose={closeViewer}
		onDelete={trashShown}
		panelLabel="Near-identical photos"
		onStep={(delta) => review.step(delta)}
	>
		{#snippet panel()}
			{#if review.error}<Alert>{review.error}</Alert>{/if}
			<DuplicatePanel {review} mode={tab} onKeep={keep} />
		{/snippet}
	</Lightbox>
{/if}

<style>
	.muted {
		color: var(--ink-muted);
	}
</style>
