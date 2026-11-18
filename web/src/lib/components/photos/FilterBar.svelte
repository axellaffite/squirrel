<script lang="ts">
	import { tick } from 'svelte';
	import Icon from '#lib/components/icons/Icon.svelte';
	import type { IconName } from '#lib/components/icons/paths.ts';
	import FaceAvatar from '#lib/components/photos/FaceAvatar.svelte';
	import FilterPicker from '#lib/components/photos/FilterPicker.svelte';
	import Button from '#lib/components/ui/Button.svelte';
	import { getPerson, personChipLabel, type Person } from '#lib/explore/api.ts';
	import { chips, type ChipKind, type PhotoFilter } from '#lib/explore/filters.ts';

	interface Props {
		filter: PhotoFilter;
		/** The caller's people, to name and show a person filter. */
		people: readonly Person[];
		onChange: (filter: PhotoFilter) => void;
	}

	let { filter, people, onChange }: Props = $props();

	let picking = $state(false);
	let bar: HTMLDivElement | undefined = $state();

	/**
	 * Removes a chip, then keeps the keyboard in place: on the next chip's clear button, else
	 * the previous one's, else the Filter button.
	 */
	async function remove(index: number, without: PhotoFilter) {
		onChange(without);
		await tick();
		const clears = bar?.querySelectorAll<HTMLButtonElement>('button.clear') ?? [];
		const next = clears[index] ?? clears[index - 1] ?? bar?.querySelector<HTMLButtonElement>('button.open');
		next?.focus();
	}

	const ICONS: Record<Exclude<ChipKind, 'person'>, IconName> = {
		tag: 'tag',
		place: 'map-pin',
		camera: 'camera',
		kind: 'image'
	};

	/** A filtered person missing from `people` (hidden people are left out of lists). */
	let fetched: Person | null = $state(null);
	const person = (id: string) => people.find((p) => p.id === id) ?? (fetched?.id === id ? fetched : undefined);
	const list = $derived(chips(filter, (id) => personChipLabel(person(id))));

	$effect(() => {
		const id = filter.person;
		if (!id || people.some((p) => p.id === id) || fetched?.id === id) return;
		let current = true;
		void getPerson(id).then((result) => {
			if (current && result.ok) fetched = result.data;
		});
		return () => {
			current = false;
		};
	});
</script>

<!-- MASTER "Filter chip": active chips are chosen segments with a clear button. -->
<div class="bar" role="group" aria-label="Filters" bind:this={bar}>
	<Button variant="secondary" size="sm" class="open" onclick={() => (picking = true)} aria-haspopup="dialog">
		<Icon name="sliders-horizontal" />Filter
	</Button>
	{#if list.length > 0}
		<ul class="chips">
			{#each list as chip, index (chip.kind)}
				<li class="chip">
					{#if chip.kind === 'person'}
						<FaceAvatar face={filter.person ? person(filter.person)?.cover : null} />
					{:else}
						<Icon name={ICONS[chip.kind]} />
					{/if}
					<span class="label">{chip.label}</span>
					<button type="button" class="clear" aria-label={`Remove filter ${chip.label}`} title="Remove" onclick={() => remove(index, chip.without)}>
						<Icon name="x" />
					</button>
				</li>
			{/each}
		</ul>
		<Button variant="quiet" size="sm" onclick={() => onChange({})}>Clear filters</Button>
	{/if}
</div>

{#if picking}
	<FilterPicker {filter} {people} onPick={(next) => onChange(next)} onClose={() => (picking = false)} />
{/if}

<style>
	.bar {
		display: flex;
		flex-wrap: wrap;
		align-items: center;
		gap: var(--space-2);
		min-width: 0;
	}

	.chips {
		display: flex;
		flex-wrap: wrap;
		gap: var(--space-2);
		min-width: 0;
		margin: 0;
		padding: 0;
		list-style: none;
	}

	.chip {
		display: inline-flex;
		align-items: center;
		gap: var(--space-2);
		min-height: var(--chip-h);
		max-width: 100%;
		padding: 0 var(--space-1) 0 var(--space-3);
		border-radius: var(--radius-full);
		background: var(--card);
		box-shadow: var(--shadow-knob);
		color: var(--ink);
		font-size: var(--text-sm);
		line-height: var(--text-sm-lh);
		font-weight: var(--weight-label);
		letter-spacing: var(--track-label);
	}

	/* A person chip leads with its avatar, flush with the pill's edge. */
	.chip:has(:global(.avatar)) {
		padding-left: var(--space-1);
	}

	.chip :global(.avatar) {
		width: calc(var(--chip-h) - 2 * var(--space-1));
		height: calc(var(--chip-h) - 2 * var(--space-1));
	}

	.label {
		overflow: hidden;
		text-overflow: ellipsis;
		white-space: nowrap;
	}

	/* The chip's clear button: a 20 px x in a round hit area of 44 px. */
	.clear {
		position: relative;
		display: inline-grid;
		place-items: center;
		width: var(--chip-h);
		height: calc(var(--chip-h) - 2 * var(--space-1));
		padding: 0;
		border: 0;
		border-radius: var(--radius-full);
		background: transparent;
		color: var(--ink-muted);
		cursor: pointer;
		transition:
			background-color var(--dur-fast) var(--ease),
			color var(--dur-fast) var(--ease),
			transform var(--dur-fast) var(--ease);
	}

	.clear :global(.icon) {
		width: var(--chip-clear);
		height: var(--chip-clear);
	}

	.clear::before {
		content: '';
		position: absolute;
		inset: calc((var(--chip-h) - var(--tap)) / 2);
	}

	@media (hover: hover) {
		.clear:hover {
			background: var(--tint-strong);
			color: var(--ink);
		}
	}

	.clear:focus-visible {
		outline: var(--ring-width) solid var(--accent);
		outline-offset: var(--ring-offset);
	}

	.clear:active {
		transform: translateY(var(--press));
	}

	@media (forced-colors: active) {
		.chip {
			border: 1px solid CanvasText;
		}
	}
</style>
