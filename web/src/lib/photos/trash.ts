/** Moving photos to the trash: one request per photo, a few at a time. */
import { trashNode } from '#lib/files/api.ts';
import { mapLimit } from '#lib/photos/batch.ts';
import type { Outcome } from '#lib/photos/api.ts';

/** Requests running at once. */
const PARALLEL = 4;

export interface TrashResult {
	/** Ids that went to the trash. */
	moved: string[];
	/** Null when everything worked. */
	error: string | null;
}

export async function trashPhotos(
	ids: readonly string[],
	trash: (id: string) => Promise<Outcome<unknown>> = trashNode
): Promise<TrashResult> {
	const outcomes = await mapLimit(ids, PARALLEL, trash);
	const moved = ids.filter((_, index) => outcomes[index].ok);
	const failures = outcomes.filter((outcome) => !outcome.ok);
	if (failures.length === 0) return { moved, error: null };
	const first = failures[0];
	const reason = first.ok ? '' : first.error;
	const count = failures.length === 1 ? '1 photo was' : `${failures.length} photos were`;
	return { moved, error: `${count} not moved to the trash. ${reason}` };
}

/**
 * The photo to show after `id` leaves `order` (the next one, else the previous one), or null
 * when there is none or `id` is not in `order` (e.g. opened from the address, month not loaded).
 */
export function neighbourAfterRemoval<T extends { id: string }>(order: readonly T[], id: string): T | null {
	const index = order.findIndex((item) => item.id === id);
	if (index < 0) return null;
	return order[index + 1] ?? order[index - 1] ?? null;
}
