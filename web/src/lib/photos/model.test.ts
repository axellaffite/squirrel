import { describe, expect, it } from 'vitest';
import {
	cssLength,
	estimateMonthHeight,
	layoutMonth,
	monthAt,
	monthPlaceholders,
	monthsToLoad,
	reanchor,
	replaceEstimate,
	type Metrics
} from '#lib/photos/model.ts';

const M: Metrics = { rowHeight: 100, gap: 4, monthHead: 50, dayHead: 30, dayGap: 10, monthGap: 20 };
const months = [
	{ month: '2024-07', count: 10 },
	{ month: '2024-06', count: 1 },
	{ month: '2023-12', count: 40 }
];

const asset = (id: string, takenAt: string, width = 300, height = 200) => ({ id, taken_at: takenAt, width, height });

describe('monthPlaceholders', () => {
	it('estimates each month from its count and stacks them', () => {
		// heading + ceil(count × 1.4 × row / width) × (row + gap)
		expect(estimateMonthHeight(10, 1000, M)).toBe(50 + 2 * 104);
		expect(estimateMonthHeight(1, 1000, M)).toBe(50 + 104);
		const { boxes, height } = monthPlaceholders(months, 1000, M);
		expect(boxes.map((b) => [b.month, b.top, b.height, b.estimated])).toEqual([
			['2024-07', 0, 258, true],
			['2024-06', 278, 154, true],
			['2023-12', 452, 50 + 6 * 104, true]
		]);
		expect(height).toBe(452 + 674);
	});

	it('gives nothing for an empty library or no width', () => {
		expect(monthPlaceholders([], 1000, M)).toEqual({ boxes: [], height: 0 });
		expect(monthPlaceholders(months, 0, M).boxes[0].height).toBe(50 + 104);
	});
});

describe('layoutMonth', () => {
	it('groups by day under the month heading, newest day first as given', () => {
		const items = [asset('a', '2024-07-14T18:00:00'), asset('b', '2024-07-14T09:00:00'), asset('c', '2024-07-02T12:00:00', 200, 300)];
		const layout = layoutMonth(items, 1000, M, 'en-GB');
		const headers = layout.blocks.filter((b) => b.kind === 'header');
		expect(headers.map((h) => h.label)).toEqual(['Sun 14 Jul 2024', 'Tue 2 Jul 2024']);
		expect(layout.blocks[0].top).toBe(50);
		const rows = layout.blocks.filter((b) => b.kind === 'row');
		expect(rows[0].kind === 'row' && rows[0].items.map((i) => i.item.asset.id)).toEqual(['a', 'b']);
		expect(layout.height).toBe(50 + 30 + 100 + 10 + 30 + 100);
	});

	it('keeps the given order without day headings when asked', () => {
		const items = [asset('a', '2024-07-02T12:00:00'), asset('b', '2024-07-14T09:00:00'), asset('c', '2023-01-01T00:00:00')];
		const layout = layoutMonth(items, 1000, M, 'en-GB', false);
		expect(layout.blocks.every((b) => b.kind === 'row')).toBe(true);
		const ids = layout.blocks.flatMap((b) => (b.kind === 'row' ? b.items.map((i) => i.item.asset.id) : []));
		expect(ids).toEqual(['a', 'b', 'c']);
		expect(layout.blocks[0].top).toBe(50);
	});

	it('treats an unknown size as square', () => {
		const layout = layoutMonth([{ id: 'x', taken_at: '2024-07-01T00:00:00', width: null, height: null }], 1000, M);
		const row = layout.blocks[1];
		expect(row.kind === 'row' && row.items[0].width).toBe(100);
	});
});

describe('replaceEstimate', () => {
	const { boxes } = monthPlaceholders(months, 1000, M);

	it('shifts later months and compensates scroll when the month is above the viewport', () => {
		const result = replaceEstimate(boxes, '2024-07', 300, 1000);
		expect(result.boxes[0]).toMatchObject({ height: 300, estimated: false });
		expect(result.boxes[1].top).toBe(320);
		expect(result.height).toBe(1126 + 42);
		expect(result.delta).toBe(42);
	});

	it('does not compensate when the month is in or below the viewport', () => {
		expect(replaceEstimate(boxes, '2024-07', 300, 100).delta).toBe(0);
		expect(replaceEstimate(boxes, '2023-12', 300, 100).delta).toBe(0);
	});

	it('ignores an unknown month', () => {
		const result = replaceEstimate(boxes, '1999-01', 300, 0);
		expect(result.boxes).toBe(boxes);
		expect(result.delta).toBe(0);
	});
});

describe('the load window', () => {
	const { boxes } = monthPlaceholders(months, 1000, M);
	const none = new Set<string>();

	it('loads months within one viewport of the view, nearest first', () => {
		expect(monthsToLoad(boxes, 0, 200, none, none)).toEqual(['2024-07', '2024-06']);
		expect(monthsToLoad(boxes, 600, 800, none, none)).toEqual(['2023-12', '2024-06']);
	});

	it('keeps at most two requests in flight', () => {
		expect(monthsToLoad(boxes, 0, 2000, new Set(['2024-07']), none)).toEqual(['2024-06']);
		expect(monthsToLoad(boxes, 0, 2000, new Set(['2024-07', '2024-06']), none)).toEqual([]);
	});

	it('skips settled months (loaded, or failed until a reload)', () => {
		expect(monthsToLoad(boxes, 0, 200, none, new Set(['2024-07']))).toEqual(['2024-06']);
	});
});

describe('monthAt and reanchor', () => {
	const { boxes } = monthPlaceholders(months, 1000, M);

	it('finds the month at a position', () => {
		expect(monthAt(boxes, -5)).toBe(0);
		expect(monthAt(boxes, 270)).toBe(0);
		expect(monthAt(boxes, 278)).toBe(1);
		expect(monthAt(boxes, 99999)).toBe(2);
		expect(monthAt([], 10)).toBe(-1);
	});

	it('keeps the same point of the same month in view after a relayout', () => {
		const wider = monthPlaceholders(months, 2000, M).boxes;
		// 452 + half of 674 in the old layout → half of the same month in the new one.
		const anchored = reanchor(boxes, wider, 452 + 337);
		expect(anchored).toBe(wider[2].top + wider[2].height / 2);
		expect(reanchor([], wider, 50)).toBe(50);
	});
});

describe('cssLength', () => {
	it('reads px and rem tokens', () => {
		expect(cssLength('7rem', 16)).toBe(112);
		expect(cssLength(' 4px ', 16)).toBe(4);
		expect(cssLength('0.25rem', 20)).toBe(5);
		expect(cssLength('', 16)).toBe(0);
		expect(cssLength('auto', 16)).toBe(0);
	});
});
