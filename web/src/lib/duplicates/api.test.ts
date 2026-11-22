import { describe, expect, it, vi } from 'vitest';
import { createApiClient } from '#lib/api/client.ts';
import { keepDuplicate, listDuplicates, openDuplicateCount, restoreDuplicates } from '#lib/duplicates/api.ts';

function reply(status: number, body?: unknown) {
	return body === undefined
		? new Response(null, { status })
		: new Response(JSON.stringify(body), { status, headers: { 'content-type': 'application/json' } });
}

function answering(answer: (request: Request) => Response) {
	const fetch = vi.fn(async (input: Request) => answer(input));
	return { fetch, client: createApiClient({ baseUrl: 'http://squirrel.test', fetch: fetch as typeof globalThis.fetch }) };
}

describe('duplicates api', () => {
	it('collects every page of a state', async () => {
		const { fetch, client } = answering((request) => {
			const cursor = new URL(request.url).searchParams.get('cursor');
			return reply(200, { items: [{ id: cursor ?? 'first' }], open_count: 2, next_cursor: cursor ? null : 'next' });
		});
		const result = await listDuplicates('resolved', client);
		expect(result).toEqual({ ok: true, data: [{ id: 'first' }, { id: 'next' }] });
		expect(new URL(fetch.mock.calls[0][0].url).searchParams.get('state')).toBe('resolved');
	});

	it('reads the open count from a one-group page', async () => {
		const { fetch, client } = answering(() => reply(200, { items: [], open_count: 3, next_cursor: null }));
		expect(await openDuplicateCount(client)).toEqual({ ok: true, data: 3 });
		expect(new URL(fetch.mock.calls[0][0].url).searchParams.get('limit')).toBe('1');
	});

	it('keeps the photo given', async () => {
		const { fetch, client } = answering(() => reply(200, { id: 'g', state: 'resolved' }));
		await keepDuplicate('g', 'photo', client);
		const request = fetch.mock.calls[0][0];
		expect(new URL(request.url).pathname).toBe('/api/duplicates/g/keep');
		expect(await request.json()).toEqual({ asset: 'photo' });
	});

	it('restores one photo or all, and reads 204 as a group that is gone', async () => {
		const { fetch, client } = answering(() => reply(204));
		expect(await restoreDuplicates('g', null, client)).toEqual({ ok: true, data: null });
		expect(await fetch.mock.calls[0][0].json()).toEqual({});
		await restoreDuplicates('g', 'photo', client);
		expect(await fetch.mock.calls[1][0].json()).toEqual({ asset: 'photo' });
	});

	it('reports a missing group', async () => {
		const { client } = answering(() => reply(404, { error: { code: 'not_found', message: 'not found' } }));
		const result = await keepDuplicate('g', 'photo', client);
		expect(result.ok).toBe(false);
	});
});
