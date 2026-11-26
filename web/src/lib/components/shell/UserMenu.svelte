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
		/** `rail`: an avatar at the top right of the sidebar, opening downwards over the panel. */
		tone?: 'panel' | 'rail';
		/** Below 1024 px Invitations is not on the rail: the menu links to it instead. */
		canInvite?: boolean;
	}

	let { displayName, username, theme, onthemechange, onlogout, tone = 'panel', canInvite = false }: Props = $props();

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

<div class="menu" class:rail={tone === 'rail'} bind:this={root} onfocusout={onFocusout}>
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
		{#if tone !== 'rail'}
			<span class="name">{displayName}</span>
			<Icon name="chevron-down" />
		{/if}
	</button>

	{#if open}
		<div id={panelId} class="panel">
			<div class="who">
				<strong>{displayName}</strong>
				<span class="muted">@{username}</span>
			</div>
			<ThemePicker value={theme} onchange={onthemechange} />
			<a class="item" href="/settings/appearance" onclick={() => close(false)}>
				<Icon name="image" />
				<span>Appearance</span>
			</a>
			<a class="item" href="/settings/friends" onclick={() => close(false)}>
				<Icon name="users" />
				<span>Friends</span>
			</a>
			<a class="item" href="/settings/devices" onclick={() => close(false)}>
				<Icon name="smartphone" />
				<span>Devices</span>
			</a>
			<a class="item" href="/settings/faces" onclick={() => close(false)}>
				<Icon name="scan-face" />
				<span>Faces</span>
			</a>
			{#if canInvite}
				<a class="item phone-only" href="/settings/invitations" onclick={() => close(false)}>
					<Icon name="user-plus" />
					<span>Invitations</span>
				</a>
			{/if}
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
		position: relative;
		display: inline-flex;
		align-items: center;
		gap: var(--space-2);
		min-height: var(--tap);
		padding: var(--space-1) var(--space-3) var(--space-1) var(--space-1);
		border: 0;
		border-radius: var(--radius-control);
		background: var(--card);
		box-shadow: var(--shadow-card);
		color: var(--ink);
		font-size: var(--text-sm);
		font-weight: var(--weight-label);
		cursor: pointer;
		transition:
			box-shadow var(--dur) var(--ease),
			background-color var(--dur-fast) var(--ease),
			transform var(--dur-fast) var(--ease);
	}

	/* On the sidebar the trigger is the avatar alone: a 36 px circle (44 px hit area). */
	.rail .trigger {
		justify-content: center;
		width: var(--tap);
		padding: 0;
		border-radius: var(--radius-full);
		background: transparent;
		box-shadow: none;
		color: var(--rail-ink);
	}

	@media (hover: hover) {
		.trigger:hover {
			box-shadow: var(--shadow-hover);
		}

		.rail .trigger:hover {
			box-shadow: none;
		}

		.rail .trigger:hover .avatar {
			background: var(--rail-well);
		}
	}

	.trigger:focus-visible,
	.item:focus-visible,
	.logout:focus-visible {
		outline: var(--ring-width) solid var(--accent);
		outline-offset: var(--ring-offset);
	}

	.trigger:active {
		transform: translateY(var(--press));
	}

	.avatar {
		display: grid;
		place-items: center;
		width: var(--well-sm);
		height: var(--well-sm);
		flex: none;
		border-radius: var(--radius-full);
		background: var(--rail);
		color: var(--rail-ink);
		font-size: var(--text-sm);
		font-weight: var(--weight-strong);
	}

	.rail .avatar {
		background: var(--rail-active);
		color: var(--rail-active-ink);
		transition: background-color var(--dur-fast) var(--ease);
	}

	.rail .trigger:focus-visible {
		outline-color: var(--rail-ink);
	}

	.name {
		flex: 1;
		max-width: var(--name-max);
		overflow: hidden;
		text-overflow: ellipsis;
		white-space: nowrap;
		text-align: left;
	}

	@media (max-width: 30rem) {
		.menu:not(.rail) .name {
			display: none;
		}
	}

	.panel {
		position: absolute;
		z-index: 20;
		top: calc(100% + var(--space-2));
		right: 0;
		display: grid;
		gap: var(--space-1);
		width: min(var(--menu-w), calc(100vw - 2 * var(--gutter)));
		padding: var(--space-1);
		border: 1px solid var(--hair);
		border-radius: var(--radius-card);
		background: var(--card);
		color: var(--ink);
		box-shadow: var(--shadow-pop);
		opacity: 1;
		transform: none;
		transition:
			opacity var(--dur) var(--ease),
			transform var(--dur) var(--ease);
	}

	.rail .panel {
		right: auto;
		left: 0;
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
		padding: var(--space-2) var(--space-3) var(--space-1);
		overflow-wrap: anywhere;
		font-weight: var(--weight-label);
	}

	.who .muted {
		font-size: var(--text-sm);
		font-weight: var(--weight-body);
	}

	.item,
	.logout {
		display: flex;
		align-items: center;
		gap: var(--space-3);
		min-height: var(--tap);
		padding: var(--space-2) var(--space-3);
		border: 0;
		border-radius: var(--radius-sm);
		background: transparent;
		color: var(--ink);
		font-weight: var(--weight-label);
		text-decoration: none;
		cursor: pointer;
		transition:
			background-color var(--dur-fast) var(--ease),
			transform var(--dur-fast) var(--ease);
	}

	@media (hover: hover) {
		.item:hover,
		.logout:hover {
			background: var(--tint);
		}
	}

	.item:active,
	.logout:active {
		transform: translateY(var(--press));
	}

	@media (min-width: 64rem) {
		.phone-only {
			display: none;
		}
	}

	@media (forced-colors: active) {
		.trigger,
		.panel {
			border: 1px solid CanvasText;
		}
	}
</style>
