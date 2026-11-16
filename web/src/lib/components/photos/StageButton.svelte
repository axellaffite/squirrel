<script lang="ts">
	import type { HTMLButtonAttributes } from 'svelte/elements';
	import Icon from '#lib/components/icons/Icon.svelte';
	import type { IconName } from '#lib/components/icons/paths.ts';

	interface Props extends Omit<HTMLButtonAttributes, 'children'> {
		icon: IconName;
		/** Accessible name, including the photo it acts on. */
		label: string;
		/** Short tooltip. */
		tooltip?: string;
		/** Renders a link (e.g. a download) styled the same way. */
		href?: string;
		download?: boolean;
	}

	let { icon, label, tooltip, href, download = false, type = 'button', class: className = '', ...rest }: Props = $props();
</script>

<!-- A control on the graphite lightbox stage: a rail well circle with a rail-ink icon. -->
{#if href}
	<a class="stage-btn {className}" {href} download={download ? '' : undefined} aria-label={label} title={tooltip ?? label}>
		<Icon name={icon} />
	</a>
{:else}
	<button {type} class="stage-btn {className}" aria-label={label} title={tooltip ?? label} {...rest}>
		<Icon name={icon} />
	</button>
{/if}

<style>
	.stage-btn {
		display: inline-grid;
		place-items: center;
		width: var(--control-md);
		height: var(--control-md);
		flex: none;
		padding: 0;
		border: 0;
		border-radius: var(--radius-full);
		background: var(--rail-well);
		color: var(--rail-ink);
		cursor: pointer;
		transition:
			background-color var(--dur-fast) var(--ease),
			color var(--dur-fast) var(--ease),
			transform var(--dur-fast) var(--ease);
	}

	@media (hover: hover) {
		.stage-btn:hover {
			background: var(--rail-well-hover);
		}
	}

	.stage-btn:focus-visible {
		outline: var(--ring-width) solid var(--rail-ink);
		outline-offset: var(--ring-offset);
	}

	.stage-btn:active {
		transform: translateY(var(--press));
	}

	.stage-btn[aria-expanded='true'] {
		background: var(--rail-active);
		color: var(--rail-active-ink);
	}

	.stage-btn:disabled {
		opacity: var(--disabled-opacity);
		cursor: not-allowed;
	}

	@media (forced-colors: active) {
		.stage-btn {
			border: 1px solid ButtonText;
		}
	}
</style>
