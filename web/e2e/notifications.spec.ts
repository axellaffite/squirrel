import { randomUUID } from 'node:crypto';
import { expect, test, type Page } from '@playwright/test';
import { login, openDialog, workspace } from './helpers/files.ts';
import { newUser } from './helpers/photos.ts';

const bell = (page: Page) => page.getByRole('button', { name: /^Notifications/ });
const inbox = (page: Page) => page.getByRole('list', { name: 'Notifications' });

test('notifies shares and comments, and leads to the subject', async ({ browser }) => {
	test.setTimeout(120_000);
	const { page: bob, username: bobName } = await newUser(browser, 'bob');
	const alice = await login(browser, 'alice');
	await expect(bell(bob)).toHaveAccessibleName('Notifications');

	// Alice shares a folder with Bob.
	const folder = await workspace(alice, 'Trip');
	await alice.getByRole('button', { name: `Share ${folder.name}`, exact: true }).click();
	await openDialog(alice).getByLabel('Username').fill(bobName);
	await openDialog(alice).getByRole('radio', { name: 'Read' }).check();
	await openDialog(alice).getByRole('button', { name: 'Share', exact: true }).click();
	await expect(openDialog(alice).getByTestId('share-row')).toContainText(`@${bobName}`);
	await openDialog(alice).getByRole('button', { name: 'Close' }).click();

	// Bob sees the badge, opens the panel and follows the notification into the folder.
	await bob.reload();
	await expect(bell(bob)).toHaveAccessibleName('Notifications, 1 unread');
	await expect(bob.getByTestId('unread-badge').filter({ visible: true })).toHaveText('1');
	await bell(bob).click();
	const shared = inbox(bob).getByTestId('notification').filter({ hasText: folder.name });
	await expect(shared).toContainText(`shared “${folder.name}” with you · can view`);
	await shared.getByRole('button').click();
	await expect(bob.getByRole('heading', { level: 1, name: folder.name, exact: true })).toBeVisible();
	await expect(bell(bob)).toHaveAccessibleName('Notifications');
	await expect(bob.getByTestId('unread-badge')).toHaveCount(0);

	// Bob comments; Alice is told and lands on the thread.
	const note = `Lovely ${randomUUID().slice(0, 6)}`;
	await bob.getByRole('button', { name: `Comments on ${folder.name}` }).click();
	await openDialog(bob).getByLabel('Add a comment').fill(note);
	await openDialog(bob).getByRole('button', { name: 'Post comment' }).click();
	await expect(openDialog(bob).getByTestId('comment')).toContainText(note);

	await alice.goto('/');
	await expect(bell(alice)).toHaveAccessibleName(/^Notifications, \d+ unread$/);
	await bell(alice).click();
	const commented = inbox(alice).getByTestId('notification').filter({ hasText: note });
	await expect(commented).toContainText(`bob commented on “${folder.name}”: ${note}`);
	await commented.getByRole('button').click();
	await expect(alice.getByRole('heading', { level: 1, name: folder.name, exact: true })).toBeVisible();
	await expect(openDialog(alice).getByTestId('comment').filter({ hasText: note })).toBeVisible();
	await expect(alice).toHaveURL(folder.url);

	// "Mark all read" clears the rest.
	await openDialog(alice).getByRole('button', { name: 'Close' }).click();
	await bell(alice).click();
	const markAll = alice.getByRole('button', { name: 'Mark all read' });
	if (await markAll.isVisible()) await markAll.click();
	await expect(bell(alice)).toHaveAccessibleName('Notifications');

	await alice.context().close();
	await bob.context().close();
});
