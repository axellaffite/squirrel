<script lang="ts">
	import { page } from '$app/state';
	import AlbumView from '#lib/components/photos/AlbumView.svelte';
</script>

{#key page.params.id}
	<AlbumView albumId={page.params.id ?? ''} />
{/key}
