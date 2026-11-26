<script lang="ts">
	import { goto } from '$app/navigation';
	import Icon from '#lib/components/icons/Icon.svelte';
	import NotificationBell from '#lib/components/notifications/NotificationBell.svelte';
	import { isActive, navItems } from '#lib/nav.ts';

	interface Props {
		path: string;
		canInvite: boolean;
	}

	let { path, canInvite }: Props = $props();

	let query = $state('');
	let input: HTMLInputElement | undefined = $state();

	/** The section the page belongs to, named as in the sidebar. */
	const section = $derived(navItems(canInvite).find((item) => isActive(item.href, path))?.label ?? 'Squirrel');

	function submit(event: SubmitEvent) {
		event.preventDefault();
		const q = query.trim();
		void goto(q ? `/search?q=${encodeURIComponent(q)}` : '/search');
		query = '';
		input?.blur();
	}

	/** "/" focuses search, unless the person is already typing somewhere. */
	function onKeydown(event: KeyboardEvent) {
		if (event.key !== '/' || event.metaKey || event.ctrlKey || event.altKey) return;
		const target = event.target as HTMLElement | null;
		if (target?.closest('input, textarea, select, [contenteditable="true"]')) return;
		event.preventDefault();
		input?.focus();
	}
</script>

<svelte:window onkeydown={onKeydown} />

<!-- From 1024 px: the panel's top row, page name on the left, search and notifications on the right. -->
<div class="topbar">
	<p class="section">{section}</p>
	<div class="tools">
		{#if path !== '/search'}
			<form class="search" role="search" onsubmit={submit}>
				<Icon name="search" />
				<label class="visually-hidden" for="topbar-search">Search Squirrel</label>
				<input
					bind:this={input}
					bind:value={query}
					id="topbar-search"
					type="search"
					placeholder="Search files and photos"
					autocomplete="off"
					enterkeyhint="search"
				/>
				<kbd aria-hidden="true">/</kbd>
			</form>
		{/if}
		<NotificationBell />
	</div>
</div>

<style>
	.topbar {
		display: none;
	}

	@media (min-width: 64rem) {
		.topbar {
			display: flex;
			align-items: center;
			justify-content: space-between;
			gap: var(--space-4);
			min-height: var(--control-md);
		}
	}

	.section {
		margin: 0;
		color: var(--ink);
		font-weight: var(--weight-label);
		letter-spacing: var(--track-label);
	}

	.tools {
		display: flex;
		align-items: center;
		gap: var(--space-2);
	}

	.search {
		display: flex;
		align-items: center;
		gap: var(--space-2);
		width: var(--menu-w);
		min-height: var(--control-sm);
		padding: 0 var(--space-2) 0 var(--space-3);
		border: 1px solid var(--edge);
		border-radius: var(--radius-control);
		background: var(--card);
		color: var(--ink-muted);
		transition: border-color var(--dur-fast) var(--ease);
	}

	@media (hover: hover) {
		.search:hover {
			border-color: var(--ink-muted);
		}
	}

	.search:focus-within {
		outline: var(--ring-width) solid var(--accent);
		outline-offset: var(--ring-offset);
	}

	.search :global(svg) {
		flex: none;
		width: var(--icon-sm);
		height: var(--icon-sm);
	}

	input {
		flex: 1;
		min-width: 0;
		border: 0;
		background: none;
		color: var(--ink);
		font: inherit;
		font-size: var(--text-sm);
		outline: none;
	}

	input::placeholder {
		color: var(--ink-muted);
	}

	kbd {
		display: grid;
		place-items: center;
		min-width: var(--icon-sm);
		height: var(--icon-sm);
		border: 1px solid var(--hair-strong);
		border-radius: var(--radius-tile);
		color: var(--ink-muted);
		font-family: var(--font-mono);
		font-size: var(--text-xs);
		line-height: 1;
	}

	.search:focus-within kbd {
		visibility: hidden;
	}
</style>
