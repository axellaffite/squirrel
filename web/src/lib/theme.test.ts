import { describe, expect, it } from 'vitest';
import { applyTheme, parseTheme, readTheme, saveTheme, THEME_KEY } from '#lib/theme.ts';

function memoryStorage(initial: Record<string, string> = {}) {
	const data = new Map(Object.entries(initial));
	return {
		getItem: (key: string) => data.get(key) ?? null,
		setItem: (key: string, value: string) => void data.set(key, value),
		removeItem: (key: string) => void data.delete(key),
		data
	};
}

const throwing = {
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

describe('parseTheme', () => {
	it('accepts the three choices', () => {
		expect(parseTheme('light')).toBe('light');
		expect(parseTheme('dark')).toBe('dark');
		expect(parseTheme('system')).toBe('system');
	});

	it('falls back to system for anything else', () => {
		expect(parseTheme(null)).toBe('system');
		expect(parseTheme('purple')).toBe('system');
	});
});

describe('readTheme / saveTheme', () => {
	it('round-trips a choice', () => {
		const storage = memoryStorage();
		saveTheme('dark', storage);
		expect(readTheme(storage)).toBe('dark');
	});

	it('forgets the key when the user goes back to system', () => {
		const storage = memoryStorage({ [THEME_KEY]: 'light' });
		saveTheme('system', storage);
		expect(storage.data.has(THEME_KEY)).toBe(false);
	});

	it('survives blocked storage', () => {
		expect(readTheme(throwing)).toBe('system');
		expect(() => saveTheme('dark', throwing)).not.toThrow();
	});
});

describe('applyTheme', () => {
	it('sets data-theme for an explicit choice and removes it for system', () => {
		const root = { dataset: {} as Record<string, string | undefined> };
		applyTheme('light', root);
		expect(root.dataset.theme).toBe('light');
		applyTheme('system', root);
		expect(root.dataset.theme).toBeUndefined();
	});
});
