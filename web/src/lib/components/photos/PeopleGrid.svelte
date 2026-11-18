<script lang="ts">
	import FaceAvatar from '#lib/components/photos/FaceAvatar.svelte';
	import type { Person } from '#lib/explore/api.ts';
	import { personLabel } from '#lib/explore/faces.ts';
	import { photoCount } from '#lib/photos/albums.ts';

	interface Props {
		people: readonly Person[];
		/** Accessible name of the list. */
		label?: string;
	}

	let { people, label = 'People' }: Props = $props();
</script>

<!-- MASTER "People grid": auto-fill of --person-min, 3 columns on phones; avatar, name, count. -->
<ul class="grid" aria-label={label}>
	{#each people as person (person.id)}
		<li>
			<a class="person" href={`/explore/people/${person.id}`}>
				<FaceAvatar face={person.cover} size="lg" />
				<span class="name" class:unnamed={!person.name}>{personLabel(person.name)}</span>
				<span class="count tabular">{photoCount(person.face_count)}</span>
			</a>
		</li>
	{/each}
</ul>

<style>
	.grid {
		display: grid;
		grid-template-columns: repeat(3, minmax(0, 1fr));
		gap: var(--space-3);
		margin: 0;
		padding: 0;
		list-style: none;
	}

	@media (min-width: 48rem) {
		.grid {
			grid-template-columns: repeat(auto-fill, minmax(var(--person-min), 1fr));
			gap: var(--space-4);
		}
	}

	.person {
		display: grid;
		justify-items: center;
		gap: var(--space-1);
		padding: var(--space-2);
		border-radius: var(--radius-card);
		color: var(--ink);
		text-align: center;
		text-decoration: none;
		transition:
			background-color var(--dur-fast) var(--ease),
			transform var(--dur-fast) var(--ease);
	}

	.person :global(.avatar) {
		margin-bottom: var(--space-1);
	}

	.name {
		max-width: 100%;
		overflow: hidden;
		text-overflow: ellipsis;
		white-space: nowrap;
		font-weight: var(--weight-label);
		letter-spacing: var(--track-label);
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
			background: var(--tint-strong);
		}
	}

	.person:focus-visible {
		outline: var(--ring-width) solid var(--accent);
		outline-offset: var(--ring-offset);
	}

	.person:active {
		transform: translateY(var(--press));
	}
</style>
