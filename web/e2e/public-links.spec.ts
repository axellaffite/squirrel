import { expect, test } from '@playwright/test';
import { login, openDialog, row, upload, uploadItem, workspace } from './helpers/files.ts';

const LINK_PASSWORD = 'open sesame please';
const publicPort = Number(process.env.SQUIRREL_E2E_PORT ?? '8099') + 1;
const publicOrigin = `http://127.0.0.1:${publicPort}`;

test('a password-protected folder link opens for a visitor, then stops once revoked', async ({ browser }) => {
	test.setTimeout(120_000);
	const alice = await login(browser, 'alice');
	const folder = await workspace(alice, 'Holiday');
	await upload(alice, { name: 'notes.txt', mimeType: 'text/plain', buffer: Buffer.from('See you at the beach') });
	await expect(uploadItem(alice, 'notes.txt')).toContainText('Done');
	await expect(row(alice, 'notes.txt')).toBeVisible();

	// Alice makes a link with a password and copies its URL.
	await alice.getByRole('button', { name: `Share ${folder.name}`, exact: true }).click();
	const dialog = openDialog(alice);
	await expect(dialog.getByRole('heading', { name: 'Public link' })).toBeVisible();
	await dialog.getByLabel('Password (optional)').fill(LINK_PASSWORD);
	await dialog.getByRole('button', { name: 'Create link' }).click();
	const url = await dialog.getByTestId('public-link-url').inputValue();
	expect(url.startsWith(`${publicOrigin}/s/`)).toBe(true);
	await expect(dialog.getByTestId('public-link-row')).toContainText('No expiry · Password');

	// A visitor without an account opens it on the public listener.
	const visitor = await (await browser.newContext()).newPage();
	await visitor.goto(url);
	await expect(visitor.getByRole('heading', { name: 'Password needed' })).toBeVisible();
	await visitor.getByLabel('Password', { exact: true }).fill('not the password');
	await visitor.getByRole('button', { name: 'Open' }).click();
	await expect(visitor.getByText('Wrong password.')).toBeVisible();
	await visitor.getByLabel('Password', { exact: true }).fill(LINK_PASSWORD);
	await visitor.getByRole('button', { name: 'Open' }).click();

	await expect(visitor.getByRole('heading', { level: 1, name: folder.name })).toBeVisible();
	await expect(visitor.getByText('Shared by ')).toBeVisible();
	const file = visitor.getByTestId('public-node').filter({ hasText: 'notes.txt' });
	await file.getByRole('button', { name: 'notes.txt' }).click();
	const viewer = openDialog(visitor);
	await expect(viewer.getByRole('heading', { name: 'notes.txt' })).toBeVisible();
	const download = viewer.getByRole('link', { name: 'Download' });
	const content = await visitor.request.get(new URL((await download.getAttribute('href')) ?? '', publicOrigin).href);
	expect(content.status()).toBe(200);
	expect(await content.text()).toBe('See you at the beach');
	await viewer.getByRole('button', { name: 'Close' }).click();

	// The public listener serves nothing of the private app.
	expect((await visitor.request.get(`${publicOrigin}/login`)).status()).toBe(404);
	expect((await visitor.request.get(`${publicOrigin}/api/auth/me`)).status()).toBe(404);

	// Alice revokes it: the visitor's next visit finds nothing.
	await dialog.getByRole('button', { name: /^Revoke the link/ }).click();
	await expect(dialog.getByTestId('public-link-row')).toHaveCount(0);
	await visitor.reload();
	await expect(visitor.getByRole('heading', { name: 'Link unavailable' })).toBeVisible();
});
