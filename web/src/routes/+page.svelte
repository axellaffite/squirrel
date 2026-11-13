<script lang="ts">
	import { session } from '#lib/session.svelte.ts';
</script>

<svelte:head><title>Home · Squirrel</title></svelte:head>

{#if session.me}
	<main class="page">
		<h1>Hello, {session.me.display_name}</h1>
		<p><a href="/files">Open your files</a></p>
	</main>
{/if}
