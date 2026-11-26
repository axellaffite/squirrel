<script lang="ts">
	import { onMount } from 'svelte';
	import Page from '#lib/components/shell/Page.svelte';
	import SidebarWash from '#lib/components/shell/SidebarWash.svelte';
	import Alert from '#lib/components/ui/Alert.svelte';
	import { listMonth, listMonths, thumbnailUrl, type AssetSummary } from '#lib/photos/api.ts';
	import { isBlurhash } from '#lib/photos/blurhash.ts';
	import { blurred } from '#lib/photos/blurred.svelte.ts';
	import { PRESETS, PRESET_KEYS, DEFAULT_SIDEBAR, type SidebarChoice } from '#lib/sidebar.ts';
	import { chooseSidebar, sidebar } from '#lib/sidebar.svelte.ts';

	/** Photos offered for "Your photo": the most recent ones that have a blurhash. */
	const PICK_LIMIT = 24;

	let photos: AssetSummary[] = $state([]);
	let loading = $state(true);
	let error: string | null = $state(null);
	let choosingPhoto = $state(sidebar.choice.kind === 'photo');

	const current = $derived(sidebar.choice);
	const presetValue = (c: SidebarChoice) => (c.kind === 'preset' ? c.preset : 'photo');
	/** The wash the "Your photo" tile shows: the chosen photo, or the newest one as a hint. */
	const photoTile = $derived<SidebarChoice | null>(
		current.kind === 'photo'
			? current
			: photos[0]?.blurhash
				? { kind: 'photo', id: photos[0].id, blurhash: photos[0].blurhash }
				: null
	);

	onMount(async () => {
		const months = await listMonths();
		if (!months.ok) {
			error = months.error;
			loading = false;
			return;
		}
		const found: AssetSummary[] = [];
		for (const { month } of months.data) {
			const page = await listMonth(month);
			if (!page.ok) {
				error = page.error;
				break;
			}
			found.push(...page.data.filter((a) => a.kind === 'photo' && isBlurhash(a.blurhash)));
			if (found.length >= PICK_LIMIT) break;
		}
		photos = found.slice(0, PICK_LIMIT);
		loading = false;
	});

	function pickPreset(value: string) {
		if (value === 'photo') {
			choosingPhoto = true;
			if (photoTile) chooseSidebar(photoTile);
			return;
		}
		choosingPhoto = false;
		const preset = PRESET_KEYS.find((k) => k === value);
		chooseSidebar(preset ? { kind: 'preset', preset } : DEFAULT_SIDEBAR);
	}

	function pickPhoto(asset: AssetSummary) {
		if (asset.blurhash) chooseSidebar({ kind: 'photo', id: asset.id, blurhash: asset.blurhash });
	}
</script>

<svelte:head><title>Appearance · Squirrel</title></svelte:head>

<Page title="Appearance" back={{ href: '/settings', label: 'Settings' }}>
	{#snippet description()}How Squirrel looks on this device.{/snippet}

	<section class="setting" aria-labelledby="sidebar-heading">
		<div class="setting-text">
			<h2 id="sidebar-heading">Sidebar</h2>
			<p class="muted">The image behind the navigation, blurred into a colour wash. Labels stay readable on every one.</p>
		</div>

		<div class="tiles" role="radiogroup" aria-labelledby="sidebar-heading">
			{#each PRESET_KEYS as key (key)}
				<label class="tile">
					<input type="radio" name="sidebar" value={key} checked={presetValue(current) === key} onchange={() => pickPreset(key)} />
					<span class="pv" aria-hidden="true">
						<SidebarWash choice={{ kind: 'preset', preset: key }} />
						<i></i><i></i><i></i>
					</span>
					<span class="name">{PRESETS[key].label}{#if key === DEFAULT_SIDEBAR.preset}<small>Default</small>{/if}</span>
				</label>
			{/each}
			<label class="tile">
				<input
					type="radio"
					name="sidebar"
					value="photo"
					checked={current.kind === 'photo'}
					disabled={!photoTile}
					onchange={() => pickPreset('photo')}
				/>
				<span class="pv" aria-hidden="true">
					{#if photoTile}<SidebarWash choice={photoTile} />{/if}
					<i></i><i></i><i></i>
				</span>
				<span class="name">Your photo</span>
			</label>
		</div>

		{#if error}<Alert>{error}</Alert>{/if}
		{#if !loading && photos.length === 0 && !error}
			<p class="muted small">Upload photos to use one of yours.</p>
		{/if}

		{#if choosingPhoto && photos.length > 0}
			<div class="photos" role="radiogroup" aria-label="Choose your photo">
				<!-- Photos the blur filter matches are not offered as the sidebar's wash. -->
				{#each photos.filter((photo) => !blurred.has(photo.id)) as asset (asset.id)}
					<label class="photo">
						<input
							type="radio"
							name="sidebar-photo"
							value={asset.id}
							checked={current.kind === 'photo' && current.id === asset.id}
							onchange={() => pickPhoto(asset)}
						/>
						<img src={thumbnailUrl(asset.id, 'thumb', asset.thumbnail.version)} alt={asset.name} loading="lazy" />
					</label>
				{/each}
			</div>
		{/if}
	</section>
</Page>

<style>
	.setting {
		display: grid;
		gap: var(--space-4);
		padding: var(--card-pad);
		border: 1px solid var(--hair);
		border-radius: var(--radius-card);
		background: var(--card);
		box-shadow: var(--shadow-card);
	}

	.setting-text {
		display: grid;
		gap: var(--space-1);
	}

	h2 {
		margin: 0;
		font-size: var(--text-lg);
		line-height: var(--text-lg-lh);
		font-weight: var(--weight-heading);
		letter-spacing: var(--track-heading);
	}

	p {
		margin: 0;
	}

	.muted {
		color: var(--ink-muted);
	}

	.small {
		font-size: var(--text-sm);
	}

	.tiles {
		display: grid;
		grid-template-columns: repeat(auto-fill, minmax(var(--album-min), 1fr));
		gap: var(--space-3);
	}

	input {
		position: absolute;
		opacity: 0;
		pointer-events: none;
	}

	.tile {
		position: relative;
		display: grid;
		gap: var(--space-2);
		cursor: pointer;
	}

	.tile:has(input:disabled) {
		opacity: var(--disabled-opacity);
		cursor: not-allowed;
	}

	/* A miniature sidebar: the wash, its scrim and three label lines. */
	.pv {
		position: relative;
		isolation: isolate;
		display: grid;
		align-content: start;
		gap: var(--space-2);
		aspect-ratio: var(--album-ratio);
		padding: var(--space-3);
		border-radius: var(--radius-card);
		background: var(--rail);
		box-shadow: 0 0 0 1px var(--hair);
		transition: box-shadow var(--dur-fast) var(--ease);
	}

	.pv i {
		display: block;
		height: var(--space-2);
		border-radius: var(--radius-full);
		background: var(--rail-ink);
	}

	.pv i + i {
		width: 70%;
		background: var(--rail-muted);
	}

	.pv i:last-child {
		width: 55%;
	}

	@media (hover: hover) {
		.tile:hover .pv {
			box-shadow:
				0 0 0 1px var(--hair-strong),
				var(--shadow-hover);
		}
	}

	.tile:has(input:checked) .pv {
		box-shadow: var(--select-ring);
	}

	.tile:has(input:focus-visible) .pv {
		box-shadow:
			0 0 0 var(--ring-offset) var(--panel-solid),
			0 0 0 calc(var(--ring-offset) + var(--ring-width)) var(--accent);
	}

	.name {
		display: flex;
		justify-content: space-between;
		align-items: baseline;
		gap: var(--space-2);
		font-weight: var(--weight-label);
	}

	small {
		color: var(--ink-muted);
		font-size: var(--text-xs);
	}

	.photos {
		display: grid;
		grid-template-columns: repeat(auto-fill, minmax(var(--face-md), 1fr));
		gap: var(--space-1);
	}

	.photo {
		position: relative;
		display: block;
		aspect-ratio: 1;
		border-radius: var(--radius-tile);
		overflow: hidden;
		cursor: pointer;
	}

	.photo img {
		width: 100%;
		height: 100%;
		object-fit: cover;
		display: block;
	}

	.photo:has(input:checked) {
		box-shadow: var(--select-ring);
	}

	.photo:has(input:focus-visible) {
		outline: var(--ring-width) solid var(--accent);
		outline-offset: var(--ring-offset);
	}
</style>
