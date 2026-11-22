import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import { SEARCH_BUSY, SEARCH_UNAVAILABLE, type SearchPage } from '#lib/explore/api.ts';
import { BUSY_RETRIES, BUSY_RETRY_MS, orderFromParams, PhotoSearcher, STILL_TOO_FAST, TOO_LONG, withOrder, type RunPhotoSearch } from '#lib/explore/searcher.svelte.ts';

const photo = (id: string) => ({ id }) as never;
const ids = (searcher: PhotoSearcher) => searcher.items.map((item) => (item as { id: string }).id);

function results(list: string[], hasMore = false, nextCursor: string | null = null) {
	return { ok: true as const, data: { items: list.map(photo), has_more: hasMore, next_cursor: nextCursor } };
}

const isFirst = (page: SearchPage) => (page.order === 'date' ? page.cursor === null : page.offset === 0);

beforeEach(() => vi.useFakeTimers());
afterEach(() => vi.useRealTimers());

describe('the order in the address', () => {
	it('defaults to the timeline and keeps only best match', () => {
		expect(orderFromParams(new URLSearchParams(''))).toBe('date');
		expect(orderFromParams(new URLSearchParams('order=relevance'))).toBe('relevance');
		expect(orderFromParams(new URLSearchParams('order=nonsense'))).toBe('date');
		const best = withOrder('http://local/search?q=dog&tag=beach', 'relevance');
		expect(best.search).toBe('?q=dog&tag=beach&order=relevance');
		expect(withOrder(best, 'date').search).toBe('?q=dog&tag=beach');
	});
});

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

	it('pages a timeline with the cursor until the server has no more', async () => {
		const run = vi.fn<RunPhotoSearch>(async (_q, page) => (isFirst(page) ? results(['a', 'b'], true, 'next') : results(['c'])));
		const searcher = new PhotoSearcher(run, 10);
		searcher.now('dog');
		await vi.advanceTimersByTimeAsync(0);
		expect(run.mock.calls[0][1]).toEqual({ order: 'date', cursor: null });
		searcher.more();
		await vi.advanceTimersByTimeAsync(0);
		expect(run.mock.calls[1][1]).toEqual({ order: 'date', cursor: 'next' });
		expect(ids(searcher)).toEqual(['a', 'b', 'c']);
		expect(searcher.hasMore).toBe(false);
	});

	it('pages the best matches by offset', async () => {
		const run = vi.fn<RunPhotoSearch>(async (_q, page) => (isFirst(page) ? results(['a', 'b'], true) : results(['c'])));
		const searcher = new PhotoSearcher(run, 10);
		searcher.setOrder('relevance');
		searcher.now('dog');
		await vi.advanceTimersByTimeAsync(0);
		expect(run.mock.calls[0][1]).toEqual({ order: 'relevance', offset: 0 });
		searcher.more();
		await vi.advanceTimersByTimeAsync(0);
		expect(run.mock.calls[1][1]).toEqual({ order: 'relevance', offset: 2 });
		expect(ids(searcher)).toEqual(['a', 'b', 'c']);
	});

	it('searches again in the new order, and shows the old one until it answers', async () => {
		let release: (() => void) | undefined;
		const run = vi.fn<RunPhotoSearch>((_q, page) =>
			page.order === 'date'
				? Promise.resolve(results(['new', 'old'], true, 'next'))
				: new Promise((resolve) => (release = () => resolve(results(['old', 'new'], true))))
		);
		const searcher = new PhotoSearcher(run, 10);
		searcher.now('dog');
		await vi.advanceTimersByTimeAsync(0);
		expect(searcher.itemsOrder).toBe('date');
		searcher.setOrder('relevance');
		searcher.setOrder('relevance');
		expect(run).toHaveBeenCalledTimes(2);
		expect(searcher.order).toBe('relevance');
		expect(searcher.itemsOrder).toBe('date');
		release?.();
		await vi.advanceTimersByTimeAsync(0);
		expect(searcher.itemsOrder).toBe('relevance');
		expect(ids(searcher)).toEqual(['old', 'new']);
	});

	it('changes only the order to come without a query', async () => {
		const run = vi.fn<RunPhotoSearch>(async () => results(['a']));
		const searcher = new PhotoSearcher(run, 10);
		searcher.setOrder('relevance');
		expect(run).not.toHaveBeenCalled();
		searcher.now('dog');
		await vi.advanceTimersByTimeAsync(0);
		expect(run.mock.calls[0][1]).toEqual({ order: 'relevance', offset: 0 });
	});

	it('asks for no next date page without a cursor', async () => {
		const run = vi.fn<RunPhotoSearch>(async () => results(['a'], true, null));
		const searcher = new PhotoSearcher(run, 10);
		searcher.now('dog');
		await vi.advanceTimersByTimeAsync(0);
		expect(searcher.hasMore).toBe(false);
		searcher.more();
		expect(run).toHaveBeenCalledTimes(1);
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

	it('says a query is too long instead of doing nothing', async () => {
		const run = vi.fn<RunPhotoSearch>(async () => results(['a']));
		const searcher = new PhotoSearcher(run, 10);
		searcher.now('x'.repeat(201));
		expect(searcher.error).toBe(TOO_LONG);
		searcher.type('dog');
		await vi.advanceTimersByTimeAsync(10);
		expect(searcher.error).toBeNull();
		searcher.type('y'.repeat(250));
		expect(searcher.error).toBe(TOO_LONG);
		expect(run).toHaveBeenCalledTimes(1);
	});

	it('waits out a rate limit: the same query typed again sends nothing sooner', async () => {
		let calls = 0;
		const run = vi.fn<RunPhotoSearch>(async () =>
			++calls === 1 ? { ok: false, error: 'slow down', status: 429, retryAfterMs: 1000 } : results(['a'])
		);
		const searcher = new PhotoSearcher(run, 10);
		searcher.now('dog');
		await vi.advanceTimersByTimeAsync(0);
		searcher.type('dog');
		await vi.advanceTimersByTimeAsync(500);
		expect(run).toHaveBeenCalledTimes(1);
		await vi.advanceTimersByTimeAsync(500);
		expect(run).toHaveBeenCalledTimes(2);
		expect(ids(searcher)).toEqual(['a']);
	});

	it('loads no next page while a request is pending', async () => {
		let release: (() => void) | undefined;
		const run = vi.fn<RunPhotoSearch>((_q, page) =>
			isFirst(page) ? Promise.resolve(results(['a'], true, 'next')) : new Promise((resolve) => (release = () => resolve(results(['b']))))
		);
		const searcher = new PhotoSearcher(run, 10);
		searcher.now('dog');
		await vi.advanceTimersByTimeAsync(0);
		searcher.more();
		searcher.more();
		expect(run).toHaveBeenCalledTimes(2);
		release?.();
		await vi.advanceTimersByTimeAsync(0);
		expect(ids(searcher)).toEqual(['a', 'b']);
	});
});
