import { expect, test } from '@playwright/test';
import { login, openDialog, row, uploadDone, workspace } from './helpers/files.ts';

// Runs at phone width (see playwright.config.ts): row actions live in an actions sheet.

test('row actions go through the actions sheet on phones', async ({ browser }) => {
	const page = await login(browser, 'alice');
	await workspace(page, 'Phone');
	await uploadDone(page, { name: 'memo.txt', mimeType: 'text/plain', buffer: Buffer.from('from a phone') });

	const memo = row(page, 'memo.txt');
	await expect(memo.getByRole('button', { name: 'Rename memo.txt' })).toBeHidden();
	await memo.getByRole('button', { name: 'Actions for memo.txt' }).click();
	let sheet = openDialog(page);
	await expect(sheet.getByRole('heading', { name: 'memo.txt' })).toBeVisible();
	await expect(sheet.getByRole('link', { name: 'Download memo.txt' })).toBeVisible();
	await sheet.getByRole('button', { name: 'Rename memo.txt' }).click();

	// The rename dialog opens once the sheet has closed.
	const dialog = openDialog(page);
	await expect(dialog.getByRole('heading', { name: 'Rename “memo.txt”' })).toBeVisible();
	await dialog.getByLabel('Name').fill('note.txt');
	await dialog.getByRole('button', { name: 'Save' }).click();
	await expect(openDialog(page)).toHaveCount(0);
	await expect(row(page, 'note.txt')).toBeVisible();

	await row(page, 'note.txt').getByRole('button', { name: 'Actions for note.txt' }).click();
	sheet = openDialog(page);
	await sheet.getByRole('button', { name: 'Preview note.txt' }).click();
	await expect(openDialog(page).getByTestId('text-preview')).toHaveText('from a phone');
	await openDialog(page).getByRole('button', { name: 'Close' }).click();
	await expect(openDialog(page)).toHaveCount(0);

	await row(page, 'note.txt').getByRole('button', { name: 'Actions for note.txt' }).click();
	await openDialog(page).getByRole('button', { name: 'Move to trash note.txt' }).click();
	await expect(row(page, 'note.txt')).toHaveCount(0);
});
