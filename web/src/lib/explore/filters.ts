/**
 * The photo filter (timeline, months and photo search): its address form (`?person=…&tag=…`),
 * the query it sends, and the chips that show it. Mirrors the server's rules: a region or city
 * needs a country, a camera model needs its make.
 */

export type PhotoKind = 'photo' | 'screenshot';

export interface PhotoFilter {
	person?: string;
	tag?: string;
	country?: string;
	/** Empty: places without a region (as Explore lists them). */
	region?: string;
	city?: string;
	camera_make?: string;
	camera_model?: string;
	kind?: PhotoKind;
}

/** The address keys of a filter, in chip order. */
export const FILTER_KEYS = [
	'person',
	'tag',
	'country',
	'region',
	'city',
	'camera_make',
	'camera_model',
	'kind'
] as const satisfies readonly (keyof PhotoFilter)[];

const KINDS: readonly PhotoKind[] = ['photo', 'screenshot'];
/** The server's limit on a filter value. */
const MAX_VALUE = 200;

/** The checked filter: anything the server would refuse is left out, never sent. */
export function normalizeFilter(raw: PhotoFilter): PhotoFilter {
	const out: PhotoFilter = {};
	const text = (value: string | undefined) => {
		const trimmed = value?.trim();
		return trimmed && trimmed.length <= MAX_VALUE ? trimmed : undefined;
	};
	const person = text(raw.person);
	if (person) out.person = person;
	const tag = text(raw.tag);
	if (tag) out.tag = tag;
	const country = text(raw.country);
	if (country) {
		out.country = country;
		if (raw.region === '') out.region = '';
		else {
			const region = text(raw.region);
			if (region) out.region = region;
		}
		const city = text(raw.city);
		if (city) out.city = city;
	}
	const make = text(raw.camera_make);
	if (make) {
		out.camera_make = make;
		const model = text(raw.camera_model);
		if (model) out.camera_model = model;
	}
	if (raw.kind && KINDS.includes(raw.kind)) out.kind = raw.kind;
	return out;
}

export const isEmptyFilter = (filter: PhotoFilter): boolean => Object.keys(normalizeFilter(filter)).length === 0;

export function sameFilter(a: PhotoFilter, b: PhotoFilter): boolean {
	const x = normalizeFilter(a);
	const y = normalizeFilter(b);
	return FILTER_KEYS.every((key) => x[key] === y[key]);
}

/** The filter in an address's query. */
export function filterFromParams(params: Pick<URLSearchParams, 'get'>): PhotoFilter {
	const raw: PhotoFilter = {};
	for (const key of FILTER_KEYS) {
		const value = params.get(key);
		if (value !== null) (raw as Record<string, string>)[key] = value;
	}
	return normalizeFilter(raw);
}

/** `url` with its filter keys replaced by `filter`'s (other keys, like `q`, kept). */
export function withFilter(url: string | URL, filter: PhotoFilter): URL {
	const next = new URL(url);
	for (const key of FILTER_KEYS) next.searchParams.delete(key);
	const checked = normalizeFilter(filter);
	for (const key of FILTER_KEYS) {
		const value = checked[key];
		if (value !== undefined) next.searchParams.set(key, value);
	}
	return next;
}

/** `/photos?…` showing this filter. */
export function photosHref(filter: PhotoFilter): string {
	const url = withFilter('http://local/photos', filter);
	return `${url.pathname}${url.search}`;
}

/** The query parameters for the API (absent keys omitted). */
export function filterQuery(filter: PhotoFilter): PhotoFilter {
	return normalizeFilter(filter);
}

/** "birthday-cake" → "Birthday cake". */
export function tagLabel(slug: string): string {
	const words = slug.replace(/[-_]+/g, ' ').trim();
	return words.charAt(0).toUpperCase() + words.slice(1);
}

/** A country's name from its ISO code, in the browser's language ("FR" → "France"). */
export function countryName(code: string, locale?: string): string {
	try {
		return new Intl.DisplayNames(locale ? [locale] : undefined, { type: 'region' }).of(code.toUpperCase()) ?? code;
	} catch {
		return code;
	}
}

export type ChipKind = 'person' | 'tag' | 'place' | 'camera' | 'kind';

export interface Chip {
	kind: ChipKind;
	label: string;
	/** The filter once this chip is removed. */
	without: PhotoFilter;
}

const KIND_LABELS: Record<PhotoKind, string> = { photo: 'Photos only', screenshot: 'Screenshots' };

/** One chip per filter part. `personName` names a person id (null: unnamed). */
export function chips(filter: PhotoFilter, personName: (id: string) => string | null = () => null): Chip[] {
	const f = normalizeFilter(filter);
	const out: Chip[] = [];
	const drop = (...keys: (keyof PhotoFilter)[]) => {
		const next = { ...f };
		for (const key of keys) delete next[key];
		return next;
	};
	if (f.person) out.push({ kind: 'person', label: personName(f.person) ?? 'Unnamed person', without: drop('person') });
	if (f.tag) out.push({ kind: 'tag', label: tagLabel(f.tag), without: drop('tag') });
	if (f.country) {
		const label = f.city ?? (f.region ? f.region : countryName(f.country));
		out.push({ kind: 'place', label, without: drop('country', 'region', 'city') });
	}
	if (f.camera_make) {
		const label = f.camera_model ? `${f.camera_make} ${f.camera_model}` : f.camera_make;
		out.push({ kind: 'camera', label, without: drop('camera_make', 'camera_model') });
	}
	if (f.kind) out.push({ kind: 'kind', label: KIND_LABELS[f.kind], without: drop('kind') });
	return out;
}

/**
 * The filter for a place as Explore lists it: a city keeps its region (empty when it has none,
 * which matches places without a region), a country stands alone.
 */
export function placeFilter(place: { country_code: string; region?: string | null; city?: string | null }): PhotoFilter {
	if (!place.city) return { country: place.country_code };
	return { country: place.country_code, region: place.region ?? '', city: place.city };
}
