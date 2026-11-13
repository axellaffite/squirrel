export type Outcome<T> = { ok: true; data: T } | { ok: false; error: string; status: number };

interface PageBody<T> {
	items: T[];
	next_cursor?: string | null;
}

/** Fetches every page of a keyset-paginated listing. */
export async function collectPages<T>(
	fetchPage: (cursor: string | null) => Promise<Outcome<PageBody<T>>>
): Promise<Outcome<T[]>> {
	const items: T[] = [];
	let cursor: string | null = null;
	do {
		const page = await fetchPage(cursor);
		if (!page.ok) return page;
		items.push(...page.data.items);
		cursor = page.data.next_cursor ?? null;
	} while (cursor);
	return { ok: true, data: items };
}
