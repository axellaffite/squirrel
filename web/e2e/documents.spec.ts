import { expect, test } from '@playwright/test';
import { openTopFolder, row, upload, uploadItem } from './helpers/files.ts';
import { fixture, newUser, serverHasPdfium, WORKER_TIMEOUT } from './helpers/photos.ts';

interface Hit {
	node: { name: string };
}

test('searches file names and document text', async ({ browser }) => {
	test.setTimeout(120_000);
	const { page } = await newUser(browser, 'reader');
	await openTopFolder(page, 'Documents');
	await upload(page, fixture('notes.txt'), fixture('invoice.pdf'));
	for (const name of ['notes.txt', 'invoice.pdf']) await expect(uploadItem(page, name)).toContainText('Done');

	// Text is extracted in the background: wait for the queue rather than polling the search
	// (searches are rate limited per client).
	await expect
		.poll(
			async () => {
				const status = (await (await page.request.get('/api/media/status')).json()) as { queued: number; processing: number };
				return status.queued + status.processing;
			},
			{ timeout: WORKER_TIMEOUT }
		)
		.toBe(0);

	// From the Files page: the search field leads to the results page.
	const field = page.getByRole('searchbox', { name: 'Search files' });
	await field.fill('squir budg');
	await field.press('Enter');
	await expect(page).toHaveURL(/\/files\/search\?q=squir/);
	const result = page.getByRole('list', { name: 'Search results' }).getByRole('listitem').filter({ hasText: 'notes.txt' });
	await expect(result).toBeVisible();
	await expect(result.locator('mark').filter({ hasText: /^squirrel$/i })).toBeVisible();

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

	const found = (await (await page.request.get('/api/search/documents?q=acorn%20stor')).json()) as { items: Hit[] };
	expect(found.items.map((hit) => hit.node.name)).toContain('invoice.pdf');
	const search = page.getByRole('searchbox', { name: 'Search files' });
	await search.fill('acorn stor');
	const pdf = page.getByRole('list', { name: 'Search results' }).getByRole('listitem').filter({ hasText: 'invoice.pdf' });
	await expect(pdf).toBeVisible();
	await expect(pdf.locator('img')).toBeVisible();

	// And the Documents folder shows the PDF's first page as its thumbnail.
	await openTopFolder(page, 'Documents');
	await expect(row(page, 'invoice.pdf').locator('img')).toBeVisible();
	await page.context().close();
});
