/** The scrubber's geometry: where each month sits along the column, as a fraction of its height. */

export interface MonthMark {
	month: string;
	top: number;
}

/** Year labels closer than this (px) to the previous one are skipped. */
export const MIN_LABEL_GAP = 16;

/** Each month's position along the column (its section's top over the total height), 0–1. */
export function positions(marks: readonly MonthMark[], total: number): number[] {
	return marks.map((mark) => (total > 0 ? Math.min(1, Math.max(0, mark.top / total)) : 0));
}

/** The month whose tick is nearest to `fraction` (0 top, 1 bottom); -1 when there are none. */
export function monthAtFraction(fraction: number, at: readonly number[]): number {
	if (at.length === 0) return -1;
	let lo = 0;
	let hi = at.length - 1;
	while (lo < hi) {
		const mid = (lo + hi) >> 1;
		if (at[mid] < fraction) lo = mid + 1;
		else hi = mid;
	}
	if (lo > 0 && fraction - at[lo - 1] <= at[lo] - fraction) return lo - 1;
	return lo;
}

export interface YearLabel {
	year: string;
	fraction: number;
}

/** One label per year at its newest month, skipping labels that would crowd the previous one. */
export function yearLabels(
	marks: readonly MonthMark[],
	at: readonly number[],
	heightPx: number,
	minGap: number = MIN_LABEL_GAP
): YearLabel[] {
	const labels: YearLabel[] = [];
	let lastYear = '';
	let lastY = -Infinity;
	marks.forEach((mark, index) => {
		const year = mark.month.slice(0, 4);
		if (year === lastYear) return;
		lastYear = year;
		const y = at[index] * heightPx;
		if (y - lastY < minGap) return;
		labels.push({ year, fraction: at[index] });
		lastY = y;
	});
	return labels;
}

const monthNumber = (month: string) => Number(month.slice(0, 4)) * 12 + Number(month.slice(5, 7)) - 1;

/** The month index nearest to `target` (months newest first). */
function nearestTo(months: readonly string[], target: number): number {
	let best = 0;
	months.forEach((month, index) => {
		if (Math.abs(monthNumber(month) - target) < Math.abs(monthNumber(months[best]) - target)) best = index;
	});
	return best;
}

/**
 * The slider's keys (months newest first, so "up" is newer): Up/Down one month, PageUp/PageDown
 * a year, Home/End newest/oldest. Null for other keys.
 */
export function stepMonth(index: number, key: string, months: readonly string[]): number | null {
	const last = months.length - 1;
	if (last < 0) return null;
	const current = Math.min(Math.max(index, 0), last);
	switch (key) {
		case 'ArrowUp':
			return Math.max(0, current - 1);
		case 'ArrowDown':
			return Math.min(last, current + 1);
		case 'Home':
			return 0;
		case 'End':
			return last;
		case 'PageUp':
		case 'PageDown': {
			const target = monthNumber(months[current]) + (key === 'PageUp' ? 12 : -12);
			const found = nearestTo(months, target);
			if (found !== current) return found;
			return key === 'PageUp' ? Math.max(0, current - 1) : Math.min(last, current + 1);
		}
		default:
			return null;
	}
}

/** A month jumped to with the scrubber, and where that left the page. */
export interface Jump {
	month: string;
	scrollY: number;
}

/** A jump holds until the page scrolls elsewhere (the user, or a layout change moving it). */
export const jumpHolds = (jump: Jump | null, scrollY: number): jump is Jump =>
	jump !== null && Math.abs(scrollY - jump.scrollY) <= 1;

/**
 * The scrubber's current month: the month jumped to while it still holds (near the end a short
 * page cannot bring it to the top), else the month at the top of the view.
 */
export function currentMonthIndex(
	boxes: readonly { month: string; top: number }[],
	viewTop: number,
	jumped: string | null
): number {
	if (jumped !== null) {
		const index = boxes.findIndex((box) => box.month === jumped);
		if (index >= 0) return index;
	}
	if (boxes.length === 0) return -1;
	let current = 0;
	for (let i = 0; i < boxes.length && boxes[i].top <= Math.max(0, viewTop); i++) current = i;
	return current;
}
