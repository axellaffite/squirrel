import { describe, expect, it } from 'vitest';
import { DEFAULT_SORT, nextSort, sortLabel, sortNodes, type Sort } from './sort.ts';

const node = (name: string, over: { kind?: 'file' | 'folder'; size?: number; modified_at?: string } = {}) => ({
	name,
	kind: over.kind ?? 'file',
	size: over.size ?? 0,
	modified_at: over.modified_at ?? '2026-11-27T10:00:00Z'
});

const names = (nodes: { name: string }[]) => nodes.map((n) => n.name);
const by = (key: Sort['key'], dir: Sort['dir']): Sort => ({ key, dir });

describe('sortNodes', () => {
	const items = [
		node('b.txt', { size: 30, modified_at: '2026-11-20T10:00:00Z' }),
		node('Zeta', { kind: 'folder', modified_at: '2026-11-01T10:00:00Z' }),
		node('IMG_10.jpg', { size: 10, modified_at: '2026-11-25T10:00:00Z' }),
		node('alpha', { kind: 'folder', modified_at: '2026-11-26T10:00:00Z' }),
		node('IMG_2.jpg', { size: 20, modified_at: '2026-11-22T10:00:00Z' })
	];

	it('puts folders first, then files, by name A to Z by default (numbers in order, case ignored)', () => {
		expect(names(sortNodes(items, DEFAULT_SORT))).toEqual(['alpha', 'Zeta', 'b.txt', 'IMG_2.jpg', 'IMG_10.jpg']);
	});

	it('keeps folders first when sorted by name, either way', () => {
		expect(names(sortNodes(items, by('name', 'desc')))).toEqual(['Zeta', 'alpha', 'IMG_10.jpg', 'IMG_2.jpg', 'b.txt']);
	});

	it('mixes folders and files by any other column', () => {
		expect(names(sortNodes(items, by('size', 'desc')))).toEqual(['b.txt', 'IMG_2.jpg', 'IMG_10.jpg', 'alpha', 'Zeta']);
		expect(names(sortNodes(items, by('modified', 'desc')))).toEqual(['alpha', 'IMG_10.jpg', 'IMG_2.jpg', 'b.txt', 'Zeta']);
		expect(names(sortNodes(items, by('modified', 'asc')))).toEqual(['Zeta', 'b.txt', 'IMG_2.jpg', 'IMG_10.jpg', 'alpha']);
		expect(names(sortNodes(items, by('kind', 'asc')))).toEqual(['alpha', 'Zeta', 'IMG_2.jpg', 'IMG_10.jpg', 'b.txt']);
	});

	it('does not change the list it is given', () => {
		const copy = [...items];
		sortNodes(items, by('size', 'asc'));
		expect(items).toEqual(copy);
	});
});

describe('nextSort', () => {
	it('reverses the column already sorted', () => {
		expect(nextSort(DEFAULT_SORT, 'name')).toEqual(by('name', 'desc'));
		expect(nextSort(by('name', 'desc'), 'name')).toEqual(by('name', 'asc'));
	});

	it('starts a new column A to Z for text, largest or newest first for numbers', () => {
		expect(nextSort(DEFAULT_SORT, 'kind')).toEqual(by('kind', 'asc'));
		expect(nextSort(DEFAULT_SORT, 'size')).toEqual(by('size', 'desc'));
		expect(nextSort(DEFAULT_SORT, 'modified')).toEqual(by('modified', 'desc'));
	});
});

describe('sortLabel', () => {
	it('says how a header sorts', () => {
		expect(sortLabel(DEFAULT_SORT, 'name', 'Name')).toBe('Name, sorted ascending');
		expect(sortLabel(by('size', 'desc'), 'size', 'Size')).toBe('Size, sorted descending');
		expect(sortLabel(DEFAULT_SORT, 'size', 'Size')).toBe('Sort by Size');
	});
});
