import { describe, expect, it } from 'vitest';
import { isArrowKey, neighbour } from '#lib/photos/keyboard.ts';

// Row 1: three tiles; row 2: two wide tiles; row 3 (after a day header): one tile.
const rects = [
	{ left: 0, top: 0, width: 100, height: 100 },
	{ left: 104, top: 0, width: 100, height: 100 },
	{ left: 208, top: 0, width: 100, height: 100 },
	{ left: 0, top: 104, width: 150, height: 100 },
	{ left: 154, top: 104, width: 154, height: 100 },
	{ left: 0, top: 240, width: 100, height: 100 }
];

describe('neighbour', () => {
	it('steps left and right in reading order', () => {
		expect(neighbour(rects, 1, 'ArrowRight')).toBe(2);
		expect(neighbour(rects, 2, 'ArrowRight')).toBe(3);
		expect(neighbour(rects, 0, 'ArrowLeft')).toBe(-1);
		expect(neighbour(rects, 5, 'ArrowRight')).toBe(-1);
	});

	it('moves to the closest centre of the next or previous row', () => {
		expect(neighbour(rects, 2, 'ArrowDown')).toBe(4);
		expect(neighbour(rects, 0, 'ArrowDown')).toBe(3);
		expect(neighbour(rects, 4, 'ArrowUp')).toBe(2);
		expect(neighbour(rects, 4, 'ArrowDown')).toBe(5);
		expect(neighbour(rects, 0, 'ArrowUp')).toBe(-1);
		expect(neighbour(rects, 9, 'ArrowUp')).toBe(-1);
	});

	it('recognises arrow keys', () => {
		expect(isArrowKey('ArrowUp')).toBe(true);
		expect(isArrowKey('Enter')).toBe(false);
	});
});
