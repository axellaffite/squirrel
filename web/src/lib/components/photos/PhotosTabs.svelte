<script lang="ts">
	import { isActive } from '#lib/nav.ts';

	interface Props {
		/** The current path, to mark the active tab. */
		path: string;
	}

	let { path }: Props = $props();

	const tabs = [
		{ href: '/photos', label: 'Timeline' },
		{ href: '/photos/albums', label: 'Albums' }
	];

	// The timeline is only "/photos" itself; Albums covers its sub-pages.
	const current = (href: string) => (href === '/photos' ? path === href : isActive(href, path));
</script>

<!-- MASTER's segmented control, as links: tint pill track, the chosen segment a card pill. -->
<nav class="tabs" aria-label="Photos">
	{#each tabs as tab (tab.href)}
		<a href={tab.href} class="tab" aria-current={current(tab.href) ? 'page' : undefined}>{tab.label}</a>
	{/each}
</nav>

<style>
	.tabs {
		display: inline-flex;
		justify-self: start;
		gap: var(--space-1);
		padding: var(--space-1);
		border-radius: var(--radius-full);
		background: var(--tint);
	}

	.tab {
		position: relative;
		display: inline-flex;
		align-items: center;
		min-height: var(--control-sm);
		padding: 0 var(--space-4);
		border-radius: var(--radius-full);
		color: var(--ink-muted);
		font-size: var(--text-sm);
		line-height: var(--text-sm-lh);
		font-weight: var(--weight-label);
		letter-spacing: var(--track-label);
		text-decoration: none;
		transition:
			color var(--dur-fast) var(--ease),
			background-color var(--dur-fast) var(--ease),
			box-shadow var(--dur) var(--ease),
			transform var(--dur-fast) var(--ease);
	}

	/* 44 px hit area around the 36 px pill. */
	.tab::before {
		content: '';
		position: absolute;
		inset: calc((var(--control-sm) - var(--tap)) / 2) 0;
	}

	@media (hover: hover) {
		.tab:hover {
			color: var(--ink);
		}
	}

	.tab:focus-visible {
		outline: var(--ring-width) solid var(--accent);
		outline-offset: var(--ring-offset);
	}

	.tab:active {
		transform: translateY(var(--press));
	}

	.tab[aria-current='page'] {
		background: var(--card);
		color: var(--ink);
		box-shadow: var(--shadow-knob);
	}

	@media (forced-colors: active) {
		.tabs {
			border: 1px solid CanvasText;
		}

		.tab[aria-current='page'] {
			outline: 1px solid LinkText;
		}
	}
</style>
