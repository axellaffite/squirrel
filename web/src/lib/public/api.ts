/**
 * The visitor's side of a public link (`/s/{token}`). A client of its own, without the
 * session middleware: a visitor has no session, and a locked link's 401 must not end the
 * owner's session when they open their own link.
 */
import createClient from 'openapi-fetch';
import { errorMessage } from '#lib/api/client.ts';
import type { components, paths } from '#lib/api/schema.d.ts';
import type { Outcome } from '#lib/files/pages.ts';

export type LinkInfo = components['schemas']['PublicLinkInfo'];
export type FolderListing = components['schemas']['PublicFolderListing'];
export type PublicNode = components['schemas']['NodeDto'];
export type AlbumPhoto = components['schemas']['AssetSummary'];
export type PhotoPage = components['schemas']['TimelinePage'];

const client = createClient<paths>({ baseUrl: '', credentials: 'same-origin' });

/** Every answer of a link: the data, gone (404), locked (401 `link_locked`), or another error. */
export type LinkOutcome<T> = Outcome<T> & { gone?: boolean; locked?: boolean };

function errorCode(error: unknown): string | null {
	if (typeof error !== 'object' || error === null || !('error' in error)) return null;
	const code = (error as { error?: { code?: unknown } }).error?.code;
	return typeof code === 'string' ? code : null;
}

async function call<T>(request: Promise<{ data?: T; error?: unknown; response: Response }>): Promise<LinkOutcome<T>> {
	try {
		const { data, error, response } = await request;
		if (response.ok) return { ok: true, data: data as T };
		const code = errorCode(error);
		return {
			ok: false,
			error: errorMessage(error, response),
			status: response.status,
			gone: response.status === 404 && code === 'not_found',
			locked: code === 'link_locked'
		};
	} catch {
		return { ok: false, error: errorMessage(null), status: 0 };
	}
}

const path = (token: string) => ({ path: { token } });

export const linkInfo = (token: string): Promise<LinkOutcome<LinkInfo>> =>
	call(client.GET('/api/public/{token}', { params: path(token) }));

/** `null` on success; the message to show otherwise. */
export async function unlock(token: string, password: string): Promise<string | null> {
	const result = await call(client.POST('/api/public/{token}/unlock', { params: path(token), body: { password } }));
	if (result.ok) return null;
	return result.status === 401 ? 'Wrong password.' : result.error;
}

export const listFolder = (token: string, parentId: string | null, cursor: string | null): Promise<LinkOutcome<FolderListing>> =>
	call(
		client.GET('/api/public/{token}/nodes', {
			params: { ...path(token), query: { parent_id: parentId ?? undefined, cursor: cursor ?? undefined } }
		})
	);

export const albumPhotos = (token: string, cursor: string | null): Promise<LinkOutcome<PhotoPage>> =>
	call(client.GET('/api/public/{token}/album/items', { params: { ...path(token), query: { cursor: cursor ?? undefined } } }));

/** Starts a zip of `ids` and returns where to download it. */
export async function zipUrl(token: string, ids: readonly string[]): Promise<LinkOutcome<string>> {
	const result = await call(client.POST('/api/public/{token}/archive', { params: path(token), body: { node_ids: [...ids] } }));
	if (!result.ok) return result;
	return { ok: true, data: `/api/public/${encodeURIComponent(token)}/archive/${encodeURIComponent(result.data.ticket)}` };
}

const base = (token: string, id: string) => `/api/public/${encodeURIComponent(token)}/files/${encodeURIComponent(id)}`;

/** A grid thumbnail, cached for good when `version` is the current one. */
export const thumbUrl = (token: string, id: string, version: number): string =>
	`${base(token, id)}/thumbnail?size=thumb&v=${version}`;

/** A large preview of a photo, about `px` device pixels on its long side. */
export const previewUrl = (token: string, id: string, px: number): string =>
	`${base(token, id)}/thumbnail?size=preview&px=${Math.max(1, Math.round(px))}`;

/** The file itself; `download` forces a save rather than showing it. */
export const contentUrl = (token: string, id: string, download = false): string =>
	`${base(token, id)}/content${download ? '?download=true' : ''}`;
