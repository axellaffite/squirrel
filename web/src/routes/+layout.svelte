<script lang="ts">
	import '../app.css';
	import favicon from '#lib/assets/favicon.svg';
	import { goto } from '$app/navigation';
	import { page } from '$app/state';
	import { onMount } from 'svelte';
	import { loadSession, logout, session } from '#lib/session.svelte.ts';
	import type { LayoutProps } from './$types';

	let { children }: LayoutProps = $props();

	const PUBLIC_PREFIXES = ['/login', '/setup', '/join/'];
	const isPublic = $derived(PUBLIC_PREFIXES.some((prefix) => page.url.pathname.startsWith(prefix)));

	onMount(loadSession);

	$effect(() => {
		if (!session.loaded || session.unreachable) return;
		const path = page.url.pathname;
		if (session.needsSetup && path !== '/setup') {
			goto('/setup', { replaceState: true });
		} else if (!session.needsSetup && path === '/setup') {
			goto(session.me ? '/' : '/login', { replaceState: true });
		} else if (!session.me && !isPublic) {
			goto('/login', { replaceState: true });
		} else if (session.me && path === '/login') {
			goto('/', { replaceState: true });
		}
	});

	async function handleLogout() {
		await logout();
		await goto('/login');
	}
</script>

<svelte:head>
	<link rel="icon" href={favicon} />
	<title>Squirrel</title>
</svelte:head>

{#if !session.loaded}
	<main class="center"><p class="muted">Loading…</p></main>
{:else if session.unreachable}
	<main class="center"><p class="error">The Squirrel server is unreachable. Check that it is running.</p></main>
{:else}
	{#if session.me}
		<header class="topbar">
			<a href="/" class="brand">Squirrel</a>
			<nav>
				<a href="/settings/invitations">Invitations</a>
				<button class="link" onclick={handleLogout}>Log out</button>
			</nav>
		</header>
	{/if}
	{@render children()}
{/if}
