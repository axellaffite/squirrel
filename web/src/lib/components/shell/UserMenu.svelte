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
		/** `rail`: full-width trigger on the graphite rail, opening upwards. */
		tone?: 'panel' | 'rail';
	}

	let { displayName, username, theme, onthemechange, onlogout, tone = 'panel' }: Props = $props();

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
		<span class="name">{displayName}</span>
		<Icon name={tone === 'rail' ? 'more' : 'chevron-down'} />
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
		position: relative;
		display: inline-flex;
		align-items: center;
		gap: var(--space-2);
		min-height: var(--tap);
		padding: var(--space-1) var(--space-3) var(--space-1) var(--space-1);
		border: 0;
		border-radius: var(--radius-full);
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

	.rail .trigger {
		width: 100%;
		background: var(--rail-well);
		box-shadow: none;
		color: var(--rail-ink);
	}

	@media (hover: hover) {
		.trigger:hover {
			box-shadow: var(--shadow-hover);
		}

		.rail .trigger:hover {
			background: var(--rail-well-hover);
			box-shadow: none;
		}
	}

	.trigger:focus-visible,
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
		gap: var(--space-4);
		width: min(var(--menu-w), calc(100vw - 2 * var(--gutter)));
		padding: var(--space-4);
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
		top: auto;
		right: auto;
		bottom: calc(100% + var(--space-2));
		left: 0;
	}

	@starting-style {
		.panel {
			opacity: 0;
			transform: translateY(calc(-1 * var(--rise)));
		}

		.rail .panel {
			transform: translateY(var(--rise));
		}
	}

	.who {
		display: grid;
		min-width: 0;
		padding-inline: var(--space-1);
		overflow-wrap: anywhere;
		font-weight: var(--weight-label);
	}

	.who .muted {
		font-size: var(--text-sm);
		font-weight: var(--weight-body);
	}

	.logout {
		display: flex;
		align-items: center;
		gap: var(--space-3);
		min-height: var(--tap);
		padding: var(--space-2) var(--space-3);
		border: 0;
		border-radius: var(--radius-full);
		background: var(--tint);
		color: var(--ink);
		font-weight: var(--weight-label);
		cursor: pointer;
		transition:
			background-color var(--dur-fast) var(--ease),
			transform var(--dur-fast) var(--ease);
	}

	@media (hover: hover) {
		.logout:hover {
			background: var(--tint-strong);
		}
	}

	.logout:active {
		transform: translateY(var(--press));
	}

	@media (min-width: 64rem) {
		.logout {
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
