import { afterEach, describe, expect, it, vi } from 'vitest';
import {
	createApiClient,
	isUnauthorizedExempt,
	onStaleCsrf,
	onUnauthorized,
	setCsrfToken
} from '#lib/api/client.ts';

function envelope(status: number, code: string): Response {
	return new Response(JSON.stringify({ error: { code, message: code } }), {
		status,
		headers: { 'Content-Type': 'application/json' }
	});
}

function clientAnswering(response: () => Response) {
	const fetchMock = vi.fn(async (_input: Request) => response());
	const client = createApiClient({ baseUrl: 'http://squirrel.test', fetch: fetchMock as typeof fetch });
	return { client, fetchMock };
}

afterEach(() => {
	onUnauthorized(null);
	onStaleCsrf(null);
	setCsrfToken(null);
});

describe('isUnauthorizedExempt', () => {
	it.each([
		['/api/auth/login', true],
		['/api/auth/me', true],
		['/api/setup/status', true],
		['/api/setup/admin', true],
		['/api/join/{token}', true],
		['/api/auth/logout', false],
		['/api/invitations', false],
		['/api/invitations/{id}', false],
		['/api/admin/settings', false]
	])('%s → %s', (path, expected) => {
		expect(isUnauthorizedExempt(path)).toBe(expected);
	});
});

describe('session middleware', () => {
	it('reports a 401 from an authenticated endpoint', async () => {
		const handler = vi.fn();
		onUnauthorized(handler);
		const { client } = clientAnswering(() => envelope(401, 'unauthorized'));
		const { response } = await client.GET('/api/invitations');
		expect(response.status).toBe(401);
		expect(handler).toHaveBeenCalledOnce();
	});

	it('ignores a 401 from login, me, setup and join endpoints', async () => {
		const handler = vi.fn();
		onUnauthorized(handler);
		const { client } = clientAnswering(() => envelope(401, 'invalid_credentials'));
		await client.POST('/api/auth/login', { body: { username: 'a', password: 'b' } });
		await client.GET('/api/auth/me');
		await client.GET('/api/setup/status');
		await client.GET('/api/join/{token}', { params: { path: { token: 't' } } });
		expect(handler).not.toHaveBeenCalled();
	});

	it('does not report successful responses', async () => {
		const unauthorized = vi.fn();
		const stale = vi.fn();
		onUnauthorized(unauthorized);
		onStaleCsrf(stale);
		const { client } = clientAnswering(() => Response.json([]));
		await client.GET('/api/invitations');
		expect(unauthorized).not.toHaveBeenCalled();
		expect(stale).not.toHaveBeenCalled();
	});

	it('reloads the session after a 403 forbidden on a state-changing request', async () => {
		const stale = vi.fn();
		onStaleCsrf(stale);
		const { client } = clientAnswering(() => envelope(403, 'forbidden'));
		const { error } = await client.POST('/api/invitations', { body: {} });
		expect(error).toEqual({ error: { code: 'forbidden', message: 'forbidden' } });
		await vi.waitFor(() => expect(stale).toHaveBeenCalledOnce());
	});

	it('coalesces concurrent 403s into a single reload', async () => {
		let release: () => void = () => undefined;
		const stale = vi.fn(() => new Promise<void>((resolve) => (release = resolve)));
		onStaleCsrf(stale);
		const { client } = clientAnswering(() => envelope(403, 'forbidden'));
		await Promise.all([
			client.POST('/api/invitations', { body: {} }),
			client.POST('/api/auth/logout')
		]);
		await vi.waitFor(() => expect(stale).toHaveBeenCalledOnce());
		release();
		await client.POST('/api/invitations', { body: {} });
		await vi.waitFor(() => expect(stale).toHaveBeenCalledTimes(2));
	});

	it('does not reload after a 403 on a GET or with another error code', async () => {
		const stale = vi.fn();
		onStaleCsrf(stale);
		const forbiddenGet = clientAnswering(() => envelope(403, 'forbidden'));
		await forbiddenGet.client.GET('/api/admin/settings');
		const otherCode = clientAnswering(() => envelope(403, 'something_else'));
		await otherCode.client.POST('/api/invitations', { body: {} });
		const notJson = clientAnswering(() => new Response('nope', { status: 403 }));
		await notJson.client.POST('/api/invitations', { body: {} });
		await new Promise((resolve) => setTimeout(resolve, 0));
		expect(stale).not.toHaveBeenCalled();
	});

	it('sends the CSRF token on state-changing requests only', async () => {
		setCsrfToken('token-123');
		const { client, fetchMock } = clientAnswering(() => new Response(null, { status: 204 }));
		await client.POST('/api/auth/logout');
		await client.GET('/api/auth/me');
		const [post, get] = fetchMock.mock.calls.map(([request]) => request);
		expect(post.headers.get('X-CSRF-Token')).toBe('token-123');
		expect(get.headers.get('X-CSRF-Token')).toBeNull();
	});
});
