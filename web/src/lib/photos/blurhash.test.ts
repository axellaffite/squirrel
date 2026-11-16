
import { describe, expect, it } from 'vitest';
import { averageColor, decodeBlurhash, isBlurhash } from '#lib/photos/blurhash.ts';

const HASH = 'LEHV6nWB2yk8pyo0adR*.7kCMdnj';

describe('blurhash', () => {
	it('validates length and alphabet', () => {
		expect(isBlurhash(HASH)).toBe(true);
		expect(isBlurhash('LEHV6nWB2yk8')).toBe(false);
		expect(isBlurhash('L!!!!!!!!!!!!!!!!!!!!!!!!!!')).toBe(false);
		expect(isBlurhash(null)).toBe(false);
	});

	it('gives the average colour without decoding', () => {
		expect(averageColor(HASH)).toBe('rgb(151 150 149)');
	});

	it('decodes to opaque pixels whose mean is the average colour', () => {
		const px = decodeBlurhash(HASH, 32, 32);
		expect(px.length).toBe(32 * 32 * 4);
		const sum = [0, 0, 0];
		for (let i = 0; i < px.length; i += 4) for (let c = 0; c < 3; c++) sum[c] += px[i + c];
		expect(sum.map((s) => Math.round(s / 1024))).toEqual([150, 150, 148]);
		expect(px[3]).toBe(255);
	});
});
