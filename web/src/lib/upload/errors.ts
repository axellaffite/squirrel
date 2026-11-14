/** An HTTP failure; `retryable` for transient server-side conditions. */
export class UploadError extends Error {
	constructor(
		message: string,
		readonly status: number,
		readonly retryable: boolean,
		/** How long the server asked us to wait (`Retry-After`), if it said. */
		readonly retryAfterMs: number | null = null
	) {
		super(message);
		this.name = 'UploadError';
	}
}

/** The request never got an answer: fetch itself failed (offline, connection reset, DNS…). */
export class NetworkError extends Error {
	constructor(cause: unknown) {
		super('The connection to the server was lost. Check your network and try again.', { cause });
		this.name = 'NetworkError';
	}
}

/** Statuses worth retrying: timeouts, rate limits and server errors other than a full disk. */
export function isRetryableStatus(status: number): boolean {
	return status === 408 || status === 429 || (status >= 500 && status !== 507);
}

/** The server answered "try again later". */
export function isServerRetryable(error: unknown): boolean {
	return error instanceof UploadError && error.retryable;
}

/** Server said "try again", or the network failed. Other errors (bugs, cancellations) are final. */
export function isTransient(error: unknown): boolean {
	return isServerRetryable(error) || error instanceof NetworkError;
}

export function isAbortError(error: unknown): boolean {
	return error instanceof Error && error.name === 'AbortError';
}

/** `Retry-After` in milliseconds: delay-seconds or an HTTP date; null when absent or invalid. */
export function parseRetryAfter(value: string | null, now: number = Date.now()): number | null {
	const trimmed = value?.trim();
	if (!trimmed) return null;
	if (/^\d+$/.test(trimmed)) return Number(trimmed) * 1000;
	if (!/[a-z]/i.test(trimmed)) return null;
	const date = Date.parse(trimmed);
	return Number.isNaN(date) ? null : Math.max(0, date - now);
}
