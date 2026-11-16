import { randomUUID } from 'node:crypto';
import { existsSync, readFileSync } from 'node:fs';
import { fileURLToPath } from 'node:url';
import { expect, type Browser, type Locator, type Page } from '@playwright/test';
import { csrfToken, login, PASSWORD, type UploadFile } from './files.ts';

/** Worker output (thumbnails, places, text) is awaited at most this long. */
export const WORKER_TIMEOUT = 60_000;

const FIXTURES = fileURLToPath(new URL('../fixtures/', import.meta.url));
const MIME: Record<string, string> = {
	jpg: 'image/jpeg',
	png: 'image/png',
	txt: 'text/plain',
	pdf: 'application/pdf'
};

/** A generated fixture from `web/e2e/fixtures` (see its README). */
export function fixture(name: string): UploadFile {
	const extension = name.split('.').pop() ?? '';
	return { name, mimeType: MIME[extension] ?? 'application/octet-stream', buffer: readFileSync(`${FIXTURES}${name}`) };
}

/** Whether the E2E server runs with pdfium (same rule as scripts/e2e-server.sh). */
export function serverHasPdfium(): boolean {
	if (process.env.SQUIRREL_PDFIUM_DIR) return true;
	return existsSync(fileURLToPath(new URL('../../../target/pdfium/lib', import.meta.url)));
}

/**
 * A brand-new user invited by alice, logged in in its own browser context. Photo scenarios run
 * as fresh users so other specs' uploads (images in alice's drive) never show in their timeline.
 */
export async function newUser(browser: Browser, label: string): Promise<{ page: Page; username: string }> {
	const admin = await login(browser, 'alice');
	const invitation = await admin.request.post('/api/invitations', {
		headers: { 'X-CSRF-Token': await csrfToken(admin) },
		data: {}
	});
	expect(invitation.status()).toBe(201);
	const { token } = (await invitation.json()) as { token: string };
	await admin.context().close();

	const username = `${label}-${randomUUID().slice(0, 8)}`;
	const page = await (await browser.newContext()).newPage();
	const joined = await page.request.post(`/api/join/${token}`, {
		data: { username, display_name: label, password: PASSWORD }
	});
	expect(joined.status()).toBe(201);
	await page.goto('/');
	await expect(page.getByRole('heading', { name: `Hello, ${label}` })).toBeVisible();
	return { page, username };
}

/** Uploads through the Photos page's Upload button (into the Photos folder). */
export async function uploadPhotos(page: Page, ...files: UploadFile[]): Promise<void> {
	await page.goto('/photos');
	const chooser = page.waitForEvent('filechooser');
	await page.getByRole('button', { name: 'Upload photos to Photos' }).click();
	await (await chooser).setFiles(files);
}

interface MonthCount {
	month: string;
	count: number;
}

/** Waits until the timeline holds `count` photos and nothing is still being prepared. */
export async function waitForTimeline(page: Page, count: number): Promise<void> {
	await expect
		.poll(
			async () => {
				const months = (await (await page.request.get('/api/photos/months')).json()) as MonthCount[];
				const status = (await (await page.request.get('/api/media/status')).json()) as {
					queued: number;
					processing: number;
				};
				return { photos: months.reduce((sum, m) => sum + m.count, 0), pending: status.queued + status.processing };
			},
			{ timeout: WORKER_TIMEOUT }
		)
		.toEqual({ photos: count, pending: 0 });
}

interface Summary {
	id: string;
	name: string;
}

/** The timeline id (file node id) of the photo named `name`. */
export async function photoId(page: Page, name: string): Promise<string> {
	const response = await page.request.get('/api/photos?limit=500');
	const { items } = (await response.json()) as { items: Summary[] };
	const found = items.find((item) => item.name === name);
	expect(found, `${name} in the timeline`).toBeTruthy();
	return found!.id;
}

export function tiles(page: Page): Locator {
	return page.locator('[data-tile]');
}

export function tile(page: Page, id: string): Locator {
	return page.locator(`[data-tile][data-id="${id}"]`);
}

/** The lightbox (a modal dialog named after the photo). */
export function lightbox(page: Page): Locator {
	return page.locator('dialog.lightbox[open]');
}
