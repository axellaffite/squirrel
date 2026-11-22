import { describe, expect, it } from 'vitest';
import { layoutMonth, type Metrics } from '#lib/photos/model.ts';
import { groupByMonth, layoutTimeline, newestFirst } from '#lib/photos/searchTimeline.ts';

const M: Metrics = { rowHeight: 100, gap: 4, monthHead: 50, dayHead: 30, dayGap: 10, monthGap: 20 };

const asset = (id: string, takenAt: string) => ({ id, taken_at: takenAt, width: 300, height: 200 });

const ids = (items: readonly { id: string }[]) => items.map((item) => item.id);

describe('newestFirst', () => {
	it('orders by capture time, then id, both descending, like the server', () => {
		const items = [
			asset('a', '2023-01-15T12:00:00'),
			asset('b', '2024-07-01T10:00:00'),
			asset('d', '2024-07-01T10:00:00'),
			asset('c', '2026-11-20T21:00:00'),
			asset('e', '2024-07-01T09:59:59')
		];
		expect(ids(newestFirst(items))).toEqual(['c', 'd', 'b', 'e', 'a']);
	});

	it('keeps an already sorted list as it is, without changing the input', () => {
		const items = [asset('c', '2025-02-01T00:00:00'), asset('b', '2025-01-31T23:59:00'), asset('a', '2024-12-31T00:00:00')];
		const copy = [...items];
		expect(newestFirst(items)).toEqual(items);
		expect(items).toEqual(copy);
	});
});

describe('groupByMonth', () => {
	it('puts consecutive photos of a month together, newest month first', () => {
		const groups = groupByMonth([
			asset('a', '2025-02-01T08:00:00'),
			asset('b', '2025-01-31T23:59:00'),
			asset('c', '2025-01-02T10:00:00'),
			asset('d', '2019-03-02T08:00:00')
		]);
		expect(groups.map((g) => [g.month, ids(g.items)])).toEqual([
			['2025-02', ['a']],
			['2025-01', ['b', 'c']],
			['2019-03', ['d']]
		]);
	});

	it('gives no month for no results', () => {
		expect(groupByMonth([])).toEqual([]);
	});
});

describe('layoutTimeline', () => {
	it('lays out every month with its day headings and stacks them', () => {
		const groups = groupByMonth([
			asset('a', '2025-02-01T08:00:00'),
			asset('b', '2025-01-31T23:59:00'),
			asset('c', '2025-01-02T10:00:00')
		]);
		const { boxes, layouts, height } = layoutTimeline(groups, 1000, M, 'en-GB');
		const feb = layoutMonth(groups[0].items, 1000, M, 'en-GB');
		const jan = layoutMonth(groups[1].items, 1000, M, 'en-GB');
		expect(boxes.map((b) => [b.month, b.count, b.top, b.height, b.estimated])).toEqual([
			['2025-02', 1, 0, feb.height, false],
			['2025-01', 2, feb.height + M.monthGap, jan.height, false]
		]);
		expect(height).toBe(feb.height + M.monthGap + jan.height);
		const headers = layouts.get('2025-01')?.blocks.filter((block) => block.kind === 'header');
		expect(headers?.map((block) => block.label)).toEqual(['Fri 31 Jan 2025', 'Thu 2 Jan 2025']);
	});

	it('is empty for no results', () => {
		expect(layoutTimeline([], 1000, M)).toEqual({ boxes: [], layouts: new Map(), height: 0 });
	});
});
