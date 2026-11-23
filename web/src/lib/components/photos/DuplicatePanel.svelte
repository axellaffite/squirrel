<script lang="ts">
	import Badge from '#lib/components/ui/Badge.svelte';
	import Button from '#lib/components/ui/Button.svelte';
	import type { DuplicateMember } from '#lib/duplicates/api.ts';
	import { groupCaption, suggestedKeeper } from '#lib/duplicates/model.ts';
	import type { DuplicatesReview } from '#lib/duplicates/review.svelte.ts';
	import { formatBytes } from '#lib/files/format.ts';
	import { thumbnailUrl } from '#lib/photos/api.ts';
	import { dimensions } from '#lib/photos/info.ts';

	interface Props {
		review: DuplicatesReview;
		/** `open`: choose a photo to keep; `resolved`: show hidden photos again. */
		mode: 'open' | 'resolved';
		onKeep: () => void;
	}

	let { review, mode, onKeep }: Props = $props();

	const group = $derived(review.group);
	const photos = $derived(review.photos);
	const shown = $derived(review.shown);
	const suggested = $derived(group ? suggestedKeeper(group.members) : null);
	const hiddenCount = $derived(photos.filter((photo) => photo.hidden).length);

	function thumbSrc(photo: DuplicateMember): string | null {
		const { id, thumbnail } = photo.asset;
		return thumbnail.state === 'ready' ? thumbnailUrl(id, 'thumb', thumbnail.version) : null;
	}

	function badge(photo: DuplicateMember): { tone: 'accent' | 'success' | 'neutral'; text: string } | null {
		if (group?.kept_id === photo.asset.id) return { tone: 'success', text: 'Kept' };
		if (photo.hidden) return { tone: 'neutral', text: 'Hidden' };
		if (mode === 'open' && suggested?.asset.id === photo.asset.id) return { tone: 'accent', text: 'Suggested' };
		return null;
	}
</script>

{#if group && shown}
	<div class="panel">
		<p class="head">
			<span class="count">Group {review.groupIndex + 1} of {mode === 'open' ? review.openCount : review.groups.length}</span>
			<span class="muted">{groupCaption(group.members)}</span>
		</p>

		<ul class="photos" aria-label="Photos in this group">
			{#each photos as photo, index (photo.asset.id)}
				{@const src = thumbSrc(photo)}
				{@const tag = badge(photo)}
				<li>
					<button
						type="button"
						class="photo"
						aria-current={index === review.photoIndex ? 'true' : undefined}
						aria-label={`Show ${photo.asset.name}${tag ? ` (${tag.text.toLowerCase()})` : ''}`}
						data-member={photo.asset.id}
						onclick={() => review.select(photo.asset.id)}
					>
						<span class="thumb" class:dim={photo.hidden}>{#if src}<img {src} alt="" />{/if}</span>
						<span class="text">
							<span class="name">{photo.asset.name}</span>
							<span class="muted">
								{[dimensions(photo.asset.width, photo.asset.height), formatBytes(photo.size)].filter(Boolean).join(' · ')}
							</span>
							{#if tag}<span class="tag"><Badge tone={tag.tone}>{tag.text}</Badge></span>{/if}
						</span>
						{#if tag}<span class="tag narrow"><Badge tone={tag.tone}>{tag.text}</Badge></span>{/if}
					</button>
				</li>
			{/each}
		</ul>

		<div class="actions">
			{#if mode === 'open'}
				<Button disabled={review.busy} onclick={onKeep}>Keep this</Button>
				<p class="muted hint">
					The other {photos.length - 1 === 1 ? 'photo is' : `${photos.length - 1} photos are`} hidden, not deleted.
				</p>
			{:else}
				{#if shown.hidden}
					<Button variant="secondary" disabled={review.busy} onclick={() => review.restore('shown')}>Show again</Button>
				{/if}
				{#if hiddenCount > 0}
					<Button variant="quiet" disabled={review.busy} onclick={() => review.restore('all')}>Show all again</Button>
				{/if}
			{/if}
		</div>
	</div>
{/if}

<style>
	.panel {
		display: grid;
		gap: var(--space-3);
	}

	.head {
		display: flex;
		flex-wrap: wrap;
		justify-content: space-between;
		gap: var(--space-2);
		margin: 0;
	}

	.count {
		font-weight: var(--weight-label);
		letter-spacing: var(--track-label);
		font-variant-numeric: tabular-nums;
	}

	.muted {
		color: var(--ink-muted);
		font-size: var(--text-sm);
		line-height: var(--text-sm-lh);
		font-variant-numeric: tabular-nums;
	}

	/* Phones: a row of thumbnails under the photo. */
	.photos {
		display: flex;
		gap: var(--space-2);
		margin: 0;
		padding: var(--space-1);
		overflow-x: auto;
		list-style: none;
	}

	.photos li {
		flex: none;
	}

	.photo {
		display: grid;
		gap: var(--space-2);
		padding: var(--space-1);
		border: 0;
		border-radius: var(--radius-sm);
		background: transparent;
		color: inherit;
		font: inherit;
		text-align: start;
		cursor: pointer;
		transition:
			background-color var(--dur-fast) var(--ease),
			box-shadow var(--dur-fast) var(--ease);
	}

	.thumb {
		display: block;
		width: var(--face-lg);
		aspect-ratio: var(--album-ratio);
		border-radius: var(--radius-tile);
		background: var(--tint);
		overflow: hidden;
	}

	.thumb img {
		display: block;
		width: 100%;
		height: 100%;
		object-fit: cover;
	}

	.dim img {
		opacity: var(--disabled-opacity);
	}

	/* Names and facts only where there is room (the column). */
	.text {
		display: none;
	}

	.name {
		overflow-wrap: anywhere;
	}

	@media (hover: hover) {
		.photo:hover {
			background: var(--tint);
		}
	}

	.photo:focus-visible {
		outline: var(--ring-width) solid var(--accent);
		outline-offset: var(--ring-offset);
	}

	.photo:active {
		transform: translateY(var(--press));
	}

	.photo[aria-current='true'] .thumb {
		box-shadow: var(--select-ring);
	}

	.actions {
		display: flex;
		flex-wrap: wrap;
		align-items: center;
		gap: var(--space-3);
	}

	.hint {
		margin: 0;
	}

	/* Wide screens: a column of photos with their facts, next to the viewer. */
	@media (min-width: 64rem) {
		.photos {
			flex-direction: column;
			overflow-x: visible;
		}

		.photo {
			grid-template-columns: auto minmax(0, 1fr);
			align-items: start;
			width: 100%;
		}

		.photo[aria-current='true'] {
			background: var(--tint);
		}

		.text {
			display: grid;
			gap: var(--space-1);
		}

		.narrow {
			display: none;
		}
	}

	@media (prefers-reduced-motion: reduce) {
		.photo {
			transition: none;
		}
	}

	@media (forced-colors: active) {
		.photo[aria-current='true'] {
			outline: 2px solid Highlight;
		}
	}
</style>
