/** `?photo=<id>` in the address, so reloading or sharing the tab reopens the same photo. */

const PARAM = 'photo';

export function photoInAddress(url: URL | { searchParams: { get: (name: string) => string | null } }): string | null {
	return url.searchParams.get(PARAM) || null;
}

/** A copy of `href` showing photo `id`, or no photo when `id` is null. */
export function withPhoto(href: string, id: string | null): URL {
	const url = new URL(href);
	if (id) url.searchParams.set(PARAM, id);
	else url.searchParams.delete(PARAM);
	return url;
}
