<script lang="ts">
	import { page } from '$app/state';
	import { onMount } from 'svelte';
	import Icon from '#lib/components/icons/Icon.svelte';
	import MergeDialog from '#lib/components/photos/MergeDialog.svelte';
	import PeopleGrid from '#lib/components/photos/PeopleGrid.svelte';
	import PhotosTabs from '#lib/components/photos/PhotosTabs.svelte';
	import SelectionBar from '#lib/components/photos/SelectionBar.svelte';
	import Page from '#lib/components/shell/Page.svelte';
	import Alert from '#lib/components/ui/Alert.svelte';
	import Button from '#lib/components/ui/Button.svelte';
	import EmptyState from '#lib/components/ui/EmptyState.svelte';
	import Toggle from '#lib/components/ui/Toggle.svelte';
	import { listPeople, type Person } from '#lib/explore/api.ts';
	import { personLabel } from '#lib/explore/faces.ts';
	import { emptyReason, getMlSettings, MAX_MERGE, mergePeople } from '#lib/explore/people.ts';
	import { mediaStatus } from '#lib/photos/api.ts';

	let people = $state<Person[] | null>(null);
	let error: string | null = $state(null);
	let showHidden = $state(false);
	let selecting = $state(false);
	let selected = $state(new Set<string>());
	let merging = $state(false);
	let notice: string | null = $state(null);
	let reason: ReturnType<typeof emptyReason> = $state(null);

	const chosen = $derived((people ?? []).filter((person) => selected.has(person.id)));

	async function load() {
		const result = await listPeople(showHidden);
		if (result.ok) {
			people = result.data;
			error = null;
			const present = new Set(result.data.map((person) => person.id));
			selected = new Set([...selected].filter((id) => present.has(id)));
		} else error = result.error;
	}

	async function explainEmpty() {
		const [settings, status] = await Promise.all([getMlSettings(), mediaStatus()]);
		reason = emptyReason({
			facesEnabled: settings.ok ? settings.data.faces_enabled : null,
			facesInstalled: status.ok ? status.data.ml.models_installed.faces : null,
			analysing: status.ok ? status.data.ml.queued : 0
		});
	}

	function toggle(id: string) {
		const next = new Set(selected);
		if (next.has(id)) next.delete(id);
		else if (next.size < MAX_MERGE + 1) next.add(id);
		selected = next;
	}

	function stopSelecting() {
		selecting = false;
		selected = new Set();
	}

	async function merge(keep: Person): Promise<string | null> {
		const from = chosen.filter((person) => person.id !== keep.id).map((person) => person.id);
		const result = await mergePeople(keep.id, from);
		if (!result.ok) return result.error;
		notice = `Merged ${chosen.length} people into ${personLabel(result.data.name)}.`;
		stopSelecting();
		await load();
		return null;
	}

	onMount(async () => {
		await load();
		if (people?.length === 0) await explainEmpty();
	});
</script>

<svelte:head><title>People · Squirrel</title></svelte:head>

<Page title="People">
	{#snippet above()}<a class="back" href="/explore">Explore</a>{/snippet}
	{#snippet description()}{#if people}<span class="tabular">{people.length} {people.length === 1 ? 'person' : 'people'}</span>{/if}{/snippet}
	{#snippet actions()}
		{#if people && people.length > 1}
			{#if selecting}
				<Button variant="secondary" onclick={stopSelecting}>Done</Button>
			{:else}
				<Button variant="secondary" onclick={() => (selecting = true)}><Icon name="merge" />Merge people</Button>
			{/if}
		{/if}
	{/snippet}

	<PhotosTabs path={page.url.pathname} />
	<div class="tools">
		<Toggle label="Show hidden people" bind:checked={showHidden} onchange={() => void load()} />
	</div>
	<p class="visually-hidden" aria-live="polite">{notice ?? ''}</p>
	{#if notice}<Alert tone="success" role="note">{notice}</Alert>{/if}
	{#if error}<Alert>{error}</Alert>{/if}
	{#if selecting}<p class="muted">Choose the people who are the same person, then merge them.</p>{/if}

	{#if people === null}
		{#if !error}<p class="muted" aria-live="polite">Loading…</p>{/if}
	{:else if people.length === 0}
		{#if reason}
			<EmptyState icon="users" title={reason.title}>
				{reason.text}
				{#if reason.settings}<a href="/settings/faces">Open Settings › Faces</a>{/if}
			</EmptyState>
		{:else}
			<EmptyState icon="users" title="No people yet">People appear here once faces are found in your photos.</EmptyState>
		{/if}
	{:else}
		<PeopleGrid {people} {selecting} {selected} onToggle={toggle} />
	{/if}
</Page>

{#if selecting && selected.size > 0}
	<SelectionBar count={selected.size} label="Selected people" onClear={() => (selected = new Set())}>
		<Button size="sm" disabled={selected.size < 2} onclick={() => (merging = true)}>
			<Icon name="merge" /><span class="label">Merge</span>
		</Button>
	</SelectionBar>
{/if}

{#if merging && chosen.length > 1}
	<MergeDialog people={chosen} onMerge={merge} onClose={() => (merging = false)} />
{/if}

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

	.tools {
		display: flex;
		justify-content: flex-start;
	}
</style>
