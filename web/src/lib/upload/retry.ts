export interface RetryOptions {
	attempts: number;
	sleep: (ms: number) => Promise<void>;
	/** Whether `error` is worth another attempt. */
	retryable: (error: unknown) => boolean;
}

const BASE_DELAY_MS = 500;

export const delay = (ms: number) => new Promise<void>((resolve) => setTimeout(resolve, ms));

/** Runs `operation`, retrying with exponential backoff while `retryable` says so. */
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
			await options.sleep(BASE_DELAY_MS * 2 ** (attempt - 1));
		}
	}
}

/** Runs `worker` over `items` with at most `concurrency` in flight; stops on the first failure. */
export async function runPool<T>(items: T[], concurrency: number, worker: (item: T) => Promise<void>): Promise<void> {
	let next = 0;
	let failed = false;
	const lane = async () => {
		while (!failed && next < items.length) {
			const item = items[next++];
			try {
				await worker(item);
			} catch (error) {
				failed = true;
				throw error;
			}
		}
	};
	await Promise.all(Array.from({ length: Math.min(concurrency, items.length) }, lane));
}
