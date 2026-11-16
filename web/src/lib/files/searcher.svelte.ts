/**
 * As-you-type search: waits for a pause in typing, aborts the request a newer query replaced, and
 * pages with "Load more". Only the latest query's answers are ever shown.
 */
import {
	isSearchable,
	MAX_RESULTS,
	normalizeQuery,
	SEARCH_DEBOUNCE_MS,
	searchDocuments,
	type SearchHit,
	type SearchOutcome
} from '#lib/files/search.ts';

export type RunSearch = (query: string, offset: number, signal: AbortSignal) => Promise<SearchOutcome>;

export class Searcher {
	/** The query the results belong to ('' before any search). */
	query = $state('');
	items: SearchHit[] = $state.raw([]);
	hasMore = $state(false);
	loading = $state(false);
	error: string | null = $state(null);

	readonly #run: RunSearch;
	readonly #delay: number;
	#timer: ReturnType<typeof setTimeout> | undefined;
	#controller: AbortController | null = null;
	/** The query of the request in flight, if any. */
	#pending: string | null = null;

	constructor(run: RunSearch = searchDocuments, delay: number = SEARCH_DEBOUNCE_MS) {
		this.#run = run;
		this.#delay = delay;
	}

	/** A new query typed: searches after the pause, or clears for a query too short to send. */
	type(raw: string): void {
		clearTimeout(this.#timer);
		const query = normalizeQuery(raw);
		// A request for another query is now useless (e.g. "abcd" after backspacing to "abc").
		if (this.#pending !== null && this.#pending !== query) this.#cancel();
		if (query === this.#pending) return;
		if (query === this.query && this.query !== '') return;
		if (!isSearchable(query)) {
			this.#cancel();
			this.query = '';
			this.items = [];
			this.hasMore = false;
			this.error = null;
			return;
		}
		this.#timer = setTimeout(() => void this.#fetch(query, 0), this.#delay);
	}

	/** Searches at once (e.g. the form was submitted). */
	now(raw: string): void {
		clearTimeout(this.#timer);
		const query = normalizeQuery(raw);
		if (isSearchable(query)) void this.#fetch(query, 0);
	}

	/** The next page of the current results. */
	more(): void {
		if (!this.hasMore || this.loading || this.items.length >= MAX_RESULTS) return;
		void this.#fetch(this.query, this.items.length);
	}

	dispose(): void {
		clearTimeout(this.#timer);
		this.#cancel();
	}

	#cancel() {
		this.#controller?.abort();
		this.#controller = null;
		this.#pending = null;
		this.loading = false;
	}

	async #fetch(query: string, offset: number) {
		this.#cancel();
		const controller = new AbortController();
		this.#controller = controller;
		this.#pending = query;
		this.loading = true;
		const result = await this.#run(query, offset, controller.signal);
		if (controller !== this.#controller) return;
		this.#controller = null;
		this.#pending = null;
		this.loading = false;
		if (!result.ok) {
			if ('aborted' in result) return;
			this.error = result.error;
			return;
		}
		this.error = null;
		this.query = query;
		this.items = offset === 0 ? result.data.items : [...this.items, ...result.data.items];
		this.hasMore = result.data.has_more && this.items.length < MAX_RESULTS;
	}
}
