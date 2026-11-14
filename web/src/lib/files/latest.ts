export type Latest<T> = { stale: false; value: T } | { stale: true };

/**
 * Tags responses so only the most recent request counts: when a newer request started
 * meanwhile, the older one resolves `{ stale: true }` (even if it failed) and is ignored.
 *
 *   const track = latestOnly();
 *   const result = await track(listFolder(id));
 *   if (result.stale) return;
 */
export function latestOnly(): <T>(request: Promise<T>) => Promise<Latest<T>> {
	let sequence = 0;
	return async <T>(request: Promise<T>): Promise<Latest<T>> => {
		const ticket = ++sequence;
		try {
			const value = await request;
			return ticket === sequence ? { stale: false, value } : { stale: true };
		} catch (error) {
			if (ticket !== sequence) return { stale: true };
			throw error;
		}
	};
}
