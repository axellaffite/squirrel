<script lang="ts" module>
	export type ButtonVariant = 'primary' | 'secondary' | 'quiet' | 'danger' | 'danger-solid';
	export type ButtonSize = 'sm' | 'md' | 'lg';
</script>

<script lang="ts">
	import type { Snippet } from 'svelte';
	import type { HTMLButtonAttributes } from 'svelte/elements';

	interface Props extends HTMLButtonAttributes {
		variant?: ButtonVariant;
		size?: ButtonSize;
		/** Stretch to the container width. */
		full?: boolean;
		/** Renders a link styled as a button. */
		href?: string;
		children: Snippet;
	}

	let {
		variant = 'primary',
		size = 'md',
		full = false,
		href,
		type = 'button',
		class: className = '',
		children,
		...rest
	}: Props = $props();
</script>

{#if href}
	<a {href} class="btn {variant} {size} {className}" class:full>{@render children()}</a>
{:else}
	<button {type} class="btn {variant} {size} {className}" class:full {...rest}>{@render children()}</button>
{/if}

<style>
	.btn {
		display: inline-flex;
		align-items: center;
		justify-content: center;
		gap: var(--space-2);
		min-height: var(--tap);
		padding: var(--space-2) var(--space-5);
		border: 1px solid transparent;
		border-radius: var(--radius-md);
		font-family: inherit;
		font-size: var(--text-md);
		line-height: var(--text-md-lh);
		font-weight: var(--weight-label);
		text-decoration: none;
		cursor: pointer;
		user-select: none;
		box-shadow: var(--raise-2);
		transition:
			box-shadow var(--dur-fast) var(--ease),
			transform var(--dur-fast) var(--ease),
			background-color var(--dur-fast) var(--ease),
			color var(--dur-fast) var(--ease);
	}

	.sm {
		min-height: var(--tap);
		padding: var(--space-1) var(--space-3);
		font-size: var(--text-sm);
		border-radius: var(--radius-sm);
		box-shadow: var(--raise-1);
	}

	.lg {
		padding: var(--space-3) var(--space-6);
		font-size: var(--text-lg);
	}

	.full {
		width: 100%;
	}

	.primary {
		background: var(--accent);
		color: var(--on-accent);
	}

	.secondary,
	.danger {
		background: var(--surface);
		color: var(--ink);
	}

	.danger {
		color: var(--danger);
	}

	.danger-solid {
		background: var(--danger);
		color: var(--surface);
	}

	.quiet {
		background: transparent;
		color: var(--ink-muted);
		box-shadow: none;
	}

	@media (hover: hover) {
		.btn:not(:disabled, .quiet):hover {
			box-shadow: var(--raise-3);
		}

		.sm:not(:disabled, .quiet):hover {
			box-shadow: var(--raise-2);
		}

		.quiet:not(:disabled):hover {
			color: var(--ink);
		}
	}

	.btn:focus-visible {
		outline: var(--ring-width) solid var(--accent);
		outline-offset: var(--ring-offset);
	}

	.btn:not(:disabled):active {
		box-shadow: var(--inset-2);
		transform: translateY(var(--press));
	}

	.sm:not(:disabled):active,
	.quiet:not(:disabled):active {
		box-shadow: var(--inset-1);
	}

	.btn:disabled {
		opacity: var(--disabled-opacity);
		box-shadow: none;
		cursor: not-allowed;
	}

	@media (forced-colors: active) {
		.btn {
			border-color: ButtonText;
		}
	}
</style>
