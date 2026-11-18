import { expect, test, type Page } from '@playwright/test';
import { fixture, newUser, photoId, tiles, uploadPhotos, waitForTimeline } from './helpers/photos.ts';
import { pet, requireModels, waitForMl } from './helpers/ml.ts';

// One fresh user's library, analysed once, then searched and explored.
test.describe.configure({ mode: 'serial' });

let page: Page;

test.beforeAll(async ({ browser }) => {
	test.setTimeout(300_000);
	({ page } = await newUser(browser, 'searcher'));
	await requireModels(page, 'clip');
	await uploadPhotos(
		page,
		fixture('red-circle.jpg'),
		fixture('blue-square.jpg'),
		fixture('green-triangle.jpg'),
		fixture('Screenshot_2024-07-15-10-00-00.png'),
		pet('dog-a'),
		pet('cat-a')
	);
	await waitForTimeline(page, 6);
	await waitForMl(page);
});

test.afterAll(async () => {
	await page?.context().close();
});

test('a text search finds the photo that shows it, best match first', async () => {
	await requireModels(page, 'clip');
	const circle = await photoId(page, 'red-circle.jpg');
	await page.goto('/search?q=red%20circle');
	await expect(tiles(page).first()).toBeVisible({ timeout: 60_000 });
	await expect(tiles(page).first()).toHaveAttribute('data-id', circle);
	await expect(page.getByText(/^\d+( or more)? photos?$/)).toBeAttached();
});

test('search is multilingual and a dog query leaves the cat out', async () => {
	await requireModels(page, 'clip');
	const circle = await photoId(page, 'red-circle.jpg');
	await page.goto('/search?q=cercle%20rouge');
	await expect(tiles(page).first()).toHaveAttribute('data-id', circle, { timeout: 60_000 });
	const dog = await photoId(page, 'dog-a.jpg');
	const cat = await photoId(page, 'cat-a.jpg');
	await page.goto('/search?q=chien');
	await expect(tiles(page).first()).toHaveAttribute('data-id', dog, { timeout: 60_000 });
	await expect(page.getByText(/^\d+( or more)? photos?$/)).toBeAttached();
	await expect(page.locator(`[data-tile][data-id="${cat}"]`)).toHaveCount(0);
});

test('a filter chip narrows the search', async () => {
	await requireModels(page, 'clip');
	const circle = await photoId(page, 'red-circle.jpg');
	await page.goto('/search?q=red%20circle&kind=screenshot');
	await expect(page.getByRole('button', { name: 'Remove filter Screenshots' })).toBeVisible();
	// Settled first (a count or the empty state), so the absence below means something.
	await expect(page.getByText(/^(\d+( or more)? photos?|No photos match “red circle”)$/).first()).toBeAttached({ timeout: 60_000 });
	await expect(page.locator(`[data-tile][data-id="${circle}"]`)).toHaveCount(0);
	await page.getByRole('button', { name: 'Remove filter Screenshots' }).click();
	await expect(page.locator(`[data-tile][data-id="${circle}"]`)).toBeVisible({ timeout: 60_000 });
});

test('Explore › Tags lists tags that filter the timeline', async () => {
	await requireModels(page, 'clip');
	await page.goto('/explore/tags');
	const cards = page.getByRole('list', { name: 'Tags' }).getByRole('link');
	await expect(cards.first()).toBeVisible();
	const name = (await cards.first().locator('.name').textContent())?.trim() ?? '';
	await cards.first().click();
	await expect(page).toHaveURL(/\/photos\?tag=/);
	await expect(page.getByRole('button', { name: `Remove filter ${name}` })).toBeVisible();
	await expect(tiles(page).first()).toBeVisible();
});
