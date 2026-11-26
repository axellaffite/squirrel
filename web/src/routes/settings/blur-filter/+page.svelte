<script lang="ts">
	import { onMount } from 'svelte';
	import RulesEditor from '#lib/components/photos/RulesEditor.svelte';
	import RulesPreview from '#lib/components/photos/RulesPreview.svelte';
	import Page from '#lib/components/shell/Page.svelte';
	import Alert from '#lib/components/ui/Alert.svelte';
	import Button from '#lib/components/ui/Button.svelte';
	import Toggle from '#lib/components/ui/Toggle.svelte';
	import { listPeople, type Person } from '#lib/explore/api.ts';
	import { emptyRulesForm, formFromRules, rulesBody, rulesProblem, type RulesForm } from '#lib/photos/albumRules.ts';
	import { blurred, getBlurFilter, saveBlurFilter, type BlurFilter } from '#lib/photos/blurred.svelte.ts';

	/** The filter as saved on the server; null while loading. */
	let saved: BlurFilter | null = $state.raw(null);
	let enabled = $state(true);
	let form: RulesForm = $state.raw(emptyRulesForm());
	let people: Person[] = $state.raw([]);
	let error: string | null = $state(null);
	let notice: string | null = $state(null);
	let busy = $state(false);

	function show(filter: BlurFilter) {
		saved = filter;
		enabled = filter.rules ? filter.enabled : true;
		form = formFromRules(filter.rules);
	}

	onMount(async () => {
		const [filter, found] = await Promise.all([getBlurFilter(), listPeople(true)]);
		if (found.ok) people = found.data;
		if (filter.ok) show(filter.data);
		else error = filter.error;
	});

	async function store(next: BlurFilter, done: string) {
		busy = true;
		notice = null;
		try {
			const result = await saveBlurFilter(next);
			if (!result.ok) {
				error = result.error;
				return;
			}
			error = null;
			show(result.data);
			notice = done;
			await blurred.refresh(true);
		} finally {
			busy = false;
		}
	}

	function submit(event: SubmitEvent) {
		event.preventDefault();
		error = rulesProblem(form);
		if (error) return;
		void store({ enabled, rules: rulesBody(form) }, enabled ? 'Blur filter saved.' : 'Blur filter saved, turned off.');
	}
</script>

<svelte:head><title>Blur filter · Squirrel</title></svelte:head>

<Page title="Blur filter" back={{ href: '/settings', label: 'Settings' }}>
	{#snippet description()}Keep some photos blurred when your drive is open in front of others.{/snippet}

	{#if error}<Alert>{error}</Alert>{/if}
	<p class="visually-hidden" aria-live="polite">{notice ?? ''}</p>
	{#if notice}<Alert tone="success" role="note">{notice}</Alert>{/if}

	{#if saved === null}
		{#if !error}<p class="muted" aria-live="polite">Loading…</p>{/if}
	{:else}
		<section class="well" aria-label="Blur filter">
			<Toggle label="Blur photos matching this filter" bind:checked={enabled} disabled={busy} />
			<p class="muted">
				Matching photos show blurred everywhere: the timeline, albums and their covers, the files. Open one and
				choose Show to see it, or show them all at once from the timeline until you leave.
			</p>
			<!-- The editor stays outside the form: Enter in a picker's search field must not submit it. -->
			<RulesEditor {form} {people} onChange={(next) => (form = next)} />
			<RulesPreview {form} thumbs={false} />
			<form class="actions" onsubmit={submit} novalidate>
				<Button type="submit" disabled={busy}>Save</Button>
				{#if saved.rules}
					<Button
						variant="secondary"
						disabled={busy}
						onclick={() => void store({ enabled: false, rules: null }, 'Blur filter removed: nothing is blurred any more.')}
					>
						Remove filter
					</Button>
				{/if}
			</form>
		</section>
	{/if}
</Page>

<style>
	.well {
		display: grid;
		gap: var(--space-4);
		max-width: var(--measure);
		padding: var(--well-pad);
		border-radius: var(--radius-well);
		background: var(--tint);
	}

	.actions {
		display: flex;
		flex-wrap: wrap;
		gap: var(--space-2);
	}
</style>
