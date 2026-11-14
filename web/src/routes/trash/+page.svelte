<script lang="ts">
	import { onMount, untrack } from 'svelte';
	import ConfirmButton from '#lib/components/ConfirmButton.svelte';
	import Icon from '#lib/components/icons/Icon.svelte';
	import Page from '#lib/components/shell/Page.svelte';
	import Alert from '#lib/components/ui/Alert.svelte';
	import Button from '#lib/components/ui/Button.svelte';
	import EmptyState from '#lib/components/ui/EmptyState.svelte';
	import ListRow from '#lib/components/ui/ListRow.svelte';
	import { deleteForever, emptyTrash, listTrash, restoreEntry, type TrashEntry } from '#lib/files/api.ts';
	import { formatBytes, formatDate } from '#lib/files/format.ts';
	import { latestOnly } from '#lib/files/latest.ts';
	import { nodeIcon } from '#lib/files/nodeIcon.ts';

	let entries: TrashEntry[] = $state([]);
	let loaded = $state(false);
	let error: string | null = $state(null);
	let notice: string | null = $state(null);
	let arriving: Set<string> | null = null;
	// Actions reload the list; a slower earlier reload must not overwrite a newer one.
	const track = latestOnly();

	async function load() {
		const result = await track(listTrash());
		if (result.stale) return;
		if (result.value.ok) {
			entries = result.value.data;
			// Rows present on the first load arrive once; later rows just appear.
			arriving ??= new Set(untrack(() => entries.slice(0, 8).map((entry) => entry.node.id)));
		} else error = result.value.error;
		loaded = true;
	}

	async function act(run: () => Promise<{ ok: boolean; error?: string }>, success: string) {
		error = null;
		notice = null;
		const result = await run();
		if (!result.ok) error = result.error ?? 'Something went wrong.';
		else notice = success;
		await load();
	}

	const arrival = (entry: TrashEntry, index: number) => (arriving?.has(entry.node.id) ? index + 1 : undefined);

	onMount(load);
</script>

<svelte:head><title>Trash · Squirrel</title></svelte:head>

<Page title="Trash">
	{#snippet description()}Items are deleted for good 30 days after they were moved here.{/snippet}
	{#snippet actions()}
		<ConfirmButton
			label="Empty trash"
			confirmLabel="Delete everything forever"
			disabled={entries.length === 0}
			onConfirm={() => act(emptyTrash, 'Trash emptied.')}
		/>
	{/snippet}

	<div class="live" aria-live="polite">{#if notice}<Alert tone="success" role="status">{notice}</Alert>{/if}</div>
	{#if error}<Alert>{error}</Alert>{/if}

	{#if entries.length > 0}
		<ul class="rows" aria-label="Items in the trash">
			{#each entries as entry, index (entry.node.id)}
				<ListRow arrive={arrival(entry, index)}>
					{#snippet icon()}<Icon name={nodeIcon(entry.node)} />{/snippet}
					{#snippet title()}{entry.node.name}{/snippet}
					{#snippet meta()}
						{entry.node.kind === 'folder' ? 'Folder' : formatBytes(entry.node.size)}
						<span aria-hidden="true">·</span>
						Deleted {formatDate(entry.deleted_at)}
					{/snippet}
					{#snippet actions()}
						<Button
							variant="secondary"
							size="sm"
							aria-label={`Restore ${entry.node.name}`}
							onclick={() => act(() => restoreEntry(entry.node.id), `Restored “${entry.node.name}”.`)}
						>
							<Icon name="restore" /><span class="label">Restore</span>
						</Button>
						<ConfirmButton
							label="Delete forever"
							confirmLabel="Confirm"
							icon="trash"
							subject={entry.node.name}
							onConfirm={() => act(() => deleteForever(entry.node.id), 'Deleted forever.')}
						/>
					{/snippet}
				</ListRow>
			{/each}
		</ul>
	{:else if !loaded}
		<p class="muted" aria-live="polite">Loading…</p>
	{:else if !error}
		<EmptyState icon="trash" title="The trash is empty">Deleted files and folders wait here for 30 days.</EmptyState>
	{/if}
</Page>

<style>
	/* No box of its own, so an empty live region adds no gap. */
	.live {
		display: contents;
	}

	.rows {
		display: grid;
		gap: var(--space-2);
		margin: 0;
		padding: 0;
		list-style: none;
	}

	@media (max-width: 30rem) {
		.label {
			display: none;
		}
	}
</style>
