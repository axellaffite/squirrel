<script lang="ts">
	import type { HTMLButtonAttributes } from 'svelte/elements';
	import Icon from '#lib/components/icons/Icon.svelte';
	import type { IconName } from '#lib/components/icons/paths.ts';

	interface Props extends Omit<HTMLButtonAttributes, 'children'> {
		icon: IconName;
		/** Accessible name (also the tooltip), e.g. "Rename report.pdf". */
		label: string;
		/** Short tooltip when the accessible name carries extra context. */
		tooltip?: string;
		tone?: 'neutral' | 'danger';
		/** `tint` sits in a faint well at rest (for use on bare panels). */
		fill?: 'none' | 'tint';
		/** Renders a link styled as an icon button. */
		href?: string;
		download?: boolean;
		/** Link-only: opens in a new tab (with `rel="noopener"`). */
		newTab?: boolean;
		/** Link-only: the link points at the page shown (`aria-current="page"`). */
		current?: boolean;
		element?: HTMLButtonElement | HTMLAnchorElement;
	}

	let {
		icon,
		label,
		tooltip,
		tone = 'neutral',
		fill = 'none',
		href,
		download = false,
		newTab = false,
		current = false,
		element = $bindable(),
		type = 'button',
		class: className = '',
		...rest
	}: Props = $props();
</script>

{#if href}
	<a
		bind:this={element}
		{href}
		download={download ? '' : undefined}
		target={newTab ? '_blank' : undefined}
		rel={newTab ? 'noopener' : undefined}
		aria-current={current ? 'page' : undefined}
		class="icon-btn {tone} {fill} {className}"
		aria-label={label}
		title={tooltip ?? label}
	>
		<Icon name={icon} />
	</a>
{:else}
	<button
		bind:this={element}
		{type}
		class="icon-btn {tone} {fill} {className}"
		aria-label={label}
		title={tooltip ?? label}
		{...rest}
	>
		<Icon name={icon} />
	</button>
{/if}

<style>
	/* Quiet round button, icon only: 36 px circle, 44 px hit area. */
	.icon-btn {
		position: relative;
		display: inline-grid;
		place-items: center;
		width: var(--control-sm);
		height: var(--control-sm);
		flex: none;
		padding: 0;
		border: 0;
		border-radius: var(--radius-control);
		background: transparent;
		color: var(--ink-muted);
		cursor: pointer;
		transition:
			color var(--dur-fast) var(--ease),
			background-color var(--dur-fast) var(--ease),
			transform var(--dur-fast) var(--ease);
	}

	.icon-btn::before {
		content: '';
		position: absolute;
		inset: calc((var(--control-sm) - var(--tap)) / 2);
		border-radius: var(--radius-control);
	}

	.tint {
		background: var(--tint);
		color: var(--ink);
	}

	.danger {
		color: var(--danger);
	}

	@media (hover: hover) {
		.icon-btn:not(:disabled):hover {
			background: var(--tint-strong);
			color: var(--ink);
		}

		.danger:not(:disabled):hover {
			color: var(--danger);
		}
	}

	.icon-btn:focus-visible {
		outline: var(--ring-width) solid var(--accent);
		outline-offset: var(--ring-offset);
	}

	.icon-btn:not(:disabled):active {
		background: var(--tint-strong);
		transform: translateY(var(--press));
	}

	.icon-btn:disabled {
		opacity: var(--disabled-opacity);
		cursor: not-allowed;
	}

	@media (forced-colors: active) {
		.icon-btn {
			border: 1px solid ButtonText;
		}
	}
</style>
