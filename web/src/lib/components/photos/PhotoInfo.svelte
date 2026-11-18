<script lang="ts">
	import Icon from '#lib/components/icons/Icon.svelte';
	import Locator from '#lib/components/photos/Locator.svelte';
	import Badge from '#lib/components/ui/Badge.svelte';
	import FaceAvatar from '#lib/components/photos/FaceAvatar.svelte';
	import { peopleOnPhoto, photoFaces, type Face } from '#lib/explore/api.ts';
	import { personLabel } from '#lib/explore/faces.ts';
	import { photosHref, tagLabel } from '#lib/explore/filters.ts';
	import { formatBytes } from '#lib/files/format.ts';
	import type { AssetDetails, AssetSummary } from '#lib/photos/api.ts';
	import { fullDateTime, offsetLabel } from '#lib/photos/dates.ts';
	import { formatCoordinates, openStreetMapUrl } from '#lib/photos/geo.ts';
	import { cameraName, dimensions, exposureLine, placeLine } from '#lib/photos/info.ts';

	interface Props {
		asset: AssetSummary;
		/** Null while loading or when it failed. */
		details: AssetDetails | null;
		error: string | null;
	}

	let { asset, details, error }: Props = $props();

	const offset = $derived(offsetLabel(asset.taken_offset));
	const place = $derived(details?.place ? placeLine(details.place) : null);
	const camera = $derived(details?.camera ?? null);
	const cameraLine = $derived(camera ? cameraName(camera) : null);
	const exposure = $derived(camera ? exposureLine(camera) : null);
	const size = $derived(dimensions(asset.width, asset.height));

	/** This photo's faces, for the people's avatars (cropped from this very photo). */
	let faces: Face[] = $state([]);
	const people = $derived(details ? peopleOnPhoto(details.people, faces) : []);

	$effect(() => {
		const id = details?.asset.id;
		const wanted = (details?.people.length ?? 0) > 0;
		faces = [];
		if (!id || !wanted) return;
		let current = true;
		void photoFaces(id).then((result) => {
			if (current && result.ok) faces = result.data;
		});
		return () => {
			current = false;
		};
	});
</script>

<!-- Every value is rendered as text: nothing from EXIF is ever read as HTML or a URL. -->
<div class="info">
	<section class="item" aria-label="Date">
		<span class="well"><Icon name="images" /></span>
		<div class="text">
			<p class="main">{fullDateTime(asset.taken_at)}</p>
			{#if offset}<p class="muted tabular">{offset}</p>{/if}
		</div>
	</section>

	{#if details?.location}
		{@const location = details.location}
		<section class="item" aria-label="Place">
			<span class="well"><Icon name="map-pin" /></span>
			<div class="text">
				{#if place}<p class="main">{place}</p>{/if}
				<Locator latitude={location.latitude} longitude={location.longitude} {place} />
				<p class="muted tabular">{formatCoordinates(location.latitude, location.longitude)}</p>
				<a class="link" href={openStreetMapUrl(location.latitude, location.longitude)} target="_blank" rel="noreferrer noopener">
					Open in OpenStreetMap<Icon name="external-link" />
				</a>
				{#if place}<p class="attribution">Place names: GeoNames (CC BY 4.0)</p>{/if}
			</div>
		</section>
	{/if}

	{#if people.length > 0}
		<section class="item" aria-label="People">
			<span class="well"><Icon name="users" /></span>
			<ul class="text people">
				{#each people as person (person.id)}
					<li>
						<a class="person" href={photosHref({ person: person.id })}>
							<FaceAvatar face={person.face} photoAspect={asset.width && asset.height ? asset.width / asset.height : null} />
							<span class:unnamed={!person.name}>{personLabel(person.name)}</span>
						</a>
					</li>
				{/each}
			</ul>
		</section>
	{/if}

	{#if details && details.tags.length > 0}
		<section class="item" aria-label="Tags">
			<span class="well"><Icon name="tag" /></span>
			<ul class="text badges">
				{#each details.tags as tag (tag)}
					<li><a href={photosHref({ tag })} aria-label={`Photos tagged ${tagLabel(tag)}`}><Badge>{tagLabel(tag)}</Badge></a></li>
				{/each}
			</ul>
		</section>
	{/if}

	{#if cameraLine || exposure || camera?.lens}
		<section class="item" aria-label="Camera">
			<span class="well"><Icon name="camera" /></span>
			<div class="text">
				{#if cameraLine}<p class="main">{cameraLine}</p>{/if}
				{#if camera?.lens}<p class="muted">{camera.lens}</p>{/if}
				{#if exposure}<p class="muted tabular">{exposure}</p>{/if}
			</div>
		</section>
	{/if}

	<section class="item" aria-label="File">
		<span class="well"><Icon name="file" /></span>
		<div class="text">
			<p class="main name">{asset.name}</p>
			<p class="muted tabular">
				{#if size}{size}{/if}{#if size && details}<span aria-hidden="true">{' · '}</span>{/if}{#if details}{formatBytes(details.size)}{/if}
			</p>
			{#if details?.parent_id}<a class="link" href={`/files/${details.parent_id}`}>Show in Files</a>{/if}
			{#if details && details.albums.length > 0}
				<ul class="albums" aria-label="Albums">
					{#each details.albums as album (album.id)}
						<li><a href={`/photos/albums/${album.id}`}><Badge>{album.name}</Badge></a></li>
					{/each}
				</ul>
			{/if}
		</div>
	</section>

	{#if error}<p class="muted">Details are unavailable right now.</p>{/if}
</div>

<style>
	.info {
		display: grid;
		gap: var(--space-5);
		align-content: start;
	}

	.item {
		display: grid;
		grid-template-columns: var(--well-sm) minmax(0, 1fr);
		gap: var(--space-3);
		align-items: start;
	}

	.well {
		display: grid;
		place-items: center;
		width: var(--well-sm);
		height: var(--well-sm);
		border-radius: var(--radius-full);
		background: var(--tint);
		color: var(--ink);
	}

	.text {
		display: grid;
		gap: var(--space-2);
		min-width: 0;
		padding-top: var(--space-2);
	}

	.main {
		font-weight: var(--weight-label);
	}

	.name {
		overflow-wrap: anywhere;
	}

	.muted {
		font-size: var(--text-sm);
		line-height: var(--text-sm-lh);
	}

	.link {
		display: inline-flex;
		align-items: center;
		gap: var(--space-1);
		justify-self: start;
		min-height: var(--tap);
		border-radius: var(--radius-sm);
		font-size: var(--text-sm);
		font-weight: var(--weight-label);
	}

	.attribution {
		color: var(--ink-muted);
		font-size: var(--text-xs);
		line-height: var(--text-xs-lh);
	}

	.people {
		margin: 0;
		padding-left: 0;
		list-style: none;
	}

	.person {
		display: flex;
		align-items: center;
		gap: var(--space-3);
		min-height: var(--tap);
		border-radius: var(--radius-full);
		color: var(--ink);
		font-weight: var(--weight-label);
		text-decoration: none;
	}

	.person:focus-visible,
	.badges a:focus-visible {
		outline: var(--ring-width) solid var(--accent);
		outline-offset: var(--ring-offset);
	}

	.unnamed {
		color: var(--ink-muted);
	}

	.badges {
		display: flex;
		flex-wrap: wrap;
		gap: var(--space-2);
		margin: 0;
		padding-left: 0;
		list-style: none;
	}

	.badges a {
		display: inline-flex;
		border-radius: var(--radius-full);
		text-decoration: none;
	}

	.albums {
		display: flex;
		flex-wrap: wrap;
		gap: var(--space-2);
		margin: 0;
		padding: 0;
		list-style: none;
	}

	.albums a {
		display: inline-flex;
		border-radius: var(--radius-full);
		text-decoration: none;
	}

	@media (forced-colors: active) {
		.well {
			border: 1px solid CanvasText;
		}
	}
</style>
