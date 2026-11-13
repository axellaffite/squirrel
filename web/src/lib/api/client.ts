import createClient, { type Middleware } from 'openapi-fetch';
import type { paths } from './schema';

const SAFE_METHODS = new Set(['GET', 'HEAD', 'OPTIONS']);
const DEFAULT_ERROR = 'Something went wrong. Please try again.';

let csrfToken: string | null = null;

export function setCsrfToken(token: string | null): void {
	csrfToken = token;
}

const csrfMiddleware: Middleware = {
	onRequest({ request }) {
		if (csrfToken && !SAFE_METHODS.has(request.method)) {
			request.headers.set('X-CSRF-Token', csrfToken);
		}
		return request;
	}
};

export const api = createClient<paths>({ baseUrl: '', credentials: 'same-origin' });
api.use(csrfMiddleware);

/** Extracts the server's `{ error: { message } }` envelope, with sensible fallbacks. */
export function errorMessage(error: unknown, response?: Response): string {
	if (response?.status === 429) return 'Too many attempts. Wait a moment and try again.';
	if (typeof error === 'object' && error !== null && 'error' in error) {
		const detail = (error as { error?: { message?: unknown } }).error;
		if (typeof detail?.message === 'string') return detail.message;
	}
	return DEFAULT_ERROR;
}
