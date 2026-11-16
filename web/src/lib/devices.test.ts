import { describe, expect, it } from 'vitest';
import { deviceLabel, lastSeen, sortDevices } from '#lib/devices.ts';

const device = (overrides: Partial<Parameters<typeof deviceLabel>[0]> = {}) => ({
	id: 'a',
	name: 'Pixel',
	platform: 'android' as const,
	model: 'Pixel 8' as string | null,
	app_version: '0.1.0',
	created_at: '2026-11-01T10:00:00Z',
	last_seen_at: '2026-11-16T10:00:00Z',
	revoked_at: null as string | null,
	current: false,
	...overrides
});

const NOW = new Date('2026-11-16T12:00:00Z');

describe('deviceLabel', () => {
	it('joins the model, the platform and the app version', () => {
		expect(deviceLabel(device())).toBe('Pixel 8 · Android · 0.1.0');
		expect(deviceLabel(device({ platform: 'ios', model: 'iPhone 15' }))).toBe('iPhone 15 · iOS · 0.1.0');
	});

	it('leaves out an unknown model', () => {
		expect(deviceLabel(device({ model: null }))).toBe('Android · 0.1.0');
	});
});

describe('lastSeen', () => {
	it('says active now within five minutes', () => {
		expect(lastSeen(device({ last_seen_at: '2026-11-16T11:56:00Z' }), NOW)).toBe('Active now');
		expect(lastSeen(device({ last_seen_at: '2026-11-16T12:00:30Z' }), NOW)).toBe('Active now');
	});

	it('counts minutes, then hours, within a day', () => {
		expect(lastSeen(device({ last_seen_at: '2026-11-16T11:48:00Z' }), NOW)).toBe('12 min ago');
		expect(lastSeen(device({ last_seen_at: '2026-11-16T10:00:00Z' }), NOW)).toBe('2 h ago');
		expect(lastSeen(device({ last_seen_at: '2026-11-15T12:30:00Z' }), NOW)).toBe('23 h ago');
	});

	it('shows the date beyond a day, with the year when it differs', () => {
		expect(lastSeen(device({ last_seen_at: '2026-11-14T09:00:00Z' }), NOW)).toBe('14 Nov');
		expect(lastSeen(device({ last_seen_at: '2025-03-02T09:00:00Z' }), NOW)).toBe('2 Mar 2025');
	});
});

describe('sortDevices', () => {
	it('lists live devices by last activity, then revoked ones', () => {
		const old = device({ id: 'old', last_seen_at: '2026-11-01T00:00:00Z' });
		const recent = device({ id: 'recent', last_seen_at: '2026-11-16T00:00:00Z' });
		const revoked = device({ id: 'revoked', last_seen_at: '2026-11-16T11:00:00Z', revoked_at: '2026-11-16T11:00:00Z' });
		const older = device({ id: 'older-revoked', last_seen_at: '2026-10-01T00:00:00Z', revoked_at: '2026-10-01T00:00:00Z' });
		const input = [revoked, old, older, recent];
		expect(sortDevices(input).map((d) => d.id)).toEqual(['recent', 'old', 'revoked', 'older-revoked']);
		expect(input[0]).toBe(revoked);
	});
});
