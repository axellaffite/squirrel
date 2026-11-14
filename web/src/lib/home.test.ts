import { describe, expect, it } from 'vitest';
import { recentItems, uploadTarget } from '#lib/home.ts';

const node = (id: string, name: string, modified_at = '2026-11-01T00:00:00Z') => ({ id, name, modified_at });

describe('recentItems', () => {
	it('sorts newest first and keeps at most the count', () => {
		const items = [
			node('a', 'a', '2026-11-01T10:00:00Z'),
			node('b', 'b', '2026-11-03T10:00:00Z'),
			node('c', 'c', '2026-11-02T10:00:00Z')
		];
		expect(recentItems(items, 2).map((item) => item.id)).toEqual(['b', 'c']);
	});

	it('does not reorder the input', () => {
		const items = [node('a', 'a', '2026-11-01T10:00:00Z'), node('b', 'b', '2026-11-03T10:00:00Z')];
		recentItems(items, 2);
		expect(items.map((item) => item.id)).toEqual(['a', 'b']);
	});
});

describe('uploadTarget', () => {
	it('prefers the Files folder', () => {
		expect(uploadTarget([node('1', 'Documents'), node('2', 'Files'), node('3', 'Photos')])?.id).toBe('2');
	});

	it('falls back to the first folder, or null', () => {
		expect(uploadTarget([node('1', 'Documents')])?.id).toBe('1');
		expect(uploadTarget([])).toBeNull();
	});
});
