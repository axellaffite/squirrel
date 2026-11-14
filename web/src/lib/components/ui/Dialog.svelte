<script lang="ts">
	import type { Snippet } from 'svelte';
	import IconButton from '#lib/components/ui/IconButton.svelte';

	interface Props {
		/** Shown with showModal() when true; set false (or Escape, scrim, close button) to close. */
		open?: boolean;
		title: string;
		/** Called after the dialog closed, whatever closed it. */
		onclose?: () => void;
		children: Snippet;
		/** Footer buttons, laid out on the right (full width on phones). */
		actions?: Snippet;
		/** Extra buttons next to the close button in the header. */
		tools?: Snippet;
		/** `wide` fits previews: up to the content width on larger screens. */
		size?: 'default' | 'wide';
	}

	let { open = $bindable(false), title, onclose, children, actions, tools, size = 'default' }: Props = $props();

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

	/** Closes through the native dialog, so focus returns and `onclose` runs once. */
	export function close() {
		dialog?.close();
	}

	/** A click on the dialog element itself (not its content) landed on the scrim. */
	function handleClick(event: MouseEvent) {
		if (event.target === dialog) dialog?.close();
	}
</script>

<dialog
	bind:this={dialog}
	class:wide={size === 'wide'}
	aria-labelledby={titleId}
	onclose={handleClose}
	onclick={handleClick}
>
	<div class="panel">
		<header>
			<h2 id={titleId}>{title}</h2>
			<div class="tools">
				{@render tools?.()}
				<IconButton icon="x" label="Close" fill="tint" onclick={() => dialog?.close()} />
			</div>
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
		border-radius: var(--radius-panel) var(--radius-panel) 0 0;
		background: var(--card);
		color: var(--ink);
		box-shadow: var(--shadow-pop);
		opacity: 0;
		transform: translateY(var(--rise-sheet));
		/* Keep the sheet on its own layer so the rise does not repaint its shadow each frame. */
		will-change: transform, opacity;
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
			border-radius: var(--radius-panel);
		}

		dialog.wide {
			width: min(100% - 2 * var(--gutter), var(--content-max));
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
		min-width: 0;
		font-size: var(--text-xl);
		line-height: var(--text-xl-lh);
		overflow-wrap: anywhere;
	}

	.tools {
		display: flex;
		align-items: center;
		gap: var(--space-2);
		flex: none;
	}

	.content {
		min-width: 0;
	}

	footer {
		display: flex;
		flex-direction: column-reverse;
		gap: var(--space-2);
	}

	@media (min-width: 48rem) {
		footer {
			flex-direction: row;
			justify-content: flex-end;
		}
	}

	@media (forced-colors: active) {
		dialog {
			border: 1px solid CanvasText;
		}
	}
</style>
