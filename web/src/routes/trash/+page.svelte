<script lang="ts">
	import { onMount } from 'svelte';
	import ConfirmButton from '#lib/components/ConfirmButton.svelte';
	import { deleteForever, emptyTrash, listTrash, restoreEntry, type TrashEntry } from '#lib/files/api.ts';
	import { formatBytes, formatDate } from '#lib/files/format.ts';

	let entries: TrashEntry[] = $state([]);
	let loaded = $state(false);
	let error: string | null = $state(null);
	let notice: string | null = $state(null);

	async function load() {
		const result = await listTrash();
		if (result.ok) entries = result.data;
		else error = result.error;
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

	onMount(load);
</script>

<svelte:head><title>Trash · Squirrel</title></svelte:head>

<main class="page">
	<header class="files-header">
		<h1>Trash</h1>
		<div class="files-actions">
			<a href="/files">Back to files</a>
			<ConfirmButton
				label="Empty trash"
				confirmLabel="Delete everything forever"
				disabled={entries.length === 0}
				onConfirm={() => act(emptyTrash, 'Trash emptied.')}
			/>
		</div>
	</header>
	<p class="muted">Items are deleted for good 30 days after they were moved here.</p>
	<div aria-live="polite">{#if notice}<p>{notice}</p>{/if}</div>
	{#if error}<p class="error" role="alert">{error}</p>{/if}

	<table>
		<thead><tr><th>Name</th><th>Size</th><th>Deleted</th><th><span class="visually-hidden">Actions</span></th></tr></thead>
		<tbody>
			{#each entries as entry (entry.node.id)}
				<tr>
					<td>{entry.node.kind === 'folder' ? '📁' : '📄'} {entry.node.name}</td>
					<td>{entry.node.kind === 'folder' ? '—' : formatBytes(entry.node.size)}</td>
					<td>{formatDate(entry.deleted_at)}</td>
					<td class="row-actions">
						<button class="link" onclick={() => act(() => restoreEntry(entry.node.id), `Restored “${entry.node.name}”.`)}
							>Restore</button
						>
						<ConfirmButton
							label="Delete forever"
							confirmLabel="Confirm"
							onConfirm={() => act(() => deleteForever(entry.node.id), 'Deleted forever.')}
						/>
					</td>
				</tr>
			{:else}
				<tr><td colspan="4" class="muted">{loaded ? 'The trash is empty.' : 'Loading…'}</td></tr>
			{/each}
		</tbody>
	</table>
</main>
