/**
 * The app-wide blurred photos (see `blurState.ts`): loaded once signed in, refreshed when the
 * tab comes back and after the filter or the library changes. Every thumbnail, cover and
 * preview asks `blurred.has(id)` before showing a photo.
 */
import { api, errorMessage } from '#lib/api/client.ts';
import type { components } from '#lib/api/schema.d.ts';
import type { Outcome } from '#lib/files/pages.ts';
import {
	hasBlurred,
	initialBlurState,
	isBlurred,
	withIds,
	withRevealAll,
	withRevealed,
	type BlurState
} from '#lib/photos/blurState.ts';

export type BlurFilter = components['schemas']['BlurFilterDto'];

/** This browser remembers whether the filter blurred anything, for the first paint. */
const ACTIVE_KEY = 'squirrel.blur-filter.active';
/** Refreshes closer together than this are skipped. */
const MIN_REFRESH_MS = 10_000;

function storage(): Storage | null {
	try {
		return typeof localStorage === 'undefined' ? null : localStorage;
	} catch {
		return null;
	}
}

function readActive(): boolean {
	try {
		return storage()?.getItem(ACTIVE_KEY) === '1';
	} catch {
		return false;
	}
}

function saveActive(active: boolean): void {
	try {
		storage()?.setItem(ACTIVE_KEY, active ? '1' : '0');
	} catch {
		// Private window or blocked storage: the first paint is just not blurred ahead.
	}
}

async function call<T>(request: Promise<{ data?: T; error?: unknown; response: Response }>): Promise<Outcome<T>> {
	try {
		const { data, error, response } = await request;
		if (response.ok) return { ok: true, data: data as T };
		return { ok: false, error: errorMessage(error, response), status: response.status };
	} catch {
		return { ok: false, error: errorMessage(null), status: 0 };
	}
}

export const getBlurFilter = (): Promise<Outcome<BlurFilter>> => call(api.GET('/api/me/blur-filter'));

/** Saves the filter; `rules: null` forgets it. */
export const saveBlurFilter = (filter: BlurFilter): Promise<Outcome<BlurFilter>> =>
	call(api.PUT('/api/me/blur-filter', { body: filter }));

class Blurred {
	#state: BlurState = $state.raw(initialBlurState(readActive()));
	#lastRefresh = 0;
	#pending: Promise<void> | null = null;

	/** Whether photo `id` shows blurred right now. */
	has(id: string): boolean {
		return isBlurred(this.#state, id);
	}

	/** Whether any photo is blurred by the filter (revealed or not). */
	get any(): boolean {
		return hasBlurred(this.#state);
	}

	get revealAll(): boolean {
		return this.#state.revealAll;
	}

	reveal(id: string): void {
		this.#state = withRevealed(this.#state, id);
	}

	setRevealAll(revealAll: boolean): void {
		this.#state = withRevealAll(this.#state, revealAll);
	}

	/** Asks the server again; `force` skips the throttle (the filter just changed). */
	refresh(force = false): Promise<void> {
		if (!force && Date.now() - this.#lastRefresh < MIN_REFRESH_MS) return this.#pending ?? Promise.resolve();
		this.#lastRefresh = Date.now();
		this.#pending = this.#load().finally(() => (this.#pending = null));
		return this.#pending;
	}

	async #load(): Promise<void> {
		const result = await call(api.GET('/api/photos/blurred'));
		if (!result.ok) {
			// Photos are shown rather than blurred forever; the next refresh tries again at once.
			this.#lastRefresh = 0;
			if (this.#state.ids === null) this.#state = withIds(this.#state, []);
			return;
		}
		this.#state = withIds(this.#state, result.data.node_ids);
		saveActive(result.data.node_ids.length > 0);
	}

	/** Signed out: nothing is known any more (the next account starts afresh). */
	reset(): void {
		this.#state = initialBlurState(readActive());
		this.#lastRefresh = 0;
	}
}

export const blurred = new Blurred();

/** Loads the list and keeps it fresh while the tab is visible; returns the cleanup. */
export function watchBlurred(): () => void {
	void blurred.refresh(true);
	const visible = () => {
		if (document.visibilityState === 'visible') void blurred.refresh();
	};
	document.addEventListener('visibilitychange', visible);
	return () => {
		document.removeEventListener('visibilitychange', visible);
		blurred.reset();
	};
}
