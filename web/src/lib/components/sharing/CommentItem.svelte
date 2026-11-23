<script lang="ts">
	import { tick } from 'svelte';
	import Button from '#lib/components/ui/Button.svelte';
	import IconButton from '#lib/components/ui/IconButton.svelte';
	import TextArea from '#lib/components/ui/TextArea.svelte';
	import type { Comment } from '#lib/sharing/api.ts';
	import { commentProblem, commentWhen, memberName } from '#lib/sharing/logic.ts';

	interface Props {
		comment: Comment;
		/** Resolves an error message, or null once saved. */
		onEdit: (body: string) => Promise<string | null>;
		onDelete: () => void;
	}

	let { comment, onEdit, onDelete }: Props = $props();

	let editing = $state(false);
	let draft = $state('');
	let error: string | null = $state(null);
	let busy = $state(false);
	let field: HTMLTextAreaElement | undefined = $state();
	const author = $derived(memberName(comment.author));

	async function startEditing() {
		draft = comment.body;
		error = null;
		editing = true;
		await tick();
		field?.focus();
	}

	async function save(event: SubmitEvent) {
		event.preventDefault();
		error = commentProblem(draft);
		if (error) return;
		busy = true;
		error = await onEdit(draft.trim());
		busy = false;
		if (!error) editing = false;
	}
</script>

<li class="comment" data-testid="comment">
	<div class="head">
		<span class="author" class:gone={!comment.author}>{author}</span>
		<span class="when tabular">
			<time datetime={comment.created_at}>{commentWhen(comment.created_at)}</time>
			{#if comment.edited_at}<span aria-hidden="true">·</span> edited{/if}
		</span>
		<span class="tools">
			{#if comment.mine && !editing}
				<IconButton icon="pencil" label={`Edit your comment from ${commentWhen(comment.created_at)}`} tooltip="Edit" onclick={startEditing} />
			{/if}
			{#if comment.can_delete}
				<IconButton icon="trash" tone="danger" label={`Delete the comment by ${author}`} tooltip="Delete" onclick={onDelete} />
			{/if}
		</span>
	</div>
	{#if editing}
		<form class="edit" onsubmit={save}>
			<TextArea label="Edit your comment" hideLabel bind:value={draft} bind:element={field} {error} />
			<div class="buttons">
				<Button variant="quiet" size="sm" onclick={() => (editing = false)}>Cancel</Button>
				<Button type="submit" size="sm" disabled={busy}>Save</Button>
			</div>
		</form>
	{:else}
		<p class="body">{comment.body}</p>
	{/if}
</li>

<style>
	.comment {
		display: grid;
		gap: var(--space-1);
		padding: var(--space-3) var(--space-3) var(--space-3) var(--space-4);
		border-radius: var(--radius-card);
		background: var(--card);
		box-shadow: var(--shadow-card);
	}

	.head {
		display: flex;
		flex-wrap: wrap;
		align-items: center;
		gap: 0 var(--space-2);
		min-height: var(--control-sm);
	}

	.author {
		font-weight: var(--weight-label);
		letter-spacing: var(--track-label);
	}

	.gone,
	.when {
		color: var(--ink-muted);
	}

	.when {
		font-size: var(--text-sm);
		line-height: var(--text-sm-lh);
	}

	.tools {
		display: flex;
		margin-left: auto;
	}

	.body {
		white-space: pre-wrap;
		overflow-wrap: anywhere;
	}

	.edit {
		display: grid;
		gap: var(--space-2);
	}

	.buttons {
		display: flex;
		justify-content: flex-end;
		gap: var(--space-2);
	}

	@media (forced-colors: active) {
		.comment {
			border: 1px solid CanvasText;
		}
	}
</style>
