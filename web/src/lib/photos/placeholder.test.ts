import { describe, expect, it, vi } from 'vitest';
import { Lru, createPlaceholders } from '#lib/photos/placeholder.ts';

const HASH = 'LEHV6nWB2yk8pyo0adR*.7kCMdnj';

describe('Lru', () => {
	it('evicts the least recently used entry', () => {
		const lru = new Lru<number>(2);
		lru.set('a', 1);
		lru.set('b', 2);
		expect(lru.get('a')).toBe(1);
		lru.set('c', 3);
		expect(lru.get('b')).toBeUndefined();
		expect(lru.get('a')).toBe(1);
		expect(lru.get('c')).toBe(3);
		expect(lru.size).toBe(2);
	});
});

describe('createPlaceholders', () => {
	it('renders each hash once and caches it', () => {
		const render = vi.fn((pixels: Uint8ClampedArray) => `data:${pixels.length}`);
		const placeholders = createPlaceholders(render, 500);
		expect(placeholders.cached(HASH)).toBeUndefined();
		expect(placeholders.load(HASH)).toBe('data:4096');
		expect(placeholders.load(HASH)).toBe('data:4096');
		expect(placeholders.cached(HASH)).toBe('data:4096');
		expect(render).toHaveBeenCalledTimes(1);
	});

	it('gives null for an invalid hash without rendering', () => {
		const render = vi.fn(() => 'x');
		const placeholders = createPlaceholders(render, 500);
		expect(placeholders.load('nope')).toBeNull();
		expect(placeholders.load(null)).toBeNull();
		expect(render).not.toHaveBeenCalled();
	});

	it('gives null when rendering fails', () => {
		const placeholders = createPlaceholders(() => null, 500);
		expect(placeholders.load(HASH)).toBeNull();
	});
});
