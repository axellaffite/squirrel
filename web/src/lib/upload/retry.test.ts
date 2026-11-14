import { describe, expect, it, vi } from 'vitest';
import { UploadError } from '#lib/upload/errors.ts';
import { abortableDelay, backoffDelay, runPool, withRetry } from '#lib/upload/retry.ts';

const busy = (retryAfterMs: number | null = null) => new UploadError('busy', 503, true, retryAfterMs);
const signal = () => new AbortController().signal;

function deferred<T = void>() {
	let resolve!: (value: T) => void;
	let reject!: (error: unknown) => void;
	const promise = new Promise<T>((res, rej) => {
		resolve = res;
		reject = rej;
	});
	return { promise, resolve, reject };
}

describe('backoffDelay', () => {
	it('uses exponential backoff with full jitter, capped', () => {
		const half = () => 0.5;
		expect([1, 2, 3, 4, 5, 6, 7].map((attempt) => backoffDelay(attempt, null, half))).toEqual([
			250, 500, 1000, 2000, 4000, 7500, 7500
		]);
		expect(backoffDelay(10, null, () => 0.999_999)).toBeLessThanOrEqual(15_000);
	});

	it('honors the server Retry-After, within reason', () => {
		expect(backoffDelay(1, 3000, () => 0)).toBe(3000);
		expect(backoffDelay(1, 10 * 60_000, () => 0)).toBe(60_000);
	});
});

describe('withRetry', () => {
	it('retries until the attempts run out, sleeping between them', async () => {
		const sleeps: number[] = [];
		const operation = vi.fn(async () => {
			throw busy(1000);
		});
		const options = {
			attempts: 3,
			retryable: () => true,
			sleep: async (ms: number) => void sleeps.push(ms)
		};
		await expect(withRetry(operation, options, signal())).rejects.toThrow('busy');
		expect(operation).toHaveBeenCalledTimes(3);
		expect(sleeps).toEqual([1000, 1000]);
	});

	it('stops waiting as soon as it is cancelled', async () => {
		const controller = new AbortController();
		const operation = vi.fn(async () => {
			throw busy();
		});
		const options = { attempts: 5, retryable: () => true, sleep: abortableDelay };
		const pending = withRetry(operation, options, controller.signal);
		await new Promise((resolve) => setTimeout(resolve, 0));
		controller.abort();
		await expect(pending).rejects.toMatchObject({ name: 'AbortError' });
		expect(operation).toHaveBeenCalledTimes(1);
	});
});

describe('abortableDelay', () => {
	it('resolves after the delay', async () => {
		vi.useFakeTimers();
		try {
			const pending = abortableDelay(1000, signal());
			await vi.advanceTimersByTimeAsync(1000);
			await expect(pending).resolves.toBeUndefined();
		} finally {
			vi.useRealTimers();
		}
	});

	it('rejects at once when cancelled, before or during the wait', async () => {
		const controller = new AbortController();
		const pending = abortableDelay(60_000, controller.signal);
		controller.abort();
		await expect(pending).rejects.toMatchObject({ name: 'AbortError' });
		await expect(abortableDelay(10, controller.signal)).rejects.toMatchObject({ name: 'AbortError' });
	});
});

describe('runPool', () => {
	it('aborts the other lanes on failure and waits for them before rethrowing', async () => {
		const slow = deferred();
		const seen: AbortSignal[] = [];
		const events: string[] = [];
		const pool = runPool(
			[0, 1, 2, 3],
			2,
			async (item, laneSignal) => {
				seen.push(laneSignal);
				if (item === 0) {
					await slow.promise;
					events.push('slow settled');
					return;
				}
				throw new Error('boom');
			},
			signal()
		);
		const outcome = pool.catch((error: Error) => events.push(`pool ${error.message}`));
		await new Promise((resolve) => setTimeout(resolve, 0));
		expect(seen[0].aborted).toBe(true);
		slow.resolve();
		await outcome;
		expect(events).toEqual(['slow settled', 'pool boom']);
		expect(seen).toHaveLength(2);
	});

	it('stops starting work once the caller cancels', async () => {
		const controller = new AbortController();
		const worker = vi.fn(async (_item: number, laneSignal: AbortSignal) => {
			controller.abort();
			expect(laneSignal.aborted).toBe(true);
		});
		await expect(runPool([0, 1, 2], 1, worker, controller.signal)).rejects.toMatchObject({ name: 'AbortError' });
		expect(worker).toHaveBeenCalledTimes(1);
	});

	it('runs every item when nothing fails', async () => {
		const done: number[] = [];
		await runPool([0, 1, 2, 3, 4], 2, async (item) => void done.push(item), signal());
		expect(done.sort()).toEqual([0, 1, 2, 3, 4]);
	});
});
