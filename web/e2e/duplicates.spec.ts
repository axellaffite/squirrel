import { expect, test, type Page } from '@playwright/test';
import { csrfToken } from './helpers/files.ts';
import { fixture, newUser, photoId, uploadPhotos, waitForTimeline, WORKER_TIMEOUT } from './helpers/photos.ts';
import { ML_TIMEOUT, requireModels, waitForMl } from './helpers/ml.ts';

// One fresh user with two copies of one scene (different size and quality) and an unrelated photo.
test.describe.configure({ mode: 'serial' });

let page: Page;
let group = '';
/** A query both copies match (generated scenes are soft, colourful blobs). */
const QUERY = 'colorful abstract blurry painting';

test.beforeAll(async ({ browser }) => {
	test.setTimeout(300_000);
	({ page } = await newUser(browser, 'twins'));
	await uploadPhotos(page, fixture('meadow.jpg'), fixture('meadow-copy.jpg'), fixture('paris-2024.jpg'));
	await waitForTimeline(page, 3);
});

test.afterAll(async () => {
	await page?.context().close();
});

interface Group {
	id: string;
	members: { asset: { name: string } }[];
}

async function openGroups(p: Page): Promise<Group[]> {
	return ((await (await p.request.get('/api/duplicates')).json()) as { items: Group[] }).items;
}

async function timelineNames(p: Page): Promise<string[]> {
	const { items } = (await (await p.request.get('/api/photos?limit=500')).json()) as { items: { name: string }[] };
	return items.map((item) => item.name).sort();
}

async function searchNames(p: Page, order: 'relevance' | 'date'): Promise<string[]> {
	const response = await p.request.get(`/api/search/photos?q=${encodeURIComponent(QUERY)}&order=${order}`);
	expect(response.status()).toBe(200);
	const { items } = (await response.json()) as { items: { name: string }[] };
	return items.map((item) => item.name).sort();
}

function mainPreview(p: Page) {
	return p.locator('[data-shown]');
}

function strip(p: Page) {
	return p.getByRole('list', { name: 'Photos in this group' });
}

test('the two copies are grouped, the other photo is not', async () => {
	await expect.poll(async () => (await openGroups(page)).length, { timeout: WORKER_TIMEOUT }).toBe(1);
	const [only] = await openGroups(page);
	group = only.id;
	expect(only.members.map((m) => m.asset.name).sort()).toEqual(['meadow-copy.jpg', 'meadow.jpg']);
	// Nothing is hidden until the user decides.
	expect(await timelineNames(page)).toEqual(['meadow-copy.jpg', 'meadow.jpg', 'paris-2024.jpg']);
});

test('the review shows the suggested keeper first and the strip and arrows move through the group', async () => {
	await page.goto('/explore');
	await page.getByRole('link', { name: 'Review' }).click();
	await expect(page).toHaveURL(/\/explore\/duplicates$/);
	await expect(page.getByRole('heading', { level: 1, name: 'Duplicates' })).toBeVisible();
	// The larger copy is suggested and shown first, with its facts.
	await expect(mainPreview(page)).toHaveAttribute('alt', 'meadow.jpg');
	await expect(page.getByText('Suggested')).toBeVisible();
	await expect(page.getByText('1600 × 1200')).toBeVisible();
	await expect(strip(page).getByRole('button')).toHaveCount(2);

	await strip(page).getByRole('button', { name: 'Show meadow-copy.jpg' }).click();
	await expect(mainPreview(page)).toHaveAttribute('alt', 'meadow-copy.jpg');
	await expect(page.getByText('1024 × 768')).toBeVisible();
	await page.keyboard.press('ArrowUp');
	await expect(mainPreview(page)).toHaveAttribute('alt', 'meadow.jpg');
	await page.keyboard.press('ArrowDown');
	await expect(mainPreview(page)).toHaveAttribute('alt', 'meadow-copy.jpg');
});

test('on a phone the strip sits under the main preview', async () => {
	await page.setViewportSize({ width: 390, height: 844 });
	await page.goto('/explore/duplicates');
	const preview = await mainPreview(page).boundingBox();
	const row = await strip(page).boundingBox();
	expect(preview && row && row.y > preview.y + preview.height - 1).toBe(true);
	await page.setViewportSize({ width: 1280, height: 800 });
});

test('search finds both copies before a decision', async () => {
	test.setTimeout(ML_TIMEOUT + 60_000);
	await requireModels(page, 'clip');
	await waitForMl(page);
	for (const order of ['relevance', 'date'] as const) {
		const names = await searchNames(page, order);
		expect(names, order).toContain('meadow.jpg');
		expect(names, order).toContain('meadow-copy.jpg');
	}
});

test('keeping the photo shown hides the other from the timeline and search', async () => {
	await page.goto('/explore/duplicates');
	await strip(page).getByRole('button', { name: 'Show meadow-copy.jpg' }).click();
	await page.getByRole('button', { name: 'Keep this' }).click();
	await expect(page.getByText('Kept meadow-copy.jpg; the other photos are hidden.')).toBeVisible();
	await expect(page.getByText('No duplicates to review')).toBeVisible();
	expect(await timelineNames(page)).toEqual(['meadow-copy.jpg', 'paris-2024.jpg']);
	// The hidden copy still opens by id.
	const hidden = await (await page.request.get(`/api/duplicates?state=resolved`)).json();
	expect(hidden.items[0].kept_id).toBe(await photoId(page, 'meadow-copy.jpg'));
	const status = await (await page.request.get('/api/media/status')).json();
	if (status.ml.models_installed.clip && status.ml.state === 'ready') {
		for (const order of ['relevance', 'date'] as const) {
			const names = await searchNames(page, order);
			expect(names, order).not.toContain('meadow.jpg');
			expect(names, order).toContain('meadow-copy.jpg');
		}
	}
});

test('restoring brings the hidden copy back', async () => {
	await page.goto('/explore/duplicates');
	await page.getByRole('radio', { name: 'Resolved' }).check();
	await strip(page).getByRole('button', { name: 'Show meadow.jpg (hidden)' }).click();
	await expect(page.getByText('Hidden', { exact: true })).toBeVisible();
	await page.getByRole('button', { name: 'Show again' }).click();
	await expect(page.getByText('Nothing hidden')).toBeVisible();
	expect(await timelineNames(page)).toEqual(['meadow-copy.jpg', 'meadow.jpg', 'paris-2024.jpg']);
	// Open again: the user decides anew.
	expect((await openGroups(page)).map((g) => g.id)).toEqual([group]);
});

test('another user sees none of it and cannot decide for her', async ({ browser }) => {
	const { page: bob } = await newUser(browser, 'stranger');
	await uploadPhotos(bob, fixture('meadow.jpg'));
	await waitForTimeline(bob, 1);
	expect(await openGroups(bob)).toEqual([]);
	await bob.goto('/explore/duplicates');
	await expect(bob.getByText('No duplicates to review')).toBeVisible();
	const response = await bob.request.post(`/api/duplicates/${group}/keep`, {
		headers: { 'X-CSRF-Token': await csrfToken(bob) },
		data: { asset: await photoId(bob, 'meadow.jpg') }
	});
	expect(response.status()).toBe(404);
	await bob.context().close();
});
