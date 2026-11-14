<script lang="ts">
	import Meter from '#lib/components/ui/Meter.svelte';
	import { storageUsage, type StorageUsage } from '#lib/files/api.ts';
	import { formatBytes, storageSummary } from '#lib/files/format.ts';
	import { latestOnly } from '#lib/files/latest.ts';

	let { refresh = 0 }: { refresh?: number } = $props();
	let usage: StorageUsage | null = $state(null);
	const track = latestOnly();

	$effect(() => {
		void refresh; // reload when the parent bumps it
		void track(storageUsage()).then((result) => {
			if (!result.stale && result.value.ok) usage = result.value.data;
		});
	});
</script>

{#if usage}
	<section class="storage" aria-label="Storage" data-testid="storage-usage">
		<p class="summary">
			<span>{storageSummary(usage)}</span>
			{#if usage.trash_bytes > 0}<span class="muted">{formatBytes(usage.trash_bytes)} in trash</span>{/if}
		</p>
		{#if usage.quota_bytes != null}
			<Meter label="Storage used" value={usage.used_bytes} max={usage.quota_bytes} valueText={storageSummary(usage)} />
		{/if}
	</section>
{/if}

<style>
	.storage {
		display: grid;
		gap: var(--space-3);
		padding: var(--well-pad);
		border-radius: var(--radius-well);
		background: var(--tint);
	}

	.summary {
		max-width: none;
		display: flex;
		flex-wrap: wrap;
		justify-content: space-between;
		gap: var(--space-1) var(--space-3);
		padding-inline: var(--space-1);
		font-size: var(--text-sm);
		line-height: var(--text-sm-lh);
		font-weight: var(--weight-label);
		font-variant-numeric: tabular-nums;
	}

	.summary .muted {
		font-weight: var(--weight-body);
	}
</style>
