<script lang="ts">
	import type { Snippet } from 'svelte';
	import Icon from '#lib/components/icons/Icon.svelte';
	import SidebarWash from '#lib/components/shell/SidebarWash.svelte';
	import { isActive, navGroups } from '#lib/nav.ts';
	import { sidebar } from '#lib/sidebar.svelte.ts';

	interface Props {
		path: string;
		canInvite: boolean;
		/** Account controls at the foot of the sidebar (≥ 1024 px only). */
		footer?: Snippet;
		/** Global tools beside the brand at the top of the sidebar (≥ 1024 px only). */
		tools?: Snippet;
	}

	let { path, canInvite, footer, tools }: Props = $props();

	const groups = $derived(navGroups(canInvite));
</script>

<!--
	Phones and tablets: a floating white pill bar, icon-only, every tab state a top notch.
	From 1024 px: the sidebar, grouped into sections, over the photo wash the owner picked.
-->
<aside class="rail">
	<div class="wash-slot"><SidebarWash choice={sidebar.choice} /></div>
	<div class="head">
		<a href="/" class="brand">Squirrel</a>
		{#if tools}{@render tools()}{/if}
	</div>
	<nav aria-label="Main">
		{#each groups as group (group.key)}
			<div class="group" class:end={group.key === 'end'}>
				{#if group.label}<p class="group-label" id="nav-{group.key}">{group.label}</p>{/if}
				<ul aria-labelledby={group.label ? `nav-${group.key}` : undefined}>
					{#each group.items as item (item.href)}
						<li class:sidebar-only={item.sidebarOnly}>
							<a href={item.href} class="item" aria-current={isActive(item.href, path) ? 'page' : undefined}>
								<Icon name={item.icon} />
								<span class="label">{item.label}</span>
							</a>
						</li>
					{/each}
				</ul>
			</div>
		{/each}
	</nav>
	{#if footer}<div class="foot">{@render footer()}</div>{/if}
</aside>

<style>
	/* ---------------------------------------------------------------- phone bar (< 1024 px) */
	.rail {
		position: fixed;
		z-index: 10;
		left: calc(var(--frame) + env(safe-area-inset-left));
		right: calc(var(--frame) + env(safe-area-inset-right));
		bottom: calc(var(--space-3) + env(safe-area-inset-bottom));
		height: var(--rail-bar-h);
		padding: var(--rail-bar-pad);
		border: 1px solid var(--hair);
		border-radius: var(--radius-full);
		background: var(--card);
		color: var(--ink-muted);
		box-shadow: var(--shadow-hover);
		/* Own compositor layer so scrolling under the floating bar does not repaint it. */
		transform: translateZ(0);
	}

	.wash-slot,
	.head,
	.foot,
	.group-label {
		display: none;
	}

	nav,
	.group,
	ul {
		display: contents;
	}

	/* Five equal columns: every icon centred in the same width. */
	nav {
		display: grid;
		grid-auto-flow: column;
		grid-auto-columns: 1fr;
		height: 100%;
	}

	ul {
		margin: 0;
		padding: 0;
		list-style: none;
	}

	li {
		display: flex;
		min-width: 0;
	}

	/* The phone bar keeps five items (MASTER Layout); these join from 1024 px. */
	.sidebar-only {
		display: none;
	}

	.item {
		--notch: transparent;
		--tab-notch-w: var(--notch-w);
		position: relative;
		flex: 1;
		display: grid;
		place-items: center;
		min-height: var(--tap);
		color: var(--ink-muted);
		text-decoration: none;
		-webkit-tap-highlight-color: transparent;
		transition: color var(--dur-fast) var(--ease);
	}

	/* Every tab state is a notch hanging just inside the bar's top hairline. */
	.item::before {
		content: '';
		position: absolute;
		left: 50%;
		top: calc(-1 * var(--rail-bar-pad));
		width: var(--tab-notch-w);
		height: var(--notch-h);
		margin-left: calc(var(--tab-notch-w) / -2);
		border-radius: 0 0 var(--notch-h) var(--notch-h);
		background: var(--notch);
		transition: background-color var(--dur-fast) var(--ease);
	}

	.item :global(svg) {
		transition: transform var(--dur-fast) var(--ease);
	}

	.label {
		position: absolute;
		width: 1px;
		height: 1px;
		overflow: hidden;
		clip-path: inset(50%);
		white-space: nowrap;
	}

	@media (hover: hover) {
		.item:hover {
			--notch: var(--edge);
			color: var(--ink);
		}
	}

	.item:active {
		--notch: var(--accent);
		color: var(--accent-ink);
	}

	.item:active :global(svg) {
		transform: translateY(var(--press));
	}

	.item[aria-current='page'] {
		--notch: var(--accent);
		color: var(--accent-ink);
	}

	.item:focus-visible {
		--notch: var(--ink);
		--tab-notch-w: var(--notch-w-focus);
		outline: none;
		color: var(--ink);
	}

	/* ---------------------------------------------------------------- sidebar (≥ 1024 px) */
	@media (min-width: 64rem) {
		.rail {
			position: sticky;
			top: var(--frame);
			left: auto;
			right: auto;
			bottom: auto;
			isolation: isolate;
			display: flex;
			flex-direction: column;
			gap: var(--space-5);
			height: calc(100svh - 2 * var(--frame));
			padding: var(--rail-pad-y) var(--rail-pad-x);
			border: 0;
			border-radius: var(--radius-rail);
			background: var(--rail);
			color: var(--rail-ink);
			box-shadow: none;
			transform: none;
		}

		.wash-slot {
			display: block;
			position: absolute;
			inset: 0;
			z-index: -1;
			overflow: hidden;
			border-radius: inherit;
		}

		.head {
			display: flex;
			align-items: center;
			justify-content: space-between;
			gap: var(--space-2);
			padding: 0 var(--space-1) 0 var(--space-2);
		}

		.brand {
			display: block;
			border-radius: var(--radius-sm);
			color: var(--rail-ink);
			font-size: var(--text-lg);
			line-height: var(--text-lg-lh);
			font-weight: var(--weight-strong);
			letter-spacing: var(--track-heading);
			text-decoration: none;
		}

		.brand:focus-visible {
			outline: var(--ring-width) solid var(--rail-ink);
			outline-offset: var(--ring-offset);
		}

		nav {
			display: flex;
			flex-direction: column;
			gap: var(--space-5);
			flex: 1;
			height: auto;
			min-height: 0;
			overflow-y: auto;
		}

		.group {
			display: grid;
			gap: var(--space-2);
		}

		.group.end {
			margin-top: auto;
		}

		.group-label {
			display: block;
			margin: 0;
			padding: 0 var(--space-2);
			color: var(--rail-muted);
			font-size: var(--text-sm);
			line-height: var(--text-sm-lh);
			font-weight: var(--weight-label);
		}

		ul {
			display: grid;
			gap: var(--nav-gap);
		}

		li,
		.sidebar-only {
			display: block;
		}

		.item {
			display: flex;
			align-items: center;
			gap: var(--space-3);
			min-height: var(--nav-item);
			padding: 0 var(--space-2);
			border-radius: var(--radius-control);
			color: var(--rail-muted);
			font-size: var(--text-md);
			line-height: var(--text-md-lh);
			font-weight: var(--weight-label);
			letter-spacing: var(--track-label);
			transition:
				background-color var(--dur-fast) var(--ease),
				color var(--dur-fast) var(--ease);
		}

		.item::before {
			display: none;
		}

		.item :global(svg) {
			width: var(--icon-sm);
			height: var(--icon-sm);
			flex: none;
		}

		.label {
			position: static;
			width: auto;
			height: auto;
			overflow: visible;
			clip-path: none;
		}

		@media (hover: hover) {
			.item:hover {
				background: var(--rail-well-hover);
				color: var(--rail-ink);
			}
		}

		.item:active {
			color: var(--rail-ink);
		}

		.item[aria-current='page'] {
			background: var(--rail-active);
			color: var(--rail-active-ink);
			font-weight: var(--weight-strong);
		}

		.item:focus-visible {
			color: var(--rail-ink);
			outline: var(--ring-width) solid var(--rail-ink);
			outline-offset: calc(-1 * var(--ring-width));
		}

		.foot {
			display: grid;
			gap: var(--space-2);
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
