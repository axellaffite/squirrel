<script lang="ts">
	import { goto } from '$app/navigation';
	import { onMount, tick, untrack } from 'svelte';
	import ConfirmDialog from '#lib/components/ConfirmDialog.svelte';
	import Icon from '#lib/components/icons/Icon.svelte';
	import FaceAvatar from '#lib/components/photos/FaceAvatar.svelte';
	import Lightbox from '#lib/components/photos/Lightbox.svelte';
	import PersonPicker, { type PersonChoice } from '#lib/components/photos/PersonPicker.svelte';
	import SelectionBar from '#lib/components/photos/SelectionBar.svelte';
	import Timeline from '#lib/components/photos/Timeline.svelte';
	import Page from '#lib/components/shell/Page.svelte';
	import Alert from '#lib/components/ui/Alert.svelte';
	import Button from '#lib/components/ui/Button.svelte';
	import EmptyState from '#lib/components/ui/EmptyState.svelte';
	import TextField from '#lib/components/ui/TextField.svelte';
	import { getPerson, type Face, type Person } from '#lib/explore/api.ts';
	import { personLabel } from '#lib/explore/faces.ts';
	import { bulk, faceTileLabels, facesCount, Generations, moveRequest, pruneSelection } from '#lib/explore/personFaces.ts';
	import {
		assignFace,
		cleanName,
		listPersonFaces,
		mergePeople,
		renamePerson,
		setPersonCover,
		setPersonHidden,
		splitPerson,
		unassignFace
	} from '#lib/explore/people.ts';
	import type { AssetSummary } from '#lib/photos/api.ts';
	import { TimelineData } from '#lib/photos/timeline.svelte.ts';
	import { neighbourAfterRemoval, trashPhotos } from '#lib/photos/trash.ts';

	interface Props {
		personId: string;
	}

	let { personId }: Props = $props();
	// The page is keyed by person: another id is another component.
	const id = untrack(() => personId);
	const data = new TimelineData();

	let person = $state<Person | null>(null);
	let error: string | null = $state(null);
	let notice: string | null = $state(null);
	let tab: 'photos' | 'faces' = $state('photos');
	let faces = $state<Face[] | null>(null);
	let selected = $state(new Set<string>());
	let renaming = $state(false);
	let name = $state('');
	let nameError: string | null = $state(null);
	let viewing: AssetSummary | null = $state(null);
	let timeline: Timeline | undefined = $state();
	/** What the person picker is open for. */
	let picking: 'merge' | 'move' | null = $state(null);
	let mergeInto: Person | null = $state(null);

	const label = $derived(person ? personLabel(person.name) : 'Person');
	const tileLabels = $derived(faceTileLabels(faces ?? []));

	/** Reloads are numbered: an older answer never overwrites a newer one. */
	const personLoads = new Generations();
	const faceLoads = new Generations();

	async function loadPerson() {
		const ticket = personLoads.next();
		const result = await getPerson(id);
		if (!personLoads.current(ticket)) return;
		if (result.ok) person = result.data;
		else error = result.error;
	}

	async function loadFaces() {
		const ticket = faceLoads.next();
		const result = await listPersonFaces(id);
		if (!faceLoads.current(ticket)) return;
		if (result.ok) {
			faces = result.data;
			selected = pruneSelection(selected, result.data);
		} else error = result.error;
	}

	/** After a change: the person, their faces and photos again. */
	async function refresh(message: string) {
		notice = message;
		await Promise.all([loadPerson(), loadFaces(), data.reload()]);
	}

	async function startRename() {
		name = person?.name ?? '';
		nameError = null;
		renaming = true;
		await tick();
		document.getElementById('person-name')?.focus();
	}

	let renameButton: HTMLButtonElement | HTMLAnchorElement | undefined = $state();
	let savingName = $state(false);

	/** Closes the name field and gives the keyboard back to the Rename button. */
	async function stopRenaming() {
		renaming = false;
		await tick();
		renameButton?.focus();
	}

	async function saveName(event: SubmitEvent) {
		event.preventDefault();
		if (savingName) return;
		const clean = name.trim() === '' ? null : cleanName(name);
		if (name.trim() !== '' && clean === null) {
			nameError = 'Names use 1 to 100 characters, without control characters.';
			return;
		}
		savingName = true;
		const result = await renamePerson(id, clean);
		savingName = false;
		if (!result.ok) {
			nameError = result.error;
			return;
		}
		person = result.data;
		notice = clean ? `Renamed to ${clean}.` : 'Name removed.';
		await stopRenaming();
	}

	function nameKeys(event: KeyboardEvent) {
		if (event.key === 'Escape') {
			event.preventDefault();
			void stopRenaming();
		}
	}

	async function toggleHidden() {
		if (!person) return;
		const result = await setPersonHidden(id, !person.hidden);
		if (!result.ok) {
			error = result.error;
			return;
		}
		person = result.data;
		notice = result.data.hidden ? `${label} is hidden from Explore and filters.` : `${label} is shown again.`;
	}

	function toggleFace(faceId: string) {
		const next = new Set(selected);
		if (next.has(faceId)) next.delete(faceId);
		else next.add(faceId);
		selected = next;
	}

	/** A correction of several faces is running: a second click does nothing. */
	let correcting = $state(false);
	/** "Not {name}" asked to confirm, for this many faces. */
	let confirmingNot = $state(0);

	async function notThisPerson(): Promise<string | null> {
		if (correcting) return null;
		correcting = true;
		const ids = [...selected];
		const result = await bulk(ids, unassignFace);
		correcting = false;
		selected = result.stillSelected;
		error = result.error;
		await refresh(`${facesCount(result.done)} no longer ${label}.`);
		return null;
	}

	async function picked(choice: PersonChoice) {
		const purpose = picking;
		picking = null;
		if (purpose === 'merge' && 'person' in choice) {
			mergeInto = choice.person;
			return;
		}
		if (purpose !== 'move' || correcting) return;
		const ids = [...selected];
		const request = moveRequest(choice);
		correcting = true;
		if (request.kind === 'split') {
			const result = await splitPerson(id, ids);
			correcting = false;
			if (!result.ok) {
				error = result.error;
				return;
			}
			selected = new Set();
			await refresh(`${facesCount(ids.length)} moved to a new person.`);
			return;
		}
		const target = 'person' in choice ? personLabel(choice.person.name) : '';
		const result = await bulk(ids, (faceId) => assignFace(faceId, { person: request.person }));
		correcting = false;
		selected = result.stillSelected;
		error = result.error;
		await refresh(`${facesCount(result.done)} moved to ${target}.`);
	}

	async function merge(): Promise<string | null> {
		if (!mergeInto) return null;
		const result = await mergePeople(mergeInto.id, [id]);
		if (!result.ok) return result.error;
		await goto(`/explore/people/${mergeInto.id}`);
		return null;
	}

	async function useAsCover() {
		const [faceId] = [...selected];
		const result = await setPersonCover(id, faceId);
		if (!result.ok) {
			error = result.error;
			return;
		}
		person = result.data;
		selected = new Set();
		notice = 'Cover chosen.';
	}

	async function deleteViewed(asset: AssetSummary): Promise<string | null> {
		const after = neighbourAfterRemoval(data.ordered, asset.id);
		const result = await trashPhotos([asset.id]);
		if (result.error) return result.error;
		data.remove(result.moved);
		viewing = after;
		return null;
	}

	function closeViewer() {
		const shown = viewing?.id;
		viewing = null;
		if (shown) void timeline?.focusTile(shown);
	}

	onMount(() => {
		void loadPerson();
		void loadFaces();
		void data.setFilter({ person: id });
	});
</script>

<svelte:head><title>{label} · Squirrel</title></svelte:head>

<Page title={label}>
	{#snippet above()}<a class="back" href="/explore/people">People</a>{/snippet}
	{#snippet description()}
		{#if person}<span class="tabular">{person.face_count} {person.face_count === 1 ? 'photo' : 'photos'}{person.hidden ? ' · hidden' : ''}</span>{/if}
	{/snippet}
	{#snippet actions()}
		{#if person}
			<Button variant="secondary" bind:element={renameButton} onclick={startRename}><Icon name="pencil" />{person.name ? 'Rename' : 'Add a name'}</Button>
			<Button variant="secondary" onclick={toggleHidden}><Icon name="eye-off" />{person.hidden ? 'Unhide' : 'Hide'}</Button>
			<Button variant="secondary" onclick={() => (picking = 'merge')}><Icon name="merge" />Merge into…</Button>
		{/if}
	{/snippet}

	{#if person}
		<div class="who">
			<FaceAvatar face={person.cover} size="lg" />
			{#if renaming}
				<form class="rename" onsubmit={saveName}>
					<TextField id="person-name" label={`Name of ${label}`} bind:value={name} error={nameError} maxlength={100} onkeydown={nameKeys} hint="Enter saves, Escape cancels. Empty removes the name." />
					<div class="rename-actions">
						<Button type="submit" size="sm" disabled={savingName}>Save</Button>
						<Button variant="quiet" size="sm" onclick={() => void stopRenaming()}>Cancel</Button>
					</div>
				</form>
			{/if}
		</div>
	{/if}

	<p class="visually-hidden" aria-live="polite">{notice ?? ''}</p>
	{#if notice}<Alert tone="success" role="note">{notice}</Alert>{/if}
	{#if error}<Alert>{error}</Alert>{/if}

	<div class="tabs" role="tablist" aria-label="Show">
		<button type="button" role="tab" class="tab" aria-selected={tab === 'photos'} onclick={() => (tab = 'photos')}>Photos</button>
		<button type="button" role="tab" class="tab" aria-selected={tab === 'faces'} onclick={() => (tab = 'faces')}>Faces</button>
	</div>

	{#if tab === 'photos'}
		{#if data.months === null}
			{#if !data.error}<p class="muted" aria-live="polite">Loading…</p>{/if}
		{:else if data.months.length === 0}
			<EmptyState icon="images" title="No photos">This person's photos are all in the trash.</EmptyState>
		{:else}
			<section class="library" aria-label={`Photos of ${label}`}>
				<Timeline bind:this={timeline} {data} selected={new Set()} selecting={false} onOpen={(asset) => (viewing = asset)} onToggle={(asset) => (viewing = asset)} />
			</section>
		{/if}
	{:else if faces === null}
		<p class="muted" aria-live="polite">Loading…</p>
	{:else if faces.length === 0}
		<EmptyState icon="scan-face" title="No faces">Faces moved elsewhere or deleted leave this person empty.</EmptyState>
	{:else}
		<p class="muted">Select faces that are not {label}, or the one to show as the cover.</p>
		<ul class="faces" aria-label={`Faces of ${label}`}>
			{#each faces as face, index (face.id)}
				<li>
					<button
						type="button"
						class="face"
						aria-pressed={selected.has(face.id)}
						aria-label={tileLabels[index]}
						onclick={() => toggleFace(face.id)}
					>
						<FaceAvatar face={{ face_id: face.id, photo_id: face.photo_id, thumbnail_version: face.thumbnail_version, box: face.box }} size="md" />
					</button>
				</li>
			{/each}
		</ul>
	{/if}
</Page>

{#if tab === 'faces' && selected.size > 0}
	<SelectionBar count={selected.size} label="Selected faces" onClear={() => (selected = new Set())}>
		<Button variant="secondary" size="sm" disabled={correcting} onclick={() => (confirmingNot = selected.size)}>
			<Icon name="x" /><span class="label">Not {label}</span>
		</Button>
		<Button variant="secondary" size="sm" onclick={() => (picking = 'move')}><Icon name="arrow-right" /><span class="label">Move to…</span></Button>
		{#if selected.size === 1}
			<Button variant="secondary" size="sm" onclick={useAsCover}><Icon name="image" /><span class="label">Use as cover</span></Button>
		{/if}
	</SelectionBar>
{/if}

{#if picking}
	<PersonPicker
		title={picking === 'merge' ? `Merge ${label} into…` : 'Move the faces to…'}
		exclude={[id]}
		allowNew={picking === 'move'}
		onPick={picked}
		onClose={() => (picking = null)}
	/>
{/if}

{#if confirmingNot > 0}
	<ConfirmDialog
		title={`Remove ${facesCount(confirmingNot)} from ${label}?`}
		confirmLabel="Remove"
		onConfirm={notThisPerson}
		onClose={() => (confirmingNot = 0)}
	>
		They won't be grouped with anyone again unless you move them.
	</ConfirmDialog>
{/if}

{#if mergeInto}
	<ConfirmDialog
		title={`Merge ${label} into ${personLabel(mergeInto.name)}?`}
		confirmLabel="Merge"
		onConfirm={merge}
		onClose={() => (mergeInto = null)}
	>
		All of {label}'s faces become {personLabel(mergeInto.name)}'s, and {label} is removed.
	</ConfirmDialog>
{/if}

{#if viewing}
	<Lightbox asset={viewing} items={data.ordered} onNavigate={(asset) => (viewing = asset)} onClose={closeViewer} onDelete={deleteViewed} />
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

	.who {
		display: flex;
		flex-wrap: wrap;
		align-items: center;
		gap: var(--space-4);
	}

	.rename {
		display: grid;
		gap: var(--space-2);
		flex: 1;
		min-width: min(100%, var(--menu-w));
		max-width: var(--dialog-max);
	}

	.rename-actions {
		display: flex;
		gap: var(--space-2);
	}

	/* MASTER segmented control, as tabs. */
	.tabs {
		display: inline-flex;
		justify-self: start;
		gap: var(--space-1);
		padding: var(--space-1);
		border-radius: var(--radius-full);
		background: var(--tint);
	}

	.tab {
		position: relative;
		min-height: var(--control-sm);
		padding: 0 var(--space-4);
		border: 0;
		border-radius: var(--radius-full);
		background: transparent;
		color: var(--ink-muted);
		font: inherit;
		font-size: var(--text-sm);
		font-weight: var(--weight-label);
		cursor: pointer;
		transition:
			color var(--dur-fast) var(--ease),
			background-color var(--dur-fast) var(--ease),
			box-shadow var(--dur) var(--ease);
	}

	.tab::before {
		content: '';
		position: absolute;
		inset: calc((var(--control-sm) - var(--tap)) / 2) 0;
	}

	@media (hover: hover) {
		.tab:hover {
			color: var(--ink);
		}
	}

	.tab:focus-visible {
		outline: var(--ring-width) solid var(--accent);
		outline-offset: var(--ring-offset);
	}

	.tab[aria-selected='true'] {
		background: var(--card);
		color: var(--ink);
		box-shadow: var(--shadow-knob);
	}

	.library {
		display: grid;
		min-width: 0;
	}

	.faces {
		display: grid;
		grid-template-columns: repeat(auto-fill, minmax(var(--face-md), 1fr));
		gap: var(--space-3);
		margin: 0;
		padding: 0;
		list-style: none;
	}

	.face {
		display: grid;
		place-items: center;
		padding: var(--space-1);
		border: 0;
		border-radius: var(--radius-full);
		background: transparent;
		cursor: pointer;
		transition: transform var(--dur-fast) var(--ease);
	}

	.face[aria-pressed='true'] :global(.avatar) {
		box-shadow: var(--select-ring);
	}

	.face:focus-visible {
		outline: var(--ring-width) solid var(--accent);
		outline-offset: var(--ring-offset);
	}

	.face:active {
		transform: translateY(var(--press));
	}
</style>
