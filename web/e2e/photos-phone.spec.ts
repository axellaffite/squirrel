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

async function swipe(target: Locator, dx: number): Promise<void> {
	const box = (await target.boundingBox())!;
	const y = box.y + box.height / 2;
	const from = box.x + box.width / 2;
	await target.dispatchEvent('pointerdown', { pointerType: 'touch', isPrimary: true, clientX: from, clientY: y });
	await target.dispatchEvent('pointerup', { pointerType: 'touch', isPrimary: true, clientX: from + dx, clientY: y });
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
	await swipe(viewer.locator('.stage'), -120);
	await expect(viewer).toHaveAttribute('aria-label', 'Photo taken 1 May 2023 09:00');
	await page.context().close();
});
