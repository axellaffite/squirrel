import { describe, expect, it } from 'vitest';
import { expiryNotice, newestInvitation, validityDays } from '#lib/invitations.ts';

const at = (created_at: string, expires_at: string) => ({ created_at, expires_at });

describe('validityDays', () => {
	it('derives whole days from created_at and expires_at', () => {
		expect(validityDays(at('2026-11-13T10:00:00Z', '2026-11-20T10:00:00Z'))).toBe(7);
		expect(validityDays(at('2026-11-13T10:00:00Z', '2026-11-16T10:00:01.5Z'))).toBe(3);
	});

	it('reports at least one day for short invitations', () => {
		expect(validityDays(at('2026-11-13T10:00:00Z', '2026-11-13T12:00:00Z'))).toBe(1);
	});

	it('returns null when unknown or invalid', () => {
		expect(validityDays(null)).toBeNull();
		expect(validityDays(at('nope', '2026-11-20T10:00:00Z'))).toBeNull();
		expect(validityDays(at('2026-11-20T10:00:00Z', '2026-11-13T10:00:00Z'))).toBeNull();
	});
});

describe('newestInvitation', () => {
	it('picks the most recently created invitation', () => {
		const older = at('2026-11-01T00:00:00Z', '2026-11-08T00:00:00Z');
		const newer = at('2026-11-10T00:00:00Z', '2026-11-24T00:00:00Z');
		expect(newestInvitation([older, newer])).toBe(newer);
		expect(newestInvitation([newer, older])).toBe(newer);
		expect(newestInvitation([])).toBeNull();
	});
});

describe('expiryNotice', () => {
	it('states the number of days when known', () => {
		expect(expiryNotice(7)).toBe('Links expire after 7 days.');
		expect(expiryNotice(1)).toBe('Links expire after 1 day.');
	});

	it('omits the number when unknown', () => {
		expect(expiryNotice(null)).toBe('Links expire after a limited time.');
	});
});
