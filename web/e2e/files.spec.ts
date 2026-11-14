import { readFile } from 'node:fs/promises';
import { expect, test } from '@playwright/test';
import { setQuota } from './helpers/admin.ts';
import {
	createFolder,
	csrfToken,
	login,
	MIB,
	nodeId,
	openDialog,
	openRow,
	openTopFolder,
	patterned,
	row,
	upload,
	uploadDone,
	uploadItem,
	workspace
} from './helpers/files.ts';

// A 1×1 PNG.
const PNG = Buffer.from(
	'iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAYAAAAfFcSJAAAADUlEQVR42mNk+M9QDwADhgGAWjR9awAAAABJRU5ErkJggg==',
	'base64'
);
const PDF = Buffer.from('%PDF-1.4\n1 0 obj <<>> endobj\ntrailer <<>>\n%%EOF\n');

test('uploads a multi-chunk file, downloads it intact and links a re-upload without sending it', async ({
	browser
}) => {
	test.setTimeout(120_000);
	const page = await login(browser, 'alice');
	const chunkPuts: string[] = [];
	page.on('request', (request) => {
		if (request.method() === 'PUT' && request.url().includes('/chunks/')) chunkPuts.push(request.url());
	});

	await workspace(page, 'Uploads');
	const big = patterned(9 * MIB + 123, 1); // two 8 MiB chunks
	await uploadDone(page, { name: 'big.bin', mimeType: 'application/octet-stream', buffer: big });
	expect(chunkPuts).toHaveLength(2);

	// Through the row's download link: the browser saves the exact bytes under the file's name.
	const downloadEvent = page.waitForEvent('download');
	await row(page, 'big.bin').getByRole('link', { name: 'Download big.bin' }).click();
	const download = await downloadEvent;
	expect(download.suggestedFilename()).toBe('big.bin');
	expect(Buffer.compare(await readFile((await download.path())!), big)).toBe(0);

	const href = await row(page, 'big.bin').getByRole('link', { name: 'Download big.bin' }).getAttribute('href');
	const response = await page.request.get(href!);
	expect(response.status()).toBe(200);
	expect(response.headers()['content-disposition']).toMatch(/^attachment;.*filename="big\.bin"/);
	expect(response.headers()['content-length']).toBe(String(big.length));

	// The same content again, in another folder: linked without a single chunk.
	await workspace(page, 'Copies');
	const putsBefore = chunkPuts.length;
	await uploadDone(page, { name: 'big copy.bin', mimeType: 'application/octet-stream', buffer: big }, 'Already on your drive');
	expect(chunkPuts).toHaveLength(putsBefore);
});

test('previews text, images and PDFs', async ({ browser }) => {
	const page = await login(browser, 'alice');
	await workspace(page, 'Previews');
	await upload(
		page,
		{ name: 'hello.txt', mimeType: 'text/plain', buffer: Buffer.from('hello squirrel') },
		{ name: 'pixel.png', mimeType: 'image/png', buffer: PNG },
		{ name: 'doc.pdf', mimeType: 'application/pdf', buffer: PDF }
	);
	for (const name of ['hello.txt', 'pixel.png', 'doc.pdf']) {
		await expect(uploadItem(page, name)).toContainText('Done');
		await expect(row(page, name)).toBeVisible();
	}

	await openRow(page, 'hello.txt');
	let dialog = openDialog(page);
	await expect(dialog.getByTestId('text-preview')).toHaveText('hello squirrel');
	await dialog.getByRole('button', { name: 'Close' }).click();
	await expect(openDialog(page)).toHaveCount(0);

	await openRow(page, 'pixel.png');
	dialog = openDialog(page);
	const image = dialog.getByRole('img', { name: 'pixel.png' });
	await expect(image).toBeVisible();
	await expect.poll(() => image.evaluate((img: HTMLImageElement) => img.naturalWidth)).toBe(1);
	await dialog.getByRole('button', { name: 'Close' }).click();
	await expect(openDialog(page)).toHaveCount(0);

	// PDF: the iframe points at the inline content URL (headless Chromium has no PDF viewer, so the
	// rendering itself is checked by hand).
	await openRow(page, 'doc.pdf');
	dialog = openDialog(page);
	const frame = dialog.getByTitle('PDF preview of doc.pdf');
	await expect(frame).toBeVisible();
	const src = await frame.getAttribute('src');
	const pdf = await page.request.get(src!);
	expect(pdf.status()).toBe(200);
	expect(pdf.headers()['content-type']).toBe('application/pdf');
	expect(pdf.headers()['content-disposition']).toMatch(/^inline;/);
	const csp = pdf.headers()['content-security-policy'];
	expect(csp).toContain("frame-ancestors 'self'");
	expect(csp).not.toContain('sandbox');
	expect(Buffer.compare(await pdf.body(), PDF)).toBe(0);

	// Reopening revalidates: 304 with the same framing policy.
	const again = await page.request.get(src!, { headers: { 'If-None-Match': pdf.headers()['etag'] } });
	expect(again.status()).toBe(304);
	expect(again.headers()['content-security-policy']).toBe(csp);
	expect(again.headers()['x-frame-options']).toBe('SAMEORIGIN');
	await dialog.getByRole('button', { name: 'Close' }).click();
	await expect(openDialog(page)).toHaveCount(0);
});

test('creates folders, renames and moves', async ({ browser }) => {
	const page = await login(browser, 'alice');
	const space = await workspace(page, 'Tree');
	await uploadDone(page, { name: 'draft.txt', mimeType: 'text/plain', buffer: Buffer.from('draft') });

	await row(page, 'draft.txt').getByRole('button', { name: 'Rename draft.txt' }).click();
	let dialog = openDialog(page);
	await dialog.getByLabel('Name').fill('notes.txt');
	await dialog.getByRole('button', { name: 'Save' }).click();
	await expect(row(page, 'notes.txt')).toBeVisible();
	await expect(row(page, 'draft.txt')).toHaveCount(0);

	await createFolder(page, 'Archive');

	// A name already taken is refused in the dialog.
	await page.getByRole('button', { name: 'New folder' }).click();
	dialog = openDialog(page);
	await dialog.getByLabel('Name').fill('Archive');
	await dialog.getByRole('button', { name: 'Create folder' }).click();
	await expect(dialog.getByLabel('Name')).toHaveAttribute('aria-invalid', 'true');
	await dialog.getByRole('button', { name: 'Cancel' }).click();
	await expect(openDialog(page)).toHaveCount(0);

	await row(page, 'notes.txt').getByRole('button', { name: 'Move notes.txt' }).click();
	dialog = openDialog(page);
	await dialog.getByRole('button', { name: 'Documents', exact: true }).click();
	await dialog.getByRole('button', { name: space.name, exact: true }).click();
	await dialog.getByRole('button', { name: 'Archive', exact: true }).click();
	await expect(dialog.getByText(`To: Documents › ${space.name} › Archive`)).toBeVisible();
	await dialog.getByRole('button', { name: 'Move here' }).click();
	await expect(openDialog(page)).toHaveCount(0);
	await expect(row(page, 'notes.txt')).toHaveCount(0);

	await openRow(page, 'Archive');
	await expect(page.getByRole('heading', { level: 1, name: 'Archive' })).toBeVisible();
	await expect(page.getByRole('navigation', { name: 'Breadcrumb' })).toContainText(space.name);
	await expect(row(page, 'notes.txt')).toBeVisible();
});

test('deletes to the trash, restores, and deletes forever', async ({ browser }) => {
	const page = await login(browser, 'alice');
	const space = await workspace(page, 'Trash');
	await uploadDone(page, { name: 'keep.txt', mimeType: 'text/plain', buffer: Buffer.from('keep me') });
	await uploadDone(page, { name: 'drop.txt', mimeType: 'text/plain', buffer: Buffer.from('drop me') });

	await row(page, 'keep.txt').getByRole('button', { name: 'Delete keep.txt' }).click();
	await expect(row(page, 'keep.txt')).toHaveCount(0);
	await row(page, 'drop.txt').getByRole('button', { name: 'Delete drop.txt' }).click();
	await expect(row(page, 'drop.txt')).toHaveCount(0);

	await page.getByRole('navigation', { name: 'Main' }).getByRole('link', { name: 'Trash' }).click();
	await expect(page.getByRole('heading', { level: 1, name: 'Trash' })).toBeVisible();
	await page.getByRole('button', { name: 'Restore keep.txt' }).click();
	await expect(page.getByRole('status')).toHaveText('Restored “keep.txt”.');
	await expect(page.getByRole('button', { name: 'Restore keep.txt' })).toHaveCount(0);

	await page.getByRole('button', { name: 'Delete forever drop.txt' }).click();
	await page.getByRole('button', { name: 'Confirm drop.txt' }).click();
	await expect(page.getByRole('status')).toHaveText('Deleted forever.');
	await expect(page.getByRole('button', { name: 'Restore drop.txt' })).toHaveCount(0);

	await page.goto(space.url);
	await expect(row(page, 'keep.txt')).toBeVisible();
	await expect(row(page, 'drop.txt')).toHaveCount(0);
});

test('another user can neither see, fetch nor change alice’s files', async ({ browser }) => {
	const alice = await login(browser, 'alice');
	const space = await workspace(alice, 'Private');
	await uploadDone(alice, { name: 'secret.txt', mimeType: 'text/plain', buffer: Buffer.from('for alice only') });
	const fileId = await nodeId(alice, 'secret.txt');
	expect(fileId).toBeTruthy();
	const folderId = new URL(space.url).pathname.split('/').at(-1)!;

	const bob = await login(browser, 'bob');
	for (const path of [`/api/files/${fileId}/content`, `/api/nodes/${fileId}`, `/api/nodes/${folderId}`]) {
		expect((await bob.request.get(path)).status(), path).toBe(404);
	}
	expect((await bob.request.get(`/api/nodes?parent_id=${folderId}`)).status()).toBe(404);
	const headers = { 'X-CSRF-Token': await csrfToken(bob) };
	expect((await bob.request.patch(`/api/nodes/${fileId}`, { headers, data: { name: 'mine.txt' } })).status()).toBe(404);
	expect((await bob.request.delete(`/api/nodes/${fileId}`, { headers })).status()).toBe(404);

	await bob.goto(space.url);
	await expect(bob.getByRole('alert')).toContainText('This folder does not exist or was deleted.');
	await openTopFolder(bob, 'Documents');
	await expect(row(bob, space.name)).toHaveCount(0);

	// Nothing bob tried changed alice's file.
	await alice.reload();
	await expect(row(alice, 'secret.txt')).toBeVisible();
});

test('an upload over the quota is refused with a clear message', async ({ browser }) => {
	setQuota('bob', MIB);
	try {
		const bob = await login(browser, 'bob');
		await openTopFolder(bob, 'Files');
		await upload(bob, { name: 'too-big.bin', mimeType: 'application/octet-stream', buffer: patterned(2 * MIB, 2) });
		await expect(uploadItem(bob, 'too-big.bin')).toContainText('exceed your storage quota');
		await expect(row(bob, 'too-big.bin')).toHaveCount(0);
		await expect(bob.getByTestId('storage-usage')).toContainText('of 1.0 MB used');
	} finally {
		setQuota('bob', 'unlimited');
	}
});
