<script lang="ts">
	import { goto } from '$app/navigation';
	import Icon from '#lib/components/icons/Icon.svelte';
	import NotificationList from '#lib/components/notifications/NotificationList.svelte';
	import Dialog from '#lib/components/ui/Dialog.svelte';
	import { inbox } from '#lib/notifications/inbox.svelte.ts';
	import { badgeText, bellLabel } from '#lib/notifications/logic.ts';

	interface Props {
		/**
		 * `rail`: on the graphite sidebar (≥ 1024 px), opening a popover beside it. `panel`: in the
		 * phone top row, opening a sheet (a centred dialog on tablets).
		 */
		tone?: 'panel' | 'rail';
	}

	let { tone = 'panel' }: Props = $props();

	let open = $state(false);
	let root: HTMLDivElement | undefined = $state();
	let trigger: HTMLButtonElement | undefined = $state();
	const uid = $props.id();
	const panelId = `notifications-${uid}`;
	const badge = $derived(badgeText(inbox.unread));

	function navigate(href: string) {
		open = false;
		void goto(href);
	}

	function close(returnFocus: boolean) {
		open = false;
		if (returnFocus) trigger?.focus();
	}

	function onKeydown(event: KeyboardEvent) {
		if (tone === 'rail' && open && event.key === 'Escape') close(true);
	}

	function onPointerdown(event: PointerEvent) {
		if (tone === 'rail' && open && root && !root.contains(event.target as Node)) close(false);
	}
</script>

<svelte:window onkeydown={onKeydown} onpointerdown={onPointerdown} />

<div class="bell" class:rail={tone === 'rail'} bind:this={root}>
	<button
		bind:this={trigger}
		type="button"
		class="trigger"
		aria-label={bellLabel(inbox.unread)}
		title="Notifications"
		aria-expanded={open}
		aria-controls={tone === 'rail' ? panelId : undefined}
		onclick={() => (open = !open)}
	>
		<Icon name="bell" />
		{#if badge}<span class="count tabular" aria-hidden="true" data-testid="unread-badge">{badge}</span>{/if}
	</button>

	{#if tone === 'rail' && open}
		<section id={panelId} class="popover" aria-labelledby={`${panelId}-title`}>
			<h2 id={`${panelId}-title`}>Notifications</h2>
			<NotificationList onNavigate={navigate} />
		</section>
	{/if}
</div>

{#if tone === 'panel' && open}
	<Dialog bind:open title="Notifications" onclose={() => (open = false)}>
		<NotificationList onNavigate={navigate} />
	</Dialog>
{/if}

<style>
	.bell {
		position: relative;
	}

	/* A 36 px round button in a tint well, 44 px hit area, like the search button beside it. */
	.trigger {
		position: relative;
		display: inline-grid;
		place-items: center;
		width: var(--control-sm);
		height: var(--control-sm);
		padding: 0;
		border: 0;
		border-radius: var(--radius-control);
		background: var(--tint);
		color: var(--ink);
		cursor: pointer;
		transition:
			background-color var(--dur-fast) var(--ease),
			transform var(--dur-fast) var(--ease);
	}

	.trigger::before {
		content: '';
		position: absolute;
		inset: calc((var(--control-sm) - var(--tap)) / 2);
		border-radius: var(--radius-control);
	}

	.rail .trigger {
		background: var(--rail-well);
		color: var(--rail-ink);
	}

	@media (hover: hover) {
		.trigger:hover {
			background: var(--tint-strong);
		}

		.rail .trigger:hover {
			background: var(--rail-well-hover);
		}
	}

	.trigger:focus-visible {
		outline: var(--ring-width) solid var(--accent);
		outline-offset: var(--ring-offset);
	}

	.rail .trigger:focus-visible {
		outline-color: var(--rail-ink);
	}

	.trigger:active {
		transform: translateY(var(--press));
	}

	.count {
		position: absolute;
		top: calc(-1 * var(--space-1));
		right: calc(-1 * var(--space-1));
		min-width: var(--icon-sm);
		height: var(--icon-sm);
		padding: 0 var(--space-1);
		border-radius: var(--radius-full);
		background: var(--accent);
		color: var(--on-accent);
		font-size: var(--text-xs);
		line-height: var(--icon-sm);
		font-weight: var(--weight-strong);
		text-align: center;
		pointer-events: none;
	}

	/* Drops from the bell over the panel, beside the rail. */
	.popover {
		position: absolute;
		z-index: 20;
		top: calc(100% + var(--space-2));
		left: 0;
		display: grid;
		gap: var(--space-3);
		width: min(var(--dialog-max), calc(100vw - 2 * var(--gutter)));
		max-height: var(--sheet-max);
		overflow-y: auto;
		padding: var(--space-4);
		border-radius: var(--radius-card);
		background: var(--card);
		color: var(--ink);
		box-shadow: var(--shadow-pop);
		transition:
			opacity var(--dur) var(--ease),
			transform var(--dur) var(--ease);
	}

	@starting-style {
		.popover {
			opacity: 0;
			transform: translateY(calc(-1 * var(--rise)));
		}
	}

	h2 {
		font-size: var(--text-lg);
		line-height: var(--text-lg-lh);
		font-weight: var(--weight-heading);
		letter-spacing: var(--track-heading);
	}

	@media (forced-colors: active) {
		.trigger,
		.popover {
			border: 1px solid CanvasText;
		}
	}
</style>
