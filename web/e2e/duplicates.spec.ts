import { expect, test, type Page } from '@playwright/test';
import { csrfToken } from './helpers/files.ts';
import { fixture, lightbox, newUser, photoId, tiles, uploadPhotos, waitForTimeline, WORKER_TIMEOUT } from './helpers/photos.ts';
import { ML_TIMEOUT, requireModels, waitForMl } from './helpers/ml.ts';

// One fresh user with two copies of one scene (different size and quality) and an unrelated
// photo; a second pair arrives later for stepping between groups.
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

/** The photo the standard viewer shows (its preview's alt is the file name). */
function shownPhoto(p: Page) {
	return lightbox(p).getByTestId('lightbox-stage').locator('img');
}

function panel(p: Page) {
	return lightbox(p).getByRole('complementary', { name: 'Near-identical photos' });
}

function groupPhotos(p: Page) {
	return panel(p).getByRole('list', { name: 'Photos in this group' }).getByRole('button');
}

async function openReview(p: Page) {
	await p.goto('/explore/duplicates');
	await expect(p.getByRole('heading', { level: 1, name: 'Duplicates' })).toBeVisible();
}

test('the two copies are grouped, the other photo is not', async () => {
	await expect.poll(async () => (await openGroups(page)).length, { timeout: WORKER_TIMEOUT }).toBe(1);
	const [only] = await openGroups(page);
	group = only.id;
	expect(only.members.map((m) => m.asset.name).sort()).toEqual(['meadow-copy.jpg', 'meadow.jpg']);
	// Nothing is hidden until the user decides.
	expect(await timelineNames(page)).toEqual(['meadow-copy.jpg', 'meadow.jpg', 'paris-2024.jpg']);
});

test('a grid tile opens the standard viewer, whose panel lists the group and switches the photo', async () => {
	await page.goto('/explore');
	await page.getByRole('link', { name: 'Review' }).click();
	await expect(page).toHaveURL(/\/explore\/duplicates$/);
	await expect(page.getByRole('radio', { name: 'To review (1)' })).toBeChecked();
	// One tile per group: the suggested keeper (the larger copy), with the group's size.
	await expect(tiles(page)).toHaveCount(1);
	await expect(tiles(page).first()).toHaveAccessibleName(/one of 2 near-identical photos$/);
	await expect(tiles(page).first()).toContainText('×2');
	await tiles(page).first().click();

	await expect(lightbox(page)).toBeVisible();
	await expect(shownPhoto(page)).toHaveAttribute('alt', 'meadow.jpg');
	// The viewer's own tools are there (details, faces, download).
	await expect(lightbox(page).getByRole('button', { name: /^Details of the photo/ })).toBeVisible();
	await expect(groupPhotos(page)).toHaveCount(2);
	await expect(panel(page).getByRole('button', { name: 'Show meadow.jpg (suggested)' })).toHaveAttribute('aria-current', 'true');

	await panel(page).getByRole('button', { name: 'Show meadow-copy.jpg' }).click();
	await expect(shownPhoto(page)).toHaveAttribute('alt', 'meadow-copy.jpg');
	await expect(panel(page).getByRole('button', { name: 'Show meadow-copy.jpg' })).toHaveAttribute('aria-current', 'true');
	await page.keyboard.press('ArrowUp');
	await expect(shownPhoto(page)).toHaveAttribute('alt', 'meadow.jpg');
	await page.keyboard.press('ArrowDown');
	await expect(shownPhoto(page)).toHaveAttribute('alt', 'meadow-copy.jpg');
	await page.keyboard.press('Escape');
	await expect(lightbox(page)).toHaveCount(0);
});

test('on a phone the group is a strip under the photo', async () => {
	await page.setViewportSize({ width: 390, height: 844 });
	await openReview(page);
	await tiles(page).first().click();
	const stage = await lightbox(page).getByTestId('lightbox-stage').boundingBox();
	const strip = await panel(page).boundingBox();
	expect(stage && strip && strip.y >= stage.y + stage.height - 1).toBe(true);
	await page.keyboard.press('Escape');
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
	await openReview(page);
	await tiles(page).first().click();
	await panel(page).getByRole('button', { name: 'Show meadow-copy.jpg' }).click();
	await panel(page).getByRole('button', { name: 'Keep this' }).click();
	// No group left: the viewer closes.
	await expect(lightbox(page)).toHaveCount(0);
	await expect(page.getByText('Kept meadow-copy.jpg; the other photos are hidden.')).toBeVisible();
	await expect(page.getByText('No duplicates to review')).toBeVisible();
	expect(await timelineNames(page)).toEqual(['meadow-copy.jpg', 'paris-2024.jpg']);
	const resolved = await (await page.request.get('/api/duplicates?state=resolved')).json();
	expect(resolved.items[0].kept_id).toBe(await photoId(page, 'meadow-copy.jpg'));
	const status = await (await page.request.get('/api/media/status')).json();
	if (status.ml.models_installed.clip && status.ml.state === 'ready') {
		for (const order of ['relevance', 'date'] as const) {
			const names = await searchNames(page, order);
			expect(names, order).not.toContain('meadow.jpg');
			expect(names, order).toContain('meadow-copy.jpg');
		}
	}
});

test('the resolved tab shows a hidden copy again', async () => {
	await openReview(page);
	await page.getByRole('radio', { name: 'Resolved' }).check();
	await tiles(page).first().click();
	await panel(page).getByRole('button', { name: 'Show meadow.jpg (hidden)' }).click();
	await expect(shownPhoto(page)).toHaveAttribute('alt', 'meadow.jpg');
	await panel(page).getByRole('button', { name: 'Show again' }).click();
	await expect(lightbox(page)).toHaveCount(0);
	await expect(page.getByText('Nothing hidden')).toBeVisible();
	expect(await timelineNames(page)).toEqual(['meadow-copy.jpg', 'meadow.jpg', 'paris-2024.jpg']);
	// Open again: the user decides anew.
	expect((await openGroups(page)).map((g) => g.id)).toEqual([group]);
});

test('left and right step between groups; keeping one moves on to the next group', async () => {
	await uploadPhotos(page, fixture('pond.jpg'), fixture('pond-copy.jpg'));
	await waitForTimeline(page, 5);
	await expect.poll(async () => (await openGroups(page)).length, { timeout: WORKER_TIMEOUT }).toBe(2);
	await openReview(page);
	await expect(tiles(page)).toHaveCount(2);
	await tiles(page).first().click();
	const first = (await shownPhoto(page).getAttribute('alt')) ?? '';
	const other = first.startsWith('pond') ? 'meadow.jpg' : 'pond.jpg';
	await page.keyboard.press('ArrowRight');
	await expect(shownPhoto(page)).toHaveAttribute('alt', other);
	await page.keyboard.press('ArrowLeft');
	await expect(shownPhoto(page)).toHaveAttribute('alt', first);
	// Keep the shown photo of the first group: the viewer goes on to the other group.
	await panel(page).getByRole('button', { name: 'Keep this' }).click();
	await expect(shownPhoto(page)).toHaveAttribute('alt', other);
	await expect(groupPhotos(page)).toHaveCount(2);
	await page.keyboard.press('Escape');
	await expect(tiles(page)).toHaveCount(1);
	await expect(page.getByRole('radio', { name: 'To review (1)' })).toBeChecked();
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
