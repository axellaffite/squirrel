<script lang="ts">
	import type { Snippet } from 'svelte';
	import Icon from '#lib/components/icons/Icon.svelte';
	import type { IconName } from '#lib/components/icons/paths.ts';

	interface Props {
		path: string;
		canInvite: boolean;
		/** Account controls at the foot of the sidebar (≥ 1024 px only). */
		footer?: Snippet;
	}

	let { path, canInvite, footer }: Props = $props();

	interface NavItem {
		href: string;
		label: string;
		icon: IconName;
	}

	const items: NavItem[] = $derived([
		{ href: '/', label: 'Home', icon: 'home' },
		{ href: '/files', label: 'Files', icon: 'folder' },
		{ href: '/trash', label: 'Trash', icon: 'trash' },
		...(canInvite ? [{ href: '/settings/invitations', label: 'Invitations', icon: 'user-plus' as const }] : [])
	]);

	const isActive = (href: string) =>
		href === '/' ? path === '/' : path === href || path.startsWith(`${href}/`);
</script>

<!-- The graphite rail: a floating bar at the bottom on phones and tablets, a sidebar from 1024 px. -->
<aside class="rail">
	<a href="/" class="brand">Squirrel</a>
	<nav aria-label="Main">
		<ul>
			{#each items as item (item.href)}
				<li>
					<a href={item.href} class="item" aria-current={isActive(item.href) ? 'page' : undefined}>
						<span class="dot"><Icon name={item.icon} /></span>
						<span class="label">{item.label}</span>
					</a>
				</li>
			{/each}
		</ul>
	</nav>
	{#if footer}<div class="foot">{@render footer()}</div>{/if}
</aside>

<style>
	.rail {
		position: fixed;
		z-index: 10;
		left: calc(var(--frame) + env(safe-area-inset-left));
		right: calc(var(--frame) + env(safe-area-inset-right));
		bottom: calc(var(--space-3) + env(safe-area-inset-bottom));
		padding: var(--space-2);
		border-radius: var(--radius-rail);
		background: var(--rail);
		color: var(--rail-ink);
		box-shadow: var(--shadow-pop);
		/* Own compositor layer so scrolling under the floating bar does not repaint it. */
		transform: translateZ(0);
	}

	.brand,
	.foot {
		display: none;
	}

	ul {
		display: flex;
		justify-content: space-around;
		gap: var(--space-1);
		margin: 0;
		padding: 0;
		list-style: none;
	}

	li {
		flex: 1;
		display: flex;
		justify-content: center;
		min-width: 0;
	}

	.item {
		display: grid;
		justify-items: center;
		gap: var(--space-1);
		min-width: var(--tap);
		padding: var(--space-1) var(--space-2);
		border-radius: var(--radius-card);
		color: var(--rail-muted);
		font-size: var(--text-xs);
		line-height: var(--text-xs-lh);
		font-weight: var(--weight-label);
		text-decoration: none;
		transition:
			color var(--dur-fast) var(--ease),
			transform var(--dur-fast) var(--ease);
	}

	.dot {
		display: grid;
		place-items: center;
		width: var(--control-sm);
		height: var(--control-sm);
		border-radius: var(--radius-full);
		background: var(--rail-well);
		transition:
			background-color var(--dur-fast) var(--ease),
			color var(--dur-fast) var(--ease),
			box-shadow var(--dur) var(--ease);
	}

	@media (hover: hover) {
		.item:hover {
			color: var(--rail-ink);
		}

		.item:hover .dot {
			background: var(--rail-well-hover);
		}
	}

	.item:focus-visible {
		outline: var(--ring-width) solid var(--rail-ink);
		outline-offset: var(--ring-offset);
	}

	.item:active {
		transform: translateY(var(--press));
	}

	.item[aria-current='page'] {
		color: var(--rail-ink);
		font-weight: var(--weight-strong);
	}

	.item[aria-current='page'] .dot {
		background: var(--rail-active);
		color: var(--rail-active-ink);
		box-shadow: var(--shadow-active);
	}

	@media (min-width: 64rem) {
		.rail {
			position: sticky;
			top: var(--frame);
			left: auto;
			right: auto;
			bottom: auto;
			display: flex;
			flex-direction: column;
			gap: var(--space-2);
			height: calc(100svh - 2 * var(--frame));
			padding: var(--space-5) var(--space-4);
			box-shadow: none;
		}

		.brand {
			display: block;
			align-self: flex-start;
			padding: 0 var(--space-2) var(--space-4);
			border-radius: var(--radius-sm);
			color: var(--rail-ink);
			font-size: var(--text-xl);
			line-height: var(--text-xl-lh);
			font-weight: var(--weight-strong);
			letter-spacing: var(--track-heading);
			text-decoration: none;
		}

		.brand:focus-visible {
			outline: var(--ring-width) solid var(--rail-ink);
			outline-offset: var(--ring-offset);
		}

		ul {
			flex-direction: column;
			gap: var(--space-1);
		}

		li {
			display: block;
		}

		.item {
			display: flex;
			align-items: center;
			gap: var(--space-3);
			padding: var(--space-1);
			border-radius: var(--radius-full);
			font-size: var(--text-md);
			line-height: var(--text-md-lh);
		}

		.foot {
			display: grid;
			gap: var(--space-2);
			margin-top: auto;
		}
	}

	@media (forced-colors: active) {
		.rail {
			border: 1px solid CanvasText;
		}

		.item[aria-current='page'] {
			outline: 1px solid LinkText;
		}
	}
</style>
