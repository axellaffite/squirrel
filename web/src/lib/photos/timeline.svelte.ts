/** The timeline's data: month counts, and the items of the months loaded so far. */
import { listMonth, listMonths, type AssetSummary, type MonthCount, type Outcome } from '#lib/photos/api.ts';

export interface TimelineSource {
	listMonths: () => Promise<Outcome<MonthCount[]>>;
	listMonth: (month: string) => Promise<Outcome<AssetSummary[]>>;
}

/** What became of one month request. */
export type MonthLoad =
	| { status: 'loaded'; items: readonly AssetSummary[] }
	/** A reload overtook it (or it was already loading): its answer was dropped. */
	| { status: 'stale' }
	/** It failed; the month is not asked again until the next `reload`. */
	| { status: 'failed' };

export class TimelineData {
	/** Null until the first answer. */
	months: MonthCount[] | null = $state.raw(null);
	error: string | null = $state(null);
	/** Items per loaded month, newest first. Replaced, never mutated. */
	items: Readonly<Record<string, readonly AssetSummary[]>> = $state.raw({});

	readonly #source: TimelineSource;
	#loading = new Set<string>();
	#loaded = new Set<string>();
	#failed = new Set<string>();
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

	/** Months not to request now: loaded, or failed since the last reload. */
	get settled(): ReadonlySet<string> {
		return new Set([...this.#loaded, ...this.#failed]);
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
		this.#failed = new Set();
		const present = new Set(result.data.map((entry) => entry.month));
		this.items = Object.fromEntries(Object.entries(this.items).filter(([month]) => present.has(month)));
		this.months = result.data;
	}

	/** Fetches one month. */
	async loadMonth(month: string): Promise<MonthLoad> {
		if (this.#loading.has(month)) return { status: 'stale' };
		const generation = this.#generation;
		this.#loading.add(month);
		try {
			const result = await this.#source.listMonth(month);
			if (generation !== this.#generation) return { status: 'stale' };
			if (!result.ok) {
				this.error = result.error;
				this.#failed.add(month);
				return { status: 'failed' };
			}
			this.#loaded.add(month);
			this.items = { ...this.items, [month]: result.data };
			return { status: 'loaded', items: result.data };
		} finally {
			this.#loading.delete(month);
		}
	}

	/** Takes photos out at once (e.g. moved to the trash), counts included; empty months go. */
	remove(ids: Iterable<string>): void {
		const gone = new Set(ids);
		if (gone.size === 0 || !this.months) return;
		const removed = new Map<string, number>();
		const items: Record<string, readonly AssetSummary[]> = {};
		for (const [month, list] of Object.entries(this.items)) {
			const kept = list.filter((item) => !gone.has(item.id));
			items[month] = kept;
			if (kept.length !== list.length) removed.set(month, list.length - kept.length);
		}
		if (removed.size === 0) return;
		this.items = items;
		this.months = this.months
			.map((entry) => ({ ...entry, count: entry.count - (removed.get(entry.month) ?? 0) }))
			.filter((entry) => entry.count > 0);
	}
}
