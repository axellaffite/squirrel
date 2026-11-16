import { expect, test, type Page } from '@playwright/test';
import { openTopFolder, row, upload, uploadItem } from './helpers/files.ts';
import { fixture, newUser, serverHasPdfium, WORKER_TIMEOUT } from './helpers/photos.ts';

// One reader uploads both documents once; the text test always runs, the PDF one needs pdfium.
test.describe.configure({ mode: 'serial' });

let page: Page;

test.beforeAll(async ({ browser }) => {
	test.setTimeout(120_000);
	({ page } = await newUser(browser, 'reader'));
	await openTopFolder(page, 'Documents');
	await upload(page, fixture('notes.txt'), fixture('invoice.pdf'));
	for (const name of ['notes.txt', 'invoice.pdf']) await expect(uploadItem(page, name)).toContainText('Done');

	// Text is extracted in the background: wait for the queue rather than polling the search
	// (searches are rate limited per client).
	await expect
		.poll(
			async () => {
				const status = (await (await page.request.get('/api/media/status')).json()) as {
					queued: number;
					processing: number;
				};
				return status.queued + status.processing;
			},
			{ timeout: WORKER_TIMEOUT }
		)
		.toBe(0);
});

test.afterAll(async () => {
	await page?.context().close();
});

function results(query: RegExp) {
	return page.getByRole('list', { name: 'Search results' }).getByRole('listitem').filter({ hasText: query });
}

test('finds a text file by words of its content, from the Files page', async () => {
	await openTopFolder(page, 'Documents');
	const field = page.getByRole('searchbox', { name: 'Search files' });
	await field.fill('squir budg');
	await field.press('Enter');
	await expect(page).toHaveURL(/\/files\/search\?q=squir/);
	const result = results(/notes\.txt/);
	await expect(result).toBeVisible();
	await expect(result.locator('mark').filter({ hasText: /^squirrel$/i })).toBeVisible();
});

test('finds a PDF by its text and shows its first page as a thumbnail', async () => {
	test.skip(!serverHasPdfium(), 'pdfium is not installed (scripts/fetch-pdfium.sh): PDFs are unsupported');
	const listing = (await (await page.request.get('/api/nodes')).json()) as { items: { id: string; name: string }[] };
	const documents = listing.items.find((node) => node.name === 'Documents')!;
	await expect
		.poll(
			async () => {
				const children = (await (await page.request.get(`/api/nodes?parent_id=${documents.id}`)).json()) as {
					items: { name: string; thumbnail?: unknown }[];
				};
				return children.items.find((node) => node.name === 'invoice.pdf')?.thumbnail != null;
			},
			{ timeout: WORKER_TIMEOUT }
		)
		.toBe(true);

	await page.goto('/files/search?q=acorn%20stor');
	const pdf = results(/invoice\.pdf/);
	await expect(pdf).toBeVisible();
	await expect(pdf.locator('img')).toBeVisible();

	await openTopFolder(page, 'Documents');
	await expect(row(page, 'invoice.pdf').locator('img')).toBeVisible();
});
