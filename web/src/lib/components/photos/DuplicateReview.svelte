<script lang="ts">
	import Badge from '#lib/components/ui/Badge.svelte';
	import Button from '#lib/components/ui/Button.svelte';
	import IconButton from '#lib/components/ui/IconButton.svelte';
	import { groupCaption, memberFacts, suggestedKeeper } from '#lib/duplicates/model.ts';
	import type { DuplicatesReview } from '#lib/duplicates/review.svelte.ts';
	import { thumbnailUrl } from '#lib/photos/api.ts';
	import { dayLabel, dayKey } from '#lib/photos/dates.ts';
	import { windowPreviewPx } from '#lib/photos/previewSize.ts';

	interface Props {
		review: DuplicatesReview;
		/** `open`: choose a photo to keep; `resolved`: show hidden photos again. */
		mode: 'open' | 'resolved';
		/** After "Keep this" succeeded (the group left the queue). */
		onkept?: (group: NonNullable<DuplicatesReview['group']>, kept: string) => void;
	}

	let { review, mode, onkept }: Props = $props();

	const group = $derived(review.group);
	const photos = $derived(review.photos);
	const shown = $derived(review.shown);
	const suggested = $derived(group ? suggestedKeeper(group.members) : null);
	const facts = $derived(shown ? memberFacts(shown) : []);
	const hiddenCount = $derived(photos.filter((photo) => photo.hidden).length);

	function previewSrc(photo: NonNullable<typeof shown>): string | null {
		const { id, thumbnail, width, height } = photo.asset;
		if (thumbnail.state !== 'ready') return null;
		const size = width && height ? { width, height } : null;
		return thumbnailUrl(id, 'preview', thumbnail.version, windowPreviewPx(size));
	}

	function thumbSrc(photo: NonNullable<typeof shown>): string | null {
		const { id, thumbnail } = photo.asset;
		return thumbnail.state === 'ready' ? thumbnailUrl(id, 'thumb', thumbnail.version) : null;
	}

	async function keep() {
		const current = group;
		const kept = shown?.asset.id;
		const resolved = await review.keepShown();
		if (resolved && current && kept) onkept?.(current, kept);
	}
</script>

{#if group && shown}
	<section class="review" aria-label="Duplicate group">
		<div class="bar">
			<IconButton
				icon="chevron-left"
				label="Previous group"
				fill="tint"
				disabled={review.groupIndex === 0}
				onclick={() => review.moveGroup(-1)}
			/>
			<p class="where">
				<span class="count">Group {review.groupIndex + 1} of {review.groups.length}</span>
				<span class="muted">{dayLabel(dayKey(group.taken_at))} · {groupCaption(group.members)}</span>
			</p>
			<IconButton
				icon="chevron-right"
				label="Next group"
				fill="tint"
				disabled={review.groupIndex >= review.groups.length - 1}
				onclick={() => review.moveGroup(1)}
			/>
		</div>

		<div class="layout">
			<figure class="stage">
				{#key shown.asset.id}
					{@const src = previewSrc(shown)}
					{#if src}
						<img class="main" {src} alt={shown.asset.name} data-shown={shown.asset.id} />
					{:else}
						<div class="main missing" data-shown={shown.asset.id}><p class="muted">No preview yet.</p></div>
					{/if}
				{/key}
				<figcaption class="details">
					<div class="badges">
						{#if suggested && shown.asset.id === suggested.asset.id && mode === 'open'}
							<Badge tone="accent">Suggested</Badge>
						{/if}
						{#if group.kept_id === shown.asset.id}<Badge tone="success">Kept</Badge>{/if}
						{#if shown.hidden}<Badge>Hidden</Badge>{/if}
					</div>
					<dl class="facts">
						{#each facts as fact (fact.label)}
							<div class="fact">
								<dt>{fact.label}</dt>
								<dd>{fact.value}</dd>
							</div>
						{/each}
					</dl>
					<div class="actions">
						{#if mode === 'open'}
							<Button disabled={review.busy} onclick={keep}>Keep this</Button>
							<p class="hint muted">The other {photos.length - 1 === 1 ? 'photo is' : `${photos.length - 1} photos are`} hidden, not deleted.</p>
						{:else}
							{#if shown.hidden}
								<Button variant="secondary" disabled={review.busy} onclick={() => review.restore('shown')}>Show again</Button>
							{/if}
							{#if hiddenCount > 0}
								<Button variant="quiet" disabled={review.busy} onclick={() => review.restore('all')}>Show all again</Button>
							{/if}
						{/if}
					</div>
				</figcaption>
			</figure>

			<ul class="strip" aria-label="Photos in this group">
				{#each photos as photo, index (photo.asset.id)}
					{@const src = thumbSrc(photo)}
					<li>
						<button
							type="button"
							class="thumb"
							class:hidden={photo.hidden}
							aria-label={`Show ${photo.asset.name}${photo.hidden ? ' (hidden)' : ''}`}
							aria-current={index === review.photoIndex ? 'true' : undefined}
							data-strip={photo.asset.id}
							onclick={() => review.select(photo.asset.id)}
						>
							{#if src}<img {src} alt="" />{/if}
						</button>
					</li>
				{/each}
			</ul>
		</div>
	</section>
{/if}

<style>
	.review {
		display: grid;
		gap: var(--space-4);
	}

	.bar {
		display: flex;
		align-items: center;
		gap: var(--space-3);
	}

	.where {
		display: grid;
		flex: 1;
		min-width: 0;
		margin: 0;
		text-align: center;
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

	.layout {
		display: grid;
		gap: var(--space-3);
	}

	.stage {
		display: grid;
		gap: var(--space-4);
		min-width: 0;
		margin: 0;
	}

	.main {
		display: block;
		width: 100%;
		max-height: var(--preview-max);
		border-radius: var(--radius-card);
		background: var(--tint);
		object-fit: contain;
	}

	.missing {
		display: grid;
		place-items: center;
		aspect-ratio: var(--album-ratio);
	}

	.details {
		display: grid;
		gap: var(--space-3);
		padding: var(--card-pad);
		border-radius: var(--radius-card);
		background: var(--card);
		box-shadow: var(--shadow-card);
	}

	.badges {
		display: flex;
		flex-wrap: wrap;
		gap: var(--space-2);
	}

	.badges:empty {
		display: none;
	}

	.facts {
		display: grid;
		gap: var(--space-2);
		margin: 0;
	}

	.fact {
		display: flex;
		justify-content: space-between;
		gap: var(--space-3);
	}

	dt {
		color: var(--ink-muted);
		font-size: var(--text-sm);
		line-height: var(--text-sm-lh);
	}

	dd {
		min-width: 0;
		margin: 0;
		overflow-wrap: anywhere;
		text-align: end;
		font-variant-numeric: tabular-nums;
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

	/* Phones: the strip is a row under the main preview. */
	.strip {
		display: flex;
		gap: var(--space-2);
		margin: 0;
		padding: var(--space-1);
		overflow-x: auto;
		list-style: none;
	}

	.strip li {
		flex: none;
	}

	.thumb {
		display: block;
		width: var(--photo-row);
		aspect-ratio: var(--album-ratio);
		padding: 0;
		border: 0;
		border-radius: var(--radius-tile);
		background: var(--tint);
		cursor: pointer;
		overflow: hidden;
		transition: box-shadow var(--dur-fast) var(--ease);
	}

	.thumb img {
		display: block;
		width: 100%;
		height: 100%;
		object-fit: cover;
	}

	.thumb.hidden img {
		opacity: var(--disabled-opacity);
	}

	@media (hover: hover) {
		.thumb:hover {
			box-shadow: var(--shadow-hover);
		}
	}

	.thumb:focus-visible {
		outline: var(--ring-width) solid var(--accent);
		outline-offset: var(--ring-offset);
	}

	.thumb:active {
		transform: translateY(var(--press));
	}

	.thumb[aria-current='true'] {
		box-shadow: var(--select-ring);
	}

	/* Wide screens: the strip is a column on the right of the main preview. */
	@media (min-width: 64rem) {
		.layout {
			grid-template-columns: minmax(0, 1fr) auto;
			align-items: start;
		}

		.strip {
			flex-direction: column;
			max-height: var(--preview-max);
			overflow-x: visible;
			overflow-y: auto;
		}
	}

	@media (prefers-reduced-motion: reduce) {
		.thumb {
			transition: none;
		}
	}

	@media (forced-colors: active) {
		.thumb[aria-current='true'] {
			outline: 2px solid Highlight;
		}
	}
</style>
