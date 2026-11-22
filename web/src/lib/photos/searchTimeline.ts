/**
 * Search results as a timeline: a short list held whole in memory (at most 200 photos), so every
 * month is laid out for real at once, with the Photos page's month and day sections.
 */
import { monthKey } from '#lib/photos/dates.ts';
import { layoutMonth, type Metrics, type MonthBox, type MonthLayout, type Placeable } from '#lib/photos/model.ts';

export interface MonthGroup<A> {
	/** `YYYY-MM`. */
	month: string;
	items: A[];
}

export interface TimelineLayout<A> {
	boxes: MonthBox[];
	layouts: ReadonlyMap<string, MonthLayout<A>>;
	height: number;
}

/**
 * Newest capture first, equal times by id descending: the server's order (`taken_at DESC,
 * node_id DESC`), kept as is when already sorted. Capture times are wall-clock ISO strings of
 * one shape, so comparing them as strings orders them.
 */
export function newestFirst<A extends Placeable>(items: readonly A[]): A[] {
	return [...items].sort((a, b) => {
		if (a.taken_at !== b.taken_at) return a.taken_at < b.taken_at ? 1 : -1;
		if (a.id === b.id) return 0;
		return a.id < b.id ? 1 : -1;
	});
}

/** Consecutive photos of the same month, in the given order (newest first). */
export function groupByMonth<A extends Placeable>(items: readonly A[]): MonthGroup<A>[] {
	const groups: MonthGroup<A>[] = [];
	for (const item of items) {
		const month = monthKey(item.taken_at);
		const last = groups[groups.length - 1];
		if (last && last.month === month) last.items.push(item);
		else groups.push({ month, items: [item] });
	}
	return groups;
}

/** Every month laid out (heading, day headings, justified rows) and stacked newest first. */
export function layoutTimeline<A extends Placeable>(
	groups: readonly MonthGroup<A>[],
	width: number,
	m: Metrics,
	locale?: string
): TimelineLayout<A> {
	const layouts = new Map<string, MonthLayout<A>>();
	const boxes: MonthBox[] = [];
	let top = 0;
	for (const group of groups) {
		const layout = layoutMonth(group.items, width, m, locale);
		layouts.set(group.month, layout);
		boxes.push({ month: group.month, count: group.items.length, top, height: layout.height, estimated: false });
		top += layout.height + m.monthGap;
	}
	return { boxes, layouts, height: Math.max(0, top - m.monthGap) };
}
