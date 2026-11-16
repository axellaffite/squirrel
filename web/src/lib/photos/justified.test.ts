import { describe, expect, it } from 'vitest';
import { justify, stack, visible, type Block } from '#lib/photos/justified.ts';

const OPTIONS = { width: 1000, targetHeight: 200, gap: 4 };
const landscape = { aspect: 3 / 2 };
const portrait = { aspect: 3 / 4 };

describe('justify', () => {
	it('fills full rows to the width and keeps the last row at the target height', () => {
		const { rows, height } = justify([landscape, landscape, landscape, landscape, portrait], OPTIONS);
		expect(rows).toHaveLength(2);
		expect(rows[0]).toMatchObject({ top: 0, height: 165 });
		expect(rows[0].tiles.map((t) => t.width)).toEqual([247, 247, 247, 247]);
		const last = rows[0].tiles[3];
		expect(last.left + last.width).toBe(1000);
		expect(rows[1]).toMatchObject({ top: 169, height: 200 });
		expect(rows[1].tiles[0]).toMatchObject({ left: 0, width: 150 });
		expect(height).toBe(369);
	});

	it('never makes a full row taller than the target', () => {
		const tiles = Array.from({ length: 30 }, (_, i) => ({ aspect: 0.6 + (i % 5) * 0.4 }));
		const { rows } = justify(tiles, OPTIONS);
		for (const row of rows.slice(0, -1)) {
			expect(row.height).toBeLessThanOrEqual(200);
			const end = row.tiles[row.tiles.length - 1];
			expect(end.left + end.width).toBe(1000);
		}
	});

	it('gives nothing for no tiles', () => {
		expect(justify([], OPTIONS)).toEqual({ rows: [], height: 0 });
	});

	it('clamps panoramas to aspect 3', () => {
		const { rows } = justify([{ aspect: 20 }], OPTIONS);
		expect(rows[0].tiles[0].width).toBe(600);
	});

	it('treats unknown aspects as square', () => {
		const { rows } = justify([{ aspect: Number.NaN }], OPTIONS);
		expect(rows[0].tiles[0].width).toBe(200);
	});
});

describe('stack and visible', () => {
	const sections = [
		{ key: 'a', label: 'A', items: [landscape, landscape, landscape, landscape, portrait] },
		{ key: 'b', label: 'B', items: [landscape] }
	];
	const layout = stack(sections, { ...OPTIONS, headerHeight: 30, sectionGap: 10 });

	it('stacks blocks without overlap', () => {
		const { blocks } = layout;
		for (let i = 1; i < blocks.length; i++) {
			expect(blocks[i].top).toBeGreaterThanOrEqual(blocks[i - 1].top + blocks[i - 1].height);
		}
		expect(blocks.map((b) => b.kind)).toEqual(['header', 'row', 'row', 'header', 'row']);
		expect(layout.height).toBe(30 + 369 + 10 + 30 + 200);
	});

	it('returns only the blocks intersecting the window', () => {
		const shown = visible(layout.blocks, 420, 50, 0) as Block<unknown>[];
		expect(shown.map((b) => b.key)).toEqual(['h:b', 'r:b:0']);
		expect(visible(layout.blocks, 0, 10, 0).map((b) => b.key)).toEqual(['h:a']);
	});
});
