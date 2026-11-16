import { describe, expect, it } from 'vitest';
import { mapLimit } from '#lib/photos/batch.ts';

describe('mapLimit', () => {
	it('keeps order and never runs more than the limit at once', async () => {
		let running = 0;
		let peak = 0;
		const results = await mapLimit([5, 1, 4, 2, 3, 0], 4, async (n) => {
			running++;
			peak = Math.max(peak, running);
			await new Promise((resolve) => setTimeout(resolve, n));
			running--;
			return n * 10;
		});
		expect(results).toEqual([50, 10, 40, 20, 30, 0]);
		expect(peak).toBe(4);
	});

	it('handles nothing', async () => {
		expect(await mapLimit([], 4, async (n: number) => n)).toEqual([]);
	});
});
