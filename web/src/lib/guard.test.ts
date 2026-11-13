import { describe, expect, it } from 'vitest';
import { isPublicPath, redirectFor, type GuardState } from '#lib/guard.ts';

const me = { username: 'alice' };
const base: GuardState = { loaded: true, unreachable: false, needsSetup: false, me: null, path: '/' };

function at(path: string, overrides: Partial<GuardState> = {}): string | null {
	return redirectFor({ ...base, path, ...overrides });
}

describe('isPublicPath', () => {
	it.each([
		['/login', true],
		['/setup', true],
		['/join/abc', true],
		['/join/', true],
		['/', false],
		['/settings/invitations', false],
		['/login/extra', false],
		['/loginx', false],
		['/setupx', false],
		['/join', false],
		['/joined', false]
	])('%s → %s', (path, expected) => {
		expect(isPublicPath(path)).toBe(expected);
	});
});

describe('redirectFor', () => {
	it('never redirects before the session is loaded', () => {
		for (const path of ['/', '/login', '/setup', '/settings/invitations']) {
			expect(at(path, { loaded: false })).toBeNull();
			expect(at(path, { loaded: false, needsSetup: true })).toBeNull();
		}
	});

	it('never redirects while the server is unreachable', () => {
		for (const path of ['/', '/login', '/setup', '/settings/invitations']) {
			expect(at(path, { unreachable: true })).toBeNull();
			expect(at(path, { unreachable: true, needsSetup: true })).toBeNull();
		}
	});

	describe('when the instance needs setup', () => {
		it.each(['/', '/login', '/join/abc', '/settings/invitations', '/unknown'])(
			'sends %s to /setup',
			(path) => {
				expect(at(path, { needsSetup: true })).toBe('/setup');
				expect(at(path, { needsSetup: true, me })).toBe('/setup');
			}
		);
		it('lets /setup render', () => {
			expect(at('/setup', { needsSetup: true })).toBeNull();
		});
	});

	describe('anonymous visitor on a set-up instance', () => {
		it.each([
			['/', '/login'],
			['/settings/invitations', '/login'],
			['/unknown', '/login'],
			['/login/extra', '/login'],
			['/setup', '/login'],
			['/login', null],
			['/join/abc', null]
		])('%s → %s', (path, expected) => {
			expect(at(path)).toBe(expected);
		});
	});

	describe('logged-in user on a set-up instance', () => {
		it.each([
			['/', null],
			['/settings/invitations', null],
			['/unknown', null],
			['/join/abc', null],
			['/login', '/'],
			['/setup', '/']
		])('%s → %s', (path, expected) => {
			expect(at(path, { me })).toBe(expected);
		});
	});
});
