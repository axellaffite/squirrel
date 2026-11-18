import { expect, test } from '@playwright/test';
import { fixture, newUser, photoId, tiles, uploadPhotos, waitForTimeline } from './helpers/photos.ts';
import { portraits, requireModels, status, waitForMl, waitForPeople } from './helpers/ml.ts';

test('on a phone, search, Explore and people fit the screen', async ({ browser }) => {
	test.setTimeout(420_000);
	const { page } = await newUser(browser, 'pocket');
	await requireModels(page, 'clip');
	await uploadPhotos(page, fixture('red-circle.jpg'), fixture('blue-square.jpg'));
	await waitForTimeline(page, 2);
	await waitForMl(page);
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
	if ((await status(page)).ml.models_installed.faces) {
		await uploadPhotos(page, ...portraits());
		await waitForMl(page);
		await waitForPeople(page, 2);
		await page.goto('/explore/people');
		await expect(page.getByRole('list', { name: 'People' }).getByRole('link')).toHaveCount(2);
		expect(await page.evaluate(() => document.documentElement.scrollWidth)).toBeLessThanOrEqual(390);
	}
	await page.context().close();
});
