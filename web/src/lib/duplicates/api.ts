/** Duplicates: the caller's groups of near-identical photos, keeping one, restoring hidden ones. */
import { api, errorMessage } from '#lib/api/client.ts';
import type { components } from '#lib/api/schema.d.ts';
import type { Outcome } from '#lib/files/pages.ts';

type Client = typeof api;

export type DuplicateGroup = components['schemas']['DuplicateGroupDto'];
export type DuplicateMember = components['schemas']['DuplicateMemberDto'];
export type GroupState = components['schemas']['DuplicateGroupState'];
export type DuplicatesPage = components['schemas']['DuplicatesPage'];

async function call<T>(request: Promise<{ data?: T; error?: unknown; response: Response }>): Promise<Outcome<T>> {
	try {
		const { data, error, response } = await request;
		if (response.ok) return { ok: true, data: data as T };
		return { ok: false, error: errorMessage(error, response), status: response.status };
	} catch {
		return { ok: false, error: errorMessage(null), status: 0 };
	}
}

/** Groups fetched per page (a few screens of tiles). */
export const GROUP_PAGE = 60;

/** One page of the groups in `state`, newest first, after `cursor`. */
export const listDuplicates = (
	state: GroupState,
	cursor: string | null = null,
	client: Client = api
): Promise<Outcome<DuplicatesPage>> =>
	call(client.GET('/api/duplicates', { params: { query: { state, limit: GROUP_PAGE, cursor: cursor ?? undefined } } }));

/** How many groups wait for the user's decision. */
export async function openDuplicateCount(client: Client = api): Promise<Outcome<number>> {
	const outcome = await call(client.GET('/api/duplicates', { params: { query: { state: 'open', limit: 1 } } }));
	return outcome.ok ? { ok: true, data: outcome.data.open_count } : outcome;
}

/** Keeps `asset`: the group's other photos are hidden everywhere (not deleted). */
export const keepDuplicate = (group: string, asset: string, client: Client = api): Promise<Outcome<DuplicateGroup>> =>
	call(client.POST('/api/duplicates/{id}/keep', { params: { path: { id: group } }, body: { asset } }));

/**
 * Shows hidden photos of `group` again (`asset` only, or all of them). `null`: the group is gone
 * (one photo was left in it).
 */
export async function restoreDuplicates(
	group: string,
	asset: string | null = null,
	client: Client = api
): Promise<Outcome<DuplicateGroup | null>> {
	const outcome = await call(
		client.POST('/api/duplicates/{id}/restore', {
			params: { path: { id: group } },
			body: asset ? { asset } : {}
		})
	);
	if (!outcome.ok) return outcome;
	return { ok: true, data: (outcome.data as DuplicateGroup | undefined) ?? null };
}
