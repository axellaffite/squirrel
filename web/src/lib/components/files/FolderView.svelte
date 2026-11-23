<script lang="ts">
	import { goto, replaceState } from '$app/navigation';
	import { page } from '$app/state';
	import { onMount } from 'svelte';
	import Breadcrumbs from '#lib/components/files/Breadcrumbs.svelte';
	import DropZone from '#lib/components/files/DropZone.svelte';
	import FileTable from '#lib/components/files/FileTable.svelte';
	import MoveDialog from '#lib/components/files/MoveDialog.svelte';
	import NameDialog from '#lib/components/files/NameDialog.svelte';
	import NodeActions from '#lib/components/files/NodeActions.svelte';
	import SearchField from '#lib/components/files/SearchField.svelte';
	import Preview from '#lib/components/files/Preview.svelte';
	import StorageMeter from '#lib/components/files/StorageMeter.svelte';
	import Icon from '#lib/components/icons/Icon.svelte';
	import Page from '#lib/components/shell/Page.svelte';
	import CommentsDialog from '#lib/components/sharing/CommentsDialog.svelte';
	import ShareDialog from '#lib/components/sharing/ShareDialog.svelte';
	import IconButton from '#lib/components/ui/IconButton.svelte';
	import Alert from '#lib/components/ui/Alert.svelte';
	import Button from '#lib/components/ui/Button.svelte';
	import EmptyState from '#lib/components/ui/EmptyState.svelte';
	import {
		createFolder,
		createTextFile,
		getNode,
		listFolder,
		moveNode,
		renameNode,
		trashNode,
		type FileNode,
		type PathSegment
	} from '#lib/files/api.ts';
	import { latestOnly } from '#lib/files/latest.ts';
	import { mediaStatus } from '#lib/photos/api.ts';
	import { createPoller } from '#lib/photos/status.ts';
	import type { Access, Member } from '#lib/sharing/api.ts';
	import { abilities, memberName, moveRoot } from '#lib/sharing/logic.ts';
	import { enqueue, onUploadsSettled } from '#lib/upload/uploads.svelte.ts';

	let { folderId }: { folderId: string | null } = $props();

	type Dialog =
		| { kind: 'new-folder' }
		| { kind: 'new-text-file' }
		| { kind: 'rename'; node: FileNode }
		| { kind: 'move'; node: FileNode }
		| { kind: 'preview'; node: FileNode }
		| { kind: 'actions'; node: FileNode }
		| { kind: 'share'; node: FileNode }
		| { kind: 'comments'; node: FileNode }
		| null;

	let folder = $state<FileNode | null>(null);
	let path: PathSegment[] = $state([]);
	/** The caller's access to this folder (the top level is always theirs). */
	let access: Access = $state('owner');
	let sharedBy: Member | null = $state(null);
	let items: FileNode[] = $state([]);
	let loaded = $state(false);
	let loading = $state(true);
	let error: string | null = $state(null);
	let dialog = $state<Dialog>(null);
	let storageVersion = $state(0);
	let uploadInput: HTMLInputElement | undefined = $state();
	// Uploads, dialogs and navigation can overlap loads: only the latest one counts.
	const track = latestOnly();

	const can = $derived(abilities(access));
	/** Files can only live inside a folder; the top level is fixed. */
	const canWrite = $derived(folderId !== null && folder !== null && can.edit);
	const title = $derived(folderId === null ? 'My drive' : (folder?.name ?? 'Folder'));
	const selectedId = $derived(dialog && 'node' in dialog ? dialog.node.id : null);

	async function fetchFolder() {
		let details = null;
		if (folderId !== null) {
			details = await getNode(folderId);
			if (!details.ok || details.data.node.kind !== 'folder') return { details, listing: null };
		}
		return { details, listing: await listFolder(folderId) };
	}

	async function load() {
		loading = true;
		const result = await track(fetchFolder());
		if (result.stale) return;
		const { details, listing } = result.value;
		loading = false;
		loaded = true;
		if (details && (!details.ok || details.data.node.kind !== 'folder')) {
			error = details.ok || details.status === 404 ? 'This folder does not exist or was deleted.' : details.error;
			return;
		}
		if (details?.ok) {
			folder = details.data.node;
			path = details.data.path;
			access = details.data.access;
			sharedBy = details.data.shared_by ?? null;
		}
		if (!listing) return;
		if (listing.ok) {
			items = listing.data;
			error = null;
			openRequested();
		} else error = listing.error;
	}

	/** `?open=<id>` (from search results): previews that file once, then leaves the address clean. */
	function openRequested() {
		const id = page.url.searchParams.get('open');
		if (!id) return;
		const url = new URL(page.url.href);
		url.searchParams.delete('open');
		replaceState(url, {});
		const node = items.find((item) => item.id === id);
		if (node) open(node);
		else error = 'That file is no longer in this folder.';
	}

	function changed() {
		storageVersion += 1;
		return load();
	}

	// Once uploaded files are processed, one more refresh shows their thumbnails.
	const poller = createPoller({
		fetch: mediaStatus,
		onCount: () => {},
		onSettled: () => void load(),
		onProgress: () => void load(),
		visible: () => document.visibilityState === 'visible'
	});

	onMount(() => {
		void load();
		// One refresh per settled batch of uploads, not one per file.
		const unsubscribe = onUploadsSettled((parentIds) => {
			if (folderId === null || !parentIds.has(folderId)) return;
			void changed();
			poller.start({ assumePending: true });
		});
		// Coming back to the tab: catch up on what finished meanwhile.
		const visible = () => poller.start();
		document.addEventListener('visibilitychange', visible);
		return () => {
			unsubscribe();
			poller.stop();
			document.removeEventListener('visibilitychange', visible);
		};
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

	/** Creates the file, then opens it in the editor. */
	async function createAndEdit(parent: string, name: string): Promise<string | null> {
		const result = await createTextFile(parent, name);
		if (!result.ok) return result.error;
		void goto(`/edit/${result.data.id}`);
		return null;
	}

	async function outcome(result: { ok: boolean; error?: string }): Promise<string | null> {
		if (!result.ok) return result.error ?? 'Something went wrong.';
		await load();
		return null;
	}
</script>

<svelte:head><title>{folder?.name ?? 'Files'} · Squirrel</title></svelte:head>

<Page {title}>
	{#snippet above()}
		{#if folderId !== null}
			{#if can.owner}<Breadcrumbs {path} current={folder?.name ?? null} />
			{:else}<Breadcrumbs {path} current={folder?.name ?? null} rootLabel="Shared" rootHref="/shared" />{/if}
		{/if}
	{/snippet}
	{#snippet description()}
		{#if sharedBy}Shared by {memberName(sharedBy)}{#if loaded && !error && items.length > 0}<span aria-hidden="true"> · </span>{/if}{/if}
		{#if folderId === null}Open a folder to add files.{:else if loaded && !error && items.length > 0}{items.length}
			{items.length === 1 ? 'item' : 'items'}{/if}
	{/snippet}
	{#snippet actions()}
		<SearchField />
		{#if folder}
			{@const current = folder}
			<span class="subject-tools">
				{#if can.owner}
					<IconButton icon="share" label={`Share ${current.name}`} tooltip="Share" fill="tint" onclick={() => (dialog = { kind: 'share', node: current })} />
				{/if}
				<IconButton icon="message-circle" label={`Comments on ${current.name}`} tooltip="Comments" fill="tint" onclick={() => (dialog = { kind: 'comments', node: current })} />
			</span>
		{/if}
		{#if canWrite}
			<Button variant="secondary" onclick={() => (dialog = { kind: 'new-folder' })}>
				<Icon name="folder-plus" />New folder
			</Button>
			<Button variant="secondary" onclick={() => (dialog = { kind: 'new-text-file' })}>
				<Icon name="file-text" />New text file
			</Button>
			<Button onclick={() => uploadInput?.click()}><Icon name="upload" />Upload files</Button>
			<input bind:this={uploadInput} type="file" multiple hidden onchange={chosen} />
		{/if}
	{/snippet}

	{#if error}<Alert>{error}</Alert>{/if}
	{#if !loaded}
		<p class="muted" aria-live="polite">Loading…</p>
	{:else if !error || items.length > 0}
		<DropZone enabled={canWrite} onFiles={upload}>
			{#if items.length > 0}
				<FileTable
					{items}
					{selectedId}
					canEdit={can.edit}
					onShare={can.owner ? (node) => (dialog = { kind: 'share', node }) : undefined}
					onOpen={open}
					onRename={(node) => (dialog = { kind: 'rename', node })}
					onMove={(node) => (dialog = { kind: 'move', node })}
					onDelete={remove}
					onActions={(node) => (dialog = { kind: 'actions', node })}
				/>
			{:else if !error && !loading}
				<EmptyState icon="upload" title="This folder is empty">
					Drop files here or use “Upload files”.
				</EmptyState>
			{/if}
		</DropZone>
	{/if}

	<StorageMeter refresh={storageVersion} />
</Page>

<!-- Keyed: a dialog opened while the previous one is still fading out gets its own instance. -->
{#key dialog}
	{#if dialog?.kind === 'new-folder' && folderId !== null}
		{@const parent = folderId}
		<NameDialog
			title="New folder"
			submitLabel="Create folder"
			onSubmit={async (name) => outcome(await createFolder(parent, name))}
			onClose={() => (dialog = null)}
		/>
	{:else if dialog?.kind === 'new-text-file' && folderId !== null}
		{@const parent = folderId}
		<NameDialog
			title="New text file"
			initial="Untitled.txt"
			submitLabel="Create and edit"
			onSubmit={async (name) => createAndEdit(parent, name)}
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
			root={folder ? moveRoot(access, path, folder) : null}
			onMove={async (parentId) => outcome(await moveNode(node.id, parentId))}
			onClose={() => (dialog = null)}
		/>
	{:else if dialog?.kind === 'preview'}
		<Preview node={dialog.node} canEdit={can.edit} onClose={() => (dialog = null)} />
	{:else if dialog?.kind === 'share'}
		<ShareDialog subject={{ node_id: dialog.node.id }} name={dialog.node.name} onClose={() => (dialog = null)} />
	{:else if dialog?.kind === 'comments'}
		<CommentsDialog subject={{ node_id: dialog.node.id }} name={dialog.node.name} onClose={() => (dialog = null)} />
	{:else if dialog?.kind === 'actions'}
		<!-- The chosen action runs after onClose cleared `dialog`: use the node the sheet hands back. -->
		<NodeActions
			node={dialog.node}
			canEdit={can.edit}
			onShare={can.owner ? (node) => (dialog = { kind: 'share', node }) : undefined}
			onOpen={open}
			onRename={(node) => (dialog = { kind: 'rename', node })}
			onMove={(node) => (dialog = { kind: 'move', node })}
			onDelete={remove}
			onClose={() => (dialog = null)}
		/>
	{/if}
{/key}

<style>
	.subject-tools {
		display: flex;
		align-items: center;
		gap: var(--space-2);
	}
</style>
