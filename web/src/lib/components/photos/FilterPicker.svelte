<script lang="ts">
	import { onMount } from 'svelte';
	import Icon from '#lib/components/icons/Icon.svelte';
	import type { IconName } from '#lib/components/icons/paths.ts';
	import FaceAvatar from '#lib/components/photos/FaceAvatar.svelte';
	import Alert from '#lib/components/ui/Alert.svelte';
	import Dialog from '#lib/components/ui/Dialog.svelte';
	import { listCameras, listPlaces, listTags, type CameraGroup, type Person, type PlaceGroup, type TagGroup } from '#lib/explore/api.ts';
	import { personLabel, type FaceCrop } from '#lib/explore/faces.ts';
	import { countryName, placeFilter, sameFilter, tagLabel, type PhotoFilter } from '#lib/explore/filters.ts';
	import { photoCount } from '#lib/photos/albums.ts';

	interface Props {
		filter: PhotoFilter;
		people: readonly Person[];
		/** Called with the filter once one more part is chosen. */
		onPick: (filter: PhotoFilter) => void;
		onClose: () => void;
	}

	let { filter, people, onPick, onClose }: Props = $props();

	let dialog: Dialog | undefined = $state();
	let open = $state(true);
	let tags: TagGroup[] | null = $state(null);
	let places: PlaceGroup[] | null = $state(null);
	let cameras: CameraGroup[] | null = $state(null);
	let error: string | null = $state(null);

	interface Option {
		key: string;
		label: string;
		count: number | null;
		icon?: IconName;
		face?: FaceCrop | null;
		next: PhotoFilter;
		chosen: boolean;
	}

	const named = $derived(people.filter((person) => !person.hidden));

	const sections = $derived.by((): { title: string; options: Option[] }[] => [
		{
			title: 'People',
			options: named.map((person) => ({
				key: person.id,
				label: personLabel(person.name),
				count: person.face_count,
				face: person.cover ?? null,
				next: { ...filter, person: person.id },
				chosen: filter.person === person.id
			}))
		},
		{
			title: 'Tags',
			options: (tags ?? []).map((tag) => ({
				key: tag.tag,
				label: tagLabel(tag.tag),
				count: tag.count,
				icon: 'tag' as const,
				next: { ...filter, tag: tag.tag },
				chosen: filter.tag === tag.tag
			}))
		},
		{
			title: 'Places',
			options: (places ?? []).map((place) => ({
				key: `${place.country_code}/${place.region ?? ''}/${place.city ?? ''}`,
				label: [place.city, place.country ?? countryName(place.country_code)].filter(Boolean).join(', '),
				count: place.count,
				icon: 'map-pin' as const,
				next: { ...filter, region: undefined, city: undefined, ...placeFilter(place) },
				chosen: sameFilter(placeFilter(place), { country: filter.country, region: filter.region, city: filter.city })
			}))
		},
		{
			title: 'Cameras',
			options: (cameras ?? []).map((camera) => ({
				key: `${camera.make}/${camera.model ?? ''}`,
				label: camera.model ? `${camera.make} ${camera.model}` : camera.make,
				count: camera.count,
				icon: 'camera' as const,
				next: { ...filter, camera_make: camera.make, camera_model: camera.model ?? undefined },
				chosen: filter.camera_make === camera.make && filter.camera_model === (camera.model ?? undefined)
			}))
		},
		{
			title: 'Kind',
			options: [
				{ key: 'photo', label: 'Photos only', count: null, icon: 'image' as const, next: { ...filter, kind: 'photo' as const }, chosen: filter.kind === 'photo' },
				{ key: 'screenshot', label: 'Screenshots', count: null, icon: 'image' as const, next: { ...filter, kind: 'screenshot' as const }, chosen: filter.kind === 'screenshot' }
			]
		}
	]);

	const loading = $derived(tags === null || places === null || cameras === null);

	onMount(async () => {
		const [t, p, c] = await Promise.all([listTags(), listPlaces('city'), listCameras()]);
		tags = t.ok ? t.data : [];
		places = p.ok ? p.data : [];
		cameras = c.ok ? c.data : [];
		const failed = [t, p, c].find((outcome) => !outcome.ok);
		if (failed && !failed.ok) error = failed.error;
	});

	function pick(option: Option) {
		onPick(option.next);
		dialog?.close();
	}
</script>

<Dialog bind:this={dialog} bind:open title="Filter photos" onclose={onClose}>
	<div class="body">
		{#if error}<Alert>{error}</Alert>{/if}
		{#each sections as section (section.title)}
			{#if section.options.length > 0}
				<section aria-labelledby={`filter-${section.title}`}>
					<h3 id={`filter-${section.title}`}>{section.title}</h3>
					<ul class="options">
						{#each section.options as option (option.key)}
							<li>
								<button type="button" class="option" aria-pressed={option.chosen} onclick={() => pick(option)}>
									{#if section.title === 'People'}
										<FaceAvatar face={option.face} />
									{:else}
										<span class="well"><Icon name={option.icon ?? 'image'} /></span>
									{/if}
									<span class="text">
										<span class="name">{option.label}</span>
										{#if option.count !== null}<span class="count tabular">{photoCount(option.count)}</span>{/if}
									</span>
									{#if option.chosen}<Icon name="check" />{/if}
								</button>
							</li>
						{/each}
					</ul>
				</section>
			{/if}
		{/each}
		{#if loading && !error}<p class="muted" aria-live="polite">Loading…</p>{/if}
	</div>
</Dialog>

<style>
	.body {
		display: grid;
		gap: var(--space-4);
	}

	section {
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

	.options {
		display: grid;
		gap: var(--space-1);
		max-height: var(--picker-max);
		margin: 0;
		padding: var(--space-2);
		overflow-y: auto;
		list-style: none;
		border-radius: var(--radius-well);
		background: var(--tint);
	}

	.option {
		display: flex;
		align-items: center;
		gap: var(--space-3);
		width: 100%;
		min-height: var(--row-min);
		padding: var(--space-2);
		border: 0;
		border-radius: var(--radius-card);
		background: transparent;
		color: var(--ink);
		text-align: left;
		cursor: pointer;
		transition:
			background-color var(--dur-fast) var(--ease),
			box-shadow var(--dur) var(--ease),
			transform var(--dur-fast) var(--ease);
	}

	.option[aria-pressed='true'] {
		background: var(--card);
		box-shadow: var(--shadow-knob);
	}

	.well {
		display: grid;
		place-items: center;
		width: var(--face-sm);
		height: var(--face-sm);
		flex: none;
		border-radius: var(--radius-full);
		background: var(--card);
		box-shadow: var(--shadow-card);
		color: var(--ink-muted);
	}

	.text {
		display: grid;
		flex: 1;
		min-width: 0;
	}

	.name {
		overflow: hidden;
		text-overflow: ellipsis;
		white-space: nowrap;
		font-weight: var(--weight-label);
	}

	.count {
		color: var(--ink-muted);
		font-size: var(--text-sm);
		line-height: var(--text-sm-lh);
	}

	@media (hover: hover) {
		.option:hover {
			background: var(--card);
			box-shadow: var(--shadow-card);
		}
	}

	.option:focus-visible {
		outline: var(--ring-width) solid var(--accent);
		outline-offset: calc(-1 * var(--ring-width));
	}

	.option:active {
		transform: translateY(var(--press));
	}
</style>
