import { describe, expect, it, vi } from 'vitest';
import { collectPages } from '#lib/files/pages.ts';

describe('collectPages', () => {
	it('follows cursors until the last page', async () => {
		const fetchPage = vi.fn(async (cursor: string | null) =>
			cursor === null
				? { ok: true as const, data: { items: [1, 2], next_cursor: 'c2' } }
				: { ok: true as const, data: { items: [3], next_cursor: null } }
		);
		expect(await collectPages(fetchPage)).toEqual({ ok: true, data: [1, 2, 3] });
		expect(fetchPage).toHaveBeenNthCalledWith(2, 'c2');
	});

	it('stops at the first error', async () => {
		const failure = { ok: false as const, error: 'boom', status: 500 };
		expect(await collectPages(async () => failure)).toEqual(failure);
	});

	it('stops when the server hands back a cursor it already gave', async () => {
		const fetchPage = vi.fn(async () => ({ ok: true as const, data: { items: [1], next_cursor: 'same' } }));
		const result = await collectPages(fetchPage);
		expect(result).toMatchObject({ ok: false, status: 0 });
		expect(fetchPage).toHaveBeenCalledTimes(2);
	});

	it('gives up after too many pages', async () => {
		let page = 0;
		const fetchPage = vi.fn(async () => ({ ok: true as const, data: { items: [page], next_cursor: `c${++page}` } }));
		const result = await collectPages(fetchPage, 5);
		expect(result).toEqual({ ok: false, error: expect.stringMatching(/too many/i), status: 0 });
		expect(fetchPage).toHaveBeenCalledTimes(5);
	});
});
