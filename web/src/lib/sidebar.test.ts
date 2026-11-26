import { describe, expect, it } from 'vitest';
import {
	DEFAULT_SIDEBAR,
	PRESET_KEYS,
	SCRIM_FLOOR,
	SIDEBAR_KEY,
	contrast,
	mutedOnScrim,
	parseSidebar,
	presetColors,
	readSidebar,
	saveSidebar,
	scrimAlpha,
	washBackground
} from '#lib/sidebar.ts';

const BLURHASH = 'LEHV6nWB2yk8pyo0adR*.7kCMdnj';

function memoryStorage(initial: Record<string, string> = {}) {
	const data = new Map(Object.entries(initial));
	return {
		getItem: (k: string) => data.get(k) ?? null,
		setItem: (k: string, v: string) => void data.set(k, v),
		removeItem: (k: string) => void data.delete(k),
		data
	};
}

describe('parseSidebar', () => {
	it('falls back to Forest for missing or unknown values', () => {
		expect(parseSidebar(null)).toEqual(DEFAULT_SIDEBAR);
		expect(parseSidebar('nope')).toEqual(DEFAULT_SIDEBAR);
		expect(parseSidebar('{"kind":"preset","preset":"lava"}')).toEqual(DEFAULT_SIDEBAR);
		expect(DEFAULT_SIDEBAR).toEqual({ kind: 'preset', preset: 'forest' });
	});

	it('reads a preset and a photo with a valid blurhash', () => {
		expect(parseSidebar('{"kind":"preset","preset":"dusk"}')).toEqual({ kind: 'preset', preset: 'dusk' });
		expect(parseSidebar(JSON.stringify({ kind: 'photo', id: 'a1', blurhash: BLURHASH }))).toEqual({
			kind: 'photo',
			id: 'a1',
			blurhash: BLURHASH
		});
	});

	it('rejects a photo without a usable blurhash', () => {
		expect(parseSidebar('{"kind":"photo","id":"a1","blurhash":"x"}')).toEqual(DEFAULT_SIDEBAR);
		expect(parseSidebar('{"kind":"photo","blurhash":"' + BLURHASH + '"}')).toEqual(DEFAULT_SIDEBAR);
	});
});

describe('scrimAlpha', () => {
	it('keeps muted rail ink at 4.5:1 on the lightest point of every preset', () => {
		for (const key of PRESET_KEYS) {
			const colours = presetColors(key);
			const alpha = scrimAlpha(colours);
			expect(alpha).toBeGreaterThanOrEqual(SCRIM_FLOOR);
			for (const c of colours) expect(mutedOnScrim(c, alpha)).toBeGreaterThanOrEqual(4.5);
		}
	});

	it('darkens a bright wash more than a dark one, and never goes below the floor', () => {
		expect(scrimAlpha(presetColors('beach'))).toBeGreaterThan(scrimAlpha(presetColors('forest')));
		expect(scrimAlpha(presetColors('graphite'))).toBe(SCRIM_FLOOR);
		expect(scrimAlpha([[255, 255, 255]])).toBeLessThan(1);
		expect(mutedOnScrim([255, 255, 255], scrimAlpha([[255, 255, 255]]))).toBeGreaterThanOrEqual(4.5);
	});
});

describe('contrast', () => {
	it('matches the WCAG ratio for black on white', () => {
		expect(contrast([0, 0, 0], [255, 255, 255])).toBeCloseTo(21, 1);
	});
});

describe('washBackground', () => {
	it('paints radial spots over the preset base colour', () => {
		const css = washBackground('forest');
		expect(css).toContain('radial-gradient(');
		expect(css.trim().endsWith('#55705f')).toBe(true);
	});
});

describe('readSidebar / saveSidebar', () => {
	it('round-trips a choice and removes the key for the default', () => {
		const storage = memoryStorage();
		saveSidebar({ kind: 'preset', preset: 'beach' }, storage);
		expect(readSidebar(storage)).toEqual({ kind: 'preset', preset: 'beach' });
		saveSidebar(DEFAULT_SIDEBAR, storage);
		expect(storage.data.has(SIDEBAR_KEY)).toBe(false);
		expect(readSidebar(storage)).toEqual(DEFAULT_SIDEBAR);
	});

	it('survives storage that throws', () => {
		const broken = {
			getItem: () => {
				throw new Error('blocked');
			},
			setItem: () => {
				throw new Error('blocked');
			},
			removeItem: () => {
				throw new Error('blocked');
			}
		};
		expect(readSidebar(broken)).toEqual(DEFAULT_SIDEBAR);
		expect(() => saveSidebar({ kind: 'preset', preset: 'dusk' }, broken)).not.toThrow();
	});
});
