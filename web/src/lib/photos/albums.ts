/** Wording and ordering for albums. */

/** The API accepts at most this many photos per add or remove request. */
export const MAX_ITEMS_PER_REQUEST = 500;

interface Named {
	name: string;
}

/** Albums by name, the way people look for them in a picker (accents and case ignored). */
export function sortByName<T extends Named>(albums: readonly T[], locale?: string): T[] {
	return [...albums].sort((a, b) =>
		a.name.localeCompare(b.name, locale, { sensitivity: 'base', numeric: true })
	);
}

/** "1 photo", "12 photos". */
export const photoCount = (count: number): string => (count === 1 ? '1 photo' : `${count} photos`);

/** "Added 3 photos to Holidays" (or why nothing changed). */
export function addedMessage(changed: number, requested: number, album: string): string {
	if (changed === 0) return requested === 1 ? `This photo is already in ${album}.` : `These photos are already in ${album}.`;
	return `Added ${photoCount(changed)} to ${album}.`;
}

/** Splits ids into request-sized batches. */
export function batches<T>(items: readonly T[], size: number = MAX_ITEMS_PER_REQUEST): T[][] {
	const out: T[][] = [];
	for (let i = 0; i < items.length; i += size) out.push(items.slice(i, i + size));
	return out;
}

/** The server's AlbumName limit, in characters (after trimming and NFC). */
export const ALBUM_NAME_MAX = 100;

/** Control characters (Cc) and invisible formatting characters (Cf), refused by the server. */
const HIDDEN = /[\p{Cc}\p{Cf}]/u;

/**
 * Mirrors the server's AlbumName rules: 1–100 characters once trimmed and NFC-normalised, no
 * control or invisible formatting characters. Unlike file names, "/", "." and ".." are fine.
 */
export function albumNameProblem(raw: string): string | null {
	const name = raw.trim().normalize('NFC');
	const length = [...name].length;
	if (length === 0) return 'Enter a name.';
	if (length > ALBUM_NAME_MAX) return `Album names have at most ${ALBUM_NAME_MAX} characters.`;
	if (HIDDEN.test(name)) return 'Names cannot contain invisible or control characters.';
	return null;
}
