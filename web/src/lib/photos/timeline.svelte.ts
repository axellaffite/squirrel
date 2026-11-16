/** The timeline's data: month counts, and the items of the months loaded so far. */
import { listMonth, listMonths, type AssetSummary, type MonthCount, type Outcome } from '#lib/photos/api.ts';

export interface TimelineSource {
	listMonths: () => Promise<Outcome<MonthCount[]>>;
	listMonth: (month: string) => Promise<Outcome<AssetSummary[]>>;
}

export class TimelineData {
	/** Null until the first answer. */
	months: MonthCount[] | null = $state.raw(null);
	error: string | null = $state(null);
	/** Items per loaded month, newest first. Replaced, never mutated. */
	items: Readonly<Record<string, readonly AssetSummary[]>> = $state.raw({});

	readonly #source: TimelineSource;
	#loading = new Set<string>();
	#loaded = new Set<string>();
	#generation = 0;

	constructor(source: TimelineSource = { listMonths, listMonth }) {
		this.#source = source;
	}

	get loading(): ReadonlySet<string> {
		return this.#loading;
	}

	/** Months whose current items arrived (cleared by `reload`, so they are fetched again). */
	get loaded(): ReadonlySet<string> {
		return this.#loaded;
	}

	get total(): number | null {
		return this.months?.reduce((sum, entry) => sum + entry.count, 0) ?? null;
	}

	/** Every loaded photo in timeline order (newest first). */
	get ordered(): AssetSummary[] {
		return (this.months ?? []).flatMap((entry) => this.items[entry.month] ?? []);
	}

	/** (Re)loads the month list. Loaded items stay shown until their month is fetched again. */
	async reload(): Promise<void> {
		const generation = ++this.#generation;
		const result = await this.#source.listMonths();
		if (generation !== this.#generation) return;
		if (!result.ok) {
			this.error = result.error;
			return;
		}
		this.error = null;
		this.#loaded = new Set();
		const present = new Set(result.data.map((entry) => entry.month));
		this.items = Object.fromEntries(Object.entries(this.items).filter(([month]) => present.has(month)));
		this.months = result.data;
	}

	/** Fetches one month; resolves its items, or null if it failed or a reload overtook it. */
	async loadMonth(month: string): Promise<readonly AssetSummary[] | null> {
		if (this.#loading.has(month)) return null;
		const generation = this.#generation;
		this.#loading.add(month);
		try {
			const result = await this.#source.listMonth(month);
			if (generation !== this.#generation) return null;
			if (!result.ok) {
				this.error = result.error;
				return null;
			}
			this.#loaded.add(month);
			this.items = { ...this.items, [month]: result.data };
			return result.data;
		} finally {
			this.#loading.delete(month);
		}
	}
}
