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
		padding: var(--card-pad);
		border: 1px solid var(--hair);
		border-radius: var(--radius-card);
		background: var(--card);
		box-shadow: var(--shadow-card);
		color: var(--ink);
	}

	.interactive {
		text-decoration: none;
		cursor: pointer;
		transition:
			border-color var(--dur) var(--ease),
			box-shadow var(--dur) var(--ease),
			transform var(--dur-fast) var(--ease);
	}

	@media (hover: hover) {
		.interactive:hover {
			border-color: var(--hair-strong);
			box-shadow: var(--shadow-hover);
		}
	}

	.interactive:focus-visible {
		outline: var(--ring-width) solid var(--accent);
		outline-offset: var(--ring-offset);
	}

	.interactive:active {
		transform: translateY(var(--press));
	}

	@media (forced-colors: active) {
		.card {
			border: 1px solid CanvasText;
		}
	}
</style>
