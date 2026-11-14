<script lang="ts">
	import Icon from '#lib/components/icons/Icon.svelte';
	import type { IconName } from '#lib/components/icons/paths.ts';

	interface Props {
		path: string;
		canInvite: boolean;
	}

	let { path, canInvite }: Props = $props();

	interface NavItem {
		href: string;
		label: string;
		icon: IconName;
	}

	const items: NavItem[] = $derived([
		{ href: '/files', label: 'Files', icon: 'folder' },
		{ href: '/trash', label: 'Trash', icon: 'trash' },
		...(canInvite ? [{ href: '/settings/invitations', label: 'Invitations', icon: 'user-plus' as const }] : [])
	]);

	const isActive = (href: string) => path === href || path.startsWith(`${href}/`);
</script>

<!-- Phones and tablets: a raised bar at the bottom. ≥ 1024 px: inline in the top bar. -->
<nav class="nav" aria-label="Main">
	{#each items as item (item.href)}
		<a href={item.href} class="item" aria-current={isActive(item.href) ? 'page' : undefined}>
			<Icon name={item.icon} size={24} />
			<span>{item.label}</span>
		</a>
	{/each}
</nav>

<style>
	.nav {
		position: fixed;
		z-index: 10;
		left: calc(var(--gutter) + env(safe-area-inset-left));
		right: calc(var(--gutter) + env(safe-area-inset-right));
		bottom: calc(var(--space-3) + env(safe-area-inset-bottom));
		display: grid;
		grid-auto-flow: column;
		grid-auto-columns: 1fr;
		gap: var(--space-2);
		padding: var(--space-2);
		border-radius: var(--radius-xl);
		background: var(--surface);
		box-shadow: var(--raise-2);
	}

	.item {
		display: grid;
		justify-items: center;
		align-content: center;
		gap: var(--space-1);
		min-height: calc(var(--tap) + var(--space-3));
		padding: var(--space-1) var(--space-2);
		border-radius: var(--radius-lg);
		color: var(--ink-muted);
		font-size: var(--text-xs);
		line-height: var(--text-xs-lh);
		font-weight: var(--weight-label);
		text-decoration: none;
		transition:
			color var(--dur-fast) var(--ease),
			box-shadow var(--dur-fast) var(--ease),
			transform var(--dur-fast) var(--ease);
	}

	@media (hover: hover) {
		.item:hover {
			color: var(--ink);
		}
	}

	.item:focus-visible {
		outline: var(--ring-width) solid var(--accent);
		outline-offset: var(--ring-offset);
	}

	.item:active {
		box-shadow: var(--inset-1);
		transform: translateY(var(--press));
	}

	.item[aria-current='page'] {
		color: var(--accent);
		box-shadow: var(--inset-2);
	}

	@media (min-width: 64rem) {
		.nav {
			position: static;
			display: flex;
			gap: var(--space-2);
			padding: 0;
			background: none;
			box-shadow: none;
		}

		.item {
			display: inline-flex;
			align-items: center;
			gap: var(--space-2);
			min-height: var(--tap);
			padding: var(--space-2) var(--space-4);
			border-radius: var(--radius-md);
			font-size: var(--text-md);
			line-height: var(--text-md-lh);
		}
	}

	@media (forced-colors: active) {
		.nav {
			border: 1px solid CanvasText;
		}

		.item[aria-current='page'] {
			outline: 1px solid LinkText;
		}
	}
</style>
