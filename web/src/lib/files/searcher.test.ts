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

	it('keeps the shown results when backspacing to them while a longer query is in flight', async () => {
		let release: (() => void) | undefined;
		const signals: AbortSignal[] = [];
		const run = vi.fn<RunSearch>((q, _offset, signal) => {
			signals.push(signal);
			if (q === 'abcd') return new Promise((resolve) => (release = () => resolve(results(['abcd']))));
			return Promise.resolve(results(['abc']));
		});
		const searcher = new Searcher(run, 10);
		searcher.now('abc');
		await vi.advanceTimersByTimeAsync(0);
		searcher.type('abcd');
		await vi.advanceTimersByTimeAsync(10);
		searcher.type('abc');
		expect(signals[1].aborted).toBe(true);
		release?.();
		await vi.advanceTimersByTimeAsync(20);
		expect(searcher.query).toBe('abc');
		expect(searcher.items.map((h) => (h as { node: { id: string } }).node.id)).toEqual(['abc']);
		expect(run).toHaveBeenCalledTimes(2);
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

	it('retries once after a rate limit, then says to wait', async () => {
		const tooFast = { ok: false as const, error: 'Trying again', status: 429, retryAfterMs: 1000 };
		const run = vi.fn<RunSearch>().mockResolvedValueOnce(tooFast).mockResolvedValueOnce(results(['a']));
		const searcher = new Searcher(run, 10);
		searcher.now('lease');
		await vi.advanceTimersByTimeAsync(0);
		expect(searcher.error).toBe('Trying again');
		await vi.advanceTimersByTimeAsync(1000);
		expect(run).toHaveBeenCalledTimes(2);
		expect(searcher.error).toBeNull();
		expect(searcher.items).toHaveLength(1);

		run.mockResolvedValueOnce(tooFast).mockResolvedValueOnce(tooFast);
		searcher.now('other');
		await vi.advanceTimersByTimeAsync(1000);
		expect(run).toHaveBeenCalledTimes(4);
		expect(searcher.error).toMatch(/still busy/);
	});

	it('clears older results when a different query fails', async () => {
		const run = vi
			.fn<RunSearch>()
			.mockResolvedValueOnce(results(['a']))
			.mockResolvedValueOnce({ ok: false, error: 'took too long', status: 503 });
		const searcher = new Searcher(run, 10);
		searcher.now('lease');
		await vi.advanceTimersByTimeAsync(0);
		searcher.now('other');
		await vi.advanceTimersByTimeAsync(0);
		expect(searcher.items).toEqual([]);
		expect(searcher.query).toBe('');
		expect(searcher.error).toBe('took too long');
	});

	it('loads more pages and keeps errors calm', async () => {
		const run = vi
			.fn<RunSearch>()
			.mockResolvedValueOnce(results(['a', 'b'], true))
			.mockResolvedValueOnce({ ok: false, error: 'slow down', status: 500 });
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
