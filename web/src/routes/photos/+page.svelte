<script lang="ts">
	import { onMount } from 'svelte';
	import { replaceState } from '$app/navigation';
	import { page } from '$app/state';
	import Icon from '#lib/components/icons/Icon.svelte';
	import Lightbox from '#lib/components/photos/Lightbox.svelte';
	import PhotosTabs from '#lib/components/photos/PhotosTabs.svelte';
	import Scrubber from '#lib/components/photos/Scrubber.svelte';
	import SelectionBar from '#lib/components/photos/SelectionBar.svelte';
	import Timeline, { type Geometry } from '#lib/components/photos/Timeline.svelte';
	import Page from '#lib/components/shell/Page.svelte';
	import Alert from '#lib/components/ui/Alert.svelte';
	import Button from '#lib/components/ui/Button.svelte';
	import Dialog from '#lib/components/ui/Dialog.svelte';
	import EmptyState from '#lib/components/ui/EmptyState.svelte';
	import { listFolder, type FileNode } from '#lib/files/api.ts';
	import { mediaStatus, photoDetails, type AssetSummary } from '#lib/photos/api.ts';
	import { Selection } from '#lib/photos/selection.svelte.ts';
	import { createPoller, preparingText } from '#lib/photos/status.ts';
	import { TimelineData } from '#lib/photos/timeline.svelte.ts';
	import { trashPhotos } from '#lib/photos/trash.ts';
	import { enqueue, onUploadsSettled } from '#lib/upload/uploads.svelte.ts';

	const data = new TimelineData();
	let photosFolder = $state<FileNode | null>(null);
	let picker: HTMLInputElement | undefined = $state();
	let preparing = $state(0);
	let geometry: Geometry | undefined = $state();
	let timeline: Timeline | undefined = $state();

	const selection = new Selection();
	let viewing: AssetSummary | null = $state(null);
	let confirmingDelete = $state(false);
	let deleting = $state(false);
	let deleteError: string | null = $state(null);

	const total = $derived(data.total);

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

	/** Keeps `?photo=<id>` in the address, so a reload reopens the same photo. */
	function rememberPhoto(id: string | null) {
		const url = new URL(page.url.href);
		if (id) url.searchParams.set('photo', id);
		else url.searchParams.delete('photo');
		replaceState(url, {});
	}

	function open(asset: AssetSummary) {
		viewing = asset;
		rememberPhoto(asset.id);
	}

	function closeViewer() {
		const id = viewing?.id;
		viewing = null;
		rememberPhoto(null);
		if (id) void timeline?.focusTile(id);
	}

	function toggle(asset: AssetSummary, range: boolean) {
		if (range) selection.selectRange(asset.id, data.ordered.map((item) => item.id));
		else selection.toggle(asset.id);
	}

	async function openFromAddress() {
		const id = page.url.searchParams.get('photo');
		if (!id) return;
		const result = await photoDetails(id);
		if (result.ok) viewing = result.data.asset;
		else rememberPhoto(null);
	}

	async function deleteViewed(asset: AssetSummary): Promise<string | null> {
		const order = data.ordered;
		const index = order.findIndex((item) => item.id === asset.id);
		const result = await trashPhotos([asset.id]);
		if (result.error) return result.error;
		const after = order[index + 1] ?? order[index - 1] ?? null;
		if (after) open(after);
		else closeViewer();
		void data.reload();
		return null;
	}

	async function deleteSelected() {
		deleting = true;
		deleteError = null;
		const result = await trashPhotos([...selection.ids]);
		deleting = false;
		const moved = new Set(result.moved);
		selection.keepOnly(new Set([...selection.ids].filter((id) => !moved.has(id))));
		deleteError = result.error;
		if (!result.error) confirmingDelete = false;
		void data.reload();
	}

	function keydown(event: KeyboardEvent) {
		if (event.key === 'Escape' && selection.active && !viewing && !confirmingDelete) selection.clear();
	}

	onMount(() => {
		void data.reload();
		void findPhotosFolder();
		void openFromAddress();
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
<svelte:window onkeydown={keydown} />

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
			<Timeline bind:this={timeline} bind:geometry {data} selected={selection.ids} selecting={selection.active} onOpen={open} onToggle={toggle} />
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

{#if selection.active}
	<SelectionBar count={selection.size} busy={deleting} onDelete={() => (confirmingDelete = true)} onClear={() => selection.clear()} />
{/if}

{#if confirmingDelete}
	<Dialog
		open
		title={`Move ${selection.size} ${selection.size === 1 ? 'photo' : 'photos'} to the trash?`}
		onclose={() => {
			confirmingDelete = false;
			deleteError = null;
		}}
	>
		<div class="confirm">
			<p class="muted">They stay in the trash for 30 days before they are deleted for good.</p>
			{#if deleteError}<Alert>{deleteError}</Alert>{/if}
		</div>
		{#snippet actions()}
			<Button variant="secondary" onclick={() => (confirmingDelete = false)}>Cancel</Button>
			<Button variant="danger-solid" onclick={deleteSelected} disabled={deleting || selection.size === 0}>Move to trash</Button>
		{/snippet}
	</Dialog>
{/if}

{#if viewing}
	<Lightbox asset={viewing} items={data.ordered} onNavigate={open} onClose={closeViewer} onDelete={deleteViewed} />
{/if}

<style>
	.live {
		display: contents;
	}

	.confirm {
		display: grid;
		gap: var(--space-3);
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
