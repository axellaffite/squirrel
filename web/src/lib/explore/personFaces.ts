/** The person page's face corrections: what to send, and what stays selected afterwards. */
import type { Outcome } from '#lib/files/pages.ts';
import { tileDateTime } from '#lib/photos/dates.ts';

/** The selection, without faces that left the list (moved, split off, deleted). */
export function pruneSelection(selected: ReadonlySet<string>, faces: readonly { id: string }[]): Set<string> {
	const present = new Set(faces.map((face) => face.id));
	return new Set([...selected].filter((id) => present.has(id)));
}

/** Where "Move to…" sends the faces: a new person is a split of this one, else each face is assigned. */
export type MoveRequest = { kind: 'split' } | { kind: 'assign'; person: string };

export function moveRequest(choice: { person: { id: string } } | { newPerson: true }): MoveRequest {
	return 'newPerson' in choice ? { kind: 'split' } : { kind: 'assign', person: choice.person.id };
}

/** The outcome of a correction on several faces. */
export interface BulkResult {
	/** Faces the change was made for. */
	done: number;
	/** Faces it failed for: they stay selected, so the user can try again. */
	stillSelected: Set<string>;
	error: string | null;
}

/** Runs `step` for each face, at most `limit` at once. */
export async function bulk(
	ids: readonly string[],
	step: (id: string) => Promise<Outcome<unknown>>,
	limit = 4
): Promise<BulkResult> {
	const failed = new Set<string>();
	let error: string | null = null;
	for (let start = 0; start < ids.length; start += limit) {
		const batch = ids.slice(start, start + limit);
		const results = await Promise.all(batch.map(step));
		results.forEach((result, index) => {
			if (!result.ok) {
				failed.add(batch[index]);
				error ??= result.error;
			}
		});
	}
	return { done: ids.length - failed.size, stillSelected: failed, error };
}

/** "3 faces" / "1 face". */
export const facesCount = (count: number): string => `${count} ${count === 1 ? 'face' : 'faces'}`;

/**
 * Tells apart answers of successive reloads: only the latest one may be shown.
 * `next()` starts a reload and returns its ticket; `current(ticket)` says whether it is the latest.
 */
export class Generations {
	#latest = 0;

	next(): number {
		this.#latest += 1;
		return this.#latest;
	}

	current(ticket: number): boolean {
		return ticket === this.#latest;
	}
}

/**
 * Each face tile's accessible name, unique on the page: "Face from 14 July 2024 18:30", with
 * "(2)" added when two photos share the minute, or an ordinal without a date.
 */
export function faceTileLabels(faces: readonly { taken_at?: string | null }[], locale?: string): string[] {
	const seen = new Map<string, number>();
	return faces.map((face, index) => {
		const base = face.taken_at ? `Face from ${tileDateTime(face.taken_at, locale)}` : `Face ${index + 1}`;
		const count = (seen.get(base) ?? 0) + 1;
		seen.set(base, count);
		return count === 1 ? base : `${base} (${count})`;
	});
}
