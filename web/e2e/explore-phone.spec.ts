import { expect, test } from '@playwright/test';
import { fixture, newUser, photoId, tiles, uploadPhotos, waitForTimeline } from './helpers/photos.ts';
import { requireModels, waitForMl } from './helpers/ml.ts';

test('on a phone, search, Explore and people fit the screen', async ({ browser }) => {
	test.setTimeout(300_000);
	const { page } = await newUser(browser, 'pocket');
	await uploadPhotos(page, fixture('red-circle.jpg'), fixture('blue-square.jpg'));
	await waitForTimeline(page, 2);
	await waitForMl(page);
	await requireModels(page, 'clip');
	const circle = await photoId(page, 'red-circle.jpg');

	// Search lives in the top row on phones, not in the bottom bar.
	await page.goto('/photos');
	await page.getByRole('link', { name: 'Open search' }).click();
	await page.getByRole('searchbox', { name: 'Search photos' }).fill('red circle');
	await expect(tiles(page).first()).toHaveAttribute('data-id', circle, { timeout: 60_000 });
	const width = await page.evaluate(() => document.documentElement.scrollWidth);
	expect(width).toBeLessThanOrEqual(390);

	// Explore is a Photos tab.
	await page.goto('/photos');
	await page.getByRole('navigation', { name: 'Photos' }).getByRole('link', { name: 'Explore' }).click();
	await expect(page.getByRole('heading', { level: 1, name: 'Explore' })).toBeVisible();
	await page.goto('/explore/people');
	await expect(page.getByRole('heading', { level: 1, name: 'People' })).toBeVisible();
	await page.context().close();
});
