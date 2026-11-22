/**
 * As-you-type photo search: waits for a pause in typing, aborts the request a newer query (or
 * filter, or order) replaced, pages as the view nears the end. Only the latest search's answers
 * are ever shown.
 */
import {
	MAX_PHOTO_QUERY,
	MAX_PHOTO_RESULTS,
	searchPhotos,
	type PhotoSearchOutcome,
	type SearchOrder,
	type SearchPage
} from '#lib/explore/api.ts';
import { normalizeFilter, sameFilter, type PhotoFilter } from '#lib/explore/filters.ts';
import { MAX_RETRY_WAIT_MS, SEARCH_DEBOUNCE_MS, normalizeQuery } from '#lib/files/search.ts';
import type { AssetSummary } from '#lib/photos/api.ts';

export type RunPhotoSearch = (
	query: string,
	page: SearchPage,
	filter: PhotoFilter,
	signal: AbortSignal
) => Promise<PhotoSearchOutcome>;

/** While the server's text model starts (503 `busy`), the search is tried again this often… */
export const BUSY_RETRY_MS = 2000;
/** …this many times, then the user is told to try again. */
export const BUSY_RETRIES = 5;

/** Shown for a query the server would refuse. */
export const TOO_LONG = `Searches use at most ${MAX_PHOTO_QUERY} characters.`;

/** Shown when the retry after a 429 was rate-limited again. */
export const STILL_TOO_FAST = 'The server is still busy with searches. Wait a moment, then search again.';

/** Results come as a timeline unless the user asks for the best matches first. */
export const DEFAULT_ORDER: SearchOrder = 'date';

/** The address keeps only a departure from the default: `?order=relevance`. */
export const orderFromParams = (params: Pick<URLSearchParams, 'get'>): SearchOrder =>
	params.get('order') === 'relevance' ? 'relevance' : DEFAULT_ORDER;

/** `url` showing `order` (other keys kept). */
export function withOrder(url: string | URL, order: SearchOrder): URL {
	const next = new URL(url);
	if (order === DEFAULT_ORDER) next.searchParams.delete('order');
	else next.searchParams.set('order', order);
	return next;
}

const firstPage = (order: SearchOrder): SearchPage =>
	order === 'date' ? { order: 'date', cursor: null } : { order: 'relevance', offset: 0 };

const isFirstPage = (page: SearchPage): boolean => (page.order === 'date' ? page.cursor === null : page.offset === 0);

export const isPhotoQuery = (query: string): boolean => {
	const q = normalizeQuery(query);
	return q.length >= 1 && q.length <= MAX_PHOTO_QUERY;
};

export class PhotoSearcher {
	/** The query the results belong to ('' before any search). */
	query = $state('');
	filter: PhotoFilter = $state.raw({});
	/** The order asked for (the next search's). */
	order: SearchOrder = $state(DEFAULT_ORDER);
	items: AssetSummary[] = $state.raw([]);
	/** The order `items` came in: the view follows it, never an order still loading. */
	itemsOrder: SearchOrder = $state(DEFAULT_ORDER);
	hasMore = $state(false);
	loading = $state(false);
	error: string | null = $state(null);
	/** Search is not set up on this server (no text model): nothing to retry. */
	unavailable = $state(false);

	readonly #run: RunPhotoSearch;
	readonly #delay: number;
	/** The pause after typing. */
	#timer: ReturnType<typeof setTimeout> | undefined;
	/** A retry of the request in flight's search (busy or rate-limited); part of that request. */
	#retry: ReturnType<typeof setTimeout> | undefined;
	#controller: AbortController | null = null;
	/** The query of the request in flight, if any. */
	#pending: string | null = null;
	/** The query typed last (searched once the filter changes). */
	#typed = '';
	/** Where the next date-ordered page starts. */
	#cursor: string | null = null;

	constructor(run: RunPhotoSearch = searchPhotos, delay: number = SEARCH_DEBOUNCE_MS) {
		this.#run = run;
		this.#delay = delay;
	}

	/** A new query typed: searches after the pause, or clears for an empty one. */
	type(raw: string): void {
		clearTimeout(this.#timer);
		const query = normalizeQuery(raw);
		this.#typed = query;
		if (this.#pending !== null && this.#pending !== query) this.#cancel();
		if (query === this.#pending) return;
		if (query === this.query && this.query !== '') return;
		if (!isPhotoQuery(query)) {
			this.#cancel();
			this.#clear();
			if (query.length > MAX_PHOTO_QUERY) this.error = TOO_LONG;
			return;
		}
		this.#timer = setTimeout(() => void this.#fetch(query, firstPage(this.order)), this.#delay);
	}

	/** Searches at once (the form was submitted, or the page opened with a query). */
	now(raw: string): void {
		clearTimeout(this.#timer);
		const query = normalizeQuery(raw);
		this.#typed = query;
		if (isPhotoQuery(query)) void this.#fetch(query, firstPage(this.order));
		else if (query.length > MAX_PHOTO_QUERY) {
			this.#cancel();
			this.#clear();
			this.error = TOO_LONG;
		}
	}

	/** Narrows the results: searches the current query again at once. */
	setFilter(filter: PhotoFilter): void {
		if (sameFilter(filter, this.filter)) return;
		this.filter = normalizeFilter(filter);
		clearTimeout(this.#timer);
		if (isPhotoQuery(this.#typed)) void this.#fetch(this.#typed, firstPage(this.order));
		else this.#clear();
	}

	/** Orders the results otherwise: searches the current query again at once. */
	setOrder(order: SearchOrder): void {
		if (order === this.order) return;
		this.order = order;
		clearTimeout(this.#timer);
		if (isPhotoQuery(this.#typed)) void this.#fetch(this.#typed, firstPage(order));
	}

	/** The next page of the current results. */
	more(): void {
		// Not while a request (or its retry) is pending: it would replace it.
		if (!this.hasMore || this.loading || this.#pending !== null || this.items.length >= MAX_PHOTO_RESULTS) return;
		if (this.itemsOrder === 'relevance') {
			void this.#fetch(this.query, { order: 'relevance', offset: this.items.length });
		} else if (this.#cursor !== null) {
			void this.#fetch(this.query, { order: 'date', cursor: this.#cursor });
		}
	}

	dispose(): void {
		clearTimeout(this.#timer);
		clearTimeout(this.#retry);
		this.#cancel();
	}

	#clear() {
		this.query = '';
		this.items = [];
		this.hasMore = false;
		this.#cursor = null;
		this.error = null;
	}

	#cancel() {
		clearTimeout(this.#retry);
		this.#controller?.abort();
		this.#controller = null;
		this.#pending = null;
		this.loading = false;
	}

	async #fetch(query: string, page: SearchPage, retried = false, busyTries = 0) {
		this.#cancel();
		const controller = new AbortController();
		this.#controller = controller;
		this.#pending = query;
		this.loading = true;
		const filter = this.filter;
		const result = await this.#run(query, page, filter, controller.signal);
		if (controller !== this.#controller) return;
		this.#controller = null;
		this.#pending = null;
		this.loading = false;
		if (!result.ok) {
			if ('aborted' in result) return;
			this.unavailable = result.unavailable === true;
			// Busy: the text model is starting (the page warmed it up). Keep the query and try again.
			if (result.status === 503 && !this.unavailable && busyTries < BUSY_RETRIES) {
				this.error = null;
				this.loading = true;
				this.#pending = query;
				this.#retry = setTimeout(() => void this.#fetch(query, page, retried, busyTries + 1), BUSY_RETRY_MS);
				return;
			}
			const wait = result.retryAfterMs;
			if (result.status === 429 && !retried && wait !== undefined && wait <= MAX_RETRY_WAIT_MS) {
				this.error = result.error;
				// Still this query's request: typing it again waits for the retry, never sooner.
				this.#pending = query;
				this.#retry = setTimeout(() => void this.#fetch(query, page, true), wait);
				return;
			}
			this.error = result.status === 429 ? STILL_TOO_FAST : result.error;
			// Results of another search must not pass for this one's.
			if (isFirstPage(page)) {
				this.query = '';
				this.items = [];
				this.hasMore = false;
				this.#cursor = null;
			}
			return;
		}
		this.error = null;
		this.unavailable = false;
		this.query = query;
		this.itemsOrder = page.order;
		this.items = isFirstPage(page) ? result.data.items : [...this.items, ...result.data.items];
		this.#cursor = page.order === 'date' ? (result.data.next_cursor ?? null) : null;
		const reachable = page.order === 'relevance' || this.#cursor !== null;
		this.hasMore = result.data.has_more && reachable && this.items.length < MAX_PHOTO_RESULTS;
	}
}
