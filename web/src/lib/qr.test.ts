import { describe, expect, it } from 'vitest';
import { QUIET_ZONE, qrShape } from '#lib/qr.ts';

const ADDRESS = 'https://home.tail1234.ts.net:8443';

describe('qrShape', () => {
	it('draws a square with the quiet zone scanners need around it', () => {
		const shape = qrShape(ADDRESS);
		// Version 3 (29 modules) holds 33 bytes at the medium correction level.
		expect(shape.size).toBe(29 + 2 * QUIET_ZONE);
		// The first dark run is the top edge of the top-left finder pattern: 7 modules.
		expect(shape.path.startsWith(`M${QUIET_ZONE} ${QUIET_ZONE}h7v1h-7z`)).toBe(true);
	});

	it('leaves the quiet zone empty', () => {
		const { path, size } = qrShape(ADDRESS);
		const runs = [...path.matchAll(/M(\d+) (\d+)h(\d+)/g)].map(([, x, y, length]) => ({
			x: Number(x),
			y: Number(y),
			end: Number(x) + Number(length)
		}));
		expect(runs.length).toBeGreaterThan(0);
		for (const run of runs) {
			expect(run.x).toBeGreaterThanOrEqual(QUIET_ZONE);
			expect(run.y).toBeGreaterThanOrEqual(QUIET_ZONE);
			expect(run.end).toBeLessThanOrEqual(size - QUIET_ZONE);
			expect(run.y).toBeLessThan(size - QUIET_ZONE);
		}
	});

	it('is the same for the same text and grows with it', () => {
		expect(qrShape(ADDRESS)).toEqual(qrShape(ADDRESS));
		expect(qrShape(`${ADDRESS}/${'x'.repeat(200)}`).size).toBeGreaterThan(qrShape(ADDRESS).size);
	});
});
