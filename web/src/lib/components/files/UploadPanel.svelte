<script lang="ts">
	import Icon from '#lib/components/icons/Icon.svelte';
	import Button from '#lib/components/ui/Button.svelte';
	import IconButton from '#lib/components/ui/IconButton.svelte';
	import { progressPercent, statusLabel, uploadSummary } from '#lib/upload/queue-logic.ts';
	import { cancel, clearFinished, retry, uploadQueue } from '#lib/upload/uploads.svelte.ts';

	const active = new Set(['queued', 'hashing', 'uploading', 'finishing']);
	// One sentence that changes with upload states, not with every progress tick.
	const summary = $derived(uploadSummary(uploadQueue.items));
</script>

<!-- Always mounted, so screen readers hear the first announcement too. -->
<p class="visually-hidden" aria-live="polite" aria-atomic="true">{summary}</p>

{#if uploadQueue.items.length > 0}
	<section class="uploads" aria-labelledby="uploads-title">
		<header>
			<h2 id="uploads-title">Uploads</h2>
			<Button variant="quiet" size="sm" onclick={clearFinished}>Clear finished</Button>
		</header>
		<ul>
			{#each uploadQueue.items as item (item.id)}
				<li data-testid="upload-item">
					<span class="well" class:done={item.status === 'done'} class:failed={item.status === 'failed'}>
						<Icon name={item.status === 'done' ? 'check' : item.status === 'failed' ? 'alert' : 'upload'} />
					</span>
					<div class="main">
						<span class="name">{item.name}</span>
						<progress max="100" value={progressPercent(item)} aria-label={`Upload progress for ${item.name}`}></progress>
						<span class="status" class:error={item.status === 'failed'}>{statusLabel(item)}</span>
					</div>
					{#if active.has(item.status)}
						<IconButton icon="x" label={`Cancel upload of ${item.name}`} tooltip="Cancel" onclick={() => cancel(item.id)} />
					{:else if item.status === 'failed' || item.status === 'cancelled'}
						<IconButton icon="refresh" label={`Retry upload of ${item.name}`} tooltip="Retry" onclick={() => retry(item.id)} />
					{/if}
				</li>
			{/each}
		</ul>
	</section>
{/if}

<style>
	.uploads {
		display: grid;
		gap: var(--space-3);
		padding: var(--card-pad);
		border-radius: var(--radius-card);
		background: var(--card);
		box-shadow: var(--shadow-card);
	}

	header {
		display: flex;
		justify-content: space-between;
		align-items: center;
		gap: var(--space-3);
		padding-left: var(--space-1);
	}

	h2 {
		font-size: var(--text-lg);
		line-height: var(--text-lg-lh);
	}

	ul {
		display: grid;
		gap: var(--space-2);
		margin: 0;
		padding: 0;
		list-style: none;
	}

	li {
		display: flex;
		align-items: center;
		gap: var(--space-3);
	}

	.well {
		display: grid;
		place-items: center;
		width: var(--well-sm);
		height: var(--well-sm);
		flex: none;
		border-radius: var(--radius-full);
		background: var(--tint);
		color: var(--ink-muted);
	}

	.well.done {
		color: var(--success);
	}

	.well.failed {
		color: var(--danger);
	}

	.main {
		display: grid;
		gap: var(--space-1);
		flex: 1;
		min-width: 0;
	}

	.name {
		overflow: hidden;
		text-overflow: ellipsis;
		white-space: nowrap;
		font-weight: var(--weight-label);
	}

	.status {
		color: var(--ink-muted);
		font-size: var(--text-sm);
		line-height: var(--text-sm-lh);
		font-variant-numeric: tabular-nums;
	}

	.status.error {
		color: var(--danger);
		font-weight: var(--weight-label);
	}

	/* Progress: a thin pill track with the orchid fill. */
	progress {
		width: 100%;
		height: var(--progress-track);
		border: 0;
		border-radius: var(--radius-full);
		background: var(--track);
		overflow: hidden;
		appearance: none;
	}

	progress::-webkit-progress-bar {
		background: var(--track);
		border-radius: var(--radius-full);
	}

	progress::-webkit-progress-value {
		background: var(--accent);
		border-radius: var(--radius-full);
	}

	progress::-moz-progress-bar {
		background: var(--accent);
		border-radius: var(--radius-full);
	}

	@media (forced-colors: active) {
		.uploads {
			border: 1px solid CanvasText;
		}
	}
</style>
