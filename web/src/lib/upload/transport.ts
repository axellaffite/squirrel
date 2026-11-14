import type { Middleware } from 'openapi-fetch';
import { api, errorMessage } from '#lib/api/client.ts';
import { isAbortError, isRetryableStatus, NetworkError, parseRetryAfter, UploadError } from '#lib/upload/errors.ts';
import type { UploadTransport } from '#lib/upload/uploader.ts';

function uploadError(error: unknown, response: Response): UploadError {
	const status = response.status;
	const retryAfterMs = status === 429 || status === 503 ? parseRetryAfter(response.headers.get('Retry-After')) : null;
	return new UploadError(errorMessage(error, response), status, isRetryableStatus(status), retryAfterMs);
}

/**
 * `onError` only runs when fetch itself rejects, so wrapping there tells real transport
 * failures (retryable) apart from bugs that happen to throw a TypeError (not retryable).
 */
const networkErrors: Middleware = {
	onError({ error }) {
		return isAbortError(error) ? undefined : new NetworkError(error);
	}
};
const middleware = [networkErrors];

/**
 * Every call goes through the API client, so the CSRF header and the session
 * handlers (401, stale CSRF) apply to chunk uploads too. Chunks are sent as raw bytes.
 */
export function createTransport(client = api): UploadTransport {
	return {
		async start(body, signal) {
			const { data, error, response } = await client.POST('/api/uploads', { body, signal, middleware });
			if (!response.ok || !data) throw uploadError(error, response);
			return 'upload_id' in data ? { kind: 'session', session: data } : { kind: 'deduplicated', node: data.node };
		},
		async putChunk(uploadId, index, body, signal) {
			const { error, response } = await client.PUT('/api/uploads/{id}/chunks/{index}', {
				params: { path: { id: uploadId, index } },
				// The schema describes the body as bytes; send the Blob untouched.
				body: body as unknown as number[],
				bodySerializer: (blob) => blob as unknown as Blob,
				headers: { 'Content-Type': 'application/octet-stream' },
				signal,
				middleware
			});
			if (!response.ok) throw uploadError(error, response);
		},
		async complete(uploadId, signal) {
			const { data, error, response } = await client.POST('/api/uploads/{id}/complete', {
				params: { path: { id: uploadId } },
				signal,
				middleware
			});
			if (!response.ok || !data) throw uploadError(error, response);
			return data;
		},
		async abort(uploadId) {
			await client.DELETE('/api/uploads/{id}', { params: { path: { id: uploadId } } });
		}
	};
}
