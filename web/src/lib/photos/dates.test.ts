import { describe, expect, it } from 'vitest';
import {
	dayKey,
	dayLabel,
	fullDateTime,
	monthKey,
	monthLabel,
	offsetLabel,
	shortMonthLabel,
	tileDateTime
} from '#lib/photos/dates.ts';

describe('dates', () => {
	it('keys days and months on the wall clock', () => {
		expect(dayKey('2024-07-14T18:30:05')).toBe('2024-07-14');
		expect(monthKey('2024-07-14T18:30:05')).toBe('2024-07');
	});

	it('labels days without shifting them through UTC', () => {
		expect(dayLabel('2024-07-14', 'en-GB')).toBe('Sun 14 Jul 2024');
		expect(dayLabel('2024-01-01', 'en-GB')).toBe('Mon 1 Jan 2024');
	});

	it('labels months', () => {
		expect(monthLabel('2024-07', 'en-GB')).toBe('July 2024');
		expect(shortMonthLabel('2024-07', 'en-GB')).toBe('Jul 2024');
	});

	it('writes the capture time in full', () => {
		expect(fullDateTime('2024-07-14T18:30:05', 'en-GB')).toBe('Sunday 14 July 2024, 18:30');
		expect(tileDateTime('2024-07-14T18:30:05', 'en-GB')).toBe('14 July 2024 18:30');
	});

	it('names the UTC offset', () => {
		expect(offsetLabel('+02:00')).toBe('UTC+02:00');
		expect(offsetLabel('-05:30')).toBe('UTC-05:30');
		expect(offsetLabel(null)).toBeNull();
		expect(offsetLabel('bogus')).toBeNull();
	});
});
