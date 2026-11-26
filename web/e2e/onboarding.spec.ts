import { expect, test, type Page } from '@playwright/test';

const PASSWORD = 'correct horse battery';
const SETUP_CODE = 'e2e-setup-token'; // matches SQUIRREL_SETUP_TOKEN in scripts/e2e-server.sh

// The tests share one fresh instance and build on each other's state.
test.describe.configure({ mode: 'serial' });

async function fillAccount(page: Page, username: string, displayName: string) {
	await page.getByLabel('Username').fill(username);
	await page.getByLabel('Display name').fill(displayName);
	await page.getByLabel('Password', { exact: true }).fill(PASSWORD);
	await page.getByLabel('Confirm password').fill(PASSWORD);
}

test('a wrong setup code is rejected', async ({ page }) => {
	await page.goto('/');
	await expect(page).toHaveURL(/\/setup$/);
	await page.getByLabel('Setup code').fill('not-the-setup-code');
	await fillAccount(page, 'mallory', 'Mallory');
	await page.getByRole('button', { name: 'Create administrator account' }).click();
	await expect(page.getByRole('alert')).toContainText('setup code is incorrect');
	await expect(page).toHaveURL(/\/setup$/);
});

test('first user sets up the drive and invites a second user', async ({ page, browser }) => {
	await page.goto('/');
	await expect(page).toHaveURL(/\/setup$/);
	await page.getByLabel('Setup code').fill(SETUP_CODE);
	await fillAccount(page, 'alice', 'Alice');
	await page.getByRole('button', { name: 'Create administrator account' }).click();
	await expect(page.getByRole('heading', { name: 'Hello, Alice' })).toBeVisible();

	await page.getByRole('link', { name: 'Invitations' }).click();
	await page.getByRole('button', { name: 'Create invitation' }).click();
	const link = (await page.getByTestId('invite-link').textContent())?.trim();
	expect(link).toContain('/join/');

	const bob = await (await browser.newContext()).newPage();
	await bob.goto(link!);
	await fillAccount(bob, 'bob', 'Bob');
	await bob.getByRole('button', { name: 'Join Squirrel' }).click();
	await expect(bob.getByRole('heading', { name: 'Hello, Bob' })).toBeVisible();

	const carol = await (await browser.newContext()).newPage();
	await carol.goto(link!);
	await expect(carol.getByText('invalid, expired or already used')).toBeVisible();

	await carol.goto('/setup');
	await expect(carol).toHaveURL(/\/login$/);

	// Log out lives in the account menu (the avatar at the top of the sidebar).
	await page.getByRole('button', { name: 'Account and theme' }).locator('visible=true').click();
	await page.getByRole('button', { name: 'Log out' }).locator('visible=true').click();
	await expect(page).toHaveURL(/\/login$/);
	await page.getByLabel('Username').fill('Alice');
	await page.getByLabel('Password').fill(PASSWORD);
	await page.getByRole('button', { name: 'Log in' }).click();
	await expect(page.getByRole('heading', { name: 'Hello, Alice' })).toBeVisible();
});

test('an anonymous visitor is sent to the login page without an error', async ({ page }) => {
	const invitationRequests: string[] = [];
	page.on('request', (request) => {
		if (new URL(request.url()).pathname.startsWith('/api/invitations')) {
			invitationRequests.push(request.url());
		}
	});
	await page.goto('/settings/invitations');
	await expect(page).toHaveURL(/\/login$/);
	await expect(page.getByRole('heading', { name: 'Log in' })).toBeVisible();
	await expect(page.getByRole('alert')).toHaveCount(0);
	expect(invitationRequests).toEqual([]);
});
