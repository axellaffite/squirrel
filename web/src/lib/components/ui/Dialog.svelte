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

	/**
	 * Parents unmount a dialog as soon as it closes. This keeps the closed element in the DOM for
	 * `--dur-fast` (0 with reduced motion) so its CSS exit transition can play. A dialog removed
	 * while still open (e.g. on navigation) goes at once, as before.
	 */
	function linger(node: HTMLDialogElement) {
		if (node.open) return { duration: 0 };
		// The build may minify the token from `160ms` to `.16s`.
		const token = getComputedStyle(node).getPropertyValue('--dur-fast').trim();
		const value = parseFloat(token) || 0;
		return { duration: token.endsWith('ms') ? value : value * 1000 };
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
	inert={!open}
	onclose={handleClose}
	onclick={handleClick}
	out:linger|global
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
	/* Fixed like a modal dialog even while closing, where it may already have left the top layer. */
	dialog {
		position: fixed;
		inset: 0;
		z-index: 30;
		width: 100%;
		max-width: 100%;
		max-height: var(--sheet-max);
		margin: auto auto 0;
		padding: 0;
		border: 1px solid var(--hair);
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

	/* While it fades out, a closing dialog takes no clicks. */
	dialog:not([open]) {
		pointer-events: none;
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

	/* The page behind is blurred (static radius, never animated) under the scrim. */
	dialog::backdrop {
		background: var(--scrim);
		-webkit-backdrop-filter: blur(var(--backdrop-blur));
		backdrop-filter: blur(var(--backdrop-blur));
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
