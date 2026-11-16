import { api, errorMessage } from '#lib/api/client.ts';
import type { components } from '#lib/api/schema.d.ts';
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

/** `[{month, count}]`, newest first. */
export const listMonths = (): Promise<Outcome<MonthCount[]>> => call(api.GET('/api/photos/months'));

/** Every photo of one month (`YYYY-MM`), newest first. */
export function listMonth(month: string): Promise<Outcome<AssetSummary[]>> {
	return collectPages((cursor) =>
		call(api.GET('/api/photos', { params: { query: { month, cursor: cursor ?? undefined, limit: MONTH_PAGE } } }))
	);
}

export const photoDetails = (id: string): Promise<Outcome<AssetDetails>> =>
	call(api.GET('/api/photos/{id}', { params: { path: { id } } }));

export const mediaStatus = (): Promise<Outcome<MediaStatus>> => call(api.GET('/api/media/status'));

/** The thumbnail URL; `version` makes it a new, immutably cached URL whenever it is regenerated. */
export function thumbnailUrl(id: string, size: ThumbnailSize, version: number): string {
	return `/api/files/${encodeURIComponent(id)}/thumbnail?size=${size}&v=${version}`;
}
