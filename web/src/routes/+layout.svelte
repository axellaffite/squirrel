<script lang="ts">
	import '../app.css';
	import favicon from '#lib/assets/favicon.svg';
	import { goto } from '$app/navigation';
	import { page } from '$app/state';
	import { onMount } from 'svelte';
	import { redirectFor } from '#lib/guard.ts';
	import { LOGOUT_ERROR } from '#lib/session-logic.ts';
	import { loadSession, logout, session } from '#lib/session.svelte.ts';
	import type { LayoutProps } from './$types';

	let { children }: LayoutProps = $props();

	let logoutError: string | null = $state(null);
	let retrying = $state(false);

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

	onMount(loadSession);

	$effect(() => {
		if (target !== null) goto(target, { replaceState: true });
	});

	$effect(() => {
		if (!session.me) logoutError = null;
	});

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
		<div role="alert">
			<p class="error">The Squirrel server is unreachable. Check that it is running.</p>
		</div>
		<button onclick={retry} disabled={retrying}>{retrying ? 'Retrying…' : 'Retry'}</button>
	</main>
{:else}
	{#if session.me}
		<header class="topbar">
			<a href="/" class="brand">Squirrel</a>
			<nav>
				<a href="/files">Files</a>
				{#if session.me.can_invite}<a href="/settings/invitations">Invitations</a>{/if}
				<button class="link" onclick={handleLogout}>Log out</button>
			</nav>
			{#if logoutError}<p class="error" role="alert">{logoutError}</p>{/if}
		</header>
	{/if}
	{#if target === null}
		{@render children()}
	{:else}
		<main class="center"><p class="muted">Loading…</p></main>
	{/if}
{/if}
