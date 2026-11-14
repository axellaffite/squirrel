import { randomUUID } from 'node:crypto';
import { expect, type Browser, type Locator, type Page } from '@playwright/test';

export const PASSWORD = 'correct horse battery'; // accounts created by onboarding.spec.ts
export const MIB = 1024 * 1024;

export interface UploadFile {
	name: string;
	mimeType: string;
	buffer: Buffer;
}

/** Deterministic bytes; `seed` keeps contents distinct between tests (no accidental dedup). */
export function patterned(size: number, seed = 0): Buffer {
	const buffer = Buffer.alloc(size);
	for (let i = 0; i < size; i += 1) buffer[i] = (i * 31 + seed) % 251;
	return buffer;
}

/** A fresh browser context logged in as `username`. */
export async function login(browser: Browser, username: string): Promise<Page> {
	const page = await (await browser.newContext()).newPage();
	await page.goto('/login');
	await page.getByLabel('Username').fill(username);
	await page.getByLabel('Password').fill(PASSWORD);
	await page.getByRole('button', { name: 'Log in' }).click();
	await expect(page.getByRole('heading', { name: /^Hello, / })).toBeVisible();
	return page;
}

/** The dialog currently open (a closing one lingers, closed, for its exit animation). */
export function openDialog(page: Page): Locator {
	return page.locator('dialog[open]');
}

export function rows(page: Page): Locator {
	return page.getByRole('list', { name: 'Files and folders' }).getByRole('listitem');
}

/** The row whose name is exactly `name`. */
export function row(page: Page, name: string): Locator {
	return rows(page).filter({ has: page.getByRole('button', { name, exact: true }) });
}

export async function openRow(page: Page, name: string): Promise<void> {
	await row(page, name).getByRole('button', { name, exact: true }).click();
}

/** Opens a folder from the top level ("My drive"). */
export async function openTopFolder(page: Page, name: string): Promise<void> {
	await page.getByRole('navigation', { name: 'Main' }).getByRole('link', { name: 'Files', exact: true }).click();
	await expect(page.getByRole('heading', { level: 1, name: 'My drive' })).toBeVisible();
	await openRow(page, name);
	await expect(page.getByRole('heading', { level: 1, name, exact: true })).toBeVisible();
}

export async function createFolder(page: Page, name: string): Promise<void> {
	await page.getByRole('button', { name: 'New folder' }).click();
	const dialog = openDialog(page);
	await dialog.getByLabel('Name').fill(name);
	await dialog.getByRole('button', { name: 'Create folder' }).click();
	await expect(openDialog(page)).toHaveCount(0);
	await expect(row(page, name)).toBeVisible();
}

/**
 * Creates a uniquely named folder in Documents and opens it, so every test works in its own
 * space. Returns the folder's name and URL.
 */
export async function workspace(page: Page, label: string): Promise<{ name: string; url: string }> {
	const name = `${label} ${randomUUID().slice(0, 8)}`;
	await openTopFolder(page, 'Documents');
	await createFolder(page, name);
	await openRow(page, name);
	await expect(page.getByRole('heading', { level: 1, name, exact: true })).toBeVisible();
	return { name, url: page.url() };
}

/** Uploads through the "Upload files" button and the native file chooser. */
export async function upload(page: Page, ...files: UploadFile[]): Promise<void> {
	const chooser = page.waitForEvent('filechooser');
	await page.getByRole('button', { name: 'Upload files' }).click();
	await (await chooser).setFiles(files);
}

export function uploadItem(page: Page, name: string): Locator {
	return page.getByTestId('upload-item').filter({ hasText: name }).last();
}

/** Uploads one file and waits until the panel reports it and the listing shows it. */
export async function uploadDone(page: Page, file: UploadFile, status = 'Done'): Promise<void> {
	await upload(page, file);
	await expect(uploadItem(page, file.name)).toContainText(status, { timeout: 60_000 });
	await expect(row(page, file.name)).toBeVisible();
}

export function nodeId(page: Page, name: string): Promise<string | null> {
	return row(page, name).getAttribute('data-node-id');
}

export async function csrfToken(page: Page): Promise<string> {
	const me = await page.request.get('/api/auth/me');
	expect(me.status()).toBe(200);
	return ((await me.json()) as { csrf_token: string }).csrf_token;
}
