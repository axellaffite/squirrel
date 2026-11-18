import { expect, test, type Page } from '@playwright/test';
import { openDialog } from './helpers/files.ts';
import { fixture, newUser, photoId, tiles, uploadPhotos, waitForTimeline } from './helpers/photos.ts';
import { people, portrait, portraits, requireModels, waitForMl, waitForPeople } from './helpers/ml.ts';

// One fresh user's portraits (two people, four photos each), corrected step by step.
test.describe.configure({ mode: 'serial' });

let page: Page;
let ada = '';

test.beforeAll(async ({ browser }) => {
	test.setTimeout(300_000);
	({ page } = await newUser(browser, 'family'));
	await requireModels(page, 'faces');
	await uploadPhotos(page, ...portraits(), fixture('red-circle.jpg'));
	await waitForTimeline(page, 9);
	await waitForMl(page);
});

test.afterAll(async () => {
	await page?.context().close();
});

function grid(p: Page) {
	return p.getByRole('list', { name: 'People' });
}

test('clustering makes two people', async () => {
	await requireModels(page, 'faces');
	await waitForPeople(page, 2);
	await page.goto('/explore/people');
	await expect(grid(page).getByRole('link')).toHaveCount(2);
});

test('a person is renamed on their page', async () => {
	await requireModels(page, 'faces');
	await page.goto('/explore/people');
	await grid(page).getByRole('link').first().click();
	await expect(page).toHaveURL(/\/explore\/people\/[0-9a-f-]{36}$/);
	ada = page.url().split('/').pop() ?? '';
	await page.getByRole('button', { name: 'Add a name' }).click();
	await page.getByLabel(/^Name of /).fill('Ada');
	await page.keyboard.press('Enter');
	await expect(page.getByRole('heading', { level: 1, name: 'Ada' })).toBeVisible();
	await expect(tiles(page)).toHaveCount(4);
});

test('the lightbox shows her face box, and "Not Ada" takes the face off her page', async () => {
	await requireModels(page, 'faces');
	await page.goto(`/explore/people/${ada}`);
	await tiles(page).first().click();
	await page.getByRole('button', { name: /^Show the faces/ }).click();
	await page.getByRole('button', { name: 'Face of Ada' }).click();
	await openDialog(page).getByRole('button', { name: 'Not Ada' }).click();
	await expect.poll(async () => (await people(page)).find((p) => p.id === ada)?.face_count).toBe(3);
	await expect(page.getByRole('button', { name: 'Unnamed face' })).toBeVisible();
	await page.keyboard.press('Escape');
});

test('another user never sees her people, tags or places, even holding the same photo', async ({ browser }) => {
	await requireModels(page, 'faces');
	await requireModels(page, 'clip');
	const { page: bob } = await newUser(browser, 'neighbour');
	// The same bytes as two of hers: a portrait and the red circle.
	await uploadPhotos(bob, portrait('koch-a'), fixture('red-circle.jpg'));
	await waitForTimeline(bob, 2);
	await waitForMl(bob);
	const bobsPhoto = await photoId(bob, 'koch-a.jpg');
	expect((await bob.request.get(`/api/people/${ada}`)).status()).toBe(404);
	// One face of his own is not a person yet (three are needed): his people are empty.
	expect(await people(bob, true)).toEqual([]);
	// The shared photo's details show no person of hers.
	const details = (await (await bob.request.get(`/api/photos/${bobsPhoto}`)).json()) as { people: { id: string }[] };
	expect(details.people).toEqual([]);
	const faces = (await (await bob.request.get(`/api/photos/${bobsPhoto}/faces`)).json()) as { person: unknown }[];
	expect(faces.every((face) => face.person === null)).toBe(true);
	expect(await (await bob.request.get('/api/places')).json()).toEqual([]);
	// His tags (the red circle has some) only ever lead to his own two photos.
	const own = new Set([bobsPhoto, await photoId(bob, 'red-circle.jpg')]);
	const tags = (await (await bob.request.get('/api/tags')).json()) as { tag: string; count: number }[];
	expect(tags.length).toBeGreaterThan(0);
	for (const { tag, count } of tags) {
		expect(count).toBeLessThanOrEqual(2);
		const tagged = (await (await bob.request.get(`/api/photos?tag=${tag}`)).json()) as { items: { id: string }[] };
		expect(tagged.items.every((item) => own.has(item.id))).toBe(true);
	}
	const filtered = (await (await bob.request.get(`/api/photos?person=${ada}`)).json()) as { items: unknown[] };
	expect(filtered.items).toEqual([]);
	// His search sees only his own photos, never her others.
	const found = (await (await bob.request.get('/api/search/photos?q=astronaut%20portrait')).json()) as {
		items: { id: string }[];
	};
	expect(found.items.every((item) => own.has(item.id))).toBe(true);
	await bob.goto('/explore/people');
	await expect(bob.getByText('Ada', { exact: true })).toHaveCount(0);
	await bob.context().close();
});

test('a hidden person leaves the grid until hidden people are shown', async () => {
	await requireModels(page, 'faces');
	await page.goto(`/explore/people/${ada}`);
	await page.getByRole('button', { name: 'Hide' }).click();
	await expect(page.getByRole('button', { name: 'Unhide' })).toBeVisible();
	await page.goto('/explore/people');
	await expect(grid(page).getByRole('link', { name: /Ada/ })).toHaveCount(0);
	await page.getByRole('switch', { name: 'Show hidden people' }).click();
	await expect(grid(page).getByRole('link', { name: /Ada/ })).toBeVisible();
	await grid(page).getByRole('link', { name: /Ada/ }).click();
	await page.getByRole('button', { name: 'Unhide' }).click();
	await expect(page.getByRole('button', { name: 'Hide' })).toBeVisible();
});

test('two people merge into one, and split back', async () => {
	await requireModels(page, 'faces');
	await page.goto('/explore/people');
	await page.getByRole('button', { name: 'Merge people' }).click();
	await expect(grid(page).getByRole('button')).toHaveCount(2);
	for (const person of await grid(page).getByRole('button').all()) await person.click();
	await page.getByRole('toolbar', { name: 'Selected people' }).getByRole('button', { name: 'Merge' }).click();
	// Ada is kept by default (she is named), and the choice is the user's.
	await expect(openDialog(page).getByRole('radio', { name: /Ada/ })).toBeChecked();
	await expect(openDialog(page)).toContainText('Ada is selected by default. Chosen because they are the only one with a name.');
	await openDialog(page).getByRole('radio', { name: /Unnamed/ }).check();
	await expect(openDialog(page).getByRole('button', { name: 'Merge into Unnamed' })).toBeVisible();
	await openDialog(page).getByRole('radio', { name: /Ada/ }).check();
	await openDialog(page).getByRole('button', { name: 'Merge into Ada' }).click();
	await expect.poll(async () => (await people(page)).length).toBe(1);

	await page.goto(`/explore/people/${ada}`);
	await page.getByRole('tab', { name: 'Faces' }).click();
	const faces = page.getByRole('list', { name: 'Faces of Ada' }).getByRole('button');
	await expect(faces).toHaveCount(7);
	await faces.nth(0).click();
	await faces.nth(1).click();
	await page.getByRole('toolbar', { name: 'Selected faces' }).getByRole('button', { name: 'Move to…' }).click();
	await openDialog(page).getByRole('button', { name: 'New person' }).click();
	await expect.poll(async () => (await people(page)).length).toBe(2);
	await expect(faces).toHaveCount(5);
});

test('turning faces off deletes them all; turning them on finds the people again', async () => {
	await requireModels(page, 'faces');
	await page.goto('/settings/faces');
	const toggle = page.getByRole('switch', { name: 'Recognise faces in my photos' });
	await expect(toggle).toBeChecked();
	await toggle.click();
	// The question states what is deleted, counted when it opens; until confirmed, nothing is.
	await expect(openDialog(page)).toContainText('This deletes 2 people and 8 faces');
	await expect(toggle).toBeChecked();
	await openDialog(page).getByRole('button', { name: 'Cancel' }).click();
	await expect(openDialog(page)).toHaveCount(0);
	await expect(toggle).toBeChecked();
	expect(await people(page)).toHaveLength(2);
	await toggle.click();
	await openDialog(page).getByRole('button', { name: 'Turn off and delete' }).click();
	await expect(toggle).not.toBeChecked();
	await expect.poll(async () => people(page)).toEqual([]);
	await page.goto('/explore/people');
	await expect(page.getByText('Face recognition is off')).toBeVisible();

	await page.goto('/settings/faces');
	await page.getByRole('switch', { name: 'Recognise faces in my photos' }).click();
	await expect(page.getByRole('switch', { name: 'Recognise faces in my photos' })).toBeChecked();
	const found = await waitForPeople(page, 2);
	expect(found.map((p) => p.name)).toEqual([null, null]);
});
