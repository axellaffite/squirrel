/**
 * As-you-type photo search: waits for a pause in typing, aborts the request a newer query (or
 * filter) replaced, pages with "Load more". Only the latest search's answers are ever shown.
 */
import { MAX_PHOTO_QUERY, MAX_PHOTO_RESULTS, searchPhotos, type PhotoSearchOutcome } from '#lib/explore/api.ts';
import { normalizeFilter, sameFilter, type PhotoFilter } from '#lib/explore/filters.ts';
import { MAX_RETRY_WAIT_MS, SEARCH_DEBOUNCE_MS, normalizeQuery } from '#lib/files/search.ts';
import type { AssetSummary } from '#lib/photos/api.ts';

export type RunPhotoSearch = (
	query: string,
	offset: number,
	filter: PhotoFilter,
	signal: AbortSignal
) => Promise<PhotoSearchOutcome>;

/** Shown when the retry after a 429 was rate-limited again. */
export const STILL_TOO_FAST = 'The server is still busy with searches. Wait a moment, then search again.';

export const isPhotoQuery = (query: string): boolean => {
	const q = normalizeQuery(query);
	return q.length >= 1 && q.length <= MAX_PHOTO_QUERY;
};

export class PhotoSearcher {
	/** The query the results belong to ('' before any search). */
	query = $state('');
	filter: PhotoFilter = $state.raw({});
	items: AssetSummary[] = $state.raw([]);
	hasMore = $state(false);
	loading = $state(false);
	error: string | null = $state(null);
	/** Search is not set up on this server (no text model): nothing to retry. */
	unavailable = $state(false);

	readonly #run: RunPhotoSearch;
	readonly #delay: number;
	#timer: ReturnType<typeof setTimeout> | undefined;
	#controller: AbortController | null = null;
	/** The query of the request in flight, if any. */
	#pending: string | null = null;
	/** The query typed last (searched once the filter changes). */
	#typed = '';

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
			return;
		}
		this.#timer = setTimeout(() => void this.#fetch(query, 0), this.#delay);
	}

	/** Searches at once (the form was submitted, or the page opened with a query). */
	now(raw: string): void {
		clearTimeout(this.#timer);
		const query = normalizeQuery(raw);
		this.#typed = query;
		if (isPhotoQuery(query)) void this.#fetch(query, 0);
	}

	/** Narrows the results: searches the current query again at once. */
	setFilter(filter: PhotoFilter): void {
		if (sameFilter(filter, this.filter)) return;
		this.filter = normalizeFilter(filter);
		clearTimeout(this.#timer);
		if (isPhotoQuery(this.#typed)) void this.#fetch(this.#typed, 0);
		else this.#clear();
	}

	/** The next page of the current results. */
	more(): void {
		if (!this.hasMore || this.loading || this.items.length >= MAX_PHOTO_RESULTS) return;
		void this.#fetch(this.query, this.items.length);
	}

	dispose(): void {
		clearTimeout(this.#timer);
		this.#cancel();
	}

	#clear() {
		this.query = '';
		this.items = [];
		this.hasMore = false;
		this.error = null;
	}

	#cancel() {
		this.#controller?.abort();
		this.#controller = null;
		this.#pending = null;
		this.loading = false;
	}

	async #fetch(query: string, offset: number, retried = false) {
		this.#cancel();
		const controller = new AbortController();
		this.#controller = controller;
		this.#pending = query;
		this.loading = true;
		const filter = this.filter;
		const result = await this.#run(query, offset, filter, controller.signal);
		if (controller !== this.#controller) return;
		this.#controller = null;
		this.#pending = null;
		this.loading = false;
		if (!result.ok) {
			if ('aborted' in result) return;
			this.unavailable = result.unavailable === true;
			const wait = result.retryAfterMs;
			if (result.status === 429 && !retried && wait !== undefined && wait <= MAX_RETRY_WAIT_MS) {
				this.error = result.error;
				this.#timer = setTimeout(() => void this.#fetch(query, offset, true), wait);
				return;
			}
			this.error = result.status === 429 ? STILL_TOO_FAST : result.error;
			// Results of another search must not pass for this one's.
			if (offset === 0) {
				this.query = '';
				this.items = [];
				this.hasMore = false;
			}
			return;
		}
		this.error = null;
		this.unavailable = false;
		this.query = query;
		this.items = offset === 0 ? result.data.items : [...this.items, ...result.data.items];
		this.hasMore = result.data.has_more && this.items.length < MAX_PHOTO_RESULTS;
	}
}
