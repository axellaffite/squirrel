<script lang="ts">
	import Icon from '#lib/components/icons/Icon.svelte';
	import ThemePicker from '#lib/components/shell/ThemePicker.svelte';
	import type { ThemeChoice } from '#lib/theme.ts';

	interface Props {
		displayName: string;
		username: string;
		theme: ThemeChoice;
		onthemechange: (choice: ThemeChoice) => void;
		onlogout: () => void;
	}

	let { displayName, username, theme, onthemechange, onlogout }: Props = $props();

	let open = $state(false);
	let root: HTMLDivElement | undefined = $state();
	let trigger: HTMLButtonElement | undefined = $state();
	const uid = $props.id();
	const panelId = `user-menu-${uid}`;
	const initial = $derived(displayName.trim().charAt(0).toUpperCase() || '?');

	function close(returnFocus: boolean) {
		open = false;
		if (returnFocus) trigger?.focus();
	}

	function onKeydown(event: KeyboardEvent) {
		if (open && event.key === 'Escape') close(true);
	}

	function onPointerdown(event: PointerEvent) {
		if (open && root && !root.contains(event.target as Node)) close(false);
	}

	function onFocusout(event: FocusEvent) {
		const next = event.relatedTarget as Node | null;
		if (open && next && root && !root.contains(next)) close(false);
	}
</script>

<svelte:window onkeydown={onKeydown} onpointerdown={onPointerdown} />

<div class="menu" bind:this={root} onfocusout={onFocusout}>
	<button
		bind:this={trigger}
		type="button"
		class="trigger"
		aria-expanded={open}
		aria-controls={panelId}
		aria-label="Account and theme"
		onclick={() => (open = !open)}
	>
		<span class="avatar" aria-hidden="true">{initial}</span>
		<span class="name">{displayName}</span>
		<Icon name="chevron-down" />
	</button>

	{#if open}
		<div id={panelId} class="panel">
			<div class="who">
				<strong>{displayName}</strong>
				<span class="muted">@{username}</span>
			</div>
			<ThemePicker value={theme} onchange={onthemechange} />
			<!-- On ≥ 1024 px "Log out" sits in the top bar instead, so only one is ever visible. -->
			<button type="button" class="logout" onclick={onlogout}>
				<Icon name="log-out" />
				<span>Log out</span>
			</button>
		</div>
	{/if}
</div>

<style>
	.menu {
		position: relative;
	}

	.trigger {
		display: inline-flex;
		align-items: center;
		gap: var(--space-2);
		min-height: var(--tap);
		padding: var(--space-1) var(--space-2) var(--space-1) var(--space-1);
		border: 0;
		border-radius: var(--radius-full);
		background: var(--surface);
		box-shadow: var(--raise-1);
		color: var(--ink);
		font-weight: var(--weight-label);
		cursor: pointer;
		transition:
			box-shadow var(--dur-fast) var(--ease),
			transform var(--dur-fast) var(--ease);
	}

	@media (hover: hover) {
		.trigger:hover {
			box-shadow: var(--raise-2);
		}
	}

	.trigger:focus-visible,
	.logout:focus-visible {
		outline: var(--ring-width) solid var(--accent);
		outline-offset: var(--ring-offset);
	}

	.trigger:active,
	.trigger[aria-expanded='true'] {
		box-shadow: var(--inset-1);
	}

	.trigger:active {
		transform: translateY(var(--press));
	}

	.avatar {
		display: grid;
		place-items: center;
		width: calc(var(--tap) - var(--space-3));
		height: calc(var(--tap) - var(--space-3));
		border-radius: var(--radius-full);
		background: var(--accent);
		color: var(--on-accent);
		font-size: var(--text-sm);
		font-weight: var(--weight-heading);
	}

	.name {
		max-width: var(--name-max);
		overflow: hidden;
		text-overflow: ellipsis;
		white-space: nowrap;
	}

	@media (max-width: 30rem) {
		.name {
			display: none;
		}
	}

	.panel {
		position: absolute;
		z-index: 20;
		top: calc(100% + var(--space-3));
		right: 0;
		display: grid;
		gap: var(--space-4);
		width: min(var(--menu-w), calc(100vw - 2 * var(--gutter)));
		padding: var(--space-4);
		border-radius: var(--radius-lg);
		background: var(--surface);
		box-shadow: var(--raise-3);
		opacity: 1;
		transform: none;
		transition:
			opacity var(--dur) var(--ease),
			transform var(--dur) var(--ease);
	}

	@starting-style {
		.panel {
			opacity: 0;
			transform: translateY(calc(-1 * var(--rise)));
		}
	}

	.who {
		display: grid;
		min-width: 0;
		overflow-wrap: anywhere;
	}

	.who .muted {
		font-size: var(--text-sm);
	}

	.logout {
		display: flex;
		align-items: center;
		gap: var(--space-3);
		min-height: var(--tap);
		padding: var(--space-2) var(--space-3);
		border: 0;
		border-radius: var(--radius-md);
		background: transparent;
		color: var(--ink);
		font-weight: var(--weight-label);
		cursor: pointer;
		transition:
			box-shadow var(--dur-fast) var(--ease),
			color var(--dur-fast) var(--ease),
			transform var(--dur-fast) var(--ease);
	}

	@media (hover: hover) {
		.logout:hover {
			box-shadow: var(--raise-1);
		}
	}

	.logout:active {
		box-shadow: var(--inset-1);
		transform: translateY(var(--press));
	}

	@media (min-width: 64rem) {
		.logout {
			display: none;
		}
	}
</style>
