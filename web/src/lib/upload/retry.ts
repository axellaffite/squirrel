import { UploadError } from '#lib/upload/errors.ts';

export interface RetryOptions {
	attempts: number;
	/** Waits `ms`, rejecting early with an AbortError when `signal` aborts. */
	sleep: (ms: number, signal: AbortSignal) => Promise<void>;
	/** Whether `error` is worth another attempt. */
	retryable: (error: unknown) => boolean;
	/** Source of jitter, in [0, 1). */
	random?: () => number;
}

const BASE_DELAY_MS = 500;
const MAX_DELAY_MS = 15_000;
/** Upper bound on a server-requested wait, so a bogus header cannot stall an upload for hours. */
const MAX_RETRY_AFTER_MS = 60_000;

const abortError = () => new DOMException('Upload cancelled', 'AbortError');

/** Resolves after `ms`, or rejects with an AbortError as soon as `signal` aborts. */
export function abortableDelay(ms: number, signal: AbortSignal): Promise<void> {
	return new Promise((resolve, reject) => {
		if (signal.aborted) return reject(abortError());
		const onAbort = () => {
			clearTimeout(timer);
			reject(abortError());
		};
		const timer = setTimeout(() => {
			signal.removeEventListener('abort', onAbort);
			resolve();
		}, ms);
		signal.addEventListener('abort', onAbort, { once: true });
	});
}

/**
 * Wait before attempt `attempt + 1`: the server's `Retry-After` when given (capped),
 * otherwise exponential backoff with full jitter, capped at 15 s.
 */
export function backoffDelay(attempt: number, retryAfterMs: number | null, random: () => number = Math.random): number {
	if (retryAfterMs !== null) return Math.min(retryAfterMs, MAX_RETRY_AFTER_MS);
	return Math.floor(random() * Math.min(MAX_DELAY_MS, BASE_DELAY_MS * 2 ** (attempt - 1)));
}

function retryAfterOf(error: unknown): number | null {
	return error instanceof UploadError ? error.retryAfterMs : null;
}

/** Runs `operation`, retrying with backoff while `retryable` says so. */
export async function withRetry<T>(
	operation: () => Promise<T>,
	options: RetryOptions,
	signal: AbortSignal
): Promise<T> {
	for (let attempt = 1; ; attempt += 1) {
		try {
			return await operation();
		} catch (error) {
			if (signal.aborted || !options.retryable(error) || attempt >= options.attempts) throw error;
			await options.sleep(backoffDelay(attempt, retryAfterOf(error), options.random), signal);
			signal.throwIfAborted();
		}
	}
}

/**
 * Runs `worker` over `items` with at most `concurrency` in flight. On the first failure
 * (or when `signal` aborts) the other lanes are aborted through their signal, and the
 * pool waits for every lane to settle before rethrowing: no work outlives the call.
 */
export async function runPool<T>(
	items: readonly T[],
	concurrency: number,
	worker: (item: T, signal: AbortSignal) => Promise<void>,
	signal: AbortSignal
): Promise<void> {
	signal.throwIfAborted();
	const inner = new AbortController();
	const forwardAbort = () => inner.abort(signal.reason);
	signal.addEventListener('abort', forwardAbort, { once: true });
	let next = 0;
	// Widened explicitly: TypeScript cannot see the lanes assigning it.
	let failure = null as { error: unknown } | null;
	const lane = async () => {
		while (!inner.signal.aborted && next < items.length) {
			const item = items[next++];
			try {
				await worker(item, inner.signal);
			} catch (error) {
				failure ??= { error };
				inner.abort();
			}
		}
	};
	try {
		await Promise.allSettled(Array.from({ length: Math.min(concurrency, items.length) }, lane));
	} finally {
		signal.removeEventListener('abort', forwardAbort);
	}
	if (failure) throw failure.error;
	signal.throwIfAborted();
}
