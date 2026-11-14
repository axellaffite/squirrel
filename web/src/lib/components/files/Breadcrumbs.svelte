<script lang="ts">
	import type { PathSegment } from '#lib/files/api.ts';

	let { path, current }: { path: PathSegment[]; current: string | null } = $props();
</script>

<nav aria-label="Breadcrumb" class="breadcrumbs">
	<ol>
		<li>
			{#if current === null}<span aria-current="page">My drive</span>{:else}<a href="/files">My drive</a>{/if}
		</li>
		{#each path as segment (segment.id)}
			<li><a href={`/files/${segment.id}`}>{segment.name}</a></li>
		{/each}
		{#if current !== null}<li><span aria-current="page">{current}</span></li>{/if}
	</ol>
</nav>

<style>
	ol { display: flex; flex-wrap: wrap; gap: 0.25rem; list-style: none; padding: 0; margin: 0; }
	li + li::before { content: '›'; margin-right: 0.25rem; color: var(--ink-muted); }
</style>
