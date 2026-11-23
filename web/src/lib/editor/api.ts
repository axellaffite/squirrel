import { currentCsrfToken } from '#lib/api/client.ts';
import { openProblem, type OpenProblem } from '#lib/editor/logic.ts';

/** The answer to `POST /api/files/{id}/collab`. */
export interface CollabTicket {
	/** Identifies the document state matching the file's current content. */
	epoch: string;
	can_write: boolean;
	max_bytes: number;
}

export type OpenOutcome = { ok: true; ticket: CollabTicket } | { ok: false; problem: OpenProblem };

function isTicket(value: unknown): value is CollabTicket {
	if (typeof value !== 'object' || value === null) return false;
	const body = value as Record<string, unknown>;
	return typeof body.epoch === 'string' && body.epoch !== '' && typeof body.can_write === 'boolean' && typeof body.max_bytes === 'number';
}

async function errorCode(response: Response): Promise<string | null> {
	try {
		const body = (await response.json()) as { error?: { code?: unknown } };
		return typeof body?.error?.code === 'string' ? body.error.code : null;
	} catch {
		return null;
	}
}

/** Opens a file for live editing (not in the OpenAPI schema yet, hence a plain fetch). */
export async function openCollab(id: string): Promise<OpenOutcome> {
	const headers: Record<string, string> = { Accept: 'application/json' };
	const token = currentCsrfToken();
	if (token) headers['X-CSRF-Token'] = token;
	try {
		const response = await fetch(`/api/files/${encodeURIComponent(id)}/collab`, {
			method: 'POST',
			credentials: 'same-origin',
			headers
		});
		if (!response.ok) return { ok: false, problem: openProblem(response.status, await errorCode(response)) };
		const body: unknown = await response.json();
		return isTicket(body) ? { ok: true, ticket: body } : { ok: false, problem: 'failed' };
	} catch {
		return { ok: false, problem: 'failed' };
	}
}

/** `ws://host/api/files/{id}/collab` (y-websocket appends `/<room>?epoch=…`). */
export function collabServerUrl(id: string, location: Pick<Location, 'protocol' | 'host'>): string {
	const scheme = location.protocol === 'https:' ? 'wss' : 'ws';
	return `${scheme}://${location.host}/api/files/${encodeURIComponent(id)}/collab`;
}
