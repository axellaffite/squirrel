import { describe, expect, it } from 'vitest';
import { chunkRange, missingChunks } from '#lib/upload/plan.ts';

describe('upload plan', () => {
	it('lists chunks not yet received', () => {
		expect(missingChunks(4, [1, 3])).toEqual([0, 2]);
		expect(missingChunks(0, [])).toEqual([]);
	});

	it('computes byte ranges with a short last chunk', () => {
		expect(chunkRange(0, 4, 10)).toEqual([0, 4]);
		expect(chunkRange(2, 4, 10)).toEqual([8, 10]);
	});
});
