import { afterEach, describe, expect, it, vi } from 'vitest';
import { createApiClient, setCsrfToken } from '#lib/api/client.ts';
import { createTransport } from '#lib/upload/transport.ts';
import { UploadError } from '#lib/upload/uploader.ts';
import { NetworkError } from '#lib/upload/errors.ts';

function transportAnswering(response: () => Response) {
	const fetchMock = vi.fn(async (_input: Request) => response());
	const client = createApiClient({ baseUrl: 'http://squirrel.test', fetch: fetchMock as typeof fetch });
	return { transport: createTransport(client), fetchMock };
}

const signal = () => new AbortController().signal;
const envelope = (status: number, code: string) =>
	Response.json({ error: { code, message: code } }, { status });

afterEach(() => setCsrfToken(null));

describe('upload transport', () => {
	it('sends chunks as raw bytes with the CSRF header', async () => {
		setCsrfToken('token-1');
		const { transport, fetchMock } = transportAnswering(() => new Response(null, { status: 204 }));
		await transport.putChunk('u1', 2, new Blob([new Uint8Array([1, 2, 3])]), signal());
		const [request] = fetchMock.mock.calls[0];
		expect(request.method).toBe('PUT');
		expect(new URL(request.url).pathname).toBe('/api/uploads/u1/chunks/2');
		expect(request.headers.get('X-CSRF-Token')).toBe('token-1');
		expect(request.headers.get('Content-Type')).toBe('application/octet-stream');
		expect(new Uint8Array(await request.arrayBuffer())).toEqual(new Uint8Array([1, 2, 3]));
	});

	it('tells sessions from deduplicated uploads', async () => {
		const sessionBody = { upload_id: 'u1', chunk_size: 4, chunk_count: 1, received_chunks: [], expires_at: '' };
		const session = transportAnswering(() => Response.json(sessionBody, { status: 201 }));
		const request = { parent_id: 'p', name: 'a', size: 1, sha256: 'f'.repeat(64) };
		expect(await session.transport.start(request, signal())).toEqual({ kind: 'session', session: sessionBody });
		const dedup = transportAnswering(() => Response.json({ deduplicated: true, node: { id: 'n' } }));
		expect(await dedup.transport.start(request, signal())).toEqual({ kind: 'deduplicated', node: { id: 'n' } });
	});

	it.each([
		[503, 'busy', true],
		[507, 'storage_full', false],
		[507, 'quota_exceeded', false],
		[422, 'hash_mismatch', false]
	])('maps %d %s to a %s-retryable error', async (status, code, retryable) => {
		const { transport } = transportAnswering(() => envelope(status, code));
		const error = await transport.complete('u1', signal()).catch((e: unknown) => e);
		expect(error).toBeInstanceOf(UploadError);
		expect(error).toMatchObject({ status, retryable });
	});
});

describe('upload transport failures', () => {
	it('reports an abort the server refused', async () => {
		const { transport } = transportAnswering(() => envelope(409, 'upload_busy'));
		const error = await transport.abort('u1').catch((e: unknown) => e);
		expect(error).toBeInstanceOf(UploadError);
		expect(error).toMatchObject({ status: 409 });
	});


	it('wraps fetch transport failures in a NetworkError', async () => {
		const fetchMock = vi.fn(async () => {
			throw new TypeError('Failed to fetch');
		});
		const client = createApiClient({ baseUrl: 'http://squirrel.test', fetch: fetchMock as typeof fetch });
		const error = await createTransport(client)
			.putChunk('u1', 0, new Blob([]), signal())
			.catch((e: unknown) => e);
		expect(error).toBeInstanceOf(NetworkError);
		expect((error as NetworkError).cause).toBeInstanceOf(TypeError);
	});

	it('leaves cancellations as they are', async () => {
		const fetchMock = vi.fn(async () => {
			throw new DOMException('aborted', 'AbortError');
		});
		const client = createApiClient({ baseUrl: 'http://squirrel.test', fetch: fetchMock as typeof fetch });
		const error = await createTransport(client)
			.complete('u1', signal())
			.catch((e: unknown) => e);
		expect(error).not.toBeInstanceOf(NetworkError);
		expect(error).toMatchObject({ name: 'AbortError' });
	});

	it('reads Retry-After on 429 and 503', async () => {
		const { transport } = transportAnswering(
			() =>
				new Response(JSON.stringify({ error: { code: 'busy', message: 'busy' } }), {
					status: 503,
					headers: { 'Content-Type': 'application/json', 'Retry-After': '2' }
				})
		);
		const error = await transport.complete('u1', signal()).catch((e: unknown) => e);
		expect(error).toMatchObject({ status: 503, retryable: true, retryAfterMs: 2000 });
	});
});
