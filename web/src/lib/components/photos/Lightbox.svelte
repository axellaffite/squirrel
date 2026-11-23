<script lang="ts">
	import FaceBoxes from '#lib/components/photos/FaceBoxes.svelte';
	import FaceMenu from '#lib/components/photos/FaceMenu.svelte';
	import PersonPicker, { type PersonChoice } from '#lib/components/photos/PersonPicker.svelte';
	import { photoFaces, type Face } from '#lib/explore/api.ts';
	import { personLabel } from '#lib/explore/faces.ts';
	import { assignFace } from '#lib/explore/people.ts';
	import { onMount, type Snippet } from 'svelte';
	import LightboxPhoto from '#lib/components/photos/LightboxPhoto.svelte';
	import PhotoInfo from '#lib/components/photos/PhotoInfo.svelte';
	import StageButton from '#lib/components/photos/StageButton.svelte';
	import ConfirmDialog from '#lib/components/ConfirmDialog.svelte';
	import { contentUrl } from '#lib/files/format.ts';
	import { latestOnly } from '#lib/files/latest.ts';
	import { photoDetails, thumbnailUrl, type AssetDetails, type AssetSummary } from '#lib/photos/api.ts';
	import { windowPreviewPx } from '#lib/photos/previewSize.ts';
	import { tileDateTime } from '#lib/photos/dates.ts';

	/** A horizontal finger move longer than this (px) goes to the neighbouring photo. */
	const SWIPE_MIN = 50;

	interface Props {
		asset: AssetSummary;
		/** The photos to step through, in timeline order. */
		items: readonly AssetSummary[];
		onNavigate: (asset: AssetSummary) => void;
		onClose: () => void;
		/** Moves the photo to the trash; resolves an error message, or null on success. */
		onDelete: (asset: AssetSummary) => Promise<string | null>;
		/** Offers "Add to album" when given. */
		onAddToAlbum?: (asset: AssetSummary) => void;
		/**
		 * The entry of `items` the arrows step from, when the photo shown is not one of them
		 * (a page whose items stand for groups of photos). Defaults to `asset.id`.
		 */
		current?: string;
		/**
		 * An extra panel next to the photo (below it on phones), e.g. the other photos of a
		 * group. Its accessible name is `panelLabel`.
		 */
		panel?: Snippet;
		panelLabel?: string;
		/** ↑/↓ (-1/+1) while the lightbox has the keyboard; only used with a panel. */
		onStep?: (delta: number) => void;
	}

	let { asset, items, onNavigate, onClose, onDelete, onAddToAlbum, current, panel, panelLabel, onStep }: Props = $props();

	let dialog: HTMLDialogElement | undefined = $state();
	let infoOpen = $state(false);
	let details: AssetDetails | null = $state(null);
	let detailsError: string | null = $state(null);
	let confirming = $state(false);
	/** Face boxes shown over the photo ("People"). */
	let showFaces = $state(false);
	let faces: Face[] = $state([]);
	let facesError: string | null = $state(null);
	/** The face whose menu is open, and the one the person picker is open for. */
	let menuFace: Face | null = $state(null);
	let pickingFor: Face | null = $state(null);
	let faceNotice: string | null = $state(null);

	async function loadFaces(id: string) {
		const result = await photoFaces(id);
		if (asset.id !== id) return;
		if (result.ok) {
			faces = result.data;
			facesError = null;
		} else facesError = result.error;
	}

	$effect(() => {
		const id = asset.id;
		faces = [];
		facesError = null;
		faceNotice = null;
		if (showFaces) void loadFaces(id);
	});

	async function assignPicked(choice: PersonChoice) {
		const face = pickingFor;
		if (!face) return;
		const result = await assignFace(face.id, 'person' in choice ? { person: choice.person.id } : { newPerson: null });
		if (!result.ok) {
			facesError = result.error;
			return;
		}
		faceNotice = 'person' in choice ? `This face is ${personLabel(choice.person.name)}.` : 'This face is a new person.';
		void loadFaces(asset.id);
	}
	let swipe: { x: number; y: number } | null = null;
	const track = latestOnly();

	const index = $derived(items.findIndex((item) => item.id === (current ?? asset.id)));
	const previous = $derived(index > 0 ? items[index - 1] : null);
	const next = $derived(index >= 0 && index < items.length - 1 ? items[index + 1] : null);
	const when = $derived(tileDateTime(asset.taken_at));

	$effect(() => {
		const id = asset.id;
		details = null;
		detailsError = null;
		void track(photoDetails(id)).then((result) => {
			if (result.stale) return;
			if (result.value.ok) details = result.value.data;
			else detailsError = result.value.error;
		});
	});

	// Neighbours' previews are fetched ahead, so stepping through feels instant.
	$effect(() => {
		for (const neighbour of [previous, next]) {
			if (neighbour?.thumbnail.state !== 'ready') continue;
			const size = neighbour.width && neighbour.height ? { width: neighbour.width, height: neighbour.height } : null;
			new Image().src = thumbnailUrl(neighbour.id, 'preview', neighbour.thumbnail.version, windowPreviewPx(size));
		}
	});

	function go(target: AssetSummary | null) {
		if (target) onNavigate(target);
	}

	/**
	 * ←/→ step through photos wherever focus is in the lightbox (window listener), but not while
	 * another dialog (confirmation, album picker) is over it or a field has the focus.
	 */
	function keydown(event: KeyboardEvent) {
		if (confirming || event.defaultPrevented || !dialog?.open) return;
		const target = event.target as Element | null;
		const owner = target?.closest?.('dialog');
		if ((owner && owner !== dialog) || target?.closest?.('input, textarea, select')) return;
		if (event.key === 'ArrowLeft') go(previous);
		else if (event.key === 'ArrowRight') go(next);
		else if (onStep && (event.key === 'ArrowUp' || event.key === 'ArrowDown')) onStep(event.key === 'ArrowUp' ? -1 : 1);
		else return;
		event.preventDefault();
	}

	function pointerdown(event: PointerEvent) {
		swipe = event.pointerType === 'touch' ? { x: event.clientX, y: event.clientY } : null;
	}

	function pointerup(event: PointerEvent) {
		if (!swipe) return;
		const dx = event.clientX - swipe.x;
		const dy = event.clientY - swipe.y;
		swipe = null;
		if (Math.abs(dx) < SWIPE_MIN || Math.abs(dx) < Math.abs(dy)) return;
		go(dx > 0 ? previous : next);
	}

	onMount(() => {
		dialog?.showModal();
	});
</script>

<svelte:window onkeydown={keydown} />

<dialog bind:this={dialog} class="lightbox" class:with-info={infoOpen} class:with-panel={!!panel} aria-label={`Photo taken ${when}`} onclose={onClose}>
	<div class="stage" data-testid="lightbox-stage" onpointerdown={pointerdown} onpointerup={pointerup} role="presentation">
		<LightboxPhoto {asset}>
			{#snippet overlay()}
				{#if showFaces && faces.length > 0}
					<FaceBoxes {faces} onPick={(face) => (menuFace = face)} />
				{/if}
			{/snippet}
		</LightboxPhoto>
		<p class="visually-hidden" aria-live="polite">
			{#if showFaces}{faceNotice ?? (facesError ? facesError : `${faces.length} ${faces.length === 1 ? 'face' : 'faces'} in this photo`)}{/if}
		</p>
		<div class="bar">
			<StageButton icon="x" label="Close the photo viewer" tooltip="Close" onclick={() => dialog?.close()} />
			<div class="tools">
				<StageButton
					icon="scan-face"
					label={showFaces ? `Hide the faces in the photo taken ${when}` : `Show the faces in the photo taken ${when}`}
					tooltip="People"
					aria-pressed={showFaces}
					onclick={() => (showFaces = !showFaces)}
				/>
				<StageButton
					icon="info"
					label={`Details of the photo taken ${when}`}
					tooltip="Details"
					aria-expanded={infoOpen}
					aria-controls="photo-info"
					onclick={() => (infoOpen = !infoOpen)}
				/>
				{#if onAddToAlbum}
					<StageButton icon="library" label={`Add the photo taken ${when} to an album`} tooltip="Add to album" onclick={() => onAddToAlbum(asset)} />
				{/if}
				<StageButton icon="download" label={`Download the photo taken ${when}`} tooltip="Download" href={contentUrl(asset.id, true)} download />
				<StageButton icon="trash" label={`Move the photo taken ${when} to the trash`} tooltip="Move to trash" onclick={() => (confirming = true)} />
			</div>
		</div>
		<!-- Always rendered (aria-disabled at the ends), so a focused arrow never vanishes under focus. -->
		<StageButton
			class="nav prev"
			icon="chevron-left"
			label={previous ? `Previous photo, taken ${tileDateTime(previous.taken_at)}` : 'Previous photo'}
			tooltip="Previous"
			aria-disabled={previous ? undefined : 'true'}
			onclick={() => go(previous)}
		/>
		<StageButton
			class="nav next"
			icon="chevron-right"
			label={next ? `Next photo, taken ${tileDateTime(next.taken_at)}` : 'Next photo'}
			tooltip="Next"
			aria-disabled={next ? undefined : 'true'}
			onclick={() => go(next)}
		/>
	</div>
	{#if panel}
		<aside class="panel" aria-label={panelLabel}>{@render panel()}</aside>
	{/if}
	{#if infoOpen}
		<aside class="sheet" id="photo-info" aria-label="Photo details">
			<PhotoInfo {asset} {details} error={detailsError} />
		</aside>
	{/if}

	{#if menuFace}
		<FaceMenu
			face={menuFace}
			photoAspect={asset.width && asset.height ? asset.width / asset.height : null}
			onChoosePerson={() => (pickingFor = menuFace)}
			onChanged={(message) => {
				faceNotice = message;
				void loadFaces(asset.id);
			}}
			onClose={() => (menuFace = null)}
		/>
	{/if}

	{#if pickingFor}
		<PersonPicker
			title="This is…"
			exclude={pickingFor.person ? [pickingFor.person.id] : []}
			allowNew
			onPick={assignPicked}
			onClose={() => (pickingFor = null)}
		/>
	{/if}

	{#if confirming}
		<ConfirmDialog title="Move this photo to the trash?" confirmLabel="Move to trash" onConfirm={() => onDelete(asset)} onClose={() => (confirming = false)}>
			{asset.name} stays in the trash for 30 days before it is deleted for good.
		</ConfirmDialog>
	{/if}
</dialog>

<style>
	/* A full-viewport graphite stage; the dialog's standard fade on arrival, instant steps. */
	.lightbox {
		position: fixed;
		inset: 0;
		z-index: 40;
		display: grid;
		grid-template-rows: minmax(0, 1fr);
		width: 100vw;
		max-width: none;
		height: 100dvh;
		max-height: none;
		margin: 0;
		padding: 0;
		border: 0;
		background: var(--rail);
		color: var(--rail-ink);
		opacity: 0;
		transition:
			opacity var(--dur-fast) var(--ease),
			overlay var(--dur-fast) allow-discrete,
			display var(--dur-fast) allow-discrete;
	}

	.lightbox:not([open]) {
		display: none;
	}

	.lightbox[open] {
		opacity: 1;
		transition-duration: var(--dur-slow);
	}

	@starting-style {
		.lightbox[open] {
			opacity: 0;
		}
	}

	.lightbox::backdrop {
		background: var(--scrim);
	}

	.with-info {
		grid-template-rows: minmax(0, 1fr) minmax(0, 1fr);
	}

	/* Phones: the panel is a strip under the photo. */
	.with-panel {
		grid-template-rows: minmax(0, 1fr) auto;
	}

	.with-panel.with-info {
		grid-template-rows: minmax(0, 1fr) auto minmax(0, 1fr);
	}

	.panel {
		min-width: 0;
		min-height: 0;
		overflow: auto;
		padding: var(--space-3) var(--space-4) calc(var(--space-3) + env(safe-area-inset-bottom));
		background: var(--card);
		box-shadow: var(--shadow-pop);
		color: var(--ink);
		overscroll-behavior: contain;
	}

	.stage {
		position: relative;
		display: grid;
		min-height: 0;
		padding: calc(var(--control-md) + 2 * var(--space-4)) var(--space-4)
			calc(var(--space-4) + env(safe-area-inset-bottom));
		touch-action: pan-y pinch-zoom;
	}

	.bar {
		position: absolute;
		top: calc(var(--space-4) + env(safe-area-inset-top));
		left: var(--space-4);
		right: var(--space-4);
		display: flex;
		justify-content: space-between;
		gap: var(--space-2);
	}

	.tools {
		display: flex;
		gap: var(--space-2);
	}

	.stage :global(.nav) {
		position: absolute;
		top: 50%;
		transform: translateY(-50%);
	}

	.stage :global(.nav:active) {
		transform: translateY(calc(-50% + var(--press)));
	}

	.stage :global(.prev) {
		left: var(--space-4);
	}

	.stage :global(.next) {
		right: var(--space-4);
	}

	.sheet {
		overflow-y: auto;
		padding: var(--space-5) var(--space-4) calc(var(--space-5) + env(safe-area-inset-bottom));
		border-radius: var(--radius-panel) var(--radius-panel) 0 0;
		background: var(--card);
		box-shadow: var(--shadow-pop);
		color: var(--ink);
		overscroll-behavior: contain;
	}

	/* From 768 px the photo gets side room for the arrows. */
	@media (min-width: 48rem) {
		.stage {
			padding-left: calc(var(--control-md) + 2 * var(--space-4));
			padding-right: calc(var(--control-md) + 2 * var(--space-4));
		}
	}

	/* From 1024 px the details are a panel on the right. */
	@media (min-width: 64rem) {
		.with-info {
			grid-template-rows: minmax(0, 1fr);
			grid-template-columns: minmax(0, 1fr) var(--info-w);
		}

		.sheet {
			margin: var(--space-3);
			border-radius: var(--radius-panel);
		}

		/* The panel is a column on the right of the photo (then the details, when open). */
		.with-panel {
			grid-template-rows: minmax(0, 1fr);
			grid-template-columns: minmax(0, 1fr) var(--info-w);
		}

		.with-panel.with-info {
			grid-template-rows: minmax(0, 1fr);
			grid-template-columns: minmax(0, 1fr) var(--info-w) var(--info-w);
		}

		.panel {
			margin: var(--space-3);
			padding: var(--space-4);
			border-radius: var(--radius-panel);
		}
	}

	@media (forced-colors: active) {
		.lightbox,
		.sheet,
		.panel {
			border: 1px solid CanvasText;
		}
	}
</style>
