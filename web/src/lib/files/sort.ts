/** Sorting a folder's listing by a column; folders always come first (no I/O). */
import { kindLabel } from '#lib/files/format.ts';

export type SortKey = 'name' | 'kind' | 'size' | 'modified';
export type SortDir = 'asc' | 'desc';

export interface Sort {
	key: SortKey;
	dir: SortDir;
}

export const DEFAULT_SORT: Sort = { key: 'name', dir: 'asc' };

interface Sortable {
	kind: string;
	name: string;
	size: number;
	modified_at: string;
}

/** "IMG_2" before "IMG_10", and "alpha" next to "Alpha". */
const collator = new Intl.Collator(undefined, { numeric: true, sensitivity: 'base' });

function compareBy(key: SortKey, a: Sortable, b: Sortable): number {
	switch (key) {
		case 'name':
			return collator.compare(a.name, b.name);
		case 'kind':
			return collator.compare(kindLabel(a), kindLabel(b));
		case 'size':
			return a.size - b.size;
		case 'modified':
			return Date.parse(a.modified_at) - Date.parse(b.modified_at);
	}
}

/** A sorted copy: folders, then files, each by `sort`, ties by name A to Z. */
export function sortNodes<T extends Sortable>(nodes: readonly T[], sort: Sort): T[] {
	const sign = sort.dir === 'asc' ? 1 : -1;
	const folderRank = (node: Sortable) => (node.kind === 'folder' ? 0 : 1);
	return [...nodes].sort(
		(a, b) => folderRank(a) - folderRank(b) || sign * compareBy(sort.key, a, b) || collator.compare(a.name, b.name)
	);
}

/** Clicking a header: the same column reverses; another starts A to Z, or largest and newest first. */
export function nextSort(current: Sort, key: SortKey): Sort {
	if (current.key === key) return { key, dir: current.dir === 'asc' ? 'desc' : 'asc' };
	return { key, dir: key === 'size' || key === 'modified' ? 'desc' : 'asc' };
}

/** The header button's accessible name. */
export function sortLabel(current: Sort, key: SortKey, label: string): string {
	if (current.key !== key) return `Sort by ${label}`;
	return `${label}, sorted ${current.dir === 'asc' ? 'ascending' : 'descending'}`;
}
