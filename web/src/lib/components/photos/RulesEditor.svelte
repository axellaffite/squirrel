<script lang="ts">
	import Icon from '#lib/components/icons/Icon.svelte';
	import FilterPicker from '#lib/components/photos/FilterPicker.svelte';
	import PersonPicker from '#lib/components/photos/PersonPicker.svelte';
	import RuleChips from '#lib/components/photos/RuleChips.svelte';
	import Button from '#lib/components/ui/Button.svelte';
	import SegmentedControl from '#lib/components/ui/SegmentedControl.svelte';
	import TextField from '#lib/components/ui/TextField.svelte';
	import { personChipLabel, type Person } from '#lib/explore/api.ts';
	import { tagLabel, type PhotoFilter } from '#lib/explore/filters.ts';
	import {
		MAX_RULE_ITEMS,
		cameraLabel,
		placeLabel,
		withItem,
		withoutItem,
		type RuleKind,
		type RulesForm
	} from '#lib/photos/albumRules.ts';
	import type { MatchMode } from '#lib/photos/api.ts';

	interface Props {
		form: RulesForm;
		/** The caller's people (hidden ones included), to name and show the chosen ones. */
		people: readonly Person[];
		onChange: (form: RulesForm) => void;
	}

	let { form, people, onChange }: Props = $props();

	type Picking = 'person' | 'tags' | 'places' | 'cameras' | null;
	let picking: Picking = $state(null);
	const uid = $props.id();

	const MATCH = [
		{ value: 'any', label: 'Any of them' },
		{ value: 'all', label: 'All of them' }
	] as const satisfies readonly { value: MatchMode; label: string }[];

	const KINDS = [
		{ value: 'any', label: 'Any' },
		{ value: 'photo', label: 'Photos' },
		{ value: 'screenshot', label: 'Screenshots' }
	] as const satisfies readonly { value: RuleKind; label: string }[];

	const PICKER_TITLES = { tags: 'Add a tag', places: 'Choose a place', cameras: 'Choose a camera' } as const;

	const update = (patch: Partial<RulesForm>) => onChange({ ...form, ...patch });

	const personChips = $derived(
		form.people.map((id) => {
			const person = people.find((p) => p.id === id);
			return { key: id, label: personChipLabel(person) ?? 'Unnamed', face: person?.cover ?? null };
		})
	);
	const tagChips = $derived(form.tags.map((tag) => ({ key: tag, label: tagLabel(tag), icon: 'tag' as const })));
	const placeChips = $derived(form.place ? [{ key: 'place', label: placeLabel(form.place), icon: 'map-pin' as const }] : []);
	const cameraChips = $derived(form.camera ? [{ key: 'camera', label: cameraLabel(form.camera), icon: 'camera' as const }] : []);

	/** One part chosen in the shared filter picker. */
	function picked(filter: PhotoFilter) {
		if (filter.tag) update({ tags: withItem(form.tags, filter.tag) });
		else if (filter.country) update({ place: { country: filter.country, region: filter.region, city: filter.city } });
		else if (filter.camera_make) update({ camera: { make: filter.camera_make, model: filter.camera_model } });
	}
</script>

<div class="groups">
	<section class="group" aria-labelledby={`people-${uid}`}>
		<h3 id={`people-${uid}`}>People</h3>
		{#if personChips.length > 0}
			<RuleChips chips={personChips} subject="person" onRemove={(id) => update({ people: withoutItem(form.people, id) })} />
		{/if}
		<div class="row">
			<Button variant="secondary" size="sm" disabled={form.people.length >= MAX_RULE_ITEMS} onclick={() => (picking = 'person')} aria-haspopup="dialog">
				<Icon name="plus" />Add person
			</Button>
			{#if form.people.length > 1}
				<SegmentedControl label="Photos with" name={`people-match-${uid}`} options={MATCH} value={form.peopleMatch} onchange={(value) => update({ peopleMatch: value })} />
			{/if}
		</div>
	</section>

	<section class="group" aria-labelledby={`tags-${uid}`}>
		<h3 id={`tags-${uid}`}>Tags</h3>
		{#if tagChips.length > 0}
			<RuleChips chips={tagChips} subject="tag" onRemove={(tag) => update({ tags: withoutItem(form.tags, tag) })} />
		{/if}
		<div class="row">
			<Button variant="secondary" size="sm" disabled={form.tags.length >= MAX_RULE_ITEMS} onclick={() => (picking = 'tags')} aria-haspopup="dialog">
				<Icon name="plus" />Add tag
			</Button>
			{#if form.tags.length > 1}
				<SegmentedControl label="Photos with" name={`tags-match-${uid}`} options={MATCH} value={form.tagsMatch} onchange={(value) => update({ tagsMatch: value })} />
			{/if}
		</div>
	</section>

	<section class="group" aria-labelledby={`place-${uid}`}>
		<h3 id={`place-${uid}`}>Place</h3>
		{#if placeChips.length > 0}
			<RuleChips chips={placeChips} subject="place" onRemove={() => update({ place: null })} />
		{:else}
			<div class="row">
				<Button variant="secondary" size="sm" onclick={() => (picking = 'places')} aria-haspopup="dialog"><Icon name="map-pin" />Choose place</Button>
			</div>
		{/if}
	</section>

	<section class="group" aria-labelledby={`camera-${uid}`}>
		<h3 id={`camera-${uid}`}>Camera</h3>
		{#if cameraChips.length > 0}
			<RuleChips chips={cameraChips} subject="camera" onRemove={() => update({ camera: null })} />
		{:else}
			<div class="row">
				<Button variant="secondary" size="sm" onclick={() => (picking = 'cameras')} aria-haspopup="dialog"><Icon name="camera" />Choose camera</Button>
			</div>
		{/if}
	</section>

	<section class="group" aria-labelledby={`kind-${uid}`}>
		<h3 id={`kind-${uid}`}>Kind</h3>
		<SegmentedControl label="Kind" name={`kind-${uid}`} options={KINDS} value={form.kind} onchange={(value) => update({ kind: value })} />
	</section>

	<section class="group" aria-labelledby={`dates-${uid}`}>
		<h3 id={`dates-${uid}`}>Taken between</h3>
		<div class="dates">
			<TextField label="From" type="date" value={form.takenFrom} max={form.takenTo || undefined} oninput={(event) => update({ takenFrom: event.currentTarget.value })} />
			<TextField label="To" type="date" value={form.takenTo} min={form.takenFrom || undefined} oninput={(event) => update({ takenTo: event.currentTarget.value })} />
		</div>
	</section>
</div>

{#if picking === 'person'}
	<PersonPicker
		title="Add a person"
		exclude={form.people}
		onPick={(choice) => {
			if ('person' in choice) update({ people: withItem(form.people, choice.person.id) });
		}}
		onClose={() => (picking = null)}
	/>
{:else if picking}
	<FilterPicker filter={{}} people={[]} title={PICKER_TITLES[picking]} only={[picking]} onPick={picked} onClose={() => (picking = null)} />
{/if}

<style>
	.groups {
		display: grid;
		gap: var(--space-4);
	}

	.group {
		display: grid;
		gap: var(--space-2);
	}

	h3 {
		color: var(--ink-muted);
		font-size: var(--text-sm);
		line-height: var(--text-sm-lh);
		font-weight: var(--weight-label);
		letter-spacing: var(--track-label);
	}

	.row {
		display: flex;
		flex-wrap: wrap;
		align-items: center;
		gap: var(--space-2);
	}

	.dates {
		display: grid;
		grid-template-columns: repeat(2, minmax(0, 1fr));
		gap: var(--space-3);
	}
</style>
