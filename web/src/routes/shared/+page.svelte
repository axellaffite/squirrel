<script lang="ts">
	import { goto, replaceState } from '$app/navigation';
	import { page } from '$app/state';
	import { onMount, untrack } from 'svelte';
	import ConfirmDialog from '#lib/components/ConfirmDialog.svelte';
	import Preview from '#lib/components/files/Preview.svelte';
	import Page from '#lib/components/shell/Page.svelte';
	import ShareDialog from '#lib/components/sharing/ShareDialog.svelte';
	import SharedByMeSection from '#lib/components/sharing/SharedByMeSection.svelte';
	import SharedWithMeSection from '#lib/components/sharing/SharedWithMeSection.svelte';
	import Icon from '#lib/components/icons/Icon.svelte';
	import Button from '#lib/components/ui/Button.svelte';
	import { getNode, type FileNode } from '#lib/files/api.ts';
	import { openRequest, withoutOpenRequest } from '#lib/notifications/logic.ts';
	import { deleteShare, sharedByMe, sharedWithMe, type SharedByMe, type SharedItem, type Subject } from '#lib/sharing/api.ts';
	import { memberName } from '#lib/sharing/logic.ts';

	interface Managed {
		subject: Subject;
		name: string;
	}

	let withMe: SharedItem[] | null = $state(null);
	let withMeError: string | null = $state(null);
	let byMe: SharedByMe[] | null = $state(null);
	let byMeError: string | null = $state(null);
	let leaving: SharedItem | null = $state(null);
	let managing: Managed | null = $state(null);
	let previewing: FileNode | null = $state(null);
	let previewComments = $state(false);

	/** Files of one's own, or shared for writing (directly or through a folder above), open in the editor as "Edit". */
	const canEdit = (node: FileNode) =>
		byMe?.some((item) => item.node?.id === node.id) || withMe?.find((item) => item.node?.id === node.id)?.permission === 'write';
	const subjectName = (item: SharedItem) => item.node?.name ?? item.album?.name ?? '';

	async function loadWithMe() {
		const result = await sharedWithMe();
		if (result.ok) {
			withMe = result.data;
			withMeError = null;
		} else withMeError = result.error;
	}

	async function loadByMe() {
		const result = await sharedByMe();
		if (result.ok) {
			byMe = result.data;
			byMeError = null;
		} else byMeError = result.error;
	}

	function open(node: FileNode) {
		if (node.kind === 'folder') void goto(`/files/${node.id}`);
		else {
			previewComments = false;
			previewing = node;
		}
	}

	async function leave(item: SharedItem): Promise<string | null> {
		const result = await deleteShare(item.share_id);
		if (!result.ok) return result.error;
		withMe = (withMe ?? []).filter((entry) => entry.share_id !== item.share_id);
		return null;
	}

	function manage(item: SharedByMe) {
		if (item.node) managing = { subject: { node_id: item.node.id }, name: item.node.name };
		else if (item.album) managing = { subject: { album_id: item.album.id }, name: item.album.name };
	}

	/**
	 * `?open=<id>` (from a notification): previews a file shared on its own, or one seen through a
	 * shared album, once; `?comments=1` with its thread shown.
	 */
	async function openRequested() {
		const request = openRequest(page.url);
		if (!request?.open) return;
		replaceState(withoutOpenRequest(page.url), {});
		const shared = withMe?.find((item) => item.node?.id === request.open)?.node;
		const node = shared ?? (await fileById(request.open));
		if (!node) return;
		previewComments = request.comments;
		previewing = node;
	}

	async function fileById(id: string): Promise<FileNode | null> {
		const result = await getNode(id);
		if (result.ok && result.data.node.kind === 'file') return result.data.node;
		withMeError = result.ok ? null : result.error;
		return null;
	}

	$effect(() => {
		void page.url.search;
		if (withMe !== null) untrack(() => void openRequested());
	});

	onMount(() => {
		void loadWithMe();
		void loadByMe();
	});
</script>

<svelte:head><title>Shared · Squirrel</title></svelte:head>

<Page title="Shared">
	{#snippet description()}What other people shared with you, and what you shared with them.{/snippet}
	{#snippet actions()}<Button variant="secondary" href="/settings/friends"><Icon name="users" />Friends</Button>{/snippet}

	<SharedWithMeSection items={withMe} error={withMeError} onOpen={open} onLeave={(item) => (leaving = item)} />
	<SharedByMeSection items={byMe} error={byMeError} onOpen={open} onManage={manage} />
</Page>

{#if leaving}
	{@const item = leaving}
	<ConfirmDialog title={`Leave “${subjectName(item)}”?`} confirmLabel="Leave" onConfirm={() => leave(item)} onClose={() => (leaving = null)}>
		You will no longer see it. {memberName(item.owner)} can share it with you again.
	</ConfirmDialog>
{/if}

{#if managing}
	<ShareDialog
		subject={managing.subject}
		name={managing.name}
		onClose={() => {
			managing = null;
			void loadByMe();
		}}
	/>
{/if}

{#if previewing}
	{#key previewing}
		<Preview
			node={previewing}
			canEdit={canEdit(previewing)}
			comments={previewComments}
			onClose={() => {
				previewing = null;
				previewComments = false;
			}}
		/>
	{/key}
{/if}
