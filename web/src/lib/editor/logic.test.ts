import { describe, expect, it } from 'vitest';
import {
	closeOutcome,
	endMessage,
	initials,
	openMessage,
	openProblem,
	PRESENCE_COLORS,
	presence,
	presenceColor,
	presenceUser,
	sanitizeUser,
	shouldReconnect,
	statusLabel
} from './logic.ts';

describe('closeOutcome', () => {
	it.each([
		[4401, { kind: 'end', reason: 'signed-out' }],
		[4404, { kind: 'end', reason: 'no-access' }],
		[4409, { kind: 'reopen' }],
		[4413, { kind: 'end', reason: 'too-large' }],
		[4429, { kind: 'end', reason: 'busy' }],
		[4507, { kind: 'end', reason: 'storage-full' }],
		[4400, { kind: 'end', reason: 'refused' }],
		[4499, { kind: 'end', reason: 'refused' }]
	])('maps %i', (code, outcome) => expect(closeOutcome(code)).toEqual(outcome));

	it.each([1000, 1001, 1006, 1011, 4000, 4500, 0])('keeps reconnecting after %i', (code) => {
		expect(closeOutcome(code)).toEqual({ kind: 'reconnect' });
		expect(shouldReconnect(code)).toBe(true);
	});

	it('never reconnects by itself after a deliberate close, a stale epoch included', () => {
		for (const code of [4401, 4404, 4409, 4413, 4429, 4507]) expect(shouldReconnect(code)).toBe(false);
	});

	it('has a message for every end', () => {
		for (const reason of ['signed-out', 'no-access', 'too-large', 'busy', 'storage-full', 'refused'] as const) {
			expect(endMessage(reason)).toMatch(/\.$/);
		}
	});
});

describe('openProblem', () => {
	it('maps the answers of the open call', () => {
		expect(openProblem(401, null)).toBe('signed-out');
		expect(openProblem(404, 'not_found')).toBe('not-found');
		expect(openProblem(422, 'not_editable')).toBe('not-editable');
		expect(openProblem(422, 'validation')).toBe('failed');
		expect(openProblem(429, null)).toBe('busy');
		expect(openProblem(500, null)).toBe('failed');
		expect(openProblem(0, null)).toBe('failed');
	});

	it('explains the 1 MiB limit when a file is not editable', () => {
		expect(openMessage('not-editable')).toContain('1 MiB');
	});
});

describe('statusLabel', () => {
	it('follows the connection', () => {
		expect(statusLabel({ phase: 'connecting', changed: false, pending: false })).toBe('Connecting…');
		expect(statusLabel({ phase: 'reconnecting', changed: true, pending: true })).toBe('Reconnecting…');
		expect(statusLabel({ phase: 'ended', changed: true, pending: false })).toBe('Disconnected');
	});

	it('says saved once changes settled', () => {
		expect(statusLabel({ phase: 'connected', changed: false, pending: false })).toBe('Connected');
		expect(statusLabel({ phase: 'connected', changed: true, pending: true })).toBe('Connected');
		expect(statusLabel({ phase: 'connected', changed: true, pending: false })).toBe('Saved automatically');
	});
});

describe('presence colours', () => {
	it('gives an account the same token colour every time', () => {
		const color = presenceColor('3f1c-user');
		expect(PRESENCE_COLORS).toContain(color);
		expect(presenceColor('3f1c-user')).toBe(color);
		expect(presenceUser('3f1c-user', 'Ada')).toEqual({
			id: '3f1c-user',
			name: 'Ada',
			color,
			colorLight: `color-mix(in srgb, ${color} 22%, transparent)`
		});
	});

	it('spreads accounts over the palette', () => {
		const used = new Set(Array.from({ length: 40 }, (_, i) => presenceColor(`user-${i}`)));
		expect(used.size).toBe(PRESENCE_COLORS.length);
	});
});

describe('sanitizeUser', () => {
	it('keeps a well-formed peer', () => {
		const user = presenceUser('u1', 'Bob');
		expect(sanitizeUser(user)).toEqual(user);
	});

	it('replaces a colour that is not a token (it ends up in a style attribute)', () => {
		const user = sanitizeUser({ id: 'u1', name: 'Eve', color: 'red; background: url(https://evil)' });
		expect(PRESENCE_COLORS).toContain(user.color);
		expect(user.colorLight).not.toContain('evil');
	});

	it('bounds and defaults the name', () => {
		expect(sanitizeUser({ name: 'x'.repeat(500) }).name).toHaveLength(64);
		expect(sanitizeUser({ name: '   ' }).name).toBe('Someone');
		expect(sanitizeUser(null).name).toBe('Someone');
		expect(sanitizeUser('nonsense').name).toBe('Someone');
	});
});

describe('initials', () => {
	it.each([
		['Ada Lovelace', 'AL'],
		['bob', 'BO'],
		['Jean Paul Sartre', 'JS'],
		['  ', '?'],
		['é', 'É']
	])('%s → %s', (name, expected) => expect(initials(name)).toBe(expected));
});

describe('presence', () => {
	const state = (id: string, name: string) => ({ user: presenceUser(id, name) });

	it('lists people once each, oneself first, then by name', () => {
		const states = new Map<number, Record<string, unknown>>([
			[3, state('carol', 'Carol')],
			[1, state('me', 'Me')],
			[2, state('bob', 'Bob')],
			[4, state('bob', 'Bob')], // a second tab
			[5, {}] // no user published yet
		]);
		const people = presence(states, 1);
		expect(people.map((p) => p.name)).toEqual(['Me', 'Bob', 'Carol']);
		expect(people[0].self).toBe(true);
		expect(people[1]).toMatchObject({ key: 'bob', initials: 'BO', self: false });
	});

	it('marks oneself even when another tab of the same account came first', () => {
		const states = new Map<number, Record<string, unknown>>([
			[7, state('me', 'Me')],
			[8, state('me', 'Me')]
		]);
		expect(presence(states, 8)).toEqual([expect.objectContaining({ key: 'me', self: true })]);
	});
});
