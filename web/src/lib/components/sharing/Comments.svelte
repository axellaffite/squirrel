<script lang="ts">
	import { onMount, untrack } from 'svelte';
	import ConfirmDialog from '#lib/components/ConfirmDialog.svelte';
	import CommentItem from '#lib/components/sharing/CommentItem.svelte';
	import Alert from '#lib/components/ui/Alert.svelte';
	import Button from '#lib/components/ui/Button.svelte';
	import TextArea from '#lib/components/ui/TextArea.svelte';
	import {
		deleteComment,
		editComment,
		listComments,
		postComment,
		type Comment,
		type Subject
	} from '#lib/sharing/api.ts';
	import { commentProblem, mergeComments, removeById, replaceById } from '#lib/sharing/logic.ts';

	interface Props {
		/** The folder, file or album whose thread this is. */
		subject: Subject;
		/** Its name, for the thread's accessible name. */
		name: string;
	}

	let { subject, name }: Props = $props();

	const target = untrack(() => subject);
	let comments: Comment[] | null = $state(null);
	let cursor: string | null = $state(null);
	let loadingMore = $state(false);
	let error: string | null = $state(null);
	let draft = $state('');
	let draftError: string | null = $state(null);
	let posting = $state(false);
	let deleting: Comment | null = $state(null);

	async function loadPage(after: string | null) {
		const result = await listComments(target, after);
		if (!result.ok) {
			error = result.error;
			return;
		}
		error = null;
		comments = mergeComments(comments ?? [], result.data.items);
		cursor = result.data.next_cursor ?? null;
	}

	async function more() {
		loadingMore = true;
		await loadPage(cursor);
		loadingMore = false;
	}

	async function post(event: SubmitEvent) {
		event.preventDefault();
		draftError = commentProblem(draft);
		if (draftError) return;
		posting = true;
		const result = await postComment(target, draft.trim());
		posting = false;
		if (!result.ok) {
			draftError = result.error;
			return;
		}
		draft = '';
		// Oldest first: a new comment is last once every page is in; otherwise the next page brings it.
		if (cursor === null) comments = [...(comments ?? []), result.data];
	}

	async function edit(comment: Comment, body: string): Promise<string | null> {
		const result = await editComment(comment.id, body);
		if (!result.ok) return result.error;
		comments = replaceById(comments ?? [], result.data);
		return null;
	}

	async function remove(comment: Comment): Promise<string | null> {
		const result = await deleteComment(comment.id);
		if (!result.ok) return result.error;
		comments = removeById(comments ?? [], comment.id);
		return null;
	}

	onMount(() => void loadPage(null));
</script>

<section class="thread" aria-label={`Comments on ${name}`}>
	{#if error}<Alert>{error}</Alert>{/if}
	{#if comments === null}
		{#if !error}<p class="muted" aria-live="polite">Loading…</p>{/if}
	{:else if comments.length === 0}
		<p class="muted">No comments yet.</p>
	{:else}
		<ul class="comments">
			{#each comments as comment (comment.id)}
				<CommentItem {comment} onEdit={(body) => edit(comment, body)} onDelete={() => (deleting = comment)} />
			{/each}
		</ul>
	{/if}
	{#if cursor !== null}
		<Button variant="quiet" size="sm" disabled={loadingMore} onclick={more}>Show more comments</Button>
	{/if}
	<form class="composer" onsubmit={post}>
		<TextArea label="Add a comment" bind:value={draft} error={draftError} placeholder="Write something…" />
		<div class="send">
			<Button type="submit" disabled={posting}>Post comment</Button>
		</div>
	</form>
</section>

{#if deleting}
	{@const comment = deleting}
	<ConfirmDialog title="Delete this comment?" confirmLabel="Delete comment" onConfirm={() => remove(comment)} onClose={() => (deleting = null)}>
		It is removed for everyone.
	</ConfirmDialog>
{/if}

<style>
	.thread,
	.composer {
		display: grid;
		gap: var(--space-3);
	}

	.comments {
		display: grid;
		gap: var(--space-2);
		margin: 0;
		padding: 0;
		list-style: none;
	}

	.send {
		display: flex;
		justify-content: flex-end;
	}
</style>
