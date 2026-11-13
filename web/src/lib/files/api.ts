import { api, errorMessage } from '#lib/api/client.ts';
import type { components } from '#lib/api/schema.d.ts';
import { collectPages, type Outcome } from '#lib/files/pages.ts';

export type { Outcome } from '#lib/files/pages.ts';
export type FileNode = components['schemas']['NodeDto'];
export type PathSegment = components['schemas']['PathSegmentDto'];
export type NodeDetails = components['schemas']['NodeDetails'];
export type TrashEntry = components['schemas']['TrashEntryDto'];
export type StorageUsage = components['schemas']['StorageUsageDto'];

/** Folders hold at most a few thousand entries at home: load them all. */
const PAGE_SIZE = 500;

async function call<T>(request: Promise<{ data?: T; error?: unknown; response: Response }>): Promise<Outcome<T>> {
	try {
		const { data, error, response } = await request;
		if (response.ok) return { ok: true, data: data as T };
		return { ok: false, error: errorMessage(error, response), status: response.status };
	} catch {
		return { ok: false, error: errorMessage(null), status: 0 };
	}
}

export function listFolder(parentId: string | null): Promise<Outcome<FileNode[]>> {
	return collectPages((cursor) =>
		call(
			api.GET('/api/nodes', {
				params: { query: { parent_id: parentId ?? undefined, cursor: cursor ?? undefined, limit: PAGE_SIZE } }
			})
		)
	);
}

export const getNode = (id: string) => call(api.GET('/api/nodes/{id}', { params: { path: { id } } }));

export const createFolder = (parentId: string, name: string) =>
	call(api.POST('/api/folders', { body: { parent_id: parentId, name } }));

export const renameNode = (id: string, name: string) =>
	call(api.PATCH('/api/nodes/{id}', { params: { path: { id } }, body: { name } }));

export const moveNode = (id: string, parentId: string) =>
	call(api.PATCH('/api/nodes/{id}', { params: { path: { id } }, body: { parent_id: parentId } }));

export const trashNode = (id: string) => call(api.DELETE('/api/nodes/{id}', { params: { path: { id } } }));

export function listTrash(): Promise<Outcome<TrashEntry[]>> {
	return collectPages((cursor) =>
		call(api.GET('/api/trash', { params: { query: { cursor: cursor ?? undefined, limit: PAGE_SIZE } } }))
	);
}

export const restoreEntry = (id: string) =>
	call(api.POST('/api/trash/{id}/restore', { params: { path: { id } } }));

export const deleteForever = (id: string) => call(api.DELETE('/api/trash/{id}', { params: { path: { id } } }));

export const emptyTrash = () => call(api.DELETE('/api/trash'));

export const storageUsage = () => call(api.GET('/api/storage'));
