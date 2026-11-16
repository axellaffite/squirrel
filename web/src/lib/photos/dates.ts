/**
 * Capture times are wall-clock strings (`2024-07-14T18:30:05`, no offset): what the camera showed.
 * They are split by hand and built with the local `Date` constructor, never parsed as UTC, so a
 * day never shifts with the viewer's time zone.
 */

const WALL_CLOCK = /^(\d{4})-(\d{2})-(\d{2})(?:T(\d{2}):(\d{2}))?/;
const OFFSET = /^[+-]\d{2}:\d{2}$/;

interface Parts {
	year: number;
	month: number;
	day: number;
	hour: number;
	minute: number;
}

function parts(value: string): Parts {
	const match = WALL_CLOCK.exec(value);
	if (!match) return { year: 1970, month: 1, day: 1, hour: 0, minute: 0 };
	const [, y, m, d, h = '0', min = '0'] = match;
	return { year: Number(y), month: Number(m), day: Number(d), hour: Number(h), minute: Number(min) };
}

function localDate(value: string): Date {
	const p = parts(value);
	return new Date(p.year, p.month - 1, p.day, p.hour, p.minute);
}

/** `2024-07-14` for `2024-07-14T18:30:05`. */
export function dayKey(takenAt: string): string {
	return takenAt.slice(0, 10);
}

/** `2024-07` for `2024-07-14T18:30:05`. */
export function monthKey(takenAt: string): string {
	return takenAt.slice(0, 7);
}

/** "Sun 14 Jul 2024". */
export function dayLabel(day: string, locale?: string): string {
	const date = localDate(day);
	const weekday = date.toLocaleDateString(locale, { weekday: 'short' });
	const rest = date.toLocaleDateString(locale, { day: 'numeric', month: 'short', year: 'numeric' });
	return `${weekday} ${rest}`;
}

/** "July 2024". */
export function monthLabel(month: string, locale?: string): string {
	return localDate(`${month}-01`).toLocaleDateString(locale, { month: 'long', year: 'numeric' });
}

/** "Jul 2024" (scrubber pill). */
export function shortMonthLabel(month: string, locale?: string): string {
	return localDate(`${month}-01`).toLocaleDateString(locale, { month: 'short', year: 'numeric' });
}

function time(date: Date, locale?: string): string {
	return date.toLocaleTimeString(locale, { hour: '2-digit', minute: '2-digit' });
}

/** "Sunday 14 July 2024, 18:30" (lightbox info panel). */
export function fullDateTime(takenAt: string, locale?: string): string {
	const date = localDate(takenAt);
	const weekday = date.toLocaleDateString(locale, { weekday: 'long' });
	const day = date.toLocaleDateString(locale, { day: 'numeric', month: 'long', year: 'numeric' });
	return `${weekday} ${day}, ${time(date, locale)}`;
}

/** "14 July 2024 18:30" (accessible names of tiles and lightbox controls). */
export function tileDateTime(takenAt: string, locale?: string): string {
	const date = localDate(takenAt);
	const day = date.toLocaleDateString(locale, { day: 'numeric', month: 'long', year: 'numeric' });
	return `${day} ${time(date, locale)}`;
}

/** "UTC+02:00" for a recorded offset, else null. */
export function offsetLabel(offset: string | null | undefined): string | null {
	return offset && OFFSET.test(offset) ? `UTC${offset}` : null;
}
