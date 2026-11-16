import { describe, expect, it, vi } from 'vitest';
import { TimelineData } from '#lib/photos/timeline.svelte.ts';

const photo = (id: string) => ({ id }) as never;

function source(months = [{ month: '2024-07', count: 2 }, { month: '2024-06', count: 1 }]) {
	return {
		listMonths: vi.fn(async () => ({ ok: true as const, data: months })),
		listMonth: vi.fn(async (month: string) => ({
			ok: true as const,
			data: month === '2024-07' ? [photo('a'), photo('b')] : [photo('c')]
		}))
	};
}

describe('TimelineData', () => {
	it('loads months, then items in timeline order', async () => {
		const data = new TimelineData(source());
		await data.reload();
		expect(data.total).toBe(3);
		await data.loadMonth('2024-06');
		await data.loadMonth('2024-07');
		expect(data.ordered.map((p) => (p as { id: string }).id)).toEqual(['a', 'b', 'c']);
		expect([...data.loaded].sort()).toEqual(['2024-06', '2024-07']);
		expect(data.loading.size).toBe(0);
	});

	it('refetches months after a reload and drops vanished ones', async () => {
		const src = source();
		const data = new TimelineData(src);
		await data.reload();
		await data.loadMonth('2024-06');
		src.listMonths.mockResolvedValueOnce({ ok: true, data: [{ month: '2024-07', count: 2 }] });
		await data.reload();
		expect(data.loaded.size).toBe(0);
		expect(data.items).toEqual({});
	});

	it('ignores a month answer that a reload overtook', async () => {
		const src = source();
		const data = new TimelineData(src);
		await data.reload();
		const pending = data.loadMonth('2024-07');
		await data.reload();
		expect(await pending).toBeNull();
		expect(data.loaded.size).toBe(0);
	});

	it('reports errors', async () => {
		const data = new TimelineData({
			listMonths: async () => ({ ok: false, error: 'down', status: 503 }),
			listMonth: async () => ({ ok: false, error: 'down', status: 503 })
		});
		await data.reload();
		expect(data.error).toBe('down');
		expect(data.months).toBeNull();
		expect(await data.loadMonth('2024-07')).toBeNull();
	});
});
