import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import { Searcher, type RunSearch } from '#lib/files/searcher.svelte.ts';

const hit = (id: string) => ({ node: { id }, snippet: [] }) as never;

function results(ids: string[], hasMore = false) {
	return { ok: true as const, data: { items: ids.map(hit), has_more: hasMore } };
}

beforeEach(() => vi.useFakeTimers());
afterEach(() => vi.useRealTimers());

describe('Searcher', () => {
	it('waits for a pause in typing and sends only the last query', async () => {
		const run = vi.fn<RunSearch>(async (q) => results([q]));
		const searcher = new Searcher(run, 250);
		searcher.type('fa');
		await vi.advanceTimersByTimeAsync(100);
		searcher.type('fac');
		await vi.advanceTimersByTimeAsync(100);
		searcher.type('fact');
		await vi.advanceTimersByTimeAsync(250);
		expect(run).toHaveBeenCalledTimes(1);
		expect(run.mock.calls[0][0]).toBe('fact');
		expect(searcher.query).toBe('fact');
		expect(searcher.items).toHaveLength(1);
	});

	it('aborts the request a newer query replaced and ignores its answer', async () => {
		const signals: AbortSignal[] = [];
		let release: (() => void) | undefined;
		const run = vi.fn<RunSearch>((q, _offset, signal) => {
			signals.push(signal);
			if (q === 'first') return new Promise((resolve) => (release = () => resolve(results(['old']))));
			return Promise.resolve(results(['new']));
		});
		const searcher = new Searcher(run, 10);
		searcher.type('first');
		await vi.advanceTimersByTimeAsync(10);
		searcher.type('second');
		await vi.advanceTimersByTimeAsync(10);
		expect(signals[0].aborted).toBe(true);
		release?.();
		await vi.advanceTimersByTimeAsync(0);
		expect(searcher.items.map((h) => (h as { node: { id: string } }).node.id)).toEqual(['new']);
	});

	it('clears for queries too short to send', async () => {
		const run = vi.fn<RunSearch>(async () => results(['a']));
		const searcher = new Searcher(run, 10);
		searcher.now('lease');
		await vi.advanceTimersByTimeAsync(0);
		searcher.type('l');
		expect(searcher.items).toEqual([]);
		expect(searcher.query).toBe('');
		await vi.advanceTimersByTimeAsync(20);
		expect(run).toHaveBeenCalledTimes(1);
	});

	it('loads more pages and keeps errors calm', async () => {
		const run = vi
			.fn<RunSearch>()
			.mockResolvedValueOnce(results(['a', 'b'], true))
			.mockResolvedValueOnce({ ok: false, error: 'slow down', status: 429 });
		const searcher = new Searcher(run, 10);
		searcher.now('lease');
		await vi.advanceTimersByTimeAsync(0);
		expect(searcher.hasMore).toBe(true);
		searcher.more();
		await vi.advanceTimersByTimeAsync(0);
		expect(run.mock.calls[1][1]).toBe(2);
		expect(searcher.error).toBe('slow down');
		expect(searcher.items).toHaveLength(2);
	});
});
