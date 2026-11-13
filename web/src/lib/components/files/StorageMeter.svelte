<script lang="ts">
	import { storageUsage, type StorageUsage } from '#lib/files/api.ts';
	import { formatBytes, storageSummary } from '#lib/files/format.ts';

	let { refresh = 0 }: { refresh?: number } = $props();
	let usage: StorageUsage | null = $state(null);

	$effect(() => {
		void refresh; // reload when the parent bumps it
		storageUsage().then((result) => {
			if (result.ok) usage = result.data;
		});
	});
</script>

{#if usage}
	<p class="muted storage" data-testid="storage-usage">
		{storageSummary(usage)}{#if usage.trash_bytes > 0} · {formatBytes(usage.trash_bytes)} in trash{/if}
		{#if usage.quota_bytes != null}
			<progress max={usage.quota_bytes} value={Math.min(usage.used_bytes, usage.quota_bytes)}></progress>
		{/if}
	</p>
{/if}
