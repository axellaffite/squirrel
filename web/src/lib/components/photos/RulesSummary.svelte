<script lang="ts">
	import { listPeople, personChipLabel, type Person } from '#lib/explore/api.ts';
	import { formFromRules, ruleParts } from '#lib/photos/albumRules.ts';
	import type { AlbumRules } from '#lib/photos/api.ts';

	interface Props {
		rules: AlbumRules;
	}

	let { rules }: Props = $props();

	let people: Person[] = $state.raw([]);
	const form = $derived(formFromRules(rules));
	const parts = $derived(ruleParts(form, (id) => personChipLabel(people.find((p) => p.id === id))));

	// People are named only once their list is in (on failure they stay "Unnamed").
	$effect(() => {
		if (form.people.length === 0) return;
		let current = true;
		void listPeople(true).then((result) => {
			if (current && result.ok) people = result.data;
		});
		return () => {
			current = false;
		};
	});
</script>

<!-- "Alice and Bob · Beach · Paris": what a dynamic album holds, for its owner. -->
{#if parts.length > 0}<span class="summary">{parts.join(' · ')}</span>{/if}

<style>
	.summary {
		display: block;
	}
</style>
