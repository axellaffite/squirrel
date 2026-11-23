import { randomUUID } from 'node:crypto';
import { expect, test, type Page } from '@playwright/test';
import { csrfToken, login, openDialog, workspace } from './helpers/files.ts';
import { newUser } from './helpers/photos.ts';

/** Shares what the page shows through its "Share …" button. */
async function shareWith(page: Page, subject: string, username: string, permission: 'Read' | 'Write') {
	await page.getByRole('button', { name: `Share ${subject}`, exact: true }).click();
	const dialog = openDialog(page);
	await expect(dialog.getByRole('heading', { name: `Share “${subject}”` })).toBeVisible();
	await dialog.getByLabel('Username').fill(username);
	await dialog.getByRole('radio', { name: permission }).check();
	await dialog.getByRole('button', { name: 'Share', exact: true }).click();
	await expect(dialog.getByTestId('share-row')).toContainText(`@${username}`);
	await dialog.getByRole('button', { name: 'Close' }).click();
	await expect(openDialog(page)).toHaveCount(0);
}

test('shares a folder read-only and an album for writing, with comments both ways', async ({ browser }) => {
	test.setTimeout(120_000);
	const { page: bob, username: bobName } = await newUser(browser, 'bob');
	const alice = await login(browser, 'alice');

	// Alice shares a folder of hers, read-only.
	const folder = await workspace(alice, 'Trip');
	await shareWith(alice, folder.name, bobName, 'Read');

	// Bob finds it under Shared, opens it, and cannot add to it.
	await bob.getByRole('navigation', { name: 'Main' }).getByRole('link', { name: 'Shared' }).click();
	await expect(bob.getByRole('heading', { level: 1, name: 'Shared' })).toBeVisible();
	const row = bob.getByTestId('shared-item').filter({ hasText: folder.name });
	await expect(row).toContainText('Can view');
	await row.getByRole('button', { name: folder.name, exact: true }).click();
	await expect(bob.getByRole('heading', { level: 1, name: folder.name, exact: true })).toBeVisible();
	await expect(bob.getByText(/Shared by /)).toBeVisible();
	await expect(bob.getByRole('navigation', { name: 'Breadcrumb' }).getByRole('link', { name: 'Shared' })).toBeVisible();
	await expect(bob.getByRole('button', { name: 'Upload files' })).toHaveCount(0);
	await expect(bob.getByRole('button', { name: 'New folder' })).toHaveCount(0);
	await expect(bob.getByRole('button', { name: `Share ${folder.name}`, exact: true })).toHaveCount(0);

	// Bob comments; Alice sees it.
	const note = `Lovely trip ${randomUUID().slice(0, 6)}`;
	await bob.getByRole('button', { name: `Comments on ${folder.name}` }).click();
	await openDialog(bob).getByLabel('Add a comment').fill(note);
	await openDialog(bob).getByRole('button', { name: 'Post comment' }).click();
	await expect(openDialog(bob).getByTestId('comment')).toContainText(note);

	await alice.reload();
	await alice.getByRole('button', { name: `Comments on ${folder.name}` }).click();
	const comment = openDialog(alice).getByTestId('comment').filter({ hasText: note });
	await expect(comment).toContainText('bob');
	await expect(comment.getByRole('button', { name: /^Edit your comment/ })).toHaveCount(0);
	await expect(comment.getByRole('button', { name: 'Delete the comment by bob' })).toBeVisible();
	await openDialog(alice).getByRole('button', { name: 'Close' }).click();

	// Alice shares an album for writing; Bob sees it.
	const albumName = `Holidays ${randomUUID().slice(0, 6)}`;
	const created = await alice.request.post('/api/albums', {
		headers: { 'X-CSRF-Token': await csrfToken(alice) },
		data: { name: albumName, node_ids: [] }
	});
	expect(created.status()).toBe(201);
	const { id } = (await created.json()) as { id: string };
	await alice.goto(`/photos/albums/${id}`);
	await expect(alice.getByRole('heading', { level: 1, name: albumName })).toBeVisible();
	await shareWith(alice, albumName, bobName, 'Write');

	await bob.goto('/shared');
	const card = bob.getByTestId('shared-album').filter({ hasText: albumName });
	await expect(card).toContainText('Can edit');
	await card.getByRole('link', { name: new RegExp(albumName) }).click();
	await expect(bob.getByRole('heading', { level: 1, name: albumName })).toBeVisible();
	await expect(bob.getByRole('button', { name: 'Delete album' })).toHaveCount(0);

	// Bob leaves the folder.
	await bob.goto('/shared');
	await bob.getByRole('button', { name: `Leave ${folder.name}` }).click();
	await openDialog(bob).getByRole('button', { name: 'Leave', exact: true }).click();
	await expect(bob.getByTestId('shared-item').filter({ hasText: folder.name })).toHaveCount(0);

	await alice.context().close();
	await bob.context().close();
});
