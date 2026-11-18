/**
 * Settings › Faces: the switch's state. It only moves once the server agreed; turning faces off
 * asks first, with counts fetched when the question opens (never stale ones).
 */
import type { Outcome } from '#lib/files/pages.ts';
import { getMlSettings, setFacesEnabled, type MlSettings } from '#lib/explore/people.ts';

export interface FacesApi {
	get: () => Promise<Outcome<MlSettings>>;
	set: (enabled: boolean) => Promise<Outcome<MlSettings>>;
}

export interface FaceCounts {
	people: number;
	faces: number;
}

/** "2 people and 8 faces". */
export function countsText(counts: FaceCounts): string {
	const people = `${counts.people} ${counts.people === 1 ? 'person' : 'people'}`;
	const faces = `${counts.faces} ${counts.faces === 1 ? 'face' : 'faces'}`;
	return `${people} and ${faces}`;
}

const countsOf = (settings: MlSettings): FaceCounts => ({ people: settings.people_count, faces: settings.faces_count });

export const ON_NOTICE = 'Face recognition is on. Your photos are being analysed; people appear in Explore as faces are found.';
export const OFF_NOTICE = 'Face recognition is off. Every face and person was deleted.';

export class FacesSwitch {
	settings: MlSettings | null = $state.raw(null);
	/** What the switch shows. */
	checked = $state(false);
	busy = $state(false);
	error: string | null = $state(null);
	notice: string | null = $state(null);
	/** The confirmation is open. */
	confirming = $state(false);
	/**
	 * The counts the confirmation states: fetched when it opens (null until then), and kept while
	 * it closes so its text never flickers.
	 */
	confirmCounts: FaceCounts | null = $state.raw(null);
	/** Counts the questions asked: each is a new dialog, never the previous one still closing. */
	asks = $state(0);

	readonly #api: FacesApi;
	#loads = 0;

	constructor(api: FacesApi = { get: getMlSettings, set: setFacesEnabled }) {
		this.#api = api;
	}

	/** (Re)reads the setting and its counts; an older answer never overwrites a newer one. */
	async load(): Promise<void> {
		const load = ++this.#loads;
		const result = await this.#api.get();
		if (load !== this.#loads) return;
		if (!result.ok) {
			this.error = result.error;
			return;
		}
		this.settings = result.data;
		if (!this.busy && !this.confirming) this.checked = result.data.faces_enabled;
	}

	/** The switch was flipped to `next` (it is shown unchanged until the server agrees). */
	async toggle(next: boolean): Promise<void> {
		this.checked = !next;
		this.notice = null;
		if (this.busy) return;
		if (!next) {
			await this.#askToTurnOff();
			return;
		}
		this.busy = true;
		const result = await this.#api.set(true);
		this.busy = false;
		if (!result.ok) {
			this.error = result.error;
			return;
		}
		this.#loads += 1;
		this.settings = result.data;
		this.checked = true;
		this.error = null;
		this.notice = ON_NOTICE;
	}

	async #askToTurnOff(): Promise<void> {
		this.confirmCounts = null;
		this.asks += 1;
		this.confirming = true;
		const load = ++this.#loads;
		const result = await this.#api.get();
		if (load !== this.#loads || !this.confirming) return;
		if (!result.ok) {
			this.confirming = false;
			this.error = result.error;
			return;
		}
		this.settings = result.data;
		this.confirmCounts = countsOf(result.data);
	}

	/** Confirmed: resolves an error to show in the dialog, or null once everything is deleted. */
	async turnOff(): Promise<string | null> {
		if (this.busy) return null;
		this.busy = true;
		const result = await this.#api.set(false);
		this.busy = false;
		if (!result.ok) return result.error;
		this.#loads += 1;
		this.settings = result.data;
		this.checked = false;
		this.error = null;
		this.notice = OFF_NOTICE;
		return null;
	}

	/** The confirmation closed (confirmed or not). */
	closed(): void {
		this.confirming = false;
	}
}
