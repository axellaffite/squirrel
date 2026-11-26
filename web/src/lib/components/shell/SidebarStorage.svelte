<script lang="ts">
	import { onMount } from 'svelte';
	import { storageUsage, type StorageUsage } from '#lib/files/api.ts';
	import { formatBytes } from '#lib/files/format.ts';
	import { latestOnly } from '#lib/files/latest.ts';
	import { onUploadsSettled } from '#lib/upload/uploads.svelte.ts';

	let usage = $state<StorageUsage | null>(null);
	const track = latestOnly();

	const ratio = $derived(usage?.quota_bytes ? Math.min(1, usage.used_bytes / usage.quota_bytes) : null);
	const figure = $derived(
		usage === null
			? ''
			: usage.quota_bytes == null
				? `${formatBytes(usage.used_bytes)} used`
				: `${formatBytes(usage.used_bytes)} of ${formatBytes(usage.quota_bytes)}`
	);

	async function load() {
		const result = await track(storageUsage());
		if (!result.stale && result.value.ok) usage = result.value.data;
	}

	onMount(() => {
		void load();
		return onUploadsSettled(() => void load());
	});
</script>

<!-- MASTER.md › "Meter": on the sidebar, a rail-well block; the figure is always printed. -->
{#if usage}
	<a class="storage" href="/files" aria-label={`Storage: ${figure}. Open files`}>
		<span class="top">
			<span>Storage</span>
			<span class="figure">{figure}</span>
		</span>
		{#if ratio !== null}
			<span class="track" aria-hidden="true"><span class="fill" style:--ratio={ratio}></span></span>
		{/if}
	</a>
{/if}

<style>
	.storage {
		display: grid;
		gap: var(--space-2);
		padding: var(--space-3);
		border-radius: var(--radius-card);
		background: var(--rail-well);
		color: var(--rail-muted);
		font-size: var(--text-sm);
		line-height: var(--text-sm-lh);
		text-decoration: none;
		transition: background-color var(--dur-fast) var(--ease);
	}

	@media (hover: hover) {
		.storage:hover {
			background: var(--rail-active);
		}
	}

	.storage:focus-visible {
		outline: var(--ring-width) solid var(--rail-ink);
		outline-offset: var(--ring-offset);
	}

	.top {
		display: flex;
		justify-content: space-between;
		gap: var(--space-2);
	}

	.figure {
		color: var(--rail-ink);
		font-weight: var(--weight-label);
		font-variant-numeric: tabular-nums;
		white-space: nowrap;
	}

	.track {
		display: block;
		height: var(--meter-track);
		border-radius: var(--radius-full);
		background: var(--rail-well);
		overflow: hidden;
	}

	.fill {
		display: block;
		height: 100%;
		border-radius: inherit;
		background: var(--rail-ink);
		transform-origin: left;
		transform: scaleX(var(--ratio));
	}
</style>
