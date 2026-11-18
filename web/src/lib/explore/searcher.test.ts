import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import { SEARCH_BUSY, SEARCH_UNAVAILABLE } from '#lib/explore/api.ts';
import { BUSY_RETRIES, BUSY_RETRY_MS, PhotoSearcher, STILL_TOO_FAST, type RunPhotoSearch } from '#lib/explore/searcher.svelte.ts';

const photo = (id: string) => ({ id }) as never;
const ids = (searcher: PhotoSearcher) => searcher.items.map((item) => (item as { id: string }).id);

function results(list: string[], hasMore = false) {
	return { ok: true as const, data: { items: list.map(photo), has_more: hasMore } };
}

beforeEach(() => vi.useFakeTimers());
afterEach(() => vi.useRealTimers());

describe('PhotoSearcher', () => {
	it('waits for a pause in typing and sends only the last query', async () => {
		const run = vi.fn<RunPhotoSearch>(async (q) => results([q]));
		const searcher = new PhotoSearcher(run, 300);
		searcher.type('do');
		await vi.advanceTimersByTimeAsync(100);
		searcher.type('dog');
		await vi.advanceTimersByTimeAsync(300);
		expect(run).toHaveBeenCalledTimes(1);
		expect(run.mock.calls[0][0]).toBe('dog');
		expect(ids(searcher)).toEqual(['dog']);
	});

	it('aborts the request a newer query replaced and ignores its answer', async () => {
		const signals: AbortSignal[] = [];
		let release: (() => void) | undefined;
		const run = vi.fn<RunPhotoSearch>((q, _o, _f, signal) => {
			signals.push(signal);
			if (q === 'cat') return new Promise((resolve) => (release = () => resolve(results(['old']))));
			return Promise.resolve(results(['new']));
		});
		const searcher = new PhotoSearcher(run, 10);
		searcher.type('cat');
		await vi.advanceTimersByTimeAsync(10);
		searcher.type('dog');
		await vi.advanceTimersByTimeAsync(10);
		expect(signals[0].aborted).toBe(true);
		release?.();
		await vi.advanceTimersByTimeAsync(0);
		expect(ids(searcher)).toEqual(['new']);
	});

	it('searches again at once with a new filter', async () => {
		const run = vi.fn<RunPhotoSearch>(async (q, _o, filter) => results([`${q}:${filter.tag ?? ''}`]));
		const searcher = new PhotoSearcher(run, 10);
		searcher.now('dog');
		await vi.advanceTimersByTimeAsync(0);
		searcher.setFilter({ tag: 'beach' });
		await vi.advanceTimersByTimeAsync(0);
		expect(ids(searcher)).toEqual(['dog:beach']);
		searcher.setFilter({ tag: 'beach' });
		expect(run).toHaveBeenCalledTimes(2);
	});

	it('pages with more until the server has no more', async () => {
		const run = vi.fn<RunPhotoSearch>(async (_q, offset) => (offset === 0 ? results(['a', 'b'], true) : results(['c'])));
		const searcher = new PhotoSearcher(run, 10);
		searcher.now('dog');
		await vi.advanceTimersByTimeAsync(0);
		searcher.more();
		await vi.advanceTimersByTimeAsync(0);
		expect(run.mock.calls[1][1]).toBe(2);
		expect(ids(searcher)).toEqual(['a', 'b', 'c']);
		expect(searcher.hasMore).toBe(false);
	});

	it('says when search is not set up, without results', async () => {
		const run = vi.fn<RunPhotoSearch>(async () => ({ ok: false, error: SEARCH_UNAVAILABLE, status: 503, unavailable: true }));
		const searcher = new PhotoSearcher(run, 10);
		searcher.now('dog');
		await vi.advanceTimersByTimeAsync(0);
		expect(searcher.unavailable).toBe(true);
		expect(searcher.error).toBe(SEARCH_UNAVAILABLE);
		expect(searcher.items).toEqual([]);
	});

	it('retries once after a rate limit, then gives up calmly', async () => {
		const run = vi.fn<RunPhotoSearch>(async () => ({ ok: false, error: 'slow down', status: 429, retryAfterMs: 1000 }));
		const searcher = new PhotoSearcher(run, 10);
		searcher.now('dog');
		await vi.advanceTimersByTimeAsync(0);
		expect(searcher.error).toBe('slow down');
		await vi.advanceTimersByTimeAsync(1000);
		expect(run).toHaveBeenCalledTimes(2);
		expect(searcher.error).toBe(STILL_TOO_FAST);
	});

	it('clears for an empty query', async () => {
		const run = vi.fn<RunPhotoSearch>(async () => results(['a']));
		const searcher = new PhotoSearcher(run, 10);
		searcher.now('dog');
		await vi.advanceTimersByTimeAsync(0);
		searcher.type('   ');
		expect(searcher.items).toEqual([]);
		expect(searcher.query).toBe('');
	});

	it('waits for the text model to start, then shows the results', async () => {
		let calls = 0;
		const run = vi.fn<RunPhotoSearch>(async () => (++calls < 3 ? { ok: false, error: SEARCH_BUSY, status: 503 } : results(['a'])));
		const searcher = new PhotoSearcher(run, 10);
		searcher.now('dog');
		await vi.advanceTimersByTimeAsync(0);
		expect(searcher.loading).toBe(true);
		expect(searcher.error).toBeNull();
		await vi.advanceTimersByTimeAsync(2 * BUSY_RETRY_MS);
		expect(ids(searcher)).toEqual(['a']);
	});

	it('says the server is busy when it stays busy', async () => {
		const run = vi.fn<RunPhotoSearch>(async () => ({ ok: false, error: SEARCH_BUSY, status: 503 }));
		const searcher = new PhotoSearcher(run, 10);
		searcher.now('dog');
		await vi.advanceTimersByTimeAsync((BUSY_RETRIES + 1) * BUSY_RETRY_MS);
		expect(run).toHaveBeenCalledTimes(BUSY_RETRIES + 1);
		expect(searcher.error).toBe(SEARCH_BUSY);
		expect(searcher.loading).toBe(false);
	});

	it('typing the same query during a busy retry keeps the retry going', async () => {
		let calls = 0;
		const run = vi.fn<RunPhotoSearch>(async () => (++calls < 2 ? { ok: false, error: SEARCH_BUSY, status: 503 } : results(['a'])));
		const searcher = new PhotoSearcher(run, 10);
		searcher.now('dog');
		await vi.advanceTimersByTimeAsync(0);
		searcher.type('dog ');
		await vi.advanceTimersByTimeAsync(BUSY_RETRY_MS);
		expect(ids(searcher)).toEqual(['a']);
		expect(searcher.loading).toBe(false);
	});
});
