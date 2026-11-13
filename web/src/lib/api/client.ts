import createClient, { type Middleware } from 'openapi-fetch';
import type { paths } from '#lib/api/schema.d.ts';

const SAFE_METHODS = new Set(['GET', 'HEAD', 'OPTIONS']);
const DEFAULT_ERROR = 'Something went wrong. Please try again.';

/** Endpoints whose 401 is an expected answer, not a sign the session ended. */
const UNAUTHORIZED_EXEMPT_PATHS = new Set(['/api/auth/login', '/api/auth/me']);
const UNAUTHORIZED_EXEMPT_PREFIXES = ['/api/setup/', '/api/join/'];

type Handler = () => void | Promise<void>;

let csrfToken: string | null = null;
let unauthorizedHandler: Handler | null = null;
let staleCsrfHandler: Handler | null = null;
let staleCsrfRecovery: Promise<void> | null = null;

export function setCsrfToken(token: string | null): void {
	csrfToken = token;
}

/** Called when an authenticated request answers 401: the session ended server-side. */
export function onUnauthorized(handler: Handler | null): void {
	unauthorizedHandler = handler;
}

/**
 * Called when a state-changing request answers 403 `forbidden`: the CSRF token may be
 * stale (e.g. session replaced in another tab), so the session should be reloaded.
 */
export function onStaleCsrf(handler: Handler | null): void {
	staleCsrfHandler = handler;
}

export function isUnauthorizedExempt(schemaPath: string): boolean {
	return (
		UNAUTHORIZED_EXEMPT_PATHS.has(schemaPath) ||
		UNAUTHORIZED_EXEMPT_PREFIXES.some((prefix) => schemaPath.startsWith(prefix))
	);
}

async function errorCode(response: Response): Promise<string | null> {
	try {
		const body: unknown = await response.clone().json();
		if (typeof body !== 'object' || body === null || !('error' in body)) return null;
		const code = (body as { error?: { code?: unknown } }).error?.code;
		return typeof code === 'string' ? code : null;
	} catch {
		return null;
	}
}

/** Runs the stale-CSRF handler, coalescing concurrent 403s into a single reload. */
function recoverFromStaleCsrf(): void {
	if (!staleCsrfHandler || staleCsrfRecovery) return;
	const handler = staleCsrfHandler;
	staleCsrfRecovery = Promise.resolve()
		.then(handler)
		.catch(() => undefined)
		.finally(() => {
			staleCsrfRecovery = null;
		});
}

export const csrfMiddleware: Middleware = {
	onRequest({ request }) {
		if (csrfToken && !SAFE_METHODS.has(request.method)) {
			request.headers.set('X-CSRF-Token', csrfToken);
		}
		return request;
	}
};

export const sessionMiddleware: Middleware = {
	async onResponse({ request, response, schemaPath }) {
		if (response.status === 401 && !isUnauthorizedExempt(schemaPath)) {
			await unauthorizedHandler?.();
		} else if (
			response.status === 403 &&
			!SAFE_METHODS.has(request.method) &&
			(await errorCode(response)) === 'forbidden'
		) {
			recoverFromStaleCsrf();
		}
		return response;
	}
};

export function createApiClient(options: { baseUrl?: string; fetch?: typeof fetch } = {}) {
	const client = createClient<paths>({
		baseUrl: options.baseUrl ?? '',
		credentials: 'same-origin',
		...(options.fetch ? { fetch: options.fetch } : {})
	});
	client.use(csrfMiddleware, sessionMiddleware);
	return client;
}

export const api = createApiClient();

/** Extracts the server's `{ error: { message } }` envelope, with sensible fallbacks. */
export function errorMessage(error: unknown, response?: Response): string {
	if (response?.status === 429) return 'Too many attempts. Wait a moment and try again.';
	if (typeof error === 'object' && error !== null && 'error' in error) {
		const detail = (error as { error?: { message?: unknown } }).error;
		if (typeof detail?.message === 'string') return detail.message;
	}
	return DEFAULT_ERROR;
}
