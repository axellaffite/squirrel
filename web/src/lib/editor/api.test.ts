import { afterEach, describe, expect, it, vi } from 'vitest';
import { setCsrfToken } from '#lib/api/client.ts';
import { collabServerUrl, openCollab } from './api.ts';

const json = (status: number, body: unknown) =>
	new Response(JSON.stringify(body), { status, headers: { 'Content-Type': 'application/json' } });

afterEach(() => {
	vi.unstubAllGlobals();
	setCsrfToken(null);
});

describe('openCollab', () => {
	it('posts with the CSRF token and returns the ticket', async () => {
		setCsrfToken('tok');
		const fetchMock = vi.fn(async () => json(200, { epoch: 'e1', can_write: true, max_bytes: 1048576 }));
		vi.stubGlobal('fetch', fetchMock);
		await expect(openCollab('a b')).resolves.toEqual({
			ok: true,
			ticket: { epoch: 'e1', can_write: true, max_bytes: 1048576 }
		});
		const [url, init] = fetchMock.mock.calls[0] as unknown as [string, RequestInit];
		expect(url).toBe('/api/files/a%20b/collab');
		expect(init.method).toBe('POST');
		expect((init.headers as Record<string, string>)['X-CSRF-Token']).toBe('tok');
	});

	it('maps refusals', async () => {
		vi.stubGlobal('fetch', vi.fn(async () => json(422, { error: { code: 'not_editable', message: 'nope' } })));
		await expect(openCollab('x')).resolves.toEqual({ ok: false, problem: 'not-editable' });
		vi.stubGlobal('fetch', vi.fn(async () => json(404, { error: { code: 'not_found' } })));
		await expect(openCollab('x')).resolves.toEqual({ ok: false, problem: 'not-found' });
	});

	it('treats a malformed answer or a network failure as a failure', async () => {
		vi.stubGlobal('fetch', vi.fn(async () => json(200, { epoch: 3 })));
		await expect(openCollab('x')).resolves.toEqual({ ok: false, problem: 'failed' });
		vi.stubGlobal('fetch', vi.fn(async () => Promise.reject(new TypeError('offline'))));
		await expect(openCollab('x')).resolves.toEqual({ ok: false, problem: 'failed' });
	});
});

describe('collabServerUrl', () => {
	it('follows the page scheme', () => {
		expect(collabServerUrl('id', { protocol: 'http:', host: 'nuc:8080' })).toBe('ws://nuc:8080/api/files/id/collab');
		expect(collabServerUrl('id', { protocol: 'https:', host: 'drive.home' })).toBe('wss://drive.home/api/files/id/collab');
	});
});
