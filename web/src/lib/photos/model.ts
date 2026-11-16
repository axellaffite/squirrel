/**
 * The timeline's geometry: every month is a box, estimated from its photo count until its items
 * load, then laid out for real (day headers + justified rows). Pure functions, so the component
 * only wires them to the DOM.
 */
import { dayKey, dayLabel } from '#lib/photos/dates.ts';
import { stack, type Block, type Section } from '#lib/photos/justified.ts';

/** Pixel sizes read from the tokens (`--photo-row`, `--space-1`, …). */
export interface Metrics {
	rowHeight: number;
	gap: number;
	monthHead: number;
	dayHead: number;
	dayGap: number;
	monthGap: number;
}

export interface MonthCount {
	month: string;
	count: number;
}

export interface MonthBox {
	month: string;
	count: number;
	top: number;
	height: number;
	/** True until the month's items are laid out. */
	estimated: boolean;
}

/** What the layout needs from a photo. */
export interface Placeable {
	id: string;
	taken_at: string;
	width?: number | null;
	height?: number | null;
}

export interface Cell<A> {
	asset: A;
	aspect: number;
}

export interface MonthLayout<A> {
	/** Blocks relative to the month's top (after its heading). */
	blocks: Block<Cell<A>>[];
	/** Heading included. */
	height: number;
}

/** Photos are 4:3 or 3:2 on average, so a row holds about 1.4 row-heights of width per photo. */
const AVERAGE_ASPECT = 1.4;

export function estimateMonthHeight(count: number, width: number, m: Metrics): number {
	const rows = width > 0 ? Math.max(1, Math.ceil((count * AVERAGE_ASPECT * m.rowHeight) / width)) : 1;
	return m.monthHead + rows * (m.rowHeight + m.gap);
}

function restack(boxes: readonly Omit<MonthBox, 'top'>[], monthGap: number): { boxes: MonthBox[]; height: number } {
	let top = 0;
	const placed = boxes.map((box) => {
		const next = { ...box, top };
		top += box.height + monthGap;
		return next;
	});
	return { boxes: placed, height: Math.max(0, top - monthGap) };
}

/** Every month with its estimated height, stacked newest first. */
export function monthPlaceholders(
	months: readonly MonthCount[],
	width: number,
	m: Metrics
): { boxes: MonthBox[]; height: number } {
	const estimated = months.map((entry) => ({
		month: entry.month,
		count: entry.count,
		height: estimateMonthHeight(entry.count, width, m),
		estimated: true
	}));
	return restack(estimated, m.monthGap);
}

/**
 * Puts a month's real height in place of its estimate. `delta` is what to add to the scroll
 * position so the view does not jump: non-zero only when the month ends above `viewTop`.
 */
export function replaceEstimate(
	boxes: readonly MonthBox[],
	month: string,
	realHeight: number,
	viewTop: number
): { boxes: readonly MonthBox[]; height: number; delta: number } {
	const index = boxes.findIndex((box) => box.month === month);
	const last = boxes[boxes.length - 1];
	const height = last ? last.top + last.height : 0;
	if (index < 0) return { boxes, height, delta: 0 };
	const old = boxes[index];
	const change = realHeight - old.height;
	const next = boxes.map((box, i) => {
		if (i === index) return { ...box, height: realHeight, estimated: false };
		return i > index ? { ...box, top: box.top + change } : box;
	});
	const above = old.top + old.height <= viewTop;
	return { boxes: next, height: height + change, delta: above ? change : 0 };
}

const aspectOf = (asset: Placeable) =>
	asset.width && asset.height && asset.width > 0 && asset.height > 0 ? asset.width / asset.height : 1;

/** A month's items (newest first) as day sections of justified rows, under the month heading. */
export function layoutMonth<A extends Placeable>(
	items: readonly A[],
	width: number,
	m: Metrics,
	locale?: string
): MonthLayout<A> {
	const sections: Section<Cell<A>>[] = [];
	for (const asset of items) {
		const key = dayKey(asset.taken_at);
		let section = sections[sections.length - 1];
		if (!section || section.key !== key) {
			section = { key, label: dayLabel(key, locale), items: [] };
			sections.push(section);
		}
		section.items.push({ asset, aspect: aspectOf(asset) });
	}
	const { blocks, height } = stack(sections, {
		width,
		targetHeight: m.rowHeight,
		gap: m.gap,
		headerHeight: m.dayHead,
		sectionGap: m.dayGap
	});
	return {
		blocks: blocks.map((block) => ({ ...block, top: block.top + m.monthHead })),
		height: m.monthHead + height
	};
}

/** Index of the month at `y` (the last box starting at or above it), -1 when there is none. */
export function monthAt(boxes: readonly MonthBox[], y: number): number {
	if (boxes.length === 0) return -1;
	let lo = 0;
	let hi = boxes.length - 1;
	while (lo < hi) {
		const mid = (lo + hi + 1) >> 1;
		if (boxes[mid].top <= y) lo = mid;
		else hi = mid - 1;
	}
	return lo;
}

/**
 * Months to request now: those within one viewport of [viewTop, viewBottom], not loading and not
 * `settled` (loaded, or failed until the next reload), nearest to the view first, keeping at most
 * `maxInFlight` requests running.
 */
export function monthsToLoad(
	boxes: readonly MonthBox[],
	viewTop: number,
	viewBottom: number,
	loading: ReadonlySet<string>,
	settled: ReadonlySet<string>,
	maxInFlight = 2
): string[] {
	const free = maxInFlight - loading.size;
	if (free <= 0) return [];
	const margin = viewBottom - viewTop;
	const from = viewTop - margin;
	const to = viewBottom + margin;
	const distance = (box: MonthBox) =>
		box.top + box.height < viewTop ? viewTop - (box.top + box.height) : Math.max(0, box.top - viewBottom);
	return boxes
		.filter((box) => box.top <= to && box.top + box.height >= from)
		.filter((box) => !loading.has(box.month) && !settled.has(box.month))
		.sort((a, b) => distance(a) - distance(b))
		.slice(0, free)
		.map((box) => box.month);
}

/** The scroll position showing the same point of the same month after a relayout. */
export function reanchor(before: readonly MonthBox[], after: readonly MonthBox[], viewTop: number): number {
	const index = monthAt(before, viewTop);
	if (index < 0) return viewTop;
	const old = before[index];
	const now = after.find((box) => box.month === old.month);
	if (!now) return viewTop;
	const fraction = old.height > 0 ? Math.min(1, Math.max(0, (viewTop - old.top) / old.height)) : 0;
	return now.top + fraction * now.height;
}

/** A length token (`7rem`, `4px`) in pixels; anything else is 0. */
export function cssLength(value: string, rootFontPx: number): number {
	const match = /^\s*(-?[\d.]+)(px|rem)\s*$/.exec(value);
	if (!match) return 0;
	const amount = Number(match[1]);
	return match[2] === 'rem' ? amount * rootFontPx : amount;
}
