<script lang="ts">
	import { page } from '$app/state';
	import PhotosTabs from '#lib/components/photos/PhotosTabs.svelte';
	import Page from '#lib/components/shell/Page.svelte';
	import EmptyState from '#lib/components/ui/EmptyState.svelte';
</script>

<svelte:head><title>Albums · Squirrel</title></svelte:head>

<!-- Placeholder until the albums pages land (Phase 3, Task 38). -->
<Page title="Albums">
	<PhotosTabs path={page.url.pathname} />
	<EmptyState icon="library" title="Albums are coming">Gather photos into albums from the timeline soon.</EmptyState>
</Page>
