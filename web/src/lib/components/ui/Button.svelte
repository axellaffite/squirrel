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
		/** Link-only attributes, used with `href`. */
		target?: string;
		rel?: string;
		download?: boolean | string;
		/** The rendered element, e.g. to move focus to it. */
		element?: HTMLButtonElement | HTMLAnchorElement;
		children: Snippet;
	}

	let {
		variant = 'primary',
		size = 'md',
		full = false,
		href,
		target,
		rel,
		download,
		element = $bindable(),
		type = 'button',
		class: className = '',
		children,
		...rest
	}: Props = $props();
</script>

{#if href}
	<a
		bind:this={element}
		{href}
		{target}
		{rel}
		download={download === true ? '' : download || undefined}
		aria-label={rest['aria-label']}
		class="btn {variant} {size} {className}"
		class:full>{@render children()}</a
	>
{:else}
	<button bind:this={element} {type} class="btn {variant} {size} {className}" class:full {...rest}
		>{@render children()}</button
	>
{/if}

<style>
	/* Buttons: 8 px corners, sized to their label. Hover changes a fill or a border colour only; presses sink 1 px. */
	.btn {
		position: relative;
		display: inline-flex;
		align-items: center;
		justify-content: center;
		gap: var(--space-2);
		min-height: var(--control-md);
		padding: var(--space-2) var(--space-4);
		border: 1px solid transparent;
		border-radius: var(--radius-control);
		font-family: inherit;
		font-size: var(--text-md);
		line-height: var(--text-md-lh);
		font-weight: var(--weight-label);
		letter-spacing: var(--track-label);
		text-decoration: none;
		white-space: nowrap;
		cursor: pointer;
		user-select: none;
		transition:
			transform var(--dur-fast) var(--ease),
			background-color var(--dur-fast) var(--ease),
			border-color var(--dur-fast) var(--ease),
			color var(--dur-fast) var(--ease);
	}

	.sm {
		min-height: var(--control-sm);
		padding: var(--space-1) var(--space-3);
		font-size: var(--text-sm);
		line-height: var(--text-sm-lh);
	}

	/* Small buttons keep a 44 px hit area. */
	.sm::before {
		content: '';
		position: absolute;
		inset: calc((var(--control-sm) - var(--tap)) / 2) 0;
	}

	.lg {
		min-height: var(--control-lg);
		padding: var(--space-3) var(--space-5);
	}

	.full {
		width: 100%;
	}

	.primary {
		background: var(--accent);
		color: var(--on-accent);
		font-weight: var(--weight-strong);
		box-shadow: var(--shadow-card);
	}

	.secondary,
	.danger {
		background: var(--card);
		border-color: var(--hair-strong);
		color: var(--ink);
		box-shadow: var(--shadow-card);
	}

	.danger {
		color: var(--danger);
	}

	.danger-solid {
		background: var(--danger);
		color: var(--on-danger);
		font-weight: var(--weight-strong);
	}

	.quiet {
		background: transparent;
		color: var(--ink-muted);
	}

	@media (hover: hover) {
		.primary:not(:disabled):hover {
			background: color-mix(in srgb, var(--accent) 88%, var(--ink));
		}

		.secondary:not(:disabled):hover,
		.danger:not(:disabled):hover {
			border-color: color-mix(in srgb, var(--ink) 28%, transparent);
		}

		.quiet:not(:disabled):hover {
			background: var(--tint-strong);
		}

		.quiet:not(:disabled):hover {
			color: var(--ink);
		}

		.danger-solid:not(:disabled):hover {
			background: color-mix(in srgb, var(--danger) 88%, var(--ink));
		}
	}

	.btn:focus-visible {
		outline: var(--ring-width) solid var(--accent);
		outline-offset: var(--ring-offset);
	}

	.btn:not(:disabled):active {
		transform: translateY(var(--press));
	}

	.btn:disabled {
		opacity: var(--disabled-opacity);
		box-shadow: none;
		cursor: not-allowed;
	}

	@media (forced-colors: active) {
		.btn {
			border: 1px solid ButtonText;
		}
	}
</style>
