<script lang="ts">
	import { page } from '$app/state';
	import FolderView from '#lib/components/files/FolderView.svelte';
</script>

{#key page.params.id}
	<FolderView folderId={page.params.id ?? null} />
{/key}
