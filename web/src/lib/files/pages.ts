export type Outcome<T> = { ok: true; data: T } | { ok: false; error: string; status: number };

interface PageBody<T> {
	items: T[];
	next_cursor?: string | null;
}

/** Enough for 100 000 entries at 500 per page; past that, something is wrong. */
export const MAX_PAGES = 200;

const REPEATED_CURSOR = 'The server sent the same page twice. Reload to try again.';
const TOO_MANY_PAGES = 'This listing has too many pages to load at once.';

/**
 * Fetches every page of a keyset-paginated listing. Stops with an error rather than
 * looping forever if the server repeats a cursor or the listing exceeds `maxPages`.
 */
export async function collectPages<T>(
	fetchPage: (cursor: string | null) => Promise<Outcome<PageBody<T>>>,
	maxPages: number = MAX_PAGES
): Promise<Outcome<T[]>> {
	const items: T[] = [];
	const seen = new Set<string>();
	let cursor: string | null = null;
	for (let pages = 0; ; pages += 1) {
		if (pages >= maxPages) return { ok: false, error: TOO_MANY_PAGES, status: 0 };
		const page = await fetchPage(cursor);
		if (!page.ok) return page;
		items.push(...page.data.items);
		cursor = page.data.next_cursor ?? null;
		if (!cursor) return { ok: true, data: items };
		if (seen.has(cursor)) return { ok: false, error: REPEATED_CURSOR, status: 0 };
		seen.add(cursor);
	}
}
