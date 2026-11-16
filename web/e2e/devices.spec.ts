import { randomUUID } from 'node:crypto';
import { expect, test, type APIRequestContext, type Browser, type Page } from '@playwright/test';
import { PASSWORD, login, openDialog } from './helpers/files.ts';

// Desktop project (see playwright.config.ts); the phone case opens its own phone-sized context.

/** Signs a phone in as alice through the API; returns its name and access token. */
async function signInPhone(request: APIRequestContext): Promise<{ name: string; token: string }> {
	const name = `Pixel ${randomUUID().slice(0, 8)}`;
	const response = await request.post('/api/auth/device/login', {
		data: {
			username: 'alice',
			password: PASSWORD,
			device: { name, platform: 'android', model: 'Pixel 8', app_version: '0.1.0' }
		}
	});
	expect(response.status()).toBe(201);
	const body = await response.json();
	return { name, token: body.tokens.access_token };
}

async function bearerStatus(request: APIRequestContext, token: string): Promise<number> {
	const response = await request.get('/api/nodes', { headers: { Authorization: `Bearer ${token}` } });
	return response.status();
}

function deviceRow(page: Page, name: string) {
	return page.getByRole('list', { name: 'Devices' }).getByRole('listitem').filter({ hasText: name });
}

async function phonePage(browser: Browser): Promise<Page> {
	const context = await browser.newContext({ viewport: { width: 390, height: 844 }, isMobile: true, hasTouch: true });
	const page = await context.newPage();
	await page.goto('/login');
	await page.getByLabel('Username').fill('alice');
	await page.getByLabel('Password').fill(PASSWORD);
	await page.getByRole('button', { name: 'Log in' }).click();
	await expect(page.getByRole('heading', { name: /^Hello, / })).toBeVisible();
	return page;
}

test('a signed-in phone is listed and can be revoked', async ({ browser, request }) => {
	const phone = await signInPhone(request);
	expect(await bearerStatus(request, phone.token)).toBe(200);

	const page = await login(browser, 'alice');
	await page.getByRole('button', { name: 'Account and theme' }).locator('visible=true').click();
	await page.getByRole('link', { name: 'Devices' }).locator('visible=true').click();
	await expect(page.getByRole('heading', { level: 1, name: 'Devices' })).toBeVisible();

	const row = deviceRow(page, phone.name);
	await expect(row).toContainText('Pixel 8 · Android · 0.1.0');
	await expect(row).toContainText('Active now');
	await row.getByRole('button', { name: `Revoke ${phone.name}` }).click();

	const dialog = openDialog(page);
	await expect(dialog.getByRole('heading', { name: `Revoke ${phone.name}?` })).toBeVisible();
	await dialog.getByRole('button', { name: 'Revoke', exact: true }).click();
	await expect(openDialog(page)).toHaveCount(0);
	await expect(row.getByText('Revoked', { exact: true })).toBeVisible();
	await expect(row.getByRole('button', { name: `Revoke ${phone.name}` })).toHaveCount(0);

	expect(await bearerStatus(request, phone.token)).toBe(401);
});

test('on a phone, revoking goes through the actions sheet', async ({ browser, request }) => {
	const phone = await signInPhone(request);
	const page = await phonePage(browser);
	await page.goto('/settings/devices');

	const row = deviceRow(page, phone.name);
	await expect(row.getByRole('button', { name: `Revoke ${phone.name}` })).toBeHidden();
	await row.getByRole('button', { name: `Actions for ${phone.name}` }).click();
	await openDialog(page).getByRole('button', { name: `Revoke ${phone.name}` }).click();

	// The confirmation opens once the sheet has closed.
	const dialog = openDialog(page);
	await expect(dialog.getByRole('heading', { name: `Revoke ${phone.name}?` })).toBeVisible();
	await dialog.getByRole('button', { name: 'Revoke', exact: true }).click();
	await expect(row.getByText('Revoked', { exact: true })).toBeVisible();
	expect(await bearerStatus(request, phone.token)).toBe(401);
});
