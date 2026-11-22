import { expect, test, type Locator } from '@playwright/test';
import { fixture, lightbox, newUser, photoId, tile, uploadPhotos, waitForTimeline } from './helpers/photos.ts';

// Runs at phone width with touch (see playwright.config.ts).

/** A finger pressing `target` until the long press fires; the caller waits for its effect. */
async function pressStart(target: Locator): Promise<{ x: number; y: number }> {
	const box = (await target.boundingBox())!;
	const point = { x: box.x + box.width / 2, y: box.y + box.height / 2 };
	await target.dispatchEvent('pointerdown', { pointerType: 'touch', isPrimary: true, clientX: point.x, clientY: point.y });
	return point;
}

/** A horizontal finger swipe: down, a move halfway, then up `dx` pixels away. */
async function swipe(target: Locator, dx: number): Promise<void> {
	const box = (await target.boundingBox())!;
	const y = box.y + box.height / 2;
	const from = box.x + box.width / 2;
	const touch = { pointerType: 'touch', isPrimary: true, pointerId: 1, clientY: y };
	await target.dispatchEvent('pointerdown', { ...touch, clientX: from });
	await target.dispatchEvent('pointermove', { ...touch, clientX: from + dx / 2 });
	await target.dispatchEvent('pointerup', { ...touch, clientX: from + dx });
}

test('photos on a phone: the bottom bar, long-press selection and swiping in the lightbox', async ({ browser }) => {
	test.setTimeout(120_000);
	const { page } = await newUser(browser, 'pocket');
	await uploadPhotos(page, fixture('paris-2024.jpg'), fixture('lyon-2023.jpg'));
	await waitForTimeline(page, 2);
	const paris = await photoId(page, 'paris-2024.jpg');

	await page.goto('/photos');
	const nav = page.getByRole('navigation', { name: 'Main' });
	await expect(nav.getByRole('link', { name: 'Photos' })).toHaveAttribute('aria-current', 'page');

	// Long press: selection mode, without a context menu.
	const parisTile = tile(page, paris);
	const point = await pressStart(parisTile);
	const bar = page.getByRole('toolbar', { name: 'Selected photos' });
	await expect(bar).toContainText('1 selected');
	await parisTile.dispatchEvent('pointerup', { pointerType: 'touch', isPrimary: true, clientX: point.x, clientY: point.y });
	await bar.getByRole('button', { name: 'Clear the selection' }).click();
	await expect(bar).toHaveCount(0);

	// A tap opens the lightbox; swiping left shows the next (older) photo.
	await page.getByRole('button', { name: 'Open photo taken 14 July 2024 18:30' }).click();
	const viewer = lightbox(page);
	await expect(viewer).toHaveAttribute('aria-label', 'Photo taken 14 July 2024 18:30');
	await swipe(viewer.getByTestId('lightbox-stage'), -120);
	await expect(viewer).toHaveAttribute('aria-label', 'Photo taken 1 May 2023 09:00');
	await page.context().close();
});

test('the lightbox asks for a preview sized to the phone, which no other user can read', async ({ browser }) => {
	test.setTimeout(120_000);
	const { page } = await newUser(browser, 'sized');
	await uploadPhotos(page, fixture('paris-2024.jpg'));
	await waitForTimeline(page, 1);
	await page.goto('/photos');
	await page.getByRole('button', { name: 'Open photo taken 14 July 2024 18:30' }).click();

	// 1200 × 800 fitted to a 390 px wide screen: the smallest bucket, never upscaled.
	const image = lightbox(page).locator('img[src*="size=preview"]');
	await expect(image).toHaveAttribute('src', /&px=1440$/);
	await expect.poll(() => image.evaluate((img: HTMLImageElement) => img.naturalWidth)).toBe(1200);
	const source = (await image.getAttribute('src'))!;
	const preview = await page.request.get(source);
	expect(preview.status()).toBe(200);
	expect(preview.headers()['content-type']).toBe('image/jpeg');
	expect(preview.headers()['cache-control']).toBe('private, max-age=604800');
	expect(preview.headers()['etag']).toMatch(/-p1440-r1"$/);

	const stranger = await newUser(browser, 'stranger');
	const denied = await stranger.page.request.get(source);
	expect(denied.status()).toBe(404);
	expect(((await denied.json()) as { error: { code: string } }).error.code).toBe('not_found');
	await stranger.page.context().close();
	await page.context().close();
});
