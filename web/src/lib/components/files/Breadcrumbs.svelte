<script lang="ts">
	import Icon from '#lib/components/icons/Icon.svelte';
	import type { PathSegment } from '#lib/files/api.ts';

	interface Props {
		/** Ancestors of the current folder, whose name is the page heading. */
		path: PathSegment[];
		current: string | null;
		/** The first crumb (the drive by default; "Albums" for album pages). */
		rootLabel?: string;
		rootHref?: string;
	}

	let { path, current, rootLabel = 'My drive', rootHref = '/files' }: Props = $props();
</script>

<nav aria-label="Breadcrumb" class="breadcrumbs">
	<ol>
		<li>
			{#if current === null}<span aria-current="page">{rootLabel}</span>{:else}<a href={rootHref}>{rootLabel}</a>{/if}
		</li>
		{#each path as segment (segment.id)}
			<li><Icon name="chevron-right" /><a href={`/files/${segment.id}`}>{segment.name}</a></li>
		{/each}
		{#if current !== null}
			<li class="current"><Icon name="chevron-right" /><span aria-current="page">{current}</span></li>
		{/if}
	</ol>
</nav>

<style>
	ol {
		display: flex;
		flex-wrap: wrap;
		align-items: center;
		gap: var(--space-1);
		margin: 0;
		padding: 0;
		list-style: none;
		font-size: var(--text-sm);
		line-height: var(--text-sm-lh);
		font-weight: var(--weight-label);
		color: var(--ink-muted);
	}

	li {
		display: inline-flex;
		align-items: center;
		gap: var(--space-1);
		min-width: 0;
	}

	a,
	[aria-current='page'] {
		display: inline-flex;
		align-items: center;
		min-height: var(--control-sm);
		padding-inline: var(--space-2);
		border-radius: var(--radius-control);
		overflow-wrap: anywhere;
	}

	a {
		position: relative;
		color: var(--ink-muted);
		text-decoration: none;
		transition:
			color var(--dur-fast) var(--ease),
			background-color var(--dur-fast) var(--ease),
			transform var(--dur-fast) var(--ease);
	}

	/* 44 px hit area around the 36 px pill, also for very short folder names. */
	a::before {
		content: '';
		position: absolute;
		top: calc((var(--control-sm) - var(--tap)) / 2);
		bottom: calc((var(--control-sm) - var(--tap)) / 2);
		left: 50%;
		width: max(100%, var(--tap));
		transform: translateX(-50%);
	}

	li:first-child a,
	li:first-child [aria-current='page'] {
		margin-left: calc(-1 * var(--space-2));
	}

	@media (hover: hover) {
		a:hover {
			color: var(--ink);
			background: var(--tint);
		}
	}

	a:active {
		color: var(--ink);
		transform: translateY(var(--press));
	}

	/* The current folder is the page heading right below; keep it for assistive tech only. */
	.current {
		position: absolute;
		width: 1px;
		height: 1px;
		overflow: hidden;
		clip: rect(0 0 0 0);
		white-space: nowrap;
	}
</style>
