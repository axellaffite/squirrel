<script lang="ts">
	import type { Snippet } from 'svelte';
	import Icon from '#lib/components/icons/Icon.svelte';

	interface Props {
		/** Shown with showModal() when true; set false (or Escape, scrim, close button) to close. */
		open?: boolean;
		title: string;
		/** Called after the dialog closed, whatever closed it. */
		onclose?: () => void;
		children: Snippet;
		/** Footer buttons, laid out on the right (full width on phones). */
		actions?: Snippet;
	}

	let { open = $bindable(false), title, onclose, children, actions }: Props = $props();

	let dialog: HTMLDialogElement | undefined = $state();
	const uid = $props.id();
	const titleId = `dialog-${uid}`;

	$effect(() => {
		if (!dialog) return;
		if (open && !dialog.open) dialog.showModal();
		// close() hands focus back to the element that opened the dialog.
		else if (!open && dialog.open) dialog.close();
	});

	function handleClose() {
		open = false;
		onclose?.();
	}

	/** A click on the dialog element itself (not its content) landed on the scrim. */
	function handleClick(event: MouseEvent) {
		if (event.target === dialog) dialog?.close();
	}
</script>

<dialog bind:this={dialog} aria-labelledby={titleId} onclose={handleClose} onclick={handleClick}>
	<div class="panel">
		<header>
			<h2 id={titleId}>{title}</h2>
			<button type="button" class="close" aria-label="Close" onclick={() => dialog?.close()}>
				<Icon name="x" />
			</button>
		</header>
		<div class="content">{@render children()}</div>
		{#if actions}<footer>{@render actions()}</footer>{/if}
	</div>
</dialog>

<style>
	/* Phones: bottom sheet. ≥ 768 px: centred dialog. Arrival rises, exit reverses faster. */
	dialog {
		width: 100%;
		max-width: 100%;
		max-height: 90dvh;
		margin: auto auto 0;
		padding: 0;
		border: 0;
		border-radius: var(--radius-xl) var(--radius-xl) 0 0;
		background: var(--surface);
		color: var(--ink);
		box-shadow: var(--raise-3);
		opacity: 0;
		transform: translateY(var(--rise-sheet));
		transition:
			opacity var(--dur-fast) var(--ease),
			transform var(--dur-fast) var(--ease),
			overlay var(--dur-fast) allow-discrete,
			display var(--dur-fast) allow-discrete;
	}

	dialog[open] {
		opacity: 1;
		transform: none;
		transition-duration: var(--dur-slow);
	}

	@starting-style {
		dialog[open] {
			opacity: 0;
			transform: translateY(var(--rise-sheet));
		}
	}

	dialog::backdrop {
		background: var(--scrim);
		opacity: 0;
		transition:
			opacity var(--dur-fast) var(--ease),
			overlay var(--dur-fast) allow-discrete,
			display var(--dur-fast) allow-discrete;
	}

	dialog[open]::backdrop {
		opacity: 1;
		transition-duration: var(--dur-slow);
	}

	@starting-style {
		dialog[open]::backdrop {
			opacity: 0;
		}
	}

	@media (min-width: 48rem) {
		dialog {
			width: min(100% - 2 * var(--gutter), var(--dialog-max));
			margin: auto;
			border-radius: var(--radius-xl);
		}
	}

	.panel {
		display: grid;
		gap: var(--space-4);
		padding: var(--space-5) var(--space-4) calc(var(--space-5) + env(safe-area-inset-bottom));
	}

	@media (min-width: 48rem) {
		.panel {
			padding: var(--space-5);
		}
	}

	header {
		display: flex;
		align-items: center;
		justify-content: space-between;
		gap: var(--space-3);
	}

	h2 {
		font-size: var(--text-lg);
		line-height: var(--text-lg-lh);
		font-weight: var(--weight-label);
		overflow-wrap: anywhere;
	}

	.close {
		display: inline-grid;
		place-items: center;
		width: var(--tap);
		height: var(--tap);
		flex: none;
		padding: 0;
		border: 0;
		border-radius: var(--radius-full);
		background: transparent;
		color: var(--ink-muted);
		cursor: pointer;
		transition:
			color var(--dur-fast) var(--ease),
			box-shadow var(--dur-fast) var(--ease);
	}

	@media (hover: hover) {
		.close:hover {
			color: var(--ink);
			box-shadow: var(--raise-1);
		}
	}

	.close:focus-visible {
		outline: var(--ring-width) solid var(--accent);
		outline-offset: var(--ring-offset);
	}

	.close:active {
		box-shadow: var(--inset-1);
		transform: translateY(var(--press));
	}

	.content {
		min-width: 0;
	}

	footer {
		display: flex;
		flex-direction: column-reverse;
		gap: var(--space-3);
	}

	@media (min-width: 48rem) {
		footer {
			flex-direction: row;
			justify-content: flex-end;
		}
	}
</style>
