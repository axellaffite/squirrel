import { api, errorMessage } from '#lib/api/client.ts';
import type { components } from '#lib/api/schema.d.ts';
import { filterQuery, type PhotoFilter } from '#lib/explore/filters.ts';
import { collectPages, type Outcome } from '#lib/files/pages.ts';

export type { Outcome } from '#lib/files/pages.ts';
export type AssetSummary = components['schemas']['AssetSummary'];
export type AssetDetails = components['schemas']['AssetDetails'];
export type MonthCount = components['schemas']['MonthCountDto'];
export type MediaStatus = components['schemas']['MediaStatus'];
export type CameraInfo = components['schemas']['CameraDto'];
export type PlaceInfo = components['schemas']['PlaceDto'];
export type LocationInfo = components['schemas']['LocationDto'];
export type ThumbnailSize = 'thumb' | 'preview';
export type Album = components['schemas']['AlbumDto'];
export type TimelinePage = components['schemas']['TimelinePage'];

/** A month rarely holds more than a few hundred photos: fetch it whole, 500 at a time. */
const MONTH_PAGE = 500;

async function call<T>(request: Promise<{ data?: T; error?: unknown; response: Response }>): Promise<Outcome<T>> {
	try {
		const { data, error, response } = await request;
		if (response.ok) return { ok: true, data: data as T };
		return { ok: false, error: errorMessage(error, response), status: response.status };
	} catch {
		return { ok: false, error: errorMessage(null), status: 0 };
	}
}

/** `[{month, count}]` of the photos matching `filter`, newest first. */
export const listMonths = (filter: PhotoFilter = {}): Promise<Outcome<MonthCount[]>> =>
	call(api.GET('/api/photos/months', { params: { query: filterQuery(filter) } }));

/** Every photo of one month (`YYYY-MM`) matching `filter`, newest first. */
export function listMonth(month: string, filter: PhotoFilter = {}): Promise<Outcome<AssetSummary[]>> {
	return collectPages((cursor) =>
		call(
			api.GET('/api/photos', {
				params: { query: { month, cursor: cursor ?? undefined, limit: MONTH_PAGE, ...filterQuery(filter) } }
			})
		)
	);
}

export const photoDetails = (id: string): Promise<Outcome<AssetDetails>> =>
	call(api.GET('/api/photos/{id}', { params: { path: { id } } }));

export const mediaStatus = (): Promise<Outcome<MediaStatus>> => call(api.GET('/api/media/status'));

/**
 * The thumbnail URL; `version` makes it a new URL whenever it is regenerated. `px` (previews
 * only, see `previewSize.ts`): the long side wanted, in device pixels; without it, 1440.
 */
export function thumbnailUrl(id: string, size: ThumbnailSize, version: number, px?: number): string {
	const sized = px === undefined ? '' : `&px=${px}`;
	return `/api/files/${encodeURIComponent(id)}/thumbnail?size=${size}&v=${version}${sized}`;
}

export const listAlbums = (): Promise<Outcome<Album[]>> => call(api.GET('/api/albums'));

export const getAlbum = (id: string): Promise<Outcome<Album>> =>
	call(api.GET('/api/albums/{id}', { params: { path: { id } } }));

/** A new album, holding `nodeIds` right away when given. */
export const createAlbum = (name: string, nodeIds: readonly string[] = []): Promise<Outcome<Album>> =>
	call(api.POST('/api/albums', { body: { name, node_ids: [...nodeIds] } }));

export const renameAlbum = (id: string, name: string): Promise<Outcome<Album>> =>
	call(api.PATCH('/api/albums/{id}', { params: { path: { id } }, body: { name } }));

/** `null` goes back to the newest photo as the cover. */
export const setAlbumCover = (id: string, nodeId: string | null): Promise<Outcome<Album>> =>
	call(api.PATCH('/api/albums/{id}', { params: { path: { id } }, body: { cover_node_id: nodeId } }));

export const deleteAlbum = (id: string): Promise<Outcome<unknown>> =>
	call(api.DELETE('/api/albums/{id}', { params: { path: { id } } }));

/** One page of an album's photos, newest first. */
export const listAlbumPage = (id: string, cursor: string | null): Promise<Outcome<TimelinePage>> =>
	call(
		api.GET('/api/albums/{id}/items', {
			params: { path: { id }, query: { cursor: cursor ?? undefined, limit: MONTH_PAGE } }
		})
	);

export const addToAlbum = (id: string, nodeIds: readonly string[]): Promise<Outcome<{ changed: number }>> =>
	call(api.POST('/api/albums/{id}/items', { params: { path: { id } }, body: { node_ids: [...nodeIds] } }));

export const removeFromAlbum = (id: string, nodeIds: readonly string[]): Promise<Outcome<{ changed: number }>> =>
	call(api.POST('/api/albums/{id}/items/remove', { params: { path: { id } }, body: { node_ids: [...nodeIds] } }));
