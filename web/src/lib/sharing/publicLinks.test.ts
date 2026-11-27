import { describe, expect, it } from 'vitest';
import { expiryFromDay, linkMeta, linkPasswordProblem, today, type PublicLink } from './publicLinks.ts';

const link = (over: Partial<PublicLink> = {}): PublicLink => ({
	id: 'l1',
	has_password: false,
	expires_at: null,
	expired: false,
	created_at: '2026-11-27T10:00:00Z',
	...over
});

describe('expiryFromDay', () => {
	const now = new Date(2026, 10, 27, 15, 0, 0);

	it('ends at the last second of the picked local day', () => {
		const result = expiryFromDay('2026-12-04', now);
		expect(result).toEqual({ expiresAt: new Date(2026, 11, 4, 23, 59, 59).toISOString() });
	});

	it('accepts today, whose end is still ahead', () => {
		expect(expiryFromDay('2026-11-27', now)).toEqual({ expiresAt: new Date(2026, 10, 27, 23, 59, 59).toISOString() });
	});

	it('means no expiry when empty', () => {
		expect(expiryFromDay('', now)).toEqual({ expiresAt: null });
		expect(expiryFromDay('  ', now)).toEqual({ expiresAt: null });
	});

	it('refuses past and impossible days', () => {
		expect(expiryFromDay('2026-11-26', now)).toEqual({ error: 'Pick a day from today on.' });
		expect(expiryFromDay('2026-02-30', now)).toEqual({ error: 'Pick a valid date.' });
		expect(expiryFromDay('next week', now)).toEqual({ error: 'Pick a valid date.' });
	});
});

describe('linkPasswordProblem', () => {
	it('allows no password, or the server lengths', () => {
		expect(linkPasswordProblem('')).toBeNull();
		expect(linkPasswordProblem('a'.repeat(10))).toBeNull();
		expect(linkPasswordProblem('a'.repeat(256))).toBeNull();
	});

	it('explains what is wrong otherwise', () => {
		expect(linkPasswordProblem('short')).toMatch(/at least 10/);
		expect(linkPasswordProblem('a'.repeat(257))).toMatch(/at most 256/);
	});
});

describe('today', () => {
	it('pads the local date', () => {
		expect(today(new Date(2026, 0, 5, 1, 2, 3))).toBe('2026-01-05');
	});
});

describe('linkMeta', () => {
	it('says when it expires and whether it has a password', () => {
		expect(linkMeta(link())).toBe('No expiry');
		expect(linkMeta(link({ has_password: true }))).toBe('No expiry · Password');
		expect(linkMeta(link({ expired: true, expires_at: '2026-11-01T00:00:00Z' }))).toBe('Expired');
		expect(linkMeta(link({ expires_at: '2026-12-04T22:59:59Z' }))).toMatch(/^Expires /);
	});
});
