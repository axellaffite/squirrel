import { describe, expect, it } from 'vitest';
import { interpretSession, logoutOutcome, type Me } from '#lib/session-logic.ts';

const me: Me = {
	id: '00000000-0000-0000-0000-000000000001',
	username: 'alice',
	display_name: 'Alice',
	role: 'admin',
	csrf_token: 'csrf',
	can_invite: true
};

const ok = (status = 200) => new Response(null, { status });

describe('interpretSession', () => {
	it('returns the user when logged in', () => {
		expect(
			interpretSession({ data: { needs_setup: false }, response: ok() }, { data: me, response: ok() })
		).toEqual({ unreachable: false, needsSetup: false, me });
	});

	it('treats a 401 from /me as logged out', () => {
		expect(
			interpretSession({ data: { needs_setup: true }, response: ok() }, { response: ok(401) })
		).toEqual({ unreachable: false, needsSetup: true, me: null });
	});

	it.each([429, 500, 502, 503, 404])('treats a %i from /me as unreachable', (status) => {
		expect(
			interpretSession({ data: { needs_setup: false }, response: ok() }, { response: ok(status) })
		).toEqual({ unreachable: true });
	});

	it('treats a failed setup status as unreachable', () => {
		expect(interpretSession({ response: ok(502) }, { data: me, response: ok() })).toEqual({
			unreachable: true
		});
		expect(interpretSession({ response: ok(502) }, { response: ok(401) })).toEqual({
			unreachable: true
		});
	});
});

describe('logoutOutcome', () => {
	it.each([200, 204, 401])('%s logs out', (status) => {
		expect(logoutOutcome(status)).toEqual({ loggedOut: true });
	});
	it.each([null, 403, 429, 500, 502])('%s keeps the session and reports an error', (status) => {
		expect(logoutOutcome(status)).toMatchObject({ loggedOut: false });
	});
});
