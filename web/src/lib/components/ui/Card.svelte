<script lang="ts">
	import type { Snippet } from 'svelte';

	interface Props {
		/** Renders the whole card as a link (interactive card). */
		href?: string;
		/** Element for a static card. */
		as?: 'div' | 'section' | 'article';
		/** Index for the arrival stagger; omit to skip the arrival animation. */
		arrive?: number;
		class?: string;
		children: Snippet;
	}

	let { href, as = 'div', arrive, class: className = '', children }: Props = $props();
</script>

{#if href}
	<a {href} class="card interactive {className}" class:arrive={arrive !== undefined} style:--i={arrive}>
		{@render children()}
	</a>
{:else}
	<svelte:element this={as} class="card {className}" class:arrive={arrive !== undefined} style:--i={arrive}>
		{@render children()}
	</svelte:element>
{/if}

<style>
	.card {
		display: block;
		padding: var(--space-4);
		border-radius: var(--radius-lg);
		background: var(--surface);
		box-shadow: var(--raise-2);
		color: var(--ink);
	}

	@media (min-width: 48rem) {
		.card {
			padding: var(--space-5);
		}
	}

	.interactive {
		text-decoration: none;
		cursor: pointer;
		transition:
			box-shadow var(--dur) var(--ease),
			transform var(--dur-fast) var(--ease);
	}

	@media (hover: hover) {
		.interactive:hover {
			box-shadow: var(--raise-3);
		}
	}

	.interactive:focus-visible {
		outline: var(--ring-width) solid var(--accent);
		outline-offset: var(--ring-offset);
	}

	.interactive:active {
		box-shadow: var(--inset-2);
		transform: translateY(var(--press));
		transition-duration: var(--dur-fast);
	}

	@media (forced-colors: active) {
		.card {
			border: 1px solid CanvasText;
		}
	}
</style>
