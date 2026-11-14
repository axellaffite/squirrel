<script lang="ts">
	import { goto } from '$app/navigation';
	import { onMount } from 'svelte';
	import Breadcrumbs from '#lib/components/files/Breadcrumbs.svelte';
	import DropZone from '#lib/components/files/DropZone.svelte';
	import FileTable from '#lib/components/files/FileTable.svelte';
	import MoveDialog from '#lib/components/files/MoveDialog.svelte';
	import NameDialog from '#lib/components/files/NameDialog.svelte';
	import Preview from '#lib/components/files/Preview.svelte';
	import StorageMeter from '#lib/components/files/StorageMeter.svelte';
	import {
		createFolder,
		getNode,
		listFolder,
		moveNode,
		renameNode,
		trashNode,
		type FileNode,
		type PathSegment
	} from '#lib/files/api.ts';
	import { enqueue, onUploadFinished } from '#lib/upload/uploads.svelte.ts';

	let { folderId }: { folderId: string | null } = $props();

	type Dialog =
		| { kind: 'new-folder' }
		| { kind: 'rename'; node: FileNode }
		| { kind: 'move'; node: FileNode }
		| { kind: 'preview'; node: FileNode }
		| null;

	let folder: FileNode | null = $state(null);
	let path: PathSegment[] = $state([]);
	let items: FileNode[] = $state([]);
	let loading = $state(true);
	let error: string | null = $state(null);
	let dialog: Dialog = $state(null);
	let storageVersion = $state(0);

	/** Files can only live inside a folder; the top level is fixed. */
	const canWrite = $derived(folderId !== null && folder !== null);

	async function load() {
		loading = true;
		error = null;
		if (folderId !== null) {
			const details = await getNode(folderId);
			if (!details.ok || details.data.node.kind !== 'folder') {
				error = details.ok || details.status === 404 ? 'This folder does not exist or was deleted.' : details.error;
				loading = false;
				return;
			}
			folder = details.data.node;
			path = details.data.path;
		}
		const listing = await listFolder(folderId);
		if (listing.ok) items = listing.data;
		else error = listing.error;
		loading = false;
	}

	function changed() {
		storageVersion += 1;
		return load();
	}

	onMount(() => {
		void load();
		return onUploadFinished((parentId) => {
			if (parentId === folderId) void changed();
		});
	});

	function open(node: FileNode) {
		if (node.kind === 'folder') void goto(`/files/${node.id}`);
		else dialog = { kind: 'preview', node };
	}

	async function remove(node: FileNode) {
		const result = await trashNode(node.id);
		if (!result.ok) error = result.error;
		else await changed();
	}

	function upload(files: File[]) {
		if (folderId !== null) enqueue(files, folderId);
	}

	function chosen(event: Event) {
		const input = event.currentTarget as HTMLInputElement;
		upload(Array.from(input.files ?? []));
		input.value = '';
	}

	async function outcome(result: { ok: boolean; error?: string }): Promise<string | null> {
		if (!result.ok) return result.error ?? 'Something went wrong.';
		await load();
		return null;
	}
</script>

<svelte:head><title>{folder?.name ?? 'Files'} · Squirrel</title></svelte:head>

<main class="page files">
	<header class="files-header">
		<Breadcrumbs {path} current={folder?.name ?? null} />
		<div class="files-actions">
			{#if canWrite}
				<button onclick={() => (dialog = { kind: 'new-folder' })}>New folder</button>
				<label class="button">
					Upload files
					<input type="file" multiple class="visually-hidden" onchange={chosen} />
				</label>
			{/if}
			<a href="/trash">Trash</a>
		</div>
	</header>

	{#if error}<p class="error" role="alert">{error}</p>{/if}
	{#if loading && items.length === 0}
		<p class="muted">Loading…</p>
	{:else}
		<DropZone enabled={canWrite} onFiles={upload}>
			<FileTable
				{items}
				onOpen={open}
				onRename={(node) => (dialog = { kind: 'rename', node })}
				onMove={(node) => (dialog = { kind: 'move', node })}
				onDelete={remove}
			/>
		</DropZone>
	{/if}
	{#if !canWrite && folderId === null}
		<p class="muted">Open a folder to add files.</p>
	{/if}

	<StorageMeter refresh={storageVersion} />
</main>

{#if dialog?.kind === 'new-folder' && folderId !== null}
	{@const parent = folderId}
	<NameDialog
		title="New folder"
		submitLabel="Create folder"
		onSubmit={async (name) => outcome(await createFolder(parent, name))}
		onClose={() => (dialog = null)}
	/>
{:else if dialog?.kind === 'rename'}
	{@const node = dialog.node}
	<NameDialog
		title={`Rename “${node.name}”`}
		initial={node.name}
		submitLabel="Save"
		onSubmit={async (name) => outcome(await renameNode(node.id, name))}
		onClose={() => (dialog = null)}
	/>
{:else if dialog?.kind === 'move'}
	{@const node = dialog.node}
	<MoveDialog
		{node}
		onMove={async (parentId) => outcome(await moveNode(node.id, parentId))}
		onClose={() => (dialog = null)}
	/>
{:else if dialog?.kind === 'preview'}
	<Preview node={dialog.node} onClose={() => (dialog = null)} />
{/if}
