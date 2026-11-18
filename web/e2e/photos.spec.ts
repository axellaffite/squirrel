import { expect, test, type Page } from '@playwright/test';
import { csrfToken, openDialog, row } from './helpers/files.ts';
import {
	fixture,
	lightbox,
	newUser,
	photoId,
	tile,
	tiles,
	uploadPhotos,
	WORKER_TIMEOUT
} from './helpers/photos.ts';

// One fresh user's library, built once, then explored scenario by scenario.
test.describe.configure({ mode: 'serial' });

let page: Page;
let paris: string;
let lyon: string;

const PARIS_LABEL = 'Open photo taken 14 July 2024 18:30';
const LYON_LABEL = 'Open photo taken 1 May 2023 09:00';

test.beforeAll(async ({ browser }) => {
	test.setTimeout(120_000);
	({ page } = await newUser(browser, 'photographer'));
	await uploadPhotos(
		page,
		fixture('paris-2024.jpg'),
		fixture('lyon-2023.jpg'),
		fixture('Screenshot_2024-07-15-10-00-00.png')
	);
	// One wait for all worker output: three photos in the timeline, nothing pending, and the
	// Paris photo named from the geodata (its geocode step runs last).
	await expect
		.poll(
			async () => {
				const months = (await (await page.request.get('/api/photos/months')).json()) as { count: number }[];
				const status = (await (await page.request.get('/api/media/status')).json()) as {
					queued: number;
					processing: number;
				};
				const photos = (await (await page.request.get('/api/photos?limit=500')).json()) as {
					items: { id: string; name: string }[];
				};
				const parisId = photos.items.find((item) => item.name === 'paris-2024.jpg')?.id;
				const place = parisId
					? ((await (await page.request.get(`/api/photos/${parisId}`)).json()) as { place?: { name: string } | null })
							.place?.name
					: undefined;
				return {
					photos: months.reduce((sum, month) => sum + month.count, 0),
					pending: status.queued + status.processing,
					place
				};
			},
			{ timeout: WORKER_TIMEOUT }
		)
		.toEqual({ photos: 3, pending: 0, place: 'Paris' });
	paris = await photoId(page, 'paris-2024.jpg');
	lyon = await photoId(page, 'lyon-2023.jpg');
});

test.afterAll(async () => {
	await page?.context().close();
});

test('the timeline groups photos by month and day, newest first', async () => {
	await page.goto('/photos');
	await expect(page.getByText('3 photos', { exact: true })).toBeVisible();
	const months = page.getByRole('heading', { level: 2 }).filter({ hasText: /^(July 2024|May 2023)$/ });
	await expect(months).toHaveText(['July 2024', 'May 2023']);
	await expect(page.getByRole('heading', { level: 3, name: 'Sun 14 Jul 2024' })).toBeVisible();
	await expect(tiles(page)).toHaveCount(3);
	// Every thumbnail actually decoded (not just requested).
	await expect
		.poll(() => tiles(page).locator('img').evaluateAll((imgs) => imgs.map((img) => (img as HTMLImageElement).naturalWidth > 0)))
		.toEqual([true, true, true]);
});

test('a filter in the address narrows the timeline, and the rail clears it', async () => {
	await page.goto('/photos?country=FR&city=Paris');
	await expect(page.getByRole('button', { name: 'Remove filter Paris' })).toBeVisible();
	await expect(tiles(page)).toHaveCount(1);
	// Same route, another address: what is shown follows it.
	await page.getByRole('navigation', { name: 'Main' }).getByRole('link', { name: 'Photos', exact: true }).click();
	await expect(page.getByRole('button', { name: 'Remove filter Paris' })).toHaveCount(0);
	await expect(tiles(page)).toHaveCount(3);
	await page.goto('/photos?country=FR&city=Lyon&kind=photo');
	await page.getByRole('button', { name: 'Remove filter Lyon' }).click();
	// The keyboard stays on the chips: the next chip's clear button.
	await expect(page.getByRole('button', { name: 'Remove filter Photos only' })).toBeFocused();
	await page.getByRole('button', { name: 'Remove filter Photos only' }).click();
	await expect(page.getByRole('button', { name: 'Filter', exact: true })).toBeFocused();
	await expect(tiles(page)).toHaveCount(3);
	await expect(page).toHaveURL(/\/photos$/);
});

test('Explore › Places groups places by country and filters by place', async () => {
	await page.goto('/explore/places');
	await expect(page.getByRole('heading', { level: 2, name: 'France' })).toBeVisible();
	await page.getByRole('link', { name: /^Paris/ }).click();
	await expect(page).toHaveURL(/country=FR/);
	await expect(page.getByRole('button', { name: 'Remove filter Paris' })).toBeVisible();
	await expect(tiles(page)).toHaveCount(1);
});

test('the search page follows its address', async () => {
	await page.goto('/search?q=dog');
	await expect(page.getByRole('searchbox', { name: 'Search photos' })).toHaveValue('dog');
	await page.getByRole('navigation', { name: 'Main' }).getByRole('link', { name: 'Search', exact: true }).click();
	await expect(page.getByRole('searchbox', { name: 'Search photos' })).toHaveValue('');
	await expect(page).toHaveURL(/\/search$/);
});

test('the lightbox shows the details, steps with the arrows and gives focus back', async () => {
	await page.goto('/photos');
	await page.getByRole('button', { name: PARIS_LABEL }).click();
	const viewer = lightbox(page);
	await expect(viewer).toHaveAttribute('aria-label', 'Photo taken 14 July 2024 18:30');

	await viewer.getByRole('button', { name: /^Details of the photo/ }).click();
	const info = viewer.getByRole('complementary', { name: 'Photo details' });
	await expect(info).toContainText('Paris, Île-de-France, France');
	await expect(info).toContainText('Google Pixel 8');
	await expect(info).toContainText('1/250 · ƒ/1.7 · ISO 64');
	await expect(info).toContainText('UTC+02:00');
	const osm = info.getByRole('link', { name: 'Open in OpenStreetMap' });
	await expect(osm).toHaveAttribute('rel', /noreferrer/);

	await page.keyboard.press('ArrowRight');
	await expect(viewer).toHaveAttribute('aria-label', 'Photo taken 1 May 2023 09:00');
	// The last photo: Next stays in place, disabled.
	await expect(viewer.getByRole('button', { name: 'Next photo' })).toHaveAttribute('aria-disabled', 'true');
	await page.keyboard.press('ArrowLeft');
	await expect(viewer).toHaveAttribute('aria-label', 'Photo taken 14 July 2024 18:30');

	await page.keyboard.press('Escape');
	await expect(lightbox(page)).toHaveCount(0);
	await expect(page.getByRole('button', { name: PARIS_LABEL })).toBeFocused();
	await expect(page).not.toHaveURL(/photo=/);
});

test('photos go into an album that can be trimmed, renamed and deleted', async () => {
	await page.goto('/photos');
	// The hover circle starts selection; then a click on a tile ("Select …") toggles it.
	await tile(page, paris).hover();
	await tile(page, paris).locator('.check').click();
	await page.getByRole('button', { name: 'Select lyon-2023.jpg' }).click();
	const bar = page.getByRole('toolbar', { name: 'Selected photos' });
	await expect(bar).toContainText('2 selected');

	await bar.getByRole('button', { name: 'Add to album' }).click();
	await openDialog(page).getByRole('button', { name: 'New album…' }).click();
	const naming = openDialog(page);
	await expect(naming.getByRole('heading', { name: 'New album' })).toBeVisible();
	await naming.getByLabel('Name').fill('Summer');
	await naming.getByRole('button', { name: 'Create album' }).click();
	await expect(page.getByRole('note')).toHaveText('Added 2 photos to Summer.');

	await page.getByRole('link', { name: 'Albums', exact: true }).click();
	await page.getByRole('link', { name: /Summer/ }).click();
	await expect(page.getByRole('heading', { level: 1, name: 'Summer' })).toBeVisible();
	await expect(page.getByText('2 photos', { exact: true })).toBeVisible();
	await expect(tiles(page)).toHaveCount(2);

	await tile(page, lyon).hover();
	await tile(page, lyon).locator('.check').click();
	await page.getByRole('toolbar', { name: 'Selected photos' }).getByRole('button', { name: 'Remove from album' }).click();
	await expect(page.getByText('1 photo', { exact: true })).toBeVisible();
	await expect(tiles(page)).toHaveCount(1);

	await page.getByRole('button', { name: 'Rename', exact: true }).click();
	const renaming = openDialog(page);
	await renaming.getByLabel('Name').fill('Summer 2024');
	await renaming.getByRole('button', { name: 'Save' }).click();
	await expect(page.getByRole('heading', { level: 1, name: 'Summer 2024' })).toBeVisible();

	await page.getByRole('button', { name: 'Delete album', exact: true }).click();
	await openDialog(page).getByRole('button', { name: 'Delete album' }).click();
	await expect(page).toHaveURL(/\/photos\/albums$/);
	await expect(page.getByRole('link', { name: /Summer 2024/ })).toHaveCount(0);

	await page.goto('/photos');
	await expect(tiles(page)).toHaveCount(3);
});

test('the scrubber jumps between months from the keyboard', async () => {
	await page.goto('/photos');
	const slider = page.getByRole('slider', { name: 'Jump to month' });
	await expect(slider).toHaveAttribute('aria-valuetext', 'July 2024');
	await slider.focus();
	await page.keyboard.press('End');
	await expect(slider).toHaveAttribute('aria-valuetext', 'May 2023');
	await expect(page.getByRole('heading', { level: 2, name: 'May 2023' })).toBeInViewport();
});

test('another user sees none of these photos or albums', async ({ browser }) => {
	const album = await page.request.post('/api/albums', {
		headers: { 'X-CSRF-Token': await csrfToken(page) },
		data: { name: 'Private', node_ids: [paris] }
	});
	expect(album.status()).toBe(201);
	const albumId = ((await album.json()) as { id: string }).id;

	const { page: stranger } = await newUser(browser, 'stranger');
	await stranger.goto('/photos');
	await expect(stranger.getByText('No photos yet')).toBeVisible();
	for (const path of [`/api/files/${paris}/thumbnail?size=thumb`, `/api/photos/${paris}`, `/api/albums/${albumId}`]) {
		expect((await stranger.request.get(path)).status(), path).toBe(404);
	}
	await stranger.context().close();
});

test('a photo deleted from the lightbox leaves the timeline and comes back from the trash', async () => {
	await page.goto('/photos');
	await page.getByRole('button', { name: LYON_LABEL }).click();
	await lightbox(page).getByRole('button', { name: 'Move the photo taken 1 May 2023 09:00 to the trash' }).click();
	const confirm = openDialog(page).filter({ hasText: 'Move this photo to the trash?' });
	await confirm.getByRole('button', { name: 'Move to trash' }).click();
	// The viewer moves on to the neighbouring photo.
	await expect(lightbox(page)).toHaveAttribute('aria-label', 'Photo taken 14 July 2024 18:30');
	await page.keyboard.press('Escape');
	await expect(tiles(page)).toHaveCount(2);
	await expect(page.getByRole('heading', { level: 2, name: 'May 2023' })).toHaveCount(0);

	await page.goto('/trash');
	await page.getByRole('button', { name: 'Restore lyon-2023.jpg' }).click();
	await expect(page.getByRole('status').filter({ hasText: 'Restored “lyon-2023.jpg”.' })).toBeVisible();
	await page.goto('/photos');
	await expect(page.getByRole('heading', { level: 2, name: 'May 2023' })).toBeVisible();
	await expect(tiles(page)).toHaveCount(3);
});

test('file lists show thumbnails', async () => {
	await page.goto('/files');
	await row(page, 'Photos').getByRole('button', { name: 'Photos', exact: true }).click();
	await expect(row(page, 'paris-2024.jpg').locator('img')).toBeVisible();
});
