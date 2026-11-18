<script lang="ts" module>
	import type { Person } from '#lib/explore/api.ts';

	export type PersonChoice = { person: Person } | { newPerson: true };
</script>

<script lang="ts">
	import { onMount } from 'svelte';
	import Icon from '#lib/components/icons/Icon.svelte';
	import FaceAvatar from '#lib/components/photos/FaceAvatar.svelte';
	import Alert from '#lib/components/ui/Alert.svelte';
	import Dialog from '#lib/components/ui/Dialog.svelte';
	import TextField from '#lib/components/ui/TextField.svelte';
	import { listPeople } from '#lib/explore/api.ts';
	import { personLabel } from '#lib/explore/faces.ts';
	import { photoCount } from '#lib/photos/albums.ts';

	interface Props {
		title: string;
		/** People not to offer (e.g. the person the faces come from). */
		exclude?: readonly string[];
		/** Offers "New person" first. */
		allowNew?: boolean;
		onPick: (choice: PersonChoice) => void;
		onClose: () => void;
	}

	let { title, exclude = [], allowNew = false, onPick, onClose }: Props = $props();

	let dialog: Dialog | undefined = $state();
	let open = $state(true);
	let people = $state<Person[] | null>(null);
	let error: string | null = $state(null);
	let query = $state('');

	const shown = $derived.by(() => {
		const words = query.trim().toLocaleLowerCase();
		return (people ?? []).filter(
			(person) => !exclude.includes(person.id) && (!words || personLabel(person.name).toLocaleLowerCase().includes(words))
		);
	});

	onMount(async () => {
		const result = await listPeople(true);
		if (result.ok) people = result.data;
		else error = result.error;
	});

	function pick(choice: PersonChoice) {
		onPick(choice);
		dialog?.close();
	}
</script>

<Dialog bind:this={dialog} bind:open {title} onclose={onClose}>
	<div class="body">
		<TextField label="Find a person by name" bind:value={query} type="search" autocomplete="off" />
		{#if error}<Alert>{error}</Alert>{/if}
		<ul class="people">
			{#if allowNew}
				<li>
					<button type="button" class="person" onclick={() => pick({ newPerson: true })}>
						<span class="well"><Icon name="plus" /></span>
						<span class="text"><span class="name">New person</span></span>
					</button>
				</li>
			{/if}
			{#each shown as person (person.id)}
				<li>
					<button type="button" class="person" onclick={() => pick({ person })}>
						<FaceAvatar face={person.cover} />
						<span class="text">
							<span class="name" class:unnamed={!person.name}>{personLabel(person.name)}</span>
							<span class="count tabular">{photoCount(person.face_count)}{person.hidden ? ' · hidden' : ''}</span>
						</span>
					</button>
				</li>
			{/each}
		</ul>
		{#if people === null && !error}<p class="muted" aria-live="polite">Loading…</p>{/if}
		{#if people !== null && shown.length === 0 && query.trim()}<p class="muted">Nobody is called “{query.trim()}”.</p>{/if}
	</div>
</Dialog>

<style>
	.body {
		display: grid;
		gap: var(--space-3);
	}

	.people {
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

	.person {
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

	.well {
		display: grid;
		place-items: center;
		width: var(--face-sm);
		height: var(--face-sm);
		flex: none;
		border-radius: var(--radius-full);
		background: var(--card);
		box-shadow: var(--shadow-card);
	}

	.text {
		display: grid;
		min-width: 0;
	}

	.name {
		overflow: hidden;
		text-overflow: ellipsis;
		white-space: nowrap;
		font-weight: var(--weight-label);
	}

	.unnamed,
	.count {
		color: var(--ink-muted);
	}

	.count {
		font-size: var(--text-sm);
		line-height: var(--text-sm-lh);
	}

	@media (hover: hover) {
		.person:hover {
			background: var(--card);
			box-shadow: var(--shadow-card);
		}
	}

	.person:focus-visible {
		outline: var(--ring-width) solid var(--accent);
		outline-offset: calc(-1 * var(--ring-width));
	}

	.person:active {
		transform: translateY(var(--press));
	}
</style>
