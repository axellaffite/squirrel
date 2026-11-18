<script lang="ts">
	import { page } from '$app/state';
	import PersonView from '#lib/components/photos/PersonView.svelte';
</script>

{#key page.params.id}
	<PersonView personId={page.params.id ?? ''} />
{/key}
