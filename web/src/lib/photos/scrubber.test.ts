import { describe, expect, it } from 'vitest';
import { monthAtFraction, positions, stepMonth, yearLabels } from '#lib/photos/scrubber.ts';

const marks = [
	{ month: '2024-07', top: 0 },
	{ month: '2024-06', top: 250 },
	{ month: '2023-12', top: 500 },
	{ month: '2023-11', top: 520 },
	{ month: '2021-02', top: 900 }
];
const at = positions(marks, 1000);

describe('scrubber geometry', () => {
	it('places months by their section top', () => {
		expect(at).toEqual([0, 0.25, 0.5, 0.52, 0.9]);
		expect(positions(marks, 0)).toEqual([0, 0, 0, 0, 0]);
	});

	it('finds the month nearest a position', () => {
		expect(monthAtFraction(0, at)).toBe(0);
		expect(monthAtFraction(0.12, at)).toBe(0);
		expect(monthAtFraction(0.13, at)).toBe(1);
		expect(monthAtFraction(0.51, at)).toBe(2);
		expect(monthAtFraction(0.515, at)).toBe(3);
		expect(monthAtFraction(1.5, at)).toBe(4);
		expect(monthAtFraction(-1, at)).toBe(0);
		expect(monthAtFraction(0.5, [])).toBe(-1);
	});

	it('labels each year once, skipping crowded labels', () => {
		expect(yearLabels(marks, at, 400)).toEqual([
			{ year: '2024', fraction: 0 },
			{ year: '2023', fraction: 0.5 },
			{ year: '2021', fraction: 0.9 }
		]);
		// At 40 px tall, 2023 sits 20 px below 2024 and 2021 16 px below 2023.
		expect(yearLabels(marks, at, 40).map((l) => l.year)).toEqual(['2024', '2023', '2021']);
		expect(yearLabels(marks, at, 30).map((l) => l.year)).toEqual(['2024', '2021']);
	});
});

describe('stepMonth', () => {
	const months = marks.map((m) => m.month);

	it('moves a month, a year, or to the ends', () => {
		expect(stepMonth(1, 'ArrowUp', months)).toBe(0);
		expect(stepMonth(0, 'ArrowUp', months)).toBe(0);
		expect(stepMonth(1, 'ArrowDown', months)).toBe(2);
		expect(stepMonth(2, 'Home', months)).toBe(0);
		expect(stepMonth(0, 'End', months)).toBe(4);
		expect(stepMonth(0, 'PageDown', months)).toBe(3);
		expect(stepMonth(2, 'PageUp', months)).toBe(0);
		expect(stepMonth(4, 'PageDown', months)).toBe(4);
		expect(stepMonth(0, 'Enter', months)).toBeNull();
		expect(stepMonth(0, 'ArrowDown', [])).toBeNull();
	});
});
