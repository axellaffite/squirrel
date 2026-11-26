import { expect, test, type Page } from '@playwright/test';
import { csrfToken } from './helpers/files.ts';
import { fixture, lightbox, newUser, photoId, tile, uploadPhotos, waitForTimeline } from './helpers/photos.ts';

// One user whose filter blurs photos taken in 2024: Paris is blurred, Lyon (2023) is not.
test.describe.configure({ mode: 'serial' });

let page: Page;
let paris: string;
let lyon: string;

test.beforeAll(async ({ browser }) => {
	test.setTimeout(120_000);
	({ page } = await newUser(browser, 'discreet'));
	await uploadPhotos(page, fixture('paris-2024.jpg'), fixture('lyon-2023.jpg'));
	await waitForTimeline(page, 2);
	paris = await photoId(page, 'paris-2024.jpg');
	lyon = await photoId(page, 'lyon-2023.jpg');
	const saved = await page.request.put('/api/me/blur-filter', {
		headers: { 'X-CSRF-Token': await csrfToken(page) },
		data: { enabled: true, rules: { taken_from: '2024-01-01' } }
	});
	expect(saved.status()).toBe(200);
});

test.afterAll(async () => {
	await page?.context().close();
});

test('matching photos are blurred in the timeline, others are not', async () => {
	await page.goto('/photos');
	await expect(tile(page, paris)).toHaveAttribute('aria-label', /^Open blurred photo/);
	await expect(tile(page, paris).locator('img')).toHaveCount(0);
	await expect(tile(page, lyon).locator('img')).toHaveCount(1);
});

test('opening a blurred photo explains why, and Show reveals it', async () => {
	await page.goto('/photos');
	await tile(page, paris).click();
	const viewer = lightbox(page);
	await expect(viewer.getByText('Hidden by your blur filter')).toBeVisible();
	await expect(viewer.locator('img[alt="paris-2024.jpg"]')).toHaveCount(0);
	await viewer.getByRole('button', { name: 'Show', exact: true }).click();
	await expect(viewer.locator('img[alt="paris-2024.jpg"]')).toBeVisible();
	await page.keyboard.press('Escape');
	await expect(tile(page, paris).locator('img'), 'the tile is revealed too').toHaveCount(1);
});

test('the timeline shows or blurs every matching photo at once', async () => {
	await page.goto('/photos');
	await expect(tile(page, paris).locator('img')).toHaveCount(0);
	await page.getByRole('button', { name: 'Show blurred' }).click();
	await expect(tile(page, paris).locator('img')).toHaveCount(1);
	await page.getByRole('button', { name: 'Blur again' }).click();
	await expect(tile(page, paris).locator('img')).toHaveCount(0);
});

test('an album cover matching the filter is blurred', async () => {
	const album = await page.request.post('/api/albums', {
		headers: { 'X-CSRF-Token': await csrfToken(page) },
		data: { name: 'Summer', node_ids: [paris] }
	});
	expect(album.status()).toBe(201);
	await page.goto('/photos/albums');
	const card = page.locator('.card').filter({ hasText: 'Summer' });
	await expect(card).toBeVisible();
	await expect(card.locator('img')).toHaveCount(0);
});

test('settings edit the filter, and turning it off shows every photo', async () => {
	await page.goto('/settings/blur-filter');
	const toggle = page.getByRole('switch', { name: 'Blur photos matching this filter' });
	await expect(toggle).toHaveAttribute('aria-checked', 'true');
	await expect(page.getByText('1 photo matches')).toBeVisible();
	await toggle.click();
	await page.getByRole('button', { name: 'Save' }).click();
	await expect(page.getByText('Blur filter saved, turned off.').last()).toBeVisible();
	await page.goto('/photos');
	await expect(tile(page, paris).locator('img')).toHaveCount(1);
	await expect(page.getByRole('button', { name: 'Show blurred' })).toHaveCount(0);
});
