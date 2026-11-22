<script lang="ts">
	import { page } from '$app/state';
	import { onMount } from 'svelte';
	import DuplicateReview from '#lib/components/photos/DuplicateReview.svelte';
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

	const actions = { keep: keepDuplicate, restore: restoreDuplicates };
	const open = new DuplicatesReview(actions);
	const resolved = new DuplicatesReview(actions, { keepsResolved: true });

	let tab: GroupState = $state('open');
	let loaded = $state(false);
	let error: string | null = $state(null);
	/** The last kept group, for "Undo". */
	let lastKept: { group: DuplicateGroup; name: string } | null = $state(null);

	const review = $derived(tab === 'open' ? open : resolved);
	const options = $derived([
		{ value: 'open' as const, label: loaded ? `To review (${open.groups.length})` : 'To review' },
		{ value: 'resolved' as const, label: 'Resolved' }
	]);

	async function load() {
		const [o, r] = await Promise.all([listDuplicates('open'), listDuplicates('resolved')]);
		if (o.ok) open.load(o.data);
		if (r.ok) resolved.load(r.data);
		const failed = [o, r].find((outcome) => !outcome.ok);
		error = failed && !failed.ok ? failed.error : null;
		loaded = true;
	}

	onMount(load);

	function onkept(group: DuplicateGroup, kept: string) {
		const name = group.members.find((member) => member.asset.id === kept)?.asset.name ?? 'the photo';
		lastKept = { group, name };
		void listDuplicates('resolved').then((r) => r.ok && resolved.load(r.data));
	}

	async function undo() {
		if (!lastKept) return;
		const { group } = lastKept;
		const outcome = await restoreDuplicates(group.id);
		if (!outcome.ok) {
			error = outcome.error;
			return;
		}
		lastKept = null;
		if (outcome.data) open.restoreToQueue(outcome.data);
		const r = await listDuplicates('resolved');
		if (r.ok) resolved.load(r.data);
	}

	/** Arrow keys move through the group's photos, unless typing or choosing elsewhere. */
	function onkeydown(event: KeyboardEvent) {
		const target = event.target as HTMLElement | null;
		if (event.altKey || event.ctrlKey || event.metaKey) return;
		if (target?.closest('input, textarea, select, [contenteditable="true"], dialog')) return;
		if (review.handleKey(event.key)) event.preventDefault();
	}
</script>

<svelte:head><title>Duplicates · Squirrel</title></svelte:head>
<svelte:window {onkeydown} />

<Page title="Duplicates">
	{#snippet description()}Near-identical photos taken within a day. Keep one: the others are hidden everywhere, never deleted.{/snippet}

	<PhotosTabs path={page.url.pathname} />
	<SegmentedControl label="Duplicates" name="duplicates-state" {options} value={tab} onchange={(value) => (tab = value)} />

	{#if error}<Alert>{error}</Alert>{/if}
	{#if review.error}<Alert>{review.error}</Alert>{/if}
	{#if lastKept && tab === 'open'}
		<Alert tone="success">
			Kept {lastKept.name}; the other photos are hidden.
			<Button variant="quiet" size="sm" onclick={undo}>Undo</Button>
		</Alert>
	{/if}

	{#if !loaded}
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
		<DuplicateReview {review} mode={tab} {onkept} />
	{/if}
</Page>

<style>
	.muted {
		color: var(--ink-muted);
	}
</style>
