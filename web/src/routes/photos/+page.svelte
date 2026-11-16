<script lang="ts">
	import { onMount } from 'svelte';
	import { page } from '$app/state';
	import Icon from '#lib/components/icons/Icon.svelte';
	import PhotosTabs from '#lib/components/photos/PhotosTabs.svelte';
	import Scrubber from '#lib/components/photos/Scrubber.svelte';
	import Timeline, { type Geometry } from '#lib/components/photos/Timeline.svelte';
	import Page from '#lib/components/shell/Page.svelte';
	import Alert from '#lib/components/ui/Alert.svelte';
	import Button from '#lib/components/ui/Button.svelte';
	import EmptyState from '#lib/components/ui/EmptyState.svelte';
	import { listFolder, type FileNode } from '#lib/files/api.ts';
	import { mediaStatus, type AssetSummary } from '#lib/photos/api.ts';
	import { createPoller, preparingText } from '#lib/photos/status.ts';
	import { TimelineData } from '#lib/photos/timeline.svelte.ts';
	import { enqueue, onUploadsSettled } from '#lib/upload/uploads.svelte.ts';

	const data = new TimelineData();
	let photosFolder = $state<FileNode | null>(null);
	let picker: HTMLInputElement | undefined = $state();
	let preparing = $state(0);
	let geometry: Geometry | undefined = $state();
	let timeline: Timeline | undefined = $state();

	const total = $derived(data.total);
	const selected: ReadonlySet<string> = new Set();

	const poller = createPoller({
		fetch: mediaStatus,
		onCount: (count) => (preparing = count),
		onSettled: () => void data.reload(),
		visible: () => document.visibilityState === 'visible'
	});

	async function findPhotosFolder() {
		const roots = await listFolder(null);
		if (roots.ok) photosFolder = roots.data.find((root) => root.name === 'Photos') ?? null;
	}

	function chosen(event: Event) {
		const input = event.currentTarget as HTMLInputElement;
		const files = Array.from(input.files ?? []);
		input.value = '';
		if (photosFolder && files.length > 0) enqueue(files, photosFolder.id);
	}

	function open(asset: AssetSummary) {
		void asset;
	}

	function toggle(asset: AssetSummary, range: boolean) {
		void asset;
		void range;
	}

	onMount(() => {
		void data.reload();
		void findPhotosFolder();
		poller.start();
		const restart = () => poller.start();
		document.addEventListener('visibilitychange', restart);
		const unsubscribe = onUploadsSettled(() => {
			void data.reload();
			poller.start();
		});
		return () => {
			poller.stop();
			unsubscribe();
			document.removeEventListener('visibilitychange', restart);
		};
	});
</script>

<svelte:head><title>Photos · Squirrel</title></svelte:head>

<Page title="Photos">
	{#snippet description()}{#if total !== null}<span class="tabular">{total} {total === 1 ? 'photo' : 'photos'}</span>{/if}{/snippet}
	{#snippet actions()}
		<input bind:this={picker} type="file" accept="image/*" multiple class="visually-hidden" tabindex="-1" aria-hidden="true" onchange={chosen} />
		<Button onclick={() => picker?.click()} disabled={!photosFolder} aria-label="Upload photos to Photos">
			<Icon name="upload" />Upload
		</Button>
	{/snippet}

	<PhotosTabs path={page.url.pathname} />

	<div class="live" aria-live="polite">
		{#if preparing > 0}<Alert tone="info" role="status">{preparingText(preparing)}</Alert>{/if}
	</div>
	{#if data.error}<Alert>{data.error}</Alert>{/if}

	{#if data.months === null}
		{#if !data.error}<p class="muted" aria-live="polite">Loading…</p>{/if}
	{:else if data.months.length === 0}
		<EmptyState icon="images" title="No photos yet">
			Upload photos, or add images anywhere in your drive: they appear here by date.
		</EmptyState>
	{:else}
		<section class="library" aria-label="Photo timeline">
			<Timeline bind:this={timeline} bind:geometry {data} {selected} selecting={false} onOpen={open} onToggle={toggle} />
			{#if geometry}
				<Scrubber
					boxes={geometry.boxes}
					total={geometry.total}
					current={geometry.current}
					onJump={(index) => timeline?.jumpTo(index)}
				/>
			{/if}
		</section>
	{/if}
</Page>

<style>
	.live {
		display: contents;
	}

	.library {
		display: grid;
		min-width: 0;
	}

	@media (min-width: 48rem) {
		.library {
			grid-template-columns: minmax(0, 1fr) var(--scrubber-w);
			gap: var(--space-4);
			align-items: start;
		}
	}
</style>
