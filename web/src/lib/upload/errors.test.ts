import { describe, expect, it } from 'vitest';
import { isTransient, NetworkError, parseRetryAfter, UploadError } from '#lib/upload/errors.ts';

describe('parseRetryAfter', () => {
	const now = Date.parse('2026-01-01T00:00:00Z');

	it.each([
		[null, null],
		['', null],
		['soon', null],
		['-1', null],
		['3', 3000],
		['0', 0],
		['Thu, 01 Jan 2026 00:00:05 GMT', 5000],
		['Wed, 31 Dec 2025 23:59:00 GMT', 0]
	])('%o → %o', (value, expected) => expect(parseRetryAfter(value, now)).toBe(expected));
});

describe('isTransient', () => {
	it('retries network failures and retryable server answers only', () => {
		expect(isTransient(new NetworkError(new TypeError('Failed to fetch')))).toBe(true);
		expect(isTransient(new UploadError('busy', 503, true))).toBe(true);
		expect(isTransient(new UploadError('quota', 507, false))).toBe(false);
		expect(isTransient(new TypeError('x is not a function'))).toBe(false);
		expect(isTransient(new DOMException('aborted', 'AbortError'))).toBe(false);
	});
});
