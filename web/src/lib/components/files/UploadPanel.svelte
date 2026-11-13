<script lang="ts">
	import { progressPercent, statusLabel } from '#lib/upload/queue-logic.ts';
	import { cancel, clearFinished, retry, uploadQueue } from '#lib/upload/uploads.svelte.ts';

	const active = new Set(['queued', 'hashing', 'uploading', 'finishing']);
	const busy = $derived(uploadQueue.items.some((item) => active.has(item.status)));

	/** Leaving the page stops the uploads; ask first. They resume when the files are added again. */
	function warnBeforeLeaving(event: BeforeUnloadEvent) {
		if (busy) event.preventDefault();
	}
</script>

<svelte:window onbeforeunload={warnBeforeLeaving} />

{#if uploadQueue.items.length > 0}
	<section class="uploads" aria-label="Uploads">
		<header>
			<h2>Uploads</h2>
			<button class="link" onclick={clearFinished}>Clear finished</button>
		</header>
		<ul>
			{#each uploadQueue.items as item (item.id)}
				<li data-testid="upload-item">
					<span class="name">{item.name}</span>
					<progress max="100" value={progressPercent(item)}></progress>
					<span class:error={item.status === 'failed'} role={item.status === 'failed' ? 'alert' : undefined}
						>{statusLabel(item)}</span
					>
					{#if active.has(item.status)}
						<button class="link" onclick={() => cancel(item.id)}>Cancel</button>
					{:else if item.status === 'failed' || item.status === 'cancelled'}
						<button class="link" onclick={() => retry(item.id)}>Retry</button>
					{/if}
				</li>
			{/each}
		</ul>
	</section>
{/if}

<style>
	.uploads { margin-top: 1.5rem; padding: 1rem; border: 1px solid var(--border); border-radius: var(--radius); background: var(--surface); }
	header { display: flex; justify-content: space-between; align-items: center; }
	h2 { font-size: 1rem; margin: 0; }
	ul { list-style: none; padding: 0; margin: 0.5rem 0 0; display: grid; gap: 0.5rem; }
	li { display: grid; grid-template-columns: minmax(0, 1fr) 8rem auto auto; gap: 0.75rem; align-items: center; }
	.name { overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
	progress { width: 100%; }
</style>
