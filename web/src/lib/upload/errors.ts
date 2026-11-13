/** An HTTP failure; `retryable` for transient server-side conditions. */
export class UploadError extends Error {
	constructor(
		message: string,
		readonly status: number,
		readonly retryable: boolean
	) {
		super(message);
		this.name = 'UploadError';
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

/** Server said "try again", or the network failed (fetch rejects with a TypeError). */
export function isTransient(error: unknown): boolean {
	return isServerRetryable(error) || error instanceof TypeError;
}
