import createClient, { type Middleware } from 'openapi-fetch';
import type { paths } from '#lib/api/schema.d.ts';

const SAFE_METHODS = new Set(['GET', 'HEAD', 'OPTIONS']);
const DEFAULT_ERROR = 'Something went wrong. Please try again.';
/** Error code the server returns when the `X-CSRF-Token` header does not match the session. */
const CSRF_INVALID = 'csrf_invalid';

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
 * Called when a state-changing request answers 403 `csrf_invalid`: the CSRF token is
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
			(await errorCode(response)) === CSRF_INVALID
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

/**
 * Messages for error codes the user can act on, worded for the file pages. They take
 * precedence over the server's own message, which is written for API clients.
 */
const FRIENDLY_MESSAGES: Record<string, string> = {
	busy: 'The server is busy. Try again in a moment.',
	storage_full: 'The server is out of disk space. Ask your administrator to free some space.',
	too_many_uploads: 'Too many uploads are in progress. Wait for some to finish, then try again.',
	quota_exceeded: 'This would exceed your storage quota. Free up some space, including the trash, then try again.',
	hash_mismatch: 'The file changed or was damaged while uploading. Try again.',
	upload_incomplete: 'Part of the file did not reach the server. Retry to send the rest.'
};

/** Extracts the server's `{ error: { message } }` envelope, with sensible fallbacks. */
export function errorMessage(error: unknown, response?: Response): string {
	const detail = errorDetail(error);
	if (detail.code !== null && Object.hasOwn(FRIENDLY_MESSAGES, detail.code)) return FRIENDLY_MESSAGES[detail.code];
	if (response?.status === 429) return 'Too many attempts. Wait a moment and try again.';
	if (detail.message !== null) return detail.message;
	if (response?.status === 503) return FRIENDLY_MESSAGES.busy;
	return DEFAULT_ERROR;
}

function errorDetail(error: unknown): { code: string | null; message: string | null } {
	if (typeof error !== 'object' || error === null || !('error' in error)) return { code: null, message: null };
	const detail = (error as { error?: { code?: unknown; message?: unknown } }).error;
	return {
		code: typeof detail?.code === 'string' ? detail.code : null,
		message: typeof detail?.message === 'string' ? detail.message : null
	};
}
