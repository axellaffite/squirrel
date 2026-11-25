import { randomUUID } from 'node:crypto';
import { expect, test } from '@playwright/test';
import { login, openDialog, workspace } from './helpers/files.ts';
import { newUser } from './helpers/photos.ts';

test('adds a friend, shares with them in one tap, then finds them suggested after removing', async ({ browser }) => {
	test.setTimeout(120_000);
	// A unique display name: Alice's friends outlive this test.
	const label = `pal${randomUUID().slice(0, 6)}`;
	const { page: friendPage, username } = await newUser(browser, label);
	await friendPage.context().close();
	const alice = await login(browser, 'alice');

	// From the account menu to the Friends page; Alice adds the friend by username.
	await alice.getByRole('button', { name: 'Account and theme' }).first().click();
	await alice.getByRole('link', { name: 'Friends' }).first().click();
	await expect(alice.getByRole('heading', { level: 1, name: 'Friends' })).toBeVisible();
	await alice.getByLabel('Username').fill(username);
	await alice.getByRole('button', { name: 'Add friend' }).click();
	const friends = alice.getByRole('list', { name: 'Friends', exact: true });
	await expect(friends).toContainText(`@${username}`);

	// An unknown username is refused with the server's message.
	await alice.getByLabel('Username').fill(`nobody-${randomUUID().slice(0, 6)}`);
	await alice.getByRole('button', { name: 'Add friend' }).click();
	await expect(alice.getByRole('alert')).toContainText('no account');

	// The Share dialog offers the friend; one tap shares, and the pick goes away.
	const folder = await workspace(alice, 'Friends');
	await alice.getByRole('button', { name: `Share ${folder.name}`, exact: true }).click();
	const dialog = openDialog(alice);
	await dialog.getByRole('button', { name: `Share with ${label}` }).click();
	await expect(dialog.getByTestId('share-row')).toContainText(`@${username}`);
	await expect(dialog.getByRole('button', { name: `Share with ${label}` })).toHaveCount(0);
	await dialog.getByRole('button', { name: 'Close' }).click();

	// Removed, the friend comes back as a suggestion (they share a folder), and is added again.
	await alice.goto('/settings/friends');
	await alice.getByRole('button', { name: `Remove ${label} from friends` }).click();
	await expect(friends.getByText(`@${username}`)).toHaveCount(0);
	const suggested = alice.getByRole('list', { name: 'Suggested friends' });
	await expect(suggested).toContainText(`@${username}`);
	await suggested.getByRole('button', { name: `Add ${label} to friends` }).click();
	await expect(friends).toContainText(`@${username}`);
	await expect(suggested.getByText(`@${username}`)).toHaveCount(0);

	await alice.context().close();
});
