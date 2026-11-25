<script lang="ts">
	import { onMount } from 'svelte';
	import Alert from '#lib/components/ui/Alert.svelte';
	import Button from '#lib/components/ui/Button.svelte';
	import EmptyState from '#lib/components/ui/EmptyState.svelte';
	import { listNotifications, markRead } from '#lib/notifications/api.ts';
	import { inbox, setUnread } from '#lib/notifications/inbox.svelte.ts';
	import {
		actorName,
		hasUnread,
		markedRead,
		mergeNotifications,
		notificationHref,
		notificationText,
		type Notification
	} from '#lib/notifications/logic.ts';
	import { commentWhen } from '#lib/sharing/logic.ts';

	interface Props {
		/** Leads to the subject; the parent closes the panel and navigates. */
		onNavigate: (href: string) => void;
	}

	let { onNavigate }: Props = $props();

	let items: Notification[] | null = $state(null);
	let cursor: string | null = $state(null);
	let loadingMore = $state(false);
	let marking = $state(false);
	let error: string | null = $state(null);

	const anyUnread = $derived(hasUnread(items) || inbox.unread > 0);

	async function loadPage(after: string | null) {
		const result = await listNotifications(after);
		if (!result.ok) {
			error = result.error;
			return;
		}
		error = null;
		items = mergeNotifications(items ?? [], result.data.items);
		cursor = result.data.next_cursor ?? null;
	}

	async function more() {
		loadingMore = true;
		await loadPage(cursor);
		loadingMore = false;
	}

	async function markAll() {
		marking = true;
		const result = await markRead();
		marking = false;
		if (!result.ok) {
			error = result.error;
			return;
		}
		items = markedRead(items ?? []);
		setUnread(result.data.unread);
	}

	function open(item: Notification) {
		if (!item.read) {
			items = markedRead(items ?? [], [item.id]);
			setUnread(Math.max(0, inbox.unread - 1));
			void markRead([item.id]).then((result) => {
				if (result.ok) setUnread(result.data.unread);
			});
		}
		onNavigate(notificationHref(item));
	}

	onMount(() => void loadPage(null));
</script>

<div class="inbox">
	{#if anyUnread}
		<div class="tools">
			<Button variant="quiet" size="sm" disabled={marking} onclick={markAll}>Mark all read</Button>
		</div>
	{/if}
	{#if error}<Alert>{error}</Alert>{/if}
	{#if items === null}
		{#if !error}<p class="muted" aria-live="polite">Loading…</p>{/if}
	{:else if items.length === 0}
		<EmptyState icon="bell" title="No notifications">
			You will see here when someone shares something with you or comments on it.
		</EmptyState>
	{:else}
		<ul class="list" aria-label="Notifications">
			{#each items as item (item.id)}
				<li data-testid="notification" class:unread={!item.read}>
					<button type="button" class="entry" onclick={() => open(item)}>
						<span class="dot" aria-hidden="true"></span>
						<span class="body">
							<span class="text">
								{#if !item.read}<span class="visually-hidden">Unread: </span>{/if}
								<strong>{actorName(item)}</strong>
								{notificationText(item)}
							</span>
							<span class="when tabular">{commentWhen(item.created_at)}</span>
						</span>
					</button>
				</li>
			{/each}
		</ul>
		{#if cursor !== null}
			<Button variant="quiet" size="sm" disabled={loadingMore} onclick={more}>Show more</Button>
		{/if}
	{/if}
</div>

<style>
	.inbox {
		display: grid;
		gap: var(--space-3);
	}

	.tools {
		display: flex;
		justify-content: flex-end;
	}

	.list {
		display: grid;
		gap: var(--space-1);
		margin: 0;
		padding: 0;
		list-style: none;
	}

	.entry {
		display: flex;
		align-items: flex-start;
		gap: var(--space-2);
		width: 100%;
		min-height: var(--tap);
		padding: var(--space-2) var(--space-3);
		border: 0;
		border-radius: var(--radius-sm);
		background: transparent;
		color: var(--ink);
		font: inherit;
		text-align: left;
		cursor: pointer;
		transition:
			background-color var(--dur-fast) var(--ease),
			transform var(--dur-fast) var(--ease);
	}

	.unread .entry {
		background: var(--tint);
	}

	@media (hover: hover) {
		.entry:hover {
			background: var(--tint-strong);
		}
	}

	.entry:focus-visible {
		outline: var(--ring-width) solid var(--accent);
		outline-offset: var(--ring-offset);
	}

	.entry:active {
		transform: translateY(var(--press));
	}

	/* Same height as the first line, so the dot sits beside it. */
	.dot {
		flex: none;
		width: var(--locator-dot);
		height: var(--text-md-lh);
	}

	.unread .dot {
		background: radial-gradient(circle closest-side, var(--accent) 99%, transparent 100%);
	}

	.body {
		display: grid;
		gap: var(--space-1);
		min-width: 0;
	}

	.text {
		font-size: var(--text-md);
		line-height: var(--text-md-lh);
		overflow-wrap: anywhere;
	}

	.text strong {
		font-weight: var(--weight-strong);
	}

	.when {
		color: var(--ink-muted);
		font-size: var(--text-sm);
		line-height: var(--text-sm-lh);
	}

	@media (forced-colors: active) {
		.unread .dot {
			background: CanvasText;
		}
	}
</style>
