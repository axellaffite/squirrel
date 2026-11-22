/** Photo search and Explore: requests and their wording. */
import { api, errorMessage } from '#lib/api/client.ts';
import type { components } from '#lib/api/schema.d.ts';
import type { FaceCrop } from '#lib/explore/faces.ts';
import { filterQuery, type PhotoFilter } from '#lib/explore/filters.ts';
import { retryDelay } from '#lib/files/search.ts';
import type { AssetSummary, Outcome } from '#lib/photos/api.ts';

type Client = typeof api;

export type TagGroup = components['schemas']['TagGroupDto'];
export type PlaceGroup = components['schemas']['PlaceGroupDto'];
export type CameraGroup = components['schemas']['CameraGroupDto'];
export type Person = components['schemas']['PersonDto'];
export type Face = components['schemas']['FaceDto'];

/** The server's limits on a photo search. */
export const MAX_PHOTO_QUERY = 200;
export const PHOTO_PAGE = 50;
/** Date-ordered pages are larger: two requests lay out the whole timeline. */
export const PHOTO_DATE_PAGE = 100;
export const MAX_PHOTO_RESULTS = 200;

/** `date`: newest capture first (the timeline); `relevance`: best match first. */
export type SearchOrder = components['schemas']['SearchOrder'];

/** Which page of a search: relevance pages by offset, date by the previous page's cursor. */
export type SearchPage = { order: 'relevance'; offset: number } | { order: 'date'; cursor: string | null };

/** Calm wording for a search that could not run. */
export const SEARCH_UNAVAILABLE = "Photo search isn't set up on this server.";
export const SEARCH_BUSY = 'Photo search is starting up or busy. Try again in a moment.';
export const SEARCH_TOO_FAST = 'Searching a little too fast. Trying again in a moment…';

export type PhotoSearchOutcome =
	| { ok: true; data: { items: AssetSummary[]; has_more: boolean; next_cursor?: string | null } }
	| { ok: false; error: string; status: number; unavailable?: boolean; retryAfterMs?: number }
	| { ok: false; aborted: true };

/** The error envelope's code, if any. */
function code(error: unknown): string | null {
	const value = (error as { error?: { code?: unknown } } | null | undefined)?.error?.code;
	return typeof value === 'string' ? value : null;
}

/** The query parameters of a page. */
function pageQuery(page: SearchPage) {
	if (page.order === 'date') {
		return { order: 'date' as const, limit: PHOTO_DATE_PAGE, ...(page.cursor ? { cursor: page.cursor } : {}) };
	}
	return { offset: page.offset, limit: Math.max(1, Math.min(PHOTO_PAGE, MAX_PHOTO_RESULTS - page.offset)) };
}

/** One page of photos matching `query` (any language) and `filter`, in the page's order. */
export async function searchPhotos(
	query: string,
	page: SearchPage,
	filter: PhotoFilter,
	signal: AbortSignal,
	client: Client = api
): Promise<PhotoSearchOutcome> {
	try {
		const { data, error, response } = await client.GET('/api/search/photos', {
			params: { query: { q: query, ...pageQuery(page), ...filterQuery(filter) } },
			signal
		});
		if (response.ok && data) return { ok: true, data };
		const status = response.status;
		if (status === 503 && code(error) === 'ml_unavailable') {
			return { ok: false, error: SEARCH_UNAVAILABLE, status, unavailable: true };
		}
		if (status === 503) return { ok: false, error: SEARCH_BUSY, status };
		if (status === 429) {
			return { ok: false, error: SEARCH_TOO_FAST, status, retryAfterMs: retryDelay(response.headers.get('Retry-After')) };
		}
		return { ok: false, error: errorMessage(error, response), status };
	} catch {
		if (signal.aborted) return { ok: false, aborted: true };
		return { ok: false, error: errorMessage(null), status: 0 };
	}
}

/** Starts the server's text model, so the first search is quick. `false`: search is not set up. */
export async function warmUpSearch(client: Client = api): Promise<boolean> {
	try {
		const { error, response } = await client.POST('/api/search/warmup');
		return !(response.status === 503 && code(error) === 'ml_unavailable');
	} catch {
		return true;
	}
}

async function call<T>(request: Promise<{ data?: T; error?: unknown; response: Response }>): Promise<Outcome<T>> {
	try {
		const { data, error, response } = await request;
		if (response.ok) return { ok: true, data: data as T };
		return { ok: false, error: errorMessage(error, response), status: response.status };
	} catch {
		return { ok: false, error: errorMessage(null), status: 0 };
	}
}

export const listTags = (): Promise<Outcome<TagGroup[]>> => call(api.GET('/api/tags'));

export const listPlaces = (level: 'city' | 'country' = 'city'): Promise<Outcome<PlaceGroup[]>> =>
	call(api.GET('/api/places', { params: { query: { level } } }));

export const listCameras = (): Promise<Outcome<CameraGroup[]>> => call(api.GET('/api/cameras'));

export const getPerson = (id: string): Promise<Outcome<Person>> =>
	call(api.GET('/api/people/{id}', { params: { path: { id } } }));

/** A person filter's chip label: the name, "Hidden person" for a hidden unnamed one, else "Unnamed". */
export function personChipLabel(person: Pick<Person, 'name' | 'hidden'> | null | undefined): string | null {
	if (!person) return null;
	if (person.name?.trim()) return person.name.trim();
	return person.hidden ? 'Hidden person' : null;
}

export const listPeople = (includeHidden = false): Promise<Outcome<Person[]>> =>
	call(api.GET('/api/people', { params: { query: { include_hidden: includeHidden } } }));

/** Faces on one photo, left to right. */
export const photoFaces = (id: string): Promise<Outcome<Face[]>> =>
	call(api.GET('/api/photos/{id}/faces', { params: { path: { id } } }));

/** One avatar per person on a photo (their first face there), in the order `people` names them. */
export function peopleOnPhoto(
	people: readonly { id: string; name?: string | null }[],
	faces: readonly Face[]
): { id: string; name: string | null; face: FaceCrop | null }[] {
	return people.map((person) => {
		const face = faces.find((f) => f.person?.id === person.id);
		return {
			id: person.id,
			name: person.name ?? null,
			face: face ? { face_id: face.id, photo_id: face.photo_id, thumbnail_version: face.thumbnail_version, box: face.box } : null
		};
	});
}

/** Places grouped under their country, in the order the server ranked them. */
export function placesByCountry(places: readonly PlaceGroup[]): { code: string; name: string | null; places: PlaceGroup[] }[] {
	const groups = new Map<string, { code: string; name: string | null; places: PlaceGroup[] }>();
	for (const place of places) {
		const group = groups.get(place.country_code) ?? { code: place.country_code, name: place.country ?? null, places: [] };
		group.places.push(place);
		groups.set(place.country_code, group);
	}
	return [...groups.values()];
}
