<script lang="ts">
	import { onMount } from 'svelte';
	import Icon from '#lib/components/icons/Icon.svelte';
	import Card from '#lib/components/ui/Card.svelte';
	import Meter from '#lib/components/ui/Meter.svelte';
	import { storageUsage, type StorageUsage } from '#lib/files/api.ts';
	import { formatBytes, storageSummary } from '#lib/files/format.ts';
	import { session } from '#lib/session.svelte.ts';

	let usage: StorageUsage | null = $state(null);
	let usageError: string | null = $state(null);

	onMount(async () => {
		const result = await storageUsage();
		if (result.ok) usage = result.data;
		else usageError = result.error;
	});
</script>

<svelte:head><title>Home · Squirrel</title></svelte:head>

{#if session.me}
	<main class="home">
		<h1 class="arrive" style:--i={0}>Hello, {session.me.display_name}</h1>

		<Card as="section" arrive={1} class="storage">
			<div class="storage-body" aria-live="polite">
				<div class="storage-head">
					<h2>Storage</h2>
					<Icon name="drive" size={24} />
				</div>
				{#if usage}
					<p class="figure tabular">{storageSummary(usage)}</p>
					{#if usage.quota_bytes != null}
						<Meter
							label="Storage used"
							value={usage.used_bytes}
							max={usage.quota_bytes}
							valueText={storageSummary(usage)}
						/>
					{/if}
					{#if usage.trash_bytes > 0}
						<p class="muted tabular small">{formatBytes(usage.trash_bytes)} in trash</p>
					{/if}
				{:else if usageError}
					<p class="muted small">Storage use is unavailable right now.</p>
				{:else}
					<p class="muted small">Loading…</p>
				{/if}
			</div>
		</Card>

		<div class="links">
			<Card href="/files" arrive={2}>
				<span class="link-body">
					<span class="well"><Icon name="folder" size={24} /></span>
					<span class="text">
						<strong>My drive</strong>
						<span class="muted small">Folders, photos and documents</span>
					</span>
					<Icon name="arrow-right" />
				</span>
			</Card>
			<Card href="/trash" arrive={3}>
				<span class="link-body">
					<span class="well"><Icon name="trash" size={24} /></span>
					<span class="text">
						<strong>Trash</strong>
						<span class="muted small">Restore or delete for good</span>
					</span>
					<Icon name="arrow-right" />
				</span>
			</Card>
		</div>
	</main>
{/if}

<style>
	.home {
		display: grid;
		gap: var(--space-5);
		max-width: var(--content-max);
		margin: 0 auto;
		padding: var(--space-5) var(--gutter);
	}

	@media (min-width: 64rem) {
		.home {
			padding-top: var(--space-6);
		}
	}

	.storage-body {
		display: grid;
		gap: var(--space-3);
	}

	.storage-head {
		display: flex;
		align-items: center;
		justify-content: space-between;
		color: var(--ink-muted);
	}

	.storage-head h2 {
		color: var(--ink);
		font-size: var(--text-lg);
		line-height: var(--text-lg-lh);
	}

	.figure {
		font-size: var(--text-xl);
		line-height: var(--text-xl-lh);
		font-weight: var(--weight-strong);
	}

	.small {
		font-size: var(--text-sm);
		line-height: var(--text-sm-lh);
	}

	.links {
		display: grid;
		gap: var(--space-4);
	}

	@media (min-width: 48rem) {
		.links {
			grid-template-columns: 1fr 1fr;
		}
	}

	.link-body {
		display: flex;
		align-items: center;
		gap: var(--space-4);
		color: var(--ink-muted);
	}

	.well {
		display: grid;
		place-items: center;
		width: calc(var(--tap) + var(--space-1));
		height: calc(var(--tap) + var(--space-1));
		flex: none;
		border-radius: var(--radius-sm);
		box-shadow: var(--inset-1);
		color: var(--accent);
	}

	.text {
		display: grid;
		flex: 1;
		min-width: 0;
	}

	.text strong {
		color: var(--ink);
		font-size: var(--text-lg);
		line-height: var(--text-lg-lh);
		font-weight: var(--weight-label);
	}
</style>
