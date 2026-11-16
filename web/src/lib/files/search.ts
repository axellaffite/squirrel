/** Document search: query rules, snippet shaping, wording, and the request itself. */
import { api, errorMessage } from '#lib/api/client.ts';
import type { components } from '#lib/api/schema.d.ts';

export type SearchHit = components['schemas']['DocumentHit'];
export type SnippetPart = components['schemas']['SnippetPart'];
export type SearchResults = components['schemas']['DocumentResults'];

/** Shorter queries are not sent. */
export const MIN_QUERY = 2;
/** The server's limit on the query's length. */
export const MAX_QUERY = 200;
/** Typing pauses this long before a search is sent (the API allows 20 at once, then 1/s). */
export const SEARCH_DEBOUNCE_MS = 300;
/** After a 429 without Retry-After, the search is retried once after this long. */
export const RETRY_AFTER_MS = 1000;
/** A Retry-After longer than this is not waited for: the user is told to try again instead. */
export const MAX_RETRY_WAIT_MS = 10_000;
/** Shown when the retry after a 429 was rate-limited again. */
export const STILL_TOO_FAST = 'The server is still busy with searches. Wait a moment, then search again.';
export const PAGE_SIZE = 20;
/** The server never goes past this many results (`offset + limit ≤ 200`). */
export const MAX_RESULTS = 200;
/** Snippets are cut after this many characters. */
export const SNIPPET_MAX = 240;

/** Trimmed, single-spaced. */
export const normalizeQuery = (query: string): string => query.trim().replace(/\s+/g, ' ');

export const isSearchable = (query: string): boolean => {
	const q = normalizeQuery(query);
	return q.length >= MIN_QUERY && q.length <= MAX_QUERY;
};

/**
 * A hint about short words, as the server treats them: one-letter words are ignored, two-letter
 * words only match whole words (prefix matching starts at three letters). Null when none.
 */
export function shortWordHint(query: string): string | null {
	const words = normalizeQuery(query).split(' ').filter((word) => word.length > 0);
	const one = words.some((word) => [...word].length === 1);
	const two = words.some((word) => [...word].length === 2);
	if (one && two) return 'One-letter words are ignored, and two-letter words only match whole words.';
	if (one) return 'One-letter words are ignored.';
	if (two) return 'Two-letter words only match whole words.';
	return null;
}

/** How many results the next page may ask for, 0 when the server's cap is reached. */
export const nextLimit = (offset: number): number => Math.max(0, Math.min(PAGE_SIZE, MAX_RESULTS - offset));

/**
 * Snippet parts as the server split them, with neighbours of the same kind merged and the whole
 * cut at `max` characters (ending with "…"). Rendered as text: never HTML.
 */
export function snippetParts(parts: readonly SnippetPart[], max: number = SNIPPET_MAX): SnippetPart[] {
	const out: SnippetPart[] = [];
	let length = 0;
	for (const part of parts) {
		if (length >= max) break;
		const room = max - length;
		const cut = part.text.length > room;
		const text = cut ? `${part.text.slice(0, room).trimEnd()}…` : part.text;
		length += cut ? room : part.text.length;
		const last = out[out.length - 1];
		if (last && last.highlight === part.highlight) out[out.length - 1] = { ...last, text: last.text + text };
		else if (text) out.push({ text, highlight: part.highlight });
	}
	return out;
}

/** Calm wording for what can go wrong while typing. */
export function searchError(status: number, fallback: string): string {
	if (status === 429) return 'Searching a little too fast. Trying again in a moment…';
	if (status === 503) return 'The search took too long. Try more precise words.';
	if (status === 422) return `Searches use ${MIN_QUERY} to ${MAX_QUERY} characters.`;
	return fallback;
}

const KINDS: Record<string, string> = {
	'application/pdf': 'PDF',
	'text/plain': 'Text',
	'text/markdown': 'Markdown'
};

/** The kind shown in a result's metadata, from its MIME type ("PDF", "Text", "JPEG image"). */
export function kindLabel(mime: string | null | undefined): string | null {
	if (!mime) return null;
	if (KINDS[mime]) return KINDS[mime];
	if (mime.startsWith('image/')) return `${mime.slice(6).toUpperCase()} image`;
	return null;
}

/** "3 pages" / "1 page", or null. */
export const pagesLabel = (count: number | null | undefined): string | null =>
	count ? (count === 1 ? '1 page' : `${count} pages`) : null;

export type SearchOutcome =
	| { ok: true; data: SearchResults }
	| { ok: false; error: string; status: number; retryAfterMs?: number }
	| { ok: false; aborted: true };

/** Milliseconds from a `Retry-After` header in seconds; the default when absent or unreadable. */
export function retryDelay(header: string | null): number {
	const seconds = header === null ? NaN : Number(header.trim());
	return Number.isFinite(seconds) && seconds >= 0 ? seconds * 1000 : RETRY_AFTER_MS;
}

/** One page of results; resolves `{aborted}` when `signal` fired (a newer search replaced it). */
export async function searchDocuments(query: string, offset: number, signal: AbortSignal): Promise<SearchOutcome> {
	try {
		const { data, error, response } = await api.GET('/api/search/documents', {
			params: { query: { q: normalizeQuery(query), offset, limit: nextLimit(offset) } },
			signal
		});
		if (response.ok && data) return { ok: true, data };
		const failure = { ok: false as const, error: searchError(response.status, errorMessage(error, response)), status: response.status };
		if (response.status !== 429) return failure;
		return { ...failure, retryAfterMs: retryDelay(response.headers.get('Retry-After')) };
	} catch (cause) {
		if (signal.aborted) return { ok: false, aborted: true };
		void cause;
		return { ok: false, error: errorMessage(null), status: 0 };
	}
}

const dayFormat = new Intl.DateTimeFormat(undefined, { dateStyle: 'medium' });

/** "12 Mar 2026": results show the day a file last changed. */
export const shortDate = (iso: string): string => dayFormat.format(new Date(iso));

/** Where a result opens: its folder, with the file's preview. */
export const resultHref = (node: { id: string; parent_id?: string | null }): string =>
	node.parent_id ? `/files/${node.parent_id}?open=${encodeURIComponent(node.id)}` : '/files';
