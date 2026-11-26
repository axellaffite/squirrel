<script lang="ts">
	import '../app.css';
	import favicon from '#lib/assets/favicon.svg';
	import { goto } from '$app/navigation';
	import { page } from '$app/state';
	import { onMount } from 'svelte';
	import UploadPanel from '#lib/components/files/UploadPanel.svelte';
	import Icon from '#lib/components/icons/Icon.svelte';
	import NotificationBell from '#lib/components/notifications/NotificationBell.svelte';
	import AppNav from '#lib/components/shell/AppNav.svelte';
	import SidebarStorage from '#lib/components/shell/SidebarStorage.svelte';
	import TopBar from '#lib/components/shell/TopBar.svelte';
	import UserMenu from '#lib/components/shell/UserMenu.svelte';
	import Alert from '#lib/components/ui/Alert.svelte';
	import Button from '#lib/components/ui/Button.svelte';
	import Card from '#lib/components/ui/Card.svelte';
	import IconButton from '#lib/components/ui/IconButton.svelte';
	import { redirectFor } from '#lib/guard.ts';
	import { LOGOUT_ERROR } from '#lib/session-logic.ts';
	import { setUnread, watchUnread } from '#lib/notifications/inbox.svelte.ts';
	import { loadSession, logout, session } from '#lib/session.svelte.ts';
	import { applyTheme, readTheme, saveTheme, type ThemeChoice } from '#lib/theme.ts';
	import { installBeforeUnloadGuard } from '#lib/upload/uploads.svelte.ts';
	import type { LayoutProps } from './$types';

	let { children }: LayoutProps = $props();

	let logoutError: string | null = $state(null);
	let retrying = $state(false);
	let theme: ThemeChoice = $state(readTheme());

	// Private pages only mount once the guard allows them, so anonymous visitors
	// never trigger their API calls.
	const target = $derived(
		redirectFor({
			loaded: session.loaded,
			unreachable: session.unreachable,
			needsSetup: session.needsSetup,
			me: session.me,
			path: page.url.pathname
		})
	);

	onMount(() => {
		void loadSession();
		// Uploads live app-wide (the panel is mounted here), so the leave warning does too.
		return installBeforeUnloadGuard();
	});

	$effect(() => {
		if (target !== null) goto(target, { replaceState: true });
	});

	$effect(() => {
		if (!session.me) logoutError = null;
	});

	// One unread poller while someone is signed in; a new account starts from zero.
	const signedIn = $derived(session.me?.username ?? null);
	$effect(() => {
		if (signedIn === null) return;
		const stop = watchUnread();
		return () => {
			stop();
			setUnread(0);
		};
	});

	function changeTheme(choice: ThemeChoice) {
		theme = choice;
		applyTheme(choice);
		saveTheme(choice);
	}

	async function handleLogout() {
		logoutError = null;
		try {
			const outcome = await logout();
			if (!outcome.loggedOut) logoutError = outcome.error;
		} catch {
			logoutError = LOGOUT_ERROR;
		}
	}

	async function retry() {
		retrying = true;
		try {
			await loadSession();
		} finally {
			retrying = false;
		}
	}
</script>

<svelte:head>
	<link rel="icon" href={favicon} />
	<title>Squirrel</title>
</svelte:head>

{#if !session.loaded}
	<main class="center"><p class="muted">Loading…</p></main>
{:else if session.unreachable}
	<main class="center">
		<Card as="section" class="unreachable">
			<div class="unreachable-body">
				<Alert>The Squirrel server is unreachable. Check that it is running.</Alert>
				<Button onclick={retry} disabled={retrying} full>
					<Icon name="refresh" />{retrying ? 'Retrying…' : 'Retry'}
				</Button>
			</div>
		</Card>
	</main>
{:else if session.me}
	<div class="shell">
		<AppNav path={page.url.pathname} canInvite={session.me.can_invite}>
			{#snippet tools()}
				<UserMenu
					tone="rail"
					displayName={session.me?.display_name ?? ''}
					username={session.me?.username ?? ''}
					{theme}
					onthemechange={changeTheme}
					onlogout={handleLogout}
				/>
			{/snippet}
			{#snippet footer()}<SidebarStorage />{/snippet}
		</AppNav>
		<div class="panel">
			<header class="phone-top">
				<a href="/" class="wordmark">Squirrel</a>
				<span class="top-tools">
					<IconButton icon="search" label="Open search" tooltip="Search" fill="tint" href="/search" current={page.url.pathname === '/search'} />
					<NotificationBell />
					<UserMenu
					displayName={session.me.display_name}
					username={session.me.username}
					{theme}
					onthemechange={changeTheme}
					onlogout={handleLogout}
					canInvite={session.me.can_invite}
					/>
				</span>
			</header>
			<TopBar path={page.url.pathname} canInvite={session.me.can_invite} />
			{#if logoutError}<Alert>{logoutError}</Alert>{/if}
			<div class="view">
				{#if target === null}
					{@render children()}
				{:else}
					<p class="muted">Loading…</p>
				{/if}
			</div>
			<UploadPanel />
		</div>
	</div>
{:else if target === null}
	{@render children()}
{:else}
	<main class="center"><p class="muted">Loading…</p></main>
{/if}

<style>
	.center {
		display: grid;
		place-items: center;
		min-height: var(--center-min);
		padding: var(--space-6) var(--gutter);
	}

	.center :global(.unreachable) {
		width: min(100%, var(--auth-max));
	}

	.unreachable-body {
		display: grid;
		gap: var(--space-4);
	}

	/* Ground (body gradient) holding the sidebar (or the phone bar) and one near-flat panel. */
	.shell {
		min-height: 100svh;
		padding: calc(var(--frame) + env(safe-area-inset-top)) var(--frame)
			calc(var(--rail-bar-h) + var(--space-5) + env(safe-area-inset-bottom));
	}

	.panel {
		display: grid;
		gap: var(--space-5);
		align-content: start;
		min-width: 0;
		min-height: calc(100svh - 2 * var(--frame) - var(--rail-bar-h) - var(--space-3));
		padding: var(--panel-pad);
		border: 1px solid var(--hair);
		border-radius: var(--radius-panel);
		background: var(--panel);
		box-shadow: var(--panel-edge);
	}

	.view {
		display: grid;
		min-width: 0;
	}

	.phone-top {
		display: flex;
		align-items: center;
		justify-content: space-between;
		gap: var(--space-3);
	}

	.top-tools {
		display: flex;
		align-items: center;
		gap: var(--space-2);
	}

	/* A 44 px tall target: the row already holds the 44 px account button. */
	.wordmark {
		display: inline-flex;
		align-items: center;
		min-height: var(--tap);
		border-radius: var(--radius-sm);
		color: var(--ink);
		font-size: var(--text-xl);
		line-height: var(--text-xl-lh);
		font-weight: var(--weight-strong);
		letter-spacing: var(--track-heading);
		text-decoration: none;
	}

	.wordmark:focus-visible {
		outline: var(--ring-width) solid var(--accent);
		outline-offset: var(--ring-offset);
	}

	@media (min-width: 64rem) {
		.shell {
			display: grid;
			grid-template-columns: var(--rail-w) minmax(0, 1fr);
			gap: var(--space-4);
			align-items: start;
			padding: calc(var(--frame) + env(safe-area-inset-top)) var(--frame) var(--frame);
		}

		.panel {
			min-height: calc(100svh - 2 * var(--frame));
		}

		.phone-top {
			display: none;
		}
	}
</style>
