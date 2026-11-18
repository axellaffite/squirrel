<script lang="ts">
	import FaceAvatar from '#lib/components/photos/FaceAvatar.svelte';
	import Alert from '#lib/components/ui/Alert.svelte';
	import Button from '#lib/components/ui/Button.svelte';
	import Dialog from '#lib/components/ui/Dialog.svelte';
	import type { Person } from '#lib/explore/api.ts';
	import { personLabel } from '#lib/explore/faces.ts';
	import { mergeReason, mergeTarget } from '#lib/explore/people.ts';
	import { photoCount } from '#lib/photos/albums.ts';

	interface Props {
		/** The people to merge (at least two). */
		people: readonly Person[];
		/** Merges into the chosen person; resolves an error, or null once done. */
		onMerge: (keep: Person) => Promise<string | null>;
		onClose: () => void;
	}

	let { people, onMerge, onClose }: Props = $props();

	let dialog: Dialog | undefined = $state();
	let open = $state(true);
	let busy = $state(false);
	let error: string | null = $state(null);
	const suggested = $derived(mergeTarget(people));
	let chosen = $state('');
	const keep = $derived(people.find((person) => person.id === (chosen || suggested?.id)) ?? null);

	async function merge() {
		if (!keep || busy) return;
		busy = true;
		error = await onMerge(keep);
		busy = false;
		if (!error) dialog?.close();
	}
</script>

<Dialog bind:this={dialog} bind:open title={`Merge ${people.length} people`} onclose={onClose}>
	<div class="body">
		<fieldset>
			<legend>Who to keep</legend>
			<ul class="people">
				{#each people as person (person.id)}
					<li>
						<label class="person">
							<input type="radio" name="keep" value={person.id} checked={person.id === keep?.id} onchange={() => (chosen = person.id)} />
							<FaceAvatar face={person.cover} />
							<span class="text">
								<span class="name" class:unnamed={!person.name}>{personLabel(person.name)}</span>
								<span class="count tabular">{photoCount(person.face_count)}</span>
							</span>
						</label>
					</li>
				{/each}
			</ul>
		</fieldset>
		{#if suggested}<p class="muted">{personLabel(suggested.name)} is selected by default. {mergeReason(suggested, people)}</p>{/if}
		<p class="muted">The others' faces all become {personLabel(keep?.name)}'s, and the others are removed.</p>
		{#if error}<Alert>{error}</Alert>{/if}
	</div>
	{#snippet actions()}
		<Button variant="secondary" onclick={() => dialog?.close()}>Cancel</Button>
		<Button variant="danger-solid" onclick={merge} disabled={busy || !keep}>Merge into {personLabel(keep?.name)}</Button>
	{/snippet}
</Dialog>

<style>
	.body {
		display: grid;
		gap: var(--space-3);
	}

	fieldset {
		display: grid;
		gap: var(--space-2);
		margin: 0;
		padding: 0;
		border: 0;
	}

	legend {
		margin-bottom: var(--space-2);
		color: var(--ink-muted);
		font-size: var(--text-sm);
		line-height: var(--text-sm-lh);
		font-weight: var(--weight-label);
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
		min-height: var(--row-min);
		padding: var(--space-2);
		border-radius: var(--radius-card);
		cursor: pointer;
		transition: background-color var(--dur-fast) var(--ease);
	}

	.person:has(input:checked) {
		background: var(--card);
		box-shadow: var(--shadow-knob);
	}

	.person:has(input:focus-visible) {
		outline: var(--ring-width) solid var(--accent);
		outline-offset: calc(-1 * var(--ring-width));
	}

	input {
		accent-color: var(--accent);
		width: var(--icon-sm);
		height: var(--icon-sm);
		margin: 0;
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
</style>
