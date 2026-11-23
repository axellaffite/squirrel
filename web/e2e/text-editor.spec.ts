import { randomUUID } from 'node:crypto';
import { expect, test, type Locator, type Page } from '@playwright/test';
import { login, nodeId, openDialog, row, uploadDone, workspace } from './helpers/files.ts';
import { newUser } from './helpers/photos.ts';

/** Shares the file of row `name` through its "Share …" row button. */
async function shareFile(page: Page, name: string, username: string, permission: 'Read' | 'Write') {
	await row(page, name).getByRole('button', { name: `Share ${name}`, exact: true }).click();
	const dialog = openDialog(page);
	await expect(dialog.getByRole('heading', { name: `Share “${name}”` })).toBeVisible();
	await dialog.getByLabel('Username').fill(username);
	// Existing shares list their own permission radios: pick the one in the new-share form.
	await dialog.locator('form').getByRole('radio', { name: permission }).check();
	await dialog.getByRole('button', { name: 'Share', exact: true }).click();
	await expect(dialog.getByTestId('share-row').filter({ hasText: `@${username}` })).toBeVisible();
	await dialog.getByRole('button', { name: 'Close' }).click();
	await expect(openDialog(page)).toHaveCount(0);
}

function editor(page: Page, name: string): Locator {
	return page.getByRole('textbox', { name: `Contents of ${name}` });
}

/** The document text, without the names y-codemirror draws at other people's cursors. */
function documentText(page: Page, name: string): Promise<string> {
	return editor(page, name).evaluate((node) => {
		const copy = node.cloneNode(true) as HTMLElement;
		copy.querySelectorAll('.cm-ySelectionCaret, .cm-ySelectionInfo').forEach((caret) => caret.remove());
		return [...copy.querySelectorAll('.cm-line')].map((line) => line.textContent ?? '').join('\n');
	});
}

/** Waits until `page` shows document text `expected`. */
async function expectText(page: Page, name: string, expected: string) {
	await expect.poll(() => documentText(page, name)).toBe(expected);
}

function status(page: Page): Locator {
	return page.getByTestId('editor-status');
}

/** Types at the very end of the document. */
async function append(page: Page, name: string, text: string) {
	await editor(page, name).click();
	await page.keyboard.press('ControlOrMeta+End');
	await page.keyboard.type(text);
}

test('two people co-edit a text file live; a reader follows without typing', async ({ browser }) => {
	test.setTimeout(120_000);
	const { page: bob, username: bobName } = await newUser(browser, 'bob');
	const { page: carol, username: carolName } = await newUser(browser, 'carol');
	const alice = await login(browser, 'alice');

	// Alice uploads a note (unique text: no deduplication with other runs) and shares it.
	const name = 'notes.txt';
	const start = `Acorn plan ${randomUUID().slice(0, 8)}.`;
	await workspace(alice, 'Editor');
	await uploadDone(alice, { name, mimeType: 'text/plain', buffer: Buffer.from(start) });
	const id = await nodeId(alice, name);
	expect(id).not.toBeNull();
	await shareFile(alice, name, bobName, 'Write');
	await shareFile(alice, name, carolName, 'Read');

	// Alice opens the editor from her folder.
	await row(alice, name).getByRole('link', { name: `Edit ${name}` }).click();
	await expect(alice.getByRole('heading', { level: 1, name })).toBeVisible();
	await expect(status(alice)).toHaveText('Connected');
	await expectText(alice, name, start);

	// Bob opens it from Shared, through the preview.
	await bob.goto('/shared');
	const shared = bob.getByTestId('shared-item').filter({ hasText: name });
	await expect(shared).toContainText('Can edit');
	await shared.getByRole('button', { name, exact: true }).click();
	await openDialog(bob).getByRole('link', { name: `Edit ${name}` }).click();
	await expect(status(bob)).toHaveText('Connected');
	await expectText(bob, name, start);
	await expect(bob.getByTestId('read-only')).toHaveCount(0);

	// Each sees the other.
	await expect(alice.getByRole('list', { name: 'People in this document' })).toContainText('bob');
	await expect(bob.getByRole('list', { name: 'People in this document' })).toContainText('alice', { ignoreCase: true });

	// Alice types; Bob sees it. Bob types; Alice sees it.
	const fromAlice = ' Hazelnuts by Alice.';
	await append(alice, name, fromAlice);
	await expectText(bob, name, start + fromAlice);
	const fromBob = ' Storage by Bob.';
	await append(bob, name, fromBob);
	const merged = start + fromAlice + fromBob;
	await expectText(alice, name, merged);
	await expectText(bob, name, merged);

	// The server saves into the file a couple of seconds after the last change.
	await expect
		.poll(async () => (await alice.request.get(`/api/files/${id}/content`)).text(), { timeout: 20_000 })
		.toBe(merged);
	await expect(status(alice)).toHaveText('Saved automatically');

	// Carol, read-only, follows live but cannot type.
	await carol.goto(`/edit/${id}`);
	await expect(carol.getByTestId('read-only')).toContainText('Read only');
	const readOnly = editor(carol, name);
	await expectText(carol, name, merged);
	await expect(readOnly).toHaveAttribute('contenteditable', 'false');
	await readOnly.click();
	await carol.keyboard.type(' Carol was here.');
	await expectText(carol, name, merged);

	const more = ' Winter is coming.';
	await append(alice, name, more);
	await expectText(carol, name, merged + more);
	await expectText(bob, name, merged + more);
	await expect
		.poll(async () => (await alice.request.get(`/api/files/${id}/content`)).text(), { timeout: 20_000 })
		.toBe(merged + more);

	await alice.context().close();
	await bob.context().close();
	await carol.context().close();
});
