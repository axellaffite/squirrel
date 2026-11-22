import { describe, expect, it, vi } from 'vitest';
import { createApiClient } from '#lib/api/client.ts';
import { peopleOnPhoto, personChipLabel, placesByCountry, SEARCH_BUSY, SEARCH_UNAVAILABLE, PHOTO_DATE_PAGE, searchPhotos, warmUpSearch } from '#lib/explore/api.ts';

function reply(status: number, body: unknown, headers: Record<string, string> = {}) {
	return new Response(JSON.stringify(body), { status, headers: { 'content-type': 'application/json', ...headers } });
}

/** A client whose every request answers `answer()`. */
function answering(answer: () => Response | Promise<Response>) {
	const fetch = vi.fn(async (_input: Request) => answer());
	return { fetch, client: createApiClient({ baseUrl: 'http://squirrel.test', fetch: fetch as typeof globalThis.fetch }) };
}

describe('searchPhotos', () => {
	it('sends the query, the page and the filter', async () => {
		const { fetch, client } = answering(() => reply(200, { items: [], has_more: false }));
		const result = await searchPhotos('dog', { order: 'relevance', offset: 0 }, { tag: 'beach', city: 'Paris' }, new AbortController().signal, client);
		expect(result.ok).toBe(true);
		const url = new URL(fetch.mock.calls[0][0].url);
		expect(url.pathname).toBe('/api/search/photos');
		expect(url.searchParams.get('q')).toBe('dog');
		expect(url.searchParams.get('tag')).toBe('beach');
		expect(url.searchParams.has('city')).toBe(false);
	});

	it('pages by date with the cursor, never an offset', async () => {
		const { fetch, client } = answering(() => reply(200, { items: [], has_more: false, next_cursor: null }));
		const signal = new AbortController().signal;
		await searchPhotos('dog', { order: 'date', cursor: null }, {}, signal, client);
		await searchPhotos('dog', { order: 'date', cursor: 'abc' }, {}, signal, client);
		const first = new URL(fetch.mock.calls[0][0].url).searchParams;
		const second = new URL(fetch.mock.calls[1][0].url).searchParams;
		expect(first.get('order')).toBe('date');
		expect(first.has('cursor')).toBe(false);
		expect(first.has('offset')).toBe(false);
		expect(first.get('limit')).toBe(String(PHOTO_DATE_PAGE));
		expect(second.get('cursor')).toBe('abc');
		await searchPhotos('dog', { order: 'relevance', offset: 150 }, {}, signal, client);
		const best = new URL(fetch.mock.calls[2][0].url).searchParams;
		expect(best.has('order')).toBe(false);
		expect([best.get('offset'), best.get('limit')]).toEqual(['150', '50']);
	});

	it('words an unset-up server, a busy one and a rate limit calmly', async () => {
		const signal = new AbortController().signal;
		const off = answering(() => reply(503, { error: { code: 'ml_unavailable', message: 'x' } })).client;
		expect(await searchPhotos('dog', { order: 'relevance', offset: 0 }, {}, signal, off)).toEqual({ ok: false, error: SEARCH_UNAVAILABLE, status: 503, unavailable: true });
		const busy = answering(() => reply(503, { error: { code: 'busy', message: 'x' } })).client;
		expect(await searchPhotos('dog', { order: 'relevance', offset: 0 }, {}, signal, busy)).toEqual({ ok: false, error: SEARCH_BUSY, status: 503 });
		const fast = answering(() => reply(429, { error: { code: 'rate_limited', message: 'x' } }, { 'Retry-After': '2' })).client;
		const limited = await searchPhotos('dog', { order: 'relevance', offset: 0 }, {}, signal, fast);
		expect(limited.ok === false && 'retryAfterMs' in limited ? limited.retryAfterMs : null).toBe(2000);
	});

	it('reports an aborted search as such', async () => {
		const controller = new AbortController();
		const { client } = answering(() => {
			controller.abort();
			throw new DOMException('aborted', 'AbortError');
		});
		expect(await searchPhotos('dog', { order: 'relevance', offset: 0 }, {}, controller.signal, client)).toEqual({ ok: false, aborted: true });
	});

	it('warm-up tells whether search is set up', async () => {
		expect(await warmUpSearch(answering(() => new Response(null, { status: 204 })).client)).toBe(true);
		const off = answering(() => reply(503, { error: { code: 'ml_unavailable', message: 'x' } })).client;
		expect(await warmUpSearch(off)).toBe(false);
	});
});

describe('placesByCountry', () => {
	it('groups places under their country in rank order', () => {
		const place = (country_code: string, city: string) => ({ country_code, country: country_code, city, count: 1 }) as never;
		const groups = placesByCountry([place('FR', 'Paris'), place('IT', 'Rome'), place('FR', 'Lyon')]);
		expect(groups.map((g) => [g.code, g.places.length])).toEqual([
			['FR', 2],
			['IT', 1]
		]);
	});
});

describe('peopleOnPhoto', () => {
	it('gives each person their face on the photo, if any', () => {
		const box = { x: 0.1, y: 0.1, width: 0.2, height: 0.2 };
		const faces = [
			{ id: 'f1', photo_id: 'p', thumbnail_version: 1, box, score: 0.9, assigned_by: 'auto', person: { id: 'a', name: 'Anna' } },
			{ id: 'f2', photo_id: 'p', thumbnail_version: 1, box, score: 0.9, assigned_by: 'auto', person: null }
		] as never[];
		expect(peopleOnPhoto([{ id: 'a', name: 'Anna' }, { id: 'b', name: null }], faces)).toEqual([
			{ id: 'a', name: 'Anna', face: { face_id: 'f1', photo_id: 'p', thumbnail_version: 1, box } },
			{ id: 'b', name: null, face: null }
		]);
	});
});

describe('personChipLabel', () => {
	it('names a hidden person, or says it is hidden', () => {
		expect(personChipLabel({ name: 'Anna', hidden: true })).toBe('Anna');
		expect(personChipLabel({ name: null, hidden: true })).toBe('Hidden person');
		expect(personChipLabel({ name: null, hidden: false })).toBeNull();
		expect(personChipLabel(undefined)).toBeNull();
	});
});
